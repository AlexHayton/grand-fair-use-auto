//! Full-screen art: the front end (`F_*.RAW` / `.RAT`), the briefing stills
//! (`CUT*.RAW` / `.RAT`) and their palettes (`F_PAL.RAW`, `CUT*.ACT`).
//!
//! Sources: AmiGTA1 `native/gta_front.h` (MIT) for the 640×480 split into an
//! upper and a lower strip. The other sizes were found from the data. Ours.
//!
//! None of these files has a header. Each picture comes twice:
//!
//! - `.RAW`: 24-bit RGB, the master.
//! - `.RAT`: one byte per pixel into a 768-byte 8-bit RGB palette (`F_PAL.RAW`
//!   for the front end, the same-named `.ACT` for a briefing still). It is a
//!   quantised copy: through its palette it comes within a few levels of the
//!   `.RAW`, and in some London files matches it exactly.
//!
//! The size comes from the file name and length; see [`screen_size`].

use crate::reader::FormatError;

const WHAT: &str = "screen image";

pub type Palette = [[u8; 3]; 256];

/// A 768-byte 8-bit RGB palette (`F_PAL.RAW`, `*.ACT`).
pub fn parse_palette(data: &[u8]) -> Result<Palette, FormatError> {
    let (chunks, rest) = data.as_chunks::<3>();
    if chunks.len() != 256 || !rest.is_empty() {
        return Err(FormatError::invalid(
            "palette",
            format!("{} bytes, expected 768", data.len()),
        ));
    }
    Ok(std::array::from_fn(|i| chunks[i]))
}

/// The width and height of a front-end or briefing picture, from its file
/// stem and pixel count. The pictures that differ between the base game and
/// London are told apart by size.
pub fn screen_size(stem: &str, pixels: usize) -> Option<(usize, usize)> {
    let stem = stem.to_ascii_uppercase();
    const SIZES: [(&str, usize, usize); 10] = [
        ("CUT", 640, 480),
        ("F_LOGO", 640, 168),
        ("F_UPPER", 640, 168),
        ("F_LOWER", 640, 312),
        ("F_PLAYN", 180, 50),
        ("F_PLAY", 102, 141),
        ("F_BMG", 100, 50),
        ("F_BMG", 75, 90),
        ("F_DMA", 78, 109),
        // London's is a copyright notice, not the DMA logo.
        ("F_DMA", 225, 38),
    ];
    SIZES
        .iter()
        .find(|(prefix, w, h)| stem.starts_with(prefix) && w * h == pixels)
        .map(|&(_, w, h)| (w, h))
}

/// A picture, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image<P> {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<P>,
}

/// A `.RAT`: palette indices.
pub fn parse_rat(data: &[u8], stem: &str) -> Result<Image<u8>, FormatError> {
    let (width, height) = size_of(stem, data.len())?;
    Ok(Image {
        width,
        height,
        pixels: data.to_vec(),
    })
}

/// A `.RAW`: 24-bit RGB.
pub fn parse_raw(data: &[u8], stem: &str) -> Result<Image<[u8; 3]>, FormatError> {
    let (rgb, rest) = data.as_chunks::<3>();
    if !rest.is_empty() {
        return Err(FormatError::invalid(
            WHAT,
            format!("{stem}: {} bytes is not whole RGB pixels", data.len()),
        ));
    }
    let (width, height) = size_of(stem, rgb.len())?;
    Ok(Image {
        width,
        height,
        pixels: rgb.to_vec(),
    })
}

fn size_of(stem: &str, pixels: usize) -> Result<(usize, usize), FormatError> {
    screen_size(stem, pixels).ok_or_else(|| {
        FormatError::invalid(WHAT, format!("{stem}: no known size for {pixels} pixels"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_come_from_name_and_length() {
        assert_eq!(screen_size("cut3", 640 * 480), Some((640, 480)));
        assert_eq!(screen_size("CUT0E", 640 * 480), Some((640, 480)));
        assert_eq!(screen_size("F_PLAYN", 9000), Some((180, 50)));
        assert_eq!(screen_size("F_PLAY4", 14382), Some((102, 141)));
        assert_eq!(screen_size("F_BMG", 6750), Some((75, 90)));
        assert_eq!(screen_size("F_BMG", 6751), None);
        assert_eq!(screen_size("F_LOGOE3", 107520), Some((640, 168)));
    }

    #[test]
    fn parses_images_and_palettes() {
        let raw = vec![7u8; 9000 * 3];
        let img = parse_raw(&raw, "F_PLAYN").unwrap();
        assert_eq!((img.width, img.height, img.pixels[0]), (180, 50, [7, 7, 7]));
        assert!(parse_raw(&raw[1..], "F_PLAYN").is_err());
        assert_eq!(parse_rat(&[1; 9000], "F_PLAYN").unwrap().pixels.len(), 9000);
        assert_eq!(parse_palette(&[5; 768]).unwrap()[255], [5, 5, 5]);
        assert!(parse_palette(&[5; 767]).is_err());
    }
}
