//! Which repository a query is about (workshop 003 D6).
//!
//! A bare `search` scopes to the repository the caller is standing in. The
//! principle is least surprise: someone in a checkout asking "where is the
//! retry policy" means THIS code, and answering from a different repository is
//! a confident, unmarked lie. `--repo all` widens; `--repo <identity>` picks.
//!
//! # Why the caller's directory has to come over the wire
//!
//! The daemon has its own working directory and it is never the caller's. This
//! is the same trap the CLI already closes for `add` by sending an absolute
//! path: the CLI knows where the user is standing and the daemon does not. So
//! `cwd` is a request parameter, and a request without one simply gets the
//! unscoped behaviour rather than a wrong scope.
//!
//! # The two different misses, said differently
//!
//! Not being in a registered worktree has two causes that need two answers:
//!
//! * the caller is in a checkout of a repository that IS indexed from another
//!   root (a second worktree, a fresh clone). Scoping to that identity is what
//!   they meant — but the content answering them came from the other checkout,
//!   and saying so is the difference between an answer and a puzzle.
//! * the caller is somewhere fs3 has never been told about. Then the honest
//!   answer is every repository plus the command that would fix it.
//!
//! Silence on either was measured as a real confusion: a search run from an
//! unregistered flowspace3 worktree answered entirely from an unrelated older
//! index, with nothing in the envelope saying the current repository was
//! absent.

use std::path::Path;

use fs3_core::IdentitySource;
use serde::Serialize;

use crate::wiring::AppState;

/// The `--repo` value that widens a query back to every repository.
pub const ALL: &str = "all";

/// How the repository filter was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeSource {
    /// The caller named a repository.
    Flag,
    /// Derived from the caller's working directory (D6).
    Cwd,
    /// Every repository — asked for, or nothing better was known.
    All,
}

/// What a query is scoped to, and why.
///
/// Serialised into `meta.scope` on every answer, so the scope is never
/// something a consumer has to infer from which results turned up.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Scope {
    /// The repository identity to filter by, or `None` for all of them.
    pub repo: Option<String>,
    /// How [`Scope::repo`] was chosen.
    pub source: ScopeSource,
    /// The caller's working directory, when it sent one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// The registered worktree root the caller is inside, when they are.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// What the caller should know about this scope. Empty is the healthy case.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl Scope {
    /// Every repository, with nothing to say about it — what a caller that
    /// named no repository and sent no working directory gets.
    #[must_use]
    pub fn unscoped() -> Self {
        Scope::everything(None)
    }

    /// Every repository, remembering where the caller was standing.
    fn everything(cwd: Option<&str>) -> Self {
        Scope {
            repo: None,
            source: ScopeSource::All,
            cwd: cwd.map(ToString::to_string),
            worktree: None,
            warnings: Vec::new(),
        }
    }

    #[must_use]
    fn warn(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}

/// Fold a scope warning into the agent steer.
///
/// `meta` is the right home for a fact about the answer, and workshop 004 says
/// a consumer that ignores `meta` still works. That is exactly why a warning
/// cannot live there ALONE: the miss this exists to report — a search answered
/// entirely from a repository the caller was not in — is invisible in `data`,
/// and a consumer reading only `data` and `next_action` would never learn of
/// it. So the warning leads the steer, and `meta.scope` keeps the structured
/// copy.
#[must_use]
pub fn steer(scope: &Scope, next: &str) -> String {
    match scope.warnings.first() {
        Some(warning) => format!("{warning} — then: {next}"),
        None => next.to_string(),
    }
}

/// Decide what one query is about.
///
/// Never fails: a store that cannot answer "which worktree is this" turns into
/// the unscoped behaviour plus a warning, because refusing a search over a
/// scoping question would trade a working answer for a broken one.
pub async fn resolve(state: &AppState, repo: Option<&str>, cwd: Option<&str>) -> Scope {
    let identities = fs3_store::repo_identities(&state.db)
        .await
        .unwrap_or_default();

    if let Some(named) = repo.map(str::trim).filter(|value| !value.is_empty()) {
        if named.eq_ignore_ascii_case(ALL) {
            return Scope::everything(cwd);
        }
        let scope = Scope {
            repo: Some(named.to_string()),
            source: ScopeSource::Flag,
            cwd: cwd.map(ToString::to_string),
            worktree: None,
            warnings: Vec::new(),
        };
        if identities.iter().any(|identity| identity == named) {
            return scope;
        }
        return scope.warn(format!(
            "no repository with identity {named:?} is indexed, so this answers from nothing — \
             `flowspace3 status` lists the roots that are registered"
        ));
    }

    let Some(cwd) = cwd.map(str::trim).filter(|value| !value.is_empty()) else {
        return Scope::everything(None);
    };

    // The common case: the caller is inside a root that was added.
    match fs3_store::worktree_containing(&state.db, cwd).await {
        Ok(Some(worktree)) => {
            return Scope {
                repo: Some(worktree.identity),
                source: ScopeSource::Cwd,
                cwd: Some(cwd.to_string()),
                worktree: Some(worktree.root_path),
                warnings: Vec::new(),
            };
        }
        Ok(None) => {}
        Err(error) => {
            return Scope::everything(Some(cwd)).warn(format!(
                "could not tell which repository {cwd} belongs to ({error}), so this searched \
                 every indexed repository"
            ));
        }
    }

    // Not in a registered root. Is it a checkout of something fs3 does know?
    let Ok(identity) = fs3_git::repo_identity(Path::new(cwd)) else {
        return Scope::everything(Some(cwd)).warn(format!(
            "{cwd} is not inside any registered root, so this searched every indexed repository \
             — index it with `flowspace3 add {cwd}`"
        ));
    };
    let key = identity.key().to_string();

    if !identities.contains(&key) {
        // A folder with no remote gets a PATH identity, and calling that "a
        // checkout of path:/tmp/thing" is a sentence that teaches nobody
        // anything. Only a remote-derived identity is worth naming, because
        // only that one could plausibly be indexed from somewhere else.
        let what = match identity.source() {
            IdentitySource::Remote => format!("{cwd} is a checkout of {key}, which is not indexed"),
            IdentitySource::Path => format!("{cwd} is not indexed"),
        };
        return Scope::everything(Some(cwd)).warn(format!(
            "{what}, so this answered from every OTHER indexed repository — index it with \
             `flowspace3 add {cwd}`"
        ));
    }

    let elsewhere = roots_of(state, &key).await;
    Scope {
        repo: Some(key.clone()),
        source: ScopeSource::Cwd,
        cwd: Some(cwd.to_string()),
        worktree: None,
        warnings: vec![format!(
            "this checkout ({cwd}) is not registered; {key} is indexed from {elsewhere}, so the \
             content answering you is that checkout's — `flowspace3 add {cwd}` indexes this one \
             as well"
        )],
    }
}

/// The registered roots of one repository, rendered for a message.
async fn roots_of(state: &AppState, identity: &str) -> String {
    let roots: Vec<String> = fs3_store::list_worktrees(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|worktree| worktree.identity == identity)
        .map(|worktree| worktree.root_path)
        .collect();

    match roots.len() {
        0 => "another checkout".to_string(),
        _ => roots.join(", "),
    }
}
