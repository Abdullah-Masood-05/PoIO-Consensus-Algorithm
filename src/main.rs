mod core;

use crate::core::{constants, crypto, disk, plot, metrics, block_history, config};
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
    version = "0.2.0",
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

    #[arg(
        short = 'c',
        long,
        help = "Load configuration from TOML file",
        value_name = "CONFIG_FILE"
    )]
    config: Option<PathBuf>,

    #[arg(
        long,
        help = "Create a default configuration file and exit",
        value_name = "CONFIG_FILE"
    )]
    init_config: Option<PathBuf>,

    #[arg(
        long,
        help = "Enable real-time progress indicator"
    )]
    progress: bool,

    #[arg(
        long,
        help = "Enable verbose logging"
    )]
    verbose: bool,
}

fn main() {
    println!("=== Proof of I/O (PoIO) Node Initializing ===");
    let args = Args::parse();

    // Handle init-config flag
    if let Some(config_path) = &args.init_config {
        match config::Config::create_default_config(config_path) {
            Ok(_) => {
                println!("Configuration file created successfully.");
                process::exit(0);
            }
            Err(e) => {
                eprintln!("Failed to create config file: {}", e);
                process::exit(1);
            }
        }
    }

    // Load configuration
    let mut conf = if let Some(config_path) = &args.config {
        match config::Config::from_file(config_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to load config file: {}", e);
                process::exit(1);
            }
        }
    } else {
        config::Config::default()
    };

    // Override config with command-line arguments
    if args.plot_size != 52428800 {
        conf.plot_size = args.plot_size;
    }
    if args.path.to_string_lossy() != "./poio_test.plot" {
        conf.plot_path = args.path.to_string_lossy().to_string();
    }
    if args.nonce != 1 {
        conf.starting_nonce = args.nonce;
    }
    if args.difficulty != 4 {
        conf.difficulty = args.difficulty;
    }
    if args.max_attempts != 1000 {
        conf.max_attempts = args.max_attempts;
    }

    if args.verbose {
        conf.print_config();
    }

    // Validate configuration
    if let Err(e) = validate_config(&conf) {
        eprintln!("Configuration error: {}", e);
        process::exit(1);
    }

    let plot_path = PathBuf::from(&conf.plot_path);
    println!("Target Plot Size: {} bytes", conf.plot_size);

    // Initialize with genesis seed (in production, this would be derived from genesis block)
    let genesis_seed: [u8; 32] = [0u8; 32];

    // Create or open the plot
    if let Err(e) = plot::initialize_plot(&plot_path, conf.plot_size, &genesis_seed) {
        eprintln!("Failed to initialize plot: {}", e);
        process::exit(1);
    }

    println!("Plot initialized successfully at {:?}", plot_path);
    println!("\n=== Starting Mining Process ===");

    // Open plot file for mining
    match File::open(&plot_path) {
        Ok(mut file) => {
            let num_chunks = conf.plot_size / constants::CHUNK_SIZE as u64;
            println!("Total chunks available: {}", num_chunks);
            println!("Difficulty target: leading {} zero bits", conf.difficulty);
            println!("Max mining attempts: {}\n", conf.max_attempts);

            // Initialize metrics and block history
            let mut metrics_collector = metrics::MetricsCollector::new();
            let mut block_history = block_history::BlockHistory::new();

            // Mine blocks
            let start_time = Instant::now();
            match mine_block(
                &mut file,
                num_chunks,
                conf.starting_nonce,
                conf.difficulty,
                conf.max_attempts,
                &mut metrics_collector,
                &mut block_history,
                args.verbose,
            ) {
                Ok((hash, nonce_used)) => {
                    let elapsed = start_time.elapsed();
                    println!("\n✓ Block found!");
                    println!("Winning nonce: {}", nonce_used);
                    println!("Hash: {}", hex_encode(&hash));
                    println!("Time elapsed: {:?}", elapsed);

                    metrics_collector.record_block_found(elapsed);
                    block_history.add_block(
                        nonce_used,
                        hex_encode(&hash),
                        conf.difficulty,
                        elapsed.as_secs_f64() * 1000.0,
                    );
                }
                Err(e) => {
                    eprintln!("✗ Mining failed: {}", e);
                    process::exit(1);
                }
            }

            // Print statistics
            if conf.enable_metrics {
                metrics_collector.print_summary();
                if let Err(e) =
                    std::fs::write(&conf.metrics_file, metrics_collector.to_json())
                {
                    eprintln!("Warning: Failed to save metrics file: {}", e);
                } else if args.verbose {
                    println!("Metrics saved to: {}", conf.metrics_file);
                }
            }

            if conf.enable_block_history {
                block_history.print_summary();
                if let Err(e) = block_history.save_to_file(&conf.block_history_file) {
                    eprintln!("Warning: Failed to save block history: {}", e);
                } else if args.verbose {
                    println!("Block history saved to: {}", conf.block_history_file);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open plot file: {}", e);
            process::exit(1);
        }
    }
}

/// Validate configuration
fn validate_config(conf: &config::Config) -> Result<(), String> {
    if conf.plot_size < constants::MIN_PLOT_SIZE {
        return Err(format!(
            "plot_size must be at least {} bytes (one chunk), got {}",
            constants::MIN_PLOT_SIZE,
            conf.plot_size
        ));
    }

    if conf.plot_size % constants::CHUNK_SIZE as u64 != 0 {
        return Err(format!(
            "plot_size must be a multiple of {} bytes (chunk size), got {}",
            constants::CHUNK_SIZE,
            conf.plot_size
        ));
    }

    if conf.difficulty > constants::MAX_DIFFICULTY {
        return Err(format!(
            "difficulty must be 0-{} bits, got {}",
            constants::MAX_DIFFICULTY,
            conf.difficulty
        ));
    }

    if conf.max_attempts == 0 {
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
    metrics: &mut metrics::MetricsCollector,
    block_history: &mut block_history::BlockHistory,
    verbose: bool,
) -> std::io::Result<([u8; 32], u64)> {
    let block_header = b"test_block_header"; // Simulated block header
    let mut nonce = starting_nonce;

    for attempt in 0..max_attempts {
        metrics.record_attempt();

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
                    metrics.record_io_bytes(chunk.len() as u64);
                }
                Err(e) => {
                    failed_reads += 1;
                    if verbose {
                        eprintln!("Warning: Failed to read chunk at offset {}: {}", offset, e);
                    }
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

        if attempt % 100 == 0 && attempt > 0 && verbose {
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
