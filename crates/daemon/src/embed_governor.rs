//! Searches never wait behind background embedding.
//!
//! Measured 2026-10-09: a search's query embed took 20-29 s, and the CLI gave
//! up near 40 s. The cause was not a queue inside the daemon. The embed lane
//! sent ten ~200k-token batches at once, each retried three times, against an
//! Azure deployment that allows 6.85M tokens per minute. The deployment
//! answered 429 with `Retry-After` (1,884 times that day), and the query embed
//! — sharing the deployment — inherited the same wait.
//!
//! Two pieces fix it, per embedder INSTANCE (the same budget boundary the embed
//! lane uses — `wiring` builds one `Arc` per configured instance, so pointer
//! identity is instance identity):
//!
//! * [`EmbedGovernor`] paces background requests by TOKENS per minute, the
//!   unit providers actually limit, halves that pace on a rate-limit response,
//!   recovers it on success, and holds every new background request while a
//!   query embed is in flight. Queries never touch the budget.
//! * [`QueryVectorCache`] answers a repeated query (paging, the prompt hook,
//!   an agent re-asking) without calling the provider at all.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

/// How long background pauses after a rate-limit response that gave no
/// `Retry-After`.
const DEFAULT_PAUSE: Duration = Duration::from_secs(10);

/// The bucket holds at most this many seconds of budget, so an idle minute
/// does not license a burst that trips the provider anyway.
const BURST_SECONDS: f64 = 10.0;

/// The pace never falls below this fraction of the configured budget: halving
/// forever would turn a bad minute into a stalled backlog.
const FLOOR_FRACTION: f64 = 1.0 / 16.0;

/// Each success wins back this fraction of the configured budget.
const RECOVERY_FRACTION: f64 = 1.0 / 32.0;

/// While a query is in flight, a waiting background request re-checks at
/// least this often, so a missed wake-up costs a moment, never a stall.
const INTERACTIVE_RECHECK: Duration = Duration::from_millis(250);

/// Paces one embedder instance's background requests and gives queries the
/// right of way.
#[derive(Debug)]
pub struct EmbedGovernor {
    /// The configured ceiling, tokens per minute. `0` disables pacing.
    budget: f64,
    bucket: Mutex<Bucket>,
    /// Query embeds currently in flight against this instance.
    interactive: AtomicUsize,
    /// Signalled when the last in-flight query finishes.
    clear: Notify,
}

#[derive(Debug)]
struct Bucket {
    /// The current pace, tokens per minute: the budget, or less after a 429.
    pace: f64,
    /// Tokens that may be sent now. May go negative: one request larger than
    /// the burst is admitted and paid back over the following seconds, rather
    /// than waiting forever for room it can never have.
    available: f64,
    refilled_at: Instant,
    /// Background sends nothing until then, after a rate-limit response.
    paused_until: Option<Instant>,
}

impl EmbedGovernor {
    /// A governor allowing `tokens_per_minute` of background embedding.
    #[must_use]
    pub fn new(tokens_per_minute: u64) -> Self {
        // Precision loss is irrelevant at token-per-minute magnitudes.
        #[allow(clippy::cast_precision_loss)]
        let budget = tokens_per_minute as f64;
        Self {
            budget,
            bucket: Mutex::new(Bucket {
                pace: budget,
                available: budget / 60.0 * BURST_SECONDS,
                refilled_at: Instant::now(),
                paused_until: None,
            }),
            interactive: AtomicUsize::new(0),
            clear: Notify::new(),
        }
    }

    /// Wait until a background request of `tokens` may be sent.
    ///
    /// Waits, in order, for: every in-flight query to finish, any rate-limit
    /// pause to end, and the token budget to have room.
    pub async fn admit_background(&self, tokens: usize) {
        loop {
            if self.interactive.load(Ordering::Acquire) > 0 {
                let cleared = self.clear.notified();
                if self.interactive.load(Ordering::Acquire) > 0 {
                    let _ = tokio::time::timeout(INTERACTIVE_RECHECK, cleared).await;
                }
                continue;
            }
            if self.budget <= 0.0 {
                return;
            }
            let wait = {
                let mut bucket = self.bucket.lock().expect("governor lock poisoned");
                let now = Instant::now();
                bucket.refill(now, self.budget);
                match bucket.paused_until {
                    Some(until) if until > now => until - now,
                    _ => {
                        bucket.paused_until = None;
                        if bucket.available > 0.0 {
                            #[allow(clippy::cast_precision_loss)]
                            let cost = tokens as f64;
                            bucket.available -= cost;
                            return;
                        }
                        let per_second = bucket.pace / 60.0;
                        Duration::from_secs_f64((-bucket.available + 1.0) / per_second)
                    }
                }
            };
            tokio::time::sleep(wait).await;
        }
    }

