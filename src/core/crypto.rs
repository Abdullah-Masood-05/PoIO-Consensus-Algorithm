use blake3::Hasher;

pub struct HashState {
    hasher: Hasher,
}

impl HashState {
    pub fn new() -> Self {
        Self { hasher: Hasher::new() }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data);
    }

    pub fn finalize(self) -> [u8; 32] {
        *self.hasher.finalize().as_bytes()
    }
}
