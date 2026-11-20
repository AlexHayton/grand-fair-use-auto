//! `.FXT` text tables (`ENGLISH.FXT`, `ENGUK.FXT`, …).
//!
//! Sources: the cipher as documented at gta.mendelsohn.de and read by
//! Carnage3D `src/GameTextsManager.cpp` (MIT, © 2019 jericho) and AmiGTA1
//! `native/gta_text.c` (MIT). Implementation ours.
//!
//! The file is a run of `[key]text\0` records under a light cipher. The first
//! byte picks the variant: `0xBF` (every file in the Director's Cut) means a
//! key of `0x63`, offset −1; `0xA6` means a key of `0x67`, offset +28. The key
//! is subtracted from the first eight bytes, doubling each time; the offset is
//! added to every byte. A byte that decodes to 195 is dropped and 64 is added
//! to the next, which moves accented letters into Windows-1252.
//!
//! Records can be followed by extra NULs. An empty key (`[]`) ends the table;
//! anything after it is ignored. Keys repeat in some files (`2500`, `2501`…);
//! the first record wins, as in AmiGTA1.

use std::collections::HashMap;

use crate::reader::FormatError;

const WHAT: &str = "FXT";
const ESCAPE: u8 = 195;

/// One `[key]text` record. The text is kept as Windows-1252 bytes, which is
/// what the fonts index; see [`cp1252_to_string`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxtEntry {
    pub key: String,
    pub text: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Fxt {
    /// Records in file order, duplicates included.
    pub entries: Vec<FxtEntry>,
    /// Decoded bytes after the end marker (often `\x1a\0[]`).
    pub trailing: usize,
    index: HashMap<String, usize>,
}

impl Fxt {
    pub fn parse(data: &[u8]) -> Result<Self, FormatError> {
        let d = decipher(data)?;
        let mut entries = Vec::new();
        let mut pos = 0;
        let trailing = loop {
            while d.get(pos) == Some(&0) {
                pos += 1;
            }
            match d.get(pos) {
                Some(b'[') => pos += 1,
                Some(&b) => {
                    return Err(FormatError::invalid(
                        WHAT,
                        format!("byte {b:#04x} at {pos} where a [key] should start"),
                    ));
                }
                None => return Err(FormatError::invalid(WHAT, "no [] end marker")),
            }
            let close = d[pos..]
                .iter()
                .position(|&b| b == b']' || b == 0)
                .filter(|&n| d[pos + n] == b']')
                .ok_or_else(|| FormatError::invalid(WHAT, format!("unclosed key at {pos}")))?;
            let key: String = d[pos..pos + close].iter().map(|&b| b as char).collect();
            pos += close + 1;
            if key.is_empty() {
                break d.len() - pos;
            }
            let end = d[pos..].iter().position(|&b| b == 0).ok_or_else(|| {
                FormatError::invalid(WHAT, format!("text of [{key}] runs off the end"))
            })?;
            entries.push(FxtEntry {
                key,
                text: d[pos..pos + end].to_vec(),
            });
            pos += end + 1;
        };
        let mut index = HashMap::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            index.entry(e.key.clone()).or_insert(i);
        }
        Ok(Self {
            entries,
            trailing,
            index,
        })
    }

    /// The text for `key` (the first record with it).
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.index
            .get(key)
            .map(|&i| self.entries[i].text.as_slice())
    }

    /// The text for `key` as a Rust string.
    pub fn string(&self, key: &str) -> Option<String> {
        self.get(key).map(cp1252_to_string)
    }

    /// Distinct keys.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

/// Undoes the cipher, returning the plain byte stream.
pub fn decipher(data: &[u8]) -> Result<Vec<u8>, FormatError> {
    let (mut key, offset): (u8, u8) = match data.first() {
        Some(0xBF) => (0x63, 0u8.wrapping_sub(1)),
        Some(0xA6) => (0x67, 28),
        Some(&b) => {
            return Err(FormatError::invalid(
                WHAT,
                format!("first byte {b:#04x} is not a known cipher"),
            ));
        }
        None => return Err(FormatError::invalid(WHAT, "empty file")),
    };
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let mut c = data[i];
        if i < 8 {
            c = c.wrapping_sub(key);
            key = key.wrapping_shl(1);
        }
        if c.wrapping_add(offset) == ESCAPE && i + 1 < data.len() {
            i += 1;
            c = data[i].wrapping_add(64);
        }
        out.push(c.wrapping_add(offset));
        i += 1;
    }
    Ok(out)
}

/// Windows-1252 to Unicode. The 0x80–0x9F block differs from Latin-1.
pub fn cp1252_to_string(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž',
        '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}',
        'ž', 'Ÿ',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9f => HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies the 0xBF cipher: the inverse of [`decipher`]. Bytes from 0xC0
    /// up are escaped, as the original files do for accented letters.
    fn encipher(plain: &[u8]) -> Vec<u8> {
        let mut raw = Vec::new();
        for &p in plain {
            if p >= 0xC0 {
                raw.push(ESCAPE.wrapping_add(1));
                raw.push(p.wrapping_sub(64).wrapping_add(1));
            } else {
                raw.push(p.wrapping_add(1));
            }
        }
        let mut key: u8 = 0x63;
        for b in raw.iter_mut().take(8) {
            *b = b.wrapping_add(key);
            key = key.wrapping_shl(1);
        }
        raw
    }

    #[test]
    fn parses_records_padding_duplicates_and_the_end_marker() {
        // A first byte of 0xBF needs a plaintext first byte of '['.
        let plain = b"[1001]Hello\0\0[2500]First\0[2500]Second\0[]\x1a\0[]";
        let data = encipher(plain);
        assert_eq!(data[0], 0xBF);
        let f = Fxt::parse(&data).unwrap();
        assert_eq!(f.entries.len(), 3);
        assert_eq!(f.len(), 2);
        assert_eq!(f.get("1001"), Some(&b"Hello"[..]));
        assert_eq!(f.get("2500"), Some(&b"First"[..]), "the first record wins");
        assert_eq!(f.trailing, 4);
    }

    #[test]
    fn the_escape_byte_moves_the_next_into_the_high_half() {
        let f = Fxt::parse(&encipher(b"[1001]Un caf\xe9 \x9c\0[]")).unwrap();
        assert_eq!(f.get("1001"), Some(&b"Un caf\xe9 \x9c"[..]));
        assert_eq!(f.string("1001").unwrap(), "Un café œ");
    }

    #[test]
    fn rejects_unknown_ciphers_and_missing_end_markers() {
        assert!(Fxt::parse(&[0x00, 1, 2]).is_err());
        assert!(Fxt::parse(&encipher(b"[1]abc\0")).is_err());
    }
}
