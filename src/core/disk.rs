use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

pub fn read_chunk_at_offset(file: &mut File, offset: u64) -> io::Result<Vec<u8>> {
    let mut buffer = vec![0u8; 4096];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut buffer)?;
    Ok(buffer)
}
