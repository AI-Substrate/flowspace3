//! Which files does fs3 even look at? — ignore-aware discovery (PRD req 41).
//!
//! The POC measured this as *the* performance lever: the same 18,628 elements
//! came out of 11% of the bytes, 13.8× faster, from file selection alone —
//! before a single parser optimisation
//! (`docs/plans/001-fs3-foundations/assets/poc/treesitter-results.md`). So the
//! walk is not a detail of the scanner; it is the scanner's budget.
//!
//! Four filters, all injected (never read from the environment here):
//!
//! 1. **What git ignores, fs3 ignores** — tracked *and* untracked-but-not-
//!    ignored, so a brand-new file indexes without `git add`. Per-repo config
//!    can force-include folders the default would miss (PRD req 41).
//! 2. **[`STANDARD_IGNORES`]** — the directories nobody indexes, denied by
//!    whole path component whether or not the repo has a `.gitignore`. A
//!    fresh clone with no ignore file is otherwise a first-run trap: its
//!    `node_modules/**/*.js` is real JavaScript, and the summarise-and-embed
//!    budget is real money.
//! 3. **An extension allow-list by family** — config/data formats are excluded
//!    by default (PRD req 43: they yield no code-shaped elements and carry
//!    PII/secrets risk in a central store).
//! 4. **A size ceiling and a binary sniff** — one 18 MB `.session.json` cost
//!    0.62 s in the POC; a PNG named `.md` costs a parse and teaches nothing.
//!
//! Precedence, highest first: `exclude` · `force_include` ·
//! [`STANDARD_IGNORES`] · git's ignore rules.
//!
//! Everything a file is refused *for* lands in [`Discovery::skipped`] with a
//! reason, because "unsupported/no-grammar files must be an observable
//! outcome, never a silent gap" (PRD req 43). What git ignores — and what the
//! deny list prunes — is **not** in that ledger: those paths are out of scope,
//! not refused, and a pruned `node_modules` must not cost a hundred thousand
//! ledger rows.
//!
//! Paths come back **relative to the root** with `/` separators, so the same
//! folder scanned from any absolute location (or any machine) yields identical
//! rows — which is what makes repo identity a filter rather than an
//! installation (PRD req 12) and keeps the repo footprint zero (req 39).
//!
//! ## Why this lives in `fs3-parsers`
//!
//! It walks a real directory, so it cannot live in `fs3-core` (workshop 001
//! rule 2: the core performs no IO). It turns the world into typed values the
//! rest of fs3 consumes — the placement rule's "produces core types from the
//! world → parsers" — and it needs [`Language`] to answer "do we have a grammar
//! for this?", which is the table [`crate::scan`] already owns. Duplicating
//! that table in another crate is exactly the per-language code PRD req 21
//! refuses. The IO is confined to [`discover`]; the decisions it makes are pure
//! functions tested without a filesystem.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use fs3_core::ScanConfig;
use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::Language;

/// How many leading bytes the binary sniff reads. A NUL in the first 8 KiB is
/// how `git diff` calls a file binary, and it is enough: real source files do
/// not open with a NUL.
const SNIFF_BYTES: usize = 8 * 1024;

/// Directory names fs3 never walks, `.gitignore` or no `.gitignore`.
///
/// A repository without a `.gitignore` is not a repository without build
/// output: `node_modules/**/*.js` is real JavaScript, `js` is in the source
/// table, and nothing else in this module would stop it — so a first scan of a
/// fresh clone could spend a summarise-and-embed budget on somebody else's
/// dependencies before anyone noticed. Git-ignore rules are the *repo's*
/// opinion; this list is fs3's, and it holds when the repo has none.
///
/// Matched against whole path COMPONENTS of directories, never substrings:
/// `src/target_types.rs`, `my-vendor/`, `builder/` and `build-output/` all
/// survive. Applied at depth > 0 only, so `flowspace3 add ./node_modules` —
/// an explicit, deliberate root — still works. ASCII-case-insensitively, so
/// `Build/` is denied too: on a case-insensitive volume it is the same
/// directory as `build/`, and case sensitivity is a property of the volume
/// rather than the platform.
///
/// `pub` so callers can *see* the policy — but a second filter should take
/// [`DiscoverySettings::standard_ignores`], the resolved value, rather than
/// this raw list: only the settings carry the `scan.standard_ignores` toggle,
/// and only a root-relative match agrees with what [`discover`] does. See
/// `docs/services/discovery.md`, "Delegation is to the settings, not the
/// const".
pub const STANDARD_IGNORES: &[&str] = &[
    ".cache",
    ".git",
    ".next",
    ".venv",
    "__pycache__",
    "build",
    "dist",
    "node_modules",
    "target",
    "vendor",
    "venv",
];

