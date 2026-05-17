// ─── main.rs ─────────────────────────────────────────────────────────────────
//
// PoIO CLI — entry point for the Proof of I/O consensus prototype.
//
// Subcommands
// ───────────
//  poio plot    — generate or regenerate a plot file
//  poio mine    — run the multi-threaded PoIO mining loop
//  poio bench   — benchmark local NVMe random-read throughput
//  poio verify  — verify a block proof JSON without disk access
//
// ─────────────────────────────────────────────────────────────────────────────

mod core;

use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use crate::core::{bench, crypto, miner, plot};
use crate::core::miner::{MinerConfig, MiningStats};

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name    = "poio",
    version = "0.2.0",
    author  = "Bazil Suhail, Abdullah Masood, Ebad Junaid",
    about   = "Proof of I/O (PoIO) — PCIe-bound consensus prototype",
    long_about = "\
Proof of I/O (PoIO) is an ASIC-resistant consensus algorithm that bounds mining\n\
capability to the physical latency of the PCIe NVMe storage interface rather\n\
than CPU computational throughput.  Each hash attempt requires 128 random 4 KiB\n\
reads from a pre-generated plot file, ensuring that enterprise hardware cannot\n\
gain an exponential advantage over consumer-grade NVMe drives.\n\n\
See IMPLEMENTATION.md for a full technical reference."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate (or regenerate) a plot file on your NVMe SSD.
    Plot {
        /// Output path for the plot file.
        #[arg(short, long, default_value = "./poio.plot")]
        path: PathBuf,

        /// Plot size in bytes.  Must be a multiple of 4096.
        /// Common values: 52428800 (50 MB), 1073741824 (1 GB).
        #[arg(short = 's', long, default_value = "52428800")]
        size: u64,

        /// 32-byte genesis seed as a hex string (64 hex chars).
        /// Defaults to all-zero (testnet genesis).
        #[arg(short, long, default_value = "0000000000000000000000000000000000000000000000000000000000000000")]
        genesis: String,

        /// Overwrite an existing plot file even if it is already the correct size.
        #[arg(short, long, default_value_t = false)]
        force: bool,
    },

    /// Start the PoIO mining loop against an existing plot file.
    Mine {
        /// Path to the plot file to mine against.
        #[arg(short, long, default_value = "./poio.plot")]
        path: PathBuf,

        /// Simulated block header (arbitrary UTF-8 string for prototype).
        #[arg(short, long, default_value = "poio_genesis_block_v1")]
        header: String,

        /// Starting nonce value.
        #[arg(short = 'n', long, default_value = "0")]
        nonce: u64,

        /// Difficulty: number of leading zero bits required in the final hash.
        /// 4  = very easy (demo),  8  = easy,  16 = medium,  24 = hard.
        #[arg(short = 'd', long, default_value = "4")]
        difficulty: u8,

        /// Number of worker threads.  Defaults to the number of logical CPUs.
        #[arg(short = 't', long)]
        threads: Option<usize>,

        /// Stop after this many total hash attempts (useful for CI / demos).
        #[arg(short = 'm', long)]
        max_attempts: Option<u64>,

        /// Save the winning proof to this JSON file.
        #[arg(short = 'o', long)]
        proof_out: Option<PathBuf>,
    },

    /// Benchmark your NVMe random 4 KiB read throughput under PoIO conditions.
    Bench {
        /// Path to the plot file used for benchmarking.
        #[arg(short, long, default_value = "./poio.plot")]
        path: PathBuf,

        /// Expected plot size in bytes (to compute num_chunks).
        #[arg(short = 's', long, default_value = "52428800")]
        size: u64,
    },

    /// Verify a block proof JSON (no disk access required).
    Verify {
        /// Path to the proof JSON file produced by `poio mine --proof-out`.
        #[arg(short, long)]
        proof: PathBuf,

        /// Difficulty level to verify against.
        #[arg(short = 'd', long, default_value = "4")]
        difficulty: u8,
    },
}

// ── Banner ────────────────────────────────────────────────────────────────────

fn print_banner() {
    println!("{}", r"
  ██████╗  ██████╗ ██╗ ██████╗
  ██╔══██╗██╔═══██╗██║██╔═══██╗
  ██████╔╝██║   ██║██║██║   ██║
  ██╔═══╝ ██║   ██║██║██║   ██║
  ██║     ╚██████╔╝██║╚██████╔╝
  ╚═╝      ╚═════╝ ╚═╝ ╚═════╝ ".bright_cyan());
    println!(
        "  {} v{}\n",
        "Proof of I/O  |  PCIe-Bound Consensus Prototype".bright_white().bold(),
        "0.2.0".dimmed()
    );
}

// ── Hex decode helper ─────────────────────────────────────────────────────────

fn parse_genesis_hex(hex: &str) -> Result<[u8; 32], String> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return Err(format!("genesis must be exactly 64 hex characters, got {}", hex.len()));
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
            .map_err(|_| format!("invalid hex character at position {}", i * 2))?;
    }
    Ok(out)
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    print_banner();
    let cli = Cli::parse();

    match cli.command {
        Commands::Plot { path, size, genesis, force } => cmd_plot(path, size, &genesis, force),
        Commands::Mine { path, header, nonce, difficulty, threads, max_attempts, proof_out } => {
            cmd_mine(path, header, nonce, difficulty, threads, max_attempts, proof_out);
        }
        Commands::Bench { path, size } => cmd_bench(path, size),
        Commands::Verify { proof, difficulty } => cmd_verify(proof, difficulty),
    }
}

