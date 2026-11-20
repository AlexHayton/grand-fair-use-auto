//! State hashes for replays: FNV-1a over a fixed little-endian encoding.
//!
//! Not `std::hash`: its output may change between Rust releases, and a
//! replay's hashes must stay comparable across builds and platforms.

/// A 64-bit FNV-1a hasher.
#[derive(Debug, Clone, Copy)]
pub struct StateHasher(u64);

impl Default for StateHasher {
    fn default() -> Self {
        StateHasher(0xcbf2_9ce4_8422_2325)
    }
}

impl StateHasher {
    pub fn bytes(&mut self, data: &[u8]) {
        for &b in data {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u8(&mut self, v: u8) {
        self.bytes(&[v]);
    }

    pub fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

/// Something that contributes to the state hash. Every field that affects
/// future ticks must be fed in.
pub trait SimHash {
    fn sim_hash(&self, h: &mut StateHasher);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_reference() {
        // The published FNV-1a 64 test vector for "a".
        let mut h = StateHasher::default();
        h.bytes(b"a");
        assert_eq!(h.finish(), 0xaf63_dc4c_8601_ec8c);
    }
}
