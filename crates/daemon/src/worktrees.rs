//! Detects linked git worktrees appearing and disappearing on disk.
//!
//! Git owns linked-worktree membership, so each scheduled pass asks git once per
//! registered repository and diffs that answer against the store. Only an
//! appeared path reaches [`crate::roots::add_root`]; an unchanged path never
//! buys another walk or emits another queue row. A vanished path reaches the
//! existing [`crate::remove::remove`] path and leaves reclamation to GC.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use fs3_store::RegisteredWorktree;

use crate::reconcile::{Pass, Reconcile};
use crate::wiring::AppState;

/// Two observations keep a transient `ENOENT` from unregistering paid content.
const MISSING_PASSES_BEFORE_REMOVE: u8 = 2;

/// Keeps the store's registered worktrees aligned with live linked worktrees.
pub struct WorktreeSupervisor {
    state: AppState,
    every_ticks: u32,
    ticks: u32,
    missing: BTreeMap<PathBuf, u8>,
}

impl WorktreeSupervisor {
    /// Build the supervisor from the wired state.
    #[must_use]
    pub fn new(state: AppState) -> Self {
        let every_ticks = state.config.indexing.worktree_reconcile_ticks;
        Self {
            state,
            every_ticks,
            // The first runner tick is boot reconciliation, not a wait period.
            ticks: every_ticks.saturating_sub(1),
            missing: BTreeMap::new(),
        }
    }

    fn due(&mut self) -> bool {
        tick_due(self.every_ticks, &mut self.ticks)
    }
}
fn tick_due(every: u32, elapsed: &mut u32) -> bool {
    if every == 0 {
        return false;
    }
    *elapsed = elapsed.saturating_add(1);
    if *elapsed < every {
        return false;
    }
    *elapsed = 0;
    true
}

