#!/usr/bin/env bash
O=/Users/jordanknight/substrate/flowspace/fs3-governance/scratch/plan-013-015-prod-after.md
{ echo "## Scoping test: same queries from cwd=~/pi-hacking/pij — $(date -u +%T), load $(uptime | sed 's/.*averages: //')"
cd /Users/jordanknight/pi-hacking/pij || exit 1
for N in expectQuarantineRefusal commonPrefixLength; do
  S=$(date +%s.%N); R=$(timeout 90 flowspace3 search "what does the $N function do" --limit 5 --json 2>/dev/null); E=$(date +%s.%N)
  H=$(echo "$R" | python3 -c "
import sys,json;d=json.load(sys.stdin);x=(d.get('data') or d);r=x.get('results',[])
print(len(r),[((y.get('path') or '')[-30:])+':'+str(y.get('kind') or '')+':'+str(y.get('name') or '')[:22] for y in r][:5], 'scope:', json.dumps(x.get('scope') or (d.get('meta') or {}).get('scope'))[:160])" 2>/dev/null)
  echo "search [$N] from pij cwd wall $(echo "$E - $S" | bc) s → $H"
done
echo "meta keys of one envelope: $(timeout 90 flowspace3 search "commonPrefixLength" --limit 2 --json 2>/dev/null | python3 -c "import sys,json;d=json.load(sys.stdin);print(list((d.get('meta') or {}).keys())[:20], list((d.get('data') or {}).keys())[:12])" 2>/dev/null)"
} >> "$O"; tail -4 "$O"
