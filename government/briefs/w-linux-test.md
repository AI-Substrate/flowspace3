# Brief: w-linux-test — standing Linux verification seat

**Seat**: (fill at canary). **Standing role, not a one-shot packet**: you are the fleet's
Linux tester. Every release and every risky merge gets proven on Linux by YOU, as a real
outside user would experience it — from-scratch installs, real installer, real releases,
your own daemon and database.

## Isolation model (Jordan ruled the need; this is how we do it TODAY)

Run everything inside Linux containment so nothing touches the mac host's stack:

- Primary vehicle: docker on the mac host running `ubuntu:24.04` (arm64 native) — the
  proven from-scratch shape (see `scratchpad` history: real curl|sh, doctor, daemon, add,
  status, search all inside one container with `--network host` REMOVED — use the
  container's own network + its own Postgres via the compose stack `doctor` provisions
  INSIDE the container, so ports/databases never collide with the host daemon).
  IMPORTANT LESSON (retro): yesterday's proof used host networking and its `add /demo`
  polluted the REAL database. Never mount the host docker socket + host network together
  for a test that writes. Fully-nested (docker-in-docker via `-v /var/run/docker.sock`)
  is acceptable ONLY with a uniquely-named compose project + non-default PG port, and
  the teardown must remove what it created.
- The OrbStack Ubuntu VM is available for VM-grade checks (systemd, real service shape).
- **PORT LEAKAGE WARNING (Jordan, 2026-08-27)**: OrbStack auto-forwards container/VM
  ports to the mac host. "Inside a container" does NOT mean "off the host's ports" —
  a test Postgres published on 5433 collides with the host's real flowspace3-db.
  Therefore: never publish default ports. Use non-default host-side ports (or no
  published ports at all — talk to services over the container network), uniquely-named
  compose projects/volumes, and VERIFY isolation before any write test: prove your test
  daemon's database is yours (e.g. its repos table is empty) before `add`ing anything.
- req-0056 (instance profiles) will make this first-class later; until then containment
  + non-default ports IS the profile.

## Standing duties

1. **Release verification** (fires when o-prime pings you with a version): from-scratch
   README walkthrough on clean Ubuntu — curl|sh install, `flowspace3 --version`,
   `agents-start-here`, `doctor` (provisions its own stack), `daemon`, `add`, `status`,
   `search`. Report PASS/FAIL with the exact command + envelope on any failure.
2. **AUTO-UPDATE proof (the headline duty, new this release)**: once v0.3.0 ships —
   install it clean, confirm `doctor` shows the update row as current, confirm the
   daemon's update supervisor probes without error and without GitHub API quota use.
   Then, when the NEXT release (v0.3.x/v0.4.0) publishes: prove the daemon downloads,
   verifies against SHA256SUMS, atomically swaps the binary, and every CLI command's
   envelope carries the restart-daemon message from the user messages queue; restart the
   daemon and confirm the new version runs. Also prove the polite failures: v0.2.0's
   missing SHA256SUMS = clean Blocked (no retry loop), unwritable install dir =
   notify-only with doctor-upgrade + reinstall guidance.
3. **Regression smokes on request**: o-prime may ping you with a merged PR to prove on
   Linux before a release is cut.
4. **Dogfood + observe**: per AGENTS.md — use flowspace3 itself where it fits, and
   `harness observe` every friction; report frictions to o-prime while in context. You
   are permanently the closest thing we have to a real user — your confusion is data.

## Report shape

PASS/FAIL per duty item, exact commands + envelopes for failures, wall-clock for the
full walkthrough (install-to-first-search time is a product metric), and anything a
first-time user would trip on. Report to pij-instant-lynx at work edges.

## Bounds

- You do not edit product code. Findings become reports to o-prime (who routes fixes) —
  or GitHub field-report issues when o-prime says so.
- Never point a test daemon at the host's database or config. If you find yourself
  typing the host's 5433, stop.
- Clean up your containers/volumes at the end of each verification run.
