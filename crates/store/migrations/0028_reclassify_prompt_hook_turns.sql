-- Migration 0028 - let the turn-class backfill re-judge `fs3 "…"` prompts.
--
-- The classifier now treats a person's turn that is ONLY an `fs3 "…"`
-- prompt-hook search as a retrieval call, hidden by default like an agent's
-- `flowspace3 search`. Turns stored before that rule were classed `work`.
-- Clearing their class hands them back to the existing backfill, which
-- re-classifies them with the same Rust rule ingest uses. The rule is not
-- copied into SQL; this predicate only picks candidates, deliberately loose
-- (any turn STARTING with `fs3` and a quote), and the classifier decides.
UPDATE elements
   SET turn_class = NULL
 WHERE kind = 'turn'
   AND turn_class = 'work'
   AND ltrim(left(raw_text, 16)) ~ '^fs3\s+["''“]';
