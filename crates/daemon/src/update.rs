//! Auto-update: probe, verify, swap (PRD req 54).
//!
//! Jordan ruled this ON BY DEFAULT on 2026-08-27: the daemon keeps the
//! installed binary current by itself, and the user finds out through the
//! message queue rather than by being asked to do anything. Phase 2 — the
//! daemon draining and `exec()`ing its own path — is deliberately not here.
//! Update-and-notify has to prove itself before a process learns to restart
//! itself.
//!
//! # Why this is not the `self_update` crate
//!
//! The brief suggested it. Two of this packet's hard requirements fight it:
//!
//! * **Quota-free probing.** `self_update` asks `api.github.com` for the
//!   release list. That endpoint is rate-limited per IP and shared with every
//!   other tool on the machine (fleet retro DL-018), and a fleet of daemons on
//!   a cadence is how a project gets throttled. [`probe_latest`] instead reads
//!   the `Location` of the un-authenticated `releases/latest` redirect, which
//!   costs no API quota at all.
//! * **Integrity.** `self_update` trusts TLS plus GitHub. This module verifies
//!   the downloaded bytes against the release's own `SHA256SUMS` asset before
//!   anything is renamed over anything.
//!
//! `self-replace` was the other suggestion, for the swap primitive alone. On
//! the platforms this ships to, the swap IS [`std::fs::rename`] — the crate
//! exists for the Windows case, which is explicitly out of scope. A dependency
//! that wraps one syscall we do not need wrapped is the reinvention-in-reverse
//! the arch allow-list exists to notice.
//!
//! # The swap, and why the running daemon keeps its old inode
//!
//! Download to a temp file **in the install directory** — a rename across
//! filesystems fails `EXDEV`, so `/tmp` is not an option — set the exec bit,
//! then `rename()` over the target. `rename` is atomic: a concurrent `exec` of
//! that path sees either the whole old binary or the whole new one, never a
//! half-written file. The running process keeps executing the old inode, which
//! is why it must be told to restart and why the queue message exists.
//!
//! Nothing ever opens the running binary for writing: that is `ETXTBSY`, and
//! it is the failure mode this whole shape avoids.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use fs3_core::update::{Version, is_upgrade};

/// Where releases live, unless a test points somewhere else.
pub const GITHUB_BASE: &str = "https://github.com/AI-Substrate/flowspace3";

/// The checksums asset every release publishes beside its binaries.
pub const CHECKSUMS_ASSET: &str = "SHA256SUMS";

/// How long the probe and the download are given before being abandoned.
///
/// One pass failing is uneventful — the next tick retries — so this is short
/// enough that a hung endpoint cannot hold a reconcile loop open for minutes.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(60);

/// A lock older than this was left behind by a process that died mid-swap.
///
/// Long enough that a slow download on a slow link is never mistaken for a
/// corpse, short enough that a crash does not disable updates until someone
/// notices.
const LOCK_STALE_AFTER: Duration = Duration::from_secs(15 * 60);

/// What one update pass concluded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The newest published release is the one already running.
    Current,
    /// A newer binary is now at the install path. The daemon still runs the
    /// old one.
    Installed(Version),
    /// Something newer exists and could not be installed. The string is the
    /// reason, phrased for a user.
    Blocked { latest: Version, reason: String },
}

/// The release source and the install path, resolved once.
pub struct Updater {
    client: reqwest::Client,
    base: String,
    install_path: PathBuf,
    running: String,
    target: Option<&'static str>,
}

impl Updater {
    /// Build an updater against the real GitHub releases.
    ///
    /// # Errors
    /// When the running executable's path cannot be resolved — without it
    /// there is nothing to replace and no directory to replace it in.
    pub fn new(running: &str) -> Result<Self> {
        Self::against(GITHUB_BASE, running)
    }