    /// A background request was rate-limited: halve the pace and pause.
    pub fn rate_limited(&self, retry_after: Option<Duration>) {
        if self.budget <= 0.0 {
            return;
        }
        let mut bucket = self.bucket.lock().expect("governor lock poisoned");
        let now = Instant::now();
        bucket.refill(now, self.budget);
        bucket.pace = (bucket.pace / 2.0).max(self.budget * FLOOR_FRACTION);
        bucket.available = bucket.available.min(0.0);
        let until = now + retry_after.unwrap_or(DEFAULT_PAUSE);
        bucket.paused_until = Some(
            bucket
                .paused_until
                .map_or(until, |current| current.max(until)),
        );
    }

    /// A background request succeeded: win back some of the pace.
    pub fn succeeded(&self) {
        if self.budget <= 0.0 {
            return;
        }
        let mut bucket = self.bucket.lock().expect("governor lock poisoned");
        bucket.pace = (bucket.pace + self.budget * RECOVERY_FRACTION).min(self.budget);
    }

    /// Mark a query embed in flight until the guard drops.
    #[must_use]
    pub fn interactive(&self) -> InteractiveGuard<'_> {
        self.interactive.fetch_add(1, Ordering::AcqRel);
        InteractiveGuard { governor: self }
    }

    /// The current pace, tokens per minute. For tests and diagnostics.
    #[must_use]
    pub fn pace(&self) -> f64 {
        self.bucket.lock().expect("governor lock poisoned").pace
    }
}

impl Bucket {
    fn refill(&mut self, now: Instant, budget: f64) {
        let elapsed = now
            .saturating_duration_since(self.refilled_at)
            .as_secs_f64();
        self.refilled_at = now;
        let ceiling = budget / 60.0 * BURST_SECONDS;
        self.available = (self.available + elapsed * self.pace / 60.0).min(ceiling);
    }
}

/// Holds background requests back while a query embed is in flight.
#[derive(Debug)]
pub struct InteractiveGuard<'a> {
    governor: &'a EmbedGovernor,
}

impl Drop for InteractiveGuard<'_> {
    fn drop(&mut self) {
        if self.governor.interactive.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.governor.clear.notify_waiters();
        }
    }
}

/// How many query vectors are kept.
pub const QUERY_CACHE_ENTRIES: usize = 512;

/// Recently embedded query vectors, by (embedder key, exact query text).
///
/// Keyed by the embedder KEY, not the repo: two repos on one model share a
/// vector space, and a vector from another model's space must never answer.
#[derive(Debug)]
pub struct QueryVectorCache {
    capacity: usize,
    tick: u64,
    entries: HashMap<(String, String), (Vec<f32>, u64)>,
}

