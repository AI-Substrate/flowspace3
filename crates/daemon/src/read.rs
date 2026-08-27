//! The read surface: fetch by address (`get`) and browse structure (`tree`).
//!
//! Search answers "what is nearest to this question" and returns lean rows;
//! this module answers "what is AT this address" and returns depth (workshop
//! 003 D4). The two halves are meant to be used together — search to find an
//! address, `get` to read it — which is why an agent no longer has to shell out
//! to `cat` to see the code fs3 just found for it.
//!
//! # Where content comes from
//!
//! A whole file is served WHOLE, out of the file-root element's `raw_text`. The
//! scanner stores the entire source on that root (`fs3_parsers`'s file
//! element), and migration 0004 keeps `raw_text` inline, so nothing has to be
//! stitched back together out of children — a reconstruction would be a second,
//! quietly different copy of the file with the gaps between declarations
//! missing. A named element is served the same way, from its own row.
//!
//! # Two ways an address is legitimately not one thing
//!
//! * `struct Rect` and `impl Rect` are two elements at ONE address (workshop
//!   002: `(address, span_start)` identifies an element, not `address` alone),
//!   so `get` can be ambiguous with nothing wrong anywhere. `--span <line>`
//!   picks one, and the ambiguity error lists every candidate with its span.
//! * one repo-relative path exists in every checkout that holds it. When those
//!   checkouts disagree about the bytes, the caller's own directory decides;
//!   when that cannot decide either, the candidates are reported rather than
//!   one of them being picked silently.
//!
//! # Parser versions
//!
//! Elements are keyed by `(blob_sha, parser_version)`, so the instant
//! `PARSER_VERSION` is bumped the current version has no rows until a re-scan.
//! Reading only the current version would 404 every address in the index during
//! that window — a silent cliff after an upgrade. So the current version is
//! preferred and the most recently written one is the fallback, and
//! `meta.parser_version` always says which answered.

use std::collections::BTreeMap;

use fs3_core::catalog;
use fs3_core::element::Element;
use fs3_core::envelope::Failure;
use fs3_core::{Address, ElementParts};
use fs3_store::IndexedFile;
use serde::{Deserialize, Serialize};

use crate::runner::fail;
use crate::scan::PARSER_VERSION;
use crate::scope::Scope;
use crate::wiring::AppState;

/// How many children deep `get` outlines by default.
const DEFAULT_GET_DEPTH: u32 = 1;

/// How many levels `tree` shows by default.
const DEFAULT_TREE_DEPTH: u32 = 2;

/// The deepest either verb will walk.
///
/// A ceiling rather than a policy: depth is bounded by a source file's own
/// nesting, and a caller asking for a hundred levels is asking for the whole
/// tree, which `tree` on the file gives them anyway.
const MAX_DEPTH: u32 = 20;

/// How many files `tree` lists before it starts counting instead.
const DEFAULT_TREE_LIMIT: i64 = 500;

/// The largest listing `tree` will build.
const MAX_TREE_LIMIT: i64 = 5_000;

/// How many nearby names an error carries.
///
/// Enough to recognise the thing you meant, few enough that a failure stays
/// readable: a 404 that pastes a thousand paths is a second problem.
const NEARBY: usize = 20;

/// What `GET /get` was asked for.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct GetRequest {
    /// `el:<repo>/<path>::<name>` — or `conv:<guid>`, once conversations exist.
    pub address: String,
    /// How many levels of children to outline. Default 1.
    #[serde(default)]
    pub depth: Option<u32>,
    /// The first line of the element to pick, when several share an address.
    #[serde(default)]
    pub span: Option<u32>,
    /// Restrict a repo-less address to one repository identity, or `all`.
    #[serde(default)]
    pub repo: Option<String>,
    /// The caller's working directory (workshop 003 D6).
    #[serde(default)]
    pub cwd: Option<String>,
}