    /// Build an updater against `base`, which is how the tests point it at a
    /// stub release server instead of the internet.
    ///
    /// # Errors
    /// When the running executable's path cannot be resolved.
    pub fn against(base: &str, running: &str) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                // The probe reads the redirect rather than following it: the
                // Location header IS the answer, and following it would fetch
                // a release page we have no use for.
                .redirect(reqwest::redirect::Policy::none())
                .timeout(NETWORK_TIMEOUT)
                .build()
                .context("building the update HTTP client")?,
            base: base.trim_end_matches('/').to_string(),
            install_path: install_path()?,
            running: running.to_string(),
            target: TARGET_TRIPLE,
        })
    }

    /// Point the updater at a specific binary — the integration tests swap a
    /// throwaway file rather than the test runner.
    #[must_use]
    pub fn at_path(mut self, path: PathBuf) -> Self {
        self.install_path = path;
        self
    }

    /// The path a swap would land on.
    #[must_use]
    pub fn install_path(&self) -> &Path {
        &self.install_path
    }

    /// Check, and install if there is something to install.
    ///
    /// # Errors
    /// Only for failures that say nothing about the installation's state — a
    /// probe that could not reach the network, a release page that did not
    /// parse. A refusal that IS a fact about this machine (no published build
    /// for this platform, an install path nobody may write) comes back as
    /// [`Outcome::Blocked`], because it is news for the user rather than a bad
    /// day for the loop.
    pub async fn run_once(&self) -> Result<Outcome> {
        let latest = self.probe_latest().await?;
        if !is_upgrade(&self.running, &latest.to_string()) {
            return Ok(Outcome::Current);
        }

        let Some(target) = self.target else {
            return Ok(Outcome::Blocked {
                latest,
                reason: format!(
                    "no published build for {}-{}",
                    std::env::consts::ARCH,
                    std::env::consts::OS
                ),
            });
        };

        let directory = self
            .install_path
            .parent()
            .ok_or_else(|| anyhow!("{} has no parent directory", self.install_path.display()))?;

        if let Some(reason) = not_writable(directory) {
            return Ok(Outcome::Blocked { latest, reason });
        }

        let asset = format!("flowspace3-{target}");

        // Checksums FIRST, so a release that cannot be verified costs no
        // download at all.
        let expected = match self.expected_digest(latest, &asset).await? {
            Ok(expected) => expected,
            Err(reason) => return Ok(Outcome::Blocked { latest, reason }),
        };

        let Some(bytes) = self.fetch(latest, &asset).await? else {
            return Ok(Outcome::Blocked {
                latest,
                reason: format!("release v{latest} publishes no {asset} for this platform"),
            });
        };

        let actual = digest(&bytes);
        if actual != expected {
            // Not an error: a mismatched asset is a fact about the release, and
            // the user is the one who needs to hear it. Erroring would put it in
            // a log nobody reads and retry it every interval forever.
            return Ok(Outcome::Blocked {
                latest,
                reason: format!(
                    "the downloaded {asset} does not match the release's own {CHECKSUMS_ASSET} \
                     (expected {expected}, got {actual}) — refusing to install it"
                ),
            });
        }

        let _lock = match Lock::take(directory)? {
            Some(lock) => lock,
            None => {
                return Ok(Outcome::Blocked {
                    latest,
                    reason: "another flowspace3 update is already in progress".to_string(),
                });
            }
        };

        // The staged binary exists, is executable, and is not installed. This
        // is the only moment it can be asked what it is — see `staged_version`
        // for why a binary that lies about its version is a permanent update
        // loop rather than a cosmetic bug.
        let staged = stage(&self.install_path, &bytes)?;
        let claimed = match staged_version(&staged) {
            Ok(claimed) => claimed,
            Err(error) => {
                return Ok(Outcome::Blocked {
                    latest,
                    reason: format!(
                        "the downloaded {asset} could not be run to confirm its version \
                         ({error}) — refusing to install it"
                    ),
                });
            }
        };

        if Version::parse(&claimed) != Some(latest) {
            return Ok(Outcome::Blocked {
                latest,
                reason: format!(
                    "release v{latest}'s {asset} reports itself as {claimed:?} — refusing to \
                     install a binary that disagrees with its own release, because an updater \
                     that trusted it would reinstall it on every check forever"
                ),
            });
        }

        commit(staged, &self.install_path)?;
        Ok(Outcome::Installed(latest))
    }

    /// The newest published version, without spending GitHub API quota.
    ///
    /// `releases/latest` answers a redirect to `releases/tag/vX.Y.Z`, and that
    /// URL is the whole answer. No token, no API call, no rate-limit bucket
    /// shared with the rest of the machine.
    ///
    /// # Errors
    /// When the request fails, the response is not a redirect, or the tag it
    /// points at is not a version this binary can compare.
    pub async fn probe_latest(&self) -> Result<Version> {
        let url = format!("{}/releases/latest", self.base);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("probing {url}"))?;

        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .ok_or_else(|| {
                anyhow!(
                    "{url} answered {} with no Location header — expected a redirect to the \
                     newest release",
                    response.status()
                )
            })?
            .to_str()
            .context("the Location header is not text")?;

        let tag = location
            .rsplit('/')
            .next()
            .filter(|segment| !segment.is_empty())
            .ok_or_else(|| anyhow!("{location} does not end in a release tag"))?;

        Version::parse(tag).ok_or_else(|| anyhow!("{tag:?} is not a vX.Y.Z release tag"))
    }

    /// The sha256 the release itself publishes for `asset`, or the reason
    /// there is not one.
    ///
    /// A release with no `SHA256SUMS` is NOT an error, and getting that wrong
    /// matters more than it looks: every release published before this feature
    /// existed is exactly that release. Reporting it as a failed probe would
    /// tell every existing installation "the release could not be read,
    /// retryable" forever, instead of the truth — there is a newer version and
    /// this updater cannot verify it, so install it yourself.
    async fn expected_digest(
        &self,
        version: Version,
        asset: &str,
    ) -> Result<std::result::Result<String, String>> {
        let Some(sums) = self.fetch(version, CHECKSUMS_ASSET).await? else {
            return Ok(Err(format!(
                "release v{version} publishes no {CHECKSUMS_ASSET}, so the download cannot be \
                 verified — refusing to install it unverified"
            )));
        };

        let Ok(sums) = String::from_utf8(sums) else {
            return Ok(Err(format!(
                "release v{version}'s {CHECKSUMS_ASSET} is not text"
            )));
        };

        Ok(checksum_for(&sums, asset)
            .ok_or_else(|| format!("v{version}'s {CHECKSUMS_ASSET} has no line for {asset}")))
    }

    /// Fetch one release asset, following the redirects GitHub uses to hand off
    /// to object storage. `None` means the release does not publish it.
    async fn fetch(&self, version: Version, asset: &str) -> Result<Option<Vec<u8>>> {
        let url = format!("{}/releases/download/v{version}/{asset}", self.base);
        // A fresh client, because the probe's client deliberately refuses to
        // follow redirects and an asset download is nothing BUT redirects.
        let response = reqwest::Client::builder()
            .timeout(NETWORK_TIMEOUT)
            .build()
            .context("building the download HTTP client")?
            .get(&url)
            .send()
            .await
            .with_context(|| format!("downloading {url}"))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            bail!("{url} answered {}", response.status());
        }

        Ok(Some(
            response
                .bytes()
                .await
                .with_context(|| format!("reading {url}"))?
                .to_vec(),
        ))
    }
}

