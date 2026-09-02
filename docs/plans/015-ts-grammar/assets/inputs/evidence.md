147. **P1 — TypeScript symbol extraction produces NOTHING repo-wide:
    every .ts file is a bare file element with `children: []`** (alpaca,
    same batch; VERIFIED by o-prime read-only on prod: elements joined
    to .ts paths — chainglass 11,286 file elements / 0 non-file;
    harness-engineering 7,237 / 0; pij 5,893 / 0). Consequences, all
    silent: `tree <file.ts>` → 48 s to return an honest-looking
    `entries: [], total: 0`; `refs <symbol>` → 0 for symbols with 5+
    references; and this is the real mechanism behind antelope's
    finding 1 ("doc-heavy repo: unscoped search never surfaces the
    source file") — TS code has no element granularity, so document
    sections dominate the candidate pool. Either the TS grammar is not
    wired in the parser set, or extraction regressed; either way three
    TypeScript repos in the index have no code symbols. ENCODE: (a) fix
    extraction (tree-sitter-typescript exists; the add-language skill is
    the recipe); (b) `tree`/`refs`/`get` on a file with zero children
    must say "no symbols extracted for .ts (no parser / parser error)"
    — row 136(a) generalised; (c) a doctor row: languages present in
    the index by file count vs languages with a parser. Row 136 family;
    supersedes it in priority.

    Also from the batch (goods): verify 0.02 s honest negative; `get
    conv:#t --repo all` reliable throughout the backfill; 101
    conversations survived the outage intact.
    ROW 147 ROOT CAUSE (o-prime, source read): NOT a regression — only
    THREE grammars are wired in crates/parsers (tree_sitter_md,
    tree_sitter_python, tree_sitter_rust). discovery.rs:132 lists ts/
    tsx/swift/sql/svelte/vue/zig/… as DISCOVERABLE extensions, so those
    files are indexed as bare file elements and never parsed. Every
    non-Rust/Python codebase in the index — three TypeScript governments,
    the C# game (row 136) — has no symbol granularity: no tree, no refs,

## Code facts (o-prime, 2026-09-02, main 57b25df)
- `crates/parsers/src/lib.rs` `Language::for_extension`: only `rs`, `py|pyi`, `md|markdown` map to a grammar; everything else is a bare file element. The doc comment says: "Adding a language is this one line plus a grammar crate; the extraction walk is generic (PRD req 21)."
- `crates/parsers/src/discovery.rs:130-132` already lists `ts`, `tsx`, `js`, `jsx`, `mjs` as discoverable extensions — they are scanned but never parsed.
- `crates/core/src/classify.rs` is heuristic (hint substrings + declaration suffixes). Against tree-sitter-typescript node kinds: `function_declaration`, `class_declaration`, `abstract_class_declaration`, `interface_declaration`, `enum_declaration`, `type_alias_declaration`, `method_definition`, `method_signature`, `function_signature` CLASSIFY today. NOT classified: `internal_module` (namespace X {} — contains "mod" but no declaration suffix), `variable_declarator` (so `const f = () => {}` and `export const handler = async () => {}` — the dominant TS style — produce NO element), `public_field_definition` (class fields incl. arrow-fn members), `export_statement` (a wrapper; spliced through correctly).
- Crate: `tree-sitter-typescript 0.23.2` exposes `LANGUAGE_TYPESCRIPT` and `LANGUAGE_TSX`; `cargo add --dry-run` resolves against the workspace's tree-sitter 0.26 (a real build is the proof).
