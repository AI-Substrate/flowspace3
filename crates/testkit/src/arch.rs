//! Mechanical enforcement of the crate graph (workshop 001 rule 1).
//!
//! Cargo already forbids cycles and undeclared imports. What it does not stop
//! is someone *adding* an edge the architecture refuses — `sqlx` in `core`,
//! `axum` in `parsers`, a mocking framework anywhere. This module closes that
//! gap: [`allowlist`] is the graph as data, [`check`] is the verdict.
//!
//! The check is pure over a [`Graph`], so its own failure mode is provable:
//! `testkit/tests/arch_drift.rs` runs it over a committed manifest that
//! contains a forbidden edge and asserts RED. That negative proof is
//! re-runnable — it is not a violate-and-revert ritual.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

/// Why the architecture check could not reach a verdict.
///
/// Distinct from a *violation*: this is the check failing, not the graph.
#[derive(Debug, thiserror::Error)]
pub enum ArchError {
    /// `cargo metadata` could not be run or returned a failure.
    #[error("could not run `cargo metadata`: {0}")]
    Metadata(String),
    /// The metadata JSON did not have the shape we read.
    #[error("could not parse `cargo metadata` output: {0}")]
    ParseMetadata(#[from] serde_json::Error),
    /// The allow-list is not valid TOML for this shape.
    #[error("could not parse the architecture allow-list: {0}")]
    ParseAllowlist(#[from] toml::de::Error),
}

/// Which dependency table an edge came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DepKind {
    /// `[dependencies]` — ships in the binary.
    Normal,
    /// `[dev-dependencies]` — tests, benches, examples.
    Dev,
    /// `[build-dependencies]` — build scripts.
    Build,
}

impl DepKind {
    /// The table name as Cargo spells it.
    pub const fn as_str(self) -> &'static str {
        match self {
            DepKind::Normal => "dependencies",
            DepKind::Dev => "dev-dependencies",
            DepKind::Build => "build-dependencies",
        }
    }
}

impl std::fmt::Display for DepKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One direct dependency edge.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Dep {
    /// Crate name as declared.
    pub name: String,
    /// Which table declared it.
    pub kind: DepKind,
}

/// One allow-list entry: a crate name, and the most privileged dependency
/// table it may appear in.
///
/// Spelled `"name"` for a shipped `[dependencies]` edge, `"name@dev"` for a
/// test-only one, `"name@build"` for a build script. Before this existed the
/// dev/normal distinction was carried by a TOML *comment*, so promoting a
/// dev-only edge into the shipped binary produced no violation at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// The crate this rule permits.
    pub dep: String,
    /// The most privileged table it may be declared in.
    pub kind: DepKind,
}

impl Rule {
    /// Does this rule permit an edge declared in `actual`?
    ///
    /// Privilege runs one way. An edge cleared to SHIP is also cleared to be
    /// used by tests, so `dependencies` implies `dev-dependencies`. The
    /// converse is the entire point of this dimension: a dev-only edge found in
    /// `[dependencies]` has been promoted into the binary. `build-dependencies`
    /// is a separate axis and implies nothing either way.
    pub const fn permits(&self, actual: DepKind) -> bool {
        matches!(
            (self.kind, actual),
            (DepKind::Normal, DepKind::Normal | DepKind::Dev)
                | (DepKind::Dev, DepKind::Dev)
                | (DepKind::Build, DepKind::Build)
        )
    }
}

impl<'de> Deserialize<'de> for Rule {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        let (dep, kind) = match raw.split_once('@') {
            None => (raw.as_str(), DepKind::Normal),
            Some((dep, "dev")) => (dep, DepKind::Dev),
            Some((dep, "build")) => (dep, DepKind::Build),
            Some((_, unknown)) => {
                return Err(serde::de::Error::custom(format!(
                    "allow-list entry `{raw}`: unknown dependency kind `@{unknown}` \
                     - use `@dev`, `@build`, or no suffix for a shipped [dependencies] edge"
                )));
            }
        };
        if dep.is_empty() {
            return Err(serde::de::Error::custom(format!(
                "allow-list entry `{raw}` names no crate"
            )));
        }
        Ok(Rule {
            dep: dep.to_string(),
            kind,
        })
    }
}

