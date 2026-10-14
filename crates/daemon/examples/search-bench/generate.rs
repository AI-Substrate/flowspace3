//! `search-bench generate`: a synthetic corpus shaped like prod, at a scale.
//!
//! The shape comes from in-database aggregates of the prod index on 2026-10-14
//! (identities hashed, no content read out). At scale 1 the corpus has:
//! - 17 file-backed repositories with prod's per-repository worktree, file and
//!   blob counts;
//! - about 1,500 conversations and 750k turns, spread over repositories as in
//!   prod;
//! - about 1.2M elements, each distinct text repeated across parser versions as
//!   in prod;
//! - about 0.9M raw and 0.6M smart vectors.
//!
//! Scale `n` repeats the repositories `n` times under new identities, so the
//! central index grows the way it does in practice: more repositories and more
//! conversations, each scope a smaller share of the whole.

use std::collections::HashSet;
use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use fs3_core::content_hash;
use sqlx::postgres::PgPoolCopyExt;
use sqlx::{Connection, PgConnection};

use crate::model::{Place, Rng, Space, VECTORS_PER_CLUSTER, Words};

/// The model key every bench vector is stored under, and the bench embedder
/// answers with.
pub const MODEL_KEY: &str = "bench-clustered@1024";
const SUMMARY_KEY: &str = "bench-summary@1";

/// One prod file-backed repository: worktrees, files, distinct blobs, and its
/// conversations and turns.
struct RepoShape {
    worktrees: usize,
    files: usize,
    blobs: usize,
    conversations: usize,
    turns: usize,
}

const fn shape(
    worktrees: usize,
    files: usize,
    blobs: usize,
    conversations: usize,
    turns: usize,
) -> RepoShape {
    RepoShape {
        worktrees,
        files,
        blobs,
        conversations,
        turns,
    }
}

/// Prod's file-backed repositories, largest first, then the repositories
/// known only through conversations (no worktree), then conversations with
/// no repository at all.
const FILE_REPOS: [RepoShape; 17] = [
    shape(10, 41_364, 5_522, 60, 65_528),
    shape(15, 25_494, 3_509, 279, 159_101),
    shape(12, 20_105, 2_515, 44, 36_884),
    shape(32, 15_109, 742, 4, 1_200),
    shape(27, 14_415, 1_057, 148, 77_070),
    shape(4, 14_026, 4_912, 18, 13_658),
    shape(10, 7_873, 1_218, 4, 1_200),
    shape(16, 6_382, 848, 7, 8_291),
    shape(11, 6_043, 1_294, 12, 26_737),
    shape(2, 3_258, 2_882, 270, 108_196),
    shape(3, 3_175, 994, 117, 51_819),
    shape(8, 2_303, 513, 6, 6_418),
    shape(1, 1_357, 1_347, 65, 13_038),
    shape(2, 1_350, 1_317, 41, 46_912),
    shape(1, 550, 550, 2, 600),
    shape(1, 366, 361, 2, 600),
    shape(1, 278, 277, 25, 5_851),
];
/// Repositories with conversations and no indexed worktree: (conversations, turns).
const CONVERSATION_REPOS: [(usize, usize); 20] = [
    (25, 20_707),
    (26, 5_972),
    (9, 5_535),
    (13, 4_900),
    (7, 4_379),
    (11, 2_570),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
    (6, 1_100),
];
const UNANCHORED_CONVERSATIONS: (usize, usize) = (272, 76_593);

/// Measured prod text lengths (median chars, log-space spread).
const TURN_CHARS: (f64, f64) = (523.0, 1.22);
const SECTION_CHARS: (f64, f64) = (703.0, 1.25);
const FUNCTION_CHARS: (f64, f64) = (439.0, 1.14);
const CONTAINER_CHARS: (f64, f64) = (300.0, 1.6);
const ROW_CHARS: (f64, f64) = (310.0, 0.83);
const MAX_CHARS: usize = 60_000;
/// Text past this many characters is embedded as a second chunk too.
const CHUNK_CHARS: usize = 8_000;
/// Share of turns that carry a summary, so smart vectors match prod's count.
const TURN_SMART_SHARE: f64 = 0.62;