/// What a file's extension says it is — the axis PRD req 43 excludes on.
///
/// This is coarser than [`Language`] on purpose: fs3 must decide whether to
/// *look at* a file long before it knows whether a grammar exists for it. A
/// file fs3 has a grammar for is never [`LanguageFamily::Unknown`] — that
/// invariant is mechanical, since the grammar table is consulted first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LanguageFamily {
    /// Code. Parsed for elements when a grammar exists, indexed either way.
    Source,
    /// Prose. Markdown is split into sections (PRD req 22); the rest indexes
    /// as a single file element.
    Document,
    /// Data/config — YAML, JSON, TOML, HCL and kin. Excluded by default
    /// (PRD req 43), revisited when fs3 has an answer for secrets.
    Config,
    /// No opinion. Binaries, lock-step artefacts, extensionless files.
    Unknown,
}

/// Code extensions. Not a grammar list — a *candidacy* list: a `.go` file is
/// worth indexing today (as a file element) and gains elements the day the
/// grammar lands, with no change here.
const SOURCE_EXTENSIONS: &[&str] = &[
    "bash", "c", "cc", "cjs", "cpp", "cs", "css", "cxx", "dart", "erl", "ex", "exs", "go", "h",
    "hh", "hpp", "hs", "htm", "html", "java", "js", "jsx", "kt", "kts", "lua", "m", "mjs", "mm",
    "php", "pl", "pm", "proto", "ps1", "psm1", "py", "pyi", "r", "rb", "rs", "scala", "scss", "sh",
    "sql", "svelte", "swift", "ts", "tsx", "vue", "zig", "zsh",
];

/// Prose extensions.
const DOCUMENT_EXTENSIONS: &[&str] = &["adoc", "markdown", "md", "mdx", "org", "rst", "txt"];

/// Data/config extensions — the PRD req 43 exclusion set.
const CONFIG_EXTENSIONS: &[&str] = &[
    "cfg",
    "conf",
    "csv",
    "env",
    "hcl",
    "ini",
    "json",
    "json5",
    "jsonc",
    "lock",
    "plist",
    "properties",
    "tf",
    "tfvars",
    "toml",
    "tsv",
    "xml",
    "yaml",
    "yml",
];

impl LanguageFamily {
    /// Classify a file extension (without the dot, any case).
    pub fn for_extension(extension: &str) -> Self {
        let extension = extension.to_ascii_lowercase();
        // Grammar first: whatever fs3 can parse is, by construction, indexable.
        // Adding a language stays one line in `Language::for_extension`.
        if let Some(language) = Language::for_extension(&extension) {
            return match language {
                Language::Markdown => LanguageFamily::Document,
                Language::Rust | Language::Python | Language::TypeScript | Language::Tsx => {
                    LanguageFamily::Source
                }
            };
        }
        let extension = extension.as_str();
        if SOURCE_EXTENSIONS.binary_search(&extension).is_ok() {
            LanguageFamily::Source
        } else if DOCUMENT_EXTENSIONS.binary_search(&extension).is_ok() {
            LanguageFamily::Document
        } else if CONFIG_EXTENSIONS.binary_search(&extension).is_ok() {
            LanguageFamily::Config
        } else {
            LanguageFamily::Unknown
        }
    }

    /// Classify a path by its extension. Extensionless files are
    /// [`LanguageFamily::Unknown`] — `Makefile` and `LICENSE` are not v1 index
    /// material, and guessing by filename is the name-matching PRD req 42
    /// refuses elsewhere.
    pub fn for_path(path: &Path) -> Self {
        match path.extension().and_then(|e| e.to_str()) {
            Some(extension) => Self::for_extension(extension),
            None => LanguageFamily::Unknown,
        }
    }

    /// The name used in reports and skip ledgers.
    pub const fn as_str(self) -> &'static str {
        match self {
            LanguageFamily::Source => "source",
            LanguageFamily::Document => "document",
            LanguageFamily::Config => "config",
            LanguageFamily::Unknown => "unknown",
        }
    }
}

/// The injected policy. Nothing here is read from disk or the environment by
/// this module: the daemon resolves config (machine defaults < worktree
/// override < flag, PRD req 40) and hands the answer down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoverySettings {
    /// Skip files larger than this. Generated bundles cost tokens and teach
    /// the index nothing.
    pub max_file_bytes: u64,
    /// Skip files smaller than this. The default skips empty files only.
    pub min_file_bytes: u64,
    /// Honour `.gitignore`, `.ignore` and `.git/info/exclude`.
    pub respect_gitignore: bool,
    /// Walk dot-files and dot-directories. `.git` is skipped regardless.
    pub include_hidden: bool,
    /// Follow symlinks. Off by default: a link loop is an infinite scan.
    pub follow_symlinks: bool,
    /// Index YAML/JSON/TOML/HCL and kin. Off by default (PRD req 43).
    pub index_config_formats: bool,
    /// Gitignore-syntax globs, relative to the root, naming paths to index
    /// **even when git ignores them** (PRD req 41: "a gitignored folder you do
    /// want indexed"). A forced path still faces the size, format and binary
    /// filters — force-include overrides git, not judgement.
    pub force_include: Vec<String>,
    /// Gitignore-syntax globs, relative to the root, naming paths fs3 must
    /// never index. The highest precedence rule there is: an explicit refusal
    /// beats an explicit inclusion.
    pub exclude: Vec<String>,
    /// Directory names never walked, matched as whole path components,
    /// **whether or not the repo has a `.gitignore`** — see
    /// [`STANDARD_IGNORES`], which is this field's default.
    ///
    /// Empty turns the deny list off (the `scan.standard_ignores = false`
    /// shape); any other list replaces it wholesale.
    pub standard_ignores: Vec<String>,
}

