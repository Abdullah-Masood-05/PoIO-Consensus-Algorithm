use std::fs::File;
use std::os::unix::fs::FileExt;
use std::io;

pub fn read_chunk_at_offset(file: &File, offset: u64) -> io::Result<Vec<u8>> {
    let mut buffer = vec![0u8; 4096];
    file.read_exact_at(&mut buffer, offset)?;
    Ok(buffer)
}
