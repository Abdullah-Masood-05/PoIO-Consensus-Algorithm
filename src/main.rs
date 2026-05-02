mod core;

use crate::core::{constants, crypto, disk, plot};
use clap::Parser;
use rand_chacha::ChaCha8Rng;
use rand_core::RngCore;
use rand_core::SeedableRng;
use std::fs::File;
use std::path::PathBuf;
use std::process;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(
    author = "PoIO Contributors",
    version = "0.1.0",
    about = "Proof of I/O (PoIO): ASIC-resistant consensus via storage I/O",
    long_about = "A read-bound hashing algorithm that ties mining capability to storage interface bandwidth, \
                   preventing hardware centralization by using random I/O operations instead of CPU/GPU power."
)]
struct Args {
    #[arg(
        short = 's',
        long,
        default_value = "52428800",
        help = "Plot file size in bytes (minimum: 4096)",
        value_name = "BYTES"
    )]
    plot_size: u64,

    #[arg(
        short = 'p',
        long,
        default_value = "./poio_test.plot",
        help = "Path to the plot file",
        value_name = "PATH"
    )]
    path: PathBuf,

    #[arg(
        short = 'n',
        long,
        default_value = "1",
        help = "Starting nonce value",
        value_name = "NONCE"
    )]
    nonce: u64,

    #[arg(
        short = 'd',
        long,
        default_value = "4",
        help = "Difficulty target in leading zero bits (0-256)",
        value_name = "BITS"
    )]
    difficulty: u8,

    #[arg(
        short = 'm',
        long,
        default_value = "1000",
        help = "Maximum mining attempts per block",
        value_name = "ATTEMPTS"
    )]
    max_attempts: u64,
}

fn main() {
    println!("=== Proof of I/O (PoIO) Node Initializing ===");
    let args = Args::parse();

    // Validate input parameters
    if let Err(e) = validate_args(&args) {
        eprintln!("Configuration error: {}", e);
        process::exit(1);
    }

    println!("Target Plot Size: {} bytes", args.plot_size);

    // Initialize with genesis seed (in production, this would be derived from genesis block)
    let genesis_seed: [u8; 32] = [0u8; 32];

    // Create or open the plot in a modular fashion
    if let Err(e) = plot::initialize_plot(&args.path, args.plot_size, &genesis_seed) {
        eprintln!("Failed to initialize plot: {}", e);
        process::exit(1);
    }

    println!("Plot initialized successfully at {:?}", args.path);
    println!("\n=== Starting Mining Process ===");

    // Open plot file for mining
    match File::open(&args.path) {
        Ok(mut file) => {
            let num_chunks = args.plot_size / constants::CHUNK_SIZE as u64;
            println!("Total chunks available: {}", num_chunks);
            println!("Difficulty target: leading {} zero bits", args.difficulty);
            println!("Max mining attempts: {}\n", args.max_attempts);

            // Mine with the provided nonce
            let start_time = Instant::now();
            match mine_block(
                &mut file,
                num_chunks,
                args.nonce,
                args.difficulty,
                args.max_attempts,
            ) {
                Ok((hash, nonce_used)) => {
                    let elapsed = start_time.elapsed();
                    println!("\n✓ Block found!");
                    println!("Winning nonce: {}", nonce_used);
                    println!("Hash: {}", hex_encode(&hash));
                    println!("Time elapsed: {:?}", elapsed);
                }
                Err(e) => {
                    eprintln!("✗ Mining failed: {}", e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open plot file: {}", e);
            process::exit(1);
        }
    }
}

/// Validate command-line arguments
fn validate_args(args: &Args) -> Result<(), String> {
    if args.plot_size < constants::MIN_PLOT_SIZE {
        return Err(format!(
            "plot_size must be at least {} bytes (one chunk), got {}",
            constants::MIN_PLOT_SIZE,
            args.plot_size
        ));
    }

    if args.plot_size % constants::CHUNK_SIZE as u64 != 0 {
        return Err(format!(
            "plot_size must be a multiple of {} bytes (chunk size), got {}",
            constants::CHUNK_SIZE,
            args.plot_size
        ));
    }

    if args.difficulty > constants::MAX_DIFFICULTY {
        return Err(format!(
            "difficulty must be 0-{} bits, got {}",
            constants::MAX_DIFFICULTY,
            args.difficulty
        ));
    }

    if args.max_attempts == 0 {
        return Err("max_attempts must be greater than 0".to_string());
    }

    Ok(())
}

/// Mine a single block by attempting different nonces
fn mine_block(
    file: &mut File,
    num_chunks: u64,
    starting_nonce: u64,
    difficulty: u8,
    max_attempts: u64,
) -> std::io::Result<([u8; 32], u64)> {
    let block_header = b"test_block_header"; // Simulated block header
    let mut nonce = starting_nonce;

    for attempt in 0..max_attempts {
        // Compute seed from block header and nonce
        let mut seed_input = block_header.to_vec();
        seed_input.extend_from_slice(&nonce.to_le_bytes());
        let seed = crypto::compute_hash(&seed_input);

        // Generate 128 random offsets using ChaCha8 PRNG
        let mut rng = ChaCha8Rng::from_seed(seed);
        let mut chunk_data =
            Vec::with_capacity((constants::CHUNKS_PER_ATTEMPT as usize) * constants::CHUNK_SIZE);

        let mut failed_reads = 0;
        for _ in 0..constants::CHUNKS_PER_ATTEMPT {
            let offset_idx = (rng.next_u64() % num_chunks) as u64;
            let offset = offset_idx * constants::CHUNK_SIZE as u64;

            // Read chunk from disk
            match disk::read_chunk_at_offset(file, offset) {
                Ok(chunk) => {
                    chunk_data.extend_from_slice(&chunk);
                }
                Err(e) => {
                    failed_reads += 1;
                    eprintln!("Warning: Failed to read chunk at offset {}: {}", offset, e);
                }
            }
        }

        // Ensure we have data to hash (at least some chunks were read)
        if chunk_data.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Failed to read any chunks (read {} of {} chunks)",
                    constants::CHUNKS_PER_ATTEMPT as u64 - failed_reads as u64,
                    constants::CHUNKS_PER_ATTEMPT
                ),
            ));
        }

        // Compute final hash
        let final_hash = crypto::compute_hash(&chunk_data);

        // Check if hash meets difficulty (leading zero bits)
        if check_difficulty(&final_hash, difficulty) {
            return Ok((final_hash, nonce));
        }

        if attempt % 100 == 0 && attempt > 0 {
            println!(
                "Attempt {}: nonce {} - hash doesn't meet difficulty",
                attempt, nonce
            );
        }

        nonce += 1;
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!(
            "Failed to find valid nonce within {} attempts",
            max_attempts
        ),
    ))
}

/// Check if hash meets difficulty requirement (leading zero bits)
/// Safely handles any difficulty value 0-255
fn check_difficulty(hash: &[u8; 32], difficulty: u8) -> bool {
    if difficulty == 0 {
        return true; // No difficulty requirement
    }

    let byte_idx = (difficulty / 8) as usize;
    let bit_idx = difficulty % 8;

    // Check all leading complete bytes are zero
    for i in 0..byte_idx {
        if hash[i] != 0 {
            return false;
        }
    }

    // Check the partial byte if needed
    if byte_idx < 32 {
        let mask = 0xFF << (8 - bit_idx);
        hash[byte_idx] & mask == 0
    } else {
        // byte_idx == 32 (difficulty == 256, but u8 max is 255, so this won't happen)
        true
    }
}

/// Encode bytes as hexadecimal string
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
