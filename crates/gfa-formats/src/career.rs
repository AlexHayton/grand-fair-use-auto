//! `PLAYER_A.DAT`: settings, high scores and the eight player slots.
//!
//! Source: AmiGTA1 `native/gta_career.c` / `.h` (MIT), whose offsets come
//! from the original's own read and write routines. Implementation ours.
//!
//! The file is a raw dump of 0x3f1 bytes of the DOS game's memory, with no
//! header. Keeping the bytes and reading fields through accessors means a
//! save written here is the original's format, and an unchanged file
//! round-trips byte for byte.
//!
//! | offset | contents |
//! |---|---|
//! | 0x000 | sound volume 0..=7 |
//! | 0x001 | music volume 0..=7 |
//! | 0x002 | text speed 1..=3 |
//! | 0x003 | music mode |
//! | 0x007 | a flag (1 by default) |
//! | 0x009 | `i32` multiplayer target score |
//! | 0x00d | `i32` multiplayer frag limit |
//! | 0x011 | language override (0xff none, 99 SPECIAL.FXT) |
//! | 0x012 | six high-score tables of 0x39: three rows of `i32` score, `name[15]`, lowest first |
//! | 0x168 | eight slots of 0x4f: `name[15]`, `i32 best[6]` (−1 locked), 8 bytes chapter, 8 bytes level, `i32 seen[6]` |
//! | 0x3e0 | the current slot |
//!
//! The six levels are, in order, Liberty City 1 and 2, San Andreas 1 and 2,
//! Vice City 1 and 2. London's `PLAYER_A.DAT` (767 bytes) and `PLAYER_E.DAT`
//! (404 bytes) have other layouts, not read yet.

use crate::reader::FormatError;

pub const CAREER_SIZE: usize = 0x3f1;
pub const SLOTS: usize = 8;
pub const LEVELS: usize = 6;
pub const NAME_LEN: usize = 15;
const OFF_HS: usize = 0x012;
const HS_TABLE: usize = 0x39;
const HS_ROW: usize = 0x13;
const OFF_SLOTS: usize = 0x168;
const SLOT: usize = 0x4f;
const SL_BEST: usize = 0x0f;
const SL_SEEN: usize = 0x37;
const OFF_CURRENT: usize = 0x3e0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Career {
    pub bytes: [u8; CAREER_SIZE],
}

impl Career {
    pub fn parse(data: &[u8]) -> Result<Self, FormatError> {
        let bytes = data.try_into().map_err(|_| {
            FormatError::invalid(
                "PLAYER_A.DAT",
                format!("{} bytes, expected {CAREER_SIZE}", data.len()),
            )
        })?;
        Ok(Self { bytes })
    }

    pub fn sound_volume(&self) -> u8 {
        self.bytes[0]
    }

    pub fn music_volume(&self) -> u8 {
        self.bytes[1]
    }

    pub fn text_speed(&self) -> u8 {
        self.bytes[2]
    }

    pub fn current_slot(&self) -> usize {
        (self.bytes[OFF_CURRENT] as usize).min(SLOTS - 1)
    }

    pub fn name(&self, slot: usize) -> String {
        latin1_name(&self.slot(slot)[..NAME_LEN])
    }

    /// The slot's best score on `level`, or `None` while it is locked.
    pub fn best(&self, slot: usize, level: usize) -> Option<i32> {
        let v = i32_at(self.slot(slot), SL_BEST + 4 * level);
        (v != -1).then_some(v)
    }

    /// Whether the level's briefing has been shown.
    pub fn seen(&self, slot: usize, level: usize) -> bool {
        i32_at(self.slot(slot), SL_SEEN + 4 * level) == 1
    }

    /// Row `row` (0 lowest, 2 highest) of a level's public table.
    pub fn high_score(&self, level: usize, row: usize) -> (i32, String) {
        let at = OFF_HS + level * HS_TABLE + row * HS_ROW;
        let e = &self.bytes[at..at + HS_ROW];
        (i32_at(e, 0), latin1_name(&e[4..4 + NAME_LEN]))
    }

    fn slot(&self, slot: usize) -> &[u8] {
        let at = OFF_SLOTS + slot * SLOT;
        &self.bytes[at..at + SLOT]
    }
}

fn i32_at(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

fn latin1_name(b: &[u8]) -> String {
    b.iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_slots_and_tables() {
        let mut b = [0u8; CAREER_SIZE];
        b[OFF_CURRENT] = 2;
        let slot2 = OFF_SLOTS + 2 * SLOT;
        b[slot2..slot2 + 5].copy_from_slice(b"Katie");
        b[slot2 + SL_BEST..slot2 + SL_BEST + 4].copy_from_slice(&1234i32.to_le_bytes());
        b[slot2 + SL_BEST + 4..slot2 + SL_BEST + 8].copy_from_slice(&(-1i32).to_le_bytes());
        b[slot2 + SL_SEEN..slot2 + SL_SEEN + 4].copy_from_slice(&1i32.to_le_bytes());
        let row = OFF_HS + HS_TABLE + 2 * HS_ROW;
        b[row..row + 4].copy_from_slice(&3i32.to_le_bytes());
        b[row + 4..row + 9].copy_from_slice(b"STEVE");
        let c = Career::parse(&b).unwrap();
        assert_eq!(c.current_slot(), 2);
        assert_eq!(c.name(2), "Katie");
        assert_eq!(c.best(2, 0), Some(1234));
        assert_eq!(c.best(2, 1), None);
        assert!(c.seen(2, 0) && !c.seen(2, 1));
        assert_eq!(c.high_score(1, 2), (3, "STEVE".into()));
        assert!(Career::parse(&b[1..]).is_err());
    }
}
