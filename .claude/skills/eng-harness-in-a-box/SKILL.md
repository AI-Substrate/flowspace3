---
name: eng-harness-in-a-box
description: |
  The engineering-harness loop with zero dependencies — an engineering harness in a box.
  Use in any repo, especially one with no harness yet: find or establish a
  focal operating surface, name the deterministic proof before working,
  capture friction as you go, and offer one encodable improvement at the end.
  Advisory always — the user is never blocked, the agent is never silent.
  When a repo has an installed harness CLI and wants the full loop, use
  eng-harness-flow instead.
---
# eng-harness-in-a-box

- We're going to do work. Assume friction will happen.
- Do the task — and leave the environment better than you found it.
- Find the one focal operating surface, or establish a tiny one. Use it, don't guess around it.
- Before work: how will done be proved, deterministically?
- Find or establish one neat capture home for observations.
- Capture friction as it bites, one line each, keep moving.
- At the end: magic wand? What did you infer that should have been proved?
- Human chooses. Encode the fix, not the memory.
- Repeat.

Guiding ethos: Don't apologise - fix. 

\[found-or-established focal surface\] + \[deterministic proof\] + \[friction capture\] + \[human-chosen encoding\] = a harness, in a box.

---

## The steps, expanded

Two goals, every session: complete the requested task, and leave the
engineering environment better than you found it. The second never displaces
the first.

Every step follows one pattern: **find it, or establish it.** The repo may
have none of this yet — that is normal, not a blocker.

1. **Focal surface — find or establish.** Look for the one place this repo is
   operated: a harness CLI, justfile, Makefile, scripts dir, package scripts.
   Found: use it — don't guess around it. Missing: offer to start a tiny one
   that wraps what already exists (build / test / run / lint). Wrap, don't
   reinvent.
2. **Proof — find or establish.** Before working, ask: how will this be proved
   done, deterministically? Name the sensors (build, tests, lint, typecheck,
   boot, smoke). A missing sensor is worth a sentence out loud — and sometimes
   ten minutes to add — never a silent workaround.
3. **Capture home — find or establish.** Discover where this repo already
   keeps harness data (`.harness/`, a retro ledger, a notes file) and use it.
   Missing: choose one neat home — `.harness/observations.md` for what humans
   read (frictions, retros, magic wands), JSONL beside it only when entries
   must be machine-read. One home, dated entries, never scattered notes.
4. **Work through the surface, capturing friction as it bites.** A misleading
   error, an undocumented step, a retry, anything you had to infer that a
   check could have proved: one line each in the capture home, then keep
   moving. Don't apologise for the environment — it is a legitimate target
   for work.
5. **Retro — two questions at the end, never skipped silently.** Record both
   answers in the capture home:
   - Magic wand: what one command, check, fixture, default, or error message
     would make the next run easier, safer, or better proven?
   - What did you have to infer that the harness should have proved?
6. **Encode — the human chooses.** Offer the best answer. If accepted, encode
   the fix, not the memory: a command that does the thing, or a check that
   proves it, beats a paragraph that explains it.

Discriminate: capture what the next person or agent would also hit; let
one-offs go. Never gate, score, or block — the paved path wins by being
easier, and a declined offer is a normal outcome, not a failure.
