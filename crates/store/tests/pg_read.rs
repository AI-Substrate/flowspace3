//! The read surface's store queries, against a real Postgres.
//!
//! Three of these functions do prefix matching, and prefix matching is where
//! path bugs live: `/srv/repo` must not claim `/srv/repo-two`, and a path
//! containing `_` must not behave as a `LIKE` wildcard. Both are silent when
//! wrong — the query still returns rows, just the wrong ones — so they are
//! proven here rather than reasoned about.

mod support;

use fs3_core::{Element, ElementKind, RepoIdentity, Span};
use fs3_store::{
    count_files_under, files_at_path, files_under, parser_versions_for_blob, register_worktree,
    repo_identities, sync_worktree_files, upsert_element_tree, worktree_containing,
};
use support::{FreshDatabase, PARSER_VERSION, unique_blob};

/// A migrated throwaway database.
async fn fresh() -> (FreshDatabase, fs3_store::PgPool) {
    let database = FreshDatabase::create().await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool)
        .await
        .expect("a fresh database migrates");
    (database, pool)
}

/// Register a root with the given files, returning its worktree id.
async fn seed(
    pool: &fs3_store::PgPool,
    identity: &str,
    root: &str,
    files: &[(&str, fs3_core::BlobRef)],
) -> i64 {
    let identity = RepoIdentity::from_remote_parts(Some("host"), identity)
        .expect("a remote identity has parts");
    let worktree = register_worktree(pool, &identity, root, Some("main"))
        .await
        .expect("registers");
    let owned: Vec<(String, fs3_core::BlobRef)> = files
        .iter()
        .map(|(path, blob)| ((*path).to_string(), blob.clone()))
        .collect();
    sync_worktree_files(pool, worktree, &owned)
        .await
        .expect("syncs the file map");
    worktree
}

/// The boundary rule, in the case that breaks a naive prefix test: one root is
/// a string prefix of another, and only the real ancestor may match.
#[tokio::test]
async fn a_path_resolves_to_the_root_it_is_actually_inside() {
    let (database, pool) = fresh().await;

    seed(&pool, "org/one", "/srv/repo", &[("a.rs", unique_blob())]).await;
    seed(
        &pool,
        "org/two",
        "/srv/repo-two",
        &[("b.rs", unique_blob())],
    )
    .await;

    let inside = worktree_containing(&pool, "/srv/repo/src/deep")
        .await
        .expect("query")
        .expect("that path is inside the first root");
    assert_eq!(inside.root_path, "/srv/repo");

    // The trap: `/srv/repo-two` starts with `/srv/repo`, and a `LIKE 'root%'`
    // would put it in the wrong repository entirely.
    let sibling = worktree_containing(&pool, "/srv/repo-two/src")
        .await
        .expect("query")
        .expect("that path is inside the second root");
    assert_eq!(sibling.root_path, "/srv/repo-two");

    // The root itself is inside itself.
    let exact = worktree_containing(&pool, "/srv/repo")
        .await
        .expect("query")
        .expect("a root contains itself");
    assert_eq!(exact.root_path, "/srv/repo");

    assert!(
        worktree_containing(&pool, "/somewhere/else")
            .await
            .expect("query")
            .is_none(),
        "and somewhere fs3 was never told about is None, not a guess"
    );

    database.destroy(pool).await;
}

/// A root registered INSIDE another is the more specific answer: the caller is
/// in both, and the nested one is the one they added on purpose.
#[tokio::test]
async fn the_longest_matching_root_wins() {
    let (database, pool) = fresh().await;

    seed(&pool, "org/outer", "/srv/mono", &[("a.rs", unique_blob())]).await;
    seed(
        &pool,
        "org/inner",
        "/srv/mono/packages/api",
        &[("b.rs", unique_blob())],
    )
    .await;

    let found = worktree_containing(&pool, "/srv/mono/packages/api/src")
        .await
        .expect("query")
        .expect("inside both");
    assert_eq!(found.root_path, "/srv/mono/packages/api");

    database.destroy(pool).await;
}

