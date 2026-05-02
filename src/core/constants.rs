/// Chunk size in bytes for disk I/O operations
pub const CHUNK_SIZE: usize = 4096;

/// Number of random chunks to read per mining attempt
pub const CHUNKS_PER_ATTEMPT: u64 = 128;

/// Maximum difficulty in bits (cannot exceed 255 bits for blake3 hash with u8)
pub const MAX_DIFFICULTY: u8 = 255;

/// Minimum plot size in bytes (must be at least one chunk)
pub const MIN_PLOT_SIZE: u64 = CHUNK_SIZE as u64;
