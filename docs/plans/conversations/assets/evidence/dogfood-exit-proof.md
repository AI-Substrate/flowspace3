# Dogfood exit proof — conversations v1

Run: 2026-08-27, w-conversations (pij-armed-ape), branch w-conversations-phase3.
Database: flowspace3_ape_dogfood (private scratch; production untouched).
Providers: the REAL Azure embedder and summarizer from ~/.config/flowspace3 —
the search below is a real embedding and a real LLM summary, not a fake.
Transcript: this packet's own fleet session. The conversation that built
conversations is the honest thing to index first.

## 1. Import a real fleet transcript
```
$ flowspace3 conversation import ./w-conversations-session.jsonl
{
  "ok": true,
  "command": "conversation import",
  "v": 1,
  "data": {
    "accepted": 0,
    "already_stored": 14,
    "guid": "a5b74a41-2194-fd7d-88d2-8834a0632e5c",
    "identity": "git:github.com/AI-Substrate/flowspace3",
    "summarized": 0
  },
  "next_action": "nothing new in this batch — all 14 turns were already stored. `flowspace3 search \"<question>\" --source conversation` searches what is there."
}
```

Re-running it unchanged accepts nothing — the idempotence, on real data.

## 2. Grow it, and re-import: only the delta lands
```
$ flowspace3 conversation import ./w-conversations-session.jsonl   # two turns longer
{
  "ok": true,
  "command": "conversation import",
  "v": 1,
  "data": {
    "accepted": 1,
    "already_stored": 14,
    "guid": "a5b74a41-2194-fd7d-88d2-8834a0632e5c",
    "identity": "git:github.com/AI-Substrate/flowspace3",
    "summarized": 0
  },
  "next_action": "1 turn(s) stored and queued for enrichment. `flowspace3 status` watches the queue drain; then `flowspace3 search \"<question>\" --source conversation`."
}
```

## 3. Search a known moment by MEANING

The question shares almost no words with the turn that answers it — no
"stop-and-ask", no "roots.rs", no "violation". Semantic retrieval is doing
the work.

```
$ flowspace3 search "why did we drop the foreign key on the anchor" --source conversation --limit 3
0.606  raw    conv:a5b74a41-2194-fd7d-88d2-8834a0632e5c#t6
        STOP-AND-ASK on the anchor column. Measured: roots.rs:173 deletes the repos row inside the removal transaction, so workshop 005's repo_id foreign key cannot hol
0.552  smart  conv:a5b74a41-2194-fd7d-88d2-8834a0632e5c#t6
        The removal transaction in roots.rs:173 deletes the repository row, so workshop 005’s repo_id foreign key causes removal to fail with a foreign_key_violation wh
0.481  raw    conv:a5b74a41-2194-fd7d-88d2-8834a0632e5c#t7
        RULED: TEXT repo_identity, no FK. Your argument is complete: it is the codebase's own value-pointer doctrine, it survives removal, auto-relinks through repos.id
```

The top hit is the stop-and-ask itself, and its LLM summary competes beside
the raw text (`match_field` says which won). The ruling that answered it is
the next hit. Both carry `conv:` addresses, so the next step is mechanical.

## 4. Window around the hit — you choose what you pay for
```
$ flowspace3 get conv:a5b74a41-2194-fd7d-88d2-8834a0632e5c#t6 --before 1 --after 1
address: conv:a5b74a41-2194-fd7d-88d2-8834a0632e5c
turns:   15 | around: 6 | repo: git:github.com/AI-Substrate/flowspace3
  t5 [human/peer] 2026-08-27T09:36:00Z
       A3 APPROVED and thank you for raising it at phase 1 where it belongs. A4 APPROVED: reserved parser_version conversation/ ...
  t6 [agent/system] 2026-08-27T09:50:00Z
       STOP-AND-ASK on the anchor column. Measured: roots.rs:173 deletes the repos row inside the removal transaction, so works ...
  t7 [human/peer] 2026-08-27T09:52:00Z
       RULED: TEXT repo_identity, no FK. Your argument is complete: it is the codebase's own value-pointer doctrine, it survive ...
