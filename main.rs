//mod utils:crypto;
//mod utils::disk;
// mod utils::plot;

mod utils; // Declares the utils/mod.rs file

// Bring your modules into the current scope
use crate::utils::crypto;
use crate::utils::disk;
use crate::utils::plot;

use clap::Parser;
use std::path::PathBuf;
use std::fs::File;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use rand_core::RngCore;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 's', long, default_value = "52428800")] // 50 MB in bytes
    plot_size: u64,

    #[arg(short = 'p', long, default_value = "./poio_test.plot")]
    path: PathBuf,

    #[arg(short = 'n', long, default_value = "1")]
    nonce: u64,

    #[arg(short = 'd', long, default_value = "4")]
    difficulty: u8,
}

fn main() {
    println!("=== Proof of I/O (PoIO) Node Initializing ===");
    let args = Args::parse();
    println!("Target Plot Size: {} bytes", args.plot_size);
    
    // Initialize loosely-coupled pipeline
    let genesis_seed: [u8; 32] = [0u8; 32];
    
    // Create or open the plot in a modular fashion
    if let Err(e) = plot::initialize_plot(&args.path, args.plot_size, &genesis_seed) {
        eprintln!("Failed to initialize plot: {}", e);
        return;
    }
    
    println!("Plot initialized successfully at {:?}", args.path);
    println!("\n=== Starting Mining Process ===");
    
    // Open plot file for mining
    match File::open(&args.path) {
        Ok(mut file) => {
            let num_chunks = args.plot_size / 4096;
            println!("Total chunks available: {}", num_chunks);
            println!("Difficulty target: leading {} zero bits", args.difficulty);
            
            // Mine with the provided nonce
            let start_time = Instant::now();
            match mine_block(&mut file, num_chunks, args.nonce, args.difficulty) {
                Ok((hash, nonce_used)) => {
                    let elapsed = start_time.elapsed();
                    println!("\nBlock found!");
                    println!("Winning nonce: {}", nonce_used);
                    println!("Hash: {}", hex_encode(&hash));
                    println!("Time elapsed: {:?}", elapsed);
                }
                Err(e) => {
                    eprintln!("Mining failed: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open plot file: {}", e);
        }
    }
    
}

/// Mine a single block by attempting different nonces
fn mine_block(
    file: &mut File,
    num_chunks: u64,
    starting_nonce: u64,
    difficulty: u8,
) -> std::io::Result<([u8; 32], u64)> {
    let block_header = b"test_block_header"; // Simulated block header
    let mut nonce = starting_nonce;
    let max_attempts = 1000; // Limit attempts for demo
    
    for attempt in 0..max_attempts {
        // Compute seed from block header and nonce
        let mut seed_input = block_header.to_vec();
        seed_input.extend_from_slice(&nonce.to_le_bytes());
        let seed = crypto::compute_hash(&seed_input);
        
        // Generate 128 random offsets using ChaCha20
        let mut rng = ChaCha8Rng::from_seed(seed);
        let mut chunk_data = Vec::new();
        
        for _ in 0..128 {
            let offset_idx = (rng.next_u64() % num_chunks) as u64;
            let offset = offset_idx * 4096;
            
            // Read chunk from disk
            if let Ok(chunk) = disk::read_chunk_at_offset(file, offset) {
                chunk_data.extend_from_slice(&chunk);
            }
        }
        
        // Compute final hash
        let final_hash = crypto::compute_hash(&chunk_data);
        
        // Check if hash meets difficulty (leading zero bits)
        if check_difficulty(&final_hash, difficulty) {
            return Ok((final_hash, nonce));
        }
        
        if attempt % 100 == 0 {
            println!("Attempt {}: nonce {} - hash doesn't meet difficulty", attempt, nonce);
        }
        
        nonce += 1;
    }
    
    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Failed to find valid nonce within max attempts",
    ))
}

/// Check if hash meets difficulty requirement (leading zero bits)
fn check_difficulty(hash: &[u8; 32], difficulty: u8) -> bool {
    let byte_idx = (difficulty / 8) as usize;
    let bit_idx = difficulty % 8;
    
    for i in 0..byte_idx {
        if hash[i] != 0 {
            return false;
        }
    }
    
    if byte_idx < 32 {
        let mask = 0xFF << (8 - bit_idx);
        hash[byte_idx] & mask == 0
    } else {
        true
    }
}

/// Encode bytes as hexadecimal string
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
