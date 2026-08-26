# Contributing to flowspace3

Thanks for your interest! This is a young project moving fast — the guide below
is short on ceremony and long on the few rules that actually gate a merge.

## Ways to contribute

Bug reports, feature ideas, docs fixes, and code are all welcome. For anything
substantial, open an issue first so the design conversation happens before the
work.

## Reporting bugs

Open a GitHub issue with:

- what you ran (the exact command) and what came back — every `flowspace3`
  command answers a JSON envelope, so paste it; the `error.code` and `fix`
  fields do most of the diagnosis for you,
- the output of `flowspace3 doctor` (it checks and repairs the whole stack and
  reports what it found),
- your platform and how you installed (script, release download, source).

Never include secrets, tokens, or private repository content.

## Development setup

```bash
git clone https://github.com/AI-Substrate/flowspace3.git
cd flowspace3
cargo build --release          # Rust stable, pinned by rust-toolchain.toml
./target/release/flowspace3 doctor   # starts the Postgres stack (Docker), creates the db, migrates
```

`doctor` is the setup command — there is no second one. The daemon and CLI ship
as one binary (`flowspace3`; the daemon is `flowspace3 daemon`). The default
providers are offline fakes, so tests and local runs need no API keys.

## Checks that gate a merge

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

CI runs these on Linux against a real pgvector Postgres. Tests that need real
provider credentials or are slow are `#[ignore]`-tagged and never run in CI —
don't add a test that needs a secret to pass.

## Commit conventions

**Conventional Commits are binding** (`feat:`, `fix:`, `docs:`, `chore:`, …) —
release-please reads them to compute versions and changelogs, so a mislabeled
commit changes what ships. Keep commits focused; imperative subject lines.

## Pull requests

- Explain the problem and the change; link the issue.
- Behavioral changes come with tests — and a fix's test should fail without
  the fix.
- Update the relevant page under `docs/services/` if you changed how a
  subsystem works.
- CI must be green.

Architecture has hard edges here: exactly two provider ports (`Embedder`,
`Summarizer`), eight workspace crates, dependency direction enforced by a
mechanical check (`cargo run -p fs3-testkit --bin fs3-arch-check`). Read
`docs/rules-idioms-architecture/fs3-architecture.md` before moving code across
crate boundaries.

## Contribution licensing

flowspace3 is [MIT-licensed](LICENSE), inbound = outbound: by submitting a
contribution you agree it is provided under the MIT License and certify you
have the right to submit it. No CLA, no sign-off requirement.

## Questions

Open a GitHub issue — or ask the binary: `flowspace3 docs list` ships the
agent/user guides offline.
