use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::error::Error;
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use rand_core::RngCore;

pub fn initialize_plot(path: &Path, total_bytes: u64, genesis_seed: &[u8; 32]) -> Result<(), Box<dyn Error>> {
    let file = File::create(path)?;
    // Use BufWriter for more efficient I/O, writing in larger chunks
    let mut writer = BufWriter::new(file);
    let mut rng = ChaCha8Rng::from_seed(*genesis_seed);
    
    // Use a 4 MB buffer instead of 4 KB for vastly improved generation speed
    let chunk_size = 4 * 1024 * 1024;
    let mut buffer = vec![0u8; chunk_size];
    
    let mut bytes_written = 0;
    while bytes_written < total_bytes {
        let write_size = std::cmp::min(chunk_size as u64, total_bytes - bytes_written) as usize;
        rng.fill_bytes(&mut buffer[..write_size]);
        writer.write_all(&buffer[..write_size])?;
        bytes_written += write_size as u64;
    }
    
    writer.flush()?;
    Ok(())
}