/// One element, with everything the store knows about it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GetResult {
    /// The canonical address of what was returned.
    pub address: String,
    /// The repository it was read from, when a live path holds it.
    pub repo: Option<String>,
    /// The file it lives in, relative to its worktree root.
    pub path: String,
    /// The worktree root, so a caller can open the file on disk.
    pub root_path: Option<String>,
    /// The element's universal category.
    pub kind: String,
    /// The grammar's own kind, or the language for a whole file.
    pub subkind: String,
    /// The declaration's own name.
    pub name: String,
    /// Inclusive 1-based `[start, end]`.
    pub span: [u32; 2],
    /// The element's exact source — the whole file when the address named one.
    pub raw_text: String,
    /// The summary, when one has been made.
    pub smart: Option<String>,
    /// Concept tags from that summary (PRD req 36).
    pub tags: Vec<String>,
    /// The chain from the file down to this element, outermost first.
    pub parents: Vec<Outline>,
    /// What is declared inside it, to the requested depth.
    pub children: Vec<Outline>,
}

/// A structural row: an address and enough to recognise it, with no content.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Outline {
    /// The child's address, in the same currency as everything else.
    pub address: String,
    /// Its universal category.
    pub kind: String,
    /// Its declared name.
    pub name: String,
    /// Inclusive 1-based `[start, end]`.
    pub span: [u32; 2],
    /// Its own children, when the requested depth reaches them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Outline>,
}

/// What `GET /tree` was asked for.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct TreeRequest {
    /// An address, a repo-relative path, or an absolute path. Absent means
    /// "where I am standing", which the `cwd` decides.
    #[serde(default)]
    pub address: Option<String>,
    /// How many levels to show. Default 2.
    #[serde(default)]
    pub depth: Option<u32>,
    /// How many files to list before reporting a count instead.
    #[serde(default)]
    pub limit: Option<i64>,
    /// Restrict to one repository identity, or `all`.
    #[serde(default)]
    pub repo: Option<String>,
    /// The caller's working directory (workshop 003 D6).
    #[serde(default)]
    pub cwd: Option<String>,
}

/// What `tree` answered with.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TreeResult {
    /// What was actually browsed, as an address or path.
    pub target: String,
    /// The repository browsed, when the target named or implied one.
    pub repo: Option<String>,
    /// `index`, `repository`, `directory` or `file` — what the target turned
    /// out to be.
    pub kind: String,
    /// How many files exist under the target (or elements, for a file).
    pub total: i64,
    /// How many of them this answer lists.
    pub showing: usize,
    /// The structure itself.
    pub entries: Vec<TreeEntry>,
}

/// One row of structure.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TreeEntry {
    /// `repository`, `directory`, `file`, or an element kind.
    pub kind: String,
    /// The segment's own name.
    pub name: String,
    /// The address to `get` or `tree` next, when the row has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The repo-relative path, for files and directories.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Inclusive 1-based `[start, end]`, for elements.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<[u32; 2]>,
    /// How many files this row contains, for directories and repositories.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<i64>,
    /// Nested structure, to the requested depth.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeEntry>,
}

/// What a resolved element address turned out to point at.
struct Located {
    file: IndexedFile,
    parser_version: String,
    root: Element,
}

/// Fetch one address.
///
/// # Errors
/// [`catalog::QUERY_INVALID_ADDRESS`] for something that is not an address,
/// [`catalog::QUERY_NOT_FOUND`] when nothing answers to it,
/// [`catalog::QUERY_INVALID_AMBIGUOUS`] when several things do, and
/// [`catalog::QUERY_NOT_IMPLEMENTED`] for a conversation address.
pub async fn get(
    state: &AppState,
    request: &GetRequest,
    scope: &Scope,
) -> Result<(GetResult, String), Failure> {
    let element = element_address(&request.address)?;
    let depth = depth_of(request.depth, DEFAULT_GET_DEPTH)?;

    let identities = fs3_store::repo_identities(&state.db)
        .await
        .map_err(fail)?;
    let parts = element.split(&identities);
    let repo = parts.repo.clone().or_else(|| scope.repo.clone());

    let located = locate(state, &parts, repo.as_deref(), scope).await?;
    let path = parts.path().to_string();

    let (chain, node) = if parts.is_whole_file() {
        (Vec::new(), &located.root)
    } else {
        pick(&located, &parts, request.span)?
    };

    let repo = Some(located.file.identity.clone());
    let smart = fs3_store::latest_summary(&state.db, node.raw_hash())
        .await
        .map_err(fail)?;

    let result = GetResult {
        address: fs3_core::element_address(repo.as_deref(), &node.address),
        repo,
        path,
        root_path: Some(located.file.root_path.clone()),
        kind: node.kind.as_str().to_string(),
        subkind: node.subkind.clone(),
        name: node.name.clone(),
        span: [node.span.start_line, node.span.end_line],
        raw_text: node.raw_text.clone(),
        smart: smart.as_ref().map(|summary| summary.text.clone()),
        tags: smart.map(|summary| summary.tags).unwrap_or_default(),
        parents: chain
            .iter()
            .map(|ancestor| outline(ancestor, located.file.identity.as_str(), 0))
            .collect(),
        children: if depth == 0 {
            Vec::new()
        } else {
            node.children
                .iter()
                .map(|child| outline(child, &located.file.identity, depth - 1))
                .collect()
        },
    };

    Ok((result, located.parser_version))
}

