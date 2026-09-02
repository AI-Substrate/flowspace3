#!/usr/bin/env bash
# ac-0001 probe — max concurrent database DDL issued BY THIS TEST PROCESS.
#
# WHY ATTRIBUTION MATTERS (learned the hard way, 2026-09-02): an earlier version
# of this script counted every create/drop database on the postmaster. On a box
# where five worktrees share one container that measures the SERVER's total DDL
# concurrency, not the promise. ac-0001 is per-process ("no test binary issues
# more than one at a time"), so the probe must be per-process too. The unattributed
# version reported max=2 for the GUARDED oversize suite -- my 1 plus a neighbour's 1.
#
# Attribution works because sqlx parses `application_name` out of the URL query
# string, and fs3_store::maintenance_url is documented to PRESERVE the query
# string (crates/store/src/admin.rs:225) -- so the tag rides onto the very
# maintenance connection that issues CREATE/DROP DATABASE.
#
# Usage (the ?application_name= is REQUIRED and must match APPNAME below):
#   export FS3_TEST_DATABASE_URL='postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test?application_name=rs-review-012'
#   export CONTAINER=flowspace3-db-test
#   ./bin/ac-0001-ddl-probe.sh store    -p fs3-store
#   ./bin/ac-0001-ddl-probe.sh oversize -p fs3-daemon --test oversize
#
# PASS for the delta review: max_concurrent_ddl == 1 AND samples_over_1 == 0,
# with a non-trivial samples_with_ddl -- a run that never caught DDL active
# proves nothing. Report samples_with_ddl alongside the max, always.
#
# Caveats that belong with the number:
#   - Sampling is ~5/sec through `docker exec`, so the max is a FLOOR, not a ceiling.
#   - A DROP DATABASE waiting on its forced checkpoint counts as in-flight.
#     That is deliberate: it is exactly the concurrency this process imposes.
#   - Tests belong on the DEDICATED test postmaster :5434 / flowspace3-db-test
#     (binding from 2026-09-02). NEVER :5433 (shared, also serves prod-ish work)
#     and NEVER :7373.
#   - The test postmaster runs a different config (parallel workers off), so
#     checkpoint counts are NOT comparable across containers. max_concurrent_ddl
#     IS comparable: it is a structural property of the code, not of the server.
#   - `foreign_ddl` is reported for context: other seats' DDL during your window.
#     It is NOT part of the verdict, but a large value means the box was busy.
#
# BLAST RADIUS -- READ THIS BEFORE CHOOSING A TARGET.
# `cargo test -p fs3-store` mints and drops ~107 databases at default
# parallelism through the UNGUARDED helper in crates/store/tests/support/mod.rs.
# The burst itself is not the hazard; the burst against a SHARED postmaster is.
# A seat ran exactly this from an earlier copy of this script on 2026-09-02 and
# the shared container went into crash recovery. So it is REFUSED unless
# CONTAINER names a dedicated test postmaster (default flowspace3-db-test), or
# you pass FS3_PROBE_I_KNOW=1 with a reason.
# If you only need to establish "more than one in flight", a small target such
# as `--test pg_lexical` (2 databases) or `--test pg_round_trip` (8) proves the
# same thing at a fraction of the load.

set -u
CONTAINER=${CONTAINER:-flowspace3-db-test}
DEDICATED=${DEDICATED:-flowspace3-db-test}
APPNAME=${APPNAME:-rs-review-012}
LABEL=$1; shift

# --check runs every guard and exits WITHOUT starting cargo, docker or a sampler.
# Guard code that can only be exercised by triggering the guarded action stays
# untested: proving this script's refusals once cost a 3-second cargo run during
# a declared read-only window. Use `--check` instead.
CHECK=0
case " $* " in *" --check "*) CHECK=1; set -- $(printf '%s\n' "$@" | grep -vx -- --check) ;; esac

case "${FS3_TEST_DATABASE_URL:-}" in
  *application_name=*) ;;
  *) echo "REFUSING: FS3_TEST_DATABASE_URL must carry ?application_name=$APPNAME so the probe can attribute." >&2; exit 2 ;;
esac

# Blast-radius guard. The 107-database burst is fine on a dedicated test
# postmaster and dangerous on a shared one, so gate on the CONTAINER, not the
# target alone.
case " $* " in
  *" -p fs3-store "*)
    case " $* " in
      *" --test "*) ;;
      *) [ "$CONTAINER" = "$DEDICATED" ] || [ "${FS3_PROBE_I_KNOW:-}" = "1" ] || {
           echo "REFUSING: '-p fs3-store' with no --test filter mints ~107 databases through the" >&2
           echo "  UNGUARDED helper, and CONTAINER=$CONTAINER is not the dedicated test postmaster" >&2
           echo "  ($DEDICATED). That combination put the shared container into crash recovery." >&2
           echo "  Point at the test postmaster:  CONTAINER=$DEDICATED $0 $LABEL $*" >&2
           echo "  Or use a small target:         $0 $LABEL -p fs3-store --test pg_lexical" >&2
           exit 3
         } ;;
    esac ;;
esac

if [ "$CHECK" = 1 ]; then
  echo "check: guards passed — label=$LABEL container=$CONTAINER appname=$APPNAME args='$*'"
  echo "check: nothing was run; drop --check to take the measurement."
  exit 0
fi

# X's MUST be terminal: BSD mktemp does not substitute a template that has a
# suffix, so "-XXXXXX.txt" yields a literal filename and two concurrent probes
# would silently share one file.
OUT=$(mktemp "/tmp/ddl-${LABEL}-XXXXXX")

sampler() {
  while true; do
    docker exec "$CONTAINER" psql -U flowspace3 -d postgres -tAF' ' -c \
      "select
         count(*) filter (where application_name = '$APPNAME'),
         count(*) filter (where application_name <> '$APPNAME')
       from pg_stat_activity
       where state='active'
         and (query ilike 'create database%' or query ilike 'drop database%')" \
      2>/dev/null >> "$OUT"
  done
}

sampler & SP=$!
sleep 1
cargo test "$@" >/dev/null 2>&1; EX=$?
kill $SP 2>/dev/null; wait $SP 2>/dev/null

SAMPLES=$(wc -l < "$OUT" | tr -d ' ')
MAX=$(awk '{print $1}' "$OUT" | sort -n | tail -1)
WITH=$(awk '$1>=1' "$OUT" | wc -l | tr -d ' ')
OVER=$(awk '$1>=2' "$OUT" | wc -l | tr -d ' ')
FOREIGN=$(awk '{print $2}' "$OUT" | sort -n | tail -1)

echo "$LABEL: exit=$EX samples=$SAMPLES max_concurrent_ddl=$MAX samples_with_ddl=$WITH samples_over_1=$OVER foreign_ddl_max=$FOREIGN"
echo "raw: $OUT"
