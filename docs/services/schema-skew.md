# Schema skew — when the binary and the database disagree

**Owner**: pij-strange-edeard · **Requirements**: PRD req 61 (skew detection and
steering), req 59 (the user messages queue this produces onto).

Two directions of disagreement, and only one of them is the one people expect.

| Direction | Meaning | Fixable by migrating? | Who says so |
|---|---|---|---|
| Database **behind** the binary | the ordinary case — a new release added migrations | **Yes**, and boot/`doctor` just do it | `schema::guard`, `doctor`'s `schema` row |
| Database **ahead** of the binary | somebody ran a newer flowspace3 against this store | **No** — there is nothing to apply | this page |

## The incident

Jordan ran a stale binary against a database a newer one had migrated. Boot died
with this, twice over:

```text
applying store migrations to … — if the store is not running: docker compose up -d:
migrations failed: migration 8 was previously applied but is missing in the
resolved migrations: migration 8 was previously applied but is missing in the
resolved migrations
```

Four defects in one line:

1. **It is sqlx's sentence, not fs3's.** "Resolved migrations" requires knowing
   sqlx's vocabulary to parse.
2. **It steers at `docker compose up -d`.** The store was perfectly healthy.
   That advice comes from a `.with_context` written for the *unreachable-store*
   case, which this is not.
3. **It never says auto-migration already ran.** Jordan's actual question was
   "why not just auto migrate" — an error that does not pre-empt that question
   gets argued with instead of followed.
4. **It printed twice**, because `StoreError::Migrate` interpolated a field that
   `#[from]` had also made a `source()`, and `flowspace3` prints the whole
   anyhow chain with `{error:#}`.

## What happens now

```text
this flowspace3 binary is OLDER than its database: the binary is 0.2.0 and
carries migrations up to 0009, but the database has already applied 0099, which
this binary has never heard of

migrating cannot fix this — auto-migration already ran, and there is nothing to
apply: the database is ahead, not behind. The store is healthy; do NOT restart
it. Upgrade the binary instead: `flowspace3 doctor upgrade`, or reinstall: `curl
-fsSL https://…/install.sh | sh`
```

The words live once, in `fs3_core::skew::SchemaSkew`, and boot, `doctor` and the
message producer all read from it — so the three cannot phrase the same finding
three different ways.

## The three places it surfaces

### Boot refuses, with the reason

`boot::serve` asks which direction the disagreement runs **before** it migrates.
Ahead → refuse with the explanation above. Behind → migrate, as always.

The refuse-versus-serve behaviour is unchanged: sqlx already refused, and an old
binary against a newer schema may be wrong in ways this code cannot audit. Only
the message changed.

### `doctor` gets its own named row

Previously this case reported **green**, which is its own small scandal:
`SchemaStatus::is_current()` is `missing.is_empty()`, so a database carrying
migrations the binary has never heard of satisfies it perfectly. Doctor said "9
migrations applied, ok" on the exact machine that could not start a daemon.

The skew check now runs *before* `is_current`, and the row carries the summary,
the fix, and a steer.

### The queue gets a second producer

```
SchemaSupervisor (reconcile, source "schema")
  └── each pass: schema_current → skew → sync_messages
```

**Not** the boot case. A boot failure cannot be a queue producer: the process
exits, so nothing is left to RETRACT the message when the situation resolves —
and the binary that hits it is by definition old enough that it may not know the
queue exists.

What this watches is the sibling nobody was watching: a daemon that booted
cleanly and *then* had its database migrated out from under it, by a newer
`doctor` or a colleague's daemon. That process keeps serving against a schema it
does not fully understand. The fact was computable all along
(`schema::ahead_of_us`) and surfaced only as a field in `flowspace3 status` you
had to know to look for.

Severity is `error`, not `warning`: unlike a pending update, this daemon is
writing *right now*.

## Why this was the right second producer

It was chosen as the seam test for the queue, and it earns that by having the
**opposite lifecycle** to the first one:

| | `update` | `schema` |
|---|---|---|
| condition arrives | on a timer, once a day | at any instant, from another process |
| condition clears | when this daemon restarts into the new binary | when the skew goes away, possibly without restarting |
| steady state | nothing to say | nothing to say |

Both drop out of the same `sync_messages(source, desired)` contract with no
clear-condition machinery, and
`one_producer_declaring_does_not_retract_another_producers_message` proves the
per-source ownership actually holds.

## Related: tests may not choose their own database

The same incident exposed how migrations 0008/0009 reached a production database
in the first place — `harness checks` on a developer machine. See
`fs3_testkit::database` and the `testdb` gate; the short version is that
`flowspace3 doctor` REPAIRS, so it applies migrations, and the test helpers used
to fall back to the shipped default address when `FS3_TEST_DATABASE_URL` was
unset. There is no fallback any more.

## Where the code is

| Concern | File |
|---|---|
| The words and the decision | `crates/core/src/skew.rs` |
| Measuring it | `crates/store/src/admin.rs` (`SchemaStatus::skew`) |
| Boot refusal | `crates/daemon/src/boot.rs` |
| The queue producer | `crates/daemon/src/skew.rs` |
| Doctor row | `crates/cli/src/doctor.rs` (`check_schema`) |
| Proof | `crates/daemon/tests/schema_skew.rs`, `crates/core/src/skew.rs` tests |

## Open, and named rather than hidden

- **A daemon that is ahead of its database at RUNTIME is still only a status
  field.** `schema::guard` rejects per-request when the database is behind; the
  reverse now produces a queue message but does not refuse. Refusing mid-flight
  is a bigger decision than this packet should make.
- **The doctor row is proven by a live transcript, not a test.** Building a
  throwaway database inside the `fs3-cli` suite would mean a second copy of the
  daemon suite's `FreshDatabase`, which is the drift the split refuses; moving
  that helper into `fs3-testkit` is the right fix and is its own packet.
