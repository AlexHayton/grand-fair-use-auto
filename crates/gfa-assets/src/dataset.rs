//! Which games are present in the data folder.
//!
//! Detection follows Carnage3D's `CarnageGame::DetectGameVersion`: London
//! 1969 is marked by `MISSUK.INI`, London 1961 by `missuke.ini`. Here the
//! London files sit in `GTADATA/UK/` beside the base game, as the
//! Director's Cut installs them.

use std::fmt;

use crate::source::AssetSource;

/// One playable game in the data folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dataset {
    /// Grand Theft Auto (1997): Liberty City, San Andreas, Vice City.
    Gta,
    /// Mission Pack #1: London 1969.
    London69,
    /// Mission Pack #2: London 1961.
    London61,
}

impl Dataset {
    pub const ALL: [Dataset; 3] = [Dataset::Gta, Dataset::London69, Dataset::London61];

    /// The folder holding this game's maps, styles and text.
    pub fn base_dir(self) -> &'static str {
        match self {
            Dataset::Gta => "GTADATA",
            Dataset::London69 | Dataset::London61 => "GTADATA/UK",
        }
    }

    /// The mission script, which also names each level's map and style.
    pub fn mission_ini(self) -> &'static str {
        match self {
            Dataset::Gta => "GTADATA/MISSION.INI",
            Dataset::London69 => "GTADATA/UK/MISSUK.INI",
            Dataset::London61 => "GTADATA/UK/MISSUKE.INI",
        }
    }

    /// The CD-audio radio tracks, as Ogg files with a `CDAUDIO.DAT` cue sheet.
    pub fn music_dir(self) -> &'static str {
        match self {
            Dataset::Gta => "MUSIC/GTA",
            Dataset::London69 | Dataset::London61 => "MUSIC/LONDON",
        }
    }
}

impl fmt::Display for Dataset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Dataset::Gta => "GTA",
            Dataset::London69 => "London 1969",
            Dataset::London61 => "London 1961",
        })
    }
}

/// The games found in a data folder, and whether their music is there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Datasets {
    pub present: Vec<Dataset>,
    pub music: Vec<Dataset>,
}

impl Datasets {
    pub fn detect(src: &dyn AssetSource) -> Self {
        let mut d = Datasets::default();
        for ds in Dataset::ALL {
            if src.exists(ds.mission_ini()) {
                d.present.push(ds);
                if src.list(ds.music_dir()).is_ok_and(|l| !l.is_empty()) {
                    d.music.push(ds);
                }
            }
        }
        d
    }

    pub fn has(&self, ds: Dataset) -> bool {
        self.present.contains(&ds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemSource;

    #[test]
    fn detects_each_game_by_its_mission_script() {
        let mut m = MemSource::new();
        m.insert("GTADATA/mission.ini", vec![]);
        m.insert("GTADATA/UK/missuke.ini", vec![]);
        m.insert("MUSIC/GTA/Track02.ogg", vec![0]);
        let d = Datasets::detect(&m);
        assert_eq!(d.present, [Dataset::Gta, Dataset::London61]);
        assert_eq!(d.music, [Dataset::Gta]);
        assert!(!d.has(Dataset::London69));
    }
}
