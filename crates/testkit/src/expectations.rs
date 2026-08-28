//! The committed fixture expectations, and the assertions they license.
//!
//! Plan 005 tk-c105. Four readers are written in parallel against four golden
//! fixture sets; what stops "my reader looks right to me" from being the
//! done-bar is a file, generated from the pinned reference oracle, that says
//! what reading those exact bytes must produce. This module loads those files
//! and turns them into assertions — so a reader that disagrees FAILS, rather
//! than opening a discussion.
//!
//! Regenerate with
//! `python3 docs/plans/005-convo-ingest/assets/inputs/tools/oracle_expectations.py`;
//! `--check` fails on drift. The driver refuses to run against a modified
//! oracle, so an expectation file can only ever come from the pinned one.
//!
//! # What a file claims, and what it does not
//!
//! Each file names its own [`Expectations::claims`] and carries its
//! [`Expectations::grade_of_proof`] in prose. Read them; they are not the
//! same for every store.
//!
//! **`structural`** (all four stores) is read off the committed bytes: the
//! store's own record-type histogram and an ordered per-record identity — the
//! id the reader will report as `RawRecord::ordinal`. It licenses a
//! SUBSEQUENCE claim, [`Expectations::assert_ordinals_are_a_subsequence`], not
//! an equality one: readers legitimately emit fewer records than the store
//! holds (claude's record-type allowlist drops `attachment` and friends, and
//! its per-block merge folds several records sharing one `message.id` into a
//! single turn). What it catches is an invented ordinal, a lost one, a
//! reordering and a duplicate.
//!
//! **`subset`** (omp, pij, metrics_db) is the oracle's own output. Every turn
//! listed must appear in the reader's output, in order — and the reader may
//! emit more, because the oracle drops record types fs3 must keep. Only the
//! kinds in [`Expectations::prose_kinds`] are compared BY TEXT: the oracle
//! renders tool calls and receipts through its own python helpers, and a Rust
//! reader reproducing a python rendering would be imitation, not agreement.
//! Everything else is held to its count.
//!
//! The claude fixtures carry no `subset` section at all — the pinned oracle
//! has no claude-native reader. That is stated in the file rather than hidden,
//! and the independent semantic check for that store is tk-c305 first light.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fs3_core::content_hash;
use serde::Deserialize;

/// Which committed fixture set an expectation file describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FixtureStore {
    /// Claude Code session jsonl plus subagent sidecars.
    Claude,
    /// omp session jsonl.
    Omp,
    /// The pij seat ledger.
    Pij,
    /// git-ai's machine-wide sqlite metrics.
    MetricsDb,
}

impl FixtureStore {
    /// Every store, so a test can sweep them without a hand-kept list.
    pub const ALL: [Self; 4] = [Self::Claude, Self::Omp, Self::Pij, Self::MetricsDb];

    /// The fixture directory name, which is also the `store` field's value.
    #[must_use]
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Omp => "omp",
            Self::Pij => "pij",
            Self::MetricsDb => "metrics_db",
        }
    }
}

/// The root of the committed conversation fixtures.
///
/// Resolved from this crate's manifest directory, so it is correct from any
/// consuming crate's test and from any working directory.
#[must_use]
pub fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/conversations")
}

/// Where the pinned oracle stood when the expectations were generated.
#[derive(Clone, Debug, Deserialize)]
pub struct Oracle {
    /// Repo-relative path to the oracle script.
    pub script: String,
    /// Its sha-256 at generation time.
    pub sha256: String,
    /// The oracle function the turns came from, or `None` when the store has
    /// no oracle reader (claude).
    pub entrypoint: Option<String>,
}

/// One record as the store wrote it, reduced to what a reader must agree on.
#[derive(Clone, Debug, Deserialize)]
pub struct Record {
    /// 1-based position in the store's own order.
    pub n: usize,
    /// The store's record-type name.
    #[serde(rename = "type")]
    pub record_type: String,
    /// The record's timestamp, as the store spelled it.
    pub ts: Option<String>,
    /// The store's natural id — the reader's future `RawRecord::ordinal`.
    pub id: Option<String>,
    /// The record this one answered, where the store keeps a chain.
    pub parent: Option<String>,
    /// sha-256 of the record's own bytes.
    pub record_sha256: String,
}

