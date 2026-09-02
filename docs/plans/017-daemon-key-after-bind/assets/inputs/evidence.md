165. **A pij-government seat's `flowspace3 daemon` is now serving prod
    :7373** (06:04:09Z, pid 77843, cwd ~/pi-hacking/fs3-rs-tap-supersedes-
    legacy, parent bun/omp gpt-5.6-sol --auto-approve, pid 96641; binary =
    our target/release/flowspace3 via PATH, database = prod :5433). It
    bound the port the second pid 901 exited for the 015 bounce; o-prime's
    relaunch in pane %50 died "address already in use". Earlier instance
    pid 29080 caused row 164. Ruled: do NOT kill another government's
    process; escalated to weasel via Jordan; prod is healthy on their
    instance (same binary/db) so the 015 receipt proceeds on it; relaunch
    in %50 once 7373 is free. Encode (fs3): the daemon must refuse to
    start against the prod database unless FS3_PROD_OWNER (or the pane
    identity) matches; and `flowspace3 daemon` in a foreign cwd should
    default to a per-cwd port/db, not the global one.
    PLAN 015 — **#103 MERGED** 82f60ec: the review-015 record is on main;
    docs worktree removed. Remaining: the row-147 receipt (retake running
    on the squatter daemon, TS symbols climbing), then tidy fox/boar seats
    and both 015 worktrees.
    ROW 147 — LIVE (06:18Z, interim): after the 015 build and the pij
    re-ingest (1,943 files enqueued, 0 unchanged under @3), prod already
    holds 553 Function + 145 container elements across the first 100 .ts
    files parsed under fs3-parsers@3 — the first TypeScript symbols the
    index has ever had. Scan queue still draining; final receipt (≈3,452
    for the pij tree) + searches follow.
    PLAN 013 — FIX PUSHED b332f46 (amistad, 06:19): option B as ruled —
    raw+smart scope keys pre-resolved once and applied inside HNSW before
    ORDER BY/LIMIT; payload/chooser page-bounded with the required hash
    union; paired geometry → Ok, 5 scoped hits, pass 1 (74 ms under load);
    no-growth geometry → 0 hits, exhausted=true, pass 2; query_embeddings
    keeps its Err; scan_incomplete on SearchOutcome + envelope meta
    (additive). Mutations: growth check removed → RED; bound Err restored
    → RED; JIT setup removed → RED; all restored. Shape 2/2 (7.3 ms, 3,113
    shared hits, HNSW rows 160, smart loops 159); store 53/53, daemon
    56/56, clippy clean; all DB tests on :5434. CI running; carp on the
    delta.
    (o-prime note) the 015 receipt retake died on a zsh quirk — an
    unquoted `$PSQL` command variable is NOT word-split in zsh, so every
    gate query was "command not found"; re-run as a bash script. Encode:
    receipt scripts are files with a bash shebang, never inline zsh.
    ROW 147 — RECEIPT, with a catch: prod now holds 586 Function + 154
    container elements under fs3-parsers@3 across 105 TypeScript blobs
    (740/740 non-file elements embedded; queue drained). BUT the pij
    extension code — 563 of the repo's 667 .ts files — lives under `.pi/`
    and discovery skips hidden directories by default (row 125) with NO
    `--include-hidden` on `flowspace3 add`, so "where does the pij
    extension register the seat" can never return a .ts element on prod;
    only the 100 visible .ts files are indexed. The grammar is proven on
    prod; the pij-extension story is blocked on row 125 → promote row 125
    to a packet (a per-root `include_hidden` opt-in). Also: a path-scoped
    search that returns 0 carries `empty_because: None` (row 144 again).
    ROW 147 — receipt searches: 740 TS elements exist and are embedded,
    but "what does the <name> function do" for three DB-chosen TypeScript
    functions from the visible pij files returned Rust functions, docs and
    conversation turns — never the TS element (walls 5–29 s at load 29).
    Hypothesis: search auto-scopes to the caller's worktree (o-prime ran
    from the flowspace3 clone; #91 scopes ask by path) — testing the same
    queries from the pij checkout's cwd before calling it a ranking miss.
    PLAN 013 — CI GREEN on b332f46 (06:25). Merge train fires on carp's
    delta verdict; bounce + search before/after after that.
    ROW 147 — **RECEIPT PROVEN on prod** (06:25Z): searching the first
    line of a TypeScript function (`function commonPrefixLength(a:

## Code facts (o-prime, main c2f4709)
- auth.rs: stage() writes a NamedTempFile beside daemon.key; StagedAuth::publish() renames it into place (mode 0600). Test 'staging_is_invisible_and_publish_replaces_with_mode_0600' at auth.rs:182.
- boot.rs serve(): 'Binding precedes credential publication' — TcpListener::bind(&address) at :577, auth.publish() at :582. The --json/sandbox path: :207 reserves 127.0.0.1:0, :224 stage(sandbox_directory.path()).
- Timeline 16:54:32 local: prod pid 1548 listening on 127.0.0.1:7373 since 16:51; pid 89658 'flowspace3 daemon --json' (cwd ~/pi-hacking/fs3-spawn-reports-bind, global config) started; daemon.key mtime became 16:54:32; clients → FS3-E-DAEMON-UNAUTHORIZED, doctor 'key stale'; 89658 never held :7373. The path that wrote the key is NOT yet known — t1 finds it.
- config: ~/.config/flowspace3/config.toml (global); FS3_CONFIG_DIR steers the loader (config.rs:36).
