//! Decoding GTA `.SDT`/`.RAW` sound banks into Kira [`StaticSoundData`].
//!
//! Sources: Carnage3D `src/AudioSampleArchive.cpp` (MIT, © 2019 jericho),
//! AmiGTA1 `native/gta_sfx.h` (MIT), and measurements on the Director's Cut
//! banks (`docs/FORMATS.md`).

use std::sync::Arc;

use gfa_assets::AssetSource;
use gfa_formats::sdt::{SampleFormat, SoundBank};
use kira::Frame;
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};

/// A decoded `.SDT`/`.RAW` sound bank, indexed by the original sample number.
pub struct SfxBank {
    samples: Vec<Option<StaticSoundData>>,
}

impl SfxBank {
    /// Load a bank by its path without extension, e.g.
    /// `"GTADATA/AUDIO/LEVEL000"`.
    pub fn load(src: &dyn AssetSource, path_no_ext: &str) -> Result<Self, String> {
        let sdt_path = format!("{path_no_ext}.SDT");
        let raw_path = format!("{path_no_ext}.RAW");
        let sdt = src.read(&sdt_path).map_err(|e| e.to_string())?;
        let raw = src.read(&raw_path).map_err(|e| e.to_string())?;
        let stem = path_no_ext.rsplit('/').next().unwrap_or(path_no_ext);
        Self::decode(&sdt, &raw, stem)
    }

    /// Decode raw `.SDT` and `.RAW` bytes for a bank named `stem` (e.g.
    /// `"LEVEL000"` or `"LEVEL001"`).
    pub fn decode(sdt: &[u8], raw: &[u8], stem: &str) -> Result<Self, String> {
        let bank = SoundBank::parse(sdt, raw.len()).map_err(|e| e.to_string())?;
        let format = SampleFormat::for_bank(stem);
        let is_frontend = stem.eq_ignore_ascii_case("LEVEL000");

        let mut samples = Vec::with_capacity(bank.entries.len());
        for (i, entry) in bank.entries.iter().enumerate() {
            let Some(bytes) = bank.sample(raw, i) else {
                samples.push(None);
                continue;
            };
            if bytes.is_empty() || entry.rate == 0 {
                samples.push(None);
                continue;
            }
            // In LEVEL000, entries 0..3 are 16-bit signed interleaved stereo
            // (measured on the data and matching Carnage3D); 3.. are 16-bit
            // mono. All other banks are 8-bit unsigned mono.
            let stereo = is_frontend && i < 3;
            let frames = decode_pcm(bytes, format, stereo);
            if frames.is_empty() {
                samples.push(None);
            } else {
                samples.push(Some(StaticSoundData {
                    sample_rate: entry.rate,
                    frames: Arc::from(frames),
                    settings: StaticSoundSettings::default(),
                    slice: None,
                }));
            }
        }
        Ok(Self { samples })
    }

    /// Decoded sound data for sample `index`, if present and non-empty.
    pub fn get(&self, index: usize) -> Option<&StaticSoundData> {
        self.samples.get(index).and_then(Option::as_ref)
    }

    /// Number of entries in the bank.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether the bank has no entries.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

fn decode_pcm(bytes: &[u8], format: SampleFormat, stereo: bool) -> Vec<Frame> {
    match format {
        SampleFormat::U8 => bytes
            .iter()
            .map(|&b| Frame::from_mono((f32::from(b) - 128.0) / 128.0))
            .collect(),
        SampleFormat::I16 if stereo => bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| {
                let l = f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0;
                let r = f32::from(i16::from_le_bytes([c[2], c[3]])) / 32768.0;
                Frame::new(l, r)
            })
            .collect(),
        SampleFormat::I16 => bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                let s = f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0;
                Frame::from_mono(s)
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sdt_bytes(entries: &[(u32, u32, u32)]) -> Vec<u8> {
        entries
            .iter()
            .flat_map(|&(o, l, r)| [o, l, r])
            .flat_map(u32::to_le_bytes)
            .collect()
    }

    #[test]
    fn decodes_u8_mono_level_bank() {
        let raw = vec![0u8, 128, 255];
        let sdt = sdt_bytes(&[(0, 3, 11025)]);
        let bank = SfxBank::decode(&sdt, &raw, "LEVEL001").unwrap();
        let s = bank.get(0).unwrap();
        assert_eq!(s.sample_rate, 11025);
        assert_eq!(s.frames.len(), 3);
        assert!((s.frames[0].left - (-1.0)).abs() < 1e-5);
        assert!(s.frames[1].left.abs() < 1e-5);
        assert!((s.frames[2].left - (127.0 / 128.0)).abs() < 1e-5);
        assert_eq!(s.frames[0].left, s.frames[0].right);
    }

    #[test]
    fn decodes_level000_stereo_first_three_and_mono_rest() {
        let mut raw = Vec::new();
        // Entry 0: 1 stereo frame (L = 16384 -> 0.5, R = -16384 -> -0.5)
        raw.extend_from_slice(&16384i16.to_le_bytes());
        raw.extend_from_slice(&(-16384i16).to_le_bytes());
        // Entries 1 & 2: 1 stereo frame each
        raw.extend_from_slice(&[0; 8]);
        // Entry 3: 2 mono frames
        raw.extend_from_slice(&16384i16.to_le_bytes());
        raw.extend_from_slice(&(-16384i16).to_le_bytes());

        let sdt = sdt_bytes(&[(0, 4, 32000), (4, 4, 32000), (8, 4, 24000), (12, 4, 44100)]);
        let bank = SfxBank::decode(&sdt, &raw, "LEVEL000").unwrap();
        let s0 = bank.get(0).unwrap();
        assert_eq!(s0.frames.len(), 1);
        assert!((s0.frames[0].left - 0.5).abs() < 1e-5);
        assert!((s0.frames[0].right - (-0.5)).abs() < 1e-5);

        let s3 = bank.get(3).unwrap();
        assert_eq!(s3.frames.len(), 2);
        assert!((s3.frames[0].left - 0.5).abs() < 1e-5);
        assert_eq!(s3.frames[0].left, s3.frames[0].right);
        assert!((s3.frames[1].left - (-0.5)).abs() < 1e-5);
    }
}
