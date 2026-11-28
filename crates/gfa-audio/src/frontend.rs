//! Front-end sound effects (`GTADATA/AUDIO/LEVEL000`) and title music.
//!
//! Sample indices come from `GTA8.EXE`'s front-end calls to `0x15e70`
//! (`0x4786a..0x4c997`):
//! - 0, 1: city select left / right (16-bit stereo)
//! - 2: confirm / enter (16-bit stereo)
//! - 3: player slot left / right
//! - 4: cancel / escape
//! - 5, 6: music mode Constant / Radio
//! - 7, 8: menu cursor up / down
//! - 9, 10, 11: text speed Slow / Normal / Fast (`8 + speed`)
//! - 12, 13: rename type character / backspace
//! - 14: sound volume test

use gfa_assets::{AssetSource, Dataset};
use gfa_formats::cue;

/// `LEVEL000` sample indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum MenuSfx {
    CityLeft = 0,
    CityRight = 1,
    Confirm = 2,
    PlayerSwitch = 3,
    Cancel = 4,
    MusicConstant = 5,
    MusicRadio = 6,
    NavUp = 7,
    NavDown = 8,
    TextSlow = 9,
    TextNormal = 10,
    TextFast = 11,
    TypeChar = 12,
    Backspace = 13,
    SoundTest = 14,
}

impl MenuSfx {
    /// Sample for text speed setting `1` (Slow), `2` (Normal) or `3` (Fast).
    pub fn for_text_speed(speed: u8) -> Self {
        match speed {
            1 => Self::TextSlow,
            2 => Self::TextNormal,
            _ => Self::TextFast,
        }
    }

    /// Sample for music mode `0` (Radio) or `1` (Constant).
    pub fn for_music_mode(mode: u8) -> Self {
        if mode == 0 {
            Self::MusicRadio
        } else {
            Self::MusicConstant
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// Read the front-end title theme Ogg bytes (CD track 2, `Track02.ogg`).
pub fn load_menu_music(src: &dyn AssetSource, dataset: Dataset) -> Option<Vec<u8>> {
    let dir = dataset.music_dir();
    let file = src
        .read(&format!("{dir}/CDAUDIO.DAT"))
        .ok()
        .and_then(|b| cue::parse(&b).ok())
        .and_then(|tracks| {
            tracks
                .into_iter()
                .find(|t| t.number == 2)
                .map(|t| t.file)
        })
        .unwrap_or_else(|| "Track02.ogg".to_string());
    src.read(&format!("{dir}/{file}")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gfa_assets::MemSource;

    #[test]
    fn loads_track02_from_cue_or_fallback() {
        let mut src = MemSource::new();
        src.insert(
            "MUSIC/GTA/CDAUDIO.DAT",
            b"FILE \"Track02.ogg\" MP3\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n".to_vec(),
        );
        src.insert("MUSIC/GTA/Track02.ogg", b"ogg-bytes".to_vec());
        assert_eq!(
            load_menu_music(&src, Dataset::Gta).as_deref(),
            Some(b"ogg-bytes".as_slice())
        );

        // Without CDAUDIO.DAT it still falls back to Track02.ogg.
        let mut fallback = MemSource::new();
        fallback.insert("MUSIC/GTA/Track02.ogg", b"fallback".to_vec());
        assert_eq!(
            load_menu_music(&fallback, Dataset::Gta).as_deref(),
            Some(b"fallback".as_slice())
        );
    }
}