impl Default for DiscoverySettings {
    fn default() -> Self {
        Self {
            max_file_bytes: 2_000_000,
            min_file_bytes: 1,
            respect_gitignore: true,
            include_hidden: false,
            follow_symlinks: false,
            index_config_formats: false,
            force_include: Vec::new(),
            exclude: Vec::new(),
            standard_ignores: STANDARD_IGNORES.iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl From<&ScanConfig> for DiscoverySettings {
    /// The `[scan]` section, plus the discovery-only knobs at their defaults.
    /// This is the whole wiring story: the composition root passes
    /// `(&config.scan).into()`.
    ///
    /// `scan.standard_ignores` is a **bool** in TOML because that is the whole
    /// question a config file needs to answer; [`DiscoverySettings`] carries
    /// the list itself, which is a superset — `false` is the empty list, and a
    /// caller with a reason can pass its own names without a config schema
    /// change.
    fn from(scan: &ScanConfig) -> Self {
        Self {
            max_file_bytes: scan.max_file_bytes,
            min_file_bytes: scan.min_file_bytes,
            respect_gitignore: scan.respect_gitignore,
            include_hidden: scan.include_hidden,
            follow_symlinks: scan.follow_symlinks,
            standard_ignores: if scan.standard_ignores {
                STANDARD_IGNORES
                    .iter()
                    .map(|name| name.to_string())
                    .collect()
            } else {
                Vec::new()
            },
            ..Self::default()
        }
    }
}

/// A file fs3 will scan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredFile {
    /// Relative to the discovery root, `/`-separated.
    pub path: String,
    /// Size on disk, in bytes.
    pub bytes: u64,
    /// What the extension says it is.
    pub family: LanguageFamily,
    /// The grammar, when fs3 has one. `None` still indexes — as a file element
    /// with `language()` of `unknown` (see [`crate::scan`]).
    pub language: Option<Language>,
}

/// Why a file fs3 *saw* was refused. Files git ignores never appear here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SkipReason {
    /// Matched a configured `exclude` glob.
    Excluded,
    /// A data/config format, with `index_config_formats` off (PRD req 43).
    ConfigFormat,
    /// A generated `*.dd.md` projection whose `*.dd.json` source is indexed.
    GeneratedSibling,
    /// An extension fs3 has no opinion about — the observable no-grammar
    /// outcome PRD req 43 demands.
    UnsupportedExtension,
    /// Over `max_file_bytes`.
    TooLarge,
    /// Under `min_file_bytes` (empty files, by default).
    TooSmall,
    /// A NUL byte in the first [`SNIFF_BYTES`] — content, not extension, is
    /// what settles this.
    Binary,
    /// The walker or `stat` refused: permissions, a race, a broken link.
    Unreadable,
}

impl SkipReason {
    /// The name used in reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            SkipReason::Excluded => "excluded",
            SkipReason::ConfigFormat => "config-format",
            SkipReason::GeneratedSibling => "generated-sibling",
            SkipReason::UnsupportedExtension => "unsupported-extension",
            SkipReason::TooLarge => "too-large",
            SkipReason::TooSmall => "too-small",
            SkipReason::Binary => "binary",
            SkipReason::Unreadable => "unreadable",
        }
    }
}

/// A file fs3 looked at and refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedFile {
    /// Relative to the discovery root, `/`-separated.
    pub path: String,
    /// Why.
    pub reason: SkipReason,
}

/// Why a directory was never walked.
///
/// One variant, deliberately. `ignore`'s walker applies its own matchers
/// *before* the callback fs3 supplies (`Walk::skip_entry` consults
/// `should_skip_entry` first), so a git-ignored or hidden directory is pruned
/// before this crate is ever asked about it and cannot honestly be reported
/// here. That half already has an answer fs3 would only be guessing at:
/// `git check-ignore -v <path>` names the file and line that did it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PruneReason {
    /// Denied by [`DiscoverySettings::standard_ignores`].
    StandardIgnore,
}

impl PruneReason {
    /// The name used in reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            PruneReason::StandardIgnore => "standard-ignore",
        }
    }
}

