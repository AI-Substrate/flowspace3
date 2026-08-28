# deterministic documents

A deterministic document (`*.dd.json`) is indexed as addressable rows. A search hit with
`kind: "row"` carries a `ddoc` object describing the row: its schema, section, permanent
id, parent trail, typed relationships, validation findings, and completion state.

```bash
flowspace3 search "which acceptance criteria are still open" --id-kind ac --gate-open
```

The hit's top-level `address` is dd's full positional address, for example:

```text
docs/plans/008-ddocs-scan/plan.dd.json#acceptance_criteria/ac-0001
```

Pass that address through unchanged. It resolves the same indexed row with
`flowspace3 get`, and it pastes directly into `ddocs get` when you need dd's source
value. Do not add `el:` or remove the section segment.

## State: believe the derived claim

A row hit deliberately reports two different claims:

- `ddoc.state_derived` is computed from the row's assertions. **Believe this one when
  it is present.** `complete: false` also names the outstanding ids in `incomplete`.
- `ddoc.state_stored` is only the value written in the source row. It is labelled
  `stored` because it may disagree with the assertions.

`ddoc.gate_terminal` says whether the stored state belongs to the schema's terminal
set. `null` means unknown, not closed. Accordingly, `--gate-open` selects rows known
to be non-terminal, `--gate-closed` selects rows known to be terminal, and rows with
unknown gate state appear in neither filtered result. With neither flag, no gate
filter is applied.

If a row is indexed before ddocs enrichment is available, relationships may be empty
and derived state and gate membership may be absent. The row remains a valid search
answer; the response's `next_action` identifies the missing enrichment.

## Filters

- `--id-kind ac` selects the raw minted-id prefix without treating the prefix set as
  closed.
- `--ddoc-schema builder/plan` matches the document's declared schema verbatim.
- `--gate-open` and `--gate-closed` preserve the known-open / known-closed / unknown
  distinction described above.

Use `flowspace3 docs get search` for ranking and general query behavior, and
`flowspace3 docs get read` for address scoping.
