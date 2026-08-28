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

set -euo pipefail

# ---------------------------------------------------------------- parameters
MAIN_ROOT=/Users/jordanknight/substrate/flowspace/flowspace3
K=8
GATE_P4=0
DB_CONTAINER=flowspace3-db
DB_USER=flowspace3
DB_NAME=flowspace3
SETTLE_DISCOVERY=30   # 6 watcher reconcile cadences (5s) — P1 patience
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

say() { printf '\n=== %s — %s\n' "$(date -u +%FT%TZ)" "$*"; }
note() { printf '    %s\n' "$*"; }

# ---------------------------------------------------------------- primitives
sql() { docker exec "$DB_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -At -c "$1"; }
sqlt() { docker exec "$DB_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME" -c "$1"; }

# The daemon runs in the foreground in a tmux pane; its log is that pane.
DAEMON_PANE=$(tmux list-panes -a -F '#{pane_id} #{pane_current_command}' 2>/dev/null \
  | awk '$2=="flowspace3"{print $1; exit}' || true)

logsnap() { # logsnap <file>
  if [[ -n "$DAEMON_PANE" ]]; then
    tmux capture-pane -p -J -S -100000 -t "$DAEMON_PANE" > "$1" 2>/dev/null || : > "$1"
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
    echo "flowspace3=$(flowspace3 --version 2>&1)"
    echo "git=$(git --version)"
    echo "docker=$(docker --version)"
    echo "cargo=$(cargo --version 2>/dev/null || echo absent)"
    echo "main_root=$MAIN_ROOT"
    echo "probe_worktree=$WT"
    echo "k_edited_files=$K"
    echo "daemon_pane=${DAEMON_PANE:-none}"
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
    if flowspace3 status 2>/dev/null | jq -e --arg p "$WT" '.data.roots[]?|select(.root_path==$p)' >/dev/null; then
      note "unregistering probe root from fs3"
      flowspace3 remove "$WT" > "$OUT/teardown-remove.json" 2>&1 || true
    fi
    if [[ -d "$WT" ]]; then
      note "removing worktree $WT"
      git -C "$MAIN_ROOT" worktree remove --force "$WT" >/dev/null 2>&1 || rm -rf "$WT"
    fi
    git -C "$MAIN_ROOT" worktree prune >/dev/null 2>&1 || true
    git -C "$MAIN_ROOT" branch -D "$SLUG" >/dev/null 2>&1 || true
  fi
  # The pane captures are scratch: each is the whole daemon scrollback, and the
  # SLICE between two of them is what the evidence dir keeps (daemon-*.log).
  rm -f "$OUT"/.log-before-* "$OUT"/.log-after-*
  note "evidence: $OUT"
  note "disk avail: $(df -h "$MAIN_ROOT" | awk 'NR==2{print $4}')"
  exit $rc
}
# EXIT alone is not enough: a supervisor's SIGTERM kills the script without it,
# and a probe worktree plus a registered root survive the run (measured
# 2026-08-28 — the cleanup then falls to a human).
trap teardown EXIT INT TERM

# ---------------------------------------------------------------- preflight
say "PREFLIGHT"
flowspace3 ping > "$OUT/ping.json"
cat "$OUT/ping.json" | sed 's/^/      /'
docker exec "$DB_CONTAINER" pg_isready -U "$DB_USER" -d "$DB_NAME" | sed 's/^/      /'
flowspace3 status > "$OUT/status-before.json"
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
note "waiting ${SETTLE_DISCOVERY}s (watcher cadence is 5s; this is 6 passes)"
sleep "$SETTLE_DISCOVERY"
flowspace3 status > "$OUT/p1-status.json"
snap 01-after-worktree-create
deltas 00-baseline 01-after-worktree-create "$OUT/p1-deltas.txt"
jobs_since 00-baseline "$OUT/p1-jobs.txt"
logdelta "$OUT/.log-before-p1" p1
P1_SEEN=$(jq -r --arg p "$WT" '[.data.roots[]|select(.root_path==$p)]|length' "$OUT/p1-status.json")
note "ANSWER P1: probe worktree present in registered roots = $P1_SEEN (1 = auto-discovered, 0 = not)"
echo "p1_auto_discovered=$P1_SEEN" >> "$OUT/receipt.env"

# Before it is registered: what does a search FROM INSIDE the new worktree do?
# (bobolink's case, 2026-08-28 — a worktree's content invisible until the whole
# tree is added as a duplicate root.)
( cd "$WT" && flowspace3 search "cli client that talks to the fs3 daemon over http" --limit 3 ) \
  > "$OUT/p1-search-from-unregistered.json" 2>&1
note "scope the daemon resolved for an unregistered checkout: $(jq -c '.meta.scope // "none"' "$OUT/p1-search-from-unregistered.json")"
note "steer: $(jq -r '.next_action // "none"' "$OUT/p1-search-from-unregistered.json")"
# What does an unlimited search return today? (Jordan 2026-08-28: search should
# default to a five-or-ten item cap — recorded here as the before-state.)
( cd "$MAIN_ROOT" && flowspace3 search "how does the daemon decide what to index" ) \
  > "$OUT/p1-search-default-limit.json" 2>&1
note "default (no --limit) result count = $(jq '.data.results | length' "$OUT/p1-search-default-limit.json")"
echo "default_search_result_count=$(jq '.data.results | length' "$OUT/p1-search-default-limit.json")" >> "$OUT/receipt.env"

# ---------------------------------------------------------------- P2
say "P2a — register the untouched worktree: what does identical content cost?"
logsnap "$OUT/.log-before-p2a"
T_P2A=$(date -u +%FT%TZ)
snap 02-before-add
flowspace3 add "$WT" > "$OUT/p2a-add.json" 2>&1
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
VERSION_FILE=${EDIT_FILES[0]}
{
  echo ""
  echo "/// Worktree-diff probe marker. This doc comment exists ONLY in the probe"
  echo "/// worktree $SLUG and never in the main checkout. It describes a"
  echo "/// pineapple lighthouse semaphore that reconciles marmalade telemetry."
  echo "#[allow(dead_code)]"
  echo "fn ${MARKER}_version_probe() -> &'static str {"
  echo "    \"pineapple lighthouse semaphore\""
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
Q_MARKER="poctest version probe pineapple lighthouse semaphore function"
Q_SHARED="cli client that talks to the fs3 daemon over http"
IDENTITY=$(sql "select r.identity from repos r join worktrees w on w.repo_id = r.id where w.root_path = '$WT'")
FILE_ADDRESS="el:$IDENTITY/$VERSION_FILE"
echo "version_file_address=$FILE_ADDRESS" >> "$OUT/receipt.env"

for cwd in "$MAIN_ROOT" "$WT"; do
  tag=main; [[ "$cwd" == "$WT" ]] && tag=worktree
  ( cd "$cwd" && flowspace3 search "$Q_MARKER" --limit 5 --source raw ) > "$OUT/p3-marker-from-$tag.json" 2>&1
  ( cd "$cwd" && flowspace3 search "$Q_SHARED" --limit 5 ) > "$OUT/p3-shared-from-$tag.json" 2>&1
  ( cd "$cwd" && flowspace3 get "$FILE_ADDRESS" ) > "$OUT/p3-get-from-$tag.json" 2>&1
  note "from $tag — divergent-content query (the function exists ONLY in the worktree):"
  jq -r '.data.results[]? | "      \(.score|tostring[0:6])  \(.path)  \(.name)"' "$OUT/p3-marker-from-$tag.json" || true
  note "from $tag — meta.scope the daemon resolved: $(jq -c '.meta.scope // "none"' "$OUT/p3-marker-from-$tag.json")"
  note "from $tag — get $VERSION_FILE resolved to span $(jq -c '.data.span // .data.parents[0].span // "none"' "$OUT/p3-get-from-$tag.json")"
done

# Is the ANSWER the same from both sides? Compare the result IDENTITIES only.
# `meta.scope` always differs (it echoes the caller's cwd), and scores carry
# float jitter — the query is embedded afresh per call, so the same question
# scored 0.7730240968 and 0.7730564295 microseconds apart on one run. Neither
# is a version difference, and a diff that reports them as one is a broken
# predicate, not a finding.
answer_identity() { jq -S '[.data.results[] | {address, path, name, kind, span, snippet}]' "$1"; }
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
# Was the worktree-only function served to a caller standing in MAIN, where it
# has never existed? That is the leak, stated as a number.
P3_LEAK=$(jq -r --arg m "$MARKER" '[.data.results[]?|select(.name|startswith($m))]|length' "$OUT/p3-marker-from-main.json")
note "ANSWER P3a2: worktree-only functions served to the MAIN checkout = $P3_LEAK"
echo "p3_wrong_version_leak_to_main=$P3_LEAK" >> "$OUT/receipt.env"
if diff -q <(jq -S '.data' "$OUT/p3-get-from-main.json") \
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
( cd "$WT" && flowspace3 search "$Q_MARKER" --limit 5 --source raw --repo "$IDENTITY" ) > "$OUT/p3-search-repo-scoped.json" 2>&1 || true
flowspace3 search --help > "$OUT/p3-search-flags.txt" 2>&1

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
flowspace3 status > "$OUT/p4-status-after-git-remove.json"
snap 07-after-git-remove
deltas 06-before-removal 07-after-git-remove "$OUT/p4-deltas-git-remove.txt"
P4_STILL=$(jq -r --arg p "$WT" '[.data.roots[]|select(.root_path==$p)]|length' "$OUT/p4-status-after-git-remove.json")
note "ANSWER P4a: root still registered after the directory vanished = $P4_STILL"
# The content that just vanished from disk: is it still SERVED? Asked from the
# main checkout, where this function has never existed.
( cd "$MAIN_ROOT" && flowspace3 search "$Q_MARKER" --limit 3 --source raw ) > "$OUT/p4-search-after-git-remove.json" 2>&1
P4_SERVED=$(jq -r --arg m "$MARKER" '[.data.results[]?|select(.name|startswith($m))]|length' "$OUT/p4-search-after-git-remove.json")
note "ANSWER P4b: deleted-worktree functions still returned by search = $P4_SERVED"
echo "p4_root_still_registered=$P4_STILL" >> "$OUT/receipt.env"
echo "p4_deleted_content_still_served=$P4_SERVED" >> "$OUT/receipt.env"
jq -r '.data.results[]? | "      \(.score|tostring[0:6])  \(.path)  \(.name)"' "$OUT/p4-search-after-git-remove.json" || true

note "step 2: flowspace3 remove (the explicit unregister)"
flowspace3 remove "$WT" > "$OUT/p4-fs3-remove.json" 2>&1 || true
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
flowspace3 gc > "$OUT/p4-gc.json" 2>&1 || true
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
( cd "$MAIN_ROOT" && flowspace3 search "$Q_MARKER" --limit 3 --source raw ) > "$OUT/p4-search-after-gc.json" 2>&1
P4_AFTER=$(jq -r --arg m "$MARKER" '[.data.results[]?|select(.name|startswith($m))]|length' "$OUT/p4-search-after-gc.json")
note "ANSWER P4c: probe functions still served AFTER gc = $P4_AFTER (0 = fully reclaimed)"
echo "p4_served_after_gc=$P4_AFTER" >> "$OUT/receipt.env"
sqlt "select count(*) as marker_elements_left from elements where name like '${MARKER}%'" >> "$OUT/p4-orphaned-after-gc.txt"
cat "$OUT/p4-orphaned-after-gc.txt" | sed 's/^/      /'
logdelta "$OUT/.log-before-p4" p4

say "DONE — evidence in $OUT"
