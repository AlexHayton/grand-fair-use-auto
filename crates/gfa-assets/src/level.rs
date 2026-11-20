//! Which files make up a level, and loading them.
//!
//! A level is a section of a mission file. Its title line names the map;
//! the map's header names the style by number, which lives beside the map.
//! The file name depends on the game (Carnage3D `GetStyleFileName`):
//! `STYLE00n` for GTA and London 1969, `STY00n` for London 1961, whose
//! folder holds both. G24 is preferred to GRY (decision log, M1).

use gfa_formats::cmp::Cmp;
use gfa_formats::mission::{self, Section};
use gfa_formats::style::Style;

use crate::dataset::Dataset;
use crate::source::{AssetError, AssetSource};

#[derive(Debug, thiserror::Error)]
pub enum LevelError {
    #[error(transparent)]
    Asset(#[from] AssetError),
    #[error("{path}: {source}")]
    Format {
        path: String,
        #[source]
        source: gfa_formats::FormatError,
    },
    #[error("{ini} has no section [{section}]")]
    NoSection { ini: &'static str, section: u32 },
    #[error("section [{0}] names no .cmp map")]
    NoMap(u32),
    #[error("no style {number} beside {map} (looked for .G24 and .GRY)")]
    NoStyle { map: String, number: u8 },
}

/// A level's files, as game paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelFiles {
    pub dataset: Dataset,
    pub section: u32,
    pub name: String,
    pub map: String,
    /// The section itself: the player's start, the declarations and the
    /// mission logic.
    pub mission: Section,
}

/// A level's map and style, parsed.
pub struct Level {
    pub files: LevelFiles,
    pub style_path: String,
    pub map: Cmp,
    pub style: Style,
}

fn parse<T>(path: &str, r: Result<T, gfa_formats::FormatError>) -> Result<T, LevelError> {
    r.map_err(|source| LevelError::Format {
        path: path.to_string(),
        source,
    })
}

/// The sections of a dataset's mission file.
pub fn sections(src: &dyn AssetSource, dataset: Dataset) -> Result<Vec<Section>, LevelError> {
    let ini = dataset.mission_ini();
    parse(ini, mission::parse(&src.read(ini)?))
}

/// The files of section `section`.
pub fn find(
    src: &dyn AssetSource,
    dataset: Dataset,
    section: u32,
) -> Result<LevelFiles, LevelError> {
    let all = sections(src, dataset)?;
    let s = all
        .iter()
        .find(|s| s.id == section)
        .ok_or(LevelError::NoSection {
            ini: dataset.mission_ini(),
            section,
        })?;
    let map = s.map().ok_or(LevelError::NoMap(section))?;
    Ok(LevelFiles {
        dataset,
        section,
        name: s.name().to_string(),
        // Mission files name maps relative to GTADATA/, London's included
        // (`uk\uk.cmp`).
        map: format!("GTADATA/{map}"),
        mission: s.clone(),
    })
}

/// The style a map asks for, beside it: G24 first, then GRY.
pub fn style_path(
    src: &dyn AssetSource,
    map_path: &str,
    number: u8,
    dataset: Dataset,
) -> Option<String> {
    let dir = map_path.rsplit_once('/').map_or("", |(d, _)| d);
    let stems: &[&str] = match dataset {
        Dataset::London61 => &["STY", "STYLE"],
        Dataset::Gta | Dataset::London69 => &["STYLE"],
    };
    stems
        .iter()
        .flat_map(|stem| ["G24", "GRY"].map(|ext| format!("{dir}/{stem}{number:03}.{ext}")))
        .find(|p| src.exists(p))
}

/// Which game plays a map: the first whose mission file names it, London
/// 1961 first (it shares the UK folder with 1969). GTA if none does.
pub fn dataset_for_map(src: &dyn AssetSource, map_path: &str) -> Dataset {
    let file = map_path.rsplit('/').next().unwrap_or(map_path);
    [Dataset::London61, Dataset::London69, Dataset::Gta]
        .into_iter()
        .find(|&d| {
            sections(src, d).is_ok_and(|ss| {
                ss.iter().any(|s| {
                    s.map().is_some_and(|m| {
                        m.rsplit('/')
                            .next()
                            .unwrap_or(&m)
                            .eq_ignore_ascii_case(file)
                    })
                })
            })
        })
        .unwrap_or(Dataset::Gta)
}

/// Load a map and the style `dataset` uses with it.
pub fn load_map(
    src: &dyn AssetSource,
    map_path: &str,
    dataset: Dataset,
) -> Result<(Cmp, String, Style), LevelError> {
    let map = parse(map_path, Cmp::parse(&src.read(map_path)?))?;
    let style_path =
        style_path(src, map_path, map.style_number, dataset).ok_or(LevelError::NoStyle {
            map: map_path.to_string(),
            number: map.style_number,
        })?;
    let style = parse(&style_path, Style::parse(&src.read(&style_path)?))?;
    Ok((map, style_path, style))
}

impl Level {
    pub fn load(src: &dyn AssetSource, files: LevelFiles) -> Result<Level, LevelError> {
        let (map, style_path, style) = load_map(src, &files.map, files.dataset)?;
        Ok(Level {
            files,
            style_path,
            map,
            style,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemSource;

    #[test]
    fn finds_the_map_and_prefers_g24() {
        let mut m = MemSource::new();
        m.insert(
            "GTADATA/UK/MISSUK.INI",
            b"[2]\nEasy London NEW, 22, uk\\uk.cmp, 0,\n1 2\n-1\n-1\n".to_vec(),
        );
        m.insert(
            "GTADATA/UK/missuke.ini",
            b"[1]\nLondon 61, 22, uk\\uke.cmp, 0,\n1 2\n-1\n-1\n".to_vec(),
        );
        m.insert("GTADATA/UK/Style001.gry", vec![]);
        m.insert("GTADATA/UK/Style001.g24", vec![]);
        m.insert("GTADATA/UK/Sty001.g24", vec![]);
        let f = find(&m, Dataset::London69, 2).unwrap();
        assert_eq!(f.map, "GTADATA/uk/uk.cmp");
        assert_eq!(f.name, "Easy London NEW");
        assert_eq!(
            style_path(&m, &f.map, 1, Dataset::London69).as_deref(),
            Some("GTADATA/uk/STYLE001.G24")
        );
        assert_eq!(
            style_path(&m, "GTADATA/UK/UKE.CMP", 1, Dataset::London61).as_deref(),
            Some("GTADATA/UK/STY001.G24")
        );
        assert!(style_path(&m, &f.map, 2, Dataset::London69).is_none());
        assert_eq!(dataset_for_map(&m, "GTADATA/UK/UKE.CMP"), Dataset::London61);
        assert_eq!(dataset_for_map(&m, "GTADATA/UK/UK.CMP"), Dataset::London69);
        assert_eq!(dataset_for_map(&m, "GTADATA/NYC.CMP"), Dataset::Gta);
        assert!(matches!(
            find(&m, Dataset::London69, 9),
            Err(LevelError::NoSection { .. })
        ));
    }
}