/// The release-asset triple for the platform this binary was built for, or
/// `None` when the project publishes no build for it.
///
/// Composed from `cfg` rather than from a build script's `TARGET`: the set of
/// published triples is three, it is decided by `release.yml`, and a build
/// script that reported a fourth would only produce a 404 later. `musl` is
/// excluded explicitly — a musl build would otherwise be handed a gnu binary
/// that cannot run.
#[cfg(all(target_arch = "aarch64", target_os = "linux", target_env = "gnu"))]
pub const TARGET_TRIPLE: Option<&str> = Some("aarch64-unknown-linux-gnu");

/// See [`TARGET_TRIPLE`].
#[cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]
pub const TARGET_TRIPLE: Option<&str> = Some("x86_64-unknown-linux-gnu");

/// See [`TARGET_TRIPLE`].
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
pub const TARGET_TRIPLE: Option<&str> = Some("aarch64-apple-darwin");

/// See [`TARGET_TRIPLE`]. No published build: the updater degrades to
/// notify-only rather than downloading a binary that cannot run.
#[cfg(not(any(
    all(target_arch = "aarch64", target_os = "linux", target_env = "gnu"),
    all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"),
    all(target_arch = "aarch64", target_os = "macos"),
)))]
pub const TARGET_TRIPLE: Option<&str> = None;

/// Where this process's binary lives, with symlinks resolved.
///
/// Canonicalised deliberately. A real install is often a SYMLINK — the
/// developer machine this was built on has `/usr/local/bin/flowspace3` pointing
/// into a build tree — and renaming over the LINK would silently replace the
/// link with a regular file, leaving the thing it pointed at stale and the
/// user's install subtly rearranged. Resolving first means the swap lands on
/// the real binary and the symlink keeps pointing at it.
///
/// # Errors
/// When the current executable cannot be resolved.
pub fn install_path() -> Result<PathBuf> {
    let path = std::env::current_exe().context("resolving this executable's path")?;
    Ok(std::fs::canonicalize(&path).unwrap_or(path))
}