impl QueryVectorCache {
    /// An empty cache holding at most `capacity` vectors.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            tick: 0,
            entries: HashMap::new(),
        }
    }

    /// The cached vector for this query, marking it recently used.
    pub fn get(&mut self, model_key: &str, query: &str) -> Option<Vec<f32>> {
        self.tick += 1;
        let tick = self.tick;
        self.entries
            .get_mut(&(model_key.to_string(), query.to_string()))
            .map(|(vector, used)| {
                *used = tick;
                vector.clone()
            })
    }

    /// Remember a query's vector, evicting the least recently used if full.
    pub fn put(&mut self, model_key: &str, query: &str, vector: Vec<f32>) {
        if self.capacity == 0 {
            return;
        }
        self.tick += 1;
        let key = (model_key.to_string(), query.to_string());
        if !self.entries.contains_key(&key) && self.entries.len() >= self.capacity {
            // A linear scan over a few hundred entries, once per new query, is
            // cheaper than the bookkeeping an ordered structure would cost.
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| key.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(key, (vector, self.tick));
    }

    /// How many vectors are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: u64 = 60_000;

    #[tokio::test(start_paused = true)]
    async fn background_is_paced_by_tokens_per_minute() {
        // 60k tokens/min = 1k/s; the bucket starts with 10 s (10k) of burst.
        let governor = EmbedGovernor::new(60_000);
        let started = Instant::now();
        for _ in 0..4 {
            governor.admit_background(5_000).await; // 20k total
        }
        // The first two ride the 10k burst. The third finds the bucket empty
        // and goes after a moment's refill, taking it 5k into debt; the
        // fourth waits for that debt (5 s at 1k/s). Each request pays for
        // itself AFTER it is sent, so 20k costs ~5 s, not 10.
        let waited = started.elapsed();
        assert!(waited >= Duration::from_secs(4), "{waited:?}");
        assert!(waited <= Duration::from_secs(6), "{waited:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_larger_than_the_burst_is_admitted_not_stranded() {
        let governor = EmbedGovernor::new(60_000);
        let started = Instant::now();
        governor.admit_background(200_000).await;
        assert!(started.elapsed() < Duration::from_millis(1));
        // …and the debt is paid before the next one goes.
        governor.admit_background(1).await;
        assert!(started.elapsed() >= Duration::from_secs(190));
    }

    #[tokio::test(start_paused = true)]
    async fn background_waits_while_a_query_is_in_flight() {
        let governor = std::sync::Arc::new(EmbedGovernor::new(0));
        let guard = governor.interactive();
        let waiting = {
            let governor = governor.clone();
            tokio::spawn(async move { governor.admit_background(10).await })
        };
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(
            !waiting.is_finished(),
            "background must hold while a query runs"
        );
        drop(guard);
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .expect("background resumes once the query finishes")
            .expect("task");
    }

    #[tokio::test(start_paused = true)]
    async fn a_rate_limit_halves_the_pace_pauses_background_and_success_recovers() {
        let governor = EmbedGovernor::new(MINUTE * 100);
        governor.rate_limited(Some(Duration::from_secs(50)));
        assert!((governor.pace() - (MINUTE * 50) as f64).abs() < 1.0);

        let started = Instant::now();
        governor.admit_background(1).await;
        assert!(
            started.elapsed() >= Duration::from_secs(50),
            "honours Retry-After"
        );

        for _ in 0..100 {
            governor.succeeded();
        }
        assert!(
            (governor.pace() - (MINUTE * 100) as f64).abs() < 1.0,
            "recovers to the budget"
        );
        for _ in 0..20 {
            governor.rate_limited(None);
        }
        assert!(
            governor.pace() >= (MINUTE * 100) as f64 * FLOOR_FRACTION - 1.0,
            "never below the floor"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn zero_disables_pacing() {
        let governor = EmbedGovernor::new(0);
        let started = Instant::now();
        for _ in 0..100 {
            governor.admit_background(1_000_000).await;
        }
        governor.rate_limited(Some(Duration::from_secs(60)));
        governor.admit_background(1).await;
        assert!(started.elapsed() < Duration::from_millis(1));
    }

    #[test]
    fn the_cache_keys_by_embedder_and_text_and_evicts_the_least_recent() {
        let mut cache = QueryVectorCache::new(2);
        cache.put("m@1024", "retry", vec![1.0]);
        cache.put("other@1024", "retry", vec![2.0]);
        assert_eq!(cache.get("m@1024", "retry"), Some(vec![1.0]));
        assert_eq!(cache.get("other@1024", "retry"), Some(vec![2.0]));
        assert_eq!(cache.get("m@1024", "Retry"), None, "exact text only");
        // "m" was used before "other", so "m" is the least recent… touch it.
        assert!(cache.get("m@1024", "retry").is_some());
        cache.put("m@1024", "backoff", vec![3.0]);
        assert_eq!(cache.len(), 2);
        assert_eq!(
            cache.get("other@1024", "retry"),
            None,
            "least recent evicted"
        );
        assert_eq!(cache.get("m@1024", "retry"), Some(vec![1.0]));
    }
}
