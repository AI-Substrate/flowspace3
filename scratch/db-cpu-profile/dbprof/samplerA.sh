#!/bin/bash
BASE="/Users/jordanknight/substrate/flowspace/flowspace3/.harness/temp/agent/dbprof"
OUT="$BASE/timeseries.csv"
echo "iso,docker_cpu_pct,mem_usage,mem_pct,block_io,net_io,pids,host_load1,host_load5,pg_active,pg_idletx,pg_conns,pg_autovac,wait_hist,top3,ndb,dbsize" > "$OUT"
for i in $(seq 1 120); do
  T=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  DS=$(docker stats --no-stream --format '{{.CPUPerc}}|{{.MemUsage}}|{{.MemPerc}}|{{.BlockIO}}|{{.NetIO}}|{{.PIDs}}' flowspace3-db 2>/dev/null | tr -d '%')
  LOAD=$(uptime | sed 's/.*averages*: //' | tr -d ',' | awk '{print $1"|"$2}')
  PG=$(docker exec -i flowspace3-db psql -U flowspace3 -d flowspace3 -f - < "$BASE/ts.sql" 2>/dev/null | grep '^A|' | head -1 | sed 's/^A|//')
  echo "$T|$DS|$LOAD|$PG" | sed 's/|/,/g' >> "$OUT"
  sleep 5
done
echo DONE_A >> "$OUT"
