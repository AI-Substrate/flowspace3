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
use fs3_core::{Address, ConversationAddress, ConversationId, ElementParts, TurnItem};
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
    /// How many turns before the addressed one a conversation window carries.
    #[serde(default)]
    pub before: Option<u32>,
    /// How many after it.
    #[serde(default)]
    pub after: Option<u32>,
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

/// What `get` answers with — an element, or a window of turns.
///
/// Untagged, so the envelope's `data` IS the payload rather than a wrapper a
/// consumer has to unwrap. The two shapes are told apart by their `address`
/// scheme, which is the discriminator workshop 003 already gave every caller,
/// so adding a tag would be a second one to keep in step.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum GetPayload {
    /// An `el:` address: one element, with its content and neighbours.
    Element(Box<GetResult>),
    /// A `conv:` address: a contiguous run of turns.
    Conversation(ConversationWindow),
}

/// A contiguous run of turns around one ordinal.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConversationWindow {
    /// `conv:<guid>` — the conversation itself.
    pub address: String,
    /// The anchor repository identity, when the conversation has one.
    pub repo: Option<String>,
    /// The anchor checkout path.
    pub worktree: Option<String>,
    /// The commit the conversation started from.
    pub base_sha: Option<String>,
    /// The conversation's title, when it has one.
    pub title: Option<String>,
    /// How many turns the conversation holds in total.
    pub turns: i64,
    /// The ordinal the window is centred on.
    pub around: u32,
    /// The turns themselves, in order.
    pub window: Vec<TurnView>,
}

