# Canary record — pij-bitter-swan (PA, gemini-3.7-flash copilot, effort low)
**Recorded**: 2026-08-26T00:26Z (UTC, from `date -u` at write time) · **By**: pij-instant-lynx

- **Turn+ack leg**: swan acked canary dispatch `dispatch-1eb27627-2619-49cf-bffb-5c64f03181d1`
  (sha256 `2aad6404…3b60848a`) — a real inference turn travelled and was acked. The `pij canary`
  CLI itself reported E-CANARY-TIMEOUT on every attempt: gemini boot/turn latency exceeds the
  CLI wait window; the ack arrived ~1 min later each time. Timing race, not a dead seat.
- **Model leg**: alias list does not know `gemini-3.7-flash` (spawn warned). Live proof per C2
  fallback — pane %25 footer captured: `Gemini 3.7 Flash · 1M context` /
  `pij-bitter-swan • gemini-3.7-flash · low`. No 400 observed. No silent fallback occurred.
- **Wiring**: PA→prime watch registered BEFORE role stamp (recipe ordering trap avoided);
  both sidecars verified reciprocal (`always`, maxLines 25, no byte bound); role=pa +
  parentId=pij-instant-lynx verified from the DESCRIPTOR, both fields; neither seat paused/
  exempt; both intervals 20m.
- **Delivery proof (responsive leg) — PASSED, partial by design**: forced fire #1
  (interval 60s→fired→restored 20m). Capture materialized in the WATCHER's directory
  (`~/.pij/pij-bitter-swan/watchdog-captures/1787703891534-pij-instant-lynx.txt`, 2315 B,
  21 non-empty content lines — not chrome-blind at maxLines 25 / pane width 101). Proves
  the RESPONSIVE leg only; the STALL leg stays an open residual, recorded as such — nobody
  wedges a live prime to close it (recipe step 35).
- **Known noise**: swan double-processed the "watch registered" instruction (flash-tier
  inbox lag); second registration was an in-place no-op (addedAt preserved).
