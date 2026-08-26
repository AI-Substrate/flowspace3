# boot — the first proof

Run `harness boot` before you change anything. If the environment is not working
before the work starts, it will certainly not be working after.

## What it computes deterministically

Four stages, stopping at the first hard failure:

1. **toolchain** — `cargo --version` resolves (error `E_NO_TOOLCHAIN` if not).
2. **crate** — is there a `Cargo.toml` yet? (Absent is reported, not failed.)
3. **build** — `cargo build --all-targets`, when a crate exists.
4. **checks** — composes `harness checks --json` and folds its verdict in.

Verdicts: `ok` → ready · `degraded` → the environment is up but the gate cannot
prove anything (no crate yet, or no `checks` extension) · `error` → the toolchain,
the build, or a quality gate is red. It also prints orientation: what this repo is
and which harness verbs to reach for next.

## What is expected back from you

- Treat the verdict as the starting fact of your session, not a formality. A
  `degraded` boot tells you what proof is missing — decide whether the work you
  are about to do needs that proof to exist first.
- Boot grows by use. When you find yourself doing a readiness step by hand
  (seeding data, starting a service, waiting on a port), that step belongs here.
  Capture it with `harness observe` at the moment it bites.