/// What `generate` was asked for.
pub struct Request {
    pub server: String,
    pub name: String,
    pub scale: f64,
    pub seed: u64,
    pub replace: bool,
    pub skip_indexes: bool,
}

/// One repository as generated, recorded in `bench_meta` for `run`.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct GeneratedRepo {
    pub identity: String,
    pub worktrees: Vec<String>,
    pub files: usize,
    pub blobs: usize,
    pub conversations: usize,
    pub turns: usize,
}

#[derive(Default, serde::Serialize, Debug)]
struct Counts {
    repos: usize,
    worktrees: usize,
    worktree_files: usize,
    conversations: usize,
    turns: usize,
    elements: usize,
    smart: usize,
    raw_vectors: usize,
    smart_vectors: usize,
}

/// A COPY stream into one table, flushed in chunks.
struct Copy {
    table: &'static str,
    buffer: String,
    copy: Option<sqlx::postgres::PgCopyIn<sqlx::pool::PoolConnection<sqlx::Postgres>>>,
}

impl Copy {
    async fn open(pool: &sqlx::PgPool, table: &'static str, columns: &str) -> Result<Self> {
        let statement = format!("COPY {table} ({columns}) FROM STDIN");
        let copy = pool
            .copy_in_raw(&statement)
            .await
            .with_context(|| format!("starting COPY into {table}"))?;
        Ok(Copy {
            table,
            buffer: String::with_capacity(4 << 20),
            copy: Some(copy),
        })
    }

    fn row(&mut self, fields: &[Field<'_>]) {
        for (index, field) in fields.iter().enumerate() {
            if index > 0 {
                self.buffer.push('\t');
            }
            match field {
                Field::Null => self.buffer.push_str("\\N"),
                Field::Text(text) => escape(&mut self.buffer, text),
                Field::Int(value) => {
                    let _ = write!(self.buffer, "{value}");
                }
                Field::Vector(vector) => {
                    self.buffer.push('[');
                    for (i, x) in vector.iter().enumerate() {
                        if i > 0 {
                            self.buffer.push(',');
                        }
                        let _ = write!(self.buffer, "{x:.6}");
                    }
                    self.buffer.push(']');
                }
            }
        }
        self.buffer.push('\n');
    }

    async fn flush_if_full(&mut self) -> Result<()> {
        if self.buffer.len() >= 4 << 20 {
            self.flush().await?;
        }
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let copy = self.copy.as_mut().expect("open");
        copy.send(self.buffer.as_bytes())
            .await
            .with_context(|| format!("COPY into {}", self.table))?;
        self.buffer.clear();
        Ok(())
    }

    async fn finish(mut self) -> Result<u64> {
        self.flush().await?;
        let rows = self.copy.take().expect("open").finish().await?;
        Ok(rows)
    }
}

enum Field<'a> {
    Null,
    Text(&'a str),
    Int(i64),
    Vector(&'a [f32]),
}

fn escape(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
}

/// A 40-hex git-blob-like id for a synthetic blob.
fn blob_id(seed: u64, repo: usize, blob: usize) -> String {
    content_hash(format!("bench-blob:{seed}:{repo}:{blob}").as_bytes())[..40].to_string()
}

fn uuid(rng: &mut Rng) -> String {
    let a = rng.next_u64();
    let b = rng.next_u64();
    format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0xfff,
        b >> 52,
        b & 0xffff_ffff_ffff
    )
}

/// Assigns each new vector a cluster and a near-duplicate group: content in
/// one file or conversation stays in its topic, and near-duplicates come in
/// small runs.
struct Placer {
    group: u64,
    left: usize,
    cluster: usize,
}

impl Placer {
    fn next(&mut self, space: &Space, topic: usize, rng: &mut Rng) -> Place {
        if self.left == 0 {
            self.group += 1;
            self.left = rng.geometric(4.0);
            self.cluster = if rng.chance(0.85) {
                space.cluster_in(topic, rng)
            } else {
                rng.below(space.clusters())
            };
        }
        self.left -= 1;
        Place {
            cluster: self.cluster,
            group: self.group,
        }
    }
}

struct Writers {
    elements: Copy,
    smart: Copy,
    embeddings: Copy,
    next_element: i64,
    seen_raw: HashSet<u128>,
    point: u64,
    counts: Counts,
}

fn hash_key(hash: &str) -> u128 {
    u128::from_str_radix(&hash[..32], 16).unwrap_or_default()
}

impl Writers {
    /// One element row. Returns its id.
    #[allow(clippy::too_many_arguments)]
    fn element(
        &mut self,
        blob_sha: &str,
        parser_version: &str,
        parent: Option<i64>,
        kind: &str,
        subkind: &str,
        name: &str,
        address: &str,
        span: (i64, i64),
        sibling: i64,
        raw_text: &str,
        raw_hash: &str,
        turn_class: Option<&str>,
    ) -> i64 {
        let id = self.next_element;
        self.next_element += 1;
        self.elements.row(&[
            Field::Int(id),
            Field::Text(blob_sha),
            Field::Text(parser_version),
            parent.map_or(Field::Null, Field::Int),
            Field::Text(kind),
            Field::Text(subkind),
            Field::Text(name),
            Field::Text(address),
            Field::Int(span.0),
            Field::Int(span.1),
            Field::Int(sibling),
            Field::Text(raw_text),
            Field::Text(raw_hash),
            Field::Text("t"),
            turn_class.map_or(Field::Null, Field::Text),
        ]);
        self.counts.elements += 1;
        id
    }

