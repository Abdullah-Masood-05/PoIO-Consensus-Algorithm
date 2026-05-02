use crate::core::constants;
use rand_chacha::ChaCha8Rng;
use rand_core::RngCore;
use rand_core::SeedableRng;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn initialize_plot(
    path: &Path,
    total_bytes: u64,
    genesis_seed: &[u8; 32],
) -> Result<(), Box<dyn Error>> {
    let mut file = File::create(path)?;
    let mut rng = ChaCha8Rng::from_seed(*genesis_seed);

    let mut buffer = vec![0u8; constants::CHUNK_SIZE];

    for _ in 0..(total_bytes / constants::CHUNK_SIZE as u64) {
        rng.fill_bytes(&mut buffer);
        file.write_all(&buffer)?;
    }

    file.flush()?;
    Ok(())
}
