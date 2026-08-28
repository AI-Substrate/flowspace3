#!/usr/bin/env bash
# probe.sh — plan 006-worktree-diff, phase 1 (INVESTIGATE).
#
# Measures what fs3 does TODAY with a git worktree of an already-registered
# repository. Four probes, one variable each:
#
#   P1 discovery      create a worktree, touch nothing — is it seen?
#   P2 incremental    register it, then edit K files — what scans, what is PAID?
#   P3 resolution     same query from main and from the worktree — which version?
#   P4 removal        remove the worktree — what dereferences, what does GC reap?
#
# Evidence, never conclusions: every claim this script supports is a file under
# out/<run-id>/ — DB snapshots, job deltas, CLI envelopes, daemon log slices.
#
# It runs against the LIVE shared stack (prime's ruling 2026-08-28: fidelity is
# the experiment; an isolated stack would measure a daemon nobody runs). It is
# therefore READ-ONLY on the database — it measures through SELECTs, the CLI and
# the daemon log, and never mutates store state by hand.
#
# Usage:
#   ./probe.sh                 # all four probes, straight through
#   ./probe.sh --gate-p4       # pause before P4 until out/<run>/GO-P4 exists
#   ./probe.sh --k 8 --main /path/to/registered/clone
#
# The probe worktree always uses a poctest- slug and is always torn down.

# -E so the ERR trap is INHERITED by functions, command substitutions and
# subshells. Without it `trap … ERR` fires only at top level, which is why two
# go-live runs exited 1 from inside a helper and wrote no abort line at all:
# `set -e` killed the script and the reporter never ran. A diagnostic that is
# silent in exactly the case it exists for is worse than no diagnostic.
set -Eeuo pipefail

# (There is no trace knob. See the note above `say` for why xtrace cannot work
# in this script on this shell without destroying the evidence it captures.)

# ---------------------------------------------------------------- parameters
MAIN_ROOT=/Users/jordanknight/substrate/flowspace/flowspace3
K=8
GATE_P4=0
DB_CONTAINER=${FS3_PROBE_DB_CONTAINER:-flowspace3-db}
DB_USER=${FS3_PROBE_DB_USER:-flowspace3}
DB_NAME=${FS3_PROBE_DB_NAME:-flowspace3}
DAEMON_URL=${FS3_PROBE_DAEMON_URL:-}
DAEMON_LOG_DIR=${FS3_DAEMON__LOG_DIR:-"$HOME/.local/state/flowspace3/logs"}
DAEMON_LOG="$DAEMON_LOG_DIR/flowspace3.log"
PROBE_CONDITION=${FS3_PROBE_CONDITION:-unspecified}
SETTLE_DISCOVERY=70   # two 30s worktree cadences plus 10s scheduling slack
SETTLE_DEBOUNCE=20    # indexing.debounce_seconds default is 10
DRAIN_TIMEOUT=600     # seconds to wait for the PROBE's own scan jobs
ENRICH_SETTLE=60      # seconds to let content-keyed enrichment follow a scan

while [[ $# -gt 0 ]]; do
  case "$1" in
    --main) MAIN_ROOT=$2; shift 2 ;;
    --k) K=$2; shift 2 ;;
    --gate-p4) GATE_P4=1; shift ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
RUN_ID=$(date -u +%Y%m%dT%H%M%SZ)
OUT="$HERE/out/run-$RUN_ID"
SLUG="poctest-wtdiff-$RUN_ID"
WT_PARENT=$(dirname "$MAIN_ROOT")
WT="$WT_PARENT/$SLUG"
MARKER="poctest_${RUN_ID}"

mkdir -p "$OUT"
exec > >(tee -a "$OUT/transcript.log") 2>&1
# NO XTRACE HERE, deliberately, and this is the third shape of the same mistake
# so it gets written down rather than re-attempted. On bash 3.2 (macOS system
# bash) xtrace can only go to stderr — `exec {fd}>` and BASH_XTRACEFD are 4.1+
# — and this script captures CLI output with `> file 2>&1`. So enabling xtrace
# writes trace lines INSIDE the captured envelopes, `jq` then fails on the
# corrupted artifact, and the run dies of its own instrumentation.
#
# The ERR trap plus `set -E` already reports the failing line, code and command,
# which is all the trace was ever wanted for. A diagnostic that damages the
# evidence is worse than none.

say() { printf '\n=== %s — %s\n' "$(date -u +%FT%TZ)" "$*"; }
note() { printf '    %s\n' "$*"; }