    /// The raw vector(s) and, when `smart`, a summary and its vector, for
    /// content not seen before. Returns whether the content was new.
    #[allow(clippy::too_many_arguments)]
    fn vectors(
        &mut self,
        space: &Space,
        words: &Words,
        place: Place,
        raw_text: &str,
        raw_hash: &str,
        smart: bool,
        topic: usize,
        rng: &mut Rng,
    ) -> bool {
        if !self.seen_raw.insert(hash_key(raw_hash)) {
            return false;
        }
        let chunks = if raw_text.len() > CHUNK_CHARS { 2 } else { 1 };
        for chunk in 0..chunks {
            self.point += 1;
            let vector = space.vector(place, self.point);
            self.embeddings.row(&[
                Field::Text(raw_hash),
                Field::Text("raw"),
                Field::Text(MODEL_KEY),
                Field::Vector(&vector),
                Field::Int(chunk),
            ]);
            self.counts.raw_vectors += 1;
        }
        if smart {
            let summary = words.text(topic, 120 + rng.below(240), rng);
            let text_hash = content_hash(summary.as_bytes());
            self.smart.row(&[
                Field::Text(raw_hash),
                Field::Text(SUMMARY_KEY),
                Field::Text(&summary),
                Field::Text(&text_hash),
                Field::Text("{bench}"),
            ]);
            self.counts.smart += 1;
            self.point += 1;
            let vector = space.vector(place, self.point);
            self.embeddings.row(&[
                Field::Text(&text_hash),
                Field::Text("smart"),
                Field::Text(MODEL_KEY),
                Field::Vector(&vector),
                Field::Int(0),
            ]);
            self.counts.smart_vectors += 1;
        }
        true
    }

