//! Embed several files four ways, and time each — what a worker pipeline
//! actually costs, and which shape of parallelism is worth having.
//!
//! The interesting result is that the obvious parallelism does nothing. One
//! loaded model is one ONNX session behind one mutex, and that session already
//! saturates every core on a single `embed` call, so firing N concurrent
//! `embed`s at it just makes them queue. What *does* help is batching, and —
//! if you are prepared to pay for it in memory — a pool of sessions each
//! given a slice of the cores.
//!
//! ```bash
//! cargo run --release -p fs3-providers --example embed_files -- FILE...
//! ```

use std::sync::Arc;
use std::time::Instant;

use fs3_core::Embedder;
use fs3_providers::{DEFAULT_LOCAL_MODEL, LocalEmbedder, LocalEmbedderConfig};

/// Chunk on paragraph boundaries up to a word budget. Crude on purpose: real
/// chunking is `fs3-parsers`' job, this only has to be representative.
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

/// `fs3_core::Result`, not `Box<dyn Error>`: this runs inside
/// `spawn_blocking`, whose return type must be `Send`, and a boxed trait
/// object is not.
fn load(intra_threads: Option<usize>) -> fs3_core::Result<LocalEmbedder> {
    let mut config = LocalEmbedderConfig::new(DEFAULT_LOCAL_MODEL)?;
    if let Some(threads) = intra_threads {
        config = config.with_intra_threads(threads);
    }
    LocalEmbedder::load(config)
}

fn rate(chunks: usize, seconds: f64) -> String {
    format!("{:.0} chunks/sec", chunks as f64 / seconds)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        return Err("usage: embed_files <file>...".into());
    }

    // Read and chunk everything up front so the timings below measure
    // embedding, not IO.
    let mut files: Vec<(String, Vec<String>)> = Vec::new();
    for path in &paths {
        let source = std::fs::read_to_string(path)?;
        files.push((path.clone(), chunk(&source, 300)));
    }
    let total_chunks: usize = files.iter().map(|(_, c)| c.len()).sum();
    let cores = std::thread::available_parallelism()?.get();

    let started = Instant::now();
    let embedder = Arc::new(tokio::task::spawn_blocking(|| load(None)).await??);
    println!(
        "load (warm)   : {:.0} ms   model {} @ {} dims   {cores} cores available",
        started.elapsed().as_secs_f64() * 1000.0,
        embedder.model(),
        embedder.dimensions()
    );
    println!(
        "corpus        : {} files, {total_chunks} chunks\n",
        files.len()
    );

    // --- 1. sequential, one file at a time -------------------------------
    println!("1. SEQUENTIAL — one embed() call per file, in order");
    let sequential_start = Instant::now();
    for (path, chunks) in &files {
        let started = Instant::now();
        let vectors = embedder.embed(chunks).await?;
        let elapsed = started.elapsed().as_secs_f64();
        println!(
            "   {:>7.1} ms  {:>4} chunks  {:>16}  {}",
            elapsed * 1000.0,
            vectors.len(),
            rate(vectors.len(), elapsed),
            path
        );
    }
    let sequential = sequential_start.elapsed().as_secs_f64();
    println!(
        "   TOTAL {:.1} ms  ({})\n",
        sequential * 1000.0,
        rate(total_chunks, sequential)
    );

    // --- 2. concurrent over ONE embedder ---------------------------------
    // The shape a worker reaches for first. It cannot win: `embed` locks the
    // single session, so these serialise — and the lock traffic is pure loss.
    println!("2. CONCURRENT — {} tasks, ONE shared embedder", files.len());
    let concurrent_start = Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    for (path, chunks) in files.clone() {
        let embedder = Arc::clone(&embedder);
        tasks.spawn(async move {
            let started = Instant::now();
            let vectors = embedder.embed(&chunks).await.expect("embed");
            (path, vectors.len(), started.elapsed().as_secs_f64())
        });
    }
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        results.push(result?);
    }
    let concurrent = concurrent_start.elapsed().as_secs_f64();
    let waited: f64 = results.iter().map(|(_, _, seconds)| seconds).sum();
    println!(
        "   TOTAL {:.1} ms  ({})  — wall-clock sum of task times {:.1} ms, i.e. queueing",
        concurrent * 1000.0,
        rate(total_chunks, concurrent),
        waited * 1000.0
    );
    println!(
        "   vs sequential: {:+.1} %\n",
        (concurrent / sequential - 1.0) * 100.0
    );

    // --- 3. one batch ----------------------------------------------------
    // Every chunk from every file in a single call: the session batches its
    // kernels once instead of once per file, and padding is amortised over a
    // bigger matrix.
    println!("3. ONE BATCH — every chunk from every file in a single embed()");
    let flat: Vec<String> = files
        .iter()
        .flat_map(|(_, chunks)| chunks.iter().cloned())
        .collect();
    let batch_start = Instant::now();
    let vectors = embedder.embed(&flat).await?;
    let batch = batch_start.elapsed().as_secs_f64();
    assert_eq!(vectors.len(), total_chunks);
    println!(
        "   TOTAL {:.1} ms  ({})  vs sequential: {:+.1} %\n",
        batch * 1000.0,
        rate(total_chunks, batch),
        (batch / sequential - 1.0) * 100.0
    );

    // --- 4. a pool of sessions -------------------------------------------
    // Real parallelism costs memory: each session is its own copy of the
    // model. Each gets a slice of the cores so the pool does not oversubscribe
    // the machine it is already saturating.
    //
    // Shards by CHUNK, not by file, on purpose. Per-file work here spans 1 to
    // 23 chunks — sharding round-robin by file hands one session half the
    // corpus and measures the imbalance instead of the parallelism.
    let pool_size = std::env::var("FS3_POOL")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4)
        .clamp(1, total_chunks.max(1));
    let per_session = (cores / pool_size).max(1);
    println!(
        "4. POOL — {pool_size} sessions, {per_session} intra-op threads each, chunks sharded evenly"
    );
    let pool_load = Instant::now();
    let mut pool = Vec::new();
    for _ in 0..pool_size {
        pool.push(Arc::new(
            tokio::task::spawn_blocking(move || load(Some(per_session))).await??,
        ));
    }
    println!(
        "   load {pool_size} sessions: {:.0} ms (memory is the price — one model copy each)",
        pool_load.elapsed().as_secs_f64() * 1000.0
    );

    let shard = total_chunks.div_ceil(pool_size);
    let pool_start = Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    for (index, slice) in flat.chunks(shard).enumerate() {
        let embedder = Arc::clone(&pool[index]);
        let slice = slice.to_vec();
        tasks.spawn(async move { embedder.embed(&slice).await.expect("embed").len() });
    }
    let mut embedded = 0;
    while let Some(result) = tasks.join_next().await {
        embedded += result?;
    }
    assert_eq!(embedded, total_chunks);
    let pooled = pool_start.elapsed().as_secs_f64();
    println!(
        "   TOTAL {:.1} ms  ({})  vs sequential: {:+.1} %  vs one batch: {:+.1} %",
        pooled * 1000.0,
        rate(total_chunks, pooled),
        (pooled / sequential - 1.0) * 100.0,
        (pooled / batch - 1.0) * 100.0
    );

    Ok(())
}
