use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};

pub fn read_chunk_at_offset(file: &mut File, offset: u64, buffer: &mut [u8]) -> io::Result<()> {
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(buffer)?;
    Ok(())
}