    async fn flush_if_full(&mut self) -> Result<()> {
        self.elements.flush_if_full().await?;
        self.smart.flush_if_full().await?;
        self.embeddings.flush_if_full().await
    }
}

fn sized(rng: &mut Rng, (median, sigma): (f64, f64)) -> usize {
    (rng.lognormal(median, sigma) as usize).clamp(24, MAX_CHARS)
}

/// One child of a blob: kind, subkind, a name, its text.
struct Child {
    kind: &'static str,
    subkind: &'static str,
    name: String,
    text: String,
}

fn children(words: &Words, topic: usize, doc: bool, rng: &mut Rng) -> Vec<Child> {
    let mut out = Vec::new();
    if doc {
        for _ in 0..rng.geometric(9.6) {
            let name = format!("{} {}", words.topical(topic, rng), words.zipf(rng));
            let text = format!(
                "## {name}\n{}",
                words.text(topic, sized(rng, SECTION_CHARS), rng)
            );
            out.push(Child {
                kind: "section",
                subkind: "atx_heading",
                name,
                text,
            });
        }
        if rng.chance(0.2) {
            for _ in 0..rng.geometric(4.4) {
                let name = words.identifier(topic, rng);
                let text = format!(
                    "| {name} | {} |",
                    words.text(topic, sized(rng, ROW_CHARS), rng)
                );
                out.push(Child {
                    kind: "row",
                    subkind: "table_row",
                    name,
                    text,
                });
            }
        }
    } else {
        for _ in 0..rng.geometric(2.35) {
            let name = words.identifier(topic, rng);
            let body = words.text(topic, sized(rng, FUNCTION_CHARS), rng);
            out.push(Child {
                kind: "function",
                subkind: "function_item",
                text: format!("fn {name}() {{\n    // {body}\n}}"),
                name,
            });
        }
        if rng.chance(0.5) {
            for _ in 0..rng.geometric(1.4) {
                let name = words.identifier(topic, rng);
                let body = words.text(topic, sized(rng, CONTAINER_CHARS), rng);
                out.push(Child {
                    kind: "container",
                    subkind: "struct_item",
                    text: format!("struct {name} {{\n    // {body}\n}}"),
                    name,
                });
            }
        }
    }
    out
}

/// How many parser versions keep a copy of a blob's elements, as prod's
/// retained older versions do.
fn versions(rng: &mut Rng) -> usize {
    match rng.unit() {
        u if u < 0.3 => 1,
        u if u < 0.5 => 2,
        _ => 3,
    }
}

const TURN_CLASSES: [(&str, f64); 5] = [
    ("tool_result", 0.44),
    ("tool_call", 0.30),
    ("work", 0.2585),
    ("retrieval_result", 0.001),
    ("retrieval_call", 0.0005),
];

fn turn_class(rng: &mut Rng) -> &'static str {
    let mut u = rng.unit();
    for (class, share) in TURN_CLASSES {
        if u < share {
            return class;
        }
        u -= share;
    }
    "work"
}

