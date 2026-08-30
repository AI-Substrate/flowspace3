import assert from 'node:assert/strict';
import test from 'node:test';

import { outputTails, parseConfigDir, parseDaemonUrl, queueCounts, runBounce } from './bounce.mjs';

const ok = (stdout = '', stderr = '') => ({ ok: true, code: 0, stdout, stderr });
const fail = (code = 1, stdout = '', stderr = '') => ({ ok: false, code, stdout, stderr });
const response = (status, body) => ({ ok: status >= 200 && status < 300, status, text: async () => JSON.stringify(body) });


function harnessContext(executor, { options = {}, env = {} } = {}) {
  const calls = [];
  const ctx = {
    cwd: '/repo',
    args: {},
    options: { drainTimeoutMs: '1000', verifyTimeoutMs: '1000', ...options },
    exec: async (command, args = [], opts = {}) => {
      calls.push({ command, args, opts });
      return executor(command, args, opts, calls);
    },
    fs: {
      exists: (path) => path === '/target/release/flowspace3',
      readText: (path) => path.endsWith('/daemon.key') ? 'test-key\n' : null,
      readdir: () => [],
      realpath: () => null,
    },
    env: { get: (name) => env[name] },
    git: { isRepo: () => true, currentBranch: () => 'w-daemon-bounce' },
    clock: { nowIso: () => '2026-08-30T00:00:00.000Z', sleep: async () => {} },
    ok: (data, extra = {}) => ({ status: 'ok', data, ...extra }),
    degraded: (data, next_action, extra = {}) => ({ status: 'degraded', data, next_action, ...extra }),
    unconfigured: (next_action, extra = {}) => ({ status: 'unconfigured', next_action, ...extra }),
    error: (code, message, extra = {}) => ({ status: 'error', error: { code, message, details: extra.details }, next_action: extra.next_action }),
  };
  return { ctx, calls };
}

function advancingRuntime(fetchResult) {
  let now = 0;
  return {
    now: () => now,
    fetch: fetchResult,
    advance: (ms) => { now += ms; },
  };
}

function wireClock(ctx, runtime) {
  ctx.clock.sleep = async (ms) => runtime.advance(ms);
}

test('helpers keep configuration, queue, and bounded compiler evidence deterministic', () => {
  assert.equal(parseDaemonUrl('x\n[daemon]\nurl = "http://127.0.0.1:7444"\n[database]\n'), 'http://127.0.0.1:7444');
  assert.equal(parseConfigDir('# config file: /tmp/fs3/config.toml (present)\n'), '/tmp/fs3');
  assert.deepEqual(queueCounts([
    { kind: 'scan_file', state: 'pending', count: 3 },
    { kind: 'embed', state: 'running', count: 2 },
  ]), { 'scan_file.pending': 3, 'embed.running': 2 });
  const details = outputTails({ stdout: 'a\nb\nc', stderr: 'd\ne\nf' }, 2);
  assert.match(details, /stdout \(last 2 lines\):\nb\nc/);
  assert.match(details, /stderr \(last 2 lines\):\ne\nf/);
});

test('stale HEAD refuses before build and names both SHAs', async () => {
  const { ctx, calls } = harnessContext((command, args) => {
    if (command === 'git' && args[0] === 'rev-parse' && args[1] === '--show-toplevel') return ok('/repo\n');
    if (command === 'git' && args[0] === 'fetch') return ok();
    if (command === 'git' && args.at(-1) === 'HEAD') return ok('head-sha\n');
    if (command === 'git' && args.at(-1) === 'origin/main') return ok('main-sha\n');
    throw new Error(`unexpected ${command} ${args.join(' ')}`);
  }, { env: { CARGO_TARGET_DIR: '/target' } });

  const result = await runBounce(ctx);
  assert.equal(result.error.code, 'E_DAEMON_BOUNCE_STALE_HEAD');
  assert.equal(result.error.details.head, 'head-sha');
  assert.equal(result.error.details.origin_main, 'main-sha');
  assert.match(result.next_action, /git pull --ff-only/);
  assert.equal(calls.some((call) => call.command === 'cargo'), false);
});

test('build failure leaves daemon untouched and retains labelled stream tails', async () => {
  const sha = 'a'.repeat(40);
  const { ctx, calls } = harnessContext((command, args) => {
    if (command === 'git' && args[0] === 'rev-parse' && args[1] === '--show-toplevel') return ok('/repo\n');
    if (command === 'git' && args[0] === 'fetch') return ok();
    if (command === 'git' && args[0] === 'rev-parse') return ok(`${sha}\n`);
    if (command === 'cargo') return fail(101, 'Compiling x\n', 'error: broken\n');
    throw new Error(`unexpected ${command} ${args.join(' ')}`);
  }, { env: { CARGO_TARGET_DIR: '/target' } });

  const result = await runBounce(ctx);
  assert.equal(result.error.code, 'E_DAEMON_BOUNCE_BUILD');
  assert.match(result.error.details.output, /stdout \(last 40 lines\):\nCompiling x/);
  assert.match(result.error.details.output, /stderr \(last 40 lines\):\nerror: broken/);
  assert.equal(calls.some((call) => call.command === 'lsof'), false);
});

