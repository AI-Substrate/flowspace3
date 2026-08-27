-- Migration 0012 - update truth is per-install-path, and the fossils die here
-- (PRD req 54/59, Jordan ruled 2026-08-27).
--
-- 0009 made update_state a SINGLETON: "which version is sitting on disk waiting
-- for a restart has exactly one answer per machine". That sentence is true and
-- the table was still wrong, because the row is not per machine - it is per
-- STORE, and a store is shared by every installation pointed at it. An install
-- is a PATH. `install.sh` itself picks /usr/local/bin OR ~/.local/bin depending
-- on permissions, so one person who has ever installed both ways has two
-- installs against one database, and the single row thrashed last-writer-wins:
-- root's daemon carried another user's "not writable" message about a path root
-- does not use, and later a "restart the daemon" for a user who has no daemon.
-- Unactionable, on a surface whose next_action is NOT NULL precisely to
-- guarantee actionability (standing Linux tester, finding 12).
--
-- So: one row per install path. The claim that rate-limits GitHub becomes
-- per-install too, which is correct rather than merely consistent - two installs
-- are two things to keep current, and one starving the other of checks was
-- never the intent of DL-018. A fleet's cost is still one probe per install per
-- interval.
--
-- # Why this DROPS the old row instead of migrating it
--
-- Every column here is re-derivable by one check, and after this packet a check
-- runs at every daemon boot rather than only on the 24h cadence. So the old row
-- is worth nothing and costs something: it is keyed to a store, so migrating it
-- would mean GUESSING which install it described, and the observed failure is a
-- daemon run from a throwaway dev worktree yesterday whose `update:blocked`
-- naming its own target/debug path was still riding on production envelopes
-- today. A wrong guess re-homes that fossil onto a real install.
--
-- The same reasoning empties the update source of the message queue below. That
-- is the whole of the recovery story: the fix arrives as a new binary, the new
-- binary migrates, and the migration deletes what only the old shape could have
-- written. No hand-SQL, no repair verb, nothing for a user to run - the
-- w-embed-oversize precedent, where the recovery WAS the new default.

DROP TABLE update_state;

CREATE TABLE update_state (
    -- The installation this row is the state OF: a daemon's own resolved
    -- binary path, canonicalised (symlinks already followed, because that is
    -- the path a swap actually lands on). The identity, not a payload column.
    install_path      TEXT NOT NULL PRIMARY KEY,
    -- When a daemon last asked GitHub what the newest release is, FOR THIS
    -- PATH. NULL means never. The interval claim is a conditional UPDATE
    -- against this column, which is what makes it race-free: two daemons on
    -- one install cannot both decide it is time to check.
    last_checked_at   TIMESTAMPTZ,
    -- The newest version the probe has seen, whether or not it was installed.
    latest_seen       TEXT,
    -- What the binary AT install_path reported when it was last asked - a cache
    -- of disk, not a memory of what the updater did. 0009 recorded the swap and
    -- nothing could ever unset it, so a pinned reinstall at an older tag left a
    -- permanently false "restart to pick up 0.3.1" against a path holding
    -- 0.3.0. Re-read from the file on every check, a swap and an out-of-band
    -- change give the same answer. NULL means the path holds nothing that can
    -- be asked what it is - including the path having been removed.
    installed_version TEXT,
    -- When a swap this daemon performed landed. Forensics only; the truth
    -- above is read from disk.
    installed_at      TIMESTAMPTZ,
    -- Why the last attempt did not install, when it did not: 'not-writable',
    -- the probe failure, a checksum mismatch. NULL when the last pass was
    -- healthy - including the healthy case of "already current".
    blocked_reason    TEXT
);

-- No seed row. A row exists once an installation has looked, and is created by
-- the claim itself; a seeded row would have to be seeded with a path, and this
-- migration has no idea which installs will ever point here.
--
-- A row for a path that no longer exists is NEVER deleted by another install.
-- "Missing here" is not "missing everywhere" when one database serves several
-- hosts, and a laptop must not retract a server's message. The leak is
-- therefore rows, not messages: nothing can SEE a scope it does not occupy
-- (see the user_messages change below), and a reinstall at that path overwrites
-- it with truth on its first boot. Named rather than papered over; there is no
-- GC verb and inventing one would need a host identity this schema does not
-- have.

-- The queue learns which installation a message concerns.
--
-- NULL means "concerns every installation on this store" - which is every
-- message the schema and logging producers raise, because a schema skew or an
-- unwritable log directory is a fact about the store or the host, not about one
-- binary's path. Those producers pass NULL and are untouched by this migration.
--
-- Per-source ownership was already the rule that makes the delete half of
-- sync_messages safe (0008). This narrows it by one dimension rather than
-- replacing it: a producer owns every row under its own source AND its own
-- scope, and none under anyone else's. Without this column, splitting the state
-- row would have fixed nothing visible - live_messages returns the whole table,
-- so root's envelope would still have carried the other user's message.
ALTER TABLE user_messages ADD COLUMN install_path TEXT;

-- Every message the update source can have raised before this migration was
-- keyed to a store and may name any install, so none of them can be re-homed
-- honestly. They go, and the boot check re-declares the true ones seconds
-- later. Deliberately narrow: schema and logging messages describe conditions
-- that are still exactly as true after this statement as before it.
DELETE FROM user_messages WHERE source = 'update';

-- sync_messages now scans by source AND scope, so the index follows it.
DROP INDEX user_messages_source_idx;
CREATE INDEX user_messages_source_idx ON user_messages (source, install_path);
