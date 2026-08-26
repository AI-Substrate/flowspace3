-- Migration 0009 - update state: what the auto-updater has done, and when it
-- last looked (PRD req 54, Jordan 2026-08-26/27).
--
-- ONE ROW, enforced. This is installation-level state, not a log: "which
-- version is sitting on disk waiting for a restart" has exactly one answer per
-- machine, and a table that could hold two would need a rule about which one
-- wins. `singleton` is that rule, spelled in the schema.
--
-- Why Postgres rather than a file beside the binary: the envelope steering has
-- to survive the OLD daemon restarting, and a restarted daemon rereads the
-- database. It is also the reason `last_checked_at` lives here - the check
-- interval is honoured against a timestamp, not a timer, so a daemon restarted
-- every ten minutes still checks once a day instead of once per boot. GitHub's
-- release endpoints are a shared rate-limited resource (fleet retro DL-018).
--
-- The interval claim is a conditional UPDATE against this row, which makes it
-- race-free for free: two daemons pointed at one database cannot both decide
-- it is time to check, because only one UPDATE can win.

CREATE TABLE update_state (
    -- Exactly one row, forever.
    singleton        BOOLEAN NOT NULL PRIMARY KEY DEFAULT TRUE,
    -- When a daemon last asked GitHub what the newest release is. NULL means
    -- never, which is why the first pass after boot always checks.
    last_checked_at  TIMESTAMPTZ,
    -- The newest version the probe has seen, whether or not it was installed.
    latest_seen      TEXT,
    -- The version now sitting at the install path because WE put it there.
    -- Set only after a successful atomic swap; this is what the envelope
    -- steering is about.
    installed_version TEXT,
    installed_at     TIMESTAMPTZ,
    -- Where the swap happened, canonicalised. Named in the message so a user
    -- with two installs knows which one moved.
    install_path     TEXT,
    -- Why the last attempt did not install, when it did not: 'not-writable',
    -- the probe failure, a checksum mismatch. NULL when the last pass was
    -- healthy - including the healthy case of "already current".
    blocked_reason   TEXT,

    CONSTRAINT update_state_is_a_singleton CHECK (singleton)
);

-- Seed the row so every later statement is an UPDATE and no caller has to
-- decide between insert and update.
INSERT INTO update_state (singleton) VALUES (TRUE);
