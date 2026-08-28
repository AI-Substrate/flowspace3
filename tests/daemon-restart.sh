#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd -P)
SCRIPT="$ROOT/bin/daemon-restart"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/fs3-daemon-restart.XXXXXX")
SOCKET="fs3-daemon-restart-$$"
FAKE="$TMP/flowspace3"
CONFIG="$TMP/config"
CC_BIN=$(command -v "${CC:-cc}")
TMUX_BIN=$(command -v tmux)
mkdir -p "$CONFIG"

cleanup() {
  "$TMUX_BIN" -L "$SOCKET" kill-server 2>/dev/null || true
  rm -rf "$TMP"
}
trap cleanup EXIT

cat >"$TMP/fake-flowspace3.c" <<'EOF'
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

static volatile sig_atomic_t running = 1;
static void stop(int signal_number) {
    (void)signal_number;
    running = 0;
}

static int daemon_main(void) {
    const char *state = getenv("FS3_FAKE_STATE");
    const char *config = getenv("FS3_CONFIG_DIR");
    char path[4096];
    FILE *file;
    if (!state || !config) return 20;
    snprintf(path, sizeof(path), "%s/current.pid", state);
    file = fopen(path, "w");
    if (!file) return 21;
    fprintf(file, "%ld\n", (long)getpid());
    fclose(file);
    snprintf(path, sizeof(path), "%s/daemon.key", config);
    file = fopen(path, "w");
    if (!file) return 22;
    fputs("fake-auth-key\n", file);
    fclose(file);
    const char *ignore_int = getenv("FS3_FAKE_IGNORE_INT");
    if (ignore_int && *ignore_int) signal(SIGINT, SIG_IGN);
    else signal(SIGINT, stop);
    signal(SIGTERM, stop);
    while (running) pause();
    return 0;
}

static int ping_main(void) {
    const char *state = getenv("FS3_FAKE_STATE");
    const char *config = getenv("FS3_CONFIG_DIR");
    char path[4096];
    char key[64];
    long pid;
    FILE *file;
    if (!state || !config) return 30;
    snprintf(path, sizeof(path), "%s/current.pid", state);
    file = fopen(path, "r");
    if (!file || fscanf(file, "%ld", &pid) != 1) return 31;
    fclose(file);
    if (kill((pid_t)pid, 0) != 0) return 32;
    snprintf(path, sizeof(path), "%s/daemon.key", config);
    file = fopen(path, "r");
    if (!file || !fgets(key, sizeof(key), file)) return 33;
    fclose(file);
    if (strcmp(key, "fake-auth-key\n") != 0) return 34;
    puts("{\"ok\":true,\"health\":\"authenticated\"}");
    return 0;
}

int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "daemon") == 0) return daemon_main();
    if (argc == 2 && strcmp(argv[1], "ping") == 0) return ping_main();
    fprintf(stderr, "usage: flowspace3 daemon|ping\n");
    return 2;
}
EOF
"$CC_BIN" -Wall -Wextra -Werror -o "$FAKE" "$TMP/fake-flowspace3.c"

reset_server() {
  "$TMUX_BIN" -L "$SOCKET" kill-server 2>/dev/null || true
}

new_shell() {
  local session="$1"
  "$TMUX_BIN" -L "$SOCKET" new-session -d -s "$session" /bin/bash --noprofile --norc
}

start_daemon() {
  local target="$1" state="$2" ignore_int="${3:-}"
  mkdir -p "$state"
  "$TMUX_BIN" -L "$SOCKET" send-keys -t "$target" -l \
    "export FS3_FAKE_STATE='$state' FS3_CONFIG_DIR='$CONFIG' FS3_FAKE_IGNORE_INT='$ignore_int'; '$FAKE' daemon"
  "$TMUX_BIN" -L "$SOCKET" send-keys -t "$target" Enter
  for _ in $(seq 1 40); do
    [[ -s "$state/current.pid" ]] && kill -0 "$(cat "$state/current.pid")" 2>/dev/null && return 0
    sleep 0.1
  done
  printf 'fake daemon did not start in %s\n' "$target" >&2
  return 1
}

