# CI RED on 43fca08 — your guard fired inside a test. Diagnose before you fix.

Run 33607516052, job `gate`, exit 101:

```
test the_real_binaries_agree_through_a_discovered_config ... FAILED
panicked at crates/daemon/tests/health.rs:164:13:
the daemon exited before serving http://127.0.0.1:33687/health: exit status: 1
```

**Exit status 1 with nothing served is the shape of a refusal, not a timeout** —
and it is almost certainly `refuse_undesignated_production_store`: that test spawns
a real daemon from a *discovered* config, which carries no `database.url`, so it
resolves to `DatabaseConfig::DEFAULT_URL` — the exact string your guard refuses —
with `owner_root` unset and cwd the CI checkout. Confirm that before changing
anything: get the daemon's stderr/stdout into the failure message and prove the
error code is `FS3-E-PROD-NOT-DESIGNATED` rather than assuming it.

## Constraints on the fix

1. **Do not weaken the guard.** Fail-closed against the prod URL is the whole point
   of ac-0004, and I am keeping it. A fix that lets an undesignated daemon through
   is a rejected fix.
2. The honest correction is on the **test** side: a test must never be pointed at
   the shipped prod URL by accident. Give it an explicit test database URL, or make
   the sealed-test path designate itself — you already found that the prod-owner
   refusal *masked* the older `fs3_testkit::sealed` refusal, so this is the same
   precedence question in CI shape. Whichever you choose, the assertion must still
   prove the two real binaries agree.
3. **The failure message must name the cause.** `the daemon exited before serving
   <url>: exit status: 1` told me nothing — the daemon's own refusal text existed
   and was thrown away. Fix that too: on a non-zero exit, the panic must carry the
   child's stderr. That is a permanent improvement, not scaffolding.

## Also answer this, it matters more than the fix

**Why was your local `harness checks` green when CI is red on the same sha?**
Either the local gate and CI hand tests different environments (a real divergence
we must know about — a green local gate that CI contradicts is a gate that lies),
or that test was already failing locally for the 19-registered-roots reason
(backlog 177) and its failure was attributed to that. Tell me which. Do not guess
between them — check.

Report the diagnosis, then the fix, then push. CI on the new sha is the gate.
