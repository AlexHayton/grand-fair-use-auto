//! Helpers shared by the tests that need the user's game data.
#![allow(dead_code)]

use std::path::PathBuf;

use gfa_assets::level::{self, LevelFiles, load_map};
use gfa_assets::{DataPath, Dataset, FsSource};
use gfa_formats::cmp::Cmp;
use gfa_formats::style::CarInfo;
use gfa_sim::replay::level_world;
use gfa_sim::script::Script;
use gfa_sim::view::SimView;
use gfa_sim::world::World;

/// The data folder: GFA_DATA or gfa.local.toml.
pub fn data() -> FsSource {
    match DataPath::resolve(None) {
        Ok(p) => FsSource::new(p.root),
        Err(e) => panic!("this test needs the game data: {e}"),
    }
}

/// The repository's root.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every script in tools/scripts/, sorted.
pub fn scripts() -> Vec<(String, String)> {
    let dir = root().join("tools/scripts");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read_to_string(&p).unwrap())
        })
        .collect();
    out.sort();
    out
}

/// A GTA level: its files, map and car table.
pub struct LevelData {
    pub files: LevelFiles,
    pub map: Cmp,
    pub cars: Vec<CarInfo>,
    pub style_path: String,
}

pub fn level(src: &FsSource, section: u32) -> LevelData {
    let files = level::find(src, Dataset::Gta, section).unwrap();
    let (map, style_path, style) = load_map(src, &files.map, Dataset::Gta).unwrap();
    LevelData {
        files,
        map,
        cars: style.cars,
        style_path,
    }
}

/// The world a script runs in, as `gfa-tool run` builds it. `oracle` runs
/// it as AmiGTA1's harnesses do: their view size, and no parked cars.
pub fn world(lv: &LevelData, script: &Script, oracle: bool) -> World {
    let view = if oracle {
        SimView::AMIGTA1
    } else {
        SimView::GAME
    };
    let mut w = level_world(
        lv.map.clone(),
        lv.cars.clone(),
        &lv.files.mission,
        true,
        view,
        script.start,
    )
    .unwrap();
    if oracle {
        w.parked.clear();
    }
    w
}