/// Browse structure under a path, a repository, or the whole index.
///
/// # Errors
/// The same codes as [`get`], for the same reasons.
pub async fn tree(
    state: &AppState,
    request: &TreeRequest,
    scope: &Scope,
) -> Result<TreeResult, Failure> {
    let depth = depth_of(request.depth, DEFAULT_TREE_DEPTH)?;
    let limit = match request.limit.unwrap_or(DEFAULT_TREE_LIMIT) {
        value if (1..=MAX_TREE_LIMIT).contains(&value) => value,
        value => {
            return Err(Failure::new(
                &catalog::QUERY_INVALID,
                format!("--limit must be between 1 and {MAX_TREE_LIMIT}, got {value}"),
            ));
        }
    };

    let identities = fs3_store::repo_identities(&state.db)
        .await
        .map_err(fail)?;

    // A target may be an address, a repo-relative path, or an absolute path on
    // this machine — `tree $(pwd)/crates` is the obvious thing to type, so it
    // works rather than being a puzzle.
    let raw = request
        .address
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let (repo, prefix) = match raw {
        None => (scope.repo.clone(), String::new()),
        Some(target) if target.starts_with('/') || target.starts_with("\\\\") => {
            absolute_target(state, target).await?
        }
        Some(target) => {
            let text = if target.starts_with("el:") || target.starts_with("conv:") {
                target.to_string()
            } else {
                format!("el:{target}")
            };
            let element = element_address(&text)?;
            let parts = element.split(&identities);
            (
                parts.repo.clone().or_else(|| scope.repo.clone()),
                parts.element.clone(),
            )
        }
    };

    // A prefix naming an indexed file browses that file's declarations.
    if !prefix.is_empty() {
        let files = fs3_store::files_at_path(&state.db, repo.as_deref(), &prefix)
            .await
            .map_err(fail)?;
        if !files.is_empty() {
            return file_tree(state, files, &prefix, depth, scope).await;
        }
    }

    // Nothing named, and no repository to browse: the index itself.
    if repo.is_none() && prefix.is_empty() {
        return index_tree(state, limit).await;
    }

    let total = fs3_store::count_files_under(&state.db, repo.as_deref(), Some(&prefix))
        .await
        .map_err(fail)?;
    if total == 0 {
        return Err(missing_path(state, repo.as_deref(), &prefix, scope).await);
    }

    let files = fs3_store::files_under(&state.db, repo.as_deref(), Some(&prefix), limit)
        .await
        .map_err(fail)?;
    let borrowed: Vec<&IndexedFile> = files.iter().collect();
    let entries = directory_entries(&borrowed, &prefix, depth, repo.as_deref());

    Ok(TreeResult {
        target: target_label(repo.as_deref(), &prefix),
        repo,
        kind: if prefix.is_empty() {
            "repository".to_string()
        } else {
            "directory".to_string()
        },
        total,
        showing: files.len(),
        entries,
    })
}

/// Every repository in the index, with how much of each is indexed.
async fn index_tree(state: &AppState, limit: i64) -> Result<TreeResult, Failure> {
    let worktrees = fs3_store::list_worktrees(&state.db).await.map_err(fail)?;
    let mut per_repo: BTreeMap<String, i64> = BTreeMap::new();
    for worktree in &worktrees {
        *per_repo.entry(worktree.identity.clone()).or_default() += worktree.file_count;
    }

    let total = per_repo.len() as i64;
    let entries: Vec<TreeEntry> = per_repo
        .into_iter()
        .take(limit as usize)
        .map(|(identity, files)| TreeEntry {
            kind: "repository".to_string(),
            name: identity.clone(),
            address: Some(fs3_core::element_address(Some(&identity), "")),
            path: None,
            span: None,
            files: Some(files),
            children: Vec::new(),
        })
        .collect();

    Ok(TreeResult {
        target: "index".to_string(),
        repo: None,
        kind: "index".to_string(),
        total,
        showing: entries.len(),
        entries,
    })
}

