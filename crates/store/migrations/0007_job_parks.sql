-- Congestion is not poison, and `attempts` must not be asked to mean both.
--
-- A rate-limited job has not failed: the provider asked us to come back later.
-- Counting that as an attempt exhausts jobs that were never broken, so a
-- sustained squeeze would terminally fail a whole backlog of perfectly good
-- work. The park path therefore returns `attempts` to its pre-claim value and
-- counts here instead.
--
-- A separate column rather than a negative attempt count, because the two
-- questions are genuinely different and an operator asks them separately: "is
-- this job broken?" reads `attempts`, "are we being throttled?" reads `parks`.
-- Folding them would make a heavily throttled healthy job indistinguishable
-- from a flaky one.
ALTER TABLE jobs ADD COLUMN parks INT NOT NULL DEFAULT 0;

COMMENT ON COLUMN jobs.parks IS
    'Times this job was parked for provider congestion. Not a failure count.';
