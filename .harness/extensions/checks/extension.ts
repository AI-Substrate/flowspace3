import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { defineExtension } from '@ai-substrate/engineering-harness/contract';

import { outputDetails, testGateVerdict } from './check-result.mjs';

/** The quality gate: every command an agent must see green before calling work done. */
const GATES: { name: string; cmd: string; args: string[]; guarded?: boolean }[] = [
  // First, because it is two seconds and needs no compilation — and because a
  // stale lock makes every gate below it a statement about a dependency set
  // that is not the one `release.yml` will build. That workflow uses
  // `--locked`, so lock drift is otherwise discovered at a TAG, on a matrix of
  // runners, by whoever is shipping. Plain `cargo test` updates the lock in
  // place and goes green, which is exactly how the w-auto-update packet added
  // two dependencies and landed on main with the lock un-updated (2026-08-27).
  { name: 'lock', cmd: 'cargo', args: ['metadata', '--locked', '--format-version', '1'] },
  // Before anything compiles, because the `test` gate below WRITES to whatever
  // database this names — and until 2026-08-27 it named the shipped default,
  // which on a developer machine is the real store. A `harness checks` run
  // migrated Jordan's production database that way (ruling:
  // .harness/government/rulings/2026-08-27-production-database.md).
  //
  // The test helpers refuse on their own (fs3_testkit::database), so this gate
  // changes no outcome — it changes WHEN and HOW LEGIBLY you find out: one line
  // up front instead of the same refusal repeated inside a test binary three
  // minutes into a compile.
  {
    name: 'testdb',
    cmd: 'cargo',
    args: ['run', '--quiet', '-p', 'fs3-testkit', '--bin', 'fs3-test-db-check'],
  },
  {
    name: 'checks-contract',
    cmd: 'node',
    args: ['--test', '.harness/extensions/checks/check-result.test.mjs'],
  },
  {
    name: 'daemon-bounce-contract',
    cmd: 'node',
    args: ['--test', '.harness/extensions/daemon/bounce.test.mjs'],
  },
  { name: 'fmt', cmd: 'cargo', args: ['fmt', '--all', '--check'] },
  { name: 'clippy', cmd: 'cargo', args: ['clippy', '--all-targets', '--', '-D', 'warnings'] },
  // `guarded`: this is the gate that WRITES. The production database's schema
  // version is snapshotted either side of it and any change fails the run —
  // see `productionSchema` below.
  {
    name: 'test',
    cmd: 'cargo',
    args: ['run', '--quiet', '-p', 'fs3-testkit', '--bin', 'fs3-test-suite'],
    guarded: true,
  },
  // Architecture drift: the crate graph judged against testkit/arch-allowlist.toml.
  // Cargo stops undeclared imports and cycles; this stops declared-but-refused
  // edges (sqlx in the functional core, a mocking framework anywhere).
  { name: 'arch', cmd: 'cargo', args: ['run', '--quiet', '-p', 'fs3-testkit', '--bin', 'fs3-arch-check'] },
];

/** Where prose lives. Every markdown link in these files has to resolve. */
const DOCS_ROOT = 'docs/how';

/**
 * Signposts that must stay real (bp-0008 / ac-0008). The pair is asserted
 * explicitly rather than left to the generic link sweep, because the failure
 * mode is deletion: with the guide gone there is no link left to be broken.
 */
const REQUIRED_SIGNPOSTS: { file: string; links: string }[] = [
  { file: 'README.md', links: 'docs/how/architecture.md' },
];

/** `](target)`, with an optional title. */
const MARKDOWN_LINK = /\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;

/** Fenced blocks are samples, not links — a broken path in an example is fine. */
const FENCED_BLOCK = /```[\s\S]*?```/g;

function markdownFiles(cwd: string): string[] {
  // Signpost sources first; they live at the root, so they never collide with
  // the DOCS_ROOT sweep below.
  const files = REQUIRED_SIGNPOSTS.map((signpost) => signpost.file);
  const root = join(cwd, DOCS_ROOT);
  if (existsSync(root)) {
    for (const entry of readdirSync(root)) {
      if (entry.endsWith('.md')) files.push(join(DOCS_ROOT, entry));
    }
  }
  return files;
}

