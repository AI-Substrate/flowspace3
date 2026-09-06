//! A booted poller must not ingest transcripts outside the test's sealed HOME.

use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

struct Daemon(Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn enabled_daemon_ignores_native_sessions_outside_its_sealed_home() {
    let ambient = tempfile::tempdir().unwrap();
    let config_dir = tempfile::tempdir().unwrap();
    let outside = ambient
        .path()
        .join(".claude/projects/-outside/outside-session.jsonl");
    std::fs::create_dir_all(outside.parent().unwrap()).unwrap();
    std::fs::write(
        &outside,
        format!(
            "{}\n",
            serde_json::json!({
                "type":"user", "uuid":"outside-turn", "cwd":ambient.path(),
                "timestamp":"2026-09-06T00:00:00Z",
                "message":{"role":"user","content":"must never enter this test database"}
            })
        ),
    )
    .unwrap();
    // SAFETY: this binary has one synchronous test. Set its fake ambient HOME
    // before creating the runtime or threads; never read the developer's HOME.
    unsafe { std::env::set_var("HOME", ambient.path()) };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let database = fs3_testkit::FreshDatabase::create("convo-home-isolation").await;
        let pool = fs3_store::connect(&database.url()).await.unwrap();
        let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        let mut config = fs3_core::Config::default();
        config.database.url = database.url();
        config.daemon.url = format!("http://{address}");
        config.daemon.log_dir = config_dir
            .path()
            .join("logs")
            .to_string_lossy()
            .into_owned();
        config.indexing.conversation_poll_ticks = 1;
        config.update.auto = false;
        std::fs::write(
            config_dir.path().join("config.toml"),
            toml::to_string(&config).unwrap(),
        )
        .unwrap();
        let log_path = config_dir.path().join("daemon.log");
        let log = std::fs::File::create(&log_path).unwrap();
        let mut command = fs3_testkit::sealed(
            Path::new(env!("CARGO_BIN_EXE_flowspace3")),
            config_dir.path(),
            fs3_testkit::TestDatabase::FromConfigFile,
        );
        let sealed_home = config_dir.path().join("home");
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "HOME" && value == Some(sealed_home.as_os_str()))
        );
        assert!(sealed_home.is_dir());
        drop(reservation);
        let mut daemon = Daemon(
            command
                .arg("daemon")
                .current_dir(config_dir.path())
                .stdout(Stdio::from(log.try_clone().unwrap()))
                .stderr(Stdio::from(log))
                .spawn()
                .unwrap(),
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut first_poll = None;
        loop {
            assert!(
                daemon.0.try_wait().unwrap().is_none(),
                "daemon exited: {}",
                std::fs::read_to_string(&log_path).unwrap()
            );
            assert!(
                Instant::now() < deadline,
                "two real polls were not observed: {}",
                std::fs::read_to_string(&log_path).unwrap()
            );
            if let Ok(key) = std::fs::read_to_string(fs3_core::daemon_key_path(config_dir.path()))
                && let Ok(response) = client
                    .get(format!("http://{address}/status"))
                    .bearer_auth(key.trim())
                    .send()
                    .await
                && let Ok(envelope) = response.json::<serde_json::Value>().await
                && let Some(polled_at) = envelope["data"]["conversations"]["last_poll_at"].as_str()
            {
                assert_eq!(envelope["data"]["conversations"]["state"], "flowing");
                for harness in envelope["data"]["conversations"]["harnesses"]
                    .as_array()
                    .unwrap()
                {
                    assert_eq!(
                        harness["tracked"], 0,
                        "outside the sealed home is not a session source"
                    );
                }
                if first_poll
                    .as_deref()
                    .is_some_and(|first| first != polled_at)
                {
                    break;
                }
                first_poll.get_or_insert_with(|| polled_at.to_owned());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let count: i64 = fs3_store::queue_depth_history(&pool)
            .await
            .unwrap()
            .into_iter()
            .filter(|row| row.kind == "ingest_session")
            .map(|row| row.depth)
            .sum();
        assert_eq!(
            count, 0,
            "an enabled poller must never enqueue an unseeded native session"
        );
        drop(daemon);
        database.destroy(pool).await;
    });
}
