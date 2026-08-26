// Throwaway POC: direct tree-sitter AST extraction for fs3.
// Answers: (1) does one generic extractor work across languages?
// (2) how fast is it? (3) does it hold up on a large real repo?
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tree_sitter::{Language, Node, Parser};
use walkdir::WalkDir;

// ---------- language registry: extension -> (name, grammar) ----------

fn language_for_ext(ext: &str) -> Option<(&'static str, Language)> {
    let l = match ext {
        "rs" => ("rust", tree_sitter_rust::LANGUAGE.into()),
        "py" | "pyi" => ("python", tree_sitter_python::LANGUAGE.into()),
        "ts" => ("typescript", tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => ("tsx", tree_sitter_typescript::LANGUAGE_TSX.into()),
        "js" | "mjs" | "cjs" | "jsx" => ("javascript", tree_sitter_javascript::LANGUAGE.into()),
        "cs" => ("csharp", tree_sitter_c_sharp::LANGUAGE.into()),
        "go" => ("go", tree_sitter_go::LANGUAGE.into()),
        "md" | "markdown" => ("markdown", tree_sitter_md::LANGUAGE.into()),
        "java" => ("java", tree_sitter_java::LANGUAGE.into()),
        "c" | "h" => ("c", tree_sitter_c::LANGUAGE.into()),
        "cpp" | "cc" | "cxx" | "hpp" => ("cpp", tree_sitter_cpp::LANGUAGE.into()),
        "rb" => ("ruby", tree_sitter_ruby::LANGUAGE.into()),
        "sh" | "bash" => ("bash", tree_sitter_bash::LANGUAGE.into()),
        "json" => ("json", tree_sitter_json::LANGUAGE.into()),
        "tf" | "hcl" => ("hcl", tree_sitter_hcl::LANGUAGE.into()),
        "html" => ("html", tree_sitter_html::LANGUAGE.into()),
        "css" => ("css", tree_sitter_css::LANGUAGE.into()),
        "yml" | "yaml" => ("yaml", tree_sitter_yaml::LANGUAGE.into()),
        "toml" => ("toml", tree_sitter_toml_ng::LANGUAGE.into()),
        _ => return None,
    };
    Some(l)
}

// ---------- universal classification (port of fs2 classify_node) ----------

fn classify(ts_kind: &str) -> &'static str {
    const ROOTS: [&str; 8] = [
        "module", "program", "source_file", "document", "compilation_unit",
        "translation_unit", "config_file", "stream",
    ];
    if ROOTS.contains(&ts_kind) {
        return "file";
    }
    if ts_kind.ends_with("_instruction") {
        return "block";
    }
    if ts_kind == "block" || ts_kind.ends_with("_block") {
        return "block";
    }
    for x in ["function", "method", "lambda", "procedure", "constructor"] {
        if ts_kind.contains(x) {
            return "callable";
        }
    }
    for x in ["class", "struct", "interface", "enum", "type_alias", "trait", "impl", "record"] {
        if ts_kind.contains(x) {
            return "type";
        }
    }
    if ts_kind.contains("heading") {
        return "section";
    }
    if ts_kind == "type_spec" || ts_kind == "type_declaration" {
        return "type";
    }
    if ts_kind.ends_with("_statement") {
        return "statement";
    }
    if ts_kind.ends_with("_expression") {
        return "expression";
    }
    for s in ["_definition", "_declaration", "_item", "_specifier"] {
        if ts_kind.ends_with(s) {
            return "definition";
        }
    }
    "other"
}

// Categories fs3 treats as first-class elements.
fn is_element(cat: &str) -> bool {
    matches!(cat, "callable" | "type" | "section")
}

// REFINEMENT (learned from the naive pass): substring classification alone is
// too eager — `struct_expression`, `interface_body`, `class_body` all match.
// An element must ALSO be declaration-shaped. Still zero per-language code.
const DECL_SUFFIXES: [&str; 5] = ["_item", "_declaration", "_definition", "_signature", "_spec"];

// Some grammars (Ruby, GDScript…) name declarations with a bare word, not a suffix.
const BARE_DECLS: [&str; 6] = ["method", "singleton_method", "class", "module", "def", "function"];

fn is_declaration_shaped(kind: &str, cat: &str) -> bool {
    // TS_NAIVE=1 reproduces the first-cut classifier (fs2 classify_node ported
    // verbatim, no declaration gate) so the before/after is re-runnable.
    if std::env::var("TS_NAIVE").is_ok() {
        return true;
    }
    if BARE_DECLS.contains(&kind) {
        return true;
    }
    match cat {
        // callables are only ever declared by a declaration-shaped node — never by
        // a C/C++ *_declarator or *_specifier (those are parts of one)
        "callable" => DECL_SUFFIXES.iter().any(|s| kind.ends_with(s)),
        // types additionally arrive as C/C++ class_specifier / struct_specifier / enum_specifier
        "type" => DECL_SUFFIXES.iter().any(|s| kind.ends_with(s)) || kind.ends_with("_specifier"),
        "section" => kind.contains("heading"),
        _ => false,
    }
}

