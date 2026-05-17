// ─── core/crypto.rs ─────────────────────────────────────────────────────────
//
// All cryptographic primitives used by PoIO:
//   • Blake3 streaming hash state
//   • Seed derivation from (block_header, nonce)
//   • Deterministic chunk-index generation from a seed
//   • Difficulty target check
//   • Asymmetric block-proof verification (no disk access required)
//
// NOTE: No heap allocation inside the hot mining path.
// ─────────────────────────────────────────────────────────────────────────────

use blake3::Hasher;
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

use crate::progress::miner::{BlockProof, CHUNK_SIZE, REQUIRED_READS};

// ── Streaming BLAKE3 wrapper ──────────────────────────────────────────────────

/// Thin wrapper around `blake3::Hasher`.  Re-created per mining attempt so
/// it stays on the stack (the internal state is ~64 bytes).
pub struct HashState {
    hasher: Hasher,
}

impl HashState {
    #[inline]
    pub fn new() -> Self {
        Self { hasher: Hasher::new() }
    }

    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    /// Consumes the state and returns the final 32-byte digest.
    #[inline]
    pub fn finalize(self) -> [u8; 32] {
        *self.hasher.finalize().as_bytes()
    }
}

// ── Seed derivation ───────────────────────────────────────────────────────────

/// Derive a 32-byte mining seed from a block header and a nonce.
/// seed = Blake3(header || nonce_le)
#[inline]
pub fn derive_seed(block_header: &[u8], nonce: u64) -> [u8; 32] {
    let mut state = HashState::new();
    state.update(block_header);
    state.update(&nonce.to_le_bytes());
    state.finalize()
}

// ── Chunk index generation ────────────────────────────────────────────────────

/// From a 32-byte seed, generate `REQUIRED_READS` pseudo-random chunk indices
/// in `[0, num_chunks)` using ChaCha8 PRNG.  Fully deterministic.
#[inline]
pub fn generate_chunk_indices(seed: &[u8; 32], num_chunks: u64) -> [u64; REQUIRED_READS] {
    let mut rng = ChaCha8Rng::from_seed(*seed);
    let mut indices = [0u64; REQUIRED_READS];
    for idx in indices.iter_mut() {
        *idx = rng.next_u64() % num_chunks;
    }
    indices
}

// ── Difficulty check ──────────────────────────────────────────────────────────

/// Returns `true` if the hash has at least `difficulty` leading zero bits.
#[inline]
pub fn meets_difficulty(hash: &[u8; 32], difficulty: u8) -> bool {
    let full_bytes = (difficulty / 8) as usize;
    let remainder  = difficulty % 8;

    for byte in &hash[..full_bytes] {
        if *byte != 0 {
            return false;
        }
    }
    if full_bytes < 32 && remainder > 0 {
        let mask = 0xFF_u8 << (8 - remainder);
        hash[full_bytes] & mask == 0
    } else {
        true
    }
}

// ── Asymmetric block-proof verification ───────────────────────────────────────
//
// A verifying node receives the `BlockProof` (which contains the 128 raw
// 4 KB chunks alongside their indices).  It never touches a disk; it simply:
//   1. Re-derives the seed from (header, nonce).
//   2. Re-derives the expected chunk indices from the seed.
//   3. Checks that each attached chunk matches its declared index (structural).
//   4. Re-computes the final hash from the attached chunks.
//   5. Checks that the final hash meets the declared difficulty.
//
// This makes verification O(1) disk I/O — purely CPU-bound.

/// Verify a `BlockProof` produced by a miner.
/// Returns `Ok(())` on success, `Err(reason)` on failure.
pub fn verify_block_proof(proof: &BlockProof, difficulty: u8) -> Result<(), &'static str> {
    // 1. Re-derive seed
    let seed = derive_seed(&proof.block_header, proof.nonce);

    // 2. Re-derive expected chunk indices
    let expected_indices = generate_chunk_indices(&seed, proof.num_chunks);

    // 3. Validate that the proof carries exactly the right number of chunks
    if proof.chunks.len() != REQUIRED_READS {
        return Err("proof contains wrong number of chunks");
    }
    if proof.chunk_indices.len() != REQUIRED_READS {
        return Err("proof contains wrong number of chunk indices");
    }

    // 4. Validate declared indices match deterministic derivation
    for i in 0..REQUIRED_READS {
        if proof.chunk_indices[i] != expected_indices[i] {
            return Err("chunk index mismatch — proof is invalid");
        }
        if proof.chunks[i].len() != CHUNK_SIZE {
            return Err("chunk has incorrect size");
        }
    }

    // 5. Re-compute the final hash from the attached chunks
    let mut final_state = HashState::new();
    for chunk in &proof.chunks {
        final_state.update(chunk);
    }
    let computed_hash = final_state.finalize();

    // 6. Ensure hash matches the claimed hash
    if computed_hash != proof.final_hash {
        return Err("final hash does not match recomputed value");
    }

    // 7. Check difficulty
    if !meets_difficulty(&computed_hash, difficulty) {
        return Err("hash does not meet required difficulty");
    }

    Ok(())
}

// ── Hex helpers ───────────────────────────────────────────────────────────────

/// Encode a byte slice as a lowercase hex string.  Pre-allocates exact capacity.
pub fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}
