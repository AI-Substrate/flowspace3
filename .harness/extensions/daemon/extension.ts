import { defineExtension } from '@ai-substrate/engineering-harness/contract';
import type { V2VerbContext, VerbResult } from '@ai-substrate/engineering-harness/contract';

import { runBounce } from './bounce.mjs';

export default defineExtension({
  name: 'daemon',
  summary: 'Build and safely drain-restart the configured flowspace3 daemon, then prove the authenticated surface is live.',
  verbs: {
    daemon: {
      summary: 'Daemon lifecycle operations — `harness daemon bounce`.',
      sub: {
        bounce: {
          summary: 'Build release, drain-restart the configured daemon, and verify its 401 authentication tell.',
          description:
            'Fetches origin/main and refuses a stale HEAD, builds the release binary, discovers the configured listener and its tmux pane, drains it with Ctrl-C, relaunches in that pane (or starts a cold daemon in a new pane), then requires the unauthenticated /health 401 tell plus authenticated health and queue reports.',
          options: [
            {
              flags: '--allow-dirty-head',
              description: 'explicitly allow HEAD to differ from origin/main; the override and reason remain visible in the result envelope',
            },
            {
              flags: '--daemon-url <url>',
              description: 'override only the locate/verify URL (for an isolated daemon whose effective config points at the same URL)',
            },
            {
              flags: '--drain-timeout-ms <milliseconds>',
              description: 'bounded drain wait (default 60000)',
              defaultValue: '60000',
            },
            {
              flags: '--verify-timeout-ms <milliseconds>',
              description: 'bounded 401-tell startup wait (default 120000)',
              defaultValue: '120000',
            },
          ],
          run: (ctx: V2VerbContext): Promise<VerbResult> => runBounce(ctx),
        },
      },
    },
  },
});