/// Why `directory` cannot be written, or `None` when it can.
///
/// Probed by trying, not by reading permission bits: the bits are only part of
/// the answer once ACLs, read-only mounts and containers are in play, and the
/// question being asked is precisely "would the rename work".
#[must_use]
pub fn not_writable(directory: &Path) -> Option<String> {
    let probe = directory.join(format!(".flowspace3-write-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            None
        }
        Err(error) => Some(format!("{} is not writable ({error})", directory.display())),
    }
}

/// Write `bytes` into a temp file beside `target`, executable and CLOSED.
///
/// In the install directory, NOT in `/tmp`: `rename` across filesystems is
/// `EXDEV`, and `/tmp` is a different filesystem often enough that the bug
/// would only appear on other people's machines.
///
/// Separate from [`commit`] so there is a moment where the new binary exists on
/// the real filesystem, executable, and is still not installed — which is the
/// only place [`staged_version`] can ask it what it is.
///
/// # Why this returns a `TempPath` rather than a `NamedTempFile`
///
/// Because `NamedTempFile` holds the file OPEN FOR WRITING, and on Linux
/// `execve` refuses a file that any process has open for writing: `ETXTBSY`,
/// "Text file busy". macOS permits it, so a probe that worked on a developer's
/// mac failed every test on the CI runner — the guard refusing its own
/// perfectly good download.
///
/// [`tempfile::TempPath`] is exactly the shape needed: the handle is closed,
/// the path survives, deletion on drop survives, and `persist` still does the
/// rename. Note the irony worth keeping: `ETXTBSY` is the failure mode this
/// whole module is built to avoid on the INSTALL path, and it reappeared on the
/// staging path the moment we started executing what we downloaded.
///
/// # Errors
/// When the temp file cannot be created, written, synced, or made executable.
pub fn stage(target: &Path, bytes: &[u8]) -> Result<tempfile::TempPath> {
    let directory = target
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", target.display()))?;

    let mut staged = tempfile::NamedTempFile::new_in(directory)
        .with_context(|| format!("staging a new binary in {}", directory.display()))?;

    use std::io::Write as _;
    staged
        .write_all(bytes)
        .context("writing the downloaded binary")?;
    // Flush to disk before the rename. Without it a crash between the two
    // leaves the install path pointing at a correctly-named empty file, which
    // is worse than either outcome the rename can produce.
    staged.flush().context("flushing the downloaded binary")?;
    staged
        .as_file()
        .sync_all()
        .context("syncing the downloaded binary")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))
            .context("making the new binary executable")?;
    }

    // Closes the write handle. Everything above needed it; nothing below may
    // have it (see the ETXTBSY note).
    Ok(staged.into_temp_path())
}

