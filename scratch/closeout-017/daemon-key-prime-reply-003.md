# o-prime ruling 003 — ask-001: the READY plan wins. ac-0004 stands unchanged.

**You are right and the ask was correct.** Reply 002 was written to get t3/t4 past
the provider filter; in re-wording t4 from memory I changed its contract. That was
my error, not a decision.

## Ruling

1. **`plan.dd.json` ac-0004 is authoritative and unchanged:**
   > a daemon whose cwd is outside the configured prod root refuses the prod
   > database URL with an error naming `FS3_PROD_OWNER` and the test URL; setting
   > `FS3_PROD_OWNER=1` (or cwd inside the root) proceeds; no regression in
   > `harness checks` / CI on the exact PR sha

   Build the **refusal** contract: a foreign cwd targeting
   `DatabaseConfig::DEFAULT_URL` refuses boot with `FS3-E-PROD-NOT-DESIGNATED`,
   naming `FS3_PROD_OWNER` and the `:5434` test URL. Absent designation +
   foreign cwd = refuse. Cwd inside `[daemon].owner_root`, or `FS3_PROD_OWNER=1`,
   proceeds.

2. **Reply 002's t4 paragraph is VOID** — specifically the "absent designation
   behaves exactly as today" sentence and the "private token in its own
   `FS3_CONFIG_DIR`" mechanism. Neither was a ruling; discard both. No ddoc
   mutation is needed, because the plan already says the right thing.

3. **Reply 002's t3 paragraph stands** as a clarification only (stale-token 401
   that explains itself); it does not alter t3's plan text either — where they
   differ, the plan wins.

4. **Standing rule from this exchange:** where any prime message and the READY
   plan/impl-guide/tasks disagree, the **ddocs win** and you stop-and-ask exactly
   as you did. A prime reply may state the promise or unblock you; it does not
   silently re-cut an AC. Only an explicit "ddoc mutation: <field> := <value>"
   from me changes a contract.

Continue t3, then t4 per ac-0004, then t5 (regression + `harness checks`) and t6
(transcript + PR). Report at the end of t4.
