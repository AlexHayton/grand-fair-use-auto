//! Tests against the user's own game data. Run with `cargo data-test`.
//!
//! Every test here is `#[ignore]`d so `cargo test` passes without the data.
//! The data folder comes from `GFA_DATA` or `gfa.local.toml`.

use gfa_assets::{AssetSource, DataPath, Dataset, Datasets, FsSource};

fn data() -> FsSource {
    match DataPath::resolve(None) {
        Ok(p) => FsSource::new(p.root),
        Err(e) => panic!("data tests need the game data: {e}"),
    }
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn finds_the_base_game() {
    let src = data();
    let d = Datasets::detect(&src);
    println!("datasets: {:?}, music: {:?}", d.present, d.music);
    assert!(d.has(Dataset::Gta), "GTADATA/MISSION.INI not found");
    for f in ["GTADATA/NYC.CMP", "GTADATA/SANB.CMP", "GTADATA/MIAMI.CMP"] {
        assert!(src.exists(f), "{f} missing");
    }
}

const MAPS: [&str; 6] = [
    "GTADATA/NYC.CMP",
    "GTADATA/SANB.CMP",
    "GTADATA/MIAMI.CMP",
    "GTADATA/UK/UK.CMP",
    "GTADATA/UK/MC.CMP",
    "GTADATA/UK/UKE.CMP",
];

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_city_map_parses() {
    use gfa_formats::cmp::{Cmp, MAP_DIM, MAP_LAYERS};
    let src = data();
    for path in MAPS {
        let bytes = src.read(path).unwrap_or_else(|e| panic!("{e}"));
        let m = Cmp::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        let mut solid = 0usize;
        for y in 0..MAP_DIM {
            for x in 0..MAP_DIM {
                let h = m.column_height(x, y);
                assert!(h <= MAP_LAYERS);
                solid += (0..h).filter(|&z| m.block(x, y, z).is_some()).count();
            }
        }
        let police_routes = m.routes.iter().filter(|r| r.kind == 0xff).count();
        let named: Vec<&str> = m
            .districts
            .iter()
            .filter(|d| !d.is_empty())
            .map(|d| d.name.as_str())
            .collect();
        println!(
            "{path}: style {} sample {}, {} blocks, {solid} placed, {} objects, {} routes ({police_routes} police), \
             {}/{}/{} police/hospital/fire, {} districts: {}",
            m.style_number,
            m.sample_number,
            m.blocks.len(),
            m.objects.len(),
            m.routes.len(),
            m.police_stations().count(),
            m.hospitals().count(),
            m.fire_stations().count(),
            m.districts.len(),
            named.join(", ")
        );
        assert!(solid > 10_000, "{path}: suspiciously empty");
    }
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn probe_london_map_files() {
    // UK.MAP and MC.MAP have CMP-like headers; find out what they are.
    let src = data();
    for path in ["GTADATA/UK/UK.MAP", "GTADATA/UK/MC.MAP"] {
        let Ok(bytes) = src.read(path) else { continue };
        let words: Vec<u32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .take(7)
            .map(|c| u32::from_le_bytes(*c))
            .collect();
        let parsed = gfa_formats::cmp::Cmp::parse(&bytes).map(|m| m.blocks.len());
        println!(
            "{path}: {} bytes, header words {words:?}, as CMP: {parsed:?}",
            bytes.len()
        );
    }
}
