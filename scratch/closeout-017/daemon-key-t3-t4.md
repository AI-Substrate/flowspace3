# daemon-key t3/t4 boundary

T3 and T4 complete under ruling 003; reply-002's conflicting t4 paragraph was discarded.

## T3

- `Auth` records the just-published key mtime and bound listener port.
- 401 details add `key_newer_than_daemon: bool`.
- Strict `current_mtime > published_mtime` avoids a normal-publication false positive.
- A later rewrite names the shared-key overwrite and `restart the daemon that owns :<port>` without exposing key bytes.
- Auth tests green 5/5; live CLI rewrite/render test green.
- Mutation: reversing the mtime comparison makes the live CLI test red; restored green.

## T4

- `[daemon].owner_root` and `FS3_DAEMON__OWNER_ROOT` configure the production owner tree.
- Before key staging or DB access, the shipped prod URL is refused outside that root unless `FS3_PROD_OWNER=1`.
- Refusal: `FS3-E-PROD-NOT-DESIGNATED`; names `FS3_PROD_OWNER=1` and `postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test`.
- Cwd inside root, explicit override, and non-prod URL proceed.
- Config + daemon guard tests green.
- Mutation: unconditional guard bypass makes the foreign-cwd test red; restored green.

No production surface touched. T3/T4 ddoc rows and done-when assertions are checked via global `ddocs`.
