//! `MUSIC/*/CDAUDIO.DAT`: the re-release's CUE sheet that maps the original
//! CD's audio tracks to `TrackNN.ogg` files.
//!
//! It is plain text, one `FILE "name" TYPE` / `TRACK nn AUDIO` / `INDEX 01
//! mm:ss:ff` group per track. CD track 1 was the data track; the sheet maps
//! it to `Track02.ogg` as a stand-in, so the radio's music starts at track 2.

use crate::reader::FormatError;

const WHAT: &str = "CUE sheet";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueTrack {
    pub number: u8,
    pub file: String,
    /// Start within the file, in CD frames (75 per second).
    pub start_frames: u32,
}

/// Parses the tracks of a CUE sheet.
pub fn parse(data: &[u8]) -> Result<Vec<CueTrack>, FormatError> {
    let text = std::str::from_utf8(data).map_err(|e| FormatError::invalid(WHAT, e))?;
    let mut tracks = Vec::new();
    let mut file: Option<String> = None;
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        let bad =
            |what: &str| FormatError::invalid(WHAT, format!("line {}: {what}: {line}", n + 1));
        let mut words = line.split_whitespace();
        match words.next() {
            Some("FILE") => {
                let open = line
                    .find('"')
                    .ok_or_else(|| bad("FILE without a quoted name"))?;
                let close = line[open + 1..]
                    .find('"')
                    .ok_or_else(|| bad("unclosed FILE name"))?;
                file = Some(line[open + 1..open + 1 + close].to_string());
            }
            Some("TRACK") => {
                let number = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .ok_or_else(|| bad("bad TRACK number"))?;
                let file = file.clone().ok_or_else(|| bad("TRACK before any FILE"))?;
                tracks.push(CueTrack {
                    number,
                    file,
                    start_frames: 0,
                });
            }
            Some("INDEX") => {
                let (Some(index), Some(time)) = (words.next(), words.next()) else {
                    return Err(bad("short INDEX"));
                };
                if index == "01" {
                    let t = tracks
                        .last_mut()
                        .ok_or_else(|| bad("INDEX before any TRACK"))?;
                    t.start_frames = msf(time).ok_or_else(|| bad("bad INDEX time"))?;
                }
            }
            _ => {}
        }
    }
    Ok(tracks)
}

fn msf(t: &str) -> Option<u32> {
    let mut parts = t.split(':').map(|p| p.parse::<u32>().ok());
    let (m, s, f) = (parts.next()??, parts.next()??, parts.next()??);
    Some((m * 60 + s) * 75 + f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tracks() {
        let sheet = b"FILE \"Track02.ogg\" MP3\r\n  TRACK 01 AUDIO\r\n    INDEX 01 00:00:00\r\n\
                      FILE \"Track03.ogg\" MP3\r\n  TRACK 03 AUDIO\r\n    INDEX 01 00:02:10\r\n";
        let t = parse(sheet).unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].number, t[0].file.as_str()), (1, "Track02.ogg"));
        assert_eq!(t[1].start_frames, 2 * 75 + 10);
        assert!(parse(b"TRACK 01 AUDIO").is_err());
    }
}
