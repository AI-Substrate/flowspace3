# Worker brief — flowspace skill for agents (.agents/skills) · new coder seat
**From**: pij-instant-lynx (o-prime) · 2026-08-26 · PRD req-0052 · Jordan: "agents may not be aware that it's installed… it has to tell it how to use it." MORE TASKS WILL FOLLOW on this seat (Jordan direct) — treat this as unit 1 of a resident engagement.

## The job
Author `.agents/skills/flowspace/SKILL.md` — the skill any agent loads to USE flowspace as a search tool. Audience: an agent in SOME OTHER repo where flowspace may or may not be installed. Lean, pointer-heavy, never duplicating what the binary teaches.

1. **Detect**: how to check it exists (`command -v flowspace3`; `flowspace3 doctor` for health) and whether THIS repo is indexed (`flowspace3 status` roots).
2. **Install (brief)**: one paragraph — convenience script one-liner + README link for the full agent-onboarding funnel; do NOT restate the funnel (README/doctor own it).
3. **Learn**: `flowspace3 docs list` / `docs get agents` / `docs get providers` are the authoritative in-binary guides — the skill points, the binary teaches. (Sawfish is building these NOW — name the topics, don't invent their content; coordinate if a name differs.)
4. **Search — the heart of the skill**: `flowspace3 search "<query>"` with modes (auto/semantic/text/regex), the filters (`--repo/--path/--limit/--min-score/--source`), reading the envelope (results, scores, `next_action`), and `el:` addresses as the currency for follow-ups. Real examples with real-looking output shapes (source them by RUNNING searches against this repo's live index).
5. **Why semantic**: a short section on when semantic beats grep (meaning-shaped questions, "where do we handle X", unfamiliar codebases) and when grep/regex mode is still right (exact identifiers) — teach the judgment, not dogma.
6. **Failure paths**: empty results (fake-vs-real embedder mismatch → run doctor), daemon down (doctor degraded → `flowspace3 daemon &`), not indexed (`flowspace3 add .`). Every error envelope carries `fix` — say "trust the fix field".

## Rules & fence
- Fence: `.agents/skills/flowspace/**` only (+ this brief's ack). Study `.agents/skills/add-provider/SKILL.md` for the house skill style.
- Verify claims by RUNNING the CLI against this repo's live index — no invented flags or output; if a flag you want is missing, note it as feedback to me, don't fabricate.
- Conventional commits, file-scoped adds, clean shared index, push-first (ruling 2026-08-26 + amendments).
- Report to pij-instant-lynx: claim · sha · one example transcript. Deviations = stop-and-ask. Then WAIT — Jordan has follow-on tasks for this seat.
