# Provider throughput — retry, rate limits, and concurrency ceilings
**Built**: 2026-08-26 (worker pij-sure-kazimir, throughput packet provider half) · **Code**: `crates/providers/src/retry.rs`, `fs3_core::Error::RateLimited`, `Embedder/Summarizer::concurrency_ceiling`

The provider half of throughput. Three pieces, and the reason they belong on one page is that they only make sense against each other — and against the daemon half, which owns the queue.

## The division of labour
| Layer | Owns | Does |
|---|---|---|
| Adapter (`retry.rs`) | one call | absorbs a **blip**: retries transient HTTP, honours `Retry-After` |
| Adapter → scheduler | the handover | surfaces `Error::RateLimited` with the service's own wait |
| Scheduler (daemon) | the queue | owns the **semaphore**, parks rate-limited claims, retries jobs |

A provider handed one request cannot see how many others are in flight, so it must not own the semaphore. What it *does* know is its own shape, and that is what it declares.

## Key decisions
- **The retry is deliberately narrow.** The runner already retries any failed job three times without discriminating by error kind, so anything retried here is retried again around it and the two multiply. This loop retries exactly one thing — 429, 502, 503, 504 — and hands everything else up on the first failure. A wrong deployment name costs one request, not nine. A test pins that per status.
- **`Retry-After` beats our backoff**, in both forms the RFC allows (delta-seconds and HTTP-date). The service knows when its window reopens and we do not. An HTTP-date already in the past yields no wait rather than an error — a second of clock skew should not become a failure.
- **Full jitter**, not backoff-plus-noise: a uniform draw from `[0, backoff]`. The failure being defended against is every worker waking simultaneously, and only full jitter actually spreads them. No `rand` dependency — a crate whose only job is to desynchronise a sleep does not pay for itself, and the low bits of the wall clock are unpredictable enough for a sleep.
- **Single sleeps are capped at 20 s.** A job holds a concurrency permit while it sleeps; a permit held for minutes is indistinguishable from a stall. The cap also means one hostile `Retry-After: 3600` cannot pin a permit for an hour.
- **`Error::RateLimited` is a type, not a formatted string**, carrying `provider`, `retry_after: Option<Duration>` and `attempts`. A string could say the same words and no scheduler could act on them. `retry_after` is an `Option` because plenty of services rate-limit without saying for how long — the default park duration is the scheduler's number to choose, not one this layer invents.
- **Ceilings are required and undefaulted.** A default is a number nobody chose, and both ways of being wrong are silent: too high thrashes a small box, too low drives a cloud provider at a fraction of its capacity, and neither surfaces as an error — only as throughput nobody can explain.

## The declared ceilings
| Provider | Ceiling | Why |
|---|---|---|
| Azure OpenAI | 32 | sized by provisioned quota, not by connections; over-quota is a 429 with `Retry-After`, which the loop absorbs and the scheduler parks on |
| OpenAI | 16 | same shape: quota-priced, answers many at once |
| openai-compat | **1** | one model on one accelerator — a second request does not run in parallel, it queues *inside the server* while holding a connection |
| local (fastembed) | **1** | the session is behind a `Mutex`; extra permits queue on the lock while denying another provider the slot |
| fakes | 64 | no network, no lock, no cost |

Intended use: `permits = min(lane_width, provider.concurrency_ceiling())`.

## Gotchas learned
- **Two retry layers multiply, and neither knows about the other.** This is the whole reason the loop is narrow. Three provider attempts inside three runner attempts is nine requests at a service that is already asking us to slow down. If the runner's policy changes, this one has to be re-argued — it is not independently correct.
- **A rate limit must not consume a job attempt.** That is the daemon half's rule, and it matters: if a 429 burns one of a job's three attempts, a busy provider exhausts jobs that were never broken. `Error::RateLimited` exists so that decision is *possible*.
- **Read `Retry-After` before the body.** Consuming the body moves the response, and the advice is on the header. Easy to get backwards and silent when you do — the header just becomes `None` and the backoff quietly ignores the service.
- **The retry must not disturb the structured-output downgrade.** A schema rejection is a client error, so it is not transient, so the loop hands it back *unchanged* rather than flattening it to an `Error` — otherwise the downgrade would stop seeing it. A test asserts a transient retry re-sends the *same* request rather than silently downgrading.
- A `Retry-After` honoured on every attempt means a service advising 20 s twice costs 40 s inside one call. That is bounded by the cap and by the attempt count, and it is the deliberate trade: ignoring the advice gets us throttled harder.

## Verify
```bash
cargo test -p fs3-providers --test retry_behaviour   # 8 behaviour tests, ~2s
cargo test -p fs3-providers --lib retry              # 8 unit tests: statuses, parsing, backoff
```

The behaviour suite proves, against a stub that fails on purpose: a single 429 is absorbed and the caller sees a success; 502/503/504 likewise; `Retry-After: 1` actually delays the next attempt by a second (the default first backoff is 500 ms, so a slower run cannot be explained by the schedule); a sustained squeeze surfaces as `RateLimited` naming the deployment and carrying the service's wait; a rate limit without advice carries `None` rather than an invented number; and 400/401/404/500 each cost exactly one request.

Keyed re-run after the refactor: Azure contract 2/2, LAN compat contract 2/2.

## Code pointers
- `crates/providers/src/retry.rs` — `with_retry`, `RetryPolicy`, `parse_retry_after`, `is_transient`, and the unified `PostFailure`/`Rejection` the three HTTP adapters share.
- `crates/core/src/error.rs` — `Error::RateLimited`.
- `crates/core/src/ports.rs` — `concurrency_ceiling` on both ports, with the no-default rationale.
- `crates/providers/tests/retry_behaviour.rs` — the behaviour proof.
- `crates/providers/tests/common/mod.rs` — `StubServer::failing_then`, and `Reply` for sending real `Retry-After` headers.