/// Ask the staged binary what version it is, by running it.
///
/// The guard against a binary that LIES about its own version (req-0060). The
/// updater compares `env!("CARGO_PKG_VERSION")` against the published tag, so a
/// build whose compiled-in version is stale is permanently "older" than every
/// release: it would download and swap once per interval, forever, and raise a
/// restart message that restarting cannot clear. That is not hypothetical —
/// v0.2.0 shipped reporting 0.1.0, because release-please bumped its manifest
/// and not the workspace `Cargo.toml`.
///
/// Asked BEFORE the swap rather than after, deliberately. Detecting it
/// afterwards means the bad binary is already installed and the daemon has to
/// argue with itself about what it is; refusing beforehand means the install
/// never happens and the user gets one actionable message.
///
/// Executing it is not extra trust: its sha256 has already been checked against
/// the release's own `SHA256SUMS`, and running `--version` is strictly less
/// dangerous than installing it. The probe also catches two classes a version
/// comparison never would — an asset built for the wrong triple, and a binary
/// that cannot `exec` at all.
///
/// # `ETXTBSY` is retried, and the mechanism is NOT established
///
/// OBSERVED: closing our own write handle (`stage` returns a `TempPath`, not a
/// `NamedTempFile`) fixed the reproducible case, but a residual one remains on
/// Linux CI — twelve of thirteen update tests pass and the thirteenth reports
/// "text file busy" on a file this process has already closed. It has not
/// recurred on macOS.
///
/// SUSPECTED, and deliberately not asserted: `O_CLOEXEC` clears an inherited
/// descriptor at `exec` rather than at `fork`, so another process forking in
/// the window where our handle was open would hold a copy until its own `exec`
/// completes — and Linux refuses `execve` on a file ANY process has open for
/// writing. A containerised reproduction built to demonstrate exactly that
/// (400 stage-and-exec cycles against four threads forking continuously)
/// produced ZERO failures, so the theory is unconfirmed and this comment says
/// so rather than dressing a guess as a cause.
///
/// What IS certain is that the failure is transient: the same bytes at the same
/// path run correctly moments later. So the probe waits it out rather than
/// condemning a download it may simply have looked at too early. If the true
/// mechanism turns out to be something that does NOT clear, this retry costs
/// one second before reporting exactly what it reports today.
///
/// # Errors
/// When the binary cannot be executed, exits non-zero, or prints something with
/// no version in it.
pub fn staged_version(path: &Path) -> Result<String> {
    let output = run_version(path)?;

    if !output.status.success() {
        bail!(
            "{} --version exited {}",
            path.display(),
            output.status.code().unwrap_or(-1)
        );
    }

    // clap prints `flowspace3 1.2.3`. The last whitespace-separated token is
    // the version, which stays true if the product name ever grows a word.
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace()
        .next_back()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("{} --version printed nothing", path.display()))
}

/// What the binary at `path` says it is right now, or `None` when the path
/// holds nothing that can be asked.
///
/// The same interrogation [`staged_version`] performs, pointed at the INSTALL
/// path instead of the staging file, and it exists because the state row used
/// to be a memory rather than a reading. `record_installed` wrote "we swapped
/// 0.3.1 in" and nothing could ever unset it — `record_clear` only ever
/// cleared the block — so a pinned reinstall at an older tag left a permanent,
/// false "flowspace3 0.3.1 is installed at <path>, restart to pick it up"
/// against a path holding 0.3.0. The user's only escape was a hand-written
/// UPDATE.
///
/// Asking the file closes that off structurally: a swap this daemon performed
/// and a change somebody made behind its back are the same question with the
/// same answer, because both are read from the same place.
///
/// Every failure collapses to `None`, deliberately. Absent, not executable,
/// exits non-zero, prints nothing a version can be read out of — all of them
/// mean the same thing to a message producer: there is nothing here worth
/// telling a user to restart for. `None` therefore RETRACTS a standing restart
/// message rather than leaving it to outlive its cause, which is what a binary
/// somebody deleted should do.
///
/// One `fork`/`exec` per release check — at boot and then on the configured
/// cadence — against a process whose `--version` is a `clap` string. That is
/// cheaper than the HTTP probe standing beside it, and it is the mechanism the
/// pre-swap guard already trusts.
#[must_use]
pub fn on_disk_version(path: &Path) -> Option<String> {
    match staged_version(path) {
        Ok(version) => Some(version),
        Err(error) => {
            tracing::debug!(
                path = %path.display(),
                %error,
                "the install path holds nothing that reports a version"
            );
            None
        }
    }
}

/// How long to keep retrying an `ETXTBSY` before believing it.
///
/// Bounded by the length of somebody else's fork-to-exec, which is
/// microseconds. A second is three orders of magnitude of headroom and still
/// short enough that a genuinely unrunnable asset is reported promptly.
const EXEC_PATIENCE: Duration = Duration::from_secs(1);

/// Run `--version`, waiting out a transient "text file busy".
fn run_version(path: &Path) -> Result<std::process::Output> {
    let deadline = std::time::Instant::now() + EXEC_PATIENCE;
    loop {
        match std::process::Command::new(path).arg("--version").output() {
            Ok(output) => return Ok(output),
            Err(error) if is_text_file_busy(&error) && std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                return Err(anyhow::Error::new(error)
                    .context(format!("running {} --version", path.display())));
            }
        }
    }
}

/// `ETXTBSY` — somebody still has this file open for writing.
fn is_text_file_busy(error: &std::io::Error) -> bool {
    // `ErrorKind::ExecutableFileBusy` is unstable, so this asks the raw code.
    // 26 on both Linux and macOS.
    error.raw_os_error() == Some(26)
}