/// `files_under` is a path prefix, on segment boundaries, and a path is DATA:
/// an underscore in a filename is a character, not a single-character wildcard.
#[tokio::test]
async fn a_prefix_listing_stops_at_segment_boundaries_and_never_globs() {
    let (database, pool) = fresh().await;

    seed(
        &pool,
        "org/prefixes",
        "/srv/prefixes",
        &[
            ("src/lib.rs", unique_blob()),
            ("src/read.rs", unique_blob()),
            ("src-generated/other.rs", unique_blob()),
            ("my_file.rs", unique_blob()),
            ("myXfile.rs", unique_blob()),
        ],
    )
    .await;
    let identity = "git:host/org/prefixes";

    let under_src: Vec<String> = files_under(&pool, Some(identity), Some("src"), 100)
        .await
        .expect("query")
        .into_iter()
        .map(|file| file.path)
        .collect();
    assert_eq!(under_src, vec!["src/lib.rs", "src/read.rs"]);
    assert_eq!(
        count_files_under(&pool, Some(identity), Some("src"))
            .await
            .expect("count"),
        2,
        "the count and the listing must agree about what is under a prefix"
    );

    // `my_file.rs` as a prefix must not also match `myXfile.rs`.
    let literal: Vec<String> = files_under(&pool, Some(identity), Some("my_file.rs"), 100)
        .await
        .expect("query")
        .into_iter()
        .map(|file| file.path)
        .collect();
    assert_eq!(literal, vec!["my_file.rs"]);

    let whole = count_files_under(&pool, Some(identity), None)
        .await
        .expect("count");
    assert_eq!(whole, 5, "no prefix is the whole repository");

    database.destroy(pool).await;
}

/// One path, two repositories: `get` needs to see BOTH to be able to report
/// the ambiguity rather than picking one.
#[tokio::test]
async fn one_path_in_two_repositories_returns_both() {
    let (database, pool) = fresh().await;

    let mine = unique_blob();
    let theirs = unique_blob();
    seed(&pool, "org/mine", "/srv/mine", &[("src/lib.rs", mine)]).await;
    seed(
        &pool,
        "org/theirs",
        "/srv/theirs",
        &[("src/lib.rs", theirs)],
    )
    .await;

    let both = files_at_path(&pool, None, "src/lib.rs")
        .await
        .expect("query");
    assert_eq!(both.len(), 2);

    let one = files_at_path(&pool, Some("git:host/org/mine"), "src/lib.rs")
        .await
        .expect("query");
    assert_eq!(one.len(), 1, "and the repo filter narrows it to one");
    assert_eq!(one[0].identity, "git:host/org/mine");

    let identities = repo_identities(&pool).await.expect("query");
    assert!(
        identities
            .windows(2)
            .all(|pair| pair[0].len() >= pair[1].len()),
        "identities come back longest first, because that is the order an \
         address is resolved in: {identities:?}"
    );

    database.destroy(pool).await;
}

/// After a parser bump the old rows survive until a re-scan. The reader must
/// be able to see BOTH versions, newest first, or every address in the index
/// 404s during that window.
#[tokio::test]
async fn parser_versions_come_back_most_recently_written_first() {
    let (database, pool) = fresh().await;

    let blob = unique_blob();
    let file = |version: &str| {
        Element::new(
            ElementKind::File,
            "rust",
            "lib.rs",
            "src/lib.rs",
            Span::new(1, 3),
            format!("// parsed by {version}\n"),
        )
    };

    upsert_element_tree(&pool, &blob, PARSER_VERSION, &file(PARSER_VERSION), |_| {
        false
    })
    .await
    .expect("writes the first tree");
    let newer = "test-parser@2";
    upsert_element_tree(&pool, &blob, newer, &file(newer), |_| false)
        .await
        .expect("writes the second tree");

    let versions = parser_versions_for_blob(&pool, blob.as_str())
        .await
        .expect("query");
    assert_eq!(versions, vec![newer, PARSER_VERSION]);

    assert!(
        parser_versions_for_blob(&pool, &"f".repeat(40))
            .await
            .expect("query")
            .is_empty(),
        "a blob nobody parsed has no versions, rather than an error"
    );

    database.destroy(pool).await;
}
