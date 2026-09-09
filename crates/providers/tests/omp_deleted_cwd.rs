//! Read-time path regression: all aliases are created by this test, so Linux CI
//! exercises the same failure mechanism as macOS /tmp and /var aliases.
#![cfg(unix)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fs3_core::{ConversationSource, Harness, IngestInput};
use fs3_providers::conversation_sources::omp::OmpSource;

struct Cleanup {
    root: PathBuf,
    tmpdir: Option<OsString>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        // SAFETY: this binary has one synchronous test and starts no threads.
        unsafe {
            match &self.tmpdir {
                Some(value) => std::env::set_var("TMPDIR", value),
                None => std::env::remove_var("TMPDIR"),
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn recorded_directory_shapes_resolve_after_each_cwd_is_deleted() {
    // The platform difference is fixture data, not a skipped test. Every case
    // below runs on both Linux CI and macOS. No fs3 helper constructs expected
    // names; even the fixed scratch-prefix spelling is literal TSV data.
    let parent = if cfg!(target_os = "macos") {
        "/private/tmp"
    } else {
        "/tmp"
    };
    assert_eq!(
        std::fs::canonicalize(parent).unwrap(),
        PathBuf::from(parent)
    );
    let namespace = format!(
        "fs3-omp-deleted-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    assert!(namespace.starts_with(|character: char| character.is_ascii_alphanumeric()));
    assert!(namespace.ends_with(|character: char| character.is_ascii_alphanumeric()));
    assert!(
        namespace
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    );
    let root = PathBuf::from(parent).join(&namespace);
    std::fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup {
        root: root.clone(),
        tmpdir: std::env::var_os("TMPDIR"),
    };
    let mut branches = [0; 3];
    let fixture = std::fs::read_to_string(
        fs3_testkit::expectations::fixtures_root().join("omp-directory-shapes.tsv"),
    )
    .unwrap();

    // Curated synthetic data preserves the independently observed shapes of
    // review-122-oracle-pairs.tsv. That private 173-pair map is NOT vendored.
    for (index, line) in fixture
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .enumerate()
    {
        let columns: Vec<_> = line.split('\t').collect();
        assert_eq!(columns.len(), 5, "fixture row {index}");
        let name = columns[0];
        let case = root.join(name);
        let home = case.join("home");
        let temporary = case.join("temp");
        let absolute = case.join("tmp");
        for target in ["physical/home", "physical/temp", "private/tmp"] {
            std::fs::create_dir_all(case.join(target)).unwrap();
        }
        std::os::unix::fs::symlink(case.join("physical/home"), &home).unwrap();
        std::os::unix::fs::symlink(case.join("physical/temp"), &temporary).unwrap();
        std::os::unix::fs::symlink(case.join("private/tmp"), &absolute).unwrap();
        // SAFETY: one synchronous test, no worker/runtime threads or siblings.
        unsafe { std::env::set_var("TMPDIR", &temporary) };
        let branch = match columns[1] {
            "home" => {
                branches[0] += 1;
                &home
            }
            "temp" => {
                branches[1] += 1;
                &temporary
            }
            "absolute" => {
                branches[2] += 1;
                &absolute
            }
            other => panic!("unknown fixture branch {other}"),
        };
        let cwd = branch.join(columns[2]);
        std::fs::create_dir_all(&cwd).unwrap();
        let physical_cwd = std::fs::canonicalize(&cwd).unwrap();
        let expected =
            columns[if cfg!(target_os = "macos") { 4 } else { 3 }].replace("{NS}", &namespace);
        let sessions = case.join("sessions");
        let directory = sessions.join(&expected);
        std::fs::create_dir_all(&directory).unwrap();
        let session_id = format!("synthetic-{index}");
        let file = directory.join(format!("2026-09-09T00-00-00_{session_id}.jsonl"));
        std::fs::write(&file, []).unwrap();
        let source = OmpSource::new(&sessions, &home);
        let input = IngestInput::Native {
            session_id,
            harness: Harness::Omp,
            folder: cwd.clone(),
        };
        let before = source.resolve(&input).unwrap_or_else(|error| {
            panic!("{name}: live cwd must match the literal fixture: {error}")
        });
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].path, file);

        std::fs::remove_dir_all(&physical_cwd).unwrap();
        assert!(!cwd.exists(), "{name}: the cwd must really be gone");
        let after = source.resolve(&input).unwrap_or_else(|error| {
            panic!("{name}: deleting cwd must not change recorded directory {expected}: {error}")
        });
        assert_eq!(after.len(), 1);
        assert_eq!(
            after[0].path, file,
            "{name}: the same recorded file survives cwd deletion"
        );
    }
    assert_eq!(
        branches,
        [3, 3, 2],
        "all naming branches, roots, colon and hyphen cases ran"
    );
}