/**
 * The docs-link gate bp-0008 asks for.
 *
 * The cargo gates have no opinion about prose: deleting docs/how/architecture.md
 * or breaking the README's link to it left `harness checks` green, so the
 * signpost every agent is told to read first was unprotected. This makes the
 * documentation a checked artifact rather than an aspiration.
 */
function docsGate(cwd: string): { ok: boolean; checked: number; problems: string[] } {
  const problems: string[] = [];
  let checked = 0;

  for (const { file, links } of REQUIRED_SIGNPOSTS) {
    const from = join(cwd, file);
    if (!existsSync(from)) {
      problems.push(`${file} is missing`);
      continue;
    }
    if (!existsSync(join(cwd, links))) {
      problems.push(`${links} is missing — ${file} signposts it`);
    }
    if (!readFileSync(from, 'utf8').includes(links)) {
      problems.push(`${file} no longer links ${links}`);
    }
  }

  for (const file of markdownFiles(cwd)) {
    const absolute = join(cwd, file);
    if (!existsSync(absolute)) continue;
    const prose = readFileSync(absolute, 'utf8').replace(FENCED_BLOCK, '');
    for (const match of prose.matchAll(MARKDOWN_LINK)) {
      const target = match[1];
      if (/^(?:[a-z][a-z0-9+.-]*:|#|\/\/)/i.test(target)) continue;
      const [path] = target.split('#');
      if (!path) continue;
      checked += 1;
      if (!existsSync(resolve(dirname(absolute), path))) {
        problems.push(`${file} links ${target}, which does not exist`);
      }
    }
  }

  return { ok: problems.length === 0, checked, problems };
}

const tail = (s: string, n = 20) => s.trimEnd().split('\n').slice(-n).join('\n');

/** Just the slice of the extension context this helper needs. */
type Runner = {
  exec(
    cmd: string,
    args: string[],
    options?: { timeoutMs?: number },
  ): Promise<{ ok: boolean; stdout?: string; stderr?: string; code?: number }>;
};

/**
 * The schema version of the database this machine calls PRODUCTION.
 *
 * Read either side of the `test` gate: `cargo test --all` is the only thing in
 * this pipeline that writes, and twice now it has written to the wrong
 * database. Migrations 0008/0009 went in through the test helpers (closed by
 * the `testdb` gate above); migration 0012 went in on 2026-08-27 through a test
 * that spawns the real daemon and never calls those helpers at all — sixteen
 * seconds after the test database got it — and took Jordan's installed CLI down
 * on schema skew.
 *
 * Every OTHER defence is a rule about a leak path somebody already knows about.
 * This one compares a number before against a number after, so the breach class
 * is caught even through a path nobody has thought of yet. That is the
 * difference between un-repeated and un-shippable.
 *
 * The probe reports; this decides. See `crates/daemon/src/bin/migration_guard.rs`
 * for why `absent` and `same-as-test` are passing answers rather than skips.
 */
async function productionSchema(
  ctx: Runner,
): Promise<{ ok: boolean; value: string; details: string }> {
  const r = await ctx.exec(
    'cargo',
    ['run', '--quiet', '-p', 'fs3-daemon', '--bin', 'fs3-migration-guard'],
    { timeoutMs: 300_000 },
  );
  return {
    ok: r.ok,
    value: (r.stdout ?? '').trim(),
    details: tail(`${r.stdout}\n${r.stderr}`, 20),
  };
}

export default defineExtension({
  name: 'checks',
  summary: 'The mandated quality gate — docs links, cargo fmt, clippy, tests, architecture drift, and the production-database guard.',
  verbs: {
    checks: {
      summary: 'Run the quality gate (docs links, cargo fmt --check, clippy -D warnings, cargo test under the production migration guard, arch drift).',
      async run(ctx) {
        if (!existsSync(join(ctx.cwd, 'Cargo.toml'))) {
          return ctx.degraded(
            { gates: GATES.map((g) => `${g.cmd} ${g.args.join(' ')}`), reason: 'no Cargo.toml' },
            'No Cargo.toml at the repo root — this repo has no crate yet, so the gate has nothing to prove. Create the crate (`cargo init`), or edit .harness/extensions/checks/extension.ts if this project is not Rust.',
          );
        }

        const results: { gate: string; command: string; ok: boolean; code: number }[] = [];

        // First, because it is instant and needs no compiler.
        const docs = docsGate(ctx.cwd);
        results.push({
          gate: 'docs',
          command: `docs-link: README.md + ${DOCS_ROOT}/*.md`,
          ok: docs.ok,
          code: docs.ok ? 0 : 1,
        });
        if (!docs.ok) {
          return ctx.error('E_CHECKS_FAILED', `docs-link found ${docs.problems.length} broken signpost(s)`, {
            details: docs.problems.join('\n'),
            data: { gate: 'docs', results, problems: docs.problems },
            next_action: 'Restore the missing file or fix the link, then re-run `harness checks`.',
          });
        }

        for (const gate of GATES) {
          // Snapshot BEFORE the gate that writes. A failed probe is a broken
          // gate, not a caught incident, so it stops the run on its own terms.
          let before: { ok: boolean; value: string; details: string } | null = null;
          if (gate.guarded) {
            before = await productionSchema(ctx);
            results.push({
              gate: 'prodguard:before',
              command: 'cargo run -p fs3-daemon --bin fs3-migration-guard',
              ok: before.ok,
              code: before.ok ? 0 : 1,
            });
            if (!before.ok) {
              return ctx.error('E_CHECKS_FAILED', 'the production migration guard could not read a schema version', {
                details: before.details,
                data: { gate: 'prodguard:before', results },
                next_action:
                  'The guard reports; it never repairs. Fix the probe (crates/daemon/src/bin/migration_guard.rs) or the configuration it reads, then re-run `harness checks`. Do not skip it: it is the only check that catches a leak through a path nobody has named yet.',
              });
            }
          }

          const r = await ctx.exec(gate.cmd, gate.args, { timeoutMs: 600_000 });
          const command = `${gate.cmd} ${gate.args.join(' ')}`;
          const verdict = gate.name === 'test' ? testGateVerdict(r) : null;
          const gateOk = verdict ? verdict.kind === 'pass' : r.ok;
          results.push({
            gate: gate.name,
            command,
            ok: gateOk,
            code: gateOk ? 0 : (r.code ?? 1),
          });

          // Snapshot AFTER, and BEFORE reporting any test failure. A run that
          // failed can still have migrated production on its way down — that is
          // the more serious finding of the two, so it is checked first and
          // reported first.
          if (before) {
            const after = await productionSchema(ctx);
            const changed = !after.ok || after.value !== before.value;
            results.push({
              gate: 'prodguard:after',
              command: 'cargo run -p fs3-daemon --bin fs3-migration-guard',
              ok: !changed,
              code: changed ? 1 : 0,
            });
            if (changed) {
              return ctx.error('E_CHECKS_FAILED', `a test run changed the PRODUCTION database (${before.value} -> ${after.value})`, {
                details: `before: ${before.value}\nafter:  ${after.value}\n\n${after.details}`,
                data: { gate: 'prodguard:after', before: before.value, after: after.value, results },
                next_action:
                  'STOP — this is the 2026-08-27 incident happening again. Something under `cargo test --all` reached the database this machine calls production. Find which test: it either opens a pool without `fs3_testkit::test_database_url()`, or spawns `flowspace3` without `fs3_testkit::sealed()`. Read `.harness/government/rulings/2026-08-27-production-database.md`. Do not re-run the gate to see if it clears — it will not, and each run may migrate further.',
              });
            }
          }
          if (verdict?.kind === 'infrastructure') {
            const wholeSuite = verdict.evidence.wholeSuiteFailed
              ? ' The whole suite failed together.'
              : '';
            return ctx.error(
              'E_CHECKS_INFRASTRUCTURE',
              `INFRASTRUCTURE FAILED — this red is not about your code.${wholeSuite}`,
              {
                details: outputDetails(r, 40),
                data: { gate: gate.name, verdict: 'infrastructure', evidence: verdict.evidence, results },
                next_action:
                  'Postgres lost one or more test connections. Stabilize the shared database cluster, then run the entire gate again. Do not bank any PASS from this run.',
              },
            );
          }

          if (!r.ok) {
            return ctx.error('E_CHECKS_FAILED', `${command} failed (exit ${r.code})`, {
              details: outputDetails(r, 40),
              data: { gate: gate.name, results },
              next_action: `Fix the failure above, then re-run \`harness checks\`.`,
            });
          }
        }
        return ctx.ok({ gates: results, passed: results.length, doc_links_checked: docs.checked });
      },
    },
  },
});
