use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

/// Represents a successfully mined block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub height: u64,
    pub timestamp: String,
    pub nonce: u64,
    pub hash: String,
    pub difficulty: u8,
    pub time_to_mine: f64, // in milliseconds
}

/// Block history ledger for tracking mined blocks
pub struct BlockHistory {
    blocks: Vec<Block>,
}

impl BlockHistory {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
        }
    }

    /// Add a block to the history
    pub fn add_block(
        &mut self,
        nonce: u64,
        hash: String,
        difficulty: u8,
        time_to_mine_ms: f64,
    ) {
        let now = chrono::Local::now();
        let block = Block {
            height: self.blocks.len() as u64,
            timestamp: now.format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            nonce,
            hash,
            difficulty,
            time_to_mine: time_to_mine_ms,
        };
        self.blocks.push(block);
    }

    /// Get all blocks
    #[allow(dead_code)]
    pub fn get_blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Get block count
    #[allow(dead_code)]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Get last block
    pub fn last_block(&self) -> Option<&Block> {
        self.blocks.last()
    }

    /// Save blocks to JSON file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.blocks)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(())
    }

    /// Load blocks from JSON file
    #[allow(dead_code)]
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let blocks: Vec<Block> = serde_json::from_reader(file)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Self { blocks })
    }

    /// Print summary statistics
    pub fn print_summary(&self) {
        if self.blocks.is_empty() {
            println!("No blocks mined yet.");
            return;
        }

        println!("\n=== Block History Summary ===");
        println!("Total Blocks: {}", self.blocks.len());

        let total_time: f64 = self.blocks.iter().map(|b| b.time_to_mine).sum();
        let avg_time = total_time / self.blocks.len() as f64;
        println!("Average Time to Mine: {:.2}ms", avg_time);
        println!("Total Time Mining: {:.2}s", total_time / 1000.0);

        if let Some(last) = self.last_block() {
            println!("\nLast Block:");
            println!("  Height: {}", last.height);
            println!("  Hash: {}", last.hash);
            println!("  Nonce: {}", last.nonce);
            println!("  Difficulty: {}", last.difficulty);
            println!("  Time: {}", last.timestamp);
        }
    }
}

impl Default for BlockHistory {
    fn default() -> Self {
        Self::new()
    }
}
