# hidden-dirs coder acknowledgement

Worktree root: `/Users/jordanknight/substrate/flowspace/fs3-hidden-dirs`
Branch/base: `016-hidden-dirs` from `82f60ec`

## Read of the unit

This is plumbing, not a discovery-mechanism rewrite. `DiscoverySettings.include_hidden` already exists, defaults false, and controls the walker's hidden filtering; the missing path is a per-worktree persisted option carried from `add`/`POST /roots` into both root scans and watcher rescans. Evidence: `assets/inputs/evidence.md:22-25`, `impl-guide.dd.md:19-21`.

The operator-visible failure is measured, not hypothetical: the pij checkout has 563 TypeScript files under `.pi/`, but the indexed worktree has zero `.pi/%` rows; only 100 non-dot-directory TypeScript paths are indexed. Evidence: `assets/inputs/evidence.md:1-12,22-25`.

Scope is the packet fence only. Migration `0024` is additive; I will not edit `0023`, embeddings/search paths, production `:7373`/`:5433`, governance, or flow state. Tests and the real-usage receipt use only `FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test` and a scratch daemon on a free non-7373 port.

## Numbered implementation plan

1. **Persist the tri-state request correctly.** Add migration `0024` with `worktrees.include_hidden BOOLEAN NOT NULL DEFAULT FALSE`; extend the worktree store read/write model; carry `include_hidden: Option<bool>` through `POST /roots`; expose the resolved bool in `RootReport`; add CLI `--include-hidden` and `--no-include-hidden`. `None` must preserve an existing true value, while explicit false clears it. Prove new-root, re-add-without-reset, explicit-disable, and column-write mutation behavior. Evidence: `plan.dd.md:55-58`; `tasks.dd.md:38,48-52`; `backpressure.dd.md:23`.
2. **Thread the resolved setting through every discovery entry.** At initial/re-add scanning and watcher rescans, compute `global_scan.include_hidden || worktree.include_hidden`. Ensure a runtime re-add change reaches the active watcher by re-reading worktree state for rescans or by a ruled restart path; never leave watcher settings stale. Evidence: `impl-guide.dd.md:19-21,73-76`; `tasks.dd.md:39,54-58`.
3. **Preserve hard exclusions.** Add opted-in/default fixture coverage for `.hidden/a.ts` and `src/b.ts`, while asserting `.git`, `node_modules`, and `.venv` remain excluded under hidden opt-in. Mutation checks independently remove the per-root contribution at each discovery call site and must turn the corresponding test red. Evidence: `plan.dd.md:57-60`; `impl-guide.dd.md:66,73-77`; `backpressure.dd.md:24`.
4. **Make exclusions and configuration observable.** Add real skip-ledger reason `hidden` with a measured count for default discovery, and surface `include_hidden` in status JSON and tree output without changing existing fields. Mutation removal of the reason must fail the ledger assertion. Evidence: `plan.dd.md:38-39,59`; `tasks.dd.md:40,60-64`; `backpressure.dd.md:25`.
5. **Keep deterministic-document progress live through the global CLI.** Before and after each task, mutate `tasks.dd.json`, plan/backpressure receipts, and generated siblings only via global `ddocs`; never hand-edit `.dd.json` or `.dd.md`. Append the execution log with command/result and mutation receipts after each task.
6. **Run scoped proof only against the dedicated test postmaster.** Use one cargo invocation at a time with the mandated `FS3_TEST_DATABASE_URL`; prove store, roots, watcher, discovery, CLI/status/tree contracts, then request the exclusive `harness checks` slot from o-prime. A red mutation tripwire is a stop-and-report, not a failure to route around. Evidence: `backpressure.dd.md:23-27`; `packet-coder.dd.md:35-43,69-77`.
7. **Take the real-usage receipt and ship the unit.** Start a scratch daemon backed by `:5434` on a free port; demonstrate default add has no `.pi/` result, opt-in add scans at least 500 `.pi/**/*.ts` files, and search returns a named `.pi/extensions/pij` function. Stop the daemon, commit with `harness commit`, open but never merge the PR, and send o-prime the report with exact commands, evidence, deviations, and assumptions. Evidence: `plan.dd.md:40,61`; `tasks.dd.md:42,72-76`; `backpressure.dd.md:27`.

## Ruling requests / packet corrections

1. `packet-coder.dd.md:i10` and `i11` remain unfilled placeholders. I will treat the five `dw-310x` assertions as tripwires and the packet fence plus cited implementation files as the reads declaration unless ruled otherwise.
2. The original evidence proposes hidden-by-default (`assets/inputs/evidence.md:13-15`), but the approved plan explicitly keeps hidden off by default and requires per-root opt-in (`plan.dd.md:34-49`). I will implement the approved plan.
3. `impl-guide.dd.md:57` calls prod-after-bounce the integration proof, while AC-0005 and the direct packet require the coder receipt on a scratch TEST daemon. I will take the TEST receipt only; o-prime owns the later prod bounce/receipt.
4. Pre-code harness state is degraded: `harness boot --json` built successfully but reports compose service `db` stopped; this packet mandates the separate `flowspace3-db-test` service. `flowspace3 doctor --json` reports prod read-only health OK, while `flowspace3 status --json` carries an unrelated historical `FS3-E-SCAN-UNPARSEABLE` for plan 014. Both were captured as harness observations. Rust LSP is configured and available.
5. Flowspace dogfood found the relevant watcher tests and plan rows, but ddoc hits repeated `address-target-untracked` warnings for in-plan relative links. Captured as harness confusion; it does not change the implementation plan.

No code will be changed before o-prime's ruling.
