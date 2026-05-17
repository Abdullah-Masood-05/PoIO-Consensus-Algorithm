use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Configuration for PoIO mining operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub plot_size: u64,
    pub plot_path: String,
    pub starting_nonce: u64,
    pub difficulty: u8,
    pub max_attempts: u64,
    pub enable_metrics: bool,
    pub enable_block_history: bool,
    pub block_history_file: String,
    pub metrics_file: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            plot_size: 52428800,          // 50 MB
            plot_path: "./poio_test.plot".to_string(),
            starting_nonce: 1,
            difficulty: 4,
            max_attempts: 1000,
            enable_metrics: true,
            enable_block_history: true,
            block_history_file: "./block_history.json".to_string(),
            metrics_file: "./mining_metrics.json".to_string(),
        }
    }
}

impl Config {
    /// Load configuration from TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let content = fs::read_to_string(path)?;
        toml::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Save configuration to TOML file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let content =
            toml::to_string_pretty(self).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, e)
            })?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Create default config file at specified location
    pub fn create_default_config<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let config = Config::default();
        config.save_to_file(&path)?;
        println!("Created default configuration file at {:?}", path.as_ref());
        Ok(config)
    }

    /// Print configuration details
    pub fn print_config(&self) {
        println!("\n=== PoIO Configuration ===");
        println!("Plot Size: {} bytes ({:.2} MB)",
            self.plot_size,
            self.plot_size as f64 / 1_000_000.0);
        println!("Plot Path: {}", self.plot_path);
        println!("Starting Nonce: {}", self.starting_nonce);
        println!("Difficulty: {} bits", self.difficulty);
        println!("Max Attempts: {}", self.max_attempts);
        println!("Enable Metrics: {}", self.enable_metrics);
        println!("Enable Block History: {}", self.enable_block_history);
        if self.enable_block_history {
            println!("Block History File: {}", self.block_history_file);
        }
        if self.enable_metrics {
            println!("Metrics File: {}", self.metrics_file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.plot_size, 52428800);
        assert_eq!(config.difficulty, 4);
        assert_eq!(config.max_attempts, 1000);
    }
}
