// ─── core/plot.rs ─────────────────────────────────────────────────────────────
//
// Plot file generation with:
//   • ChaCha8 PRNG seeded from a genesis hash (prevents compression attacks)
//   • 4 MiB write buffer to saturate NVMe sequential write speed
//   • Progress bar via `indicatif`
//   • Skip re-generation if a plot of the correct size already exists
//   • Optional genesis seed override (supports miner-ID binding in future)
//
// ─────────────────────────────────────────────────────────────────────────────

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::error::Error;

use indicatif::{ProgressBar, ProgressStyle};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// Size of the in-memory write buffer (4 MiB).  Chosen to amortise syscall
/// overhead and match typical NVMe write queue depth sweet-spots.
const WRITE_BUF: usize = 4 * 1024 * 1024;

/// Check whether a plot file already exists at `path` with exactly
/// `expected_bytes` bytes.  Used to skip expensive re-generation.
pub fn plot_is_valid(path: &Path, expected_bytes: u64) -> bool {
    match fs::metadata(path) {
        Ok(meta) => meta.len() == expected_bytes,
        Err(_)   => false,
    }
}

/// Generate (or regenerate) the plot file at `path`.
///
/// # Arguments
/// * `path`          — destination file path
/// * `total_bytes`   — desired plot size in bytes (should be a multiple of 4096)
/// * `genesis_seed`  — 32-byte seed from the network genesis block hash
/// * `force`         — if `true`, overwrite an existing valid plot
pub fn initialize_plot(
    path:         &Path,
    total_bytes:  u64,
    genesis_seed: &[u8; 32],
    force:        bool,
) -> Result<(), Box<dyn Error>> {
    // Skip if already valid and force not requested.
    if !force && plot_is_valid(path, total_bytes) {
        println!(
            "  ✓ Plot already exists at {:?} ({} bytes). Use --force to regenerate.",
            path, total_bytes
        );
        return Ok(());
    }

    println!("  Generating plot: {} bytes → {:?}", total_bytes, path);

    let file   = File::create(path)?;
    let mut writer = BufWriter::with_capacity(WRITE_BUF, file);
    let mut rng    = ChaCha8Rng::from_seed(*genesis_seed);

    // Progress bar
    let pb = ProgressBar::new(total_bytes);
    pb.set_style(
        ProgressStyle::with_template(
            "  [{elapsed_precise}] {bar:45.cyan/blue} {bytes}/{total_bytes} ({bytes_per_sec}) ETA {eta}"
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏ "),
    );

    let mut write_buf = vec![0u8; WRITE_BUF];
    let mut bytes_written: u64 = 0;

    while bytes_written < total_bytes {
        let chunk = std::cmp::min(WRITE_BUF as u64, total_bytes - bytes_written) as usize;
        rng.fill_bytes(&mut write_buf[..chunk]);
        writer.write_all(&write_buf[..chunk])?;
        bytes_written += chunk as u64;
        pb.set_position(bytes_written);
    }

    writer.flush()?;
    pb.finish_with_message("done");
    Ok(())
}