// Namespaces/modules/packages scope names but are not elements themselves.
fn is_container_only(kind: &str) -> bool {
    if std::env::var("TS_NAIVE").is_ok() {
        return false;
    }
    kind.contains("namespace")
        || kind == "mod_item"
        || kind == "module"
        || kind == "package_declaration"
        || kind == "type_declaration" // Go: wraps the named type_spec
}

#[derive(Debug, Clone)]
struct Element {
    ts_kind: String,
    category: &'static str,
    name: Option<String>,
    qualified_name: String,
    start_line: usize,
    end_line: usize,
}

// ---------- name derivation (generic, no per-language code) ----------

fn node_name(node: Node, src: &[u8]) -> Option<String> {
    for field in ["name", "declarator", "path", "pattern", "type"] {
        if let Some(c) = node.child_by_field_name(field) {
            let t = c.utf8_text(src).ok()?;
            // declarators can be whole signatures; take the leading identifier-ish run
            let t = t.trim();
            let cut = t
                .find(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == ':' || ch == '.'))
                .unwrap_or(t.len());
            let t = if cut == 0 { t } else { &t[..cut] };
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    // fallback: first named identifier child
    let mut cur = node.walk();
    for c in node.named_children(&mut cur) {
        if c.kind().contains("identifier") {
            if let Ok(t) = c.utf8_text(src) {
                return Some(t.to_string());
            }
        }
    }
    None
}


// JS/TS (and Python lambdas, Ruby procs…) name their callables via the binding
// they are assigned to: `const handleClick = () => {}`. One generic rule — a
// callable that is the value of a named binding inherits that binding's name.
fn bound_callable_name(node: Node, src: &[u8]) -> Option<String> {
    if std::env::var("TS_NAIVE").is_ok() {
        return None;
    }
    if classify(node.kind()) != "callable" || is_declaration_shaped(node.kind(), "callable") {
        return None;
    }
    let p = node.parent()?;
    if !(p.kind().contains("declarator") || p.kind().contains("assignment") || p.kind() == "pair") {
        return None;
    }
    for f in ["name", "left", "key"] {
        if let Some(c) = p.child_by_field_name(f) {
            if let Ok(t) = c.utf8_text(src) {
                if !t.trim().is_empty() {
                    return Some(t.trim().to_string());
                }
            }
        }
    }
    None
}

// ---------- extraction ----------

fn extract(lang_name: &str, src: &[u8], tree: &tree_sitter::Tree) -> Vec<Element> {
    if lang_name == "markdown" {
        return extract_markdown(src, tree);
    }
    let mut out = Vec::new();
    walk(tree.root_node(), src, &mut Vec::new(), &mut out);
    out
}

fn walk(node: Node, src: &[u8], stack: &mut Vec<String>, out: &mut Vec<Element>) {
    let cat = classify(node.kind());
    let mut pushed = false;
    if node.is_named() && is_container_only(node.kind()) {
        if let Some(n) = node_name(node, src) {
            stack.push(n);
            pushed = true;
        }
    } else if is_element(cat)
        && node.is_named()
        && (is_declaration_shaped(node.kind(), cat) || bound_callable_name(node, src).is_some())
    {
        let name = node_name(node, src).or_else(|| bound_callable_name(node, src));
        // a receiver (Go methods) scopes the name onto its type
        let recv = node
            .child_by_field_name("receiver")
            .and_then(|r| r.utf8_text(src).ok())
            .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric() && c != '_').to_string())
            .and_then(|t| t.rsplit(' ').next().map(|s| s.trim_start_matches(['*', '&']).to_string()))
            .filter(|t| !t.is_empty());
        let seg = name.clone().unwrap_or_else(|| format!("<anon@{}>", node.start_position().row + 1));
        let seg = match recv {
            Some(r) => format!("{r}.{seg}"),
            None => seg,
        };
        let mut parts = stack.clone();
        parts.push(seg.clone());
        out.push(Element {
            ts_kind: node.kind().to_string(),
            category: cat,
            name,
            qualified_name: parts.join("."),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
        });
        stack.push(seg);
        pushed = true;
    }
    let mut cur = node.walk();
    for c in node.named_children(&mut cur) {
        walk(c, src, stack, out);
    }
    if pushed {
        stack.pop();
    }
}