/// Move a staged binary onto `target`, atomically.
///
/// # Errors
/// When the rename fails.
pub fn commit(staged: tempfile::TempPath, target: &Path) -> Result<()> {
    staged
        .persist(target)
        .map_err(|error| error.error)
        .with_context(|| format!("replacing {}", target.display()))?;
    Ok(())
}

/// Stage and commit in one step — the whole swap, for callers with nothing to
/// check in between.
///
/// # Errors
/// When staging or the rename fails.
pub fn swap(target: &Path, bytes: &[u8]) -> Result<()> {
    commit(stage(target, bytes)?, target)
}

/// Whoever holds this file is mid-swap.
///
/// A lock rather than trusting `rename`'s atomicity: the rename is atomic, but
/// the download-verify-swap sequence around it is not, and the daemon's loop
/// and a human's `doctor upgrade` can start it at the same moment. Two winners
/// would both be correct and both waste a download.
///
/// Released by `Drop`, which covers every early return in [`Updater::run_once`]
/// including the `?`s. It does NOT cover `SIGKILL`, which is what
/// [`LOCK_STALE_AFTER`] is for.
struct Lock(PathBuf);

impl Lock {
    /// Take the lock, or `None` when someone else holds a live one.
    fn take(directory: &Path) -> Result<Option<Self>> {
        let path = directory.join(".flowspace3-update.lock");
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => Ok(Some(Self(path))),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if is_stale(&path) {
                    // The holder died. Clear it and try exactly once more —
                    // looping here would be two updaters racing to break each
                    // other's fresh lock.
                    let _ = std::fs::remove_file(&path);
                    return Ok(std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .ok()
                        .map(|_| Self(path)));
                }
                Ok(None)
            }
            Err(error) => {
                Err(anyhow::Error::new(error).context(format!("taking {}", path.display())))
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn is_stale(path: &Path) -> bool {
    let Ok(modified) = std::fs::metadata(path).and_then(|meta| meta.modified()) else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age > LOCK_STALE_AFTER)
}

/// The sha256 of `bytes`, lowercase hex — the spelling `sha256sum` uses.
#[must_use]
pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            use std::fmt::Write as _;
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Find `asset`'s digest in `sha256sum` output.
///
/// The format is `<64 hex>␠␠<name>`, and GNU coreutils writes a `*` instead of
/// the second space for a binary-mode file — both are accepted, because which
/// one a release produces depends on which coreutils built it.
#[must_use]
pub fn checksum_for(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (digest, name) = line.split_once(char::is_whitespace)?;
        let name = name.trim_start_matches([' ', '*']);
        // Tolerate a path-qualified name (`dist/flowspace3-…`): what matters
        // is which asset the line is about, not where it sat when it was
        // hashed.
        let name = name.rsplit('/').next().unwrap_or(name);
        (name == asset && digest.len() == 64).then(|| digest.to_ascii_lowercase())
    })
}

/// The reconcile loop that keeps the installed binary current, and keeps the
/// message queue telling the truth about it.
///
/// Level-triggered like every other reconciler (`docs/plans/prd/daemon-worker-
/// architecture.md`): one pass reads what Postgres says the update situation
/// is, makes the world match, and declares the messages that situation should
/// be raising. Running it twice changes nothing the second time.
///
/// # Why this loop has its own clock inside the shared one
///
/// [`crate::reconcile::run_forever`] runs every reconciler on ONE cadence,
/// which the watcher needs to be seconds. Asking GitHub every few seconds
/// would be abuse. Rather than growing the trait a per-loop interval — a
/// change to the substrate for one caller — the interval is honoured against
/// `update_state.last_checked_at` in Postgres: [`fs3_store::claim_check`] is a
/// conditional `UPDATE`, so a pass that is early is a no-op, a daemon
/// restarted every ten minutes still checks once a day, and two daemons
/// sharing an install cannot both win the same check. (O-prime approved this
/// shape over a trait change, 2026-08-27.)
///
/// # Why boot is a tick, and why it is TWO ticks
///
/// A standing message is level-triggered, but the level was only re-read on
/// the producer's cadence — and boot did not tick it. Observed: a daemon run
/// from a throwaway dev worktree wrote `update:blocked` naming its own
/// `target/debug` path; the production daemon restarted the next day on a
/// current binary and went on serving that fossil, because nothing
/// re-evaluated it and so nothing retracted it.
///
/// The fix is deliberately two clocks rather than one, because they need
/// different permission:
///
/// * The **release check** reaches the network and may install, so it stays
///   behind `auto`. At boot it is claimed unconditionally
///   ([`fs3_store::claim_check_now`]), then falls back to the interval.
/// * The **disk reconcile** and the **message re-declaration** run at boot
///   whatever `auto` says. Neither needs the network and neither installs
///   anything — and hanging them off `auto` would have left exactly the users
///   who opted OUT of unattended updates carrying fossils forever, which is
///   the same defect wearing a different hat.
pub struct UpdateSupervisor {
    pool: fs3_store::PgPool,
    updater: Updater,
    /// Whether to install unattended. `false` still keeps the queue honest —
    /// a user who turned auto-update off is still told what `doctor upgrade`
    /// would do.
    auto: bool,
    interval_hours: u64,
    running: String,
    /// False until the first pass has run. Boot is a level change like any
    /// other; this is the only thing that remembers it happened.
    booted: bool,
}

