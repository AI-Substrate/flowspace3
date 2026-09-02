# PR-open handoff — embed-cap-heal

- PR: https://github.com/AI-Substrate/flowspace3/pull/92
- branch: `010-embed-cap-heal`
- final head: `b07d23e46781437d771bb903457a35156740af2e`
- implementation commit: `6377a1fe4b14bc27b7894bd3a997724a87763b7f`
- local gate: `harness checks` status `ok`
- final GitHub gate: passed in 4m32s
- focused proof: provider 4/4; chunk_plan 6/6 with 7→10, 33→50, 1→2; oversize 12/12; mutation red 0/1 then restored green
- adapter behavior: OpenAI and Azure parse valid `input[N]`; absent/invalid index uses daemon bisection
- production pre-bounce: embed failed=5; jobs 1316706/1323215 duplicate the `043365…` key and collide on `jobs_live_dedupe_idx` unless repaired first
- production state: frozen by o-prime after a sibling gate tripped the production-database guard; waiting for Jordan-approved o-prime repair, merge, and bounce
- conversation ingest: this coder is rs-resident and `harness commit` reported session identity unresolvable; both commits were direct-verified.
