#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use fs3_core::RepoIdentity;
use fs3_daemon::roots::ScanFileJob;
use fs3_testkit::{FreshDatabase, TestDatabase};

const PATIENCE: Duration = Duration::from_secs(60);

struct Daemon {
    child: Child,
    log: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn temp_dir(label: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "fs3-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).expect("creating isolated test directory");
    path
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("binding an ephemeral port")
        .local_addr()
        .expect("reading the ephemeral port")
        .port()
}

fn spawn(mut command: Command, log: PathBuf) -> Daemon {
    let output = std::fs::File::create(&log).expect("creating daemon log");
    command
        .env("RUST_LOG", "fs3_daemon=info")
        .stdout(Stdio::from(output.try_clone().expect("cloning daemon log")))
        .stderr(Stdio::from(output));
    Daemon {
        child: command.spawn().expect("starting daemon"),
        log,
    }
}

fn signal(daemon: &Daemon, name: &str) {
    let status = Command::new("/bin/kill")
        .arg(format!("-{name}"))
        .arg(daemon.child.id().to_string())
        .status()
        .expect("sending shutdown signal");
    assert!(status.success(), "kill -{name} failed: {status}");
}

async fn wait_for_exit(daemon: &mut Daemon) -> ExitStatus {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(status) = daemon.child.try_wait().expect("polling daemon") {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "daemon did not exit; log:\n{}",
            log(daemon)
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn log(daemon: &Daemon) -> String {
    std::fs::read_to_string(&daemon.log).unwrap_or_default()
}

async fn wait_for_log(daemon: &mut Daemon, needle: &str) -> String {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let text = log(daemon);
        if text.contains(needle) {
            return text;
        }
        if let Some(status) = daemon.child.try_wait().expect("polling daemon") {
            panic!("daemon exited {status} before {needle:?}; log:\n{text}");
        }
        assert!(
            Instant::now() < deadline,
            "missing {needle:?}; log:\n{text}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn ready_field(line: &str, name: &str) -> String {
    line.split_whitespace()
        .find_map(|word| word.strip_prefix(&format!("{name}=")))
        .map(|value| {
            value
                .trim_matches(|c: char| c == '\"' || c == ',')
                .to_string()
        })
        .unwrap_or_else(|| panic!("sandbox ready line has no {name}= field: {line}"))
}

async fn authenticated_health(port: u16, key: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/health"))
        .bearer_auth(key.trim())
        .send()
        .await
        .expect("daemon health request")
}

async fn wait_for_health(daemons: &mut [&mut Daemon], port: u16, key_path: &Path) -> String {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let key = std::fs::read_to_string(key_path).ok();
        if let Some(key) = key
            && let Ok(response) = reqwest::Client::new()
                .get(format!("http://127.0.0.1:{port}/health"))
                .bearer_auth(key.trim())
                .send()
                .await
            && response.status().is_success()
        {
            return key;
        }
        let statuses: Vec<_> = daemons
            .iter_mut()
            .map(|daemon| daemon.child.try_wait().expect("polling daemon"))
            .collect();
        assert!(
            !statuses.iter().all(Option::is_some),
            "both daemons exited before health answered: {statuses:?}"
        );
        assert!(Instant::now() < deadline, "daemon never became healthy");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn a_port_race_loser_never_replaces_the_winners_published_key() {
    let database = FreshDatabase::create("daemon-key-race").await;
    let pool = database.pool().await;
    fs3_store::migrate(&pool)
        .await
        .expect("pre-migrating private database");
    pool.close().await;

    let config = temp_dir("daemon-key-race-config");
    let port = free_port();
    std::fs::write(
        config.join(fs3_core::CONFIG_FILE_NAME),
        format!(
            "[daemon]\nurl = \"http://127.0.0.1:{port}\"\n\n[database]\nurl = \"{}\"\n",
            database.url()
        ),
    )
    .expect("writing isolated daemon config");

    let binary = fs3_testkit::flowspace3_binary();
    let mut first_command = fs3_testkit::sealed(&binary, &config, TestDatabase::FromConfigFile);
    first_command.arg("daemon");
    let mut second_command = fs3_testkit::sealed(&binary, &config, TestDatabase::FromConfigFile);
    second_command.arg("daemon");
    let mut first = spawn(first_command, config.join("first.log"));
    let mut second = spawn(second_command, config.join("second.log"));

    let key_path = fs3_core::daemon_key_path(&config);
    let original = wait_for_health(&mut [&mut first, &mut second], port, &key_path).await;
    let deadline = Instant::now() + PATIENCE;
    let (winner, loser_status) = loop {
        let first_status = first.child.try_wait().expect("polling first daemon");
        let second_status = second.child.try_wait().expect("polling second daemon");
        match (first_status, second_status) {
            (Some(status), None) => break (&mut second, status),
            (None, Some(status)) => break (&mut first, status),
            (Some(a), Some(b)) => panic!("both race participants exited: {a}, {b}"),
            (None, None) => {}
        }
        assert!(Instant::now() < deadline, "port-race loser did not exit");
        tokio::time::sleep(Duration::from_millis(25)).await;
    };
    assert!(!loser_status.success(), "the bind loser must fail startup");
    assert_eq!(
        std::fs::read_to_string(&key_path).expect("reading published key"),
        original,
        "a failed bind must discard only its unpublished staged key"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(
            std::fs::metadata(&key_path)
                .expect("key metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert!(
        authenticated_health(port, &original)
            .await
            .status()
            .is_success()
    );

    signal(winner, "TERM");
    assert!(wait_for_exit(winner).await.success());
    database.cleanup().await.expect("dropping private database");
    std::fs::remove_dir_all(config).ok();
}

async fn sandbox_session(label: &str, force: bool) {
    let base = FreshDatabase::create(label).await;
    let ambient = temp_dir(label);
    std::fs::write(
        ambient.join(fs3_core::CONFIG_FILE_NAME),
        format!(
            r#"[database]
url = "{}"

[providers.ambient-paid]
kind = "openai_compat"
base_url = "https://example.invalid/v1"
model = "paid"
api_key_env = "FS3_AMBIENT_KEY_THAT_IS_NOT_SET"

[embedder]
active = "ambient-paid"
[summarizer]
active = "ambient-paid"
[agent]
active = "ambient-paid"
"#,
            base.url()
        ),
    )
    .expect("writing hostile ambient config");

    let binary = fs3_testkit::flowspace3_binary();
    let mut command = fs3_testkit::sealed(&binary, &ambient, TestDatabase::FromConfigFile);
    command.args(["daemon", "--sandbox"]);
    let mut daemon = spawn(command, ambient.join("sandbox.log"));
    let text = wait_for_log(&mut daemon, "sandbox=true").await;
    let ready = text
        .lines()
        .find(|line| line.contains("sandbox=true"))
        .expect("sandbox ready line");
    assert!(ready.contains("embedder=fake"), "{ready}");
    assert!(ready.contains("summarizer=fake"), "{ready}");
    let database_name = ready_field(ready, "db");
    let port: u16 = ready_field(ready, "port").parse().expect("ready port");
    let sandbox_config = PathBuf::from(ready_field(ready, "config"));
    let key = std::fs::read_to_string(fs3_core::daemon_key_path(&sandbox_config))
        .expect("ready means the key is published");
    assert!(authenticated_health(port, &key).await.status().is_success());

    let sandbox_url = fs3_store::database_url(&base.url(), &database_name)
        .expect("building sandbox database URL");
    let pool = fs3_store::connect(&sandbox_url)
        .await
        .expect("connecting to sandbox database");
    let root = temp_dir(&format!("{label}-root"));
    let identity = RepoIdentity::from_path(&root);
    let worktree_id = fs3_store::register_worktree(
        &pool,
        &identity,
        root.to_str().expect("utf-8 root"),
        Some("main"),
    )
    .await
    .expect("registering blocked root");
    for index in 0..8 {
        let path = format!("busy-{index}.rs");
        let line = format!("pub fn busy_{index}() {{}}\n");
        let text = line.repeat(10_000);
        std::fs::write(root.join(&path), &text).expect("writing busy source");
        let job = ScanFileJob {
            worktree_id,
            identity: identity.to_string(),
            path,
            blob: fs3_core::content_hash(text.as_bytes()),
        };
        fs3_store::enqueue_job(
            &pool,
            fs3_daemon::roots::SCAN_FILE,
            &job.dedupe_key(),
            &serde_json::to_value(job).expect("scan payload"),
            Duration::ZERO,
        )
        .await
        .expect("seeding busy runner");
    }
    let deadline = Instant::now() + PATIENCE;
    loop {
        let running: i64 = fs3_store::queue_depth(&pool)
            .await
            .expect("queue depth")
            .into_iter()
            .filter(|row| row.state == "running")
            .map(|row| row.depth)
            .sum();
        if running > 0 {
            break;
        }
        assert!(Instant::now() < deadline, "runner never claimed busy work");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    signal(&daemon, "TERM");
    wait_for_log(&mut daemon, "draining ").await;
    if force {
        signal(&daemon, "INT");
    }
    let status = wait_for_exit(&mut daemon).await;
    assert!(
        status.success(),
        "{} shutdown failed: {status}",
        if force {
            "forced structured"
        } else {
            "graceful"
        }
    );
    pool.close().await;

    let (maintenance_url, _) = fs3_store::maintenance_url(&base.url()).expect("maintenance URL");
    let admin = fs3_store::connect(&maintenance_url)
        .await
        .expect("connecting to maintenance DB");
    assert!(
        !fs3_store::database_exists(&admin, &database_name)
            .await
            .expect("checking sandbox cleanup"),
        "sandbox database {database_name} leaked; log:\n{}",
        log(&daemon)
    );
    admin.close().await;
    assert!(log(&daemon).contains("sandbox database dropped"));
    base.cleanup()
        .await
        .expect("dropping private base database");
    std::fs::remove_dir_all(root).ok();
    std::fs::remove_dir_all(ambient).ok();
}

#[tokio::test]
async fn sandbox_ready_sigterm_busy_drain_and_database_drop_compose() {
    sandbox_session("sandbox-term", false).await;
}

#[tokio::test]
async fn a_second_signal_cancels_busy_work_but_still_drops_the_database() {
    sandbox_session("sandbox-second-signal", true).await;
}