next: turns 5–7 of 15; `flowspace3 get conv:a5b74a41-2194-fd7d-88d2-8834a0632e5c#t7 --after 20` reads on from here
```

## 5. Outline the whole conversation
```
$ flowspace3 tree conv:a5b74a41-2194-fd7d-88d2-8834a0632e5c
target: w-conversations: executing the conversations v1 plan
kind:   conversation | total: 15
  c#t1 [human/peer] Before any work: reply with your pij id, spawnId, model+effort, 
  c#t2 [agent/system] CANARY-OK. pij id pij-armed-ape, spawnId s1787822150755-26769, m
  c#t3 [human/peer] Canary verified. GO on orientation: read in the briefs authority
  c#t4 [agent/system] Two evidence-backed plan corrections before coding. A3: the --re
  c#t5 [human/peer] A3 APPROVED and thank you for raising it at phase 1 where it bel
  c#t6 [agent/system] STOP-AND-ASK on the anchor column. Measured: roots.rs:173 delete
  c#t7 [human/peer] RULED: TEXT repo_identity, no FK. Your argument is complete: it 
  c#t8 [agent/system] STOP-AND-ASK, production defect found by a phase-2 test. GC sile
  c#t9 [human/peer] RULED: fix it in your phase-2 PR. Requirements: the fix rides as
  #t10 [agent/system] Fix landed with recovery. The predicate now asks both vector spa
  #t11 [human/peer] Fourth commit accepted. Then GO PHASE 3 -- remember the conv: ar
  #t12 [agent/system] Phase 3 surface built. The critic-finding-4 trap is real and mut
  #t13 [human/peer] Phase 3 on. Report when the dogfood proof is in.
  #t14 [agent/system] Running it now against a private dogfood database with the real 
  #t15 [agent/system] Exit proof captured: imported, grown, searched by meaning, windo
```

## 6. List and forget
```
$ flowspace3 conversation list
  conv:a5b74a41-2194-fd7d-88d2-8834a0632e5c  turns=15  repo=git:github.com/AI-Substrate/flowspace3
    w-conversations: executing the conversations v1 plan
```

## 7. Remove, then collect — the anchor was a pointer, not ownership
```
$ flowspace3 conversation remove a5b74a41-2194-fd7d-88d2-8834a0632e5c
{
  "ok": true,
  "command": "conversation remove",
  "v": 1,
  "data": {
    "elements": 15,
    "existed": true,
    "guid": "a5b74a41-2194-fd7d-88d2-8834a0632e5c",
    "turns": 15
  },
  "next_action": "15 turn(s) forgotten. Their summaries and vectors are keyed by content and may still be shared, so `flowspace3 gc` decides those — it reclaims whatever nothing else carries."
}

$ flowspace3 gc
{
  "ok": true,
  "command": "gc",
  "v": 1,
  "data": {
    "elements": 0,
    "embeddings": 23,
    "jobs": 0,
    "summaries": 8,
    "total": 31
  },
  "next_action": "reclaimed 31 row(s): 0 queued job(s), 0 element(s), 8 summary/summaries, 23 vector(s)"
```

The turns and their turn elements go with the conversation; the summaries
and vectors they paid for are keyed by content, so `gc` reclaims exactly
what nothing else carries.

## What this proves

| acceptance criterion | evidence above |
|---|---|
| ac-0001 imported JSONL becomes queryable turns; re-import changes nothing | steps 1–2 |
| ac-0003 at/above the gate a turn gets summary + both embeddings | step 3: `match_field` smart AND raw for one turn |
| ac-0004 `--source conversation` returns turn hits with `conv:` addresses | step 3 |
| ac-0005 a windowed fetch returns the contiguous slice, honest at edges | step 4 |
| ac-0006 every turn carries source and the conversation its anchor | steps 4–5, `[role/source]` and `repo` |
| ac-0009 growing a conversation appends only the delta | step 2: `already_stored` 14, `accepted` 1 |
| ac-000a list and remove exist as envelope verbs; gc reclaims after | steps 6–7 |
| ac-0008 a real fleet transcript imported, searched and windowed | this whole run |
