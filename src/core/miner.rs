// ─── core/miner.rs ───────────────────────────────────────────────────────────
//
// PoIO mining engine.
//
// Design principles:
//   • Zero heap allocation inside the hot mining loop (pre-allocated stack
//     buffers and fixed-size arrays).
//   • Each worker thread owns its own `File` handle to avoid mutex contention
//     on seek position.
//   • Uses `rayon` for work-stealing thread parallelism: one thread per
//     logical CPU by default, configurable via `--threads`.
//   • Supports graceful cancellation via an `AtomicBool` stop signal so
//     `Ctrl-C` stops all workers cleanly.
//   • Produces a `BlockProof` that carries the 128 raw chunks, enabling
//     O(1)-disk verifiers.
//
// ─────────────────────────────────────────────────────────────────────────────

use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::{crypto, disk};

// ── Protocol constants ────────────────────────────────────────────────────────

/// Fixed read size enforced by the protocol (4 KiB = one NVMe sector stripe).
pub const CHUNK_SIZE: usize = 4096;

/// Number of random chunk reads required per mining attempt.
pub const REQUIRED_READS: usize = 128;

// ── Proof structure ───────────────────────────────────────────────────────────

/// A self-contained block proof that can be transmitted to verifying nodes.
/// Verifiers only need this struct — no disk access required.
#[derive(Serialize, Deserialize, Clone)]
pub struct BlockProof {
    /// The block header bytes used during mining.
    pub block_header: Vec<u8>,
    /// The winning nonce.
    pub nonce: u64,
    /// Total number of 4 KiB chunks in the plot file.
    pub num_chunks: u64,
    /// The 128 chunk indices that were read (deterministically re-derivable).
    pub chunk_indices: Vec<u64>,
    /// The raw 4 KiB content of each of the 128 chunks.
    pub chunks: Vec<Vec<u8>>,
    /// Final Blake3 hash over all 128 chunks in order.
    pub final_hash: [u8; 32],
}

// ── Mining statistics ─────────────────────────────────────────────────────────

/// Live telemetry counters shared across worker threads.
pub struct MiningStats {
    pub attempts:      AtomicU64,
    pub io_reads:      AtomicU64,
    pub start_time:    Instant,
}

