# Validation — plan.dd.json (docker-daemon-base)
**Validator**: /validate-v2 (adaptive: lead + 1 critic) · **Date**: 2026-08-26 · **Revision**: post-fix

**Verdict**: ✅ VALIDATED WITH FIXES — 2 high, 3 medium found; all 5 verified against landed s001 code and fixed in-document; revalidate 0 errors, `harness plan ready` → ready on final bytes (basis 1d98c4894d40…).

## Contract (condensed)
Purpose: docker substrate — build+run container pair, zero-image-rebuild reload, compose stack (s001's pgvector db + new daemon service), throwaway /health daemon, engine-agnostic (OrbStack/podman), docker harness extension of paved verbs. Promise: pij-impressive-ox executes phase 1 (POC in plan assets) then phase 2 (gated on s001 exit) without clarification. Proof target: Implementation. Upstream: PRD reqs 4/15-20/33 · workshop 001 · root docker-compose.yml (s001). Consumers: ox, later CI, later real-daemon plan.

## Findings → fixes applied
| Sev | Finding (all CONFIRMED against code) | Fix |
|---|---|---|
| HIGH | In-container `cargo test --workspace` fails as specified: `store/tests/pg_round_trip.rs` panics without Postgres and the default URL (`127.0.0.1:5433`, core/src/config.rs:106) is the container itself in-network | tk-0204 + dw-0204 now require joining the compose network + `FS3_TEST_DATABASE_URL=…@db:5432/…` |
| HIGH | Compose project-name prefixing silently splits named volumes between `engine run -v` and compose services → daemon starts with empty volume; `down -v` would also delete warm caches | tk-0101/tk-0103 require explicit `name:`/`external: true`; paved down never deletes cache volumes; new execution guardrail |
| MED | Containerized fs3-daemon can't reach db with shipped defaults (file-based config via `FS3_CONFIG_DIR`; `connect_lazy` makes /health green with a broken DB path) | tk-0201 mounts container-scoped config (`database.url=…@db:5432`); dw-0201 adds a DB-path probe |
| MED | bp-0003 labeled EXISTS/phase-1 but phase 1 is fenced off the existing root compose — literal prover would hit the live s001 stack | bp-0003 → BUILD, proof pinned to the POC compose's db service |
| MED | Read-only source mount + missing Cargo.lock → EROFS on first build | tk-0102 keeps Cargo.lock in the POC crate; builds run `--locked` |

**Consumers**: ox's phase-1 packet executable as written; phase-2 traps (network, config, volumes) now named in tasks rather than discovered at integration.
**Open decision (human)**: podman live verification timing (plan open_questions).