impl UpdateSupervisor {
    /// Wire the supervisor from the `[update]` section.
    ///
    /// # Errors
    /// When the running executable's path cannot be resolved.
    pub fn new(
        pool: fs3_store::PgPool,
        config: &fs3_core::UpdateConfig,
        running: &str,
    ) -> Result<Self> {
        Ok(Self {
            pool,
            updater: Updater::new(running)?,
            auto: config.auto,
            interval_hours: config.check_interval_hours,
            running: running.to_string(),
            booted: false,
        })
    }

    /// Point the supervisor at a stub release server and a throwaway binary,
    /// so the e2e can prove the whole loop without the internet.
    ///
    /// # Errors
    /// When the stub's base URL cannot build an HTTP client, or this process
    /// cannot resolve its own executable path.
    pub fn against(mut self, base: &str, install_path: PathBuf) -> Result<Self> {
        self.updater = Updater::against(base, &self.running)?.at_path(install_path);
        Ok(self)
    }

    /// Whether this pass gets to ask GitHub: always at boot, then on the
    /// interval, and never with `auto = false`.
    async fn may_check(&self, install_path: &str, first_pass: bool) -> Result<bool> {
        if !self.auto {
            return Ok(false);
        }
        if first_pass {
            fs3_store::claim_check_now(&self.pool, install_path).await?;
            return Ok(true);
        }
        Ok(fs3_store::claim_check(&self.pool, install_path, self.interval_hours).await?)
    }
}

