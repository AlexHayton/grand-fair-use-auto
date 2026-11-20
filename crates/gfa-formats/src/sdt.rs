//! `.SDT` sound bank indexes, each describing samples in a `.RAW` file of the
//! same name (`AUDIO/LEVEL001`, `VOCALCOM`, `MISBRIEF`, …).
//!
//! Sources: Carnage3D `src/AudioSampleArchive.cpp` (MIT, © 2019 jericho) and
//! AmiGTA1 `native/gta_sfx.h` (MIT). Implementation ours.
//!
//! The index is 12-byte records `{u32 offset, u32 length, u32 rate}` and
//! nothing else. In every Director's Cut bank the records are contiguous and
//! the last ends exactly at the end of the `.RAW`.
//!
//! The index does not say how samples are encoded. Measured on the data:
//! the level banks, `VOCALCOM` and `MISBRIEF` are 8-bit unsigned mono, while
//! `LEVEL000` (the front end) is 16-bit signed. See [`SampleFormat`].

use crate::reader::{FormatError, Reader};

const WHAT: &str = "SDT";
const RECORD: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundEntry {
    pub offset: u32,
    pub length: u32,
    /// Samples per second.
    pub rate: u32,
}

/// How a bank's samples are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    U8,
    I16,
}

impl SampleFormat {
    /// The encoding of the bank with this file stem. `LEVEL000` is the front
    /// end's bank and the only 16-bit one (measured: its high bytes are
    /// smooth and its low bytes noise).
    pub fn for_bank(stem: &str) -> Self {
        if stem.eq_ignore_ascii_case("LEVEL000") {
            Self::I16
        } else {
            Self::U8
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundBank {
    pub entries: Vec<SoundEntry>,
}

impl SoundBank {
    /// Parses an index and checks every entry lies within a `.RAW` of
    /// `raw_len` bytes.
    pub fn parse(sdt: &[u8], raw_len: usize) -> Result<Self, FormatError> {
        if !sdt.len().is_multiple_of(RECORD) {
            return Err(FormatError::invalid(
                WHAT,
                format!(
                    "{} bytes is not a whole number of 12-byte records",
                    sdt.len()
                ),
            ));
        }
        let mut r = Reader::new(sdt, WHAT);
        let mut entries = Vec::with_capacity(sdt.len() / RECORD);
        while !r.is_empty() {
            let e = SoundEntry {
                offset: r.u32()?,
                length: r.u32()?,
                rate: r.u32()?,
            };
            let end = e.offset as usize + e.length as usize;
            if end > raw_len {
                return Err(FormatError::invalid(
                    WHAT,
                    format!(
                        "sample {} ends at {end}, past the {raw_len}-byte RAW",
                        entries.len()
                    ),
                ));
            }
            entries.push(e);
        }
        Ok(Self { entries })
    }

    /// The bytes of sample `i` within the bank's `.RAW`.
    pub fn sample<'a>(&self, raw: &'a [u8], i: usize) -> Option<&'a [u8]> {
        let e = self.entries.get(i)?;
        raw.get(e.offset as usize..e.offset as usize + e.length as usize)
    }

    /// Whether the samples tile the `.RAW` exactly, end to end.
    pub fn is_contiguous(&self, raw_len: usize) -> bool {
        let mut at = 0u64;
        for e in &self.entries {
            if e.offset as u64 != at {
                return false;
            }
            at += e.length as u64;
        }
        at == raw_len as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(entries: &[(u32, u32, u32)]) -> Vec<u8> {
        entries
            .iter()
            .flat_map(|&(o, l, r)| [o, l, r])
            .flat_map(u32::to_le_bytes)
            .collect()
    }

    #[test]
    fn parses_and_checks_bounds() {
        let sdt = index(&[(0, 3, 11025), (3, 2, 22050)]);
        let b = SoundBank::parse(&sdt, 5).unwrap();
        assert_eq!(b.entries[1].rate, 22050);
        assert!(b.is_contiguous(5));
        assert!(!b.is_contiguous(6));
        assert_eq!(b.sample(&[1, 2, 3, 4, 5], 1), Some(&[4, 5][..]));
        assert!(SoundBank::parse(&sdt, 4).is_err());
        assert!(SoundBank::parse(&sdt[..20], 5).is_err());
        assert_eq!(SampleFormat::for_bank("level000"), SampleFormat::I16);
        assert_eq!(SampleFormat::for_bank("LEVEL001"), SampleFormat::U8);
    }
}
