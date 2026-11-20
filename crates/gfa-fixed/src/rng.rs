//! The simulation's random numbers: linear congruential generators with
//! fixed seeds, in 32-bit arithmetic.
//!
//! After AmiGTA1 (MIT): `gta_traffic.c` and `gta_train.c` use Numerical
//! Recipes' constants, `gta_peds.c` the ANSI C example's, and the voice
//! picker a small 11-bit generator in `gta_main.c`.
//!
//! AmiGTA1 keeps its seeds in `unsigned long`, which is 32 bits on the
//! Amiga and 64 on a desktop host. Results taken from the low 32 bits agree
//! on both; the train's `seed >> 16` does not, so the host oracle's trains
//! will differ from these (the Amiga's answer is the one kept here).

/// AmiGTA1's traffic seed.
pub const TRAFFIC_SEED: u32 = 12345;
/// AmiGTA1's pedestrian seed.
pub const PED_SEED: u32 = 777;
/// AmiGTA1's train seed.
pub const TRAIN_SEED: u32 = 0x5eed;

/// `seed = seed * 1664525 + 1013904223` (Numerical Recipes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NrLcg {
    pub seed: u32,
}

impl NrLcg {
    /// A zero seed becomes 1, as in AmiGTA1's traffic.
    pub fn new(seed: u32) -> Self {
        NrLcg {
            seed: if seed == 0 { 1 } else { seed },
        }
    }

    fn step(&mut self) -> u32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        self.seed
    }

    /// Bits 16..30: the traffic's draw, `0..0x8000`.
    pub fn next15(&mut self) -> u32 {
        (self.step() >> 16) & 0x7FFF
    }

    /// Bits 16..31: the train's draw, `0..0x10000`.
    pub fn next16(&mut self) -> u32 {
        self.step() >> 16
    }
}

/// `seed = seed * 1103515245 + 12345` (the ANSI C example), drawing bits
/// 16..30.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnsiLcg {
    pub seed: u32,
}

impl AnsiLcg {
    /// A zero seed becomes 1, as in AmiGTA1's peds.
    pub fn new(seed: u32) -> Self {
        AnsiLcg {
            seed: if seed == 0 { 1 } else { seed },
        }
    }

    /// `0..0x8000`.
    pub fn next15(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        (self.seed >> 16) & 0x7FFF
    }
}

/// `x = (x * 0x79 + 1) % 0x800`: the voice-line picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct VoiceLcg {
    pub x: u32,
}

impl VoiceLcg {
    /// `0..0x800`.
    pub fn draw(&mut self) -> u32 {
        self.x = (self.x * 0x79 + 1) % 0x800;
        self.x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_draws() {
        // Worked from the recurrences.
        let mut t = NrLcg::new(TRAFFIC_SEED);
        // 12345 * 1664525 + 1013904223 = 21562081860 = 0x5_0539_1C44
        assert_eq!(12345u64 * 1_664_525 + 1_013_904_223, 0x5_0539_1C44);
        assert_eq!(t.next15(), 0x0539);
        let mut p = AnsiLcg::new(PED_SEED);
        let s = 777u64 * 1_103_515_245 + 12345;
        assert_eq!(p.next15(), ((s as u32) >> 16) & 0x7FFF);
        let mut v = VoiceLcg::default();
        assert_eq!(v.draw(), 1);
        assert_eq!(v.draw(), 0x7A);
        assert_eq!(NrLcg::new(0).seed, 1);
    }

    #[test]
    fn host_and_amiga_agree_on_masked_draws() {
        // The 64-bit host keeps more bits, but bits 16..30 only depend on
        // the low 32, so masked draws agree.
        let mut wide: u64 = u64::from(TRAFFIC_SEED);
        let mut narrow = NrLcg::new(TRAFFIC_SEED);
        for _ in 0..1000 {
            wide = wide.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            assert_eq!(((wide >> 16) & 0x7FFF) as u32, narrow.next15());
        }
    }
}
