# pij → Claude (cc-socks) delivery telemetry — read-only audit, 2026-09-02

## Summary

1. **LOGGED PROPERLY: PARTIAL.** Every Claude-side delivery gets a `delivered_messages` row + a `message.pushed`/`delivery.outcome` spine event, and the transcript cross-check shows **0 dropped, 0 duplicated** (123 sent = 123 distinct frames in the transcript, all consumed).
2. **Gap A — origin is write-only, no consumption receipt.** Claude rows are always `origin='injected-to-transport'` (= "wrote the frame to the socket, no denial heard"); omp/pi rows carry `reader-read` (the reader pulled the body). Nothing in pij records that the Claude session actually turned the frame into a user turn.
3. **Gap B — the spine lies about retries.** 23 of 123 magpie messages today have a spine `delivery.outcome=queued` and NO later `delivered` event, yet all 23 were delivered by the drain worker (jobs `done`, `delivered_messages` row, frame in transcript). `pij-rs tail`/spine readers see "queued forever".
4. **Gap C — transport is recorded (`transport:"claude-uds"` on the spine event) but not on `delivered_messages`; the socket path/reply address is recorded NOWHERE in pij (only inside Claude's transcript); daemon.log has zero uds lines; the `pij sessions` join keys the Claude session to the legacy id `pij-instant-lynx`, not to `pij-binding-magpie` where the ledger rows live.**
5. **The engineering-harness collector (git trace2 → git-ai socket → `refs/notes/ai`) is NOT in scope for pij delivery at all** — it only ingests git trace2 events; there is no link and none is claimed. Flowspace3 `conversation ingest` DOES capture the frames as conversation turns (conv `f3a6f4d9…` = session `a5a5588f…`), which is the only place consumption is durably visible.

## Q1 — How pij-rs delivers to a Claude seat

- `~/pi-hacking/pij/crates/transport/src/uds.rs:180-215` `UdsTransport::deliver`: refuses unless `seat.harness==Claude && cross_session_inbound_accept==Some(true)` and no `command`; `discover()` finds Claude's own socket `/tmp/cc-socks/<claude-pid>.sock` + peer token from Claude's session records (`SessionRecord.messaging_socket_path`, `PeerKey.peer_token`, lines 218-260); binds a per-delivery **reply inbox** `/tmp/cc-socks/pij-uds-<daemon-pid>-<n>.sock` (`REPLY_PREFIX` line 32, `ReplyInbox` line 349) — so the `from="uds:…pij-uds-46979-66.sock"` seen in Claude is the DAEMON pid (46979 = pre-restart daemon; 71772 = current, per `~/.pij-rs/daemon.runtime.json`), not the Claude pid.
- `uds.rs:304-330` `build_peer_frame`: wraps the body as `{"type":"user","msg_id":…,"message":{"content":"<cross-session-message from=… from-name=…>…"},"priority":"next","from":origin}`.
- `uds.rs:482-544` `send_frame` + `wait_for_status`: waits `ack_wait`/`hold_grace` for a `peer_message_status` frame (`held`/`denied`/`expired`/dropped, `classify_status` line 588). **Silence ⇒ `Delivered{origin: InjectedToTransport}`** (line 538: `Some(Status::Unrelated) | None => Delivered`). No positive ack exists in the protocol.
- Daemon side, first attempt: `crates/daemon/src/delivery/mod.rs:255-297` claims `note_delivered(to, msg_id, InjectedToTransport)` BEFORE calling the transport (R4-AMEND-4), `forget_delivered` + enqueue on `Queued`; publishes `delivery.outcome` at line 504. Retry path: `crates/daemon/src/pointer/worker.rs:299-303` `ack_delivery(job_id, origin)` — writes the ledger, **publishes no spine event**.
- Claude-side acceptance is a settings flag pij ensures: `crates/harnesses/src/claude_settings.rs:142` (`crossSessionInbound: accept`).

## Q2 — What pij records (sqlite `~/.pij-rs/pij.sqlite`, today ≥ 2026-09-02 00:00 AEST)

Schema: `delivered_messages(recipient, msg_id, origin ∈ {injected-to-transport, verified-arrival, reader-read}, delivered_at)`; `spine_events(kind, seat, payload)`; `jobs(kind='delivery:<seat>', state, attempt, outcome)`. No column for transport, socket path, reply address, or consumer receipt. `dispatches` is unrelated (work packets, not messages). `seats.harness_session` is NULL for `pij-binding-magpie`.

| | `pij-binding-magpie` (claude, rs id) | `pij-quixotic-takin` (omp) |
|---|---|---|
| `delivered_messages` rows | 123, all `injected-to-transport` | 7, all `reader-read` |
| spine `message.pushed` | 123 | 7 |
| spine `delivery.outcome` | 123: 100 `{delivered, injected-to-transport}` + **23 `{queued}` never followed by `delivered`** | 7 `queued` → then `pointer-announced` 7, `pointer-parked` 2, `pointer-unparked{reason:reader-read}` 2 |
| consumption receipt | **none** | `reader-read` origin + `delivery.inbox-ack` path (`http/mod.rs:22,862`) |
| transport recorded | `transport:"claude-uds"` in outcome payload only | `transport:"claude-uds"` (same string even for omp — the field names the transport crate, not the route taken) |
| jobs | 23 `delivery:pij-binding-magpie` rows, all `done`, `attempt` up to 32 | 1 `done` |

`pij-instant-lynx` (legacy id): zero rows in any rs table — all rs-era traffic is keyed on `pij-binding-magpie`.
The 23 "queued" msg_ids each have a `done` job and a `delivered_messages` row (verified: `select … having q>0` → 23 ids; `jobs … in (…)` → 23). The spine never learns they landed. Whether the retry landed the SAME message twice cannot happen by construction (UNIQUE(recipient,msg_id) + `ack_delivery`), and the transcript confirms it (below).

## Q3 — Sessions join / ledgers / tail

- `pij sessions --json` (TS): `pij-instant-lynx` → `harnessSessionId: a5a5588f…`, `generation: legacy`; **`pij-binding-magpie` → `harnessSessionId: null`, `generation: rs`.** So the seat that owns the 123 ledger rows is not joined to the Claude session; the join is on the id that owns none. (omp rs seats are null too — the rs generation does not populate the join.)
- Legacy ledgers `~/.pij/pij-instant-lynx/` (`events.ndjson` etc.): last write 26 Aug; nothing for today's rs deliveries. `~/.pij/spine/events.ndjson` was touched today but is the TS spine, not the rs one.
- `pij-rs tail` streams the rs spine (`crates/daemon/src/http/mod.rs:1201,1223` `EventFilter::all()`), so it shows `message.pushed` + `delivery.outcome` per Claude delivery — with Gap B (23 stuck at `queued`) and no consumption frame. `~/.pij-rs/daemon.log`: **0 lines** mentioning `claude-uds`/`cc-socks`/`pij-uds`; no per-msg_id lines.
- Flowspace3 `conversation ingest` (`crates/daemon/src/convo_ingest.rs`, `docs/services/convo-source-claude.md`): the transcript IS indexed — `flowspace3 search "cross-session-message … pij-uds-46979"` returns `conv:f3a6f4d9-…#t9610` etc. with today's frames, and `conversation list` maps `a5a5588f…` ↔ `f3a6f4d9…`. Fidelity here is *higher* than pij's own (it holds the body as consumed), but it is keyed by native session id, not by seat/msg_id.

## Q4 — Engineering-harness collector

Out of scope, plainly. `harness instructions commit` / `harness doctor` rows `gitai-collector` and `attribution-at-risk` describe exactly one ingress: git trace2 over `af_unix:stream:~/.git-ai/internal/daemon/trace2.sock` (`git config trace2.eventtarget`), producing `refs/notes/ai`. Nothing in `.harness/engineering-harness.md` or the doctor layer list mentions pij delivery; the only pij references in doctor output are pij-team scaffolding and `conversation ingest --pij <seat>`. No link exists and none should be inferred.

## Q5 — Dropped / duplicated?

Transcript `…/a5a5588f-0979-439f-a1bf-ddf185a089c7.jsonl` (75 MB, 01:01Z–03:58Z = 11:01–13:58 local, matching ledger seq 573…1053):
- `grep -c 'cross-session-message from="uds'` = **0** — the JSONL escapes quotes; use `from=\\"uds`. 326 lines mention the tag.
- **123 distinct reply addresses** (`pij-uds-46979-*`: 305 lines; `pij-uds-71772-*`: 16) = **123 `delivered_messages` rows = 123 `message.pushed`**. 
- Per address: 48 → `queue-operation:enqueue` + a `type:user` turn (consumed as its own turn); 75 → `enqueue` + `remove` + `type:attachment {type:"queued_command", origin:{kind:"peer", verifiedPeerPid:46979}}` (consumed as an attachment to the next turn; enqueue→remove median 9.3 s, max 50 s). 48+75 = 123, no address without a consumption record, no address consumed twice (`uniq -d` on user turns: empty).
- `msg_id` does not appear in the transcript (1 incidental hit out of 2 probed), so ledger↔transcript correlation is by reply address + time only — the address is not stored on the pij side.

## What to encode (smallest changes that make the gaps visible)

1. **Gap B (spine truth):** in `pointer/worker.rs:299-303` and `:348` publish a `delivery.outcome {delivered, origin, attempt}` after `ack_delivery` succeeds. One event, existing kind; makes `pij-rs tail` and spine counts honest. Doctor check: `count(outcome=queued msg_ids with a delivered_messages row and no later delivered outcome) == 0`.
2. **Gap C (correlation):** add `reply_origin` (the `uds:/tmp/cc-socks/pij-uds-<pid>-<n>.sock` string) to the Claude `delivery.outcome` payload — it is already in hand at `uds.rs:206` (`inbox.origin()`). Then ledger↔transcript joins are exact. Optionally populate `seats.harness_session` for rs Claude seats from the same `SessionRecord` `discover()` already reads.
3. **Gap A (consumption):** not fixable pij-side without a protocol change (Claude sends no positive ack). Cheapest honest signal: a `flowspace3`/doctor check that joins `delivered_messages` (by reply address once #2 lands) against ingested conversation turns and reports `injected-but-unconsumed` per seat.

## What I ran (all read-only)

- `grep -rn 'cc-socks|pij-uds|cross-session' ~/pi-hacking/pij` (crates + docs); `sed -n` on `crates/transport/src/uds.rs` 175-360, 380-640; `crates/daemon/src/delivery/mod.rs` 215-300, 540-580; `crates/daemon/src/pointer/worker.rs` 286-352; grep `OUTCOME_EVENT_KIND|note_delivered|ack_delivery|subscribe_live`.
- `sqlite3 -readonly ~/.pij-rs/pij.sqlite`: `.tables`, `.schema`; `seats` rows for the 3 seats; `delivered_messages` by recipient ≥ `T0=1788271200`; `spine_events` kind counts, outcome payload group-by, queued-vs-delivered per msg_id; `jobs` by `kind='delivery:pij-binding-magpie'`; `delivery.inbox-ack` by harness.
- Transcript: `grep -o 'from=\\"uds:…\\"' | sort -u | wc -l`; python pass classifying line `type`/`operation` per address and enqueue→remove latency.
- `pij sessions --json`; `pij-rs tail --help`; `harness instructions commit`; `harness doctor --json` (layer names); `git config --get-regexp trace2`; `ls ~/.git-ai/internal/daemon`; `grep -c … ~/.pij-rs/daemon.log`; `ls ~/.pij/pij-instant-lynx`; `FS3_OUTPUT=json flowspace3 search …`; `flowspace3 conversation list`.
