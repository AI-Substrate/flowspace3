//! Embed a real file with the local ONNX adapter, and time every phase.
//!
//! A smoke test you can point at anything, and the only honest way to answer
//! "what does this actually cost": model load dominates until the corpus is
//! large, and the answer changes completely between a cold and a warm cache.
//!
//! ```bash
//! cargo run --release -p fs3-providers --example embed_file -- README.md
//!
//! # cold-cache numbers (deletes the model first — it will re-download ~129 MB)
//! rm -rf ~/.cache/flowspace3/models
//! cargo run --release -p fs3-providers --example embed_file -- README.md
//! ```

use std::time::Instant;

use fs3_core::Embedder;
use fs3_providers::{DEFAULT_LOCAL_MODEL, LocalEmbedder, LocalEmbedderConfig};

/// Split markdown into chunks that fit the model's 512-token window.
///
/// Paragraph boundaries, packed up to a word budget — deliberately crude.
/// Real chunking is `fs3-parsers`' job; this only has to be representative
/// enough that the timings mean something.
fn chunk(text: &str, words_per_chunk: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut words = 0;

    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        let count = paragraph.split_whitespace().count();
        if words + count > words_per_chunk && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            words = 0;
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
        words += count;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    // Both vectors are already L2-normalised by the adapter, so the dot
    // product IS the cosine — no division needed, and dividing again would
    // just be arithmetic that cancels.
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();

    let path = std::env::args()
        .nth(1)
        .ok_or("usage: embed_file <path-to-file> [query]")?;
    let query = std::env::args().nth(2);

    let source = std::fs::read_to_string(&path)?;
    let read_at = started.elapsed();

    let chunks = chunk(&source, 300);
    let chunked_at = started.elapsed();

    // Loading downloads on a cold cache and builds an ONNX session; it blocks,
    // so it belongs on a blocking thread even in a one-shot binary.
    let load_start = Instant::now();
    let embedder = tokio::task::spawn_blocking(|| {
        LocalEmbedder::load(LocalEmbedderConfig::new(DEFAULT_LOCAL_MODEL)?)
    })
    .await??;
    let load = load_start.elapsed();

    let embed_start = Instant::now();
    let vectors = embedder.embed(&chunks).await?;
    let embed = embed_start.elapsed();

    let total = started.elapsed();
    let bytes = source.len();
    let words: usize = source.split_whitespace().count();

    println!("file          : {path}");
    println!("size          : {bytes} bytes, {words} words");
    println!("chunks        : {} (<=300 words each)", chunks.len());
    println!(
        "model         : {} @ {} dims",
        embedder.model(),
        embedder.dimensions()
    );
    println!("key           : {}", embedder.key());
    println!();
    println!("read file     : {:>8.1} ms", read_at.as_secs_f64() * 1000.0);
    println!(
        "chunk         : {:>8.1} ms",
        (chunked_at - read_at).as_secs_f64() * 1000.0
    );
    println!("LOAD MODEL    : {:>8.1} ms", load.as_secs_f64() * 1000.0);
    println!(
        "embed         : {:>8.1} ms  ({:.1} ms/chunk, {:.0} words/sec)",
        embed.as_secs_f64() * 1000.0,
        embed.as_secs_f64() * 1000.0 / chunks.len() as f64,
        words as f64 / embed.as_secs_f64()
    );
    println!("TOTAL         : {:>8.1} ms", total.as_secs_f64() * 1000.0);
    println!(
        "load share    : {:>8.1} % of total",
        load.as_secs_f64() / total.as_secs_f64() * 100.0
    );

    // Prove the vectors mean something rather than merely existing.
    if let Some(query) = query {
        let query_vector = embedder.embed(std::slice::from_ref(&query)).await?;
        let mut ranked: Vec<(f32, usize)> = vectors
            .iter()
            .enumerate()
            .map(|(index, vector)| (cosine(&query_vector[0], vector), index))
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));

        println!("\nquery         : {query:?}");
        for (score, index) in ranked.iter().take(3) {
            let preview: String = chunks[*index].chars().take(90).collect();
            println!("  {score:.4}  #{index}  {}", preview.replace('\n', " "));
        }
    }

    Ok(())
}