/// The records of one fixture file.
#[derive(Clone, Debug, Deserialize)]
pub struct FileRecords {
    /// Path relative to [`fixtures_root`].
    pub file: String,
    /// How many records the file holds.
    pub record_count: usize,
    /// Histogram of the store's own record types.
    pub by_type: BTreeMap<String, usize>,
    /// Every record, in store order.
    pub records: Vec<Record>,
}

/// A conversation's records: the main file, plus any child files.
#[derive(Clone, Debug, Deserialize)]
pub struct Structural {
    /// The conversation's own file.
    pub main: FileRecords,
    /// Child conversations — claude subagent sidecars; empty elsewhere.
    pub sidecars: Vec<FileRecords>,
}

/// One turn the oracle produced from a fixture.
#[derive(Clone, Debug, Deserialize)]
pub struct OracleTurn {
    /// 1-based position in the oracle's output.
    pub n: usize,
    /// The oracle's kind vocabulary — see [`Expectations::prose_kinds`].
    pub kind: String,
    /// The oracle's timestamp for the turn.
    pub ts: String,
    /// Length of the oracle's text, in chars.
    pub text_len: usize,
    /// sha-256 of the oracle's text, which for a prose kind is the store's
    /// verbatim text, trimmed.
    pub text_sha256: String,
    /// The first 80 characters, whitespace-collapsed — for failure messages.
    pub head: String,
}

/// What a reader must EMIT from one file, derived independently of every
/// reader.
///
/// The structural and subset claims are both blind to OVER-EMISSION: splitting
/// one merged group into three emits MORE ordinals, but they are still real
/// store ids, still in order and still repeat-free, so
/// [`Expectations::assert_ordinals_are_a_subsequence`] passes — and the prose
/// is scattered across more records rather than lost, so
/// [`Expectations::assert_oracle_prose_appears`] passes too. Subsequence
/// constrains ORDER and MEMBERSHIP, never CARDINALITY.
///
/// A broken grouping rule IS over-emission, and because the ordinal is the
/// ingest ledger's dedupe key, a changed grouping rule makes every stored
/// record look new and silently DOUBLES the conversation. This is the claim
/// that catches it.
#[derive(Clone, Debug, Deserialize)]
pub struct Emitted {
    /// The fixture file, for a sidecar; absent on a conversation's main file.
    #[serde(default)]
    pub file: Option<String>,
    /// How many records a correct reader emits.
    pub count: usize,
    /// Their ordinals, in store order — the exact sequence, not a subsequence.
    pub ordinals: Vec<String>,
    /// The allowlist and grouping rule this was derived under, in prose.
    pub rule: String,
}

/// One conversation's emitted expectation, main file plus child conversations.
#[derive(Clone, Debug, Deserialize)]
pub struct ExpectedEmitted {
    /// How the numbers were derived, and why that is not circular.
    pub grade: String,
    /// The conversation's own file.
    pub main: Emitted,
    /// Child conversations — claude subagent sidecars; empty elsewhere.
    pub sidecars: Vec<Emitted>,
}

/// A summary row per conversation in a fixture set.
#[derive(Clone, Debug, Deserialize)]
pub struct Session {
    /// Session id, or seat name for the pij ledger.
    pub key: String,
    /// Every fixture file this conversation is made of.
    pub files: Vec<String>,
    /// Records in the main file.
    pub record_count: usize,
    /// Histogram of the store's own record types.
    pub by_type: BTreeMap<String, usize>,
    /// How many turns the oracle produced; zero where there is no oracle.
    pub oracle_turns: usize,
    /// Those turns by kind.
    pub oracle_by_kind: BTreeMap<String, usize>,
    /// What a correct reader EMITS — the cardinality claim the structural and
    /// subset claims structurally cannot make.
    pub expected_emitted: ExpectedEmitted,
    /// Store-specific facts — claude's merge arithmetic and sidecar inventory,
    /// metrics-db's dialect. Deliberately untyped: they are a reader's own
    /// checklist, not a shape every store shares.
    pub extras: BTreeMap<String, serde_json::Value>,
}