pub async fn generate(request: &Request) -> Result<()> {
    if !(0.01..=20.0).contains(&request.scale) {
        bail!("--scale must be between 0.01 and 20");
    }
    let server = request.server.trim_end_matches('/');
    let admin_url = format!("{server}/postgres");
    let url = format!("{server}/{}", request.name);
    let mut admin = PgConnection::connect(&admin_url)
        .await
        .with_context(|| format!("connecting to {admin_url}"))?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&request.name)
            .fetch_one(&mut admin)
            .await?;
    if exists {
        if !request.replace {
            bail!(
                "database {} exists; pass --replace to drop and regenerate it",
                request.name
            );
        }
        sqlx::query(&format!("DROP DATABASE \"{}\" WITH (FORCE)", request.name))
            .execute(&mut admin)
            .await?;
    }
    sqlx::query(&format!("CREATE DATABASE \"{}\"", request.name))
        .execute(&mut admin)
        .await?;
    admin.close().await?;

    let pool = fs3_store::connect(&url).await?;
    fs3_store::migrate(&pool).await?;
    for statement in [
        "CREATE EXTENSION IF NOT EXISTS pg_stat_statements",
        "CREATE TABLE bench_meta (key text PRIMARY KEY, value jsonb NOT NULL)",
        // Built after the load: an index maintained row by row is many times
        // slower to fill than one built once.
        "DROP INDEX IF EXISTS embeddings_1024_vector_idx",
        "DROP INDEX IF EXISTS elements_lexical_trgm_idx",
    ] {
        sqlx::query(statement).execute(&pool).await?;
    }

    let started = std::time::Instant::now();
    let seed = request.seed;
    let copies = request.scale.ceil() as usize;
    let fraction = |count: usize, copy: usize| -> usize {
        let whole = request.scale.floor() as usize;
        if copy < whole {
            count
        } else {
            ((count as f64) * (request.scale - whole as f64)).round() as usize
        }
    };
    let total_vectors = (1_600_000.0 * request.scale) as usize;
    let space = Space::new(seed, total_vectors / VECTORS_PER_CLUSTER);
    let words = Words::new(seed, 30_000);
    let mut rng = Rng::derive(seed, 3);

    let loader = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await?;
    let mut repos_copy = Copy::open(&loader, "repos", "id, identity").await?;
    let mut worktrees_copy = Copy::open(&loader, "worktrees", "id, repo_id, root_path").await?;
    let mut files_copy =
        Copy::open(&loader, "worktree_files", "worktree_id, path, blob_sha").await?;
    let mut conversations_copy = Copy::open(
        &loader,
        "conversations",
        "guid, repo_identity, worktree, title, started_at",
    )
    .await?;
    let mut turns_copy = Copy::open(
        &loader,
        "turns",
        "conversation_id, turn_no, role, source, at, body, blob_sha",
    )
    .await?;
    let mut writers = Writers {
        elements: Copy::open(
            &loader,
            "elements",
            "id, blob_sha, parser_version, parent_id, kind, subkind, name, address, \
             span_start, span_end, sibling_order, raw_text, raw_hash, enrich, turn_class",
        )
        .await?,
        smart: Copy::open(
            &loader,
            "smart_content",
            "raw_hash, model_key, text, text_hash, tags",
        )
        .await?,
        embeddings: Copy::open(
            &loader,
            "embeddings_1024",
            "source_hash, source_kind, model_key, vector, chunk_no",
        )
        .await?,
        next_element: 1,
        seen_raw: HashSet::new(),
        point: 0,
        counts: Counts::default(),
    };
    let mut placer = Placer {
        group: 0,
        left: 0,
        cluster: 0,
    };
    let mut generated: Vec<GeneratedRepo> = Vec::new();
    let mut next_worktree: i64 = 1;
    let mut next_repo: i64 = 1;

    // Conversations: (repo identity, worktree roots, conversations, turns).
    let mut conversation_plan: Vec<(Option<String>, Vec<String>, usize, usize)> = Vec::new();

    for copy in 0..copies {
        for (index, template) in FILE_REPOS.iter().enumerate() {
            let files = fraction(template.files, copy);
            if files == 0 {
                continue;
            }
            let repo_index = copy * FILE_REPOS.len() + index;
            let identity = format!("bench:repo-{repo_index:04}");
            let repo_id = next_repo;
            next_repo += 1;
            repos_copy.row(&[Field::Int(repo_id), Field::Text(&identity)]);
            writers.counts.repos += 1;
            let worktree_count = template.worktrees.max(1);
            let roots: Vec<String> = (0..worktree_count)
                .map(|w| format!("/bench/{identity}/wt-{w:02}"))
                .collect();
            let worktree_ids: Vec<i64> = roots
                .iter()
                .map(|root| {
                    let id = next_worktree;
                    next_worktree += 1;
                    worktrees_copy.row(&[Field::Int(id), Field::Int(repo_id), Field::Text(root)]);
                    id
                })
                .collect();
            writers.counts.worktrees += worktree_count;

            // Paths, and each path's blob versions; the first is the main one.
            let paths = (files / worktree_count).max(1);
            let blob_total = fraction(template.blobs, copy).max(paths);
            let mut path_blobs: Vec<Vec<usize>> = (0..paths).map(|p| vec![p]).collect();
            for extra in paths..blob_total {
                path_blobs[rng.below(paths)].push(extra);
            }
            let repo_topic = rng.below(crate::model::TOPICS);
            for (path_index, blobs) in path_blobs.iter().enumerate() {
                let doc = rng.chance(0.45);
                let topic = if rng.chance(0.7) {
                    repo_topic
                } else {
                    rng.below(crate::model::TOPICS)
                };
                let path = format!(
                    "{}/{}{}",
                    words.topical(topic, &mut rng),
                    words.identifier(topic, &mut rng),
                    if doc { ".md" } else { ".rs" }
                );
                let path = format!("p{path_index}/{path}");
                let base = children(&words, topic, doc, &mut rng);
                for (version, blob) in blobs.iter().enumerate() {
                    let blob_sha = blob_id(seed, repo_index, *blob);
                    // A later version of a file keeps most of its parts.
                    let parts: Vec<Child> = if version == 0 {
                        base.iter()
                            .map(|c| Child {
                                kind: c.kind,
                                subkind: c.subkind,
                                name: c.name.clone(),
                                text: c.text.clone(),
                            })
                            .collect()
                    } else {
                        let mut parts = children(&words, topic, doc, &mut rng);
                        for (i, c) in base.iter().enumerate() {
                            if rng.chance(0.85) && i < parts.len() {
                                parts[i].text.clone_from(&c.text);
                                parts[i].name.clone_from(&c.name);
                            }
                        }
                        parts
                    };
                    let file_text: String = parts
                        .iter()
                        .map(|c| c.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let file_hash = content_hash(file_text.as_bytes());
                    let places: Vec<Place> = parts
                        .iter()
                        .map(|_| placer.next(&space, topic, &mut rng))
                        .collect();
                    let file_place = placer.next(&space, topic, &mut rng);
                    for parser in 0..versions(&mut rng) {
                        let parser_version = format!("fs3-parsers@{}", 3 - parser);
                        let root = writers.element(
                            &blob_sha,
                            &parser_version,
                            None,
                            "file",
                            if doc { "markdown" } else { "rust" },
                            &path,
                            &path,
                            (1, 1 + parts.len() as i64 * 10),
                            0,
                            &file_text,
                            &file_hash,
                            None,
                        );
                        if parts.is_empty() {
                            writers.vectors(
                                &space, &words, file_place, &file_text, &file_hash, true, topic,
                                &mut rng,
                            );
                        }
                        for (sibling, (part, place)) in parts.iter().zip(&places).enumerate() {
                            let hash = content_hash(part.text.as_bytes());
                            let line = 2 + sibling as i64 * 10;
                            writers.element(
                                &blob_sha,
                                &parser_version,
                                Some(root),
                                part.kind,
                                part.subkind,
                                &part.name,
                                &format!("{path}::{}", part.name),
                                (line, line + 8),
                                sibling as i64,
                                &part.text,
                                &hash,
                                None,
                            );
                            writers.vectors(
                                &space, &words, *place, &part.text, &hash, true, topic, &mut rng,
                            );
                        }
                    }
                    writers.flush_if_full().await?;
                }
                // Every worktree holds this path, mostly at its main version.
                for worktree in &worktree_ids {
                    let blob = if blobs.len() > 1 && rng.chance(0.2) {
                        blobs[1 + rng.below(blobs.len() - 1)]
                    } else {
                        blobs[0]
                    };
                    files_copy.row(&[
                        Field::Int(*worktree),
                        Field::Text(&path),
                        Field::Text(&blob_id(seed, repo_index, blob)),
                    ]);
                    writers.counts.worktree_files += 1;
                }
                files_copy.flush_if_full().await?;
            }
            conversation_plan.push((
                Some(identity.clone()),
                roots.clone(),
                fraction(template.conversations, copy),
                fraction(template.turns, copy),
            ));
            generated.push(GeneratedRepo {
                identity,
                worktrees: roots,
                files: paths * worktree_count,
                blobs: blob_total,
                conversations: fraction(template.conversations, copy),
                turns: fraction(template.turns, copy),
            });
        }
        for (index, (conversations, turns)) in CONVERSATION_REPOS.iter().enumerate() {
            let identity = format!("bench:talk-{:04}", copy * CONVERSATION_REPOS.len() + index);
            conversation_plan.push((
                Some(identity),
                Vec::new(),
                fraction(*conversations, copy),
                fraction(*turns, copy),
            ));
        }
        conversation_plan.push((
            None,
            Vec::new(),
            fraction(UNANCHORED_CONVERSATIONS.0, copy),
            fraction(UNANCHORED_CONVERSATIONS.1, copy),
        ));
    }

    for (identity, roots, conversations, turns) in conversation_plan {
        if conversations == 0 {
            continue;
        }
        for conversation in 0..conversations {
            let guid = uuid(&mut rng);
            let worktree = (!roots.is_empty() && rng.chance(0.8))
                .then(|| roots[rng.below(roots.len())].clone());
            conversations_copy.row(&[
                Field::Text(&guid),
                identity.as_deref().map_or(Field::Null, Field::Text),
                worktree.as_deref().map_or(Field::Null, Field::Text),
                Field::Text("bench conversation"),
                Field::Text("2026-01-01T00:00:00Z"),
            ]);
            writers.counts.conversations += 1;
            let count = turns / conversations + usize::from(conversation < turns % conversations);
            let topic = rng.below(crate::model::TOPICS);
            for turn_no in 1..=count {
                let human = rng.chance(0.08);
                let text = words.text(topic, sized(&mut rng, TURN_CHARS), &mut rng);
                let hash = content_hash(text.as_bytes());
                let class = turn_class(&mut rng);
                let address = format!("conv:{guid}#t{turn_no}");
                turns_copy.row(&[
                    Field::Text(&guid),
                    Field::Int(turn_no as i64),
                    Field::Text(if human { "human" } else { "agent" }),
                    Field::Text(if human { "human" } else { "system" }),
                    Field::Text("2026-01-01T00:00:00Z"),
                    Field::Text(&text),
                    Field::Text(&hash),
                ]);
                writers.element(
                    &hash,
                    "conversation/1",
                    None,
                    "turn",
                    if human { "human" } else { "agent" },
                    &format!("turn {turn_no}"),
                    &address,
                    (turn_no as i64, turn_no as i64),
                    0,
                    &text,
                    &hash,
                    Some(class),
                );
                let place = placer.next(&space, topic, &mut rng);
                let smart = rng.chance(TURN_SMART_SHARE);
                writers.vectors(&space, &words, place, &text, &hash, smart, topic, &mut rng);
                writers.counts.turns += 1;
                if turn_no % 256 == 0 {
                    writers.flush_if_full().await?;
                    turns_copy.flush_if_full().await?;
                }
            }
        }
        writers.flush_if_full().await?;
        turns_copy.flush_if_full().await?;
        eprintln!(
            "  … {} turns, {} elements, {} vectors ({:.0?})",
            writers.counts.turns,
            writers.counts.elements,
            writers.counts.raw_vectors + writers.counts.smart_vectors,
            started.elapsed()
        );
    }

    // Parents before children, so the elements' self-reference resolves.
    repos_copy.finish().await?;
    worktrees_copy.finish().await?;
    files_copy.finish().await?;
    conversations_copy.finish().await?;
    turns_copy.finish().await?;
    writers.elements.finish().await?;
    writers.smart.finish().await?;
    writers.embeddings.finish().await?;
    eprintln!("loaded in {:.0?}: {:?}", started.elapsed(), writers.counts);

    for statement in [
        "SELECT setval('elements_id_seq', (SELECT max(id) FROM elements))",
        "SELECT setval('repos_id_seq', (SELECT max(id) FROM repos))",
        "SELECT setval('worktrees_id_seq', (SELECT max(id) FROM worktrees))",
    ] {
        sqlx::query(statement).execute(&pool).await?;
    }
    let meta = serde_json::json!({
        "scale": request.scale,
        "seed": seed,
        "model_key": MODEL_KEY,
        "clusters": space.clusters(),
        "counts": writers.counts,
        "repos": generated,
        "generator": env!("CARGO_PKG_VERSION"),
        "load_secs": started.elapsed().as_secs(),
    });
    sqlx::query("INSERT INTO bench_meta (key, value) VALUES ('corpus', $1)")
        .bind(&meta)
        .execute(&pool)
        .await?;

    if !request.skip_indexes {
        build_indexes(&url).await?;
    }
    eprintln!("done in {:.0?}", started.elapsed());
    Ok(())
}

/// The two indexes the load skipped, built once, then fresh statistics.
pub async fn build_indexes(url: &str) -> Result<()> {
    let mut connection = PgConnection::connect(url).await?;
    for statement in [
        "SET maintenance_work_mem = '4GB'",
        "SET max_parallel_maintenance_workers = 4",
        "CREATE INDEX IF NOT EXISTS elements_lexical_trgm_idx ON elements \
         USING gin (lower(name || E'\\n' || raw_text) gin_trgm_ops)",
        "CREATE INDEX IF NOT EXISTS embeddings_1024_vector_idx ON embeddings_1024 \
         USING hnsw (vector vector_cosine_ops)",
        "VACUUM (ANALYZE)",
    ] {
        let started = std::time::Instant::now();
        sqlx::query(statement)
            .execute(&mut connection)
            .await
            .with_context(|| statement.to_string())?;
        eprintln!("  {statement:.60} … {:.0?}", started.elapsed());
    }
    Ok(())
}