/// The declarations inside one indexed file.
async fn file_tree(
    state: &AppState,
    files: Vec<IndexedFile>,
    path: &str,
    depth: u32,
    scope: &Scope,
) -> Result<TreeResult, Failure> {
    let file = choose_file(files, path, scope)?;
    let (_, root) = parse_tree(state, &file).await?;

    let entries: Vec<TreeEntry> = root
        .children
        .iter()
        .map(|child| element_entry(child, &file.identity, depth.saturating_sub(1)))
        .collect();
    let total = root.iter().count() as i64 - 1;

    Ok(TreeResult {
        target: fs3_core::element_address(Some(&file.identity), path),
        repo: Some(file.identity),
        kind: "file".to_string(),
        total,
        showing: entries.len(),
        entries,
    })
}

/// Resolve an absolute host path to the repository and prefix it names.
async fn absolute_target(state: &AppState, target: &str) -> Result<(Option<String>, String), Failure> {
    let target = target.trim_end_matches('/');
    let worktree = fs3_store::worktree_containing(&state.db, target)
        .await
        .map_err(fail)?
        .ok_or_else(|| {
            Failure::new(
                &catalog::QUERY_NOT_FOUND,
                format!("{target} is not inside any registered root"),
            )
            .with_detail("path", target)
            .with_fix(format!(
                "index it with `flowspace3 add {target}`, or browse an indexed repository: \
                 `flowspace3 tree` lists them"
            ))
        })?;

    let relative = target
        .strip_prefix(&worktree.root_path)
        .unwrap_or_default()
        .trim_start_matches('/')
        .to_string();
    Ok((Some(worktree.identity), relative))
}

/// Find the file an element address lives in, and parse tree that answers.
async fn locate(
    state: &AppState,
    parts: &ElementParts,
    repo: Option<&str>,
    scope: &Scope,
) -> Result<Located, Failure> {
    let path = parts.path();
    if path.is_empty() {
        return Err(Failure::new(
            &catalog::QUERY_INVALID_ADDRESS,
            "an element address must name a file: `el:<repo>/<path>::<name>`",
        )
        .with_detail("address", parts.element.clone())
        .with_fix("browse what a repository holds with `flowspace3 tree <repo>`"));
    }

    let files = fs3_store::files_at_path(&state.db, repo, path)
        .await
        .map_err(fail)?;

    if files.is_empty() {
        return Err(missing_path(state, repo, path, scope).await);
    }

    let file = choose_file(files, path, scope)?;
    let (parser_version, root) = parse_tree(state, &file).await?;
    Ok(Located {
        file,
        parser_version,
        root,
    })
}

/// Pick one of several checkouts holding the same path.
///
/// Identical bytes are not a choice at all — every candidate answers the same
/// thing, so the first is the answer. Different bytes ARE a choice, and the
/// only non-arbitrary tiebreak is where the caller is standing; failing that,
/// the candidates are reported rather than one being picked silently.
fn choose_file(
    files: Vec<IndexedFile>,
    path: &str,
    scope: &Scope,
) -> Result<IndexedFile, Failure> {
    let mut blobs: Vec<&str> = files.iter().map(|file| file.blob_sha.as_str()).collect();
    blobs.sort_unstable();
    blobs.dedup();

    if blobs.len() <= 1 {
        return files
            .into_iter()
            .next()
            .ok_or_else(|| Failure::new(&catalog::QUERY_NOT_FOUND, format!("{path} is not indexed")));
    }

    if let Some(root) = scope.worktree.as_deref()
        && let Some(here) = files.iter().find(|file| file.root_path == root)
    {
        return Ok(here.clone());
    }

    let candidates: Vec<String> = files
        .iter()
        .map(|file| format!("{} at {}", file.identity, file.root_path))
        .collect();
    Err(Failure::new(
        &catalog::QUERY_INVALID_AMBIGUOUS,
        format!(
            "{path} exists in {} checkouts holding DIFFERENT content, so there is no single \
             answer",
            files.len()
        ),
    )
    .with_detail("path", path)
    .with_detail("candidates", candidates)
    .with_fix(
        "name the repository with `--repo <identity>`, or run the command from inside the \
         checkout you mean",
    ))
}

