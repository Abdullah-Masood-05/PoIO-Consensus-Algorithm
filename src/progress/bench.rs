// ─── core/bench.rs ────────────────────────────────────────────────────────────
//
// I/O benchmark subsystem — measures real NVMe random-read throughput under
// the exact access pattern used by the PoIO mining loop.
//
// Outputs:
//   • Median + mean 4 KiB random read latency (µs)
//   • Measured IOPS (I/O operations per second)
//   • Estimated hashes/sec at REQUIRED_READS reads per hash
//   • Comparison table vs. protocol targets
//
// ─────────────────────────────────────────────────────────────────────────────

use std::fs::File;
use std::path::Path;
use std::time::Instant;

use colored::Colorize;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

use crate::verification::disk;
use crate::progress::miner::{CHUNK_SIZE, REQUIRED_READS};

/// Number of individual 4 KiB reads to perform during the benchmark.
const BENCH_READS: usize = 1024;

pub struct BenchResult {
    pub total_reads:   usize,
    pub elapsed_secs:  f64,
    pub iops:          f64,
    pub mean_latency_us: f64,
    pub median_latency_us: f64,
    pub hashes_per_sec: f64,
}

/// Run the I/O benchmark against the plot file at `path`.
pub fn run_benchmark(path: &Path, num_chunks: u64) -> std::io::Result<BenchResult> {
    let mut file = match disk::open_direct(path) {
        Ok(f)  => f,
        Err(_) => File::open(path)?, // fallback to buffered if direct fails
    };

    let mut rng = ChaCha8Rng::from_seed([0xDE; 32]);
    let mut buf = [0u8; CHUNK_SIZE];
    let mut latencies_us: Vec<f64> = Vec::with_capacity(BENCH_READS);

    let total_start = Instant::now();

    for _ in 0..BENCH_READS {
        let idx    = rng.next_u64() % num_chunks;
        let offset = idx * CHUNK_SIZE as u64;

        let t0 = Instant::now();
        disk::read_chunk_at_offset(&mut file, offset, &mut buf)?;
        let elapsed = t0.elapsed();

        latencies_us.push(elapsed.as_secs_f64() * 1_000_000.0);
    }

    let total_elapsed = total_start.elapsed().as_secs_f64();

    latencies_us.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mean = latencies_us.iter().sum::<f64>() / latencies_us.len() as f64;
    let median = latencies_us[latencies_us.len() / 2];
    let iops   = BENCH_READS as f64 / total_elapsed;

    Ok(BenchResult {
        total_reads:       BENCH_READS,
        elapsed_secs:      total_elapsed,
        iops,
        mean_latency_us:   mean,
        median_latency_us: median,
        hashes_per_sec:    iops / REQUIRED_READS as f64,
    })
}

/// Print a rich benchmark report to stdout.
pub fn print_report(result: &BenchResult) {
    println!("\n{}", "━".repeat(60).bright_cyan());
    println!(" {}",  "PoIO I/O Benchmark Report".bold().bright_white());
    println!("{}", "━".repeat(60).bright_cyan());

    let row = |label: &str, value: String, note: &str| {
        println!("  {:<28} {:>14}  {}", label.bright_yellow(), value.bright_green(), note.dimmed());
    };

    row("Total Reads",        format!("{}", result.total_reads),              "4 KiB reads");
    row("Elapsed",            format!("{:.3} s", result.elapsed_secs),        "wall clock");
    row("IOPS",               format!("{:.0}", result.iops),                  "reads/sec");
    row("Mean Latency",       format!("{:.1} µs", result.mean_latency_us),    "per 4 KiB read");
    row("Median Latency",     format!("{:.1} µs", result.median_latency_us),  "per 4 KiB read");
    row("Est. Hashes/sec",    format!("{:.2}", result.hashes_per_sec),         "at 128 reads/hash");

    println!("{}", "━".repeat(60).bright_cyan());

    // Reference table
    println!("\n  {} (128 reads per hash attempt)", "Protocol Reference".bold());
    println!("  {:<28} {:>14}  {}", "Consumer NVMe target".dimmed(), "50–80 h/s".dimmed(), "(PCIe 3.0 x4)");
    println!("  {:<28} {:>14}  {}", "Enterprise NVMe target".dimmed(), "80–120 h/s".dimmed(), "(PCIe 4.0 x4)");

    let rating = if result.hashes_per_sec >= 80.0 {
        "EXCELLENT — enterprise-class throughput".bright_green()
    } else if result.hashes_per_sec >= 40.0 {
        "GOOD — consumer NVMe within protocol target".bright_yellow()
    } else {
        "LOW — may be using buffered I/O or slow media".bright_red()
    };
    println!("\n  Your device: {}", rating);
    println!("{}\n", "━".repeat(60).bright_cyan());
}