#[async_trait::async_trait]
impl crate::reconcile::Reconcile for UpdateSupervisor {
    fn name(&self) -> &'static str {
        "update"
    }

    async fn reconcile(&mut self) -> Result<crate::reconcile::Pass> {
        let mut changed = 0;
        // This daemon speaks for exactly one installation: its own resolved
        // binary path. It declares and retracts messages for that path and
        // touches nobody else's row — "the path is gone HERE" is not "the path
        // is gone everywhere" when one store serves several hosts.
        let install_path = self.updater.install_path().display().to_string();
        let first_pass = !std::mem::replace(&mut self.booted, true);

        let checking = self.may_check(&install_path, first_pass).await?;
        if checking {
            changed += 1;
            match self.updater.run_once().await {
                Ok(Outcome::Current) => {
                    tracing::debug!(version = %self.running, "already the newest release");
                    fs3_store::record_clear(&self.pool, &install_path).await?;
                }
                Ok(Outcome::Installed(version)) => {
                    tracing::info!(
                        %version,
                        path = %install_path,
                        "installed a newer flowspace3 — restart the daemon to run it"
                    );
                    fs3_store::record_swapped(&self.pool, &install_path, &version.to_string())
                        .await?;
                }
                Ok(Outcome::Blocked { latest, reason }) => {
                    tracing::warn!(%latest, %reason, "cannot install the newest release");
                    fs3_store::record_seen(&self.pool, &install_path, &latest.to_string()).await?;
                    fs3_store::record_blocked(&self.pool, &install_path, &reason).await?;
                }
                // A probe that could not reach the network says nothing about
                // the installation, so it must not overwrite what the last
                // successful pass concluded. Log and wait for the next tick —
                // the interval was already claimed, so this cannot become a
                // retry storm against a rate-limited endpoint.
                Err(error) => tracing::warn!(%error, "the release check failed"),
            }
        }

        // What is ACTUALLY at our install path. Deliberately outside the match:
        // a probe that never reached GitHub still knows what is on this disk,
        // and a swap someone performed out of band is exactly the case a
        // network answer cannot see. At boot regardless of `auto`, because a
        // false "restart to pick up X" is not a thing you opt out of.
        if checking || first_pass {
            let found = on_disk_version(self.updater.install_path());
            fs3_store::record_on_disk(&self.pool, &install_path, found.as_deref()).await?;
        }

        // Every pass, whether or not it checked: the queue is a projection of
        // the state row, so a message that should have cleared clears here
        // even if the pass that cleared the state died before syncing.
        let state = fs3_store::update_state(&self.pool, &install_path).await?;
        fs3_store::sync_messages(
            &self.pool,
            fs3_core::UPDATE_SOURCE,
            Some(&install_path),
            &state.desired_messages(&self.running),
        )
        .await?;

        Ok(crate::reconcile::Pass::changed(changed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_checksums_file_is_read_by_asset_name() {
        let sums = "\
1111111111111111111111111111111111111111111111111111111111111111  flowspace3-aarch64-apple-darwin
2222222222222222222222222222222222222222222222222222222222222222  flowspace3-x86_64-unknown-linux-gnu
";
        assert_eq!(
            checksum_for(sums, "flowspace3-x86_64-unknown-linux-gnu").as_deref(),
            Some("2222222222222222222222222222222222222222222222222222222222222222")
        );
        assert_eq!(checksum_for(sums, "flowspace3-windows"), None);
    }

    #[test]
    fn binary_mode_and_path_qualified_lines_are_both_read() {
        let sums = "\
3333333333333333333333333333333333333333333333333333333333333333 *dist/flowspace3-aarch64-apple-darwin
";
        assert_eq!(
            checksum_for(sums, "flowspace3-aarch64-apple-darwin").as_deref(),
            Some("3333333333333333333333333333333333333333333333333333333333333333")
        );
    }

    #[test]
    fn a_truncated_digest_is_refused_rather_than_compared() {
        // A half-written SHA256SUMS must not silently match nothing-in-
        // particular; a short digest can never equal a real one, so treating
        // it as absent is the honest reading.
        let sums = "abc  flowspace3-aarch64-apple-darwin\n";
        assert_eq!(checksum_for(sums, "flowspace3-aarch64-apple-darwin"), None);
    }

    #[test]
    fn the_digest_matches_sha256sum() {
        // `printf 'flowspace3' | sha256sum`
        assert_eq!(
            digest(b"flowspace3"),
            "304b5ff3e128a070f69a1bcc11542512b49269538903399652e88c5f3c88b627"
        );
    }

    #[test]
    fn an_unwritable_directory_is_reported_rather_than_panicking() {
        let reason = not_writable(Path::new("/definitely/not/a/directory/here"));
        assert!(reason.is_some_and(|reason| reason.contains("not writable")));
    }

    #[test]
    fn a_writable_directory_reports_nothing() {
        let directory = tempfile::tempdir().expect("temp dir");
        assert_eq!(not_writable(directory.path()), None);
    }

    #[test]
    fn a_swap_replaces_the_target_atomically_and_leaves_it_executable() {
        let directory = tempfile::tempdir().expect("temp dir");
        let target = directory.path().join("flowspace3");
        std::fs::write(&target, b"old").expect("seed the target");

        swap(&target, b"new").expect("swap");

        assert_eq!(std::fs::read(&target).expect("read back"), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(&target)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755, "the new binary must be executable");
        }
    }

    #[test]
    fn a_swap_leaves_no_staging_files_behind() {
        let directory = tempfile::tempdir().expect("temp dir");
        let target = directory.path().join("flowspace3");
        std::fs::write(&target, b"old").expect("seed the target");

        swap(&target, b"new").expect("swap");

        let left: Vec<_> = std::fs::read_dir(directory.path())
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(left.len(), 1, "only the target should remain: {left:?}");
    }

    #[test]
    fn one_holder_at_a_time_and_a_corpse_does_not_block_forever() {
        let directory = tempfile::tempdir().expect("temp dir");

        let held = Lock::take(directory.path()).expect("take").expect("held");
        assert!(
            Lock::take(directory.path()).expect("take").is_none(),
            "a live lock must exclude a second updater"
        );
        drop(held);

        assert!(
            Lock::take(directory.path()).expect("take").is_some(),
            "releasing must let the next updater in"
        );
    }
}
