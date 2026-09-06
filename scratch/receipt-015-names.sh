#!/usr/bin/env bash
O=/Users/jordanknight/substrate/flowspace/fs3-governance/scratch/plan-013-015-prod-after.md
psql(){ docker exec flowspace3-db psql -U flowspace3 -d flowspace3 -Atc "$1"; }
NAMES=$(psql "select name from elements where parser_version='fs3-parsers@3' and kind='function' and length(name) between 9 and 30 and blob_sha in (select distinct blob_sha from worktree_files where worktree_id=263 and path ~ '\.(ts|tsx)$') group by name having count(*)=1 order by random() limit 3")
{ echo "## Named-function receipt (bash) — $(date -u +%T), load $(uptime | sed 's/.*averages: //')"
for N in $NAMES; do
  P=$(psql "select w.path from elements e join worktree_files w on w.blob_sha=e.blob_sha and w.worktree_id=263 where e.parser_version='fs3-parsers@3' and e.name='$N' limit 1")
  S=$(date +%s.%N); R=$(timeout 90 flowspace3 search "what does the $N function do" --limit 5 --json 2>/dev/null); E=$(date +%s.%N)
  H=$(echo "$R" | python3 -c "
import sys,json;d=json.load(sys.stdin);r=(d.get('data') or d).get('results',[])
print(len(r),[((x.get('path') or '')[-30:])+':'+str(x.get('kind') or '')+':'+str(x.get('name') or '')[:22] for x in r][:5])" 2>/dev/null)
  echo "search [$N] (defined in ${P##*/pij/}) wall $(echo "$E - $S" | bc) s → $H"
done; } >> "$O"; tail -4 "$O"