# ---------------------------------------------------------------- primitives
sql() { docker exec "$DB_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -At -c "$1"; }
sqlt() { docker exec "$DB_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -c "$1"; }
# The binary under test. Defaults to whatever `flowspace3` PATH resolves to —
# which is the INSTALLED release, not this tree's build. First light needs the
# composed binary, so it is an override rather than an assumption.
CLI=${FS3_PROBE_CLI:-flowspace3}
fs3() {
  local command=$1
  shift
  if [[ -n "$DAEMON_URL" ]]; then
    "$CLI" "$command" --daemon-url "$DAEMON_URL" "$@"
  else
    "$CLI" "$command" "$@"
  fi
}


logsnap() { # logsnap <file>
  if [[ -f "$DAEMON_LOG" ]]; then
    cp "$DAEMON_LOG" "$1"
  else
    : > "$1"
  fi
}
logdelta() { # logdelta <before> <label> — new daemon lines since <before>
  # NOTE: bash expands every word of `local` BEFORE assigning any of them, so
  # each name that references an earlier one gets its own statement.
  local before=$1 label=$2
  local after="$OUT/.log-after-$label"
  logsnap "$after"
  diff "$before" "$after" 2>/dev/null | sed -n 's/^> //p' > "$OUT/daemon-$label.log" || true
  note "daemon log slice: $(wc -l < "$OUT/daemon-$label.log" | tr -d ' ') lines -> daemon-$label.log"
}

snap() { # snap <label> — durable DB counters
  local label=$1
  local f="$OUT/snap-$label.env"
  {
    echo "at=$(date -u +%FT%TZ)"
    echo "jobs_max_id=$(sql 'select coalesce(max(id),0) from jobs')"
    echo "worktrees=$(sql 'select count(*) from worktrees')"
    echo "worktree_files=$(sql 'select count(*) from worktree_files')"
    echo "elements=$(sql 'select count(*) from elements')"
    echo "smart_content=$(sql 'select count(*) from smart_content')"
    echo "embeddings=$(sql 'select count(*) from embeddings_1024')"
    echo "repos=$(sql 'select count(*) from repos')"
    echo "probe_worktree_rows=$(sql "select count(*) from worktrees where root_path = '$WT'")"
    echo "probe_worktree_files=$(sql "select count(*) from worktree_files wf join worktrees w on w.id = wf.worktree_id where w.root_path = '$WT'")"
  } > "$f"
  note "snapshot $label -> snap-$label.env"
  cat "$f" | sed 's/^/      /'
}

deltas() { # deltas <a> <b> <outfile> — every counter that moved
  local a="$OUT/snap-$1.env" b="$OUT/snap-$2.env" out=$3
  {
    printf '%-24s %12s %12s %12s\n' counter "$1" "$2" delta
    while IFS='=' read -r k va; do
      [[ "$k" == at ]] && continue
      vb=$(grep "^$k=" "$b" | cut -d= -f2-)
      printf '%-24s %12s %12s %12s\n' "$k" "$va" "$vb" "$((vb - va))"
    done < "$a"
  } > "$out"
  cat "$out" | sed 's/^/      /'
}

jobs_since() { # jobs_since <snap-label> <outfile>
  local base
  base=$(grep '^jobs_max_id=' "$OUT/snap-$1.env" | cut -d= -f2)
  sqlt "select kind, state, count(*) from jobs where id > $base group by 1,2 order by 1,2" > "$2"
  cat "$2" | sed 's/^/      /'
}

paid_since() { # paid_since <iso-ts> <outfile> — rows a provider was paid for
  # Global counts include any concurrent work by other seats on the OTHER
  # registered roots (this runs on the live shared stack, by ruling), so every
  # global number is paired with a probe-ATTRIBUTABLE one: enrichment whose
  # content is reachable from the probe worktree, and the subset of that which
  # NO other checkout references (the divergent-only spend).
  {
    echo "-- summaries stored since $1 (global: includes other seats' work)"
    sqlt "select count(*) as summaries_global from smart_content where created_at > '$1'"
    echo "-- embeddings stored since $1 (global)"
    sqlt "select source_kind, count(*) from embeddings_1024 where created_at > '$1' group by 1 order by 1"
    echo "-- summaries attributable to the probe worktree, and divergent-only"
    sqlt "select
            count(*) filter (where reachable_from_probe) as summaries_probe,
            count(*) filter (where reachable_from_probe and not reachable_from_main) as summaries_divergent_only
          from (
            select sc.raw_hash,
                   exists (select 1 from elements e
                             join worktree_files wf on wf.blob_sha = e.blob_sha
                             join worktrees w on w.id = wf.worktree_id
                            where e.raw_hash = sc.raw_hash and w.root_path = '$WT') as reachable_from_probe,
                   exists (select 1 from elements e
                             join worktree_files wf on wf.blob_sha = e.blob_sha
                             join worktrees w on w.id = wf.worktree_id
                            where e.raw_hash = sc.raw_hash and w.root_path = '$MAIN_ROOT') as reachable_from_main
              from smart_content sc where sc.created_at > '$1') t"
    echo "-- raw embeddings attributable to the probe worktree, and divergent-only"
    sqlt "select
            count(*) filter (where reachable_from_probe) as embeddings_probe,
            count(*) filter (where reachable_from_probe and not reachable_from_main) as embeddings_divergent_only
          from (
            select em.source_hash,
                   exists (select 1 from elements e
                             join worktree_files wf on wf.blob_sha = e.blob_sha
                             join worktrees w on w.id = wf.worktree_id
                            where e.raw_hash = em.source_hash and w.root_path = '$WT') as reachable_from_probe,
                   exists (select 1 from elements e
                             join worktree_files wf on wf.blob_sha = e.blob_sha
                             join worktrees w on w.id = wf.worktree_id
                            where e.raw_hash = em.source_hash and w.root_path = '$MAIN_ROOT') as reachable_from_main
              from embeddings_1024 em where em.created_at > '$1' and em.source_kind = 'raw') t"
    echo "-- provider CALL lines in the daemon log window are counted separately (daemon-*.log)"
  } > "$2"
  cat "$2" | sed 's/^/      /'
}

wait_idle() { # wait_idle <label>
  # Scoped to THIS probe, deliberately. `flowspace3 status` reports one global
  # queue for the whole daemon, so waiting on it makes the measurement hostage
  # to every other seat sharing the stack — a run of this script sat 15 minutes
  # behind another seat's 7.8k-job root add (2026-08-28). Scan jobs are keyed
  # `scan:{worktree_id}:{path}`, which is the probe's own slice; enrichment is
  # keyed by CONTENT and cannot be attributed to a root, so it gets a bounded
  # settle plus the global figure recorded rather than blocked on.
  local label=$1
  local waited=0 mine wt_id global
  wt_id=$(sql "select coalesce(max(id),0) from worktrees where root_path = '$WT'")
  while (( waited < DRAIN_TIMEOUT )); do
    mine=$(sql "select count(*) from jobs where state in ('pending','running') and dedupe_key like 'scan:${wt_id}:%'")
    global=$(sql "select count(*) from jobs where state in ('pending','running')")
    if [[ "$mine" == "0" ]]; then
      note "probe scans drained after ${waited}s ($label); daemon-wide queue at $global (other seats)"
      break
    fi
    note "probe scans left: $mine (daemon-wide $global) — ${waited}s, $label"
    sleep 5; waited=$((waited + 5))
  done
  (( waited < DRAIN_TIMEOUT )) || note "WARNING: probe scans still pending after ${DRAIN_TIMEOUT}s ($label)"
  # Enrichment for the content those scans produced rides the shared runner.
  note "settling ${ENRICH_SETTLE}s for enrichment of the probe's own content"
  sleep "$ENRICH_SETTLE"
  echo "queue_global_at_${label}=$(sql "select count(*) from jobs where state in ('pending','running')")" >> "$OUT/receipt.env"
}

env_receipt() {
  {
    echo "run_id=$RUN_ID"
    echo "date_utc=$(date -u +%FT%TZ)"
    echo "host=$(hostname)"
    echo "flowspace3=$("$CLI" --version 2>&1) [$CLI]"
    echo "git=$(git --version)"
    echo "docker=$(docker --version)"
    echo "cargo=$(cargo --version 2>/dev/null || echo absent)"
    echo "main_root=$MAIN_ROOT"
    echo "probe_worktree=$WT"
    echo "k_edited_files=$K"
    echo "daemon_url=${DAEMON_URL:-default}"
    echo "daemon_log=$DAEMON_LOG"
    echo "database=$DB_NAME"
    echo "probe_condition=$PROBE_CONDITION"
    echo "discovery_wait_seconds=$SETTLE_DISCOVERY"
    echo "discovery_wait_reason=two 30s worktree cadences plus 10s scheduling slack"
    echo "disk_avail=$(df -h "$MAIN_ROOT" | awk 'NR==2{print $4}')"
  } > "$OUT/receipt.env"
  cat "$OUT/receipt.env" | sed 's/^/      /'
}

# ---------------------------------------------------------------- teardown
teardown() {
  local rc=$?
  say "TEARDOWN (always runs)"
  # Safety: only ever touch a poctest- path.
  if [[ "$WT" == *"/poctest-"* ]]; then
    if fs3 status 2>/dev/null | jq -e --arg p "$WT" '.data.roots[]?|select(.root_path==$p)' >/dev/null; then
      note "unregistering probe root from fs3"
      fs3 remove "$WT" > "$OUT/teardown-remove.json" 2>&1 || true
    fi
    if [[ -d "$WT" ]]; then
      note "removing worktree $WT"
      git -C "$MAIN_ROOT" worktree remove --force "$WT" >/dev/null 2>&1 || rm -rf "$WT"
    fi
    git -C "$MAIN_ROOT" worktree prune >/dev/null 2>&1 || true
    git -C "$MAIN_ROOT" branch -D "$SLUG" >/dev/null 2>&1 || true
  fi
  # Full-file snapshots are scratch; the slice between them is retained.
  rm -f "$OUT"/.log-before-* "$OUT"/.log-after-*
  note "evidence: $OUT"
  note "disk avail: $(df -h "$MAIN_ROOT" | awk 'NR==2{print $4}')"
  exit $rc
}
# EXIT alone is not enough: a supervisor's SIGTERM kills the script without it,
# and a probe worktree plus a registered root survive the run (measured
# 2026-08-28 — the cleanup then falls to a human).
trap teardown EXIT INT TERM

# A probe that dies silently is the same misleading-signal class this script
# exists to refuse: two go-live attempts aborted mid-P3 with no line, no
# command and no status in the transcript, which cost more diagnosis than the
# bugs did. `set -e` is a good default and a terrible reporter, so make it one.
trap 'rc=$?; printf "\n!!! ABORT at line %s (exit %s): %s\n" "$LINENO" "$rc" "$BASH_COMMAND" | tee -a "$OUT/abort.txt"' ERR

# ---------------------------------------------------------------- preflight
say "PREFLIGHT"
fs3 ping > "$OUT/ping.json"
cat "$OUT/ping.json" | sed 's/^/      /'
docker exec "$DB_CONTAINER" pg_isready -U "$DB_USER" -d "$DB_NAME" | sed 's/^/      /'
fs3 status > "$OUT/status-before.json"
jq -r '.data.roots[] | "      root \(.files)\t\(.root_path)"' "$OUT/status-before.json"
if ! jq -e --arg p "$MAIN_ROOT" '.data.roots[]|select(.root_path==$p)' "$OUT/status-before.json" >/dev/null; then
  echo "FATAL: $MAIN_ROOT is not a registered root — nothing to diff a worktree against" >&2
  exit 1
fi
env_receipt
snap 00-baseline
T0=$(date -u +%FT%TZ)

# ---------------------------------------------------------------- P1
say "P1 — create a worktree of a registered repo and touch NOTHING"
logsnap "$OUT/.log-before-p1"
git -C "$MAIN_ROOT" worktree add -b "$SLUG" "$WT" HEAD > "$OUT/p1-worktree-add.txt" 2>&1
cat "$OUT/p1-worktree-add.txt" | sed 's/^/      /'
note "waiting ${SETTLE_DISCOVERY}s (two 30s worktree cadences plus scheduling slack)"
sleep "$SETTLE_DISCOVERY"
fs3 status > "$OUT/p1-status.json"
snap 01-after-worktree-create
deltas 00-baseline 01-after-worktree-create "$OUT/p1-deltas.txt"
jobs_since 00-baseline "$OUT/p1-jobs.txt"
logdelta "$OUT/.log-before-p1" p1
P1_SEEN=$(jq -r --arg p "$WT" '[.data.roots[]|select(.root_path==$p)]|length' "$OUT/p1-status.json")
note "ANSWER P1: probe worktree present in registered roots = $P1_SEEN (1 = auto-discovered, 0 = not)"
echo "p1_auto_discovered=$P1_SEEN" >> "$OUT/receipt.env"

# Prove the steady-state bound with a worktree-scoped job delta. The shared
# queue may move while this sleeps; only jobs carrying this worktree id count.
if (( P1_SEEN == 1 )); then
  P1_WORKTREE_ID=$(sql "select id from worktrees where root_path = '$WT'")
  P1_NOOP_BASE=$(sql "select coalesce(max(id),0) from jobs")
  note "waiting ${SETTLE_DISCOVERY}s across unchanged reconcile passes"
  sleep "$SETTLE_DISCOVERY"
  sqlt "select id, kind, dedupe_key from jobs
         where id > $P1_NOOP_BASE
           and payload->>'worktree_id' = '$P1_WORKTREE_ID'
         order by id" > "$OUT/p1-noop-jobs.txt"
  P1_NOOP_JOBS=$(sql "select count(*) from jobs
                       where id > $P1_NOOP_BASE
                         and payload->>'worktree_id' = '$P1_WORKTREE_ID'")
else
  : > "$OUT/p1-noop-jobs.txt"
  P1_NOOP_JOBS=-1
fi
note "ANSWER P1b: jobs enqueued for unchanged registered worktree = $P1_NOOP_JOBS"
echo "p1_unchanged_reconcile_jobs=$P1_NOOP_JOBS" >> "$OUT/receipt.env"

# Ask from inside the automatically registered worktree and retain the scope
# envelope as evidence that P1 closed the earlier unregistered warning.
( cd "$WT" && fs3 search "cli client that talks to the fs3 daemon over http" --limit 3 ) \
  > "$OUT/p1-search-from-worktree.json" 2>&1
note "scope the daemon resolved for the discovered checkout: $(jq -c '.meta.scope // "none"' "$OUT/p1-search-from-worktree.json")"
note "steer: $(jq -r '.next_action // "none"' "$OUT/p1-search-from-worktree.json")"
# What does an unlimited search return today? (Jordan 2026-08-28: search should
# default to a five-or-ten item cap — recorded here as the before-state.)
( cd "$MAIN_ROOT" && fs3 search "how does the daemon decide what to index" ) \
  > "$OUT/p1-search-default-limit.json" 2>&1
note "default (no --limit) result count = $(jq '.data.results | length' "$OUT/p1-search-default-limit.json")"
echo "default_search_result_count=$(jq '.data.results | length' "$OUT/p1-search-default-limit.json")" >> "$OUT/receipt.env"

# ---------------------------------------------------------------- P2
say "P2a — explicitly re-add the auto-discovered tree: what does unchanged content cost?"
logsnap "$OUT/.log-before-p2a"
T_P2A=$(date -u +%FT%TZ)
snap 02-before-add
fs3 add "$WT" > "$OUT/p2a-add.json" 2>&1
cat "$OUT/p2a-add.json" | sed 's/^/      /'
wait_idle p2a
snap 03-after-add
deltas 02-before-add 03-after-add "$OUT/p2a-deltas.txt"
jobs_since 02-before-add "$OUT/p2a-jobs.txt"
paid_since "$T_P2A" "$OUT/p2a-paid.txt"
logdelta "$OUT/.log-before-p2a" p2a

say "P2b — edit K=$K files, all uncommitted, and measure again"
logsnap "$OUT/.log-before-p2b"
T_P2B=$(date -u +%FT%TZ)
snap 04-before-edits
# bash 3.2 (macOS system bash) has no mapfile.
EDIT_FILES=()
while IFS= read -r line; do
  EDIT_FILES[${#EDIT_FILES[@]}]=$line
done < <(cd "$WT" && git ls-files '*.rs' | grep '^crates/' | grep '/src/' | head -n "$K")
if (( ${#EDIT_FILES[@]} < K )); then
  echo "FATAL: only ${#EDIT_FILES[@]} candidate files found for K=$K" >&2; exit 1
fi
# File 0 is the VERSION PROBE: an existing element diverges in place (same path,
# same address, different content) — that is what P3 asks about. Files 1..K-1
# are appended markers, which measure the cost of divergent content.
# THE STIMULUS MUST BE RETRIEVABLE BY SOMETHING, or it cannot measure exclusion.
#
# It was a nonsense phrase — "pineapple lighthouse semaphore" — chosen because
# it was distinctive TO A HUMAN: lexically unique, obviously the probe's. That
# is the wrong kind of distinctive for a pure-vector index with no lexical
# channel. Run eight proved it with the discriminating control: elements=8,
# vectors=8, and still unranked. The same phrase sits in three indexed elements
# on the live index and a search for it returns none of them.
#
# So the marker is now semantically COHERENT (an embedder can place it: a unit
# conversion, named in the identifier, since doc comments are not indexed) and
# semantically ISOLATED in this corpus (nothing here is about temperature, so
# it competes with nothing). Both properties are required: coherent so it
# embeds somewhere meaningful, isolated so it wins its own query.
#
# DISTINCTIVE-TO-A-HUMAN IS NOT DISTINCTIVE-TO-AN-EMBEDDER.
VERSION_FILE=${EDIT_FILES[0]}
{
  echo ""
  echo "/// Worktree-diff probe marker: exists ONLY in $SLUG, never in main."
  echo "/// (Doc comments are not indexed — the identifier and body carry the"
  echo "/// meaning, which is why the name spells the conversion out.)"
  echo "#[allow(dead_code)]"
  echo "fn ${MARKER}_celsius_to_fahrenheit(celsius_reading: f64) -> f64 {"
  echo "    let fahrenheit = celsius_reading * 9.0 / 5.0 + 32.0;"
  echo "    fahrenheit"
  echo "}"
} >> "$WT/$VERSION_FILE"
for (( i=1; i<K; i++ )); do
  f=${EDIT_FILES[$i]}
  {
    echo ""
    echo "#[allow(dead_code)]"
    echo "fn ${MARKER}_marker_$i() -> u32 { $i }"
  } >> "$WT/$f"
done
printf '      edited: %s\n' "${EDIT_FILES[@]}"
printf '%s\n' "${EDIT_FILES[@]}" > "$OUT/p2b-edited-files.txt"
echo "$VERSION_FILE" > "$OUT/p3-version-file.txt"
note "waiting ${SETTLE_DEBOUNCE}s for the watcher debounce window"
sleep "$SETTLE_DEBOUNCE"
wait_idle p2b
snap 05-after-edits
deltas 04-before-edits 05-after-edits "$OUT/p2b-deltas.txt"
jobs_since 04-before-edits "$OUT/p2b-jobs.txt"
paid_since "$T_P2B" "$OUT/p2b-paid.txt"
logdelta "$OUT/.log-before-p2b" p2b
sqlt "select w.root_path, count(*) as files, count(distinct wf.blob_sha) as blobs
        from worktrees w join worktree_files wf on wf.worktree_id = w.id
       where w.root_path in ('$MAIN_ROOT','$WT') group by 1 order by 1" > "$OUT/p2b-reference-map.txt"
cat "$OUT/p2b-reference-map.txt" | sed 's/^/      /'
sqlt "select wf.path, count(distinct wf.blob_sha) as versions
        from worktree_files wf join worktrees w on w.id = wf.worktree_id
       where w.root_path in ('$MAIN_ROOT','$WT')
       group by 1 having count(distinct wf.blob_sha) > 1 order by 1" > "$OUT/p2b-divergent-paths.txt"
cat "$OUT/p2b-divergent-paths.txt" | sed 's/^/      /'

# ---------------------------------------------------------------- P3
say "P3 — same query from main and from inside the worktree"
# The query MUST target text that actually enters an element. A doc comment
# above a function does not: elements carry the item's own span, so a marker
# hidden in a doc comment is unfindable for a reason that has nothing to do
# with worktrees. The phrase below is inside the probe function's BODY.
# Describes the marker the way a caller would ask for it — meaning, not tokens.
Q_MARKER="convert a celsius temperature reading into fahrenheit"
Q_SHARED="cli client that talks to the fs3 daemon over http"
IDENTITY=$(sql "select r.identity from repos r join worktrees w on w.repo_id = r.id where w.root_path = '$WT'")
FILE_ADDRESS="el:$IDENTITY/$VERSION_FILE"
echo "version_file_address=$FILE_ADDRESS" >> "$OUT/receipt.env"

# A REFUSAL IS DATA HERE, NOT A FAILURE — and this is what killed the first five
# go-live attempts. `flowspace3 get` exits non-zero when it refuses, and under
# `set -e` that ended the run before a single p3_* predicate was written. The
# refusal it kept hitting is honest and useful: "…is registered but has not been
# parsed yet, so it has no elements to read" — the worktree's edits were queued
# behind a starved scan lane, so the caller's version genuinely did not exist
# yet. The CLI was right, the probe was wrong to die of it.
#
# These predicates read the ENVELOPES, never the exit codes; an ok:false
# envelope is a measurement. So capture the envelope and carry on, and let the
# gate below decide what it means.
# WHAT DOES THE INDEX ACTUALLY HOLD FOR THE MARKER, at the moment we query it?
#
# "Not retrievable" has two very different causes and the receipt must say
# which: the marker was never parsed or embedded (a timing/queue fact about
# this run), or it is present and simply does not rank (a retrieval fact about
# the product). Run seven could not distinguish them, which is one refusal too
# many for a control that exists to prevent exactly that ambiguity.
MARKER_ELEMENTS=$(sql "select count(*) from elements where name like '${MARKER}%'")
MARKER_VECTORS=$(sql "
  select count(*) from elements e
   where e.name like '${MARKER}%'
     and exists (select 1 from embeddings_1024 em
                  where em.source_hash = e.raw_hash and em.source_kind = 'raw')")
note "marker index state at query time: elements=$MARKER_ELEMENTS with raw vectors=$MARKER_VECTORS"
echo "p3_marker_elements=$MARKER_ELEMENTS" >> "$OUT/receipt.env"
echo "p3_marker_vectors=$MARKER_VECTORS" >> "$OUT/receipt.env"

for cwd in "$MAIN_ROOT" "$WT"; do
  tag=main; [[ "$cwd" == "$WT" ]] && tag=worktree
  ( cd "$cwd" && fs3 search "$Q_MARKER" --limit 5 --source raw ) > "$OUT/p3-marker-from-$tag.json" 2>&1 || true
  ( cd "$cwd" && fs3 search "$Q_SHARED" --limit 5 ) > "$OUT/p3-shared-from-$tag.json" 2>&1 || true
  ( cd "$cwd" && fs3 get "$FILE_ADDRESS" ) > "$OUT/p3-get-from-$tag.json" 2>&1 || true
  note "from $tag — divergent-content query (the function exists ONLY in the worktree):"
  jq -r '.data.results[]? | "      \(.score|tostring[0:6])  \(.path)  \(.name)"' "$OUT/p3-marker-from-$tag.json" || true
  note "from $tag — meta.scope the daemon resolved: $(jq -c '.meta.scope // "none"' "$OUT/p3-marker-from-$tag.json")"
  note "from $tag — get $VERSION_FILE resolved to span $(jq -c '.data.span // .data.parents[0].span // "none"' "$OUT/p3-get-from-$tag.json")"
done

# EMBEDDER GATE — read this before trusting any p3_* number.
#
# P1, P2 and P4 measure bookkeeping: rows, jobs, registrations, reclamation.
# They are true under any provider. P3 measures RETRIEVAL, which only means
# anything when the vectors carry semantics — so under a FAKE embedder the
# marker function ranks nowhere, both checkouts return the same garbage, and
# the naive predicates read "no version resolution, no leak". Both zeros look
# like results and are artifacts. Measured on the first composed run
# (2026-08-28): 8 divergent vectors existed, and the best score for a query
# quoting the function's own body verbatim was 0.1889 against unrelated files.
#
# So this refuses to emit a verdict it cannot support, and says which run would.
# Two `set -e` traps here, both found by running it (amphibian, 2026-08-28) —
# the script exited 1 mid-P3 on the NORMAL isolated-daemon shape:
#   1. a command substitution whose pipeline ends in a non-matching `grep`
#      returns 1, and an assignment adopts that status;
#   2. `[[ -z X ]] && Y` as the LAST command of a line returns 1 when the test
#      is false, which is the healthy case.
# ASK THE LIVE DAEMON, and fall back to the log only if it will not say.
#
# A gate about CURRENT semantics must not trust a historical mouth. Reading the
# boot line from log history reports what was true when it was written, which
# during a restart window is a state that no longer exists — a go-live run read
# `embedder=offline` from exactly such a window while the daemon was in fact on
# azure_openai, and refused a measurement it could have made. `ping` answers
# for the process that is running right now.
embedder_live() { fs3 ping 2>/dev/null | grep -oE 'embedder: [a-z_]+' | tail -1 | cut -d' ' -f2 || true; }
embedder_from() { grep -oE 'embedder=[a-z_]+' "$1" 2>/dev/null | tail -1 | cut -d= -f2 || true; }
EMBEDDER=$(embedder_live)
EMBEDDER_SOURCE=daemon-ping
if [[ -z "$EMBEDDER" ]]; then
  EMBEDDER=$(embedder_from "$OUT/daemon-p1.log")
  EMBEDDER_SOURCE=probe-log-slice
fi
if [[ -z "$EMBEDDER" ]]; then
  EMBEDDER=$(embedder_from "$DAEMON_LOG")
  EMBEDDER_SOURCE=daemon-log-history
fi
echo "embedder=${EMBEDDER:-unknown}" >> "$OUT/receipt.env"
echo "embedder_source=$EMBEDDER_SOURCE" >> "$OUT/receipt.env"

answer_identity() { jq -S '[.data.results[] | {address, path, name, kind, span, snippet}]' "$1"; }
P3_LEAK=$(jq -r --arg m "$MARKER" '[.data.results[]?|select(.name|startswith($m))]|length' "$OUT/p3-marker-from-main.json")
P3_FOUND=$(jq -r --arg m "$MARKER" '[.data.results[]?|select(.name|startswith($m))]|length' "$OUT/p3-marker-from-worktree.json")

# ALLOW-LIST the embedders that PROVE semantics; refuse everything else.
#
# This was a deny-list ("fake" or "unknown") until the production daemon turned
# out to report `embedder=offline` — a third value meaning exactly "no real
# vectors", which sailed straight through a gate built to catch it. Deny-lists
# fail open on the case nobody thought of, and the whole point of this gate is
# that a number nobody can defend is worse than a refusal.
#
# So: name the providers whose vectors carry meaning. Anything else — a new
# provider, a typo, an empty boot line, `offline`, `fake` — refuses, and the
# refusal says which value it saw so the next person can add it deliberately
# rather than discover it as a wrong answer.
# The names are the kinds' own serde spellings in crates/core/src/config.rs:
# `openai` (:666), `openai_compat` (:682), `azure_openai` (:707). `fake` (:664)
# and `offline` (the fresh-machine default, :85-107) are the two that mean
# "no real vectors" — and `offline` is the one that taught this gate the
# difference between a deny-list and an allow-list.
case "$EMBEDDER" in
  azure_openai|openai|openai_compat) SEMANTIC_EMBEDDER=1 ;;
  *) SEMANTIC_EMBEDDER=0 ;;
esac
if (( SEMANTIC_EMBEDDER == 0 )); then
  reason="embedder-${EMBEDDER:-unknown}"
  note "ANSWER P3: NOT MEASURABLE — embedder=${EMBEDDER:-unknown}, so retrieval semantics are not established"
  note "           (p1/p2/p4 above are unaffected: they measure bookkeeping, not ranking)"
  echo "p3_search_context_sensitive=unmeasurable-$reason" >> "$OUT/receipt.env"
  echo "p3_wrong_version_leak_to_main=unmeasurable-$reason" >> "$OUT/receipt.env"
  echo "p3_note=rerun against a daemon whose boot line proves a real embedder, over an already-embedded corpus" >> "$OUT/receipt.env"
elif (( P3_FOUND == 0 )); then
  # The control: the worktree's OWN divergent function must be findable FROM the
  # worktree. If it is not, the run proves nothing about resolution — and a
  # leak of 0 would be measuring absence, not exclusion.
  #
  # Say WHICH absence, because the two mean opposite things: not indexed yet is
  # a fact about this run's timing, while indexed-and-not-ranked is a fact about
  # retrieval and would be a finding.
  if (( MARKER_VECTORS == 0 )); then
    marker_reason="marker-not-indexed-yet (elements=$MARKER_ELEMENTS, vectors=$MARKER_VECTORS)"
    note "ANSWER P3: NOT MEASURABLE — the marker has no raw vector yet; its enrichment had not landed when the query ran"
  else
    marker_reason="marker-indexed-but-unranked (elements=$MARKER_ELEMENTS, vectors=$MARKER_VECTORS)"
    note "ANSWER P3: NOT MEASURABLE — the marker IS indexed with $MARKER_VECTORS raw vector(s) and still did not rank."
    note "           That is a RETRIEVAL finding, not a timing one — report it rather than re-running."
  fi
  echo "p3_search_context_sensitive=unmeasurable-$marker_reason" >> "$OUT/receipt.env"
  echo "p3_wrong_version_leak_to_main=unmeasurable-$marker_reason" >> "$OUT/receipt.env"
else
  if diff -q <(answer_identity "$OUT/p3-marker-from-main.json") \
            <(answer_identity "$OUT/p3-marker-from-worktree.json") >/dev/null; then
    note "ANSWER P3a: search returns the IDENTICAL answer to both checkouts — no version resolution"
    echo "p3_search_context_sensitive=0" >> "$OUT/receipt.env"
  else
    note "ANSWER P3a: search answers DIFFER between checkouts — version resolution present"
    echo "p3_search_context_sensitive=1" >> "$OUT/receipt.env"
    diff <(answer_identity "$OUT/p3-marker-from-main.json") \
         <(answer_identity "$OUT/p3-marker-from-worktree.json") > "$OUT/p3-answer-diff.txt" || true
  fi
  note "ANSWER P3a2: worktree-only functions served to the MAIN checkout = $P3_LEAK (found from its own worktree: $P3_FOUND)"
  echo "p3_wrong_version_leak_to_main=$P3_LEAK" >> "$OUT/receipt.env"
  echo "p3_marker_found_from_worktree=$P3_FOUND" >> "$OUT/receipt.env"
fi
# Same discipline as the embedder gate: refuse a verdict the evidence cannot
# support. If EITHER checkout's `get` was refused — most often "registered but
# not parsed yet", which means the caller's version does not exist in the index
# at this instant — then a difference between the two answers is a difference
# between an answer and an error, not between two versions.
GET_MAIN_OK=$(jq -r '.ok // false' "$OUT/p3-get-from-main.json" 2>/dev/null || echo false)
GET_WT_OK=$(jq -r '.ok // false' "$OUT/p3-get-from-worktree.json" 2>/dev/null || echo false)
if [[ "$GET_MAIN_OK" != "true" || "$GET_WT_OK" != "true" ]]; then
  why=$(jq -r '.error.message // "refused"' "$OUT/p3-get-from-worktree.json" 2>/dev/null || echo refused)
  note "ANSWER P3b: NOT MEASURABLE — a get was refused (main ok=$GET_MAIN_OK, worktree ok=$GET_WT_OK): $why"
  echo "p3_get_context_sensitive=unmeasurable-get-refused" >> "$OUT/receipt.env"
  echo "p3_get_refusal=$why" >> "$OUT/receipt.env"
elif diff -q <(jq -S '.data' "$OUT/p3-get-from-main.json") \
            <(jq -S '.data' "$OUT/p3-get-from-worktree.json") >/dev/null; then
  note "ANSWER P3b: get returns the SAME version to both checkouts"
  echo "p3_get_context_sensitive=0" >> "$OUT/receipt.env"
else
  note "ANSWER P3b: get returns a DIFFERENT version per checkout — context-aware today"
  echo "p3_get_context_sensitive=1" >> "$OUT/receipt.env"
fi

# Per-RESULT provenance: does a hit say which checkout it came from?
jq -r '.data.results[0] // {} | keys | join(", ")' "$OUT/p3-marker-from-worktree.json" > "$OUT/p3-result-fields.txt" 2>&1 || true
note "result envelope fields: $(cat "$OUT/p3-result-fields.txt")"

# RESOLVED-ROW INVARIANT — measurable under ANY embedder, because it checks
# resolution rather than ranking.
#
# Every returned hit must name the checkout that served it and the path it lives
# at. A row with a null path is a row the query admitted and then failed to
# resolve, and it reaches the caller as a hit with no file behind it.
#
# This exists because the composed build shipped exactly that (2026-08-28):
# the candidate gate proved a caller-anchored element carried the vector's
# raw_hash, but the representative resolver then picked the globally lowest-id
# element with that hash WITHOUT re-applying the caller scope — so with one
# body embedded in several blobs, which is what content-addressed enrichment
# exists to produce, it chose a foreign blob, the provenance LEFT JOINs found
# nothing, and the row survived with identity, path and root all null.
# Neither unit's tests could see it: it needs many checkouts of ONE repo, which
# only became the normal shape once worktrees were auto-registered.
#
# The general rule it encodes: a scope filter over content-addressed storage
# must be applied at every step that CHOOSES a row, not only where one is
# ADMITTED.
NULL_ROWS=0
for f in "$OUT"/p3-marker-from-*.json "$OUT"/p3-shared-from-*.json; do
  [[ -f "$f" ]] || continue
  n=$(jq '[.data.results[]? | select(.path == null or .worktree == null)] | length' "$f" 2>/dev/null || echo 0)
  NULL_ROWS=$((NULL_ROWS + n))
done
note "ANSWER P3c: hits returned without a resolved path/checkout = $NULL_ROWS (MUST be 0)"
echo "p3_unresolved_rows=$NULL_ROWS" >> "$OUT/receipt.env"
if (( NULL_ROWS > 0 )); then
  note "  ^ these are hits with no file behind them — see the resolved-row invariant note in this script"
  jq -c '.data.results[]? | select(.path == null or .worktree == null)' "$OUT"/p3-*-from-*.json \
    > "$OUT/p3-unresolved-rows.json" 2>/dev/null || true
fi

# EXPOSURE, not a gate — read the distinction before adding a threshold to it.
#
# This counts a DATA SHAPE: raw_hashes that pass the candidate gate for the
# caller (some caller-held blob carries an element with that hash) while the
# globally lowest-id element carrying it sits in a blob the caller does NOT
# hold. That is the population the resolver has to get right.
#
# It is deliberately NOT asserted to be zero, and the reason matters: the fix
# changes which row a query CHOOSES, not which ids exist. This number stays
# non-zero on any healthy multi-checkout database — the probe worktree creates
# the shape by construction, because appending to a file changes the FILE blob
# while leaving every pre-existing function's raw_hash identical. A "must be 0"
# here would be a gate that can never go green, which is the same misleading
# signal class as the fake-embedder zero this script already refuses to emit.
#
# So: p3_unresolved_rows is the PASS/FAIL invariant (a hit with no file behind
# it is always wrong), and this is the exposure it was measured against — how
# much of the corpus the resolver had the opportunity to get wrong. Restricted
# to hashes that actually carry a raw vector, because only those can be
# returned by a search at all. Amphibian's review measured 227 this way on a
# 20-checkout database (assets/reviews/runtime/uc-resolver-mismatch.json).
FOREIGN_REPS=$(sql "
  with wt as (select id from worktrees where root_path = '$WT'),
       held as (select distinct wf.blob_sha from worktree_files wf join wt on wf.worktree_id = wt.id),
       gated as (select distinct e.raw_hash from elements e join held h on h.blob_sha = e.blob_sha
                  where exists (select 1 from embeddings_1024 em
                                 where em.source_hash = e.raw_hash and em.source_kind = 'raw')),
       rep as (select distinct on (e.raw_hash) e.raw_hash, e.blob_sha
                 from elements e where e.raw_hash in (select raw_hash from gated)
                order by e.raw_hash, e.id)
  select count(*) from rep left join held h on h.blob_sha = rep.blob_sha where h.blob_sha is null" 2>/dev/null || echo "unknown")
note "P3d exposure: searchable raw_hashes whose representative sits in a blob the caller does NOT hold = $FOREIGN_REPS"
note "  (expected NON-ZERO on a multi-checkout database — it is the population p3_unresolved_rows=0 was proven against)"
echo "p3_foreign_representative_exposure=$FOREIGN_REPS" >> "$OUT/receipt.env"

# The store's own answer.
sqlt "select left(e.blob_sha,8) as blob, e.address, left(e.raw_text, 50) as raw_head
        from elements e where e.name like '${MARKER}%' order by e.address" > "$OUT/p3-elements-by-blob.txt"
cat "$OUT/p3-elements-by-blob.txt" | sed 's/^/      /'
sqlt "select w.root_path, wf.path, left(wf.blob_sha,8) as blob
        from worktree_files wf join worktrees w on w.id = wf.worktree_id
       where wf.path = '$VERSION_FILE' and w.root_path in ('$MAIN_ROOT','$WT')
       order by 1" > "$OUT/p3-version-file-blobs.txt"
cat "$OUT/p3-version-file-blobs.txt" | sed 's/^/      /'
# One address, two versions: the collision search has no way to disambiguate.
sqlt "select e.address, count(*) as element_rows, count(distinct e.blob_sha) as versions
        from elements e where e.address like '%$VERSION_FILE%'
       group by 1 having count(distinct e.blob_sha) > 1 order by 1 limit 10" > "$OUT/p3-colliding-addresses.txt"
cat "$OUT/p3-colliding-addresses.txt" | sed 's/^/      /'
# Explicit scoping: what does the CLI offer today?
( cd "$WT" && fs3 search "$Q_MARKER" --limit 5 --source raw --repo "$IDENTITY" ) > "$OUT/p3-search-repo-scoped.json" 2>&1 || true
fs3 search --help > "$OUT/p3-search-flags.txt" 2>&1

# ---------------------------------------------------------------- P4 gate
if (( GATE_P4 )); then
  say "P4 GATE — waiting for $OUT/GO-P4 (announce to prime, then: touch $OUT/GO-P4)"
  waited=0
  while [[ ! -f "$OUT/GO-P4" ]] && (( waited < 3600 )); do sleep 5; waited=$((waited+5)); done
  [[ -f "$OUT/GO-P4" ]] || { echo "FATAL: P4 gate timed out" >&2; exit 1; }
  note "GO received after ${waited}s"
fi

# ---------------------------------------------------------------- P4
say "P4 — remove the worktree; measure dereference and GC"
logsnap "$OUT/.log-before-p4"
T_P4=$(date -u +%FT%TZ)
snap 06-before-removal

note "step 1: git worktree remove (fs3 is told NOTHING)"
git -C "$MAIN_ROOT" worktree remove --force "$WT" > "$OUT/p4-git-remove.txt" 2>&1
cat "$OUT/p4-git-remove.txt" | sed 's/^/      /'
sleep "$SETTLE_DISCOVERY"
fs3 status > "$OUT/p4-status-after-git-remove.json"
snap 07-after-git-remove
deltas 06-before-removal 07-after-git-remove "$OUT/p4-deltas-git-remove.txt"
P4_STILL=$(jq -r --arg p "$WT" '[.data.roots[]|select(.root_path==$p)]|length' "$OUT/p4-status-after-git-remove.json")
note "ANSWER P4a: root still registered after the directory vanished = $P4_STILL"
# The content that just vanished from disk: is it still SERVED? Asked from the
# main checkout, where this function has never existed.
( cd "$MAIN_ROOT" && fs3 search "$Q_MARKER" --limit 3 --source raw ) > "$OUT/p4-search-after-git-remove.json" 2>&1
P4_SERVED=$(jq -r --arg m "$MARKER" '[.data.results[]?|select(.name|startswith($m))]|length' "$OUT/p4-search-after-git-remove.json")
note "ANSWER P4b: deleted-worktree functions still returned by search = $P4_SERVED"
echo "p4_root_still_registered=$P4_STILL" >> "$OUT/receipt.env"
echo "p4_deleted_content_still_served=$P4_SERVED" >> "$OUT/receipt.env"
jq -r '.data.results[]? | "      \(.score|tostring[0:6])  \(.path)  \(.name)"' "$OUT/p4-search-after-git-remove.json" || true

note "step 2: flowspace3 remove (the explicit unregister)"
fs3 remove "$WT" > "$OUT/p4-fs3-remove.json" 2>&1 || true
cat "$OUT/p4-fs3-remove.json" | sed 's/^/      /'
snap 08-after-fs3-remove
deltas 07-after-git-remove 08-after-fs3-remove "$OUT/p4-deltas-fs3-remove.txt"

note "step 3: orphaned-paid audit BEFORE gc — enrichment nothing references"
sqlt "select count(*) as orphaned_summaries
        from smart_content sc
       where sc.created_at > '$T0'
         and not exists (select 1 from elements e
                          join worktree_files wf on wf.blob_sha = e.blob_sha
                         where e.raw_hash = sc.raw_hash)" > "$OUT/p4-orphaned-before-gc.txt"
sqlt "select count(*) as orphaned_embeddings
        from embeddings_1024 em
       where em.created_at > '$T0'
         and em.source_kind = 'raw'
         and not exists (select 1 from elements e
                          join worktree_files wf on wf.blob_sha = e.blob_sha
                         where e.raw_hash = em.source_hash)" >> "$OUT/p4-orphaned-before-gc.txt"
cat "$OUT/p4-orphaned-before-gc.txt" | sed 's/^/      /'

note "step 4: gc (database-wide, reap-only for unreferenced content)"
fs3 gc > "$OUT/p4-gc.json" 2>&1 || true
cat "$OUT/p4-gc.json" | sed 's/^/      /'
snap 09-after-gc
deltas 08-after-fs3-remove 09-after-gc "$OUT/p4-deltas-gc.txt"
deltas 00-baseline 09-after-gc "$OUT/p4-deltas-whole-run.txt"
jobs_since 06-before-removal "$OUT/p4-jobs.txt"
sqlt "select count(*) as orphaned_summaries_after_gc
        from smart_content sc
       where sc.created_at > '$T0'
         and not exists (select 1 from elements e
                          join worktree_files wf on wf.blob_sha = e.blob_sha
                         where e.raw_hash = sc.raw_hash)" > "$OUT/p4-orphaned-after-gc.txt"
cat "$OUT/p4-orphaned-after-gc.txt" | sed 's/^/      /'
( cd "$MAIN_ROOT" && fs3 search "$Q_MARKER" --limit 3 --source raw ) > "$OUT/p4-search-after-gc.json" 2>&1
P4_AFTER=$(jq -r --arg m "$MARKER" '[.data.results[]?|select(.name|startswith($m))]|length' "$OUT/p4-search-after-gc.json")
note "ANSWER P4c: probe functions still served AFTER gc = $P4_AFTER (0 = fully reclaimed)"
echo "p4_served_after_gc=$P4_AFTER" >> "$OUT/receipt.env"
sqlt "select count(*) as marker_elements_left from elements where name like '${MARKER}%'" >> "$OUT/p4-orphaned-after-gc.txt"
cat "$OUT/p4-orphaned-after-gc.txt" | sed 's/^/      /'
logdelta "$OUT/.log-before-p4" p4

say "DONE — evidence in $OUT"