// Markdown: headings are point nodes, so sections are synthesised —
// each heading owns everything until the next heading of <= its level.
fn extract_markdown(src: &[u8], tree: &tree_sitter::Tree) -> Vec<Element> {
    let total_lines = tree.root_node().end_position().row + 1;
    let mut heads: Vec<(usize, usize, String, String)> = Vec::new(); // level, line, text, ts_kind
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.kind().contains("heading") && n.is_named() {
            let level = if n.kind().starts_with("atx") {
                // atx_h1_marker .. atx_h6_marker
                let mut c = n.walk();
                n.children(&mut c)
                    .find(|k| k.kind().starts_with("atx_h"))
                    .and_then(|k| k.kind().chars().nth(5).and_then(|d| d.to_digit(10)))
                    .unwrap_or(1) as usize
            } else {
                1
            };
            let text = n
                .utf8_text(src)
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .trim_start_matches('#')
                .trim()
                .to_string();
            heads.push((level, n.start_position().row + 1, text, n.kind().to_string()));
        }
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            stack.push(ch);
        }
    }
    heads.sort_by_key(|h| h.1);
    let mut out = Vec::new();
    let mut path: Vec<(usize, String)> = Vec::new();
    for (i, (level, line, text, kind)) in heads.iter().enumerate() {
        let end = heads[i + 1..]
            .iter()
            .find(|(l, ..)| l <= level)
            .map(|(_, ln, ..)| ln - 1)
            .unwrap_or(total_lines);
        while path.last().map(|(l, _)| l >= level).unwrap_or(false) {
            path.pop();
        }
        let qn = path
            .iter()
            .map(|(_, t)| t.as_str())
            .chain(std::iter::once(text.as_str()))
            .collect::<Vec<_>>()
            .join(" > ");
        out.push(Element {
            ts_kind: kind.clone(),
            category: "section",
            name: Some(text.clone()),
            qualified_name: qn,
            start_line: *line,
            end_line: end,
        });
        path.push((*level, text.clone()));
    }
    out
}

// ---------- per-file driver ----------

struct FileResult {
    path: PathBuf,
    lang: &'static str,
    bytes: usize,
    elements: Vec<Element>,
    parse_us: u128,
    extract_us: u128,
    has_error: bool,
    skipped: Option<&'static str>,
}

fn parse_file(path: &Path) -> Option<FileResult> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    let (lang_name, lang) = language_for_ext(&ext)?;
    let src = std::fs::read(path).ok()?;
    if std::str::from_utf8(&src).is_err() {
        return Some(FileResult {
            path: path.into(), lang: lang_name, bytes: src.len(), elements: vec![],
            parse_us: 0, extract_us: 0, has_error: false, skipped: Some("non-utf8"),
        });
    }
    let mut parser = Parser::new();
    if parser.set_language(&lang).is_err() {
        return Some(FileResult {
            path: path.into(), lang: lang_name, bytes: src.len(), elements: vec![],
            parse_us: 0, extract_us: 0, has_error: false, skipped: Some("abi-mismatch"),
        });
    }
    let t0 = Instant::now();
    let tree = parser.parse(&src, None)?;
    let parse_us = t0.elapsed().as_micros();
    let t1 = Instant::now();
    let elements = extract(lang_name, &src, &tree);
    let extract_us = t1.elapsed().as_micros();
    Some(FileResult {
        path: path.into(), lang: lang_name, bytes: src.len(), elements,
        parse_us, extract_us, has_error: tree.root_node().has_error(), skipped: None,
    })
}

// Two knobs the large-repo pass needed:
//   TS_GIT=1        — only git-tracked files (walkdir sees scratch/, worktrees/, node_modules)
//   TS_EXTS=rs,ts   — restrict to these extensions (isolate source from data blobs)
fn collect_files(root: &Path) -> Vec<PathBuf> {
    let only: Option<Vec<String>> = std::env::var("TS_EXTS")
        .ok()
        .map(|v| v.split(',').map(|s| s.trim().to_lowercase()).collect());
    let keep = |p: &PathBuf| -> bool {
        match p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) {
            Some(e) => {
                language_for_ext(&e).is_some() && only.as_ref().is_none_or(|o| o.contains(&e))
            }
            None => false,
        }
    };
    if std::env::var("TS_GIT").is_ok() {
        let out = std::process::Command::new("git")
            .args(["-C", root.to_str().unwrap(), "ls-files", "-z"])
            .output()
            .expect("git ls-files");
        return String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(|s| root.join(s))
            .filter(&keep)
            .collect();
    }
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let n = e.file_name().to_string_lossy();
            !(e.depth() > 0 && (n == ".git" || n == "node_modules" || n == "target" || n == ".venv"))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(keep)
        .collect()
}

// ---------- reporting ----------