// ── Subcommand: plot ──────────────────────────────────────────────────────────

fn cmd_plot(path: PathBuf, size: u64, genesis_hex: &str, force: bool) {
    println!("{}", "[ Plot Generation ]".bold().bright_cyan());

    // Validate size alignment
    if size % 4096 != 0 {
        eprintln!(
            "{} plot size {} is not a multiple of 4096. Rounding up.",
            "WARN".bright_yellow(),
            size
        );
    }
    let size = (size / 4096) * 4096;

    let genesis_seed = match parse_genesis_hex(genesis_hex) {
        Ok(s)  => s,
        Err(e) => { eprintln!("{} {}", "ERROR:".bright_red(), e); return; }
    };

    println!("  Path     : {:?}", path);
    println!("  Size     : {} bytes  ({:.1} MiB)", size, size as f64 / (1024.0 * 1024.0));
    println!("  Genesis  : {}", &genesis_hex[..16]);
    println!("  Chunks   : {}", size / 4096);
    println!();

    let t0 = Instant::now();
    match plot::initialize_plot(&path, size, &genesis_seed, force) {
        Ok(()) => {
            println!(
                "\n  {} Plot ready in {:.3}s",
                "✓".bright_green(),
                t0.elapsed().as_secs_f64()
            );
        }
        Err(e) => {
            eprintln!("{} Failed to generate plot: {}", "ERROR:".bright_red(), e);
        }
    }
}

// ── Subcommand: mine ──────────────────────────────────────────────────────────

fn cmd_mine(
    path:         PathBuf,
    header:       String,
    starting_nonce: u64,
    difficulty:   u8,
    threads_arg:  Option<usize>,
    max_attempts: Option<u64>,
    proof_out:    Option<PathBuf>,
) {
    println!("{}", "[ Mining ]".bold().bright_cyan());

    // Validate plot exists
    let plot_size = match std::fs::metadata(&path) {
        Ok(m)  => m.len(),
        Err(e) => {
            eprintln!(
                "{} Cannot access plot file {:?}: {}",
                "ERROR:".bright_red(), path, e
            );
            eprintln!("  Run `poio plot` first to generate a plot file.");
            return;
        }
    };

    let num_chunks = plot_size / 4096;
    if num_chunks == 0 {
        eprintln!("{} Plot file is too small (< 4096 bytes).", "ERROR:".bright_red());
        return;
    }

    let threads = threads_arg.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
    });

    println!("  Plot     : {:?}  ({} chunks)", path, num_chunks);
    println!("  Header   : {:?}", header);
    println!("  Nonce₀   : {}", starting_nonce);
    println!("  Difficulty: {} leading zero bits", difficulty);
    println!("  Threads  : {}", threads);
    if let Some(m) = max_attempts {
        println!("  Max tries: {}", m);
    }
    println!();

    // Graceful Ctrl-C handling
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = stop_flag.clone();
    ctrlc::set_handler(move || {
        println!("\n  {} Ctrl-C received — stopping...", "!".bright_yellow());
        stop_clone.store(true, Ordering::SeqCst);
    })
    .expect("failed to set Ctrl-C handler");

    let stats = Arc::new(MiningStats::new());
    let stats_display = stats.clone();
    let stop_display  = stop_flag.clone();

    // ── Progress display thread ───────────────────────────────────────────────
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("  {spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    spinner.enable_steady_tick(Duration::from_millis(100));

    let spinner_clone = spinner.clone();
    let _display_thread = thread::spawn(move || {
        while !stop_display.load(Ordering::Relaxed) {
            let attempts = stats_display.attempts.load(Ordering::Relaxed);
            let h_s      = stats_display.hashes_per_sec();
            let iops     = stats_display.iops();
            spinner_clone.set_message(format!(
                "Attempts: {:>8}  |  {:.2} h/s  |  {:.0} IOPS",
                attempts, h_s, iops
            ));
            thread::sleep(Duration::from_millis(250));
        }
    });

    // ── Launch miner ──────────────────────────────────────────────────────────
    let config = MinerConfig {
        plot_path:      path.clone(),
        num_chunks,
        block_header:   header.as_bytes().to_vec(),
        starting_nonce,
        difficulty,
        threads,
        max_attempts,
    };

    let t0 = Instant::now();
    let result = miner::run_miner(&config, stats.clone(), stop_flag.clone());
    stop_flag.store(true, Ordering::SeqCst);
    spinner.finish_and_clear();

    let elapsed = t0.elapsed();
    let total_attempts = stats.attempts.load(Ordering::Relaxed);
    let final_iops     = stats.iops();
    let final_hps      = stats.hashes_per_sec();

    println!();
    println!("{}", "━".repeat(60).bright_cyan());

    match result {
        Some(proof) => {
            let hash_hex = crypto::hex_encode(&proof.final_hash);
            println!("  {} {} found!", "BLOCK".bright_green().bold(), "✓".bright_green());
            println!("  Nonce      : {}", proof.nonce.to_string().bright_white());
            println!("  Hash       : {}", hash_hex.bright_yellow());
            println!("  Difficulty : {} leading zero bits", difficulty);
            println!("  Elapsed    : {}", miner::format_duration(elapsed).bright_white());
            println!("  Attempts   : {}", total_attempts);
            println!("  Throughput : {:.2} h/s  |  {:.0} IOPS", final_hps, final_iops);

            // Optionally write proof JSON
            if let Some(out_path) = proof_out {
                match serde_json::to_string_pretty(&proof) {
                    Ok(json) => {
                        match std::fs::write(&out_path, json) {
                            Ok(()) => println!(
                                "\n  {} Proof saved to {:?}",
                                "✓".bright_green(), out_path
                            ),
                            Err(e) => eprintln!("{} Writing proof: {}", "ERROR:".bright_red(), e),
                        }
                    }
                    Err(e) => eprintln!("{} Serializing proof: {}", "ERROR:".bright_red(), e),
                }
            } else {
                println!(
                    "\n  {} Use `--proof-out <file.json>` to save the proof for verification.",
                    "TIP".bright_cyan()
                );
            }
        }
        None => {
            if stop_flag.load(Ordering::Relaxed) && total_attempts == 0 {
                println!("  {} Mining cancelled before any attempt.", "—".dimmed());
            } else {
                println!(
                    "  {} No valid nonce found in {} attempts.",
                    "✗".bright_red(), total_attempts
                );
                println!("  Try increasing `--difficulty` or `--max-attempts`.");
            }
        }
    }
    println!("{}\n", "━".repeat(60).bright_cyan());
}

