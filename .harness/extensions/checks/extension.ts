import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { defineExtension } from '@ai-substrate/engineering-harness/contract';

/** The quality gate: every command an agent must see green before calling work done. */
const GATES: { name: string; cmd: string; args: string[] }[] = [
  { name: 'fmt', cmd: 'cargo', args: ['fmt', '--all', '--check'] },
  { name: 'clippy', cmd: 'cargo', args: ['clippy', '--all-targets', '--', '-D', 'warnings'] },
  { name: 'test', cmd: 'cargo', args: ['test', '--all'] },
];

const tail = (s: string, n = 20) => s.trimEnd().split('\n').slice(-n).join('\n');

export default defineExtension({
  name: 'checks',
  summary: 'The mandated quality gate — cargo fmt, clippy, and tests.',
  verbs: {
    checks: {
      summary: 'Run the quality gate (cargo fmt --check, clippy -D warnings, cargo test).',
      async run(ctx) {
        if (!existsSync(join(ctx.cwd, 'Cargo.toml'))) {
          return ctx.degraded(
            { gates: GATES.map((g) => `${g.cmd} ${g.args.join(' ')}`), reason: 'no Cargo.toml' },
            'No Cargo.toml at the repo root — this repo has no crate yet, so the gate has nothing to prove. Create the crate (`cargo init`), or edit .harness/extensions/checks/extension.ts if this project is not Rust.',
          );
        }

        const results: { gate: string; command: string; ok: boolean; code: number }[] = [];
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
        return ctx.ok({ gates: results, passed: results.length });
      },
    },
  },
});
