-- Migration 0030 - queue every stored conversation for the intake restatement.
--
-- Turns are append-only, so the 2026-10-13 intake fixes (write-family paths
-- read out of JSON inputs; pij deliveries, harness notices and tool results
-- given the role and source the readers now record) reach only turns read
-- after them. `restate_after` is the daemon's cursor through one
-- conversation: the last turn it has restated, 0 for none yet, and NULL once
-- the conversation is done. Conversations created from here on are stored
-- right the first time and start NULL. The rules live in Rust, beside the
-- readers they restate; nothing here judges a turn.
ALTER TABLE conversations ADD COLUMN restate_after INTEGER;

UPDATE conversations SET restate_after = 0;

CREATE INDEX conversations_restate_idx ON conversations (guid)
 WHERE restate_after IS NOT NULL;
