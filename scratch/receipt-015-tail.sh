#!/usr/bin/env bash
OUT=/Users/jordanknight/substrate/flowspace/fs3-governance/scratch/plan-013-015-prod-after.md
psql(){ docker exec flowspace3-db psql -U flowspace3 -d flowspace3 -Atc "$1"; }
TS="select distinct blob_sha from worktree_files where worktree_id=263 and path ~ '\.(ts|tsx|mts|cts)$'"
log(){ echo "$*" | tee -a "$OUT"; }
log ""; log "## CLEAN RECEIPT (bash) — $(date -u +%FT%TZ)"
for i in $(seq 1 24); do Q=$(timeout 20 flowspace3 status --json 2>/dev/null | python3 -c "import sys,json;d=json.load(sys.stdin);q=(d.get('data') or d).get('queue') or [];print(sum(int(x.get('count',0) or 0) for x in q) if isinstance(q,list) else q)" 2>/dev/null); log "queue open jobs: ${Q:-?} at +$((i*30))s"; [ "${Q:-1}" = "0" ] && break; sleep 30; done
log "### TS elements @3 by kind (pij worktree 263)"; psql "select kind, count(*) from elements where parser_version='fs3-parsers@3' and blob_sha in ($TS) group by 1 order by 2 desc" | tee -a "$OUT"
log "### TS blobs parsed under @3: $(psql "select count(distinct blob_sha) from elements where parser_version='fs3-parsers@3' and blob_sha in ($TS)") of $(psql "select count(*) from ($TS) t")"
log "### Search receipts (load: $(uptime | sed 's/.*load/load/'))"
for Q in "where does the pij extension register the seat at boot" "how does the pij extension send a message to the daemon" "where does the daemon detect new git worktrees appearing and register them" "what owns the watcher debounce"; do S=$(date +%s.%N); R=$(timeout 90 flowspace3 search "$Q" --limit 5 --json 2>/dev/null); E=$(date +%s.%N); H=$(echo "$R" | python3 -c "
import sys,json;d=json.load(sys.stdin);r=(d.get('data') or d).get('results',[])
print(len(r), [ ((x.get('path') or x.get('file') or '')[-34:]) + ':' + str(x.get('kind') or x.get('element_kind') or '') + ':' + str(x.get('name') or '')[:20] for x in r][:5])" 2>/dev/null); log "search [$Q] wall $(echo "$E - $S" | bc) s → $H"; done
log "### pg_stat_statements (search statement since the 06:02 reset)"; psql "select calls, round(mean_exec_time)::int as mean_ms from pg_stat_statements where query like 'WITH candidate_vectors AS MATERIALIZED%' order by calls desc limit 2" | tee -a "$OUT"
log "### status wall"; for i in 1 2 3; do /usr/bin/time -p flowspace3 status --json 2>&1 >/dev/null | grep real | tee -a "$OUT"; done