/// One turn, as `get` returns it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TurnView {
    /// `conv:<guid>#t<ord>` — addressable on its own.
    pub address: String,
    /// Position in the conversation.
    pub turn_no: u32,
    /// `human` or `agent`.
    pub role: String,
    /// `human`, `peer` or `system` — where the turn came from, which is not the
    /// same question as who wrote it (workshop 005, C8).
    pub source: String,
    /// Repo HEAD at time-of-turn, when there was one.
    pub head_sha: Option<String>,
    /// When it happened, RFC 3339 in UTC.
    pub at: String,
    /// The turn's prose, verbatim.
    pub body: String,
    /// Its typed sub-items, already shaped by the intake policy.
    pub items: Vec<TurnItem>,
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
    /// Who spoke, for a turn row (workshop 005's outline: role, source, time,
    /// first line). Absent on every code row, because a function has no role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Where the turn came from — `human`, `peer` or `system`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// When it happened, RFC 3339 in UTC.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
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
/// Two shapes answer here, because two things have addresses. An `el:` address
/// returns an element with its content, parents and children; a `conv:` address
/// returns a WINDOW of turns around one ordinal — the caller picks how far
/// either way and pays for exactly what it asked for (workshop 003).
///
/// # Errors
/// [`catalog::QUERY_INVALID_ADDRESS`] for something that is not an address,
/// [`catalog::QUERY_NOT_FOUND`] when nothing answers to it, and
/// [`catalog::QUERY_INVALID_AMBIGUOUS`] when several things do.
pub async fn get(
    state: &AppState,
    request: &GetRequest,
    scope: &Scope,
) -> Result<(GetPayload, String), Failure> {
    let address = address_of(&request.address)?;
    let depth = depth_of(request.depth, DEFAULT_GET_DEPTH)?;
    let identities = fs3_store::repo_identities(&state.db).await.map_err(fail)?;

    let (parts, path, whole_file, is_ddoc) = match address {
        Address::Conversation(conversation) => {
            let window = conversation_window(state, &conversation, request).await?;
            return Ok((
                GetPayload::Conversation(window),
                CONVERSATION_SOURCE.to_string(),
            ));
        }
        Address::Element(element) => {
            let parts = element.split(&identities);
            let path = parts.path().to_string();
            let whole_file = parts.is_whole_file();
            (parts, path, whole_file, false)
        }
        Address::Ddoc(ddoc) => {
            let path = ddoc.file.clone();
            let element = ddoc.render();
            (
                ElementParts {
                    repo: None,
                    element,
                },
                path,
                false,
                true,
            )
        }
    };
    let repo = parts.repo.clone().or_else(|| scope.repo.clone());

    let located = locate(state, &parts, &path, repo.as_deref(), scope).await?;
    let (chain, node) = if whole_file {
        (Vec::new(), &located.root)
    } else {
        pick(&located, &parts, request.span)?
    };

    let repo = Some(located.file.identity.clone());
    let smart = fs3_store::latest_summary(&state.db, node.raw_hash())
        .await
        .map_err(fail)?;

    let result = GetResult {
        address: if is_ddoc {
            node.address.clone()
        } else {
            fs3_core::element_address(repo.as_deref(), &node.address)
        },
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

    Ok((
        GetPayload::Element(Box::new(result)),
        located.parser_version,
    ))
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

    // A conversation is browsed by its own shape — a sequence, not a
    // hierarchy — so it is answered before the path machinery below, which has
    // no notion of a turn.
    if let Some(target) = request.address.as_deref().map(str::trim)
        && target.starts_with(fs3_core::address::CONVERSATION_SCHEME)
        && let Address::Conversation(conversation) = address_of(target)?
    {
        return conversation_outline(state, &conversation).await;
    }

    let identities = fs3_store::repo_identities(&state.db).await.map_err(fail)?;

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
            let text = if target.starts_with(fs3_core::address::ELEMENT_SCHEME) {
                target.to_string()
            } else {
                // A bare path is an element address that has not been spelled
                // as one yet, so it is completed with the parser's own scheme
                // rather than a literal — there is one spelling of `el:` in
                // this system and it lives in `fs3_core::address`.
                format!("{}{target}", fs3_core::address::ELEMENT_SCHEME)
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
            role: None,
            source: None,
            at: None,
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
async fn absolute_target(
    state: &AppState,
    target: &str,
) -> Result<(Option<String>, String), Failure> {
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
    path: &str,
    repo: Option<&str>,
    scope: &Scope,
) -> Result<Located, Failure> {
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
fn choose_file(files: Vec<IndexedFile>, path: &str, scope: &Scope) -> Result<IndexedFile, Failure> {
    let mut blobs: Vec<&str> = files.iter().map(|file| file.blob_sha.as_str()).collect();
    blobs.sort_unstable();
    blobs.dedup();

    if blobs.len() <= 1 {
        return files.into_iter().next().ok_or_else(|| {
            Failure::new(&catalog::QUERY_NOT_FOUND, format!("{path} is not indexed"))
        });
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
            format!(
                "the stored content key for {} is unreadable: {error}",
                file.path
            ),
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
    collect(
        &located.root,
        &parts.element,
        span,
        &mut chain,
        &mut matches,
    );

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

/// Parse the requested address, or say why it is not one.
fn address_of(text: &str) -> Result<Address, Failure> {
    Address::parse(text).map_err(|error| {
        Failure::new(&catalog::QUERY_INVALID_ADDRESS, error.to_string())
            .with_detail("address", text)
    })
}

/// Turn the requested address into an element address, or say why not.
fn element_address(text: &str) -> Result<fs3_core::ElementAddress, Failure> {
    match address_of(text)? {
        Address::Element(element) => Ok(element),
        Address::Conversation(conversation) => Err(Failure::new(
            &catalog::QUERY_INVALID,
            format!("{conversation} is a conversation address, and this verb browses code"),
        )
        .with_fix(format!(
            "`flowspace3 tree {conversation}` outlines a conversation; \
             `flowspace3 get {conversation}#t1` reads it"
        ))
        .with_detail("address", conversation.to_string())),
        Address::Ddoc(ddoc) => Err(Failure::new(
            &catalog::QUERY_INVALID,
            format!("{ddoc} is a ddoc address, and this path browses code"),
        )
        .with_fix(format!("`flowspace3 get {ddoc}` reads that ddoc row"))
        .with_detail("address", ddoc.to_string())),
    }
}

/// The `parser_version` a conversation answer reports.
///
/// A conversation has no parse tree, so there is no version to name. Saying
/// where the answer came from beats reporting a code parser's version for
/// content it never touched.
const CONVERSATION_SOURCE: &str = fs3_core::conversation::PARSER_VERSION;

/// How far either way a window reaches when the caller says nothing.
///
/// Workshop 003's own example is `--before 10 --after 20`, and the asymmetry is
/// the point: what came BEFORE a turn is context, what came AFTER it is what
/// happened next, and the second is usually what a reader wanted.
const DEFAULT_BEFORE: u32 = 10;
const DEFAULT_AFTER: u32 = 20;

/// The furthest a single window may reach.
///
/// Turns carry whole tool results; a window of a thousand is a scrollback dump
/// billed to the caller's context, which is the cost `get` exists to let them
/// control rather than accidentally pay.
const MAX_WINDOW: u32 = 200;

/// Read a window of turns around one ordinal.
///
/// A bare `conv:<guid>` with no ordinal is not an error: it is "show me the
/// start", so the window centres on turn 1 and reaches forward. Refusing it
/// would make the address workshop 003 defines for a whole conversation the one
/// address `get` cannot take.
async fn conversation_window(
    state: &AppState,
    address: &ConversationAddress,
    request: &GetRequest,
) -> Result<ConversationWindow, Failure> {
    let guid = ConversationId::new(address.guid.clone()).map_err(|error| {
        Failure::new(&catalog::QUERY_INVALID_ADDRESS, error.to_string())
            .with_detail("address", address.to_string())
    })?;

    let before = window_reach(request.before, DEFAULT_BEFORE)?;
    let after = window_reach(request.after, DEFAULT_AFTER)?;
    let around = address.turn.unwrap_or(1);

    let summary = fs3_store::list_conversations(
        &state.db,
        fs3_store::AnchorFilter {
            guid: Some(guid.as_str()),
            ..fs3_store::AnchorFilter::default()
        },
    )
    .await
    .map_err(fail)?
    .pop()
    .ok_or_else(|| unknown_conversation(&guid))?;

    let turns = fs3_store::window(&state.db, &guid, around, before, after)
        .await
        .map_err(fail)?;

    // An empty window inside a conversation that EXISTS means the ordinal is
    // past the end, and the honest answer names the range that does exist —
    // "not found" alone would read as "this conversation is empty".
    if turns.is_empty() {
        return Err(Failure::new(
            &catalog::QUERY_NOT_FOUND,
            format!(
                "conversation {guid} has {} turn(s); nothing sits within -{before}/+{after} of turn {around}",
                summary.turns
            ),
        )
        .with_fix(format!(
            "`flowspace3 tree conv:{guid}` lists the turns that exist"
        ))
        .with_detail("turns", summary.turns));
    }

    Ok(ConversationWindow {
        address: guid.address(),
        repo: summary.repo_identity,
        worktree: summary.worktree,
        base_sha: summary.base_sha,
        title: summary.title,
        turns: summary.turns,
        around,
        window: turns
            .into_iter()
            .map(|turn| TurnView {
                address: guid.turn_address(turn.turn_no),
                turn_no: turn.turn_no,
                role: turn.role.as_str().to_string(),
                source: turn.source.as_str().to_string(),
                head_sha: turn.head_sha,
                at: turn.at,
                body: turn.body,
                items: turn.items,
            })
            .collect(),
    })
}

/// Validate one half of a window against the ceiling.
fn window_reach(requested: Option<u32>, default: u32) -> Result<u32, Failure> {
    match requested.unwrap_or(default) {
        reach if reach <= MAX_WINDOW => Ok(reach),
        reach => Err(Failure::new(
            &catalog::QUERY_INVALID,
            format!("--before/--after must be between 0 and {MAX_WINDOW}, got {reach}"),
        )),
    }
}

/// A conversation guid nothing answers to.
fn unknown_conversation(guid: &ConversationId) -> Failure {
    Failure::new(
        &catalog::QUERY_NOT_FOUND,
        format!("no conversation {guid} is indexed"),
    )
    .with_fix(
        "`flowspace3 conversation list` shows what is indexed; \
         `flowspace3 conversation import <file>` adds one",
    )
    .with_detail("guid", guid.as_str())
}

/// The turn outline of one conversation — role, source, time, first line.
///
/// `tree`'s answer for a sequence. Deliberately lean, and for the same reason
/// the code outline is: this exists so a caller can decide WHICH turns to pay
/// for, and a row carrying a whole turn has already spent the tokens the
/// outline was meant to save.
async fn conversation_outline(
    state: &AppState,
    address: &ConversationAddress,
) -> Result<TreeResult, Failure> {
    let guid = ConversationId::new(address.guid.clone()).map_err(|error| {
        Failure::new(&catalog::QUERY_INVALID_ADDRESS, error.to_string())
            .with_detail("address", address.to_string())
    })?;

    let summary = fs3_store::list_conversations(
        &state.db,
        fs3_store::AnchorFilter {
            guid: Some(guid.as_str()),
            ..fs3_store::AnchorFilter::default()
        },
    )
    .await
    .map_err(fail)?
    .pop()
    .ok_or_else(|| unknown_conversation(&guid))?;

    let rows = fs3_store::outline(&state.db, &guid).await.map_err(fail)?;

    Ok(TreeResult {
        target: summary.title.clone().unwrap_or_else(|| guid.address()),
        repo: summary.repo_identity,
        kind: "conversation".to_string(),
        total: summary.turns,
        showing: rows.len(),
        entries: rows
            .into_iter()
            .map(|row| TreeEntry {
                kind: fs3_core::ElementKind::Turn.as_str().to_string(),
                // The first line IS the name here: it is what a reader
                // recognises a turn by, the way a declaration's name is.
                name: row.first_line,
                address: Some(guid.turn_address(row.turn_no)),
                path: None,
                span: Some([row.turn_no, row.turn_no]),
                files: None,
                role: Some(row.role.as_str().to_string()),
                source: Some(row.source.as_str().to_string()),
                at: Some(row.at),
                children: Vec::new(),
            })
            .collect(),
    })
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
        role: None,
        source: None,
        at: None,
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
                role: None,
                source: None,
                at: None,
                children: Vec::new(),
            }),
            Some((head, _)) => directories.entry(head.to_string()).or_default().push(file),
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
                role: None,
                source: None,
                at: None,
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
pub fn next_after_get(payload: &GetPayload) -> String {
    let result = match payload {
        GetPayload::Conversation(window) => {
            let first = window.window.first().map_or(1, |turn| turn.turn_no);
            let last = window.window.last().map_or(1, |turn| turn.turn_no);
            return if last as i64 >= window.turns {
                format!(
                    "turns {first}–{last} of {} — that is the end of the conversation; \
                     `flowspace3 tree {}` outlines the whole thing",
                    window.turns, window.address
                )
            } else {
                format!(
                    "turns {first}–{last} of {}; `flowspace3 get {}#t{last} --after 20` reads on \
                     from here",
                    window.turns, window.address
                )
            };
        }
        GetPayload::Element(element) => element,
    };

    if result.children.is_empty() {
        format!(
            "that is the whole element — open it at {}:{}, or browse its file with `flowspace3 \
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
                Some(identity) => fs3_core::element_address(Some(identity), ""),
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
        Element::new(
            kind,
            "item",
            name,
            address,
            Span::new(span.0, span.1),
            "body",
        )
    }

    /// The shape that makes ambiguity real: two elements, one address.
    #[test]
    fn one_address_can_match_two_elements() {
        let root = element(ElementKind::File, "lib.rs", "src/lib.rs", (1, 40)).with_children(vec![
            element(ElementKind::Container, "Rect", "src/lib.rs::Rect", (3, 8)),
            element(ElementKind::Container, "Rect", "src/lib.rs::Rect", (10, 30)),
        ]);

        let mut matches = Vec::new();
        collect(
            &root,
            "src/lib.rs::Rect",
            None,
            &mut Vec::new(),
            &mut matches,
        );
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
        let files = [
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
        let files = [
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
        let files = [
            file("crates/store/src/lib.rs"),
            file("crates/store/migrations/0001.sql"),
        ];
        let borrowed: Vec<&IndexedFile> = files.iter().collect();
        let entries = directory_entries(&borrowed, "crates/store", 1, None);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["migrations", "src"]);
        assert_eq!(entries[0].path.as_deref(), Some("crates/store/migrations"));
    }

    /// A conversation address is a real address now, so the CODE path has to
    /// refuse it as the wrong shape rather than as an unimplemented feature —
    /// and the refusal has to point at the verb that does answer it.
    #[test]
    fn a_conversation_address_is_the_wrong_shape_for_the_code_path() {
        let failure = element_address("conv:6ba7b810-9dad-11d1-80b4-00c04fd430c8")
            .expect_err("this path browses code");
        assert_eq!(failure.code, catalog::QUERY_INVALID.as_str());
        assert!(
            failure.fix.contains("tree conv:"),
            "the fix must name the verb that does answer: {}",
            failure.fix
        );
    }

    /// And the dispatcher above it takes both schemes, which is what makes the
    /// conversation arm reachable at all.
    #[test]
    fn the_dispatcher_parses_both_address_schemes() {
        assert!(matches!(
            address_of("conv:6ba7b810-9dad-11d1-80b4-00c04fd430c8#t42"),
            Ok(Address::Conversation(_))
        ));
        assert!(matches!(
            address_of("el:crates/store/src/lib.rs::migrate"),
            Ok(Address::Element(_))
        ));
        assert!(matches!(
            address_of("docs/plan.dd.json#acceptance_criteria/ac-0001"),
            Ok(Address::Ddoc(_))
        ));
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
