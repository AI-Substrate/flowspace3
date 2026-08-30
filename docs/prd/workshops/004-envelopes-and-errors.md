# Workshop 004 — Response envelopes & errors
**Type**: API Contract · **Date**: 2026-08-26 · **Author**: o-prime, direction from Jordan ("common envelopes, central error codes, actionable errors, JSON-only v1") · **Status**: AUTHORITATIVE
**Consumers**: every CLI verb, every daemon HTTP endpoint, MCP tools, workshop 003's surface.

## One envelope, everywhere

Every CLI command and daemon endpoint returns exactly one of these two shapes — JSON-only in v1 (human rendering is a later additive layer over the same envelope):

```json
{ "ok": true,  "command": "search", "v": 1, "data": { …verb-specific… }, "meta": { …optional… }, "next_action": "optional agent steer — what a consumer typically does next (PRD req 44)" }

{ "ok": false, "command": "search", "v": 1,
  "error": {
    "code": "FS3-E-STORE-UNAVAILABLE",
    "message": "cannot reach the store at postgres://…:5433/flowspace3",
    "fix": "if the stack is not running: docker compose up -d — then re-run. `flowspace3 doctor` diagnoses further.",
    "details": { "cause": "connection refused", "elapsed_ms": 5600 },
    "retryable": true } }
```

- `ok` is the ONLY discriminator — consumers never sniff shapes.
- `fix` is MANDATORY on every error: the next command or config change, concrete, copy-pasteable. This codifies the pattern already proven in the field (the missing-API-key message, the migrate compose-up hint, the Azure four-rejections mapping).
- `v` bumps only on breaking envelope change (not per-verb payload evolution).
- Daemon HTTP: same body; status mapping mechanical (`ok:true`→200, else by code class: `*-INVALID-*`→400, `*-NOT-FOUND`→404, `*-UNAVAILABLE`→503, else 500). CLI exit codes: 0 ok · 1 error · 2 usage.

## Central error-code registry

- Codes: `FS3-E-<AREA>-<PROBLEM>` (SCREAMING-KEBAB): areas = `CONFIG, STORE, GIT, SCAN, PROVIDER, QUEUE, QUERY, DAEMON, USAGE`. Human-scannable, greppable, stable forever (a retired code is never reused).
- **The registry is code**: one `fs3-core::error::Catalog` module — every code is a const with its default `fix` template beside it. `fs3_core::Error` variants carry their code; the envelope serializer reads message/fix from the variant.
- **Docs generated, never hand-written**: `docs/reference/error-codes.md` is emitted from the catalog (a tiny generator bin or test-time write) and a **drift test fails if a code exists without a docs row or a `fix` template** — same encode-don't-document muscle as the arch check.
- Adding an error = add variant + code + fix in ONE file; the test forces the docs current. A worker inventing an ad-hoc error string outside the catalog is a review flag.

## Actionable-error doctrine (the bar every `fix` must clear)

1. Name the thing that failed CONCRETELY (which url, which env var, which file, which deployment).
2. Distinguish confusable causes when the transport can't (Azure's 401 vs 403 vs 404 mapping is the exemplar — each gets its own code).
3. `fix` says what to DO, not what went wrong again — command, config line, or doctor pointer.
4. Errors never leak secrets (redaction rules from s001 rev-0002 bind here; `Secret`-typed values can't Debug-print).
5. When multiple problems exist, report ALL of them at once where the operation allows (config validation's collect-don't-fail-first rule generalizes).

## Decisions

| # | Decision | Rejected | Why |
|---|---|---|---|
| D1 | JSON-only v1, one envelope for ok+error | human text now / per-verb shapes | one path to get right; `ok` discriminator; human layer later reads the same envelope |
| D2 | Central catalog IN fs3-core with codes+fix templates as code | docs-first registry / scattered strings | single writer, compiler-checked; drift test keeps docs generated+current |
| D3 | `fix` field mandatory | optional hint | the field being required is what makes the doctrine stick |
| D4 | Mechanical HTTP-status mapping from code class | per-endpoint status choices | zero judgment calls at call sites |
| D5 | `retryable` boolean on every error | leave it to consumers | the daemon's own job runner needs it anyway (queue retry policy) |

## Open questions
1. Streaming/progress output (long scans): envelope-per-line NDJSON events vs polling `status` (sketch: polling; NDJSON later if needed).