/// One workspace crate and its direct dependency edges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrateDeps {
    /// Package name, e.g. `fs3-core`.
    pub name: String,
    /// Direct edges across every dependency table.
    pub deps: Vec<Dep>,
}

/// The workspace's direct-dependency graph — everything the check reasons over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Graph {
    /// Workspace members only. Transitive dependencies are not this check's job.
    pub crates: Vec<CrateDeps>,
}

impl Graph {
    /// Names of the crates in this workspace — used to tell an internal edge
    /// from an external one without hard-coding a prefix.
    pub fn member_names(&self) -> BTreeSet<&str> {
        self.crates.iter().map(|c| c.name.as_str()).collect()
    }

    /// Build a graph from `cargo metadata --no-deps --format-version 1` output.
    ///
    /// # Errors
    /// [`ArchError::ParseMetadata`] when the JSON is not cargo metadata.
    pub fn from_cargo_metadata(json: &str) -> Result<Self, ArchError> {
        let metadata: Metadata = serde_json::from_str(json)?;
        let members: BTreeSet<&str> = metadata
            .workspace_members
            .iter()
            .map(String::as_str)
            .collect();

        let mut crates: Vec<CrateDeps> = metadata
            .packages
            .iter()
            .filter(|package| members.contains(package.id.as_str()))
            .map(|package| {
                let mut deps: Vec<Dep> = package
                    .dependencies
                    .iter()
                    .map(|dependency| Dep {
                        name: dependency.name.clone(),
                        kind: match dependency.kind.as_deref() {
                            Some("dev") => DepKind::Dev,
                            Some("build") => DepKind::Build,
                            _ => DepKind::Normal,
                        },
                    })
                    .collect();
                deps.sort();
                deps.dedup();
                CrateDeps {
                    name: package.name.clone(),
                    deps,
                }
            })
            .collect();
        crates.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Graph { crates })
    }
}

/// The architecture, as data. See `testkit/arch-allowlist.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allowlist {
    /// Crates refused in every table of every crate.
    #[serde(default)]
    pub banned_everywhere: Vec<String>,
    /// Per-crate allowed edges, keyed by package name.
    pub crates: BTreeMap<String, CrateRules>,
}

/// The edges one crate is allowed to have.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrateRules {
    /// Allowed workspace-internal edges.
    #[serde(default)]
    pub internal: Vec<Rule>,
    /// Allowed direct external edges.
    #[serde(default)]
    pub external: Vec<Rule>,
}

impl CrateRules {
    /// The rule covering `dep`, if the allow-list carries one at all.
    fn rule_for(&self, dep: &str, internal: bool) -> Option<&Rule> {
        let table = if internal {
            &self.internal
        } else {
            &self.external
        };
        table.iter().find(|rule| rule.dep == dep)
    }
}

