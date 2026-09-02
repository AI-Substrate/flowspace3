#!/usr/bin/env bash
# Plan 016 prod receipt (row 125 / row 147 second half): run AFTER #107 merges and the release build.
set -o pipefail
O=/Users/jordanknight/substrate/flowspace/fs3-governance/scratch/plan-016-prod-after.md
psql(){ docker exec flowspace3-db psql -U flowspace3 -d flowspace3 -Atc "$1"; }
log(){ echo "$*" | tee -a "$O"; }
cd /Users/jordanknight/substrate/flowspace/flowspace3 || exit 1
echo "# Plan 016 prod receipt — $(date -u +%FT%TZ)" > "$O"
log "## Before: pij worktree rows under .pi/: $(psql "select count(*) from worktree_files where path like '.pi/%' and worktree_id=(select max(id) from worktrees where root_path='/Users/jordanknight/pi-hacking/pij')")"
pgrep -f "harness checks" >/dev/null && { log "REFUSED: a harness checks gate is running (row 158)"; exit 1; }
OLD=$(pgrep -f "target/release/flowspace3 daemon" | head -1); log "## Manual bounce (old pid $OLD) — Ctrl-C then relaunch in pane %50"
tmux send-keys -t %50 C-c; for i in $(seq 1 40); do sleep 1; kill -0 $OLD 2>/dev/null || break; done
kill -0 $OLD 2>/dev/null && { log "old daemon still alive — stop"; exit 1; }
sleep 2; tmux send-keys -t %50 "/Users/jordanknight/substrate/flowspace/flowspace3/target/release/flowspace3 daemon" Enter; sleep 5
NEW=$(pgrep -f "target/release/flowspace3 daemon" | head -1); [ -n "$NEW" ] && [ "$NEW" != "$OLD" ] || { log "relaunch failed"; exit 1; }
for i in $(seq 1 60); do sleep 10; timeout 8 flowspace3 ping >/dev/null 2>&1 && { log "healthy+authorized after $((i*10))s (pid $NEW); migration max $(psql 'select max(version) from _sqlx_migrations')"; break; }; done
timeout 8 flowspace3 ping >/dev/null 2>&1 || { log "NOT HEALTHY — stop"; exit 1; }
log "## Opt-in re-add of ~/pi-hacking/pij"
flowspace3 add /Users/jordanknight/pi-hacking/pij --include-hidden --json 2>&1 | python3 -c "import sys,json;d=json.load(sys.stdin);x=(d.get('data') or d);print('include_hidden',x.get('include_hidden'),'files',x.get('files'),'enqueued',x.get('enqueued'),'unchanged',x.get('unchanged'),'worktree',x.get('worktree_id'))" 2>/dev/null | tee -a "$O"
WT=$(psql "select max(id) from worktrees where root_path='/Users/jordanknight/pi-hacking/pij'")
for i in $(seq 1 60); do N=$(psql "select count(*) from elements e where e.kind<>'file' and e.parser_version='fs3-parsers@3' and e.blob_sha in (select blob_sha from worktree_files where worktree_id=$WT and path like '.pi/%' and path ~ '\.(ts|tsx)$')"); [ "${N:-0}" -gt 0 ] && { log ".pi/ TS non-file elements: $N after $((i*20))s"; break; }; sleep 20; done
sleep 120; log "### .pi/ TS blobs parsed @3: $(psql "select count(distinct blob_sha) from elements where parser_version='fs3-parsers@3' and blob_sha in (select blob_sha from worktree_files where worktree_id=$WT and path like '.pi/%' and path ~ '\.(ts|tsx)$')") of $(psql "select count(distinct blob_sha) from worktree_files where worktree_id=$WT and path like '.pi/%' and path ~ '\.(ts|tsx)$'") (tracked .ts under .pi/: $(git -C /Users/jordanknight/pi-hacking/pij ls-files '.pi/**/*.ts' | wc -l | tr -d ' '))"
log "### status shows include_hidden: $(timeout 20 flowspace3 status --json 2>/dev/null | python3 -c "import sys,json;d=json.load(sys.stdin);x=(d.get('data') or d);r=[w for w in (x.get('roots') or x.get('worktrees') or []) if 'pi-hacking/pij' in json.dumps(w)];print([ (w.get('root') or w.get('root_path') or w.get('path'), w.get('include_hidden')) for w in r][:2])" 2>/dev/null)"
cd /Users/jordanknight/pi-hacking/pij || exit 1
for Q in "export function daemonLocation(" "where does the pij extension register the seat at boot"; do S=$(date +%s.%N); R=$(timeout 90 flowspace3 search "$Q" --limit 5 --json 2>/dev/null); E=$(date +%s.%N); log "search [$Q] wall $(echo "$E - $S" | bc) s → $(echo "$R" | python3 -c "
import sys,json;d=json.load(sys.stdin);r=(d.get('data') or d).get('results',[])
print(len(r),[((y.get('path') or '')[-40:])+':'+str(y.get('kind') or '')+':'+str(y.get('name') or '')[:20] for y in r][:5])" 2>/dev/null)"; done
