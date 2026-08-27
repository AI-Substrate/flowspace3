//! Schema skew: the database moving on without the binary (PRD reqs 59, 61).
//!
//! The condition is manufactured the way it actually happens — a migration row
//! this binary does not carry, exactly what a newer `doctor` or a colleague's
//! daemon leaves behind — rather than by stubbing the detection out. What is
//! being proven is that a real `_sqlx_migrations` table in that state produces
//! the right words in the right places.

use fs3_daemon::reconcile::Reconcile;
use fs3_daemon::skew::SchemaSupervisor;

mod support;

/// Pretend a newer binary migrated this database. `version` is deliberately
/// far above anything fs3 bundles, so the fixture cannot collide with a real
/// migration added later.
async fn pretend_a_newer_binary_migrated(pool: &fs3_store::PgPool, version: i64) {
    sqlx::query(
        "INSERT INTO _sqlx_migrations
             (version, description, installed_on, success, checksum, execution_time)
         VALUES ($1, 'from a newer flowspace3', now(), true, '\\x00', 0)",
    )
    .bind(version)
    .execute(pool)
    .await
    .expect("seeding a migration this binary does not carry");
}

/// Any installation reading the queue. Schema messages are scoped to no
/// install — they are a fact about the STORE — so which path asks is exactly
/// the thing that must not matter here.
const ANY_INSTALL: &str = "/usr/local/bin/flowspace3";

/// The producer's whole contract in one test: it appears when the database gets
/// ahead, it does not duplicate, and it RETRACTS itself when the condition
/// clears — with no clear-condition machinery anywhere.
#[tokio::test]
async fn the_schema_producer_raises_and_then_retracts_as_the_database_moves() {
    let database = support::FreshDatabase::create("schemaskew").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    let mut supervisor = SchemaSupervisor::new(pool.clone(), "0.2.0");

    // A daemon whose binary matches its database says nothing at all. This is
    // the steady state and it must stay silent, or the queue becomes noise.
    let pass = supervisor.reconcile().await.expect("the healthy pass");
    assert_eq!(pass.changed, 0);
    assert!(
        fs3_store::live_messages(&pool, ANY_INSTALL)
            .await
            .expect("the queue")
            .is_empty(),
        "a daemon that understands its schema has nothing to report"
    );

    // Somebody runs a newer doctor against the same store.
    pretend_a_newer_binary_migrated(&pool, 9001).await;

    let pass = supervisor.reconcile().await.expect("the skewed pass");
    assert_eq!(pass.changed, 1);

    let messages = fs3_store::live_messages(&pool, ANY_INSTALL)
        .await
        .expect("the queue");
    assert_eq!(messages.len(), 1, "expected one message, got {messages:?}");
    let message = &messages[0];
    assert_eq!(message.key, "schema:ahead:9001");
    assert_eq!(message.source, "schema");
    assert_eq!(
        message.severity,
        fs3_core::Severity::Error,
        "this daemon is writing to a schema it does not understand"
    );
    assert!(
        message.text.contains("OLDER than its database"),
        "the message names the case: {}",
        message.text
    );
    assert!(message.next_action.contains("doctor upgrade"));
    assert!(
        !message.next_action.contains("docker compose"),
        "the store is healthy — steering at it is the defect being fixed"
    );

    // Declaring the same thing again is not a second message.
    supervisor.reconcile().await.expect("the repeat pass");
    assert_eq!(
        fs3_store::live_messages(&pool, ANY_INSTALL)
            .await
            .expect("the queue")
            .len(),
        1,
        "re-declaring must not duplicate"
    );

    // The situation resolves — here, by this process being the newer binary.
    // Nothing acks, nothing expires, nothing evaluates a rule: the producer
    // simply stops declaring it.
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 9001")
        .execute(&pool)
        .await
        .expect("removing the foreign migration");

    supervisor.reconcile().await.expect("the recovered pass");
    assert!(
        fs3_store::live_messages(&pool, ANY_INSTALL)
            .await
            .expect("the queue")
            .is_empty(),
        "the message must retract itself once the skew is gone"
    );

    database.destroy(pool).await;
}

/// Two producers, two sources, one queue. The seam test: the update producer
/// must not retract the schema producer's message when it declares its own.
///
/// Now also a SCOPE test. The schema producer speaks for the whole store and
/// scopes its messages `None`; the update producer speaks for one install path.
/// A reader at that path must see both — a store-wide condition is news for
/// every installation pointed at it, and scoping must narrow who owns a row,
/// not who is allowed to hear about the store they are using.
#[tokio::test]
async fn one_producer_declaring_does_not_retract_another_producers_message() {
    let database = support::FreshDatabase::create("twosources").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool).await.expect("migrations");

    pretend_a_newer_binary_migrated(&pool, 9002).await;
    SchemaSupervisor::new(pool.clone(), "0.2.0")
        .reconcile()
        .await
        .expect("the schema pass");

    // The update feature declares its own, unrelated, state, for one install.
    let update = fs3_core::UpdateState {
        installed_version: Some("0.3.0".to_string()),
        install_path: "/usr/local/bin/flowspace3".to_string(),
        ..fs3_core::UpdateState::default()
    };
    fs3_store::sync_messages(
        &pool,
        fs3_core::UPDATE_SOURCE,
        Some(&update.install_path),
        &update.desired_messages("0.2.0"),
    )
    .await
    .expect("the update pass");

    let sources: Vec<&str> = {
        let messages = fs3_store::live_messages(&pool, &update.install_path)
            .await
            .expect("the queue");
        assert_eq!(messages.len(), 2, "both must survive: {messages:?}");
        messages
            .iter()
            .map(|message| message.source.clone().leak() as &str)
            .collect()
    };
    assert!(sources.contains(&"schema"));
    assert!(sources.contains(&"update"));

    // A DIFFERENT install hears the store-wide news and none of the other
    // install's. This is the half that makes the scope worth having: a schema
    // skew is everyone's problem, a binary at somebody else's path is not.
    let elsewhere = fs3_store::live_messages(&pool, "/home/alice/.local/bin/flowspace3")
        .await
        .expect("the queue");
    assert_eq!(
        elsewhere.len(),
        1,
        "only the store-wide message crosses installs: {elsewhere:?}"
    );
    assert_eq!(elsewhere[0].source, "schema");

    database.destroy(pool).await;
}
