import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { defineExtension } from '@ai-substrate/engineering-harness/contract';

/** The quality gate: every command an agent must see green before calling work done. */
const GATES: { name: string; cmd: string; args: string[] }[] = [
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
  { name: 'fmt', cmd: 'cargo', args: ['fmt', '--all', '--check'] },
  { name: 'clippy', cmd: 'cargo', args: ['clippy', '--all-targets', '--', '-D', 'warnings'] },
  { name: 'test', cmd: 'cargo', args: ['test', '--all'] },
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

export default defineExtension({
  name: 'checks',
  summary: 'The mandated quality gate — docs links, cargo fmt, clippy, tests, and architecture drift.',
  verbs: {
    checks: {
      summary: 'Run the quality gate (docs links, cargo fmt --check, clippy -D warnings, cargo test, arch drift).',
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
          const r = await ctx.exec(gate.cmd, gate.args, { timeoutMs: 600_000 });
          const command = `${gate.cmd} ${gate.args.join(' ')}`;
          results.push({ gate: gate.name, command, ok: r.ok, code: r.code ?? -1 });
          if (!r.ok) {
            return ctx.error('E_CHECKS_FAILED', `${command} failed (exit ${r.code})`, {
              details: tail(`${r.stdout}\n${r.stderr}`, 40),
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