// ── Subcommand: bench ─────────────────────────────────────────────────────────

fn cmd_bench(path: PathBuf, size: u64) {
    println!("{}", "[ I/O Benchmark ]".bold().bright_cyan());

    if !path.exists() {
        eprintln!(
            "{} Plot file not found at {:?}. Run `poio plot` first.",
            "ERROR:".bright_red(), path
        );
        return;
    }

    let num_chunks = size / 4096;
    if num_chunks == 0 {
        eprintln!("{} Plot size too small.", "ERROR:".bright_red());
        return;
    }

    println!("  Plot     : {:?}", path);
    println!("  Chunks   : {}", num_chunks);
    println!("  Reads    : 1024 random 4 KiB reads\n");

    match bench::run_benchmark(&path, num_chunks) {
        Ok(result) => bench::print_report(&result),
        Err(e)     => eprintln!("{} Benchmark failed: {}", "ERROR:".bright_red(), e),
    }
}

// ── Subcommand: verify ────────────────────────────────────────────────────────

fn cmd_verify(proof_path: PathBuf, difficulty: u8) {
    println!("{}", "[ Block Proof Verification ]".bold().bright_cyan());
    println!("  Proof    : {:?}", proof_path);
    println!("  Difficulty: {} leading zero bits\n", difficulty);

    let json = match std::fs::read_to_string(&proof_path) {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("{} Cannot read proof file: {}", "ERROR:".bright_red(), e);
            return;
        }
    };

    let proof: miner::BlockProof = match serde_json::from_str(&json) {
        Ok(p)  => p,
        Err(e) => {
            eprintln!("{} Invalid proof JSON: {}", "ERROR:".bright_red(), e);
            return;
        }
    };

    println!("  Nonce      : {}", proof.nonce);
    println!("  Hash       : {}", crypto::hex_encode(&proof.final_hash).bright_yellow());
    println!("  Chunks     : {}", proof.chunks.len());
    println!();

    match crypto::verify_block_proof(&proof, difficulty) {
        Ok(()) => {
            println!("  {} Proof is VALID — all 128 chunk indices match deterministic derivation", "✓".bright_green().bold());
            println!("  {} Hash satisfies difficulty {}", "✓".bright_green(), difficulty);
        }
        Err(reason) => {
            println!("  {} Proof is INVALID: {}", "✗".bright_red().bold(), reason);
        }
    }
    println!();
}