/// Read the element tree for one indexed file, current parser version first.
async fn parse_tree(state: &AppState, file: &IndexedFile) -> Result<(String, Element), Failure> {
    let versions = fs3_store::parser_versions_for_blob(&state.db, &file.blob_sha)
        .await
        .map_err(fail)?;

    let version = versions
        .iter()
        .find(|version| *version == PARSER_VERSION)
        .or_else(|| versions.first())
        .ok_or_else(|| {
            Failure::new(
                &catalog::QUERY_NOT_FOUND,
                format!(
                    "{} is registered but has not been parsed yet, so it has no elements to read",
                    file.path
                ),
            )
            .with_detail("path", file.path.clone())
            .with_detail("repo", file.identity.clone())
            .with_fix(
                "wait for the queue to drain — `flowspace3 status` reports what is left — then \
                 ask again",
            )
        })?
        .clone();

    let blob = fs3_core::BlobRef::new(&file.blob_sha).map_err(|error| {
        Failure::new(
            &catalog::QUERY_NOT_FOUND,
            format!("the stored content key for {} is unreadable: {error}", file.path),
        )
    })?;

    let root = fs3_store::get_elements(&state.db, &blob, &version)
        .await
        .map_err(fail)?
        .ok_or_else(|| {
            Failure::new(
                &catalog::QUERY_NOT_FOUND,
                format!("{} has no parsed elements under {version}", file.path),
            )
            .with_detail("path", file.path.clone())
            .with_fix("re-scan the root with `flowspace3 scan <path>`")
        })?;

    Ok((version, root))
}

/// Find the one element an address names, with its ancestors.
fn pick<'a>(
    located: &'a Located,
    parts: &ElementParts,
    span: Option<u32>,
) -> Result<(Vec<&'a Element>, &'a Element), Failure> {
    let mut matches = Vec::new();
    let mut chain = Vec::new();
    collect(&located.root, &parts.element, span, &mut chain, &mut matches);

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(unknown_element(located, parts, span)),
        _ => Err(ambiguous_element(located, parts, &matches)),
    }
}

/// Every element at `address`, each with the chain of ancestors above it.
fn collect<'a>(
    node: &'a Element,
    address: &str,
    span: Option<u32>,
    chain: &mut Vec<&'a Element>,
    out: &mut Vec<(Vec<&'a Element>, &'a Element)>,
) {
    if node.address == address && span.is_none_or(|line| node.span.start_line == line) {
        out.push((chain.clone(), node));
    }
    chain.push(node);
    for child in &node.children {
        collect(child, address, span, chain, out);
    }
    chain.pop();
}

/// The address parses, the file exists, and nothing in it answers to the name.
fn unknown_element(located: &Located, parts: &ElementParts, span: Option<u32>) -> Failure {
    let found: Vec<String> = located
        .root
        .iter()
        .skip(1)
        .take(NEARBY)
        .map(|element| {
            format!(
                "{} ({} {}-{})",
                element.address,
                element.kind.as_str(),
                element.span.start_line,
                element.span.end_line
            )
        })
        .collect();

    let qualifier = match span {
        Some(line) => format!(" starting at line {line}"),
        None => String::new(),
    };

    Failure::new(
        &catalog::QUERY_NOT_FOUND,
        format!(
            "{} holds no element addressed {}{qualifier}",
            located.file.path, parts.element
        ),
    )
    .with_detail("path", located.file.path.clone())
    .with_detail("repo", located.file.identity.clone())
    .with_detail("found_here", found)
    .with_fix(format!(
        "`flowspace3 tree el:{}/{}` lists everything this file declares; whole-file content is \
         `flowspace3 get el:{}/{}`",
        located.file.identity, located.file.path, located.file.identity, located.file.path
    ))
}

