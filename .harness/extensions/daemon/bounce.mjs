import { dirname, isAbsolute, join, resolve } from 'node:path';

const BUILD_TIMEOUT_MS = 900_000;
const DEFAULT_DRAIN_TIMEOUT_MS = 60_000;
const DEFAULT_VERIFY_TIMEOUT_MS = 120_000;
const POLL_MS = 500;
const TAIL_LINES = 40;

const defaultRuntime = {
  now: () => Date.now(),
  fetch: (url, options) => fetch(url, options),
};

export function outputTails(result, lines = TAIL_LINES) {
  const tail = (value) => String(value ?? '').trimEnd().split('\n').slice(-lines).join('\n');
  return `stdout (last ${lines} lines):\n${tail(result.stdout)}\n\nstderr (last ${lines} lines):\n${tail(result.stderr)}`;
}

export function parseDaemonUrl(text) {
  const daemon = String(text ?? '').match(/(?:^|\n)\[daemon\][\s\S]*?(?:^|\n)url\s*=\s*"([^"]+)"/m);
  return daemon?.[1] ?? null;
}
export function parseConfigDir(text) {
  const match = String(text ?? '').match(/(?:^|\n)# config file:\s+(.+?)\s+\((?:present|missing)\)/);
  return match?.[1] ? dirname(match[1]) : null;
}


export function parseJsonEnvelope(text) {
  try {
    return JSON.parse(String(text ?? '').trim());
  } catch {
    return null;
  }
}

export function queueCounts(rows) {
  const counts = {};
  for (const row of Array.isArray(rows) ? rows : []) {
    const kind = typeof row?.kind === 'string' ? row.kind : 'unknown';
    const state = typeof row?.state === 'string' ? row.state : 'unknown';
    counts[`${kind}.${state}`] = Number(row?.count ?? 0);
  }
  return counts;
}

export function shellQuote(value) {
  return `'${String(value).replaceAll("'", `'"'"'`)}'`;
}

function numberOption(ctx, name, fallback) {
  const raw = ctx.options[name];
  if (raw === undefined) return fallback;
  const value = Number(raw);
  return Number.isFinite(value) && value > 0 ? value : null;
}

function failure(ctx, code, message, stage, stages, nextAction, details = {}) {
  return ctx.error(code, message, {
    details: { stage, stages, ...details },
    next_action: nextAction,
  });
}

async function gitSha(ctx, root, ref) {
  const result = await ctx.exec('git', ['rev-parse', '--verify', ref], { cwd: root, timeoutMs: 30_000 });
  return result.ok ? String(result.stdout ?? '').trim() : null;
}

async function listenerPids(ctx, port) {
  const result = await ctx.exec('lsof', ['-nP', `-iTCP:${port}`, '-sTCP:LISTEN', '-t'], { timeoutMs: 15_000 });
  if (!result.ok) {
    const stdout = String(result.stdout ?? '').trim();
    const stderr = String(result.stderr ?? '').trim();
    // lsof exits 1 with no output when the selector matched no listener.
    if (result.code === 1 && !stdout && !stderr) return [];
    throw new Error(outputTails(result));
  }
  return [...new Set(String(result.stdout ?? '').split(/\s+/).filter((pid) => /^\d+$/.test(pid)))];
}

async function parentPid(ctx, pid) {
  const result = await ctx.exec('ps', ['-o', 'ppid=', '-p', pid], { timeoutMs: 5_000 });
  const parent = String(result.stdout ?? '').trim();
  return result.ok && /^\d+$/.test(parent) ? parent : null;
}

async function descendsFrom(ctx, pid, ancestor) {
  let current = pid;
  const visited = new Set();
  while (current && current !== '1' && !visited.has(current)) {
    if (current === ancestor) return true;
    visited.add(current);
    current = await parentPid(ctx, current);
  }
  return false;
}

async function owningPane(ctx, pid) {
  const result = await ctx.exec('tmux', ['list-panes', '-a', '-F', '#{pane_id}\t#{pane_pid}'], { timeoutMs: 10_000 });
  if (!result.ok) return null;
  for (const line of String(result.stdout ?? '').split('\n')) {
    const [pane, panePid] = line.trim().split(/\s+/);
    if (pane && /^%\d+$/.test(pane) && /^\d+$/.test(panePid ?? '') && await descendsFrom(ctx, pid, panePid)) {
      return pane;
    }
  }
  return null;
}

function releaseBinary(ctx, root) {
  const configured = ctx.env.get('CARGO_TARGET_DIR');
  const target = configured ? (isAbsolute(configured) ? configured : resolve(root, configured)) : join(root, 'target');
  return join(target, 'release', process.platform === 'win32' ? 'flowspace3.exe' : 'flowspace3');
}

function launchCommand(ctx, binary) {
  const assignments = ['FS3_CONFIG_DIR', 'FS3_DATABASE__URL', 'FS3_DAEMON__URL']
    .map((name) => [name, ctx.env.get(name)])
    .filter(([, value]) => value)
    .map(([name, value]) => `${name}=${shellQuote(value)}`);
  return `${assignments.length ? `env ${assignments.join(' ')} ` : ''}${shellQuote(binary)} daemon`;
}

async function resolveDaemonConfig(ctx, binary, root) {
  const requested = typeof ctx.options.daemonUrl === 'string' ? ctx.options.daemonUrl.trim() : '';
  const shown = await ctx.exec(binary, ['config', 'show'], { cwd: root, timeoutMs: 30_000 });
  const effective = shown.ok ? parseDaemonUrl(shown.stdout) : null;
  const configDir = ctx.env.get('FS3_CONFIG_DIR') || (shown.ok ? parseConfigDir(shown.stdout) : null);
  return { url: requested || effective, requested: requested || null, effective, configDir, result: shown };
}

async function waitForNoListener(ctx, port, timeoutMs, runtime) {
  const started = runtime.now();
  let polls = 0;
  while (runtime.now() - started < timeoutMs) {
    polls += 1;
    try {
      if ((await listenerPids(ctx, port)).length === 0) {
        return { ok: true, elapsed_ms: runtime.now() - started, polls };
      }
    } catch (error) {
      return { ok: false, elapsed_ms: runtime.now() - started, polls, probe_error: error instanceof Error ? error.message : String(error) };
    }
    await ctx.clock.sleep(POLL_MS);
  }
  return { ok: false, elapsed_ms: runtime.now() - started, polls };
}

async function unauthenticatedHealth(url, runtime) {
  try {
    const response = await runtime.fetch(`${url.replace(/\/$/, '')}/health`, {
      signal: AbortSignal.timeout(2_000),
      headers: { accept: 'application/json' },
    });
    const text = await response.text();
    return { reachable: true, status: response.status, body: parseJsonEnvelope(text), text };
  } catch (error) {
    return { reachable: false, error: error instanceof Error ? error.message : String(error) };
  }
}

async function waitFor401Tell(ctx, url, timeoutMs, runtime) {
  const started = runtime.now();
  let polls = 0;
  let last = null;
  while (runtime.now() - started < timeoutMs) {
    polls += 1;
    last = await unauthenticatedHealth(url, runtime);
    if (last.reachable && last.status === 401 && last.body?.error?.code === 'FS3-E-DAEMON-UNAUTHORIZED') {
      return { ok: true, elapsed_ms: runtime.now() - started, polls, tell: last.body };
    }
    await ctx.clock.sleep(POLL_MS);
  }
  return { ok: false, elapsed_ms: runtime.now() - started, polls, last };
}

async function authenticatedReports(ctx, daemonUrl, configDir, runtime) {
  const keyPath = configDir ? join(configDir, 'daemon.key') : null;
  const key = keyPath ? ctx.fs.readText(keyPath)?.trim() : null;
  if (!key) return { keyPath, error: `daemon key is not readable at ${keyPath ?? '(unresolved config directory)'}` };
  const request = async (path) => {
    try {
      const response = await runtime.fetch(`${daemonUrl}${path}`, {
        signal: AbortSignal.timeout(5_000),
        headers: { accept: 'application/json', authorization: `Bearer ${key}` },
      });
      const text = await response.text();
      return { ok: response.ok, status: response.status, body: parseJsonEnvelope(text), text };
    } catch (error) {
      return { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
  };
  return { keyPath, health: await request('/health'), status: await request('/status') };
}

export async function runBounce(ctx, runtime = defaultRuntime) {
  const stages = [];
  const started = runtime.now();
  const rootResult = await ctx.exec('git', ['rev-parse', '--show-toplevel'], { cwd: ctx.cwd, timeoutMs: 30_000 });
  if (!rootResult.ok) {
    return failure(ctx, 'E_DAEMON_BOUNCE_NOT_REPO', 'cannot locate the repository root', 'freshness', stages,
      'Run `harness daemon bounce` from inside the flowspace3 repository.', { command: 'git rev-parse --show-toplevel', output: outputTails(rootResult) });
  }
  const root = String(rootResult.stdout ?? '').trim();
  const drainTimeoutMs = numberOption(ctx, 'drainTimeoutMs', DEFAULT_DRAIN_TIMEOUT_MS);
  const verifyTimeoutMs = numberOption(ctx, 'verifyTimeoutMs', DEFAULT_VERIFY_TIMEOUT_MS);
  if (!drainTimeoutMs || !verifyTimeoutMs) {
    return failure(ctx, 'E_DAEMON_BOUNCE_BAD_TIMEOUT', 'timeouts must be positive numbers', 'options', stages,
      'Pass positive millisecond values, e.g. `--drain-timeout-ms 60000 --verify-timeout-ms 120000`.',
      { drain_timeout_ms: ctx.options.drainTimeoutMs, verify_timeout_ms: ctx.options.verifyTimeoutMs });
  }

  const fetchStarted = runtime.now();
  const fetched = await ctx.exec('git', ['fetch', 'origin', 'main'], { cwd: root, timeoutMs: 120_000 });
  if (!fetched.ok) {
    return failure(ctx, 'E_DAEMON_BOUNCE_FETCH', '`git fetch origin main` failed; freshness is unproven', 'freshness', stages,
      'Restore access to `origin`, then re-run `harness daemon bounce`. Never bounce an unproven binary.',
      { command: 'git fetch origin main', output: outputTails(fetched) });
  }
  const head = await gitSha(ctx, root, 'HEAD');
  const originMain = await gitSha(ctx, root, 'origin/main');
  if (!head || !originMain) {
    return failure(ctx, 'E_DAEMON_BOUNCE_REFS', 'could not resolve HEAD and origin/main after fetch', 'freshness', stages,
      'Repair the local refs, then run `git fetch origin main` and re-run the bounce.', { head, origin_main: originMain });
  }
  const matched = head === originMain;
  const override = Boolean(ctx.options.allowDirtyHead);
  stages.push({ stage: 'freshness', ok: matched || override, elapsed_ms: runtime.now() - fetchStarted, head, origin_main: originMain, matched, override });
  if (!matched && !override) {
    return failure(ctx, 'E_DAEMON_BOUNCE_STALE_HEAD', 'HEAD does not match origin/main; refusing to build and bounce a stale binary', 'freshness', stages,
      '`git pull --ff-only` first, then re-run `harness daemon bounce`.', { head, origin_main: originMain });
  }

  const buildStarted = runtime.now();
  const built = await ctx.exec('cargo', ['build', '--release'], { cwd: root, timeoutMs: BUILD_TIMEOUT_MS });
  stages.push({ stage: 'build', ok: built.ok, elapsed_ms: runtime.now() - buildStarted, command: 'cargo build --release' });
  if (!built.ok) {
    return failure(ctx, 'E_DAEMON_BOUNCE_BUILD', '`cargo build --release` failed; the running daemon was not touched', 'build', stages,
      'Fix the labelled compiler output, then re-run `harness daemon bounce`.', { output: outputTails(built) });
  }
  const binary = releaseBinary(ctx, root);
  if (!ctx.fs.exists(binary)) {
    return failure(ctx, 'E_DAEMON_BOUNCE_BINARY', `release build completed but ${binary} does not exist`, 'build', stages,
      'Check CARGO_TARGET_DIR and the flowspace3 binary target, then re-run the bounce.', { binary });
  }

  const configured = await resolveDaemonConfig(ctx, binary, root);
  if (!configured.url || !configured.effective || !configured.configDir) {
    return failure(ctx, 'E_DAEMON_BOUNCE_CONFIG', 'could not resolve daemon.url and its credential directory from the effective configuration', 'locate', stages,
      'Run `flowspace3 config show`, fix `[daemon].url`, then re-run the bounce.', { output: outputTails(configured.result ?? {}) });
  }
  if (configured.requested && configured.requested.replace(/\/$/, '') !== configured.effective.replace(/\/$/, '')) {
    return failure(ctx, 'E_DAEMON_BOUNCE_CONFIG_MISMATCH', `--daemon-url ${configured.requested} does not match effective daemon.url ${configured.effective}`, 'locate', stages,
      'Point FS3_CONFIG_DIR at the isolated daemon configuration (or remove --daemon-url), then re-run the bounce.',
      { requested: configured.requested, effective: configured.effective, config_dir: configured.configDir });
  }
  let parsed;
  try {
    parsed = new URL(configured.url);
  } catch {
    return failure(ctx, 'E_DAEMON_BOUNCE_CONFIG', `daemon URL is invalid: ${configured.url}`, 'locate', stages,
      'Fix `[daemon].url` to an http(s) loopback URL with an explicit port, then re-run the bounce.');
  }
  const port = parsed.port ? Number(parsed.port) : parsed.protocol === 'https:' ? 443 : 80;
  const daemonUrl = parsed.toString().replace(/\/$/, '');
  let pids;
  try {
    pids = await listenerPids(ctx, port);
  } catch (error) {
    return failure(ctx, 'E_DAEMON_BOUNCE_LOCATE', `could not inspect listeners on daemon port ${port}`, 'locate', stages,
      `Make sure \`lsof\` is installed and can inspect port ${port}, then re-run the bounce.`,
      { port, output: error instanceof Error ? error.message : String(error) });
  }
  if (pids.length > 1) {
    return failure(ctx, 'E_DAEMON_BOUNCE_AMBIGUOUS_LISTENER', `multiple processes listen on daemon port ${port}`, 'locate', stages,
      `Inspect \`lsof -nP -iTCP:${port} -sTCP:LISTEN\`; stop the unexpected listener before bouncing.`, { pids, port });
  }
  const previousPid = pids[0] ?? null;
  const pane = previousPid ? await owningPane(ctx, previousPid) : null;
  const command = launchCommand(ctx, binary);
  stages.push({ stage: 'locate', ok: true, elapsed_ms: runtime.now() - started, daemon_url: daemonUrl, port, pid: previousPid, pane, cold: !previousPid });

  let restartPane = pane;
  if (previousPid && !pane) {
    return failure(ctx, 'E_DAEMON_BOUNCE_PANE_NOT_FOUND', `listener pid ${previousPid} is not owned by a discoverable tmux pane; it was not stopped`, 'locate', stages,
      `Run this exact command in the daemon's terminal after stopping it cleanly: ${command}`, { pid: previousPid, port, launch_command: command });
  }

  let drain = { elapsed_ms: 0, polls: 0 };
  if (previousPid && pane) {
    const interrupted = await ctx.exec('tmux', ['send-keys', '-t', pane, 'C-c'], { timeoutMs: 10_000 });
    if (!interrupted.ok) {
      return failure(ctx, 'E_DAEMON_BOUNCE_INTERRUPT', `could not send Ctrl-C to daemon pane ${pane}`, 'drain', stages,
        `Send Ctrl-C in pane ${pane}, wait for drain, then run: ${command}`, { output: outputTails(interrupted), pane, pid: previousPid });
    }
    drain = await waitForNoListener(ctx, port, drainTimeoutMs, runtime);
    stages.push({ stage: 'drain', ok: drain.ok, ...drain, pid: previousPid });
    if (!drain.ok) {
      if (drain.probe_error) {
        return failure(ctx, 'E_DAEMON_BOUNCE_DRAIN_PROBE', `daemon was interrupted, but listener state on port ${port} became unobservable`, 'drain', stages,
          `Inspect pane ${pane} and \`lsof -nP -iTCP:${port} -sTCP:LISTEN\`; restart manually with: ${command}`,
          { pane, pid: previousPid, port, launch_command: command, ...drain });
      }
      return failure(ctx, 'E_DAEMON_BOUNCE_DRAIN_TIMEOUT', `daemon still listens on port ${port} after ${drainTimeoutMs}ms`, 'drain', stages,
        `Inspect pane ${pane} and the daemon log; a second Ctrl-C forces cancellation of in-flight work.`, { pane, pid: previousPid, port, ...drain });
    }
  } else {
    const created = await ctx.exec('tmux', ['new-window', '-d', '-P', '-F', '#{pane_id}', '-n', 'fs3-daemon'], { cwd: root, timeoutMs: 10_000 });
    if (!created.ok) {
      return failure(ctx, 'E_DAEMON_BOUNCE_START_PANE', `no daemon is running on port ${port}, and no tmux pane could be created`, 'start', stages,
        `Start the daemon with this exact command: ${command}`, { cold: true, port, launch_command: command, output: outputTails(created) });
    }
    restartPane = String(created.stdout ?? '').trim() || null;
  }

  const launched = await ctx.exec('tmux', ['send-keys', '-t', restartPane, '-l', '--', command], { timeoutMs: 10_000 });
  if (!launched.ok) {
    return failure(ctx, 'E_DAEMON_BOUNCE_RELAUNCH', `daemon launch command could not be sent to pane ${restartPane}`, 'restart', stages,
      `Run this exact command in pane ${restartPane}: ${command}`, { pane: restartPane, launch_command: command, output: outputTails(launched) });
  }
  const entered = await ctx.exec('tmux', ['send-keys', '-t', restartPane, 'Enter'], { timeoutMs: 10_000 });
  if (!entered.ok) {
    return failure(ctx, 'E_DAEMON_BOUNCE_RELAUNCH', `Enter could not be sent to pane ${restartPane} after writing the daemon launch command`, 'restart', stages,
      `Press Enter in pane ${restartPane}, or run this exact command there: ${command}`, { pane: restartPane, launch_command: command, output: outputTails(entered) });
  }
  stages.push({ stage: 'restart', ok: true, pane: restartPane, command });

  const verified = await waitFor401Tell(ctx, daemonUrl, verifyTimeoutMs, runtime);
  stages.push({ stage: 'verify-401', ok: verified.ok, elapsed_ms: verified.elapsed_ms, polls: verified.polls, status: verified.last?.status ?? 401 });
  if (!verified.ok) {
    return failure(ctx, 'E_DAEMON_BOUNCE_VERIFY_TIMEOUT', `daemon did not produce the required unauthenticated 401 tell within ${verifyTimeoutMs}ms`, 'verify', stages,
      `Inspect pane ${restartPane ?? '(none)'}, run \`curl -i ${daemonUrl}/health\`, and check the daemon log before retrying.`,
      { daemon_url: daemonUrl, pane: restartPane, launch_command: command, ...verified });
  }

  const reports = await authenticatedReports(ctx, daemonUrl, configured.configDir, runtime);
  if (reports.error || !reports.health?.ok || reports.health.body?.status !== 'ok' || !reports.status?.ok || reports.status.body?.ok !== true) {
    return failure(ctx, 'E_DAEMON_BOUNCE_AUTH_VERIFY', 'the 401 tell appeared, but authenticated health/status verification failed', 'verify', stages,
      `Run \`${binary} doctor\` with the same FS3_CONFIG_DIR and inspect pane ${restartPane}.`, {
        daemon_url: daemonUrl,
        key_file: reports.keyPath,
        probe_error: reports.error ?? null,
        health: reports.health ?? null,
        status: reports.status ?? null,
      });
  }
  const health = reports.health.body;
  const status = reports.status.body.data;
  const data = {
    bounced: true,
    freshness: {
      head,
      origin_main: originMain,
      matched,
      override: override ? { flag: '--allow-dirty-head', reason: 'operator explicitly allowed HEAD to differ from origin/main' } : null,
    },
    build: { command: 'cargo build --release', binary },
    daemon: { url: daemonUrl, port, previous_pid: previousPid, pane: restartPane, cold_start: !previousPid, launch_command: command },
    drain,
    verify: {
      tell: { http_status: 401, code: verified.tell.error.code },
      version: health?.version ?? null,
      health,
      queue_counts: queueCounts(status?.queue),
      queue: status?.queue ?? [],
      elapsed_ms: verified.elapsed_ms,
      polls: verified.polls,
    },
    elapsed_ms: runtime.now() - started,
    stages,
  };
  return ctx.ok(data, {
    evidence: [{ label: `daemon health verified at ${daemonUrl}/health`, none: true }],
    next_action: 'The daemon is running the freshly built binary; monitor `flowspace3 status` while its queue drains.',
  });
}
