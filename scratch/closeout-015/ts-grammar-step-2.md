# ts-grammar step 2 report

## Result

T2 fixture/golden contract is checked in and deliberately RED on only the t3 mechanisms.

## Changed

- `crates/parsers/fixtures/sample.ts`: nested declarations, namespace, six function-valued bindings, `const x = 1` and `const cfg = {}` negatives, wrapper and `class_heritage` grep traps.
- `crates/parsers/fixtures/sample.tsx`: JSX-heavy clean parse with nested arrow binding.
- `crates/parsers/tests/fixture_elements.rs`: exact kind/subkind/address/parent/sibling/span goldens, deliberate non-elements, clean TSX parse, and no empty name/address invariant.
- `crates/testkit/arch-allowlist.toml`: the single o-prime-approved `tree-sitter-typescript` line.

## RED proof

`cargo test -p fs3-parsers --test fixture_elements`: 13 run, 11 passed, 2 new goldens failed. The actual/expected diff is exact: existing declarations and spans already match; missing rows are only `internal_module`, function-valued `variable_declarator`, `public_field_definition`, and namespace re-parenting. Those are precisely t3. No unexpected element was invented, and the TSX parse has no error.
