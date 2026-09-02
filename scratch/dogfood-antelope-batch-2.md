# flowspace3 dogfood batch 2 — pij-lonely-antelope, 2026-09-02

Load-aware this time: no timing claims, because I have now been wrong three times attributing latency and plan 013 owns that proof properly. Everything below is behaviour, not speed.

## `ask` is the strongest thing in the product, and it is not close

Real question I needed for plan 093, scoped:

    ask "How does the pij poller decide when to broadcast, and what timeout bounds a records read?" \
        --path 'apps/web/src/features/089-first-class-pij/**'

    grounded: true · iterations: 7 · tokens: 56,418 · stopped: answered · model z-ai/glm-5.3-flash

It returned the two loops with their real constants (FAST_LOOP_MS=2_000, SLOW_LOOP_MS=8_000), the coalescing rule (MAX_BROADCASTS_PER_FAST_TICK=1, "one coalesced fleet-delta, never one broadcast per spine line"), the reason for it (system-state dominates the spine ~100:1), and the full-rows-not-patches property with WHY it exists — so the client has no field-level merge in which to invent a value. Two citations, both the correct files.

I know this subsystem well and I could not fault the answer. More to the point: it explained the DESIGN INTENT behind the constants, which is the thing that is genuinely hard to get from reading code, and which is what an agent joining a codebase actually needs. `--path` scoping worked exactly as documented.

## FINDING (confirms batch-1 finding 3, and upgrades it from one-off to systemic)

`meta.empty_because` is absent on EVERY zero I have produced, across three different verbs, while the reason is present and correct in `next_action`:

    search --path 'nope/**'   -> empty_because ABSENT
                                 next_action: 'the --path filter "nope/**" matches zero indexed paths in this scope'
    refs <a real, indexed file> -> empty_because ABSENT
                                 next_action: 'no indexed ddoc rows reference that source path — this is a successful empty answer'

Note how GOOD that second string is — it distinguishes "nothing references it" from "I could not look", which is the exact distinction a consumer needs. The information is not missing; it is in the field your own agents guide calls "a steer, not an instruction", and absent from the field your brief names as the honesty contract. A consumer that branches on `empty_because` sees a reasonless zero every time, and a consumer that branches on `next_action` is reading a field documented as ignorable. Populate both, or correct the guide — I would populate both, since the strings already exist.

## SMALL: flag surface is inconsistent across subcommands

    conversation list --limit 3
    -> error: unexpected argument '--limit' found

`--limit` is documented and works on `search`. On `conversation list` it is a hard usage error. Not a bug in itself, but it is the kind of thing an agent burns a call on because the flag is right in the sibling verb. Either accept it or say "use --repo/--path to narrow" in the error.

## `conversation verify` is a model of the honesty contract — keep it exactly as is

    conversation verify --harness claude --session e144359d-...
    -> ok:false, FS3-E-QUERY-CONVERSATION-NOT-FOUND,
       "conversation 1e3b3563-... is not indexed",
       fix: "run flowspace3 conversation ingest for the session, then wait for the queue to drain and verify again",
       details.guid, retryable:false

Correct code, the derived guid so I can act on it, a fix that is a command, and `retryable:false` telling me not to spin. This is what I wanted `empty_because` to be. It also answered a question I had not asked: it showed me my own session is NOT ingested, which is why I have not been able to test `ask --conversation` on my own transcript.

## MY OWN TRAP, worth one line in agents-start-here because every agent will hit it

I reported an `ask` response as unparseable ("Extra data: line 45"). It was not. I had run `ask ... 2>&1 | python3 -c json.load`, merging stderr into stdout and corrupting the envelope. With the streams separated it is one clean JSON document, 17KB, parses first time.

Agents pipe `2>&1` into parsers constantly and your errors print to BOTH streams (I confirmed this morning: the same envelope on stdout and the message on stderr). That combination silently produces "your JSON is malformed" reports that are the caller's fault. One sentence in the guide — "parse stdout only; stderr carries a human copy" — would prevent a class of false bug reports from agents like me.

## Not done
`ask --conversation <guid>` on my own transcript: blocked, my session is not ingested, and I am not running an ingest against a shared daemon mid-session without being told it is fine. `get conv:<guid>#t<n>` on someone else's conversation I can do if you want it — say so and I will.
