//! Embed requests stay under the provider's per-request cap, whatever the text.
//!
//! On 2026-10-12 Azure refused embed requests we had estimated at 192k–200k
//! tokens as over its 300k-token request cap: raw conversation text, dense
//! enough to run under two bytes a token where the estimate assumes three. The
//! refusal was untyped, so each one failed every job in the batch.
//!
//! The fake here counts tokens its own way, denser than our estimate, and
//! enforces the cap by that count, the way the real provider does.

mod support;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use fs3_core::ports::Embedder;
use fs3_core::{Config, DatabaseConfig, Result};
use fs3_daemon::batch::REQUEST_BYTES;
use fs3_daemon::enrich::embed_items;
use fs3_daemon::wiring::AppState;
use fs3_store::SourceKind;

/// An embedder with a per-request token cap, counting 1.9 bytes a token: the
/// density prod's refused requests implied.
#[derive(Debug)]
struct RequestCapped {
    cap_tokens: usize,
    /// Bytes of each call it accepted.
    accepted: Mutex<Vec<usize>>,
    /// Texts in each call it accepted.
    accepted_texts: AtomicUsize,
    refused: AtomicUsize,
}

impl RequestCapped {
    fn new(cap_tokens: usize) -> Self {
        Self {
            cap_tokens,
            accepted: Mutex::new(Vec::new()),
            accepted_texts: AtomicUsize::new(0),
            refused: AtomicUsize::new(0),
        }
    }

    fn tokens(texts: &[String]) -> usize {
        texts.iter().map(String::len).sum::<usize>() * 10 / 19
    }
}

#[async_trait]
impl Embedder for RequestCapped {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let tokens = Self::tokens(texts);
        if tokens > self.cap_tokens {
            self.refused.fetch_add(1, Ordering::SeqCst);
            return Err(fs3_core::Error::RequestTooLarge {
                max_tokens: self.cap_tokens,
                detail: format!(
                    "Invalid 'input': maximum request size is {} tokens per request.",
                    self.cap_tokens
                ),
            });
        }
        self.accepted
            .lock()
            .expect("accepted lock")
            .push(texts.iter().map(String::len).sum());
        self.accepted_texts.fetch_add(texts.len(), Ordering::SeqCst);
        Ok(texts
            .iter()
            .map(|_| vec![0.05f32; fs3_store::EMBEDDING_DIMENSIONS])
            .collect())
    }

    fn key(&self) -> String {
        format!("capped@{}", fs3_store::EMBEDDING_DIMENSIONS)
    }

    fn concurrency_ceiling(&self) -> usize {
        1
    }

    fn max_input_tokens(&self) -> usize {
        8192
    }
}

/// Sixty 14 KB texts of hex noise: dense, and each one a single provider input.
fn dense_items() -> Vec<(String, String)> {
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    (0..60)
        .map(|_| {
            let text: String = (0..14_000)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    char::from_digit((seed % 16) as u32, 16).expect("a hex digit")
                })
                .collect();
            (fs3_core::content_hash(text.as_bytes()), text)
        })
        .collect()
}

async fn stack(
    label: &str,
    embedder: Arc<RequestCapped>,
) -> (support::FreshDatabase, AppState, Vec<(String, String)>) {
    let database = support::FreshDatabase::create(label).await;
    let config = Config {
        database: DatabaseConfig {
            url: database.url(),
        },
        ..Config::default()
    };
    let mut state = AppState::from_config(config).expect("wires");
    fs3_store::migrate(&state.db).await.expect("migrates");
    state.embedder = embedder;
    let items = dense_items();
    support::hold(&state, label, &items).await;
    (database, state, items)
}

#[tokio::test]
async fn dense_text_goes_out_in_requests_the_provider_accepts() {
    let provider = Arc::new(RequestCapped::new(300_000));
    let (database, state, items) = stack("request-cap-dense", provider.clone()).await;

    embed_items(&state, "git:dense", SourceKind::Raw, &items)
        .await
        .expect("every item is embedded");

    assert_eq!(
        provider.refused.load(Ordering::SeqCst),
        0,
        "no request is over the provider's cap by its own count"
    );
    let accepted = provider.accepted.lock().expect("accepted lock").clone();
    assert!(
        accepted.iter().all(|bytes| *bytes <= REQUEST_BYTES),
        "every request is within {REQUEST_BYTES} bytes: {accepted:?}"
    );
    assert_eq!(provider.accepted_texts.load(Ordering::SeqCst), items.len());
    database.destroy(state.db.clone()).await;
}

/// A provider whose cap is tighter than any bound we can know: the refused
/// request splits, down to single inputs, rather than failing the batch.
#[tokio::test]
async fn a_request_refused_as_too_large_splits_down_to_single_inputs() {
    // One 14 KB text is about 7.4k tokens by this count; two are over.
    let provider = Arc::new(RequestCapped::new(8_000));
    let (database, state, items) = stack("request-cap-split", provider.clone()).await;

    embed_items(&state, "git:dense", SourceKind::Raw, &items)
        .await
        .expect("every item is embedded once the requests are small enough");

    assert!(
        provider.refused.load(Ordering::SeqCst) > 0,
        "the cap was hit"
    );
    assert_eq!(
        provider.accepted_texts.load(Ordering::SeqCst),
        items.len(),
        "each item is embedded exactly once"
    );
    database.destroy(state.db.clone()).await;
}