/// A directory fs3 refused to walk, named so its absence is not silent.
///
/// The directory, never its contents: that is what makes this ledger safe.
/// Reporting the files under a denied `node_modules` was measured at 316,609
/// rows on this repo; reporting the directories themselves is ~11, and eleven
/// rows are the answer to "why is my code missing" rather than a summary of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrunedDirectory {
    /// Relative to the discovery root, `/`-separated — the same shape as every
    /// other path in a [`Discovery`].
    pub path: String,
    /// Why.
    pub reason: PruneReason,
}

/// What a walk found. All three lists are sorted by path, so a discovery
/// result is directly comparable — in a test, and between two scans of the
/// same tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Discovery {
    /// Files to scan.
    pub files: Vec<DiscoveredFile>,
    /// Files seen and refused, with the reason (PRD req 43).
    pub skipped: Vec<SkippedFile>,
    /// Directories never walked, with the reason. Their contents are in
    /// neither of the other lists — that is the point of pruning — so this is
    /// the only thing standing between a denied directory and a user who
    /// cannot work out why their code is missing.
    pub pruned: Vec<PrunedDirectory>,
}

/// Why a walk could not start. Per-file trouble is a [`SkipReason`], not an
/// error: one unreadable file must never fail a repo scan.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The root does not exist, or is not a directory.
    #[error("discovery root is not a directory: {0}")]
    NotADirectory(PathBuf),
    /// A configured glob is not valid gitignore syntax.
    #[error("invalid {setting} glob {glob:?}")]
    Glob {
        /// `force_include` or `exclude`.
        setting: &'static str,
        /// The offending pattern.
        glob: String,
        /// What the glob compiler said.
        #[source]
        source: ignore::Error,
    },
}

/// Walk `root` and decide, for every file, whether fs3 scans it.
///
/// Sequential by design: the POC's win came from *not visiting* files, not
/// from visiting them on more threads, and a deterministic order makes the
/// result assertable. `ignore::WalkParallel` is a drop-in if a measurement
/// ever asks for it.
///
/// # Errors
/// [`DiscoveryError::NotADirectory`] if `root` is not a directory, and
/// [`DiscoveryError::Glob`] if a configured glob does not compile.
pub fn discover(root: &Path, settings: &DiscoverySettings) -> Result<Discovery, DiscoveryError> {
    collect(root, root, settings)
}

/// Walk only the part of `root` that lives under `directory`, making exactly
/// the decisions a walk of the whole `root` would have made.
///
/// The watcher's question, and the reason it is answered HERE rather than in
/// the daemon: a filesystem event names a directory, re-listing the whole
/// worktree per event is the cost a debouncer exists to avoid — and walking
/// that directory *as its own root* silently changes the answer. Every
/// directory-shaped refusal is decided when the walker is offered the
/// DIRECTORY entry, and a walk that starts below one is never offered it:
/// `scratch/` matches a directory named `scratch`, never the path
/// `scratch/old/notes.md`, and the hidden filter, the `.git` refusal and
/// [`DiscoverySettings::standard_ignores`] all prune the same way. Measured
/// before this existed: one event under an ignored tree pulled 886 gitignored
/// files into the index, every one of them reaped again by the next full walk.
///
/// So the walk still starts at `root` — one walker configuration, one verdict,
/// nothing to drift — and is pruned to `directory` plus the ancestors leading
/// to it. It reads the chain's own listings and the subtree, never a sibling.
///
/// `Ok(None)` means a walk from `root` would never have descended here: the
/// caller has an event for a directory fs3 does not index, which is different
/// from a directory that holds nothing worth indexing. A `directory` outside
/// `root` answers `None` for the same reason — it is not part of that walk.
///
/// Paths in the result are relative to `root`, exactly as [`discover`] reports
/// them, so a subtree result and a whole-root result key the same way.
///
/// # Errors
/// [`DiscoveryError::NotADirectory`] if `root` or `directory` is not a
/// directory, and [`DiscoveryError::Glob`] if a configured glob does not
/// compile.
pub fn discover_subtree(
    root: &Path,
    directory: &Path,
    settings: &DiscoverySettings,
) -> Result<Option<Discovery>, DiscoveryError> {
    if !directory.is_dir() {
        return Err(DiscoveryError::NotADirectory(directory.to_path_buf()));
    }
    if !descends_to(root, directory, settings)? {
        return Ok(None);
    }
    collect(root, directory, settings).map(Some)
}

