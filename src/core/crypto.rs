use blake3::Hasher;

pub fn compute_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(data);
    let hash_array = hasher.finalize();
    *hash_array.as_bytes()
}
