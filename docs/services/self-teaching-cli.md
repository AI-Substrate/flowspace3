# Self-teaching CLI
**Built**: 2026-08-26 (worker pij-broad-sawfish, PRD reqs 44/45) · **Code**: `crates/cli/src/docs.rs`, pages in `crates/cli/docs/*.md`, doctor's providers row in `crates/cli/src/doctor.rs` · **Tests**: `crates/cli/tests/docs_bundle.rs`, `crates/cli/tests/doctor_daemon.rs`

An agent that has just installed fs3 can ask fs3 how to use fs3 — offline, with
no daemon, no network and no database.

```bash
flowspace3 docs list
flowspace3 docs get agents
```

## The topics

| topic | what it answers |
|---|---|
| `agents` | the whole operating loop, the envelope contract, the gotchas (req 45) |
| `install` | build, first run, where things are written |
| `doctor` | what it checks, repairs, and refuses to repair |
| `daemon` | boot sequence, the queue, the log stream |
| `search` | flags, hit shape, how ranking works |
| `config` | files, layering, the secrets chain |
| `providers` | registering a real model from scratch, both Azure auth modes |

## Key decisions

- **The pages live in `crates/cli/docs/`, not the repository's `docs/`.**
  `include_str!` resolves relative to the source file, so the bundle travels
  with the crate. Pointing at the repo tree would compile on a developer's
  machine and break the moment the crate is vendored or published — the bundle
  has to be part of the package, not near it.

- **These are condensed, not copied.** `docs/services/*.md` remain the
  long-form pages a human reads while working ON fs3. The bundle is for an
  agent that wants an answer in its context window: the loop, the shapes, and
  the things that otherwise cost a wrong turn. A copy would be a second thing
  nobody updates; a condensation is a different artifact with a different job.

- **One string, not paginated.** `data.text` is the whole page. The consumer is
  an agent that wants the topic in context, and making it fetch pages would be
  a worse version of the file read it is avoiding.

- **`related` exists so a caller never guesses a topic name.** Every page names
  where to go next, and a test proves those links resolve.

- **An unknown topic is a 404 whose `fix` lists the real ones.** The point of a
  fixed topic set is that the valid names are knowable; making a caller guess
  twice is the failure this avoids.

## The test that makes it worth shipping

Self-teaching docs that teach the wrong thing are **worse than none**, because
the reader trusts them — and the reader here is an agent that will run what it
is told, get a usage error, and have no way to tell "I misread the docs" from
"the docs are wrong".

`tests/docs_bundle.rs` extracts every `flowspace3 <verb>` string in the bundle
and asserts each is a real subcommand, read from the binary's own `--help`
rather than from a list in the test — a list would be a third thing to keep in
sync and would happily agree with docs that are both wrong. Mutation-checked: a
verb renamed in a page fails the test with the offending string named.

It also proves `docs get` answers with a nonexistent config directory and an
unreachable database URL, because "works offline" is the whole promise.

## Doctor's providers row

A fresh install is **not** config-less: the defaults ship `[providers.fake]`
with both ports naming it, which is what makes the offline stack work. So
"no provider configured" would be false. The row reports the true and more
useful thing:

```
providers  warn  no real provider is configured — both ports use the offline
                 fake, so everything indexed is embedded and summarised by a
                 deterministic stand-in
                 → if that is deliberate, nothing to do. Otherwise run
                   `flowspace3 docs get providers` to register one
```

It also warns when an `active` names an instance that is not in the registry
(which stops the daemon from starting at all, so catching it where the fix is
printable beats meeting it as a boot failure), and when a real provider's key
variable is unset (which otherwise fails at the first call, deep inside a job,
hours into an index).

`warn` degrades the verdict but never fails the command: a fake-only stack
works, and doctor does not know whether that was chosen. Choosing a model and
supplying credentials is a decision, and a diagnostic command must not make it
for you — so this row is reported, never repaired.

## The loop this closes

Jordan hit two failures a week apart that turn out to be one:

1. `doctor` said a plain `ok` on a machine with no daemon running.
2. A search returned zero hits, from a full index, with no hint — because the
   active embedder was the fake while the vectors had been written by Azure.

Both are the same silence: a component reporting on itself without reporting on
the thing it depends on. The daemon row fixed the first. The providers row and
the search empty-result hint fix the second — search now names `flowspace3
doctor` when nothing matches, and doctor names the active providers.

## Verify

```bash
cargo test -p fs3-cli --test docs_bundle      # 4: drift, offline, error shape
cargo test -p fs3-cli --test doctor_daemon    # 5: daemon + providers rows
flowspace3 docs list
FS3_CONFIG_DIR=/nonexistent flowspace3 docs get agents   # works with nothing configured
```

## Adding a topic

1. Write `crates/cli/docs/<name>.md`.
2. Add a `Topic` entry to `TOPICS` in `crates/cli/src/docs.rs` — name, title,
   `include_str!`, and `related`.
3. Run `cargo test -p fs3-cli`. The tests will tell you if a `related` link
   does not resolve, if the page is a stub, or if it mentions a command that
   does not exist.

There is no step 4: nothing else references the topic list.