fn print_table(r: &FileResult) {
    println!("\n### {} [{}]  {} bytes  parse {}µs  extract {}µs  error={}",
        r.path.display(), r.lang, r.bytes, r.parse_us, r.extract_us, r.has_error);
    if r.elements.is_empty() {
        println!("  (no elements)");
        return;
    }
    println!("  {:<28} {:<10} {:>9}  {}", "ts_kind", "category", "lines", "qualified_name");
    println!("  {:-<28} {:-<10} {:->9}  {:-<40}", "", "", "", "");
    for e in &r.elements {
        println!("  {:<28} {:<10} {:>4}-{:<4}  {}",
            e.ts_kind, e.category, e.start_line, e.end_line, e.qualified_name);
    }
}

fn summarize(results: &[FileResult], label: &str, wall: f64) {
    let files = results.len();
    let bytes: usize = results.iter().map(|r| r.bytes).sum();
    let elems: usize = results.iter().map(|r| r.elements.len()).sum();
    let errs = results.iter().filter(|r| r.has_error).count();
    let skipped = results.iter().filter(|r| r.skipped.is_some()).count();
    println!("\n== {label} ==");
    println!("files={files} bytes={:.2}MB elements={elems} error_trees={errs} skipped={skipped}",
        bytes as f64 / 1e6);
    println!("wall={:.3}s  {:.0} files/s  {:.2} MB/s", wall,
        files as f64 / wall, (bytes as f64 / 1e6) / wall);

    let mut per_lang: BTreeMap<&str, (usize, usize, usize, u128)> = BTreeMap::new();
    for r in results {
        let e = per_lang.entry(r.lang).or_default();
        e.0 += 1;
        e.1 += r.elements.len();
        e.2 += r.bytes;
        e.3 += r.parse_us + r.extract_us;
    }
    println!("\n| language | files | elements | MB | cpu-ms | µs/file |");
    println!("|---|---:|---:|---:|---:|---:|");
    for (l, (f, el, b, us)) in &per_lang {
        println!("| {l} | {f} | {el} | {:.2} | {:.1} | {} |",
            *b as f64 / 1e6, *us as f64 / 1000.0, us / (*f as u128).max(1));
    }
    let mut slow: Vec<_> = results.iter().collect();
    slow.sort_by_key(|r| std::cmp::Reverse(r.parse_us + r.extract_us));
    println!("\nslowest files:");
    for r in slow.iter().take(5) {
        println!("  {:>8}µs  {:>9} bytes  {}", r.parse_us + r.extract_us, r.bytes, r.path.display());
    }
    if errs > 0 {
        println!("\nfiles with ERROR nodes (first 10):");
        for r in results.iter().filter(|r| r.has_error).take(10) {
            println!("  [{}] {}", r.lang, r.path.display());
        }
    }
    if skipped > 0 {
        println!("\nskipped (first 10):");
        for r in results.iter().filter(|r| r.skipped.is_some()).take(10) {
            println!("  [{}] {} — {}", r.lang, r.path.display(), r.skipped.unwrap());
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    match mode {
        // dump element tables for the given files
        "file" => {
            for p in &args[2..] {
                match parse_file(Path::new(p)) {
                    Some(r) => print_table(&r),
                    None => println!("\n### {p} — UNSUPPORTED (no grammar for extension)"),
                }
            }
        }
        // walk a tree, dump every file's table
        "dump" => {
            for p in collect_files(Path::new(&args[2])) {
                if let Some(r) = parse_file(&p) {
                    print_table(&r);
                }
            }
        }
        // timing pass: single-thread then rayon
        "bench" => {
            let root = Path::new(&args[2]);
            let files = collect_files(root);
            println!("root: {}  candidate files: {}", root.display(), files.len());
            // warm page cache + grammar init so the single-thread pass is not
            // charged for cold I/O the parallel pass would never pay
            let _: Vec<_> = files.par_iter().filter_map(|p| parse_file(p)).collect();

            let t = Instant::now();
            let single: Vec<_> = files.iter().filter_map(|p| parse_file(p)).collect();
            let wall_s = t.elapsed().as_secs_f64();

            let t = Instant::now();
            let par: Vec<_> = files.par_iter().filter_map(|p| parse_file(p)).collect();
            let wall_p = t.elapsed().as_secs_f64();

            summarize(&single, "single-thread", wall_s);
            println!("\n-- parallel (rayon, {} threads) --", rayon::current_num_threads());
            println!("wall={:.3}s  {:.0} files/s  {:.2} MB/s  speedup={:.2}x",
                wall_p, par.len() as f64 / wall_p,
                (par.iter().map(|r| r.bytes).sum::<usize>() as f64 / 1e6) / wall_p,
                wall_s / wall_p);
            assert_eq!(
                single.iter().map(|r| r.elements.len()).sum::<usize>(),
                par.iter().map(|r| r.elements.len()).sum::<usize>(),
                "parallel and single-thread disagree on element count"
            );
            println!("determinism check: parallel element count == single-thread ✔");
        }
        _ => println!("usage: treesitter file <paths...> | dump <dir> | bench <dir>"),
    }
}
