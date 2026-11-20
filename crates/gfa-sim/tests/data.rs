//! Simulation checks against the user's own game data. Run with
//! `cargo data-test`.

use gfa_assets::level::{self, load_map};
use gfa_assets::{AssetSource, DataPath, Dataset, FsSource};
use gfa_sim::input::{Buttons, TickInput};
use gfa_sim::player::Anim;
use gfa_sim::replay::level_world;
use gfa_sim::view::SimView;
use gfa_sim::world::World;

fn data() -> FsSource {
    match DataPath::resolve(None) {
        Ok(p) => FsSource::new(p.root),
        Err(e) => panic!("data tests need the game data: {e}"),
    }
}

/// Run ahead, turning right for a third of every 120 ticks, and report the
/// distance covered (world pixels) and the final hash.
fn wander(world: &mut World, ticks: u32) -> (i32, u64) {
    let mut travelled = 0i64;
    for t in 0..ticks {
        let turn = if (t / 40) % 3 == 2 {
            Buttons::RIGHT
        } else {
            Buttons::NONE
        };
        let (x, y) = (world.player.x, world.player.y);
        world.step(TickInput::new(Buttons(Buttons::UP.0 | turn.0)));
        travelled += i64::from((world.player.x - x).abs() + (world.player.y - y).abs());
    }
    ((travelled >> 16) as i32, world.hash())
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_level_is_walkable() {
    let src = data();
    let mut levels = 0;
    for dataset in Dataset::ALL {
        let Ok(sections) = level::sections(&src, dataset) else {
            println!("{dataset}: not in the data folder");
            continue;
        };
        for section in sections.iter().filter(|s| s.map().is_some()) {
            let files = level::find(&src, dataset, section.id).unwrap();
            if !src.exists(&files.map) {
                continue;
            }
            let (map, _, _) = load_map(&src, &files.map, dataset).unwrap();
            let gta = dataset == Dataset::Gta;
            let build = || level_world(map.clone(), section, gta, SimView::GAME, None).unwrap();
            let mut world = build();
            let start = world.drain_log();
            let p = &world.player;
            assert!(
                !start[0].text.contains("no walkable layer"),
                "{dataset} [{}]: {}",
                section.id,
                start[0].text
            );
            assert!(
                gfa_sim::map::ground::walkable(p.ground),
                "{dataset} [{}]: start ground {}",
                section.id,
                p.ground
            );
            let (moved, hash) = wander(&mut world, 1000);
            // 1000 ticks of running is 1300 px if nothing is in the way; a
            // player boxed in at the start would barely move.
            assert!(
                moved > 200,
                "{dataset} [{}]: moved only {moved} px",
                section.id
            );
            assert_eq!(world.player.anim, Anim::Run);
            // The same input from the same start gives the same state.
            let mut again = build();
            assert_eq!(
                wander(&mut again, 1000).1,
                hash,
                "{dataset} [{}]: not deterministic",
                section.id
            );
            println!(
                "{dataset} [{}] {}: {}  ran {moved} px",
                section.id,
                section.name(),
                start[0].text.trim_start_matches("player: "),
            );
            levels += 1;
        }
    }
    assert!(levels >= 6, "only {levels} levels found");
}
