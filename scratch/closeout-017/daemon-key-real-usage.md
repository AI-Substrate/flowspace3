# daemon-key real-usage transcript — TEST setup only

All processes used scratch `FS3_CONFIG_DIR=/tmp/fs3-daemon-key-20260902-0810/config`, the dedicated `:5434` test postmaster, a per-run database `fs3_daemon_key_20260902_0810`, and ephemeral port `63359`. Prod `:7373`, prod DB `:5433`, and `~/.config/flowspace3` were untouched. The per-run database was dropped after the transcript.

## Daemon A

```text
$ FS3_CONFIG_DIR=/tmp/fs3-daemon-key-20260902-0810/config \
  FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test \
  target/debug/flowspace3 daemon
ready: 127.0.0.1:63359

$ target/debug/flowspace3 ping --json
healthy - fs3 daemon 0.5.0 at http://localhost:63359 (embedder: fake, summarizer: fake)
```

## Shared key before daemon B

```text
1788336617 /tmp/fs3-daemon-key-20260902-0810/config/daemon.key
3d50f80e19d92941337ab505b205d43c01362f2751c3ee12e2ce679767c8e576  /tmp/fs3-daemon-key-20260902-0810/config/daemon.key
```

## Daemon B from a foreign cwd

```text
$ cd /tmp/fs3-daemon-key-20260902-0810/foreign
$ FS3_CONFIG_DIR=/tmp/fs3-daemon-key-20260902-0810/config \
  FS3_TEST_DATABASE_URL=postgres://flowspace3:flowspace3@127.0.0.1:5434/flowspace3_test \
  /Users/jordanknight/substrate/flowspace/fs3-daemon-key-after-bind/target/debug/flowspace3 daemon --json
...
flowspace3: cannot bind 127.0.0.1:63359: Address already in use (os error 48)
exit: 1
```

## Daemon A and key after daemon B

```text
$ target/debug/flowspace3 ping --json
healthy - fs3 daemon 0.5.0 at http://localhost:63359 (embedder: fake, summarizer: fake)

$ stat .../daemon.key && sha256sum .../daemon.key
1788336617 /tmp/fs3-daemon-key-20260902-0810/config/daemon.key
3d50f80e19d92941337ab505b205d43c01362f2751c3ee12e2ce679767c8e576  /tmp/fs3-daemon-key-20260902-0810/config/daemon.key

$ list config directory
['config.toml', 'daemon.key']
```

Verdict: daemon B exited non-zero; daemon A remained authorized; key mtime and SHA-256 were byte-for-byte unchanged; no staged temp file remained.

Gate environment note: local `harness checks` mints a unique `fs3_test_<epoch>_<entropy>` child database and injects that URL, so it is not `DatabaseConfig::DEFAULT_URL`; CI supplies the prod-spelled `:5433/flowspace3` anchor directly. That input difference is why local was green while CI initially exercised the owner refusal. The health test now mints its own child in either environment.
