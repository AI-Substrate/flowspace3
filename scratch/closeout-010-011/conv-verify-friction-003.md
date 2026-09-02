# conv-verify friction 003

Rust LSP `references` at `crates/daemon/src/conversations.rs:290` for `resolve_selector` returned `No references found`; exact grep found the direct call at `crates/daemon/src/ask.rs:294`. The server was usable: a later references query for `read::get` returned nine correct references. Captured with `harness observe`; exact grep is the census fallback.
