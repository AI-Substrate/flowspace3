# Coder done reports — mutation evidence, plan 008-ddocs-scan

Persisted at the reviewer's request. Before this file existed, each coder's named mutations lived
only in pij transcripts — verifiable by me, invisible to anyone reviewing the repository. The
reviewer's done bar requires the coder's NAMED MUTATION rather than an assertion that mutation
checking happened, and it was right that the evidence had not been persisted.

**A mutation check is: break the code, watch the NAMED test fail, restore, watch it pass.** A test
never observed failing is not evidence. Each entry below is the seat's own wording.

---

## u1 — ddoc parser and discovery admission (pij-yielding-sparrow)

Commits `d79e9bc`, `711315c`. Sealed gate green; observed cargo 1.95.0, ddocs 0.1.0, node 24.7.0.

1. Row kind `Row` → `Function`: plain tree/metadata, arbitrary-depth dynamic trail, facts=None fallback, and stored custom state tests all failed.
2. Empty-section `Section` → `Container`: no-id section test failed.
3. Added sibling position to row `raw_text`: reorder test failed **with both per-id hashes changing**.
4. Forced `is_ddoc_source` false: exact source helper, full discovery admission, and watcher `discover_subtree` tests all failed.
5. Forced generated-banner result false: banner-aware generated sibling test failed.
6. Forced fallback basis to `SchemaDeclared`: explicit fallback test failed.
7. Set `gate_terminal = Some(true)` in the parser: custom stored-state/no-judgment test failed.
8. Removed the row-terminal `return`: the deep fixture emitted the id-bearing row field as a second row and the test went red.
9. Required a `text` entry before honouring schema facts: string-only schema test failed.
10. Regressed the generated-sibling reason to `ConfigFormat`: full and subtree discovery tests failed.

## u3 — persistence, filters, inverse index (pij-shallow-boa)

Commits `c3dca20`, `181493b`. Sealed gate green; same observed toolchain.

1. `ddoc_meta_round_trips_byte_identically`: forced the ddoc bind to SQL NULL.
2. `code_element_round_trip_keeps_ddoc_null`: coalesced None to `{}` JSONB.
3. `search_hit_carries_ddoc_payload_without_a_second_query`: projected NULL instead of `e.ddoc`.
4. `search_filter_id_kinds_selects_and_none_is_noop`: forced the id-kind bind to None.
5. `search_filter_gate_open_selects_known_rows_and_excludes_unknown`: inverted the true/false mapping.
6. Same gate test, unknown case: coalesced JSON null to false so the unknown row leaked into open results.
7. `search_filter_ddoc_schema_selects_and_none_is_noop`: forced the schema bind to None.
8. `rows_referencing_without_file_edges_is_empty`: returned `StoreError::RowNotFound` for an empty result.
9. `rows_referencing_returns_seeded_rows_in_stable_order`: ordered only by insertion element id; reverse-seeded rows failed exact order.
10. `replace_file_refs_reports_unattached_without_losing_attached_edges`: converted any unattached source into a hard StoreError.
11. `migration_applies_over_existing_code_elements_and_accepts_row`: omitted `row` from the widened CHECK.
12. `migration_kind_check_rejects_unknown_kind`: replaced the CHECK with `CHECK (TRUE)`.

Follow-up (parser-generation narrowing): removed the version predicate and its bind so the query
stayed valid; the two-generation test went red with `ac-dead` from `test-parser@other` leaking into
the `test-parser@1` result.

## u2 — ddocs adapter (pij-supreme-tapir)

Commits `325e523`, `614bf2e`. Sealed gate exactly:
`FS3_CONFIG_DIR=/tmp/pij-supreme-tapir-fs3-config FS3_TEST_DATABASE_URL=…/fs3_ddocs_u2_test harness checks` → 9/9.
Observed cargo 1.95.0, ddocs 0.1.0, node 24.7.0.

Mutations, each with the named test observed red then the restored suite green: version field
lookup · absent kind default · explicit file kind · graph path normalisation · schema envelope gate ·
schema-file text classification · validate command/status/error findings · fail-open links-empty
health inference · normalised-vs-author-relative file target · file-only inverse filter · snapshot
row index · dynamic map key · unknown-rel/pressure sentinel keying · exact schema map lookup ·
missing binary/`is_absent` · generic file-only dispatch · derived-state omission and gate inversion ·
unreadable schema ⇒ empty Some · unattached finding omission.

**The finding that matters most in this plan**, in the seat's own words:

> I found and fixed one tautological missing-binary oracle. It compared
> `probe_with_binary(...missing...)` to `DdocTooling::absent()`, so mutating `absent()` changed both
> actual and expected and the test stayed green forever. The repaired oracle independently asserts
> version None, facts empty, graph None, and `is_absent()`, plus a healthy negative.

That oracle defended ac-000d. A tautological version would have been a plan-level false pass:
permanently green, occupying the slot where the real check belongs, and invisible to any suite run.

## u4 — agent-facing surface (pij-sudden-pigeon)

Commits `8b515f1`, `bb8dc63`, `83cc5ca`. Sealed gate exactly:
`FS3_CONFIG_DIR=/tmp/pij-sudden-pigeon-fs3-config FS3_TEST_DATABASE_URL=…/fs3_ddocs_u4_test harness checks` → 9/9.
Observed cargo 1.95.0, ddocs 0.1.0, node 24.7.0.

1. `positional_ddoc_addresses_parse_without_changing_existing_schemes`: inverted the minted-prefix short-form predicate.
2. `ddoc_metadata_is_serialized_on_rows_and_the_key_is_absent_on_code`: removed `skip_serializing_if`; the test caught `"ddoc": null`.
3. `ddoc_filter_mapping_preserves_absent_open_and_closed`: forced `gate_open = Some(false)`.
4. `ddoc_search_flags_become_query_params_and_absence_stays_absent`: mapped `--gate-closed` to true.
5. `get_by_dd_address_resolves_the_same_row_the_parser_produced`: looked up the file address instead of the rendered row trail.
6. `every_command_the_bundle_teaches_actually_exists`: taught a nonexistent `flowspace3 ddocs`.
7. `ddoc_page_teaches_rows_citations_and_state_truth`: removed the explicit "Believe this one" derived-state instruction.
8. `three_named_ddoc_query_shapes_have_ground_truth`: removed one manifest scenario.
9. `refs_with_no_rows_is_a_successful_empty_answer`: converted empty results into `QUERY_NOT_FOUND`.
10. `refs_returns_the_source_rows_pasteable_dd_address`: wrapped the dd citation in `el:`.
11. `ddoc_degradation_notice_uses_live_worktree_tooling`: emitted the missing-binary notice unconditionally — **the healthy half failed**, which is the half that catches the real bug.

---

## Toolchain, observed not assumed

All four seats independently reported `cargo 1.95.0 (f2d3ce0bd 2026-03-21) (Homebrew)`, `ddocs 0.1.0`,
node `24.7.0`, matching the wave-0 pin. Four units read one installed toolchain, so a mismatch
between seats would have invalidated siblings retroactively and invisibly; the receipts are how we
know it did not happen.
