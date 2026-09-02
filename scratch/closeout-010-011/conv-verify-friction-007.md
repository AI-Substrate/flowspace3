# conv-verify friction 007

The Rust LSP rename did not merely miss `convo_ingest.rs`'s intended callsite: it inserted a stray bare `QUERY_CONVERSATION_NOT_FOUND` token after `verify_seat`'s closing brace, causing a syntax error. The catalog rename landed. I removed the stray token and changed the callsite manually after exact-range inspection. Captured with `harness observe`.
