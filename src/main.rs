mod crypto;
mod disk;
mod plot;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "52428800")] // 50 MB in bytes
    plot_size: u64,

    #[arg(short, long, default_value = "./poio_test.plot")]
    path: PathBuf,
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
    
    println!("Initialization successful. Ready for Phase 3 integration.");
}
