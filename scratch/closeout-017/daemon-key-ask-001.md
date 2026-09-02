# Stop-and-ask 001 — t4 contract conflict

`daemon-key-prime-reply-002.md` materially differs from the READY plan and prior GO ruling:

- Plan ac-0004 / task t4 / impl-guide require a foreign cwd targeting `DatabaseConfig::DEFAULT_URL` to **refuse boot** with `FS3-E-PROD-NOT-DESIGNATED`, naming `FS3_PROD_OWNER` and the `:5434` test URL. Cwd inside `[daemon].owner_root` or `FS3_PROD_OWNER=1` proceeds.
- Reply 002 says absent designation behaves exactly as today, and a mismatched designation continues with a token private to its own `FS3_CONFIG_DIR` while reporting on stdout.

Those cannot both hold for an absent designation against the prod URL, and the “private token” behavior does not map to the existing shared `FS3_CONFIG_DIR` model.

Recommended ruling: retain the READY plan's refusal contract (it is deterministic, already tied to bp-0004/dw-3115, and prevents the foreign daemon before staging a key); treat reply 002 as wording-only except for its t3 clarification. If reply 002 intentionally replaces ac-0004, the plan/tasks/backpressure ddocs need an o-prime mutation before I can truthfully check t4.

T3 implementation is continuing independently; no t4 source change until ruled.