/// The address is real and names more than one element.
fn ambiguous_element(
    located: &Located,
    parts: &ElementParts,
    matches: &[(Vec<&Element>, &Element)],
) -> Failure {
    let candidates: Vec<String> = matches
        .iter()
        .map(|(_, element)| {
            format!(
                "{} {} lines {}-{} (--span {})",
                element.kind.as_str(),
                element.name,
                element.span.start_line,
                element.span.end_line,
                element.span.start_line
            )
        })
        .collect();

    Failure::new(
        &catalog::QUERY_INVALID_AMBIGUOUS,
        format!(
            "{} matches {} elements in {} — one address, several declarations",
            parts.element,
            matches.len(),
            located.file.path
        ),
    )
    .with_detail("address", parts.element.clone())
    .with_detail("candidates", candidates)
    .with_fix(format!(
        "pick one with `--span <line>`: {}",
        matches
            .iter()
            .map(|(_, element)| element.span.start_line.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// No indexed file at this path — say what IS there.
async fn missing_path(state: &AppState, repo: Option<&str>, path: &str, scope: &Scope) -> Failure {
    let parent = path.rsplit_once('/').map(|(head, _)| head).unwrap_or("");

    let nearby: Vec<String> = fs3_store::files_under(&state.db, repo, Some(parent), NEARBY as i64)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|file| file.path)
        .collect();

    // The path may exist in a repository the scope excluded, which is the most
    // confusing possible miss: the file is right there on disk in front of the
    // caller, and fs3 says it does not exist.
    let elsewhere: Vec<String> = if repo.is_some() {
        fs3_store::files_at_path(&state.db, None, path)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|file| file.identity)
            .collect()
    } else {
        Vec::new()
    };

    let scoped = match repo {
        Some(identity) => format!(" in {identity}"),
        None => String::new(),
    };

    let mut failure = Failure::new(
        &catalog::QUERY_NOT_FOUND,
        format!("no indexed file at {path}{scoped}"),
    )
    .with_detail("path", path)
    .with_detail("nearby", nearby);

    if !elsewhere.is_empty() {
        failure = failure
            .with_detail("indexed_in", elsewhere.clone())
            .with_fix(format!(
                "that path IS indexed in {} — widen with `--repo all`, or name it with `--repo \
                 <identity>`",
                elsewhere.join(", ")
            ));
    } else if let Some(cwd) = scope.cwd.as_deref() {
        failure = failure.with_fix(format!(
            "check the path against `flowspace3 tree`, or index this checkout with `flowspace3 \
             add {cwd}` if it is not indexed yet"
        ));
    }

    failure
}

/// Turn the requested address into an element address, or say why not.
fn element_address(text: &str) -> Result<fs3_core::ElementAddress, Failure> {
    match Address::parse(text) {
        Ok(Address::Element(element)) => Ok(element),
        Ok(Address::Conversation(conversation)) => Err(Failure::new(
            &catalog::QUERY_NOT_IMPLEMENTED,
            format!(
                "{conversation} is a conversation address, and this build does not store \
                 conversations yet"
            ),
        )
        .with_detail("address", conversation.to_string())
        .with_detail("guid", conversation.guid)),
        Err(error) => Err(Failure::new(
            &catalog::QUERY_INVALID_ADDRESS,
            error.to_string(),
        )
        .with_detail("address", text)),
    }
}

/// Validate a depth against the ceiling.
fn depth_of(requested: Option<u32>, default: u32) -> Result<u32, Failure> {
    match requested.unwrap_or(default) {
        depth if depth <= MAX_DEPTH => Ok(depth),
        depth => Err(Failure::new(
            &catalog::QUERY_INVALID,
            format!("--depth must be between 0 and {MAX_DEPTH}, got {depth}"),
        )),
    }
}

/// Render an element as a structural row, `depth` levels deep.
fn outline(element: &Element, repo: &str, depth: u32) -> Outline {
    Outline {
        address: fs3_core::element_address(Some(repo), &element.address),
        kind: element.kind.as_str().to_string(),
        name: element.name.clone(),
        span: [element.span.start_line, element.span.end_line],
        children: if depth == 0 {
            Vec::new()
        } else {
            element
                .children
                .iter()
                .map(|child| outline(child, repo, depth - 1))
                .collect()
        },
    }
}

/// The same, in `tree`'s row shape.
fn element_entry(element: &Element, repo: &str, depth: u32) -> TreeEntry {
    TreeEntry {
        kind: element.kind.as_str().to_string(),
        name: element.name.clone(),
        address: Some(fs3_core::element_address(Some(repo), &element.address)),
        path: None,
        span: Some([element.span.start_line, element.span.end_line]),
        files: None,
        children: if depth == 0 {
            Vec::new()
        } else {
            element
                .children
                .iter()
                .map(|child| element_entry(child, repo, depth - 1))
                .collect()
        },
    }
}

/// Fold a flat path list into directories and files, `depth` levels deep.
///
/// Directories are derived rather than stored: the ref layer holds paths, and a
/// directory is a prefix several of them share. Deriving it here means a
/// directory that exists on disk but holds nothing indexed never appears —
/// which is the honest answer for a browser over an INDEX rather than a
/// filesystem.
fn directory_entries(
    files: &[&IndexedFile],
    prefix: &str,
    depth: u32,
    repo: Option<&str>,
) -> Vec<TreeEntry> {
    let mut directories: BTreeMap<String, Vec<&IndexedFile>> = BTreeMap::new();
    let mut here: Vec<TreeEntry> = Vec::new();

    for file in files {
        let rest = file
            .path
            .strip_prefix(prefix)
            .unwrap_or(&file.path)
            .trim_start_matches('/');
        match rest.split_once('/') {
            None => here.push(TreeEntry {
                kind: "file".to_string(),
                name: rest.to_string(),
                address: Some(fs3_core::element_address(repo, &file.path)),
                path: Some(file.path.clone()),
                span: None,
                files: None,
                children: Vec::new(),
            }),
            Some((head, _)) => directories
                .entry(head.to_string())
                .or_default()
                .push(file),
        }
    }

    let mut entries: Vec<TreeEntry> = directories
        .into_iter()
        .map(|(name, group)| {
            let child_prefix = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let children = if depth <= 1 {
                Vec::new()
            } else {
                directory_entries(&group, &child_prefix, depth - 1, repo)
            };
            TreeEntry {
                kind: "directory".to_string(),
                name,
                address: None,
                path: Some(child_prefix),
                span: None,
                files: Some(group.len() as i64),
                children,
            }
        })
        .collect();

    here.sort_by(|left, right| left.name.cmp(&right.name));
    entries.extend(here);
    entries
}

/// What to call the thing that was browsed.
fn target_label(repo: Option<&str>, prefix: &str) -> String {
    match (repo, prefix.is_empty()) {
        (Some(identity), true) => identity.to_string(),
        (Some(identity), false) => fs3_core::element_address(Some(identity), prefix),
        (None, true) => "index".to_string(),
        (None, false) => prefix.to_string(),
    }
}

/// What a caller typically does after a `get`.
#[must_use]
pub fn next_after_get(result: &GetResult) -> String {
    if result.children.is_empty() {
        format!(
            "that is the whole element — open it at {}:{} , or browse its file with `flowspace3 \
             tree {}`",
            result.path,
            result.span[0],
            fs3_core::element_address(result.repo.as_deref(), &result.path)
        )
    } else {
        format!(
            "{} declarations are outlined above — `flowspace3 get <address>` reads any of them in \
             full",
            result.children.len()
        )
    }
}

/// What a caller typically does after a `tree`.
#[must_use]
pub fn next_after_tree(result: &TreeResult) -> String {
    if result.showing as i64 >= result.total {
        "`flowspace3 get <address>` reads any row above in full; `flowspace3 tree <address>` goes \
         deeper"
            .to_string()
    } else {
        format!(
            "showing {} of {} — narrow with a path (`flowspace3 tree {}<dir>`) or raise --limit",
            result.showing,
            result.total,
            match &result.repo {
                Some(identity) => format!("el:{identity}/"),
                None => String::new(),
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs3_core::element::{ElementKind, Span};

    fn file(path: &str) -> IndexedFile {
        IndexedFile {
            identity: "git:host/org/repo".to_string(),
            root_path: "/checkout".to_string(),
            path: path.to_string(),
            blob_sha: "a".repeat(40),
        }
    }

    fn element(kind: ElementKind, name: &str, address: &str, span: (u32, u32)) -> Element {
        Element::new(kind, "item", name, address, Span::new(span.0, span.1), "body")
    }

    /// The shape that makes ambiguity real: two elements, one address.
    #[test]
    fn one_address_can_match_two_elements() {
        let root = element(ElementKind::File, "lib.rs", "src/lib.rs", (1, 40)).with_children(vec![
            element(ElementKind::Container, "Rect", "src/lib.rs::Rect", (3, 8)),
            element(ElementKind::Container, "Rect", "src/lib.rs::Rect", (10, 30)),
        ]);

        let mut matches = Vec::new();
        collect(&root, "src/lib.rs::Rect", None, &mut Vec::new(), &mut matches);
        assert_eq!(matches.len(), 2);

        // and --span picks exactly one of them
        let mut narrowed = Vec::new();
        collect(
            &root,
            "src/lib.rs::Rect",
            Some(10),
            &mut Vec::new(),
            &mut narrowed,
        );
        assert_eq!(narrowed.len(), 1);
        assert_eq!(narrowed[0].1.span.start_line, 10);
    }

    /// The parent chain is what makes a fetched element navigable back up.
    #[test]
    fn a_match_carries_the_chain_above_it() {
        let root = element(ElementKind::File, "lib.rs", "src/lib.rs", (1, 40)).with_children(vec![
            element(ElementKind::Container, "Rect", "src/lib.rs::Rect", (3, 20)).with_children(
                vec![element(
                    ElementKind::Function,
                    "area",
                    "src/lib.rs::Rect::area",
                    (5, 9),
                )],
            ),
        ]);

        let mut matches = Vec::new();
        collect(
            &root,
            "src/lib.rs::Rect::area",
            None,
            &mut Vec::new(),
            &mut matches,
        );
        let (chain, node) = &matches[0];
        assert_eq!(node.name, "area");
        assert_eq!(
            chain.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["lib.rs", "Rect"]
        );
    }

    /// Directories are derived from the paths that exist, and only those.
    #[test]
    fn a_flat_path_list_folds_into_directories_and_files() {
        let files = vec![
            file("crates/store/src/lib.rs"),
            file("crates/store/src/read.rs"),
            file("crates/cli/src/main.rs"),
            file("README.md"),
        ];
        let borrowed: Vec<&IndexedFile> = files.iter().collect();
        let entries = directory_entries(&borrowed, "", 1, Some("git:host/org/repo"));

        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["crates", "README.md"]);
        assert_eq!(entries[0].kind, "directory");
        assert_eq!(entries[0].files, Some(3));
        // depth 1 stops at the directory rather than listing inside it
        assert!(entries[0].children.is_empty());
        assert_eq!(entries[1].kind, "file");
        assert_eq!(
            entries[1].address.as_deref(),
            Some("el:git:host/org/repo/README.md")
        );
    }

    #[test]
    fn depth_two_descends_one_directory_further() {
        let files = vec![
            file("crates/store/src/lib.rs"),
            file("crates/cli/src/main.rs"),
        ];
        let borrowed: Vec<&IndexedFile> = files.iter().collect();
        let entries = directory_entries(&borrowed, "", 2, None);
        assert_eq!(entries[0].name, "crates");
        let inner: Vec<&str> = entries[0]
            .children
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(inner, vec!["cli", "store"]);
    }

    /// A prefix already consumed must not be repeated in the names below it.
    #[test]
    fn a_prefixed_listing_names_only_what_is_below_the_prefix() {
        let files = vec![
            file("crates/store/src/lib.rs"),
            file("crates/store/migrations/0001.sql"),
        ];
        let borrowed: Vec<&IndexedFile> = files.iter().collect();
        let entries = directory_entries(&borrowed, "crates/store", 1, None);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["migrations", "src"]);
        assert_eq!(entries[0].path.as_deref(), Some("crates/store/migrations"));
    }

    #[test]
    fn a_conversation_address_is_not_yet_rather_than_malformed() {
        let failure = element_address("conv:abc-123").expect_err("conversations do not exist yet");
        assert_eq!(failure.code, catalog::QUERY_NOT_IMPLEMENTED.as_str());
        assert_eq!(failure.http_status(), 501);
    }

    #[test]
    fn something_that_is_not_an_address_says_so() {
        let failure = element_address("crates/store/src/lib.rs").expect_err("not an address");
        assert_eq!(failure.code, catalog::QUERY_INVALID_ADDRESS.as_str());
        assert_eq!(failure.http_status(), 400);
    }

    #[test]
    fn depth_is_bounded() {
        assert_eq!(depth_of(None, 1), Ok(1));
        assert!(depth_of(Some(MAX_DEPTH + 1), 1).is_err());
    }
}