/// A refused edge, or an allow-list that has fallen out of step with reality.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    /// A workspace crate the allow-list does not describe.
    UndescribedCrate {
        /// The workspace crate with no allow-list entry.
        crate_name: String,
    },
    /// The allow-list describes a crate the workspace no longer has.
    StaleAllowlistEntry {
        /// The allow-list entry with no matching workspace crate.
        crate_name: String,
    },
    /// A workspace-internal edge the architecture refuses.
    ForbiddenInternal {
        /// The crate that declared the edge.
        crate_name: String,
        /// The workspace crate it depends on.
        dep: String,
        /// Which table declared it.
        kind: DepKind,
    },
    /// An external edge the crate's allow-list does not carry.
    ForbiddenExternal {
        /// The crate that declared the edge.
        crate_name: String,
        /// The external crate it depends on.
        dep: String,
        /// Which table declared it.
        kind: DepKind,
    },
    /// A crate refused workspace-wide.
    BannedEverywhere {
        /// The crate that declared the edge.
        crate_name: String,
        /// The refused crate.
        dep: String,
        /// Which table declared it.
        kind: DepKind,
    },
    /// An allowed edge, declared in a table the architecture does not allow it
    /// in — a dev-only dependency promoted into the shipped binary, typically.
    WrongDependencyKind {
        /// The crate that declared the edge.
        crate_name: String,
        /// The crate it depends on.
        dep: String,
        /// The table it was actually declared in.
        kind: DepKind,
        /// The most privileged table the allow-list permits.
        allowed: DepKind,
    },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::UndescribedCrate { crate_name } => write!(
                f,
                "{crate_name}: workspace member has no entry in testkit/arch-allowlist.toml \
                 - add one describing the edges it is allowed to have"
            ),
            Violation::StaleAllowlistEntry { crate_name } => write!(
                f,
                "{crate_name}: named in testkit/arch-allowlist.toml but absent from the \
                 workspace - remove the entry or restore the crate"
            ),
            Violation::ForbiddenInternal {
                crate_name,
                dep,
                kind,
            } => write!(
                f,
                "{crate_name} -> {dep} ({kind}): crate-graph edge refused by workshop 001. \
                 The dependency direction is core <- everything, daemon -> all."
            ),
            Violation::ForbiddenExternal {
                crate_name,
                dep,
                kind,
            } => write!(
                f,
                "{crate_name} -> {dep} ({kind}): not in this crate's allow-list. If the edge \
                 is genuinely architectural, add `{dep}` to [crates.{crate_name}].external in \
                 testkit/arch-allowlist.toml; if it is not, put the code in the crate that owns \
                 the concern."
            ),
            Violation::BannedEverywhere {
                crate_name,
                dep,
                kind,
            } => write!(
                f,
                "{crate_name} -> {dep} ({kind}): refused workspace-wide. Workshop 001 rule 5 - \
                 fakes over mocks; doubles come from fs3-testkit, never a mocking framework."
            ),
            Violation::WrongDependencyKind {
                crate_name,
                dep,
                kind,
                allowed,
            } => write!(
                f,
                "{crate_name} -> {dep}: declared in [{kind}] but the allow-list permits it \
                 only in [{allowed}]. {advice}",
                advice = kind_advice(dep, *kind, *allowed)
            ),
        }
    }
}

/// The actionable half of a [`Violation::WrongDependencyKind`] message.
///
/// Rendered from the ACTUAL/ALLOWED pair, never from `allowed` alone. Reading
/// `allowed` alone produced advice an agent cannot follow: a dep allow-listed
/// to ship but found in `[build-dependencies]` was told to change
/// `serde@dependencies` to `serde` — `@dependencies` is not a suffix the
/// allow-list has, and `serde` is exactly what the rule already said. A
/// diagnostic that sends someone to edit a line that is already correct is
/// worse than one that says nothing, because it costs a round trip to disbelieve.
fn kind_advice(dep: &str, actual: DepKind, allowed: DepKind) -> String {
    match (actual, allowed) {
        // The dangerous direction, and the one this dimension was built for:
        // something cleared only for tests or build scripts now ships.
        (DepKind::Normal, DepKind::Dev) => format!(
            "A dev-only edge that gets promoted ships in the binary; if the promotion \
             is deliberate, change `{dep}@dev` to `{dep}` in testkit/arch-allowlist.toml \
             and say why in the review."
        ),
        (DepKind::Normal, DepKind::Build) => format!(
            "A build-script edge that gets promoted ships in the binary; if the \
             promotion is deliberate, change `{dep}@build` to `{dep}` in \
             testkit/arch-allowlist.toml and say why in the review."
        ),
        // Build scripts are a separate axis, so no other rule spelling permits
        // this edge — `@build` or nothing.
        (DepKind::Build, _) => format!(
            "If the build-script edge is intentional, write `{dep}@build` in \
             testkit/arch-allowlist.toml; if it is not, move {dep} out of \
             [build-dependencies]."
        ),
        (DepKind::Dev, _) => format!(
            "If the test-only edge is intentional, write `{dep}@dev` in \
             testkit/arch-allowlist.toml; if it is not, move {dep} out of \
             [dev-dependencies]."
        ),
        // `check` never builds this pair — a shipped rule permits a shipped
        // edge — so say the true general thing rather than invent a suffix.
        (DepKind::Normal, DepKind::Normal) => format!(
            "Reconcile `{dep}` in testkit/arch-allowlist.toml with the table it is \
             declared in."
        ),
    }
}

