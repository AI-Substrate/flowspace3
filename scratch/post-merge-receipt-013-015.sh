#!/usr/bin/env bash
# o-prime post-merge receipt for plans 013 (search admission) + 015 (TypeScript grammar).
# Run from the main clone AFTER both PRs merge and `cargo build --release -p fs3-cli` succeeds.
# Read-only against prod except: the daemon bounce and `flowspace3 remove/add` of ONE repo.
set -o pipefail
OUT=/Users/jordanknight/substrate/flowspace/fs3-governance/scratch/plan-013-015-prod-after.md
PSQL="docker exec flowspace3-db psql -U flowspace3 -d flowspace3 -Atc"
TS="select distinct blob_sha from worktree_files where path ~ '\.(ts|tsx|mts|cts)$'"
log(){ echo "$*" | tee -a "$OUT"; }
echo "# Plan 013+015 prod receipt — $(date -u +%FT%TZ)" > "$OUT"
log "## Before bounce (pg_stat_statements for the OLD search statement)"
$PSQL "select calls, round(mean_exec_time)::int as mean_ms from pg_stat_statements where query like 'WITH candidate_vectors AS MATERIALIZED%' order by calls desc limit 2" | tee -a "$OUT"
$PSQL "select pg_stat_statements_reset()" >/dev/null 2>&1 && log "(pg_stat_statements reset)"
log "## Bounce"; pgrep -f "harness checks" >/dev/null && { log "REFUSED: a harness checks gate is running (row 158)"; exit 1; }
./bin/daemon-restart --binary /Users/jordanknight/substrate/flowspace/flowspace3/target/release/flowspace3 2>&1 | tail -2 | tee -a "$OUT"
for i in $(seq 1 60); do sleep 10; timeout 8 flowspace3 ping >/dev/null 2>&1 && { log "healthy after $((i*10))s"; break; }; done
log "## Row 147: re-ingest ~/pi-hacking/pij, then wait for TS non-file elements"
flowspace3 remove /Users/jordanknight/pi-hacking/pij --json 2>&1 | tail -c 300 | tee -a "$OUT"; echo >> "$OUT"
flowspace3 add /Users/jordanknight/pi-hacking/pij --json 2>&1 | tail -c 300 | tee -a "$OUT"; echo >> "$OUT"
for i in $(seq 1 90); do N=$($PSQL "select count(*) from elements where kind <> 'file' and parser_version = 'fs3-parsers@3' and blob_sha in ($TS)"); [ "${N:-0}" -gt 0 ] && { log "TS non-file elements @3: $N after $((i*20))s"; break; }; sleep 20; done
sleep 60; $PSQL "select kind, count(*) from elements where parser_version='fs3-parsers@3' and blob_sha in ($TS) group by 1 order by 2 desc" | tee -a "$OUT"
log "## Search receipts (load: $(uptime | sed 's/.*load/load/'))"
for Q in "where does the pij extension register the seat at boot" "where does the daemon detect new git worktrees appearing and register them" "how is retry handled for embedding jobs" "what owns the watcher debounce"; do S=$(date +%s.%N); R=$(timeout 90 flowspace3 search "$Q" --limit 3 --json 2>/dev/null); E=$(date +%s.%N); H=$(echo "$R" | python3 -c "import sys,json;d=json.load(sys.stdin);r=(d.get('data') or d).get('results',[]);print(len(r), [ (x.get('path') or '')[-40:] + ':' + str(x.get('kind') or x.get('element_kind') or '') for x in r][:3])" 2>/dev/null); log "search [$Q] wall $(echo "$E - $S" | bc) s → $H"; done
log "## After: pg_stat_statements for the NEW statement"
$PSQL "select calls, round(mean_exec_time)::int as mean_ms, left(query,60) from pg_stat_statements where query ilike '%candidate_vectors%' order by calls desc limit 3" | tee -a "$OUT"
log "## status timings"; for i in 1 2 3; do /usr/bin/time -p flowspace3 status --json 2>&1 >/dev/null | grep real | tee -a "$OUT"; done
