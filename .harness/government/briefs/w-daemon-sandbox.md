# w-daemon-sandbox — `flowspace3 daemon --sandbox` (promote the isolation the test tier already has)

Born from DL-004 (2026-08-28, leopon + louse spend incident): two seats booted
daemons against flowspace3_test — which held 15 real registered roots and a
6,520-job backlog — with ambient ~/.config/flowspace3 selecting AZURE
providers. 150 summaries + 2,475 vectors were purchased in 15 minutes before
both were stopped. Louse caught it from the right signal: a fast summarize
call is cached, a slow one is a provider. Prime deliberately neutralised
flowspace3_test the same hour (jobs/repos/worktrees cleared, verified 0).

## The sharpened finding (knobbler, via leopon)

The primitive ALREADY EXISTS: `support::FreshDatabase`
(crates/daemon/tests/support — used by schema_skew.rs, oversize.rs,
read_surface.rs) mints a unique migrated child database per test, destroys it
after, and wires fake providers in-process. The repo solved isolation
correctly for the TEST tier and left two gaps:

1. It lives in daemon test support, so no other crate can reach it.
2. Nothing offers it to a HAND-RUN daemon — the tier where all three seats
   got burned. Two seats reinventing a worse version of an existing primitive
   within minutes is the strongest argument for promotion.

## Scope

1. Promote FreshDatabase into fs3-testkit (shared, any crate's tests).
2. `flowspace3 daemon --sandbox`: mint a unique database (created + migrated),
   FORCE fake providers regardless of ambient config, pick a free port, print
   all three facts in the boot line, drop the database on clean exit (and name
   the leftover on unclean exit so it is findable).
3. Doctor/boot line must make the mode unmistakable: `sandbox=true
   embedder=fake summarizer=fake db=<minted-name> port=<n>`.
4. Docs: the isolation recipe section replaced by "use --sandbox"; the manual
   four-override incantation demoted to an appendix.
5. THIRD POSTURE (added 2026-08-28 from CONF-004, flea): `--sandbox` variants
   or a sibling flag for REAL providers + REAL READ-ONLY index with the write
   path disabled (no add/scan/enrich jobs run), so chat-only verbs (ask) can
   be proven live with zero risk of buying embeddings/summaries. First light
   for every future LLM verb wants exactly this shape. Coder rules the flag
   surface (--sandbox=fake|read-live or two flags) with reasoning.

## Interim rule (binding fleet-wide until this ships)

Any hand-booted daemon for testing: unique per-seat database (created, used,
dropped) + FS3_CONFIG_DIR at an EMPTY directory (fake providers) + unique
port + in-tree log dir + VERIFY the boot line says embedder=fake before
letting it run. Never boot any daemon against flowspace3_test.