/// Would a walk from `root` have reached `directory` at all?
///
/// Answered by running the real walker — depth-limited to the directory's own
/// depth and pruned to its ancestor chain — rather than by re-deciding
/// gitignore, hidden files and the deny list here. A second implementation of
/// those rules is one that can disagree with the first, which is the entire
/// defect this function exists to close.
///
/// Both passes are asked, because both can reach a directory: the default one,
/// and — only when there are `force_include` globs to justify the second walk —
/// the pass that turns ignores off to reach what the repo insisted on anyway.
/// A force-included subtree still faces the globs themselves in [`collect`], so
/// saying yes here costs a walk that finds nothing, never a file nobody asked
/// for.
fn descends_to(
    root: &Path,
    directory: &Path,
    settings: &DiscoverySettings,
) -> Result<bool, DiscoveryError> {
    if !root.is_dir() {
        return Err(DiscoveryError::NotADirectory(root.to_path_buf()));
    }
    if directory == root {
        return Ok(true);
    }
    let Ok(relative) = directory.strip_prefix(root) else {
        return Ok(false);
    };
    let depth = relative.components().count();
    Ok(reached(root, directory, depth, settings, true)
        || (!settings.force_include.is_empty() && reached(root, directory, depth, settings, false)))
}

/// One depth-limited probe along the chain: does `directory` survive to be
/// yielded as an entry?
fn reached(
    root: &Path,
    directory: &Path,
    depth: usize,
    settings: &DiscoverySettings,
    honour_ignores: bool,
) -> bool {
    // The probe never reports prunes: it walks the chain, not a tree, so the
    // only directory it could name is one the caller already has in hand.
    let ledger: Arc<Mutex<BTreeSet<String>>> = Arc::new(Mutex::new(BTreeSet::new()));
    walker(
        root,
        settings,
        honour_ignores,
        Some(directory.to_path_buf()),
        &ledger,
    )
    .max_depth(Some(depth))
    .build()
    .filter_map(Result::ok)
    .any(|entry| entry.path() == directory)
}

/// The body behind [`discover`] and [`discover_subtree`]: walk `root`,
/// optionally restricted to `directory`, and judge what comes back.
fn collect(
    root: &Path,
    directory: &Path,
    settings: &DiscoverySettings,
) -> Result<Discovery, DiscoveryError> {
    if !root.is_dir() {
        return Err(DiscoveryError::NotADirectory(root.to_path_buf()));
    }

    let exclude = matcher(root, &settings.exclude, "exclude")?;
    let force_include = matcher(root, &settings.force_include, "force_include")?;

    let mut collector = Collector {
        settings,
        exclude: &exclude,
        files: BTreeMap::new(),
        skipped: BTreeMap::new(),
    };

    // The deny list prunes whole directories, so what it refuses can never
    // appear in either file list. This is where those directories get named:
    // a `BTreeSet` because the two passes overlap and because sorted output
    // makes a discovery result comparable.
    let pruned: Arc<Mutex<BTreeSet<String>>> = Arc::new(Mutex::new(BTreeSet::new()));

    // `None` rather than `Some(root)` for a whole-root walk: the restriction
    // would answer yes for every entry, and the common case should not pay two
    // path comparisons per file to be told so.
    let within = (directory != root).then(|| directory.to_path_buf());

    walk(
        root,
        settings,
        true,
        within.clone(),
        &pruned,
        &mut |absolute, relative, bytes| {
            collector.consider(absolute, relative, bytes);
        },
    );

    // Second pass: paths the config asked for by name that the defaults
    // refuse — git-ignored, or under a `STANDARD_IGNORES` directory (a
    // vendored `vendor/` someone genuinely wants indexed). Running it as its
    // own walk (all ignores off, force globs on) keeps the common case — no
    // force-includes — at exactly one traversal, and keeps the semantics
    // legible: the first walk answers "what is visible by default", the
    // second "what did the repo insist on anyway".
    if !settings.force_include.is_empty() {
        walk(
            root,
            settings,
            false,
            within,
            &pruned,
            &mut |absolute, relative, bytes| {
                if force_include
                    .matched_path_or_any_parents(relative, false)
                    .is_ignore()
                {
                    collector.consider(absolute, relative, bytes);
                }
            },
        );
    }

    let pruned = std::mem::take(&mut *pruned.lock().expect("prune ledger is never poisoned"));

    Ok(Discovery {
        files: collector.files.into_values().collect(),
        skipped: collector
            .skipped
            .into_iter()
            .map(|(path, reason)| SkippedFile { path, reason })
            .collect(),
        pruned: pruned
            .into_iter()
            .map(|path| PrunedDirectory {
                path,
                reason: PruneReason::StandardIgnore,
            })
            .collect(),
    })
}

/// Compile gitignore-syntax globs into a matcher. A pattern that *matches*
/// reports `is_ignore()` — the crate's vocabulary, kept rather than wrapped so
/// the semantics stay the ones users already know from `.gitignore`.
fn matcher(
    root: &Path,
    globs: &[String],
    setting: &'static str,
) -> Result<Gitignore, DiscoveryError> {
    let mut builder = GitignoreBuilder::new(root);
    for glob in globs {
        builder
            .add_line(None, glob)
            .map_err(|source| DiscoveryError::Glob {
                setting,
                glob: glob.clone(),
                source,
            })?;
    }
    builder.build().map_err(|source| DiscoveryError::Glob {
        setting,
        glob: globs.join(", "),
        source,
    })
}