/// The allow-list compiled into the binary, so the check works from any cwd.
///
/// # Errors
/// [`ArchError::ParseAllowlist`] when the committed allow-list is malformed.
pub fn allowlist() -> Result<Allowlist, ArchError> {
    Ok(toml::from_str(include_str!("../arch-allowlist.toml"))?)
}

/// Judge a graph against an allow-list. Empty result means no drift.
///
/// Pure — which is what makes the negative proof re-runnable.
pub fn check(graph: &Graph, allowlist: &Allowlist) -> Vec<Violation> {
    let members = graph.member_names();
    let banned: BTreeSet<&str> = allowlist
        .banned_everywhere
        .iter()
        .map(String::as_str)
        .collect();

    let mut violations = Vec::new();

    for krate in &graph.crates {
        let Some(rules) = allowlist.crates.get(&krate.name) else {
            violations.push(Violation::UndescribedCrate {
                crate_name: krate.name.clone(),
            });
            continue;
        };

        for dep in &krate.deps {
            let crate_name = krate.name.clone();
            let dep_name = dep.name.clone();

            if banned.contains(dep.name.as_str()) {
                violations.push(Violation::BannedEverywhere {
                    crate_name,
                    dep: dep_name,
                    kind: dep.kind,
                });
            } else if members.contains(dep.name.as_str()) {
                match rules.rule_for(&dep.name, true) {
                    None => violations.push(Violation::ForbiddenInternal {
                        crate_name,
                        dep: dep_name,
                        kind: dep.kind,
                    }),
                    Some(rule) if !rule.permits(dep.kind) => {
                        violations.push(Violation::WrongDependencyKind {
                            crate_name,
                            dep: dep_name,
                            kind: dep.kind,
                            allowed: rule.kind,
                        });
                    }
                    Some(_) => {}
                }
            } else {
                match rules.rule_for(&dep.name, false) {
                    None => violations.push(Violation::ForbiddenExternal {
                        crate_name,
                        dep: dep_name,
                        kind: dep.kind,
                    }),
                    Some(rule) if !rule.permits(dep.kind) => {
                        violations.push(Violation::WrongDependencyKind {
                            crate_name,
                            dep: dep_name,
                            kind: dep.kind,
                            allowed: rule.kind,
                        });
                    }
                    Some(_) => {}
                }
            }
        }
    }

    // An allow-list that outlives its crate silently permits nothing and hides
    // a rename, so it is drift too.
    for name in allowlist.crates.keys() {
        if !members.contains(name.as_str()) {
            violations.push(Violation::StaleAllowlistEntry {
                crate_name: name.clone(),
            });
        }
    }

    violations
}

/// Absolute path to this workspace's root manifest, found by walking up from
/// this crate's manifest dir until a manifest declaring `[workspace]` appears,
/// so the check never depends on the caller's working directory or on how deep
/// in the tree this crate lives.
pub fn workspace_manifest_path() -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for ancestor in dir.ancestors().skip(1) {
        let candidate = ancestor.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&candidate)
            && text.contains("[workspace]")
        {
            return candidate;
        }
    }
    // Fall back to the historical sibling assumption; `cargo metadata` will
    // name the missing path in its error if this is wrong.
    dir.join("..").join("Cargo.toml")
}

/// Read the live workspace graph by shelling out to `cargo metadata`.
///
/// # Errors
/// [`ArchError::Metadata`] when cargo cannot be run or exits non-zero;
/// [`ArchError::ParseMetadata`] when its output is unreadable.
pub fn workspace_graph() -> Result<Graph, ArchError> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .arg("--manifest-path")
        .arg(workspace_manifest_path())
        .output()
        .map_err(|e| ArchError::Metadata(e.to_string()))?;

    if !output.status.success() {
        return Err(ArchError::Metadata(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Graph::from_cargo_metadata(&String::from_utf8_lossy(&output.stdout))
}

// --- the slice of `cargo metadata` this check reads -------------------------

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<MetaPackage>,
    workspace_members: Vec<String>,
}

#[derive(Deserialize)]
struct MetaPackage {
    name: String,
    id: String,
    #[serde(default)]
    dependencies: Vec<MetaDependency>,
}

#[derive(Deserialize)]
struct MetaDependency {
    name: String,
    #[serde(default)]
    kind: Option<String>,
}
