//! Clock-based CD-audio radio stations (`MUSIC/*/CDAUDIO.DAT`).
//!
//! Each station runs continuously on a shared wall clock so tuning away and
//! returning resumes where the station would be live. In Radio mode (setting 0)
//! music plays only in cars; in Constant mode (setting 1) it plays on foot too.
//! `F5` cycles stations.

use gfa_assets::{AssetSource, Dataset};
use gfa_formats::cue;

/// One CD-audio track available for radio playback.
#[derive(Debug, Clone)]
pub struct Station {
    pub track: u8,
    pub path: String,
    /// Deterministic starting offset (seconds) so stations aren't all synced to 0.
    pub offset_secs: f64,
}

/// Manages the station list, active station, and clock position.
pub struct Radio {
    stations: Vec<Station>,
    /// Index in `stations` of the police band track, if identified.
    police_index: Option<usize>,
    /// Currently playing station index (if any).
    active: Option<usize>,
    /// Last music station index chosen by the player (for `F5` cycling).
    current_music: usize,
    /// Elapsed wall-clock seconds since the level started.
    elapsed: f64,
    /// `0` = Radio (in cars only), `1` = Constant (always on).
    pub music_mode: u8,
}

impl Default for Radio {
    fn default() -> Self {
        Self::new()
    }
}

impl Radio {
    pub fn new() -> Self {
        Self {
            stations: Vec::new(),
            police_index: None,
            active: None,
            current_music: 0,
            elapsed: 0.0,
            music_mode: 0,
        }
    }

    /// Scan `MUSIC/*/CDAUDIO.DAT` for audio tracks (`number >= 2`).
    pub fn load(&mut self, src: &dyn AssetSource, dataset: Dataset) {
        self.stations.clear();
        self.active = None;
        self.police_index = None;
        let dir = dataset.music_dir();
        let Ok(bytes) = src.read(&format!("{dir}/CDAUDIO.DAT")) else {
            return;
        };
        let Ok(tracks) = cue::parse(&bytes) else {
            return;
        };
        let mut seen = Vec::new();
        for t in tracks {
            if t.number < 2 || seen.contains(&t.number) {
                continue;
            }
            let path = format!("{dir}/{}", t.file);
            if !src.exists(&path) {
                continue;
            }
            seen.push(t.number);
            let idx = self.stations.len();
            // Spread stations out across different starting offsets.
            let offset_secs = (idx as f64) * 37.0;
            // In GTA 1's CD layout, track 3 (Track03.ogg) is the police radio band.
            if dataset == Dataset::Gta && t.number == 3 {
                self.police_index = Some(idx);
            }
            self.stations.push(Station {
                track: t.number,
                path,
                offset_secs,
            });
        }
        // Start on the first non-police music station.
        self.current_music = self.next_music_index(self.stations.len().saturating_sub(1));
        log::info!(
            "radio: {} stations in {dir} (police={:?})",
            self.stations.len(),
            self.police_index.map(|i| self.stations[i].track)
        );
    }

    pub fn advance(&mut self, dt_secs: f64) {
        self.elapsed += dt_secs.max(0.0);
    }

    pub fn active(&self) -> Option<usize> {
        self.active
    }

    pub fn is_empty(&self) -> bool {
        self.stations.is_empty()
    }

    fn next_music_index(&self, from: usize) -> usize {
        if self.stations.is_empty() {
            return 0;
        }
        let n = self.stations.len();
        for step in 1..=n {
            let i = (from + step) % n;
            if Some(i) != self.police_index || n == 1 {
                return i;
            }
        }
        0
    }

    /// Pick the station for a car's `radio` byte (or police band for cop cars).
    pub fn station_for_car(&mut self, car_radio: u8, is_police: bool) -> Option<(&Station, f64)> {
        if self.stations.is_empty() {
            return None;
        }
        let idx = if is_police && let Some(p) = self.police_index {
            p
        } else {
            let music_only: Vec<usize> = (0..self.stations.len())
                .filter(|&i| Some(i) != self.police_index)
                .collect();
            if music_only.is_empty() {
                0
            } else {
                let pick = music_only[usize::from(car_radio) % music_only.len()];
                self.current_music = pick;
                pick
            }
        };
        self.tune(idx)
    }

    /// Cycle to the next music station (`F5`).
    pub fn next_station(&mut self) -> Option<(&Station, f64)> {
        if self.stations.is_empty() {
            return None;
        }
        let next = self.next_music_index(self.current_music);
        self.current_music = next;
        self.tune(next)
    }

    /// Start or resume the current music station (for Constant mode).
    pub fn ensure_constant(&mut self) -> Option<(&Station, f64)> {
        if self.stations.is_empty() || self.active.is_some() {
            return None;
        }
        self.tune(self.current_music)
    }

    /// Clear active station (when leaving a car in Radio mode).
    pub fn stop(&mut self) {
        self.active = None;
    }

    fn tune(&mut self, idx: usize) -> Option<(&Station, f64)> {
        let s = self.stations.get(idx)?;
        self.active = Some(idx);
        Some((s, s.offset_secs + self.elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gfa_assets::MemSource;

    #[test]
    fn cycles_music_stations_and_reserves_track03_for_police() {
        let mut src = MemSource::new();
        src.insert(
            "MUSIC/GTA/CDAUDIO.DAT",
            b"FILE \"Track02.ogg\" MP3\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n\
              FILE \"Track03.ogg\" MP3\n  TRACK 03 AUDIO\n    INDEX 01 00:00:00\n\
              FILE \"Track04.ogg\" MP3\n  TRACK 04 AUDIO\n    INDEX 01 00:00:00\n"
                .to_vec(),
        );
        src.insert("MUSIC/GTA/Track02.ogg", vec![1]);
        src.insert("MUSIC/GTA/Track03.ogg", vec![2]);
        src.insert("MUSIC/GTA/Track04.ogg", vec![3]);

        let mut r = Radio::new();
        r.load(&src, Dataset::Gta);
        let (cop, _) = r.station_for_car(0, true).unwrap();
        assert_eq!(cop.track, 3);
        let (m1, _) = r.station_for_car(0, false).unwrap();
        assert_eq!(m1.track, 2);
        let (m2, _) = r.next_station().unwrap();
        assert_eq!(m2.track, 4);
        let (m3, _) = r.next_station().unwrap();
        assert_eq!(m3.track, 2);
    }
}