/// Accumulates one walk's verdicts, keyed by relative path so the two passes
/// can overlap without producing duplicates.
struct Collector<'a> {
    settings: &'a DiscoverySettings,
    exclude: &'a Gitignore,
    files: BTreeMap<String, DiscoveredFile>,
    skipped: BTreeMap<String, SkipReason>,
}

impl Collector<'_> {
    /// Judge one file. `bytes` is `None` when it could not be stat'd.
    fn consider(&mut self, absolute: &Path, relative: &Path, bytes: Option<u64>) {
        let path = display_path(relative);
        if self.files.contains_key(&path) || self.skipped.contains_key(&path) {
            return;
        }
        let excluded = self
            .exclude
            .matched_path_or_any_parents(relative, false)
            .is_ignore();

        let Some(bytes) = bytes else {
            self.skipped.insert(path, SkipReason::Unreadable);
            return;
        };

        match verdict(relative, bytes, excluded, self.settings) {
            Err(reason) => {
                self.skipped.insert(path, reason);
            }
            // The sniff is last because it is the only decision that costs a
            // file open: extension and size settle the vast majority first.
            Ok(family) => match looks_binary(absolute) {
                Ok(true) => {
                    self.skipped.insert(path, SkipReason::Binary);
                }
                Ok(false) => {
                    self.files.insert(
                        path.clone(),
                        DiscoveredFile {
                            path,
                            bytes,
                            family,
                            language: Language::for_path(relative),
                        },
                    );
                }
                Err(_) => {
                    self.skipped.insert(path, SkipReason::Unreadable);
                }
            },
        }
    }
}

/// The whole filtering policy, as a pure function: no IO, no walker, testable
/// with a `&str` and a number. Precedence, highest first: an explicit
/// `exclude`, then format, then size.
fn verdict(
    relative: &Path,
    bytes: u64,
    excluded: bool,
    settings: &DiscoverySettings,
) -> Result<LanguageFamily, SkipReason> {
    if excluded {
        return Err(SkipReason::Excluded);
    }
    let family = LanguageFamily::for_path(relative);
    if family == LanguageFamily::Unknown {
        return Err(SkipReason::UnsupportedExtension);
    }
    // A deterministic document is the one deliberate exception to the JSON
    // exclusion. Keep it ahead of that branch: both full discovery and the
    // watcher subtree path share this verdict.
    if !crate::is_ddoc_source(relative)
        && family == LanguageFamily::Config
        && !settings.index_config_formats
    {
        return Err(SkipReason::ConfigFormat);
    }
    // Discovery has paths and sizes but has not opened text yet, so empty bytes
    // deliberately select only the suffix half of the shared helper. A caller
    // that owns bytes can additionally reject a renamed projection by banner.
    if crate::is_generated_sibling(relative, b"") {
        return Err(SkipReason::GeneratedSibling);
    }
    if bytes > settings.max_file_bytes {
        return Err(SkipReason::TooLarge);
    }
    if bytes < settings.min_file_bytes {
        return Err(SkipReason::TooSmall);
    }
    Ok(family)
}

/// Visit every file under `root`, handing back `(absolute, relative, size)`.
///
/// `within` restricts the traversal to one subtree without moving the walk
/// root — see [`discover_subtree`] for why those are not the same thing.
///
/// The deny list is applied in [`walker`], as a `filter_entry` prune, rather
/// than as a verdict per file: not descending into `node_modules` is the
/// entire saving, and a pruned directory costs one string comparison instead
/// of a hundred thousand ledger rows. Each directory it refuses is recorded in
/// `pruned` — the directory, never its contents — so the absence has a name.
fn walk(
    root: &Path,
    settings: &DiscoverySettings,
    honour_ignores: bool,
    within: Option<PathBuf>,
    pruned: &Arc<Mutex<BTreeSet<String>>>,
    visit: &mut dyn FnMut(&Path, &Path, Option<u64>),
) {
    for entry in walker(root, settings, honour_ignores, within, pruned).build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                // A path-bearing error is a file fs3 saw and could not read;
                // a pathless one cannot be attributed to anything, so it has
                // nowhere honest to go in the ledger.
                if let ignore::Error::WithPath { path, .. } = &error
                    && let Ok(relative) = path.strip_prefix(root)
                {
                    visit(path, relative, None);
                }
                continue;
            }
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let bytes = entry.metadata().ok().map(|metadata| metadata.len());
        visit(entry.path(), relative, bytes);
    }
}

