use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// Mining metrics tracking performance data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningMetrics {
    pub total_attempts: u64,
    pub successful_blocks: u64,
    pub total_duration: Duration,
    pub start_time: String,
    pub end_time: String,
    pub average_time_per_block: Duration,
    pub hash_rate: f64, // hashes per second
    pub io_bytes_read: u64,
}

/// Statistics tracker for mining operations
pub struct MetricsCollector {
    start_instant: Instant,
    start_time_str: String,
    total_attempts: u64,
    successful_blocks: u64,
    io_bytes_read: u64,
    block_times: Vec<Duration>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        let now = chrono::Local::now();
        Self {
            start_instant: Instant::now(),
            start_time_str: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            total_attempts: 0,
            successful_blocks: 0,
            io_bytes_read: 0,
            block_times: Vec::new(),
        }
    }

    pub fn record_attempt(&mut self) {
        self.total_attempts += 1;
    }

    pub fn record_block_found(&mut self, duration: Duration) {
        self.successful_blocks += 1;
        self.block_times.push(duration);
    }

    pub fn record_io_bytes(&mut self, bytes: u64) {
        self.io_bytes_read += bytes;
    }

    pub fn finalize(&self) -> MiningMetrics {
        let total_duration = self.start_instant.elapsed();
        let now = chrono::Local::now();
        let end_time_str = now.format("%Y-%m-%d %H:%M:%S").to_string();

        let average_time_per_block = if self.successful_blocks > 0 {
            let total_block_time: Duration = self.block_times.iter().sum();
            total_block_time / self.successful_blocks as u32
        } else {
            Duration::from_secs(0)
        };

        let hash_rate = if total_duration.as_secs_f64() > 0.0 {
            self.total_attempts as f64 / total_duration.as_secs_f64()
        } else {
            0.0
        };

        MiningMetrics {
            total_attempts: self.total_attempts,
            successful_blocks: self.successful_blocks,
            total_duration,
            start_time: self.start_time_str.clone(),
            end_time: end_time_str,
            average_time_per_block,
            hash_rate,
            io_bytes_read: self.io_bytes_read,
        }
    }

    pub fn print_summary(&self) {
        let metrics = self.finalize();
        println!("\n=== Mining Statistics ===");
        println!("Total Attempts: {}", metrics.total_attempts);
        println!("Successful Blocks: {}", metrics.successful_blocks);
        println!("Total Duration: {:.2}s", metrics.total_duration.as_secs_f64());
        println!(
            "Average Time per Block: {:.2}ms",
            metrics.average_time_per_block.as_secs_f64() * 1000.0
        );
        println!("Hash Rate: {:.2} hashes/sec", metrics.hash_rate);
        println!("Total I/O Data Read: {:.2} MB", metrics.io_bytes_read as f64 / 1_000_000.0);
        println!("Start Time: {}", metrics.start_time);
        println!("End Time: {}", metrics.end_time);
    }

    pub fn to_json(&self) -> String {
        let metrics = self.finalize();
        serde_json::to_string_pretty(&metrics).unwrap_or_else(|_| "Error serializing metrics".to_string())
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}
