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
            let (map, _, style) = load_map(&src, &files.map, dataset).unwrap();
            let gta = dataset == Dataset::Gta;
            let build = || {
                level_world(
                    map.clone(),
                    style.cars.clone(),
                    section,
                    gta,
                    SimView::GAME,
                    None,
                )
                .unwrap()
            };
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

/// The car physics the simulation reads from the G24 style must equal the
/// GRY's, which AmiGTA1 (the oracle) reads.
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn g24_and_gry_cars_agree() {
    use gfa_formats::style::Style;
    let src = data();
    for n in 1..=3 {
        let g24 = Style::parse(&src.read(&format!("GTADATA/STYLE00{n}.G24")).unwrap()).unwrap();
        let gry = Style::parse(&src.read(&format!("GTADATA/STYLE00{n}.GRY")).unwrap()).unwrap();
        assert_eq!(g24.cars.len(), gry.cars.len(), "style {n}");
        for (i, (a, b)) in g24.cars.iter().zip(&gry.cars).enumerate() {
            // Colours differ by design (HLS for 24-bit, tables for 8-bit),
            // and so may the sprite numbering; physics and doors must not.
            let physics = |c: &gfa_formats::style::CarInfo| {
                (
                    (c.width, c.length, c.vert, c.model, c.vtype, c.cx, c.cy),
                    (
                        c.moment,
                        c.mass,
                        c.thrust,
                        c.tyre_adhesion_x,
                        c.tyre_adhesion_y,
                    ),
                    (
                        c.handbrake_friction,
                        c.turn_ratio,
                        c.drive_wheel_offset,
                        c.steering_wheel_offset,
                    ),
                    (c.max_speed, c.min_speed, c.doors.clone()),
                )
            };
            assert_eq!(physics(a), physics(b), "style {n} car {i}");
        }
        println!("style {n}: {} cars agree", g24.cars.len());
    }
}