/// The one walker configuration in this crate.
///
/// Every traversal — the default pass, the force-include pass, and the
/// [`descends_to`] probe — is built here, so "what fs3 refuses to walk" has a
/// single answer that no caller can restate slightly differently.
///
/// `honour_ignores` off strips gitignore, `.ignore` files, parent ignores
/// **and the standard deny list** — that is the force-include pass, whose
/// whole job is to reach what the defaults refuse. The hidden filter and the
/// `.git` refusal survive both passes: nothing indexes a git object database,
/// at any setting.
fn walker(
    root: &Path,
    settings: &DiscoverySettings,
    honour_ignores: bool,
    within: Option<PathBuf>,
    pruned: &Arc<Mutex<BTreeSet<String>>>,
) -> WalkBuilder {
    let honour = honour_ignores && settings.respect_gitignore;
    // Deliberately keyed off `honour_ignores`, not `honour`: the deny list is
    // fs3's own opinion and must hold in a repo with no `.gitignore` at all —
    // which is exactly the case that motivated it.
    let denied: Vec<String> = if honour_ignores {
        settings.standard_ignores.clone()
    } else {
        Vec::new()
    };
    // Cloned into the walker's callback, which must be `Send + Sync + 'static`.
    // Contended only when a directory is actually refused — eleven times on a
    // real repository, not once per entry.
    let ledger = Arc::clone(pruned);
    let ledger_root = root.to_path_buf();
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(!settings.include_hidden)
        .parents(honour)
        .ignore(honour)
        .git_ignore(honour)
        .git_exclude(honour)
        // Never: `core.excludesFile` is per-developer, and an index that
        // depends on whose laptop ran the scan is not an index.
        .git_global(false)
        // fs3 indexes plain folders too (PRD req 23). Without this, a
        // `.gitignore` in a non-git folder would be silently inert — the same
        // tree would index differently for having been cloned.
        .require_git(false)
        .follow_links(settings.follow_symlinks)
        .filter_entry(move |entry| {
            // Depth 0 is the root the caller named. `flowspace3 add
            // ./node_modules` is a deliberate instruction, not an accident.
            if entry.depth() == 0 {
                return true;
            }
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            // The subtree restriction, before anything that needs a readable
            // name: it is pure path comparison, it is the cheapest refusal
            // here, and a directory with a non-UTF-8 name must not slip past
            // it on the way to the early return below.
            if !within
                .as_ref()
                .is_none_or(|target| on_the_way_to(entry.path(), target, is_dir))
            {
                return false;
            }
            let Some(name) = entry.file_name().to_str() else {
                return true;
            };
            // ASCII-case-insensitive, matching `fs3-daemon`'s watcher filter.
            // Case sensitivity is a property of the VOLUME, not the OS, so no
            // `cfg!` gets it right: on a case-insensitive volume `Build/` and
            // `build/` are one directory, and a case-sensitive check would
            // walk what the watcher refuses. Denying both is the strictly safer
            // half of that disagreement; the typed way out today is
            // `scan.standard_ignores = false`, since `force_include` has no
            // `[scan]` key yet.
            if name.eq_ignore_ascii_case(".git") {
                return false;
            }
            if !is_dir {
                return true;
            }
            // Whole component, never substring: `src/target_types.rs` is a
            // file (already returned above), `my-vendor/` and `builder/` are
            // directories whose NAMES simply are not on the list.
            if !denied
                .iter()
                .any(|denied| denied.eq_ignore_ascii_case(name))
            {
                return true;
            }
            // Refused — so name it. Only the directory: everything under it is
            // never visited, which is the saving this ledger must not undo.
            if let Ok(relative) = entry.path().strip_prefix(&ledger_root) {
                ledger
                    .lock()
                    .expect("prune ledger is never poisoned")
                    .insert(display_path(relative));
            }
            false
        });
    builder
}

/// Is this entry on the way to `target`, or inside it?
///
/// A directory earns the walk if it leads to the target (an ancestor) or lives
/// under it; anything else — a sibling, an uncle — is pruned unvisited, which
/// is what keeps a subtree walk from costing a whole-root one. A FILE only
/// earns it inside the target: the files sitting in the ancestor directories
/// are outside the subtree the caller asked about.
///
/// `Path::starts_with` compares components, not bytes, so `src2/x.rs` is
/// correctly not under `src`.
fn on_the_way_to(path: &Path, target: &Path, is_dir: bool) -> bool {
    path.starts_with(target) || (is_dir && target.starts_with(path))
}

