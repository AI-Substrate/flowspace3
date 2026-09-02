# conv-verify friction 006

Rust LSP rename reported that it applied one `QUERY_CONVERSATION_NOT_FOUND` edit to `convo_ingest.rs`, but exact grep/read showed the callsite still used `QUERY_CONVERSATION_NOT_INDEXED`; the two catalog edits did land. Captured with `harness observe`. I am correcting the single missed callsite manually after the required symbol-aware rename.
