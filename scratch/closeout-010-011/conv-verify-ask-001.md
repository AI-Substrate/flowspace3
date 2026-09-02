# conv-verify stop-and-ask 001

`harness checks` reached the full test suite and failed only `fs3-core --test error_codes`: new approved catalog code `FS3-E-QUERY-CONVERSATION-NOT-INDEXED` has no generated row in `docs/reference/error-codes.md`. The test prescribes:

```bash
FS3_UPDATE_DOCS=1 cargo test -p fs3-core --test error_codes
```

That generated reference file was not named in reply-002's expanded fence. Request approval to add `docs/reference/error-codes.md`, run the prescribed regeneration, then rerun the specific test and serialized harness gate. No workaround or suppression proposed.
