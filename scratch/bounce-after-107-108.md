# Prod bounce runbook — after #107 (016) and #108 (017) merge

**Prerequisite discovered by reading the code, not the PR body (`boot.rs:409-441`):**
`refuse_undesignated_production_store` refuses when the DB url is the shipped prod
default AND `FS3_PROD_OWNER` is unset AND `[daemon].owner_root` does not contain the
cwd. **`owner_root` unset ⇒ `designated_by_root == false` ⇒ REFUSE.** The current prod
config has no `[daemon]` section at all, so the merged binary refuses to boot until the
config is edited. This is fail-closed and correct (a foreign daemon clobbered prod twice
today, rows 165/169), and the error names its own fix — but it makes the config edit a
MERGE PREREQUISITE, not a follow-up.

**And the edit cannot be made early:** `DaemonConfig` is `deny_unknown_fields`
(`config.rs:340`), so adding `owner_root` before the new binary exists would make the
CURRENT daemon fail to parse its config on its next boot. Config and binary must move in
the same window.

## Sequence (single window, o-prime only)

```bash
cd /Users/jordanknight/substrate/flowspace/flowspace3
git pull --ff-only origin main
cargo build --release --locked            # background it; ~minutes

cp ~/.config/flowspace3/config.toml ~/.config/flowspace3/config.toml.bak-$(date +%H%M)
cat >> ~/.config/flowspace3/config.toml <<'TOML'

# [daemon] — plan 017: designate this checkout as the production owner so a
# scratch or foreign daemon cannot take over the shared key / prod database.
[daemon]
owner_root = "/Users/jordanknight/substrate/flowspace/flowspace3"
TOML

bin/daemon-restart --binary /Users/jordanknight/substrate/flowspace/flowspace3/target/release/flowspace3
# assert rc==0 AND the new pid != the old pid (rows 164/167: the script has
# refused on ambiguous candidates and has crashed with Bus error AFTER the
# Ctrl-C and BEFORE the relaunch — never trust it without checking both)
flowspace3 ping --json
flowspace3 status --json
```

## Rollback
Restore `config.toml.bak-*` and relaunch the previous binary in pane %50. The key file
is untouched by a refusal (the guard runs before key staging), so a refused boot is a
no-op, not damage.

## Then
- `bash fs3-governance/scratch/receipt-016-prod.sh` — the row-125 receipt (opt-in re-add
  of `~/pi-hacking/pij`, wait for `.pi/` TypeScript symbols, then the `daemonLocation`
  search).
- Row 178 cleanup: reap the orphaned roots in the shared `flowspace3_test` DB, now that
  both gates are done.

## Two caveats from the 017 reviewer — expect this shape on the first bounce

Both measured through the real binary (hyena, review 017), and both matter because the
refusal is the state prod is in until `owner_root` is set:

1. **The refusal fires BEFORE `logging::init`.** It never reaches the daemon log file.
   The full `FS3-E-PROD-NOT-DESIGNATED` text goes to **stderr only**.
2. **`bin/daemon-restart` will hide it.** A refusing daemon exits inside the script's
   first 0.25 s poll, so the script never sees a process and reports only the generic
   `no replacement 'flowspace3 daemon' appeared in pane %50`. **The real error is in
   pane %50's scrollback** — read the pane, not the script's output, if the bounce
   "fails" after this merge. (Combined with row 181, the script's failure output is
   twice unhelpful: it can crash between stop and start, and it cannot report a refusal.)

**Also correct the packet/runbook assertion:** the prod key mtime is now
`2026-09-02 18:29:55`, not the earlier `16:57:45` — that is daemon pid 76514 (the release
binary from the main clone) republishing on its legitimate bounce, key mtime matching its
start to the second, one daemon alive. Verified by the reviewer, who had every reason to
flag it as a fence breach and instead checked it out first.

