#!/usr/bin/env bash
# ac-0001 probe — max concurrent database DDL observed at the server.
#
# This is the measurement behind the BINDING f-1a01 delta target
# (max concurrent DDL from `cargo test -p fs3-store` must go 16 -> 1).
# Use THIS script, not a reimplementation: the sampling rate and the
# predicate both change the number, so a differently-shaped probe is
# not comparable to the 16 on record.
#
# Usage:
#   export FS3_TEST_DATABASE_URL='postgres://flowspace3:flowspace3@127.0.0.1:5433/<your-scratch-db>'
#   ./ac-0001-ddl-probe.sh store    -p fs3-store
#   ./ac-0001-ddl-probe.sh oversize -p fs3-daemon --test oversize
#
# Measured at 5c7f7bdb (pre-fix), default parallelism, container flowspace3-db:
#   store    : 634 samples, max_concurrent_ddl=16, samples_over_1=177, exit 0
#   oversize : 437 samples, max_concurrent_ddl=1,  samples_over_1=0,   exit 0
#
# Caveats that belong with the number:
#   - Sampling is ~5/sec through `docker exec`, so the max is a FLOOR, not a ceiling.
#   - A DROP DATABASE waiting on its forced checkpoint counts as in-flight.
#     That is deliberate: it is exactly the concurrency the postmaster sees.
#   - Requires a scratch database on :5433. NEVER point this at :7373.

set -u
CONTAINER=${CONTAINER:-flowspace3-db}
LABEL=$1; shift
OUT=$(mktemp "/tmp/ddl-${LABEL}-XXXXXX.txt")

sampler() {
  while true; do
    docker exec "$CONTAINER" psql -U flowspace3 -d postgres -tAc \
      "select count(*) from pg_stat_activity
        where state='active'
          and (query ilike 'create database%' or query ilike 'drop database%')" \
      2>/dev/null >> "$OUT"
  done
}

sampler & SP=$!
sleep 1
cargo test "$@" >/dev/null 2>&1; EX=$?
kill $SP 2>/dev/null; wait $SP 2>/dev/null

SAMPLES=$(wc -l < "$OUT")
MAX=$(sort -n "$OUT" | tail -1)
WITH=$(awk '$1>=1' "$OUT" | wc -l)
OVER=$(awk '$1>=2' "$OUT" | wc -l)

echo "$LABEL: exit=$EX samples=$SAMPLES max_concurrent_ddl=$MAX samples_with_ddl=$WITH samples_over_1=$OVER"
echo "raw: $OUT"

# PASS for the delta review: max_concurrent_ddl == 1 and samples_over_1 == 0,
# with a non-trivial samples_with_ddl (a run that never touched DDL proves nothing).
