import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { defineExtension } from '@ai-substrate/engineering-harness/contract';

const ORIENTATION = [
  'flowspace3 — Rust workspace, 7 crates. The engineering harness is the front door:',
  '  harness checks   the gate: fmt, clippy, tests, architecture drift — green before done',
  '  harness doctor   what is configured and what is not',
  '  harness observe  capture friction the moment it bites',
  '',
  'The proof tiers need the stack up: docker compose up -d (postgres+pgvector, :5433).',
  'Before adding a file, read docs/how/architecture.md — placement is a table, not a judgment call.',
].join('\n');

export default defineExtension({
  name: 'boot',
  summary: 'Prove the environment is ready, then compose the quality gate.',
  verbs: {
    boot: {
      summary: 'Ready the toolchain, run `harness checks`, and re-orient the agent.',
      async run(ctx) {
        const stages: { stage: string; ok: boolean; detail: string }[] = [];

        // 1 — toolchain
        const cargo = await ctx.exec('cargo', ['--version'], { timeoutMs: 60_000 });
        stages.push({
          stage: 'toolchain',
          ok: cargo.ok,
          detail: cargo.ok ? cargo.stdout.trim() : 'cargo not on PATH',
        });
        if (!cargo.ok) {
          return ctx.error('E_NO_TOOLCHAIN', 'cargo is not available on PATH', {
            data: { stages, orientation: ORIENTATION },
            next_action: 'Install the Rust toolchain (https://rustup.rs), then re-run `harness boot`.',
          });
        }

        // 2 — the crate itself
        const hasCrate = existsSync(join(ctx.cwd, 'Cargo.toml'));
        stages.push({
          stage: 'crate',
          ok: hasCrate,
          detail: hasCrate ? 'Cargo.toml present' : 'no Cargo.toml — nothing to build yet',
        });
        if (hasCrate) {
          const build = await ctx.exec('cargo', ['build', '--all-targets'], { timeoutMs: 900_000 });
          stages.push({
            stage: 'build',
            ok: build.ok,
            detail: build.ok ? 'cargo build --all-targets' : build.stderr.trimEnd().split('\n').slice(-20).join('\n'),
          });
          if (!build.ok) {
            return ctx.error('E_BUILD_FAILED', `cargo build failed (exit ${build.code})`, {
              data: { stages, orientation: ORIENTATION },
              next_action: 'Fix the build failure above, then re-run `harness boot`.',
            });
          }
        }

        // 3 — the compose stack the store and daemon tiers prove against
        const COMPOSE_UP = 'docker compose up -d';
        const pg = await ctx.exec(
          'docker',
          ['compose', 'exec', '-T', 'db', 'pg_isready', '-U', 'flowspace3', '-d', 'flowspace3'],
          { timeoutMs: 60_000 },
        );
        stages.push({
          stage: 'compose',
          ok: pg.ok,
          detail: pg.ok
            ? 'postgres+pgvector accepting connections on 127.0.0.1:5433'
            : `${pg.stderr || pg.stdout}`.trimEnd().split('\n').slice(-5).join('\n') || 'db service not reachable',
        });
        if (!pg.ok) {
          return ctx.degraded(
            { stages, orientation: ORIENTATION, verdict: 'degraded' },
            `Postgres is not answering, so the store and daemon proof tiers cannot run. Start the stack with \`${COMPOSE_UP}\`, then re-run \`harness boot\`. (If docker itself is missing, that is the thing to install — the integration tests fail rather than skip, deliberately.)`,
          );
        }

        // 4 — compose the quality gate
        if (!existsSync(join(ctx.cwd, '.harness/extensions/checks'))) {
          return ctx.degraded(
            { stages, orientation: ORIENTATION },
            'No `checks` extension exists — create one (`harness new checks --wrap "…"`) or move existing quality checks (lint, tests, typecheck) into a `checks` extension so `boot` and agents can gate on it.',
          );
        }
        const checks = await ctx.exec('harness', ['checks', '--json'], { timeoutMs: 900_000 });
        let checksStatus = 'error';
        try {
          checksStatus = JSON.parse(checks.stdout).status ?? 'error';
        } catch {
          /* non-JSON output — treat as error below */
        }
        stages.push({ stage: 'checks', ok: checksStatus === 'ok', detail: `harness checks → ${checksStatus}` });

        const data = { stages, checks: checksStatus, orientation: ORIENTATION };
        if (checksStatus === 'ok') return ctx.ok({ ...data, verdict: 'ready' });
        if (checksStatus === 'degraded' || checksStatus === 'unconfigured') {
          return ctx.degraded(
            { ...data, verdict: 'degraded' },
            'The environment is up but the quality gate cannot prove anything yet — run `harness checks` and follow its next_action.',
          );
        }
        return ctx.error('E_CHECKS_FAILED', 'harness checks failed', {
          data: { ...data, verdict: 'unhealthy' },
          details: checks.stdout.trimEnd().split('\n').slice(-20).join('\n'),
          next_action: 'Run `harness checks` to see the failing gate, fix it, then re-run `harness boot`.',
        });
      },
    },
  },
});
