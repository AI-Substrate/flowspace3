#!/usr/bin/env bash
set -o pipefail
O=/Users/jordanknight/substrate/flowspace/fs3-governance/scratch/plan-013-prod-after.md
psql(){ docker exec flowspace3-db psql -U flowspace3 -d flowspace3 -Atc "$1"; }
log(){ echo "$*" | tee -a "$O"; }
cd /Users/jordanknight/substrate/flowspace/flowspace3 || exit 1
echo "# Plan 013 prod receipt — $(date -u +%FT%TZ)" > "$O"
log "## Before (015 build, pid $(pgrep -f 'target/release/flowspace3 daemon' | head -1)): old search statement stats since the 06:02 reset"
psql "select calls, round(mean_exec_time)::int as mean_ms, left(query,40) from pg_stat_statements where query like 'WITH candidate_vectors AS MATERIALIZED%' order by calls desc limit 2" | tee -a "$O"
pgrep -f "harness checks" >/dev/null && { log "REFUSED: a harness checks gate is running (row 158)"; exit 1; }
OLD=$(pgrep -f "target/release/flowspace3 daemon" | head -1)
log "## Bounce (old pid $OLD) via bin/daemon-restart"
./bin/daemon-restart --binary /Users/jordanknight/substrate/flowspace/flowspace3/target/release/flowspace3 2>&1 | tail -3 | tee -a "$O"
RC=${PIPESTATUS[0]}; NEW=$(pgrep -f "target/release/flowspace3 daemon" | head -1)
[ "$RC" = "0" ] && [ -n "$NEW" ] && [ "$NEW" != "$OLD" ] || { log "BOUNCE NOT VERIFIED: rc=$RC old=$OLD new=$NEW — stopping"; exit 1; }
for i in $(seq 1 60); do sleep 10; timeout 8 flowspace3 ping >/dev/null 2>&1 && { log "healthy after $((i*10))s (pid $NEW)"; break; }; done
psql "select pg_stat_statements_reset()" >/dev/null && log "(pg_stat_statements reset at $(date -u +%T))"
log "## Search receipts on the 013 build (load: $(uptime | sed 's/.*averages: //'))"
for Q in "where does the daemon detect new git worktrees appearing and register them" "how is retry handled for embedding jobs" "what owns the watcher debounce" "function commonPrefixLength(a: string, b: string): number {"; do
  S=$(date +%s.%N); R=$(timeout 90 flowspace3 search "$Q" --limit 5 --json 2>/dev/null); E=$(date +%s.%N)
  H=$(echo "$R" | python3 -c "
import sys,json;d=json.load(sys.stdin);x=(d.get('data') or d);r=x.get('results',[]);m=d.get('meta') or {}
print(len(r),[((y.get('path') or '')[-28:])+':'+str(y.get('kind') or '') for y in r][:3],'scan_incomplete=',m.get('scan_incomplete'),'passes=',m.get('passes'))" 2>/dev/null)
  log "search [${Q:0:60}] wall $(echo "$E - $S" | bc) s → $H"
done
log "## New search statement stats (since reset)"; psql "select calls, round(mean_exec_time)::int as mean_ms, round(max_exec_time)::int as max_ms, left(query,40) from pg_stat_statements where query ilike '%candidate_vectors%' order by calls desc limit 3" | tee -a "$O"
log "## status wall"; for i in 1 2 3; do /usr/bin/time -p flowspace3 status --json 2>&1 >/dev/null | grep real | tee -a "$O"; done