test('cold start verify timeout is loud and returns the exact launch command', async () => {
  const sha = 'b'.repeat(40);
  const runtime = advancingRuntime(async () => { throw new Error('connection refused'); });
  const { ctx } = harnessContext((command, args) => {
    if (command === 'git' && args[0] === 'rev-parse' && args[1] === '--show-toplevel') return ok('/repo\n');
    if (command === 'git' && args[0] === 'fetch') return ok();
    if (command === 'git' && args[0] === 'rev-parse') return ok(`${sha}\n`);
    if (command === 'cargo') return ok();
    if (command === '/target/release/flowspace3' && args[0] === 'config') return ok('# config file: /tmp/fs3-isolated/config.toml (present)\n[daemon]\nurl = "http://127.0.0.1:7444"\n');
    if (command === 'lsof') return fail(1);
    if (command === 'tmux' && args[0] === 'new-window') return ok('%91\n');
    if (command === 'tmux' && args[0] === 'send-keys') return ok();
    throw new Error(`unexpected ${command} ${args.join(' ')}`);
  }, { env: { CARGO_TARGET_DIR: '/target', FS3_CONFIG_DIR: '/tmp/fs3-isolated' }, options: { verifyTimeoutMs: '1000' } });
  wireClock(ctx, runtime);

  const result = await runBounce(ctx, runtime);
  assert.equal(result.error.code, 'E_DAEMON_BOUNCE_VERIFY_TIMEOUT');
  assert.equal(result.error.details.daemon_url, 'http://127.0.0.1:7444');
  assert.match(result.error.details.launch_command, /FS3_CONFIG_DIR='\/tmp\/fs3-isolated'/);
  assert.match(result.next_action, /curl -i http:\/\/127\.0\.0\.1:7444\/health/);
});

test('running daemon drains in its discovered pane and verifies the 401 tell', async () => {
  const sha = 'c'.repeat(40);
  let lsofCalls = 0;
  const runtime = advancingRuntime(async (url, options) => {
    if (!options?.headers?.authorization) {
      return response(401, { ok: false, error: { code: 'FS3-E-DAEMON-UNAUTHORIZED' }, next_action: 'read the key' });
    }
    if (url.endsWith('/health')) return response(200, { status: 'ok', version: '0.3.0', embedder: 'fake', summarizer: 'fake' });
    return response(200, { ok: true, data: { queue: [{ kind: 'embed', state: 'pending', count: 7, with_error: 0 }] } });
  });
  const { ctx, calls } = harnessContext((command, args) => {
    if (command === 'git' && args[0] === 'rev-parse' && args[1] === '--show-toplevel') return ok('/repo\n');
    if (command === 'git' && args[0] === 'fetch') return ok();
    if (command === 'git' && args[0] === 'rev-parse') return ok(`${sha}\n`);
    if (command === 'cargo') return ok();
    if (command === '/target/release/flowspace3' && args[0] === 'config') return ok('# config file: /tmp/fs3/config.toml (present)\n[daemon]\nurl = "http://127.0.0.1:7555"\n');
    if (command === 'lsof') return ++lsofCalls === 1 ? ok('4242\n') : fail(1);
    if (command === 'tmux' && args[0] === 'list-panes') return ok('%77\t4000\n');
    if (command === 'ps') return ok('4000\n');
    if (command === 'tmux' && args[0] === 'send-keys') return ok();
    throw new Error(`unexpected ${command} ${args.join(' ')}`);
  }, { env: { CARGO_TARGET_DIR: '/target' } });
  wireClock(ctx, runtime);

  const result = await runBounce(ctx, runtime);
  assert.equal(result.status, 'ok');
  assert.equal(result.data.daemon.previous_pid, '4242');
  assert.equal(result.data.daemon.pane, '%77');
  assert.equal(result.data.verify.tell.http_status, 401);
  assert.equal(result.data.verify.tell.code, 'FS3-E-DAEMON-UNAUTHORIZED');
  assert.equal(result.data.verify.version, '0.3.0');
  assert.deepEqual(result.data.verify.queue_counts, { 'embed.pending': 7 });
  assert.equal(result.data.freshness.override, null);
  assert.ok(calls.some((call) => call.command === 'tmux' && call.args.includes('C-c')));
  assert.ok(calls.some((call) => call.command === 'tmux' && call.args.includes('Enter')));
});

test('explicit freshness override remains visible in a successful envelope', async () => {
  const runtime = advancingRuntime(async (url, options) => {
    if (!options?.headers?.authorization) return response(401, { error: { code: 'FS3-E-DAEMON-UNAUTHORIZED' } });
    if (url.endsWith('/health')) return response(200, { status: 'ok', version: '0.3.0' });
    return response(200, { ok: true, data: { queue: [] } });
  });
  const { ctx } = harnessContext((command, args) => {
    if (command === 'git' && args[0] === 'rev-parse' && args[1] === '--show-toplevel') return ok('/repo\n');
    if (command === 'git' && args[0] === 'fetch') return ok();
    if (command === 'git' && args.at(-1) === 'HEAD') return ok('branch\n');
    if (command === 'git' && args.at(-1) === 'origin/main') return ok('main\n');
    if (command === 'cargo') return ok();
    if (command === '/target/release/flowspace3' && args[0] === 'config') return ok('# config file: /tmp/fs3/config.toml (present)\n[daemon]\nurl = "http://127.0.0.1:7666"\n');
    if (command === 'lsof') return fail(1);
    if (command === 'tmux' && args[0] === 'new-window') return ok('%92\n');
    if (command === 'tmux' && args[0] === 'send-keys') return ok();
    throw new Error(`unexpected ${command} ${args.join(' ')}`);
  }, { env: { CARGO_TARGET_DIR: '/target' }, options: { allowDirtyHead: true } });
  wireClock(ctx, runtime);

  const result = await runBounce(ctx, runtime);
  assert.equal(result.status, 'ok');
  assert.equal(result.data.freshness.matched, false);
  assert.equal(result.data.freshness.override.flag, '--allow-dirty-head');
  assert.match(result.data.freshness.override.reason, /explicitly allowed/);
});
