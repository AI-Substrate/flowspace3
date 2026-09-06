#!/usr/bin/env bash
O=/Users/jordanknight/substrate/flowspace/fs3-governance/scratch/plan-013-015-prod-after.md
psql(){ docker exec flowspace3-db psql -U flowspace3 -d flowspace3 -Atc "$1"; }
cd /Users/jordanknight/pi-hacking/pij || exit 1
ROW=$(psql "select e.address||'|'||e.raw_hash||'|'||replace(left(e.raw_text,60),E'\n',' ') from elements e where e.parser_version='fs3-parsers@3' and e.kind='function' and e.name='commonPrefixLength' limit 1")
ADDR=${ROW%%|*}; REST=${ROW#*|}; RH=${REST%%|*}; TXT=${REST#*|}
{ echo "## Probe 2 — $(date -u +%T): TS element $ADDR"
echo "embedding rows for its raw_hash: $(psql "select count(*)||' model='||coalesce(string_agg(distinct model_key,','),'?')||' kinds='||coalesce(string_agg(distinct source_kind,','),'?') from embeddings_1024 where source_hash='$RH'")"
echo "a Rust function's embedding row for comparison: $(psql "select 'model='||model_key||' kind='||source_kind from embeddings_1024 x join elements e on e.raw_hash=x.source_hash where e.kind='function' and e.parser_version='fs3-parsers@3' and e.address like '%.rs::%' limit 1")"
echo "get by address: $(timeout 30 flowspace3 get "$ADDR" --json 2>&1 | python3 -c "import sys,json;d=json.load(sys.stdin);x=d.get('data') or d;print(str(x.get('name') or x.get('address') or x)[:80])" 2>/dev/null)"
S=$(date +%s.%N); R=$(timeout 90 flowspace3 search "$TXT" --limit 5 --json 2>/dev/null); E=$(date +%s.%N)
echo "search by the function's own first line [$TXT] wall $(echo "$E - $S" | bc) s → $(echo "$R" | python3 -c "
import sys,json;d=json.load(sys.stdin);r=(d.get('data') or d).get('results',[])
print(len(r),[((y.get('path') or '')[-30:])+':'+str(y.get('kind') or '')+':'+str(y.get('name') or '')[:22] for y in r][:5])" 2>/dev/null)"
} >> "$O"; tail -5 "$O"
