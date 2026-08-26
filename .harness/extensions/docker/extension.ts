import { join } from 'node:path';
import { defineExtension } from '@ai-substrate/engineering-harness/contract';

/**
 * Paved docker surface (plan 002, phase 2): `harness docker <sub>`.
 * Every sub shells through to docker/scripts/*.sh — the scripts are the
 * single implementation; the harness verb is the discoverable surface
 * (Jordan ruling 2026-08-26).
 *
 * Compose stays DB-ONLY by ruling 2026-08-26-daemon-native-on-host: the fs3
 * daemon runs natively on the host, so there is deliberately no daemon-restart
 * sub here — restart your host process; docker never touches it.
 */

const SCRIPTS = 'docker/scripts';
const TIMEOUT_MS = 1_800_000;

type VerbResult = { status: 'ok' | 'degraded' | 'unconfigured' | 'error'; [k: string]: unknown };
type Ctx = {
  cwd: string;
  args?: Record<string, string | string[] | undefined>;
  exec(cmd: string, args: string[], opts?: { timeoutMs?: number }): Promise<{ ok: boolean; stdout: string; stderr: string }>;
  ok(data?: unknown): VerbResult;
  degraded(data: unknown, detail: string): VerbResult;
  unconfigured(detail: string): VerbResult;
  error(code: string, message: string, extra?: Record<string, unknown>): VerbResult;
};

/** Variadic passthrough args from the subverb arg map. */
function rest(c: Ctx): string[] {
  const v = c.args?.args;
  if (typeof v === 'string') return [v];
  return Array.isArray(v) ? v : [];
}

/** Run a docker/scripts/*.sh and translate the exit into a verb result. */
async function sh(c: Ctx, script: string, args: string[], timeoutMs = TIMEOUT_MS) {
  const r = await c.exec('bash', [join(SCRIPTS, script), ...args], { timeoutMs });
  const out = `${r.stdout ?? ''}${r.stderr && r.stdout ? '\n' : ''}${r.stderr ?? ''}`.trimEnd();
  if (r.ok) return c.ok({ output: out });
  return c.error('E_DOCKER', out.split('\n').slice(-5).join('\n') || 'script failed', {
    details: out,
    next_action: 'Re-run the failing subcommand for the full output.',
  }) as never;
}

const subs = {
  up: {
    summary: 'Bring up the compose stack (db-only: postgres+pgvector on :5433).',
    async run(ctx) {
      return sh(ctx as unknown as Ctx, 'stack.sh', ['up'], 300_000);
    },
  },
  down: {
    summary: 'Stop the compose stack. Never deletes volumes.',
    async run(ctx) {
      return sh(ctx as unknown as Ctx, 'stack.sh', ['down'], 300_000);
    },
  },
  status: {
    summary: 'Show compose stack state.',
    async run(ctx) {
      return sh(ctx as unknown as Ctx, 'stack.sh', ['status'], 120_000);
    },
  },
  logs: {
    summary: 'Compose stack logs.',
    args: [{ name: '[args...]', description: 'Extra arguments passed through to compose logs' }],
    async run(ctx) {
return sh(ctx as unknown as Ctx, 'stack.sh', ['logs', ...rest(ctx as unknown as Ctx)], 120_000);
    },
  },
  exec: {
    summary: 'Exec into a compose service. Example: harness docker exec -- db pg_isready -U flowspace3',
    args: [{ name: '[args...]', description: 'Service and command to exec, e.g. `-- db pg_isready -U flowspace3`' }],
    async run(ctx) {
      const c = ctx as unknown as Ctx;
      const argv = rest(c);
      if (argv.length === 0) {
        return c.error('E_ARGS', 'harness docker exec needs a service + command', {
          details: 'Example: harness docker exec -- db pg_isready -U flowspace3',
        }) as never;
      }
      return sh(c, 'stack.sh', ['exec', ...argv], 300_000);
    },
  },
  build: {
    summary:
      'Build fs3-daemon for FS3_TARGET inside the pinned container (linux gnu/musl arm64+x86_64, windows-gnu). Darwin targets are refused — those build natively on the mac host.',
    async run(ctx) {
return sh(ctx as unknown as Ctx, 'build.sh', []);
    },
  },
  run: {
    summary:
      'One-shot command in-container on the compose network with FS3_TEST_DATABASE_URL exported. Default: cargo test --workspace — the paved way to run things against the stack.',
    args: [{ name: '[args...]', description: 'Command to run in-container; empty = cargo test --workspace' }],
    async run(ctx) {
return sh(ctx as unknown as Ctx, 'run.sh', rest(ctx as unknown as Ctx));
    },
  },
  lint: {
    summary:
      'Prove the surface is engine-agnostic: FS3_ENGINE coverage, compose-spec validity, no Docker-exclusive features. Exits non-zero on violation.',
    async run(ctx) {
      return sh(ctx as unknown as Ctx, 'lint.sh', [], 120_000);
    },
  },
};

export default defineExtension({
  kind: 'extension',
  name: 'docker',
  summary:
    'Paved engine-agnostic docker surface: stack up/down/status/logs/exec, cross-platform builds, one-shot in-container runs (incl. workspace tests vs the compose db).',
  verbs: {
    docker: {
      summary:
        'Paved engine-agnostic docker surface — `harness docker <up|down|status|logs|exec|build|run|lint>`.',
      sub: subs,
    },
  },
});