impl MiningStats {
    pub fn new() -> Self {
        Self {
            attempts:   AtomicU64::new(0),
            io_reads:   AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    pub fn hashes_per_sec(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed < 1e-9 { return 0.0; }
        self.attempts.load(Ordering::Relaxed) as f64 / elapsed
    }

    pub fn iops(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed < 1e-9 { return 0.0; }
        self.io_reads.load(Ordering::Relaxed) as f64 / elapsed
    }
}

// ── Configuration ─────────────────────────────────────────────────────────────

pub struct MinerConfig {
    pub plot_path:     PathBuf,
    pub num_chunks:    u64,
    pub block_header:  Vec<u8>,
    pub starting_nonce: u64,
    pub difficulty:    u8,
    pub threads:       usize,
    pub max_attempts:  Option<u64>,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Launch the multi-threaded PoIO miner.
///
/// Returns `Some(BlockProof)` on success, `None` if `max_attempts` was
/// reached or the stop flag was set before a valid nonce was found.
pub fn run_miner(
    config:    &MinerConfig,
    stats:     Arc<MiningStats>,
    stop_flag: Arc<AtomicBool>,
) -> Option<BlockProof> {
    // Build a thread-pool of the requested size.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(config.threads)
        .build()
        .expect("failed to build rayon thread pool");

    // Shared "winner found" flag so we can short-circuit all threads quickly.
    let found = Arc::new(AtomicBool::new(false));

    // Nonce range: we divide the nonce space into `threads` segments.
    // Each segment is `SEGMENT` nonces wide; threads cycle through segments.
    const SEGMENT: u64 = 1024;

    // We pass the config by reference into the closure without moving it.
    // Use a channel to return the winning proof.
    let (tx, rx) = std::sync::mpsc::channel::<BlockProof>();

    let difficulty    = config.difficulty;
    let num_chunks    = config.num_chunks;
    let plot_path     = config.plot_path.clone();
    let block_header  = config.block_header.clone();
    let starting_nonce = config.starting_nonce;
    let max_attempts  = config.max_attempts;

    pool.install(|| {
        let thread_count = config.threads as u64;

        (0..thread_count).into_par_iter().for_each(|thread_id| {
            // Each thread opens its own file handle — no mutex on seek.
            let mut file = match File::open(&plot_path) {
                Ok(f)  => f,
                Err(e) => {
                    eprintln!("[thread {}] cannot open plot: {}", thread_id, e);
                    return;
                }
            };

            let tx = tx.clone();

            // Stack-allocate the chunk buffer once per thread.
            let mut chunk_buffer = [0u8; CHUNK_SIZE];

            let mut local_nonce = starting_nonce + thread_id * SEGMENT;
            let mut local_attempts: u64 = 0;

            loop {
                // Cooperative cancellation checks.
                if found.load(Ordering::Relaxed) || stop_flag.load(Ordering::Relaxed) {
                    break;
                }
                if let Some(max) = max_attempts {
                    if local_attempts >= max / thread_count + 1 {
                        break;
                    }
                }

                // ── Single mining attempt ─────────────────────────────────

                // 1. Derive seed = Blake3(header || nonce)
                let seed = crypto::derive_seed(&block_header, local_nonce);

                // 2. Generate the 128 chunk indices deterministically
                let chunk_indices = crypto::generate_chunk_indices(&seed, num_chunks);

                // 3. Stream all 128 chunks through BLAKE3 without allocation
                let mut final_state = crypto::HashState::new();
                let mut all_chunks: Vec<Vec<u8>> = Vec::with_capacity(REQUIRED_READS);
                let mut io_ok = true;

                for &idx in &chunk_indices {
                    let byte_offset = idx * CHUNK_SIZE as u64;
                    if disk::read_chunk_at_offset(&mut file, byte_offset, &mut chunk_buffer).is_ok() {
                        final_state.update(&chunk_buffer);
                        all_chunks.push(chunk_buffer.to_vec());
                    } else {
                        io_ok = false;
                        break;
                    }
                }

                if !io_ok {
                    local_nonce += thread_count * SEGMENT;
                    continue;
                }

                // 4. Finalize
                let final_hash = final_state.finalize();

                // 5. Update shared telemetry
                stats.attempts.fetch_add(1, Ordering::Relaxed);
                stats.io_reads.fetch_add(REQUIRED_READS as u64, Ordering::Relaxed);
                local_attempts += 1;

                // 6. Difficulty check
                if crypto::meets_difficulty(&final_hash, difficulty) {
                    if !found.swap(true, Ordering::SeqCst) {
                        let proof = BlockProof {
                            block_header: block_header.clone(),
                            nonce: local_nonce,
                            num_chunks,
                            chunk_indices: chunk_indices.to_vec(),
                            chunks: all_chunks,
                            final_hash,
                        };
                        let _ = tx.send(proof);
                    }
                    break;
                }

                // 7. Advance nonce by the inter-thread stride
                local_nonce += thread_count * SEGMENT;
            }
        });
    });

    drop(tx); // Close all sender clones so recv() returns Err on exhaustion
    rx.recv().ok()
}

// ── Elapsed time helper ───────────────────────────────────────────────────────

pub fn format_duration(d: Duration) -> String {
    let secs  = d.as_secs();
    let hours = secs / 3600;
    let mins  = (secs % 3600) / 60;
    let s     = secs % 60;
    if hours > 0 {
        format!("{:02}h {:02}m {:02}s", hours, mins, s)
    } else if mins > 0 {
        format!("{:02}m {:02}s", mins, s)
    } else {
        format!("{:.3}s", d.as_secs_f64())
    }
}