run_restart() {
  local state="$1"
  shift
  DAEMON_RESTART_TMUX_BIN="$TMUX_BIN" \
    DAEMON_RESTART_TMUX_SOCKET="$SOCKET" \
    FS3_FAKE_STATE="$state" \
    FS3_CONFIG_DIR="$CONFIG" \
    "$SCRIPT" --binary "$FAKE" "$@"
}

assert_contains() {
  local output="$1" expected="$2"
  [[ "$output" == *"$expected"* ]] || {
    printf 'expected output to contain: %s\nactual:\n%s\n' "$expected" "$output" >&2
    return 1
  }
}

printf 'case: discovers, stops, restarts in the same pane, and verifies health\n'
reset_server
new_shell single
STATE_ONE="$TMP/state-one"
start_daemon single:0.0 "$STATE_ONE"
OLD=$(cat "$STATE_ONE/current.pid")
PANE=$("$TMUX_BIN" -L "$SOCKET" display-message -p -t single:0.0 '#{pane_id}')
OUTPUT=$(run_restart "$STATE_ONE" 2>&1)
NEW=$(cat "$STATE_ONE/current.pid")
[[ "$NEW" != "$OLD" ]]
kill -0 "$NEW"
assert_contains "$OUTPUT" "SUMMARY pane=$PANE old_pid=$OLD new_pid=$NEW binary=$FAKE health=ok"

printf 'case: a second run replaces rather than double-starts\n'
SECOND_OLD="$NEW"
OUTPUT=$(run_restart "$STATE_ONE" 2>&1)
SECOND_NEW=$(cat "$STATE_ONE/current.pid")
[[ "$SECOND_NEW" != "$SECOND_OLD" ]]
assert_contains "$OUTPUT" "old_pid=$SECOND_OLD new_pid=$SECOND_NEW"
COUNT=$(pgrep -P "$("$TMUX_BIN" -L "$SOCKET" display-message -p -t single:0.0 '#{pane_pid}')" flowspace3 | wc -l | tr -d ' ')
[[ "$COUNT" == "1" ]]

printf 'case: escalates a Ctrl-C timeout to SIGTERM before restarting\n'
reset_server
new_shell term
STATE_TERM="$TMP/state-term"
start_daemon term:0.0 "$STATE_TERM" 1
TERM_OLD=$(cat "$STATE_TERM/current.pid")
OUTPUT=$(run_restart "$STATE_TERM" 2>&1)
TERM_NEW=$(cat "$STATE_TERM/current.pid")
[[ "$TERM_NEW" != "$TERM_OLD" ]]
assert_contains "$OUTPUT" "pid=$TERM_OLD did not exit after Ctrl-C; sending SIGTERM"
assert_contains "$OUTPUT" "new_pid=$TERM_NEW"

printf 'case: refuses when no daemon pane exists\n'
reset_server
new_shell empty
STATE_EMPTY="$TMP/state-empty"
mkdir -p "$STATE_EMPTY"
if OUTPUT=$(run_restart "$STATE_EMPTY" 2>&1); then
  printf 'restart unexpectedly succeeded with no daemon\n' >&2
  exit 1
fi
assert_contains "$OUTPUT" "no tmux pane is running 'flowspace3 daemon'"
assert_contains "$OUTPUT" "SUMMARY pane=- old_pid=- new_pid=- binary=$FAKE health=not-checked"

printf 'case: lists and refuses multiple daemon panes without stopping either\n'
reset_server
new_shell first
new_shell second
STATE_FIRST="$TMP/state-first"
STATE_SECOND="$TMP/state-second"
start_daemon first:0.0 "$STATE_FIRST"
start_daemon second:0.0 "$STATE_SECOND"
FIRST_PID=$(cat "$STATE_FIRST/current.pid")
SECOND_PID=$(cat "$STATE_SECOND/current.pid")
if OUTPUT=$(run_restart "$STATE_FIRST" 2>&1); then
  printf 'restart unexpectedly succeeded with multiple daemons\n' >&2
  exit 1
fi
assert_contains "$OUTPUT" "multiple daemon candidates found; refusing to guess"
assert_contains "$OUTPUT" "pid=$FIRST_PID"
assert_contains "$OUTPUT" "pid=$SECOND_PID"
kill -0 "$FIRST_PID"
kill -0 "$SECOND_PID"

printf 'daemon-restart scratch tmux tests: PASS\n'
