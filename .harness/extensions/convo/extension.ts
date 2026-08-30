import { defineExtension } from '@ai-substrate/engineering-harness/contract';

/**
 * Conversation ingest surface (plan 005): `harness fs3-convo <sub>`.
 *
 * THIN BY CONSTRUCTION. Every sub is a passthrough to `flowspace3 conversation
 * …` — the CLI is the single implementation and the daemon does the work; this
 * verb is the discoverable surface, the shape the docker extension already
 * takes (Jordan ruling 2026-08-26).
 *
 * Arguments are forwarded VERBATIM rather than restated here. Restating them
 * would mint a second place where `--session` needs `--harness`, and two
 * declarations of one rule is one declaration that goes stale — the CLI's clap
 * definition stays the only statement of what a valid address is.
 *
 * Why the verb exists at all: ingest is fired from HOOKS, which run often, so
 * it must be a near-instant submit rather than a read. The route enqueues one
 * deduplicated job and returns; the daemon reads incrementally from a durable
 * cursor, so a firing with nothing new is a cheap no-op rather than a re-read.
 */

const TIMEOUT_MS = 120_000;

type VerbResult = { status: 'ok' | 'degraded' | 'unconfigured' | 'error'; [k: string]: unknown };
type Ctx = {
  cwd: string;
  args?: Record<string, string | string[] | undefined>;
  exec(
    cmd: string,
    args: string[],
    opts?: { timeoutMs?: number },
  ): Promise<{ ok: boolean; stdout: string; stderr: string }>;
  ok(data?: unknown): VerbResult;
  degraded(data: unknown, detail: string): VerbResult;
  unconfigured(detail: string): VerbResult;
  error(code: string, message: string, extra?: Record<string, unknown>): VerbResult;
};

/**
 * Run the CLI and hand its envelope back unchanged.
 *
 * The CLI already speaks the standard JSON envelope, so parsing and re-wrapping
 * would give an agent two shapes to learn for one fact. Only an unparseable
 * body falls back to raw output.
 */
async function cli(c: Ctx, args: string[]): Promise<VerbResult> {
  const r = await c.exec('flowspace3', args, { timeoutMs: TIMEOUT_MS });
  const out = `${r.stdout}${r.stderr}`.trim();
  if (r.ok) {
    try {
      return c.ok(JSON.parse(out));
    } catch {
      return c.ok({ output: out });
    }
  }
  return c.error('E_CONVO', out.split('\n').slice(-5).join('\n') || 'flowspace3 failed', {
    details: out,
    next_action: 'Is the daemon running? `flowspace3 status` says. Then re-run this verb.',
  });
}

const subs = {
  ingest: {
    summary:
      'Submit a native agent session for ingest and return immediately. Address by seat (--pij <seat>) or by session (--session <id> --harness <claude|omp|pij|metrics-db>).',
    args: [
      {
        name: '[args...]',
        description:
          'Passed through to `flowspace3 conversation ingest`, e.g. `-- --pij pij-appalling-slug`',
      },
    ],
    async run(ctx) {
      const c = ctx as unknown as Ctx;
      const rest = c.args?.args;
      const argv = Array.isArray(rest) ? rest : rest ? [rest] : [];
      if (argv.length === 0) {
        return c.error('E_ARGS', 'name a conversation to ingest', {
          details:
            'harness fs3-convo ingest -- --pij <seat>\nharness fs3-convo ingest -- --session <id> --harness <claude|omp|pij|metrics-db>',
          next_action:
            'A session id alone does not say which store holds it: claude and copilot ids are both v4 uuids and live in different stores.',
        });
      }
      return cli(c, ['conversation', 'ingest', ...argv]);
    },
  },
  list: {
    summary: 'List indexed conversations, newest first.',
    args: [
      {
        name: '[args...]',
        description: 'Passed through to `flowspace3 conversation list`, e.g. `-- --repo <identity>`',
      },
    ],
    async run(ctx) {
      const c = ctx as unknown as Ctx;
      const rest = c.args?.args;
      const argv = Array.isArray(rest) ? rest : rest ? [rest] : [];
      return cli(c, ['conversation', 'list', ...argv]);
    },
  },
};

export default defineExtension({
  kind: 'extension',
  name: 'fs3-convo',
  summary:
    'Ingest agent conversations out of the native session stores (Claude Code, omp, the pij ledger, git-ai metrics) and list what is indexed.',
  verbs: {
    'fs3-convo': {
      summary: 'Conversation ingest surface — `harness fs3-convo <ingest|list>` (renamed from `convo`: core harness 0.13 ships a convo verb; collision bricked the CLI, backlog row 106).',
      sub: subs,
    },
  },
});
