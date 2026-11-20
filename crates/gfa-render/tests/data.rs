//! Render checks against the user's own game data. Run with `cargo data-test`.
//! No GPU needed: these check what the renderer would upload.

use gfa_assets::level::{dataset_for_map, load_map};
use gfa_assets::{DataPath, FsSource};
use gfa_render::ASSET_TEXTURE_MAX;
use gfa_render::city_mesh::{CityMesh, TileBanks};
use gfa_render::sprites::{SpriteAtlas, SpriteBatch, push_startup_objects};
use gfa_render::style_tables::{PALETTES_PER_ROW, StyleTables};

const MAPS: [&str; 6] = [
    "GTADATA/NYC.CMP",
    "GTADATA/SANB.CMP",
    "GTADATA/MIAMI.CMP",
    "GTADATA/UK/UK.CMP",
    "GTADATA/UK/MC.CMP",
    "GTADATA/UK/UKE.CMP",
];

fn data() -> FsSource {
    match DataPath::resolve(None) {
        Ok(p) => FsSource::new(p.root),
        Err(e) => panic!("data tests need the game data: {e}"),
    }
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_map_builds_and_fits_webgl2() {
    let src = data();
    let max = ASSET_TEXTURE_MAX as usize;
    for path in MAPS {
        let dataset = dataset_for_map(&src, path);
        let (map, style_path, style) =
            load_map(&src, path, dataset).unwrap_or_else(|e| panic!("{path}: {e}"));
        let mesh = CityMesh::build(
            &map,
            TileBanks {
                side: style.side_tiles,
                lid: style.lid_tiles,
            },
        );
        let s = mesh.stats;
        println!(
            "{path} ({dataset}, {style_path}): {} faces, {} flat, {} sloped, {} vertices",
            s.faces,
            s.flat_blocks,
            s.sloped_blocks,
            mesh.vertices.len()
        );
        assert_eq!(s.bad_tiles, 0, "{path}: faces name tiles the style lacks");
        assert_eq!(s.degenerate, 0, "{path}: slope faces with no height");
        // Doors are virtual tiles set by the mission script (M7); none of
        // the maps uses one on its own.
        assert_eq!(s.door_faces, 0, "{path}: door tiles in the bare map");

        let tables = StyleTables::new(&style);
        assert!(tables.atlas_height <= max, "{path}: tile atlas too tall");
        assert!(
            tables.palettes.len().div_ceil(PALETTES_PER_ROW) <= max,
            "{path}: too many palettes"
        );
        let atlas = SpriteAtlas::new(&style);
        assert!(atlas.height <= max, "{path}: sprite atlas too tall");
        let mut batch = SpriteBatch::default();
        let skipped = push_startup_objects(&map, &style, &atlas, &mut batch);
        assert_eq!(skipped, 0, "{path}: startup objects with no sprite");
        assert_eq!(batch.len(), map.objects.len());
    }
}