/// One store's committed expectations.
#[derive(Clone, Debug, Deserialize)]
pub struct Expectations {
    /// The fixture directory name.
    pub store: String,
    /// Which claims this file makes: `structural`, and `subset` where an
    /// oracle reader exists.
    pub claims: Vec<String>,
    /// Oracle kinds whose text a reader can be held to verbatim.
    pub prose_kinds: Vec<String>,
    /// How to regenerate this file.
    pub regenerate: String,
    /// The pinned oracle at generation time.
    pub oracle: Oracle,
    /// What this file proves, and explicitly what it does not.
    pub grade_of_proof: String,
    /// One row per conversation.
    pub sessions: Vec<Session>,
    /// Per-conversation record identity, keyed by [`Session::key`].
    pub structural: BTreeMap<String, Structural>,
    /// Per-conversation oracle turns, keyed by [`Session::key`]; empty for
    /// claude.
    pub turns: BTreeMap<String, Vec<OracleTurn>>,
    /// sha-256 of every committed fixture byte, keyed by path relative to
    /// [`fixtures_root`].
    pub fixture_sha256: BTreeMap<String, String>,
}

impl Expectations {
    /// Load one store's expectations.
    ///
    /// # Panics
    /// If the file is missing or unparseable — both mean the fixtures and the
    /// expectations have parted company, which is not a condition to recover
    /// from inside a test.
    #[must_use]
    pub fn load(store: FixtureStore) -> Self {
        let path = fixtures_root()
            .join(store.dir_name())
            .join("expectations.json");
        let raw = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "expectations for `{}` are missing at {}: {error}\nregenerate with \
                 `python3 docs/plans/005-convo-ingest/assets/inputs/tools/oracle_expectations.py`",
                store.dir_name(),
                path.display()
            )
        });
        serde_json::from_str(&raw).unwrap_or_else(|error| {
            panic!(
                "expectations at {} do not match this loader's shape: {error}\nthe driver and \
                 `fs3_testkit::expectations` must be changed together",
                path.display()
            )
        })
    }

    /// The summary row for one conversation.
    ///
    /// # Panics
    /// If the key names no conversation in this fixture set.
    #[must_use]
    pub fn session(&self, key: &str) -> &Session {
        self.sessions
            .iter()
            .find(|session| session.key == key)
            .unwrap_or_else(|| {
                let known: Vec<&str> = self.sessions.iter().map(|s| s.key.as_str()).collect();
                panic!(
                    "`{key}` is not a conversation in the `{}` fixtures; known: {known:?}",
                    self.store
                )
            })
    }

    /// Every ordinal the store holds for one conversation, in store order.
    ///
    /// The main file's records followed by each sidecar's — the order a reader
    /// resolving main-then-children walks them in.
    ///
    /// # Panics
    /// If the key names no conversation in this fixture set.
    #[must_use]
    pub fn ordinals(&self, key: &str) -> Vec<&str> {
        let structural = self.structural.get(key).unwrap_or_else(|| {
            panic!(
                "`{key}` has no structural records in the `{}` fixtures",
                self.store
            )
        });
        std::iter::once(&structural.main)
            .chain(&structural.sidecars)
            .flat_map(|file| file.records.iter())
            .filter_map(|record| record.id.as_deref())
            .collect()
    }

    /// Prove every fixture byte is exactly what the expectations were built
    /// from.
    ///
    /// This is the alarm that stops the whole scheme rotting: edit a fixture
    /// without regenerating, and every claim in the file silently describes a
    /// file that no longer exists. Here it stops being silent.
    ///
    /// # Panics
    /// On a missing file, an extra unpinned file, or any content change.
    pub fn verify_fixtures_unchanged(&self) {
        let root = fixtures_root();
        for (relative, expected) in &self.fixture_sha256 {
            let path = root.join(relative);
            let bytes = std::fs::read(&path).unwrap_or_else(|error| {
                panic!(
                    "pinned fixture {relative} is gone ({error}) — if that was deliberate, \
                     regenerate: {}",
                    self.regenerate
                )
            });
            let actual = content_hash(&bytes);
            assert_eq!(
                &actual, expected,
                "fixture {relative} changed since its expectations were generated, so every \
                 claim about it is now describing a file that does not exist. Regenerate: {}",
                self.regenerate
            );
        }

        let store_root = root.join(&self.store);
        let mut unpinned = Vec::new();
        collect_files(&store_root, &mut unpinned);
        for path in unpinned {
            if path
                .file_name()
                .is_some_and(|name| name == "expectations.json")
            {
                continue;
            }
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            assert!(
                self.fixture_sha256.contains_key(&relative),
                "fixture {relative} exists but is not pinned by the expectations — a fixture \
                 nothing asserts over is a fixture nobody is held to. Regenerate: {}",
                self.regenerate
            );
        }
    }

    /// Assert a reader's emitted ordinals are an in-order, repeat-free
    /// subsequence of what the store holds.
    ///
    /// This is the structural done-bar. It is deliberately not equality: a
    /// reader emits fewer records than the store holds, by design.
    ///
    /// # Panics
    /// On an ordinal the store does not hold, one emitted out of store order,
    /// or one emitted twice.
    pub fn assert_ordinals_are_a_subsequence(&self, key: &str, observed: &[String]) {
        let store_order = self.ordinals(key);
        let mut cursor = 0usize;
        let mut seen: Vec<&str> = Vec::with_capacity(observed.len());

        for ordinal in observed {
            assert!(
                !seen.contains(&ordinal.as_str()),
                "the reader emitted ordinal `{ordinal}` twice for `{key}`; ordinals are the \
                 dedupe key after a rotation, so a repeat duplicates a turn in the index"
            );
            let found = store_order[cursor..]
                .iter()
                .position(|candidate| *candidate == ordinal.as_str());
            match found {
                Some(offset) => {
                    cursor += offset + 1;
                    seen.push(ordinal);
                }
                None if store_order.contains(&ordinal.as_str()) => panic!(
                    "the reader emitted ordinal `{ordinal}` for `{key}` OUT OF STORE ORDER — it \
                     appears earlier in the store than a record already emitted, so the \
                     conversation would be reassembled in the wrong sequence"
                ),
                None => panic!(
                    "the reader emitted ordinal `{ordinal}` for `{key}`, which the store does \
                     not hold — a reader may emit fewer records than the store has, never one \
                     the store never wrote"
                ),
            }
        }
    }

    /// Assert a reader emitted EXACTLY the records it should, in order.
    ///
    /// The cardinality claim. [`assert_ordinals_are_a_subsequence`] cannot make
    /// it: over-emission — one merged group split into three — yields more
    /// ordinals that are still real, still ordered and still repeat-free, so it
    /// passes. So does the prose containment claim, because splitting scatters
    /// prose rather than losing it. Both are structurally blind to exactly the
    /// failure that silently doubles a stored conversation.
    ///
    /// The expected sequence is derived from the store's bytes by
    /// `oracle_expectations.py`, applying the ruled allowlist and grouping rule
    /// a SECOND time — never harvested from a reader's output, which would pin
    /// whatever the code did that day.
    ///
    /// [`assert_ordinals_are_a_subsequence`]:
    /// Expectations::assert_ordinals_are_a_subsequence
    ///
    /// # Panics
    /// On any difference in length, membership or order, naming the first
    /// position that differs.
    pub fn assert_emitted_ordinals_match(&self, key: &str, observed: &[String]) {
        let expected = &self.session(key).expected_emitted;
        Self::compare_emitted(key, &expected.main, observed, &expected.grade);
    }

    /// The same claim for a child conversation, addressed by any trailing part
    /// of its fixture path — a file name is enough, so a test reading a
    /// tempdir copy can name the sidecar without reconstructing the fixture
    /// path.
    ///
    /// # Panics
    /// If nothing matches `file`, or on any difference.
    pub fn assert_emitted_sidecar_ordinals_match(
        &self,
        key: &str,
        file: &str,
        observed: &[String],
    ) {
        let expected = &self.session(key).expected_emitted;
        let sidecar = expected
            .sidecars
            .iter()
            .find(|candidate| {
                candidate
                    .file
                    .as_deref()
                    .is_some_and(|recorded| recorded.ends_with(file))
            })
            .unwrap_or_else(|| {
                let known: Vec<&str> = expected
                    .sidecars
                    .iter()
                    .filter_map(|candidate| candidate.file.as_deref())
                    .collect();
                panic!("no sidecar of `{key}` ends with `{file}`; this fixture has {known:?}")
            });
        Self::compare_emitted(key, sidecar, observed, &expected.grade);
    }

    fn compare_emitted(key: &str, expected: &Emitted, observed: &[String], grade: &str) {
        let where_ = expected.file.as_deref().unwrap_or(key);
        if observed.len() != expected.count {
            let verdict = if observed.len() > expected.count {
                "OVER-EMITTED: a group was split, or a record type that is store bookkeeping \
                 was emitted as a turn. Because the ordinal is the ingest ledger's dedupe key, \
                 every stored record would look new and the conversation would silently DOUBLE"
            } else {
                "UNDER-EMITTED: records were merged that should not have been, or the allowlist \
                 dropped something that carries prose"
            };
            panic!(
                "{where_}: expected {} records, reader emitted {} — {verdict}.\n  rule: {}\n  \
                 grade: {grade}",
                expected.count,
                observed.len(),
                expected.rule
            );
        }
        for (position, (want, got)) in expected.ordinals.iter().zip(observed).enumerate() {
            assert!(
                want == got,
                "{where_}: ordinal {position} is `{got}` where the store's own bytes say \
                 `{want}`. The count is right, so this is a WRONG RECORD CHOSEN rather than a \
                 miscount — for a merged group, the ordinal must be the group's FIRST member, \
                 because a later member's id changes between an incremental read and a rescan \
                 and the ledger's dedupe would miss.\n  rule: {}\n  grade: {grade}",
                expected.rule
            );
        }
    }

    /// Assert every oracle PROSE turn appears, in order, among a reader's
    /// bodies.
    ///
    /// Bodies are compared trimmed, because the oracle strips. Non-prose kinds
    /// are skipped: the oracle renders them through its own helpers, and a
    /// reader reproducing a python rendering would prove imitation, not
    /// agreement — hold those to [`Session::oracle_by_kind`] instead.
    ///
    /// # Panics
    /// When an oracle prose turn is missing from the reader's output, or
    /// appears out of order.
    pub fn assert_oracle_prose_appears(&self, key: &str, observed_bodies: &[String]) {
        let turns = self.turns.get(key).unwrap_or_else(|| {
            panic!(
                "`{key}` has no oracle turns in the `{}` fixtures — this store's claims are {:?}",
                self.store, self.claims
            )
        });
        let hashes: Vec<String> = observed_bodies
            .iter()
            .map(|body| content_hash(body.trim().as_bytes()))
            .collect();

        let mut cursor = 0usize;
        for turn in turns {
            if !self.prose_kinds.contains(&turn.kind) {
                continue;
            }
            let found = hashes[cursor..]
                .iter()
                .position(|hash| *hash == turn.text_sha256);
            match found {
                Some(offset) => cursor += offset + 1,
                None if hashes.contains(&turn.text_sha256) => panic!(
                    "oracle turn {} ({}) appears in the reader's output for `{key}` but OUT OF \
                     ORDER: \"{}\"",
                    turn.n, turn.kind, turn.head
                ),
                None => panic!(
                    "oracle turn {} ({}) is MISSING from the reader's output for `{key}`: \
                     \"{}\" — the oracle read it out of these exact bytes, so the reader \
                     dropped it",
                    turn.n, turn.kind, turn.head
                ),
            }
        }
    }
}

/// Every file under `root`, recursively. Missing directories yield nothing.
fn collect_files(root: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, into);
        } else {
            into.push(path);
        }
    }
}