#[async_trait::async_trait]
impl Reconcile for WorktreeSupervisor {
    fn name(&self) -> &'static str {
        "worktrees"
    }

    async fn reconcile(&mut self) -> Result<Pass> {
        if !self.due() {
            return Ok(Pass::QUIET);
        }

        let registered = fs3_store::list_worktrees(&self.state.db).await?;
        let plan = plan_reconciliation(
            &registered,
            &mut self.missing,
            git_worktrees,
            Path::try_exists,
        )?;
        let mut changed = 0;

        for root in plan.register {
            let report = crate::roots::add_root_with_priority(
                &self.state,
                &root,
                fs3_store::JOB_PRIORITY_NEW_WORKTREE_SCAN,
            )
            .await?;
            tracing::info!(
                root = %report.root_path,
                files = report.files,
                enqueued = report.enqueued,
                "registered a newly discovered git worktree"
            );
            changed += 1;
        }

        for root in plan.remove {
            let path = root.to_string_lossy().into_owned();
            let report = crate::remove::remove(&self.state, &path)
                .await
                .map_err(|failure| anyhow!("{}: {}", failure.code, failure.message))?;
            if report.was_registered {
                self.missing.remove(&root);
                tracing::info!(root = %path, "unregistered a vanished git worktree");
                changed += 1;
            }
        }

        Ok(Pass::changed(changed))
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct WorktreePlan {
    register: Vec<PathBuf>,
    remove: Vec<PathBuf>,
}

/// Build one diff without touching the store or queue.
fn plan_reconciliation<E, X>(
    registered: &[RegisteredWorktree],
    missing: &mut BTreeMap<PathBuf, u8>,
    mut enumerate: E,
    mut exists: X,
) -> Result<WorktreePlan>
where
    E: FnMut(&Path) -> Result<Vec<PathBuf>>,
    X: FnMut(&Path) -> std::io::Result<bool>,
{
    let registered_paths: BTreeSet<PathBuf> = registered
        .iter()
        .map(|worktree| PathBuf::from(&worktree.root_path))
        .collect();
    let mut anchors: BTreeMap<&str, Vec<PathBuf>> = BTreeMap::new();
    let mut remove = Vec::new();

    for worktree in registered {
        let root = PathBuf::from(&worktree.root_path);
        match exists(&root) {
            Ok(true) => {
                missing.remove(&root);
                anchors
                    .entry(worktree.identity.as_str())
                    .or_default()
                    .push(root);
            }
            Ok(false) => {
                let observations = missing.entry(root.clone()).or_default();
                *observations = observations.saturating_add(1);
                if *observations >= MISSING_PASSES_BEFORE_REMOVE {
                    remove.push(root);
                }
            }
            Err(error) => {
                // Ambiguity breaks consecutiveness. Keeping the previous
                // observation would turn false -> unknown -> false into two
                // absences and could unregister content during a remount.
                missing.remove(&root);
                return Err(error)
                    .with_context(|| format!("checking registered root {}", root.display()));
            }
        }
    }

    // Forget paths another actor already unregistered while this supervisor was
    // waiting for its second absence observation.
    missing.retain(|root, _| registered_paths.contains(root));

    let mut register = BTreeMap::new();
    for roots in anchors.values() {
        let anchor = &roots[0];
        for candidate in enumerate(anchor)? {
            if !exists(&candidate)
                .with_context(|| format!("checking discovered root {}", candidate.display()))?
            {
                continue;
            }
            let candidate = candidate
                .canonicalize()
                .with_context(|| format!("resolving discovered root {}", candidate.display()))?;

            // A registered subdirectory already represents this checkout. Do
            // not silently widen it to the checkout root; only sibling
            // worktrees are additions.
            let checkout_is_registered = roots.iter().any(|root| root.starts_with(&candidate));
            if !checkout_is_registered && !registered_paths.contains(&candidate) {
                register.insert(candidate.clone(), checkout_created_at(&candidate)?);
            }
        }
    }

    Ok(WorktreePlan {
        register: newest_first(register.into_iter().collect()),
        remove,
    })
}

/// Prefer filesystem birth time for the checkout marker. Git's porcelain has
/// no checkout-creation timestamp. On platforms without birth time, a linked
/// worktree's `.git` marker-file mtime is the closest stable signal; the main
/// checkout's mutable `.git` directory is conservatively treated as oldest.
fn checkout_created_at(root: &Path) -> Result<SystemTime> {
    let marker = root.join(".git");
    let metadata = marker
        .metadata()
        .with_context(|| format!("reading checkout marker {}", marker.display()))?;
    match metadata.created() {
        Ok(created) => Ok(created),
        Err(_) if metadata.is_file() => metadata
            .modified()
            .with_context(|| format!("reading checkout marker time {}", marker.display())),
        Err(_) => Ok(UNIX_EPOCH),
    }
}

fn newest_first(mut appeared: Vec<(PathBuf, SystemTime)>) -> Vec<PathBuf> {
    appeared.sort_by(|(left_path, left_time), (right_path, right_time)| {
        right_time
            .cmp(left_time)
            .then_with(|| left_path.cmp(right_path))
    });
    appeared.into_iter().map(|(path, _)| path).collect()
}

/// Ask git for the linked worktrees belonging to `anchor`'s repository.
fn git_worktrees(anchor: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(anchor)
        .args(["worktree", "list", "--porcelain", "-z"])
        .env("LC_ALL", "C")
        .output()
        .with_context(|| format!("running git worktree list from {}", anchor.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            // Plain folders are legal registered roots and have no siblings for
            // git to discover.
            return Ok(Vec::new());
        }
        bail!(
            "git worktree list failed from {} ({}): {}",
            anchor.display(),
            output.status,
            stderr.trim()
        );
    }

    Ok(parse_porcelain(&output.stdout))
}

fn parse_porcelain(output: &[u8]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut current = None;
    let mut bare = false;

    for field in output.split(|byte| *byte == 0) {
        if let Some(path) = field.strip_prefix(b"worktree ") {
            if let Some(previous) = current.take()
                && !bare
            {
                paths.push(previous);
            }
            current = std::str::from_utf8(path).ok().map(PathBuf::from);
            bare = false;
        } else if field == b"bare" {
            bare = true;
        }
    }
    if let Some(path) = current
        && !bare
    {
        paths.push(path);
    }
    paths
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io;
    use std::time::Duration;

    use super::*;

    fn registered(identity: &str, root: &Path) -> RegisteredWorktree {
        RegisteredWorktree {
            id: 1,
            identity: identity.to_string(),
            root_path: root.display().to_string(),
            ref_name: None,
            include_hidden: false,
            file_count: 0,
        }
    }

    #[test]
    fn porcelain_is_nul_safe_and_excludes_bare_repositories() {
        let parsed = parse_porcelain(
            b"worktree /code/main tree\0HEAD abc\0branch refs/heads/main\0\0worktree /code/linked\0HEAD def\0locked reason\0\0worktree /code/bare.git\0bare\0\0",
        );
        assert_eq!(
            parsed,
            vec![
                PathBuf::from("/code/main tree"),
                PathBuf::from("/code/linked")
            ]
        );
    }

    #[test]
    fn cadence_runs_at_boot_then_every_six_ticks() {
        let mut elapsed = 5;
        assert!(tick_due(6, &mut elapsed));
        for _ in 0..5 {
            assert!(!tick_due(6, &mut elapsed));
        }
        assert!(tick_due(6, &mut elapsed));
    }

    #[test]
    fn zero_ticks_disables_reconciliation() {
        let mut elapsed = 0;
        assert!(!tick_due(0, &mut elapsed));
        assert!(!tick_due(0, &mut elapsed));
    }

    #[test]
    fn unchanged_worktrees_produce_an_empty_plan() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let rows = [registered("git:example/repo", &root)];
        let calls = Cell::new(0);
        let plan = plan_reconciliation(
            &rows,
            &mut BTreeMap::new(),
            |_| {
                calls.set(calls.get() + 1);
                Ok(vec![root.clone()])
            },
            |_| Ok(true),
        )
        .unwrap();

        assert_eq!(plan, WorktreePlan::default());
        assert_eq!(
            calls.get(),
            1,
            "enumerate once per repository, not per root"
        );
    }

    #[test]
    fn a_new_sibling_is_registered_once() {
        let temp = tempfile::tempdir().unwrap();
        let main = temp.path().join("main");
        let linked = temp.path().join("linked");
        std::fs::create_dir_all(&main).unwrap();
        std::fs::create_dir_all(&linked).unwrap();
        std::fs::write(linked.join(".git"), "gitdir: /tmp/admin\n").unwrap();
        let main = main.canonicalize().unwrap();
        let linked = linked.canonicalize().unwrap();
        let rows = [registered("git:example/repo", &main)];

        let plan = plan_reconciliation(
            &rows,
            &mut BTreeMap::new(),
            |_| Ok(vec![main.clone(), linked.clone(), linked.clone()]),
            |_| Ok(true),
        )
        .unwrap();

        assert_eq!(plan.register, vec![linked]);
        assert!(plan.remove.is_empty());
    }

    #[test]
    fn appeared_worktrees_are_newest_first_with_a_stable_tie_break() {
        let oldest = UNIX_EPOCH + Duration::from_secs(1);
        let newest = UNIX_EPOCH + Duration::from_secs(2);
        let appeared_oldest_first = vec![
            (PathBuf::from("/old"), oldest),
            (PathBuf::from("/new-z"), newest),
            (PathBuf::from("/new-a"), newest),
        ];

        assert_eq!(
            newest_first(appeared_oldest_first),
            vec![
                PathBuf::from("/new-a"),
                PathBuf::from("/new-z"),
                PathBuf::from("/old"),
            ]
        );
    }

    #[test]
    fn absence_requires_two_consecutive_passes() {
        let root = PathBuf::from("/missing/worktree");
        let rows = [registered("git:example/repo", &root)];
        let mut missing = BTreeMap::new();

        let first =
            plan_reconciliation(&rows, &mut missing, |_| Ok(Vec::new()), |_| Ok(false)).unwrap();
        assert!(first.remove.is_empty());

        let second =
            plan_reconciliation(&rows, &mut missing, |_| Ok(Vec::new()), |_| Ok(false)).unwrap();
        assert_eq!(second.remove, vec![root]);
    }

    #[test]
    fn a_present_pass_clears_the_absence_streak() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let rows = [registered("git:example/repo", &root)];
        let mut missing = BTreeMap::from([(root.clone(), 1)]);

        let present = plan_reconciliation(
            &rows,
            &mut missing,
            |_| Ok(vec![root.clone()]),
            |_| Ok(true),
        )
        .unwrap();
        assert!(present.remove.is_empty());
        assert!(missing.is_empty());
    }

    #[test]
    fn an_unreachable_path_fails_instead_of_being_reaped() {
        let root = PathBuf::from("/unreachable/worktree");
        let rows = [registered("git:example/repo", &root)];
        let error = plan_reconciliation(
            &rows,
            &mut BTreeMap::new(),
            |_| Ok(Vec::new()),
            |_| Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        )
        .unwrap_err();
        assert!(error.to_string().contains("checking registered root"));
    }

    #[test]
    fn an_error_between_absences_resets_the_streak() {
        let root = PathBuf::from("/intermittent/worktree");
        let rows = [registered("git:example/repo", &root)];
        let mut missing = BTreeMap::new();

        let first =
            plan_reconciliation(&rows, &mut missing, |_| Ok(Vec::new()), |_| Ok(false)).unwrap();
        assert!(first.remove.is_empty());

        let ambiguous = plan_reconciliation(
            &rows,
            &mut missing,
            |_| Ok(Vec::new()),
            |_| Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        );
        assert!(ambiguous.is_err());
        assert!(
            missing.is_empty(),
            "an unknown observation breaks the streak"
        );

        let after_error =
            plan_reconciliation(&rows, &mut missing, |_| Ok(Vec::new()), |_| Ok(false)).unwrap();
        assert!(after_error.remove.is_empty());
        assert_eq!(missing.get(&root), Some(&1));
    }

    #[test]
    fn a_registered_subdirectory_does_not_widen_to_its_checkout_root() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().canonicalize().unwrap();
        let subdir = checkout.join("src");
        std::fs::create_dir(&subdir).unwrap();
        let rows = [registered("git:example/repo", &subdir)];

        let plan = plan_reconciliation(
            &rows,
            &mut BTreeMap::new(),
            |_| Ok(vec![checkout.clone()]),
            |_| Ok(true),
        )
        .unwrap();
        assert!(plan.register.is_empty());
    }
}