/// `/`-separated relative path — the shape that survives a Windows walk and a
/// Postgres round-trip unchanged.
fn display_path(relative: &Path) -> String {
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// A NUL byte in the first [`SNIFF_BYTES`] means binary. Content decides, not
/// the extension: the PNG someone committed as `logo.md` is caught here.
fn looks_binary(path: &Path) -> std::io::Result<bool> {
    let mut head = [0u8; SNIFF_BYTES];
    let read = File::open(path)?.read(&mut head)?;
    Ok(head[..read].contains(&0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_tables_are_sorted_for_binary_search() {
        for table in [SOURCE_EXTENSIONS, DOCUMENT_EXTENSIONS, CONFIG_EXTENSIONS] {
            let mut sorted = table.to_vec();
            sorted.sort_unstable();
            assert_eq!(table, sorted.as_slice(), "table must stay sorted");
        }
    }

    #[test]
    fn a_file_with_a_grammar_is_never_unknown() {
        for extension in ["rs", "RS", "py", "pyi", "md", "markdown"] {
            assert_ne!(
                LanguageFamily::for_extension(extension),
                LanguageFamily::Unknown,
                "{extension} has a grammar but no family",
            );
        }
        assert_eq!(
            LanguageFamily::for_extension("md"),
            LanguageFamily::Document,
        );
        assert_eq!(LanguageFamily::for_extension("rs"), LanguageFamily::Source);
    }

    #[test]
    fn config_formats_are_their_own_family() {
        for extension in ["yaml", "yml", "json", "toml", "hcl", "tf"] {
            assert_eq!(
                LanguageFamily::for_extension(extension),
                LanguageFamily::Config,
                "{extension} must be excludable by PRD req 43",
            );
        }
    }

    #[test]
    fn extensionless_and_unheard_of_files_are_unknown() {
        assert_eq!(
            LanguageFamily::for_path(Path::new("Makefile")),
            LanguageFamily::Unknown,
        );
        assert_eq!(
            LanguageFamily::for_path(Path::new("a/b/logo.png")),
            LanguageFamily::Unknown,
        );
    }

    fn verdict_of(
        path: &str,
        bytes: u64,
        settings: &DiscoverySettings,
    ) -> Result<LanguageFamily, SkipReason> {
        verdict(Path::new(path), bytes, false, settings)
    }

    #[test]
    fn exclude_outranks_everything() {
        let settings = DiscoverySettings::default();
        assert_eq!(
            verdict(Path::new("src/lib.rs"), 10, true, &settings),
            Err(SkipReason::Excluded),
        );
    }

    #[test]
    fn format_is_judged_before_size() {
        let settings = DiscoverySettings {
            max_file_bytes: 4,
            ..DiscoverySettings::default()
        };
        // Both filters would fire; the cheaper, more specific reason wins so
        // the ledger says *config format*, not "your ceiling is low".
        assert_eq!(
            verdict_of("deploy/values.yaml", 4_000, &settings),
            Err(SkipReason::ConfigFormat),
        );
    }

    #[test]
    fn config_formats_index_when_the_repo_asks() {
        let settings = DiscoverySettings {
            index_config_formats: true,
            ..DiscoverySettings::default()
        };
        assert_eq!(
            verdict_of("deploy/values.yaml", 40, &settings),
            Ok(LanguageFamily::Config),
        );
    }

    #[test]
    fn the_size_window_is_inclusive_at_both_ends() {
        let settings = DiscoverySettings {
            max_file_bytes: 100,
            min_file_bytes: 10,
            ..DiscoverySettings::default()
        };
        assert_eq!(
            verdict_of("a.rs", 100, &settings),
            Ok(LanguageFamily::Source)
        );
        assert_eq!(
            verdict_of("a.rs", 10, &settings),
            Ok(LanguageFamily::Source)
        );
        assert_eq!(
            verdict_of("a.rs", 101, &settings),
            Err(SkipReason::TooLarge)
        );
        assert_eq!(verdict_of("a.rs", 9, &settings), Err(SkipReason::TooSmall));
    }

    #[test]
    fn scan_config_is_the_injection_seam() {
        let scan = ScanConfig {
            max_file_bytes: 4_096,
            min_file_bytes: 2,
            respect_gitignore: false,
            include_hidden: true,
            follow_symlinks: true,
            standard_ignores: true,
        };
        let settings = DiscoverySettings::from(&scan);
        assert_eq!(settings.max_file_bytes, 4_096);
        assert_eq!(settings.min_file_bytes, 2);
        assert!(!settings.respect_gitignore);
        assert!(settings.include_hidden);
        assert!(settings.follow_symlinks);
        assert_eq!(settings.standard_ignores, STANDARD_IGNORES);
        // The knobs `[scan]` does not carry keep their defaults.
        assert!(!settings.index_config_formats);
        assert!(settings.force_include.is_empty());
    }

    /// The TOML bool is the whole question a config file needs to answer; the
    /// list is how the walker asks it.
    #[test]
    fn the_config_bool_maps_onto_the_list() {
        let scan = ScanConfig {
            standard_ignores: false,
            ..ScanConfig::default()
        };
        assert!(DiscoverySettings::from(&scan).standard_ignores.is_empty());
        assert_eq!(
            DiscoverySettings::from(&ScanConfig::default()).standard_ignores,
            STANDARD_IGNORES,
        );
    }

    #[test]
    fn a_missing_root_is_an_error_not_an_empty_result() {
        let error = discover(
            Path::new("definitely/not/here"),
            &DiscoverySettings::default(),
        )
        .unwrap_err();
        assert!(matches!(error, DiscoveryError::NotADirectory(_)));
    }
}
