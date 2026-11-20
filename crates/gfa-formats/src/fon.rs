//! `.FON` bitmap fonts (`PAGER1.FON`, `BIG1.FON`, `SCORE1.FON`, …).
//!
//! Sources: Carnage3D `src/Font.cpp` (MIT, © 2019 jericho), including the
//! accented-letter table, and AmiGTA1 `native/gta_font.c` (MIT).
//! Implementation ours.
//!
//! Layout: `u8` glyph count, `u8` height, then per glyph a `u8` width and
//! `width × height` palette indices, then a 768-byte 8-bit RGB palette of the
//! font's own. Index 0 is transparent. The first glyph is `!` (0x21).
//!
//! Glyph `n` is character `0x21 + n` up to `~` and DEL (glyphs 0..=94).
//! Accented Windows-1252 letters from 0xC0 follow at glyph 95 through
//! [`ACCENTS`]. Some fonts are shorter: the cutscene fonts hold a handful of
//! pictures, and the score fonts only digits.

use crate::reader::{FormatError, Reader};

pub const FIRST_CHAR: u8 = b'!';
/// Where the accented letters start.
pub const ACCENT_BASE: usize = (b'z' - FIRST_CHAR) as usize + 6;
const WHAT: &str = "FON";

/// Accented glyph for each Windows-1252 byte from 0xC0, or `None` where the
/// fonts have no glyph (Ã, Å, Ð, Ñ, Õ, ×, Ø, Ý, Þ and lower-case kin).
/// Carnage3D's `AnsiCharsOffsetTable`, with its "no glyph" zeros made
/// explicit.
pub const ACCENTS: [Option<u8>; 61] = {
    const N: Option<u8> = None;
    [
        Some(0),
        Some(1),
        Some(2),
        N,
        Some(3),
        N,
        Some(4),
        Some(5), // À Á Â Ã Ä Å Æ Ç
        Some(6),
        Some(7),
        Some(8),
        Some(9),
        Some(10),
        Some(11),
        Some(12),
        Some(13), // È É Ê Ë Ì Í Î Ï
        N,
        N,
        Some(14),
        Some(15),
        Some(16),
        N,
        Some(17),
        N, // Ð Ñ Ò Ó Ô Õ Ö ×
        N,
        Some(18),
        Some(19),
        Some(20),
        Some(21),
        N,
        N,
        Some(22), // Ø Ù Ú Û Ü Ý Þ ß
        Some(23),
        Some(24),
        Some(25),
        N,
        Some(26),
        N,
        Some(27),
        Some(28), // à á â ã ä å æ ç
        Some(29),
        Some(30),
        Some(31),
        Some(32),
        Some(33),
        Some(34),
        Some(35),
        Some(36), // è é ê ë ì í î ï
        N,
        N,
        Some(37),
        Some(38),
        Some(39),
        N,
        Some(40),
        N, // ð ñ ò ó ô õ ö ÷
        N,
        Some(41),
        Some(42),
        Some(43),
        Some(44), // ø ù ú û ü
    ]
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    pub width: u8,
    /// `width × height` palette indices, row by row.
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    pub height: u8,
    pub glyphs: Vec<Glyph>,
    /// The font's own palette, 8-bit RGB.
    pub palette: [[u8; 3]; 256],
}

impl Font {
    pub fn parse(data: &[u8]) -> Result<Self, FormatError> {
        let mut r = Reader::new(data, WHAT);
        let count = r.u8()?;
        let height = r.u8()?;
        let glyphs = (0..count)
            .map(|_| {
                let width = r.u8()?;
                let pixels = r.bytes(width as usize * height as usize)?.to_vec();
                Ok(Glyph { width, pixels })
            })
            .collect::<Result<_, FormatError>>()?;
        let mut palette = [[0u8; 3]; 256];
        for c in &mut palette {
            *c = r.array()?;
        }
        r.expect_end()?;
        Ok(Self {
            height,
            glyphs,
            palette,
        })
    }

    /// The glyph number for a Windows-1252 byte, if the font has one.
    pub fn glyph_index(&self, ch: u8) -> Option<usize> {
        let i = match ch {
            0xC0.. => ACCENT_BASE + *ACCENTS.get((ch - 0xC0) as usize)?.as_ref()? as usize,
            _ => ch.checked_sub(FIRST_CHAR)? as usize,
        };
        (i < self.glyphs.len()).then_some(i)
    }

    pub fn glyph(&self, ch: u8) -> Option<&Glyph> {
        self.glyphs.get(self.glyph_index(ch)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_glyphs_and_maps_characters() {
        let mut data = vec![2u8, 3];
        data.extend_from_slice(&[1, 5, 6, 7]); // '!' is 1 wide
        data.extend_from_slice(&[2, 0, 1, 0, 1, 0, 1]); // '"' is 2 wide
        data.extend(std::iter::repeat_n(9u8, 768));
        let f = Font::parse(&data).unwrap();
        assert_eq!(f.height, 3);
        assert_eq!(f.glyph(b'!').unwrap().pixels, vec![5, 6, 7]);
        assert_eq!(f.glyph(b'"').unwrap().width, 2);
        assert_eq!(f.glyph(b' '), None);
        assert_eq!(f.glyph(b'#'), None);
        assert_eq!(f.palette[255], [9, 9, 9]);
        data.push(0);
        assert!(Font::parse(&data).is_err(), "trailing bytes");
    }

    #[test]
    fn accents_sit_after_the_ascii_range() {
        assert_eq!(ACCENT_BASE, 95);
        let n = ACCENTS.iter().flatten().count();
        assert_eq!(n, 45, "45 accented glyphs, numbered 0..=44");
        assert_eq!(ACCENTS[0xE9 - 0xC0], Some(30)); // é
        assert_eq!(ACCENTS[0xD1 - 0xC0], None); // Ñ
    }
}
