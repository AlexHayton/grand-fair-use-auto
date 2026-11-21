//! Rendering checks against the user's game data, on a headless GPU:
//! nothing undrawn in the city, and the simulation's player, car, dents and
//! vault appear where they should. Run with `cargo data-test`.

use gfa_assets::level::{self, Level};
use gfa_assets::{DataPath, Dataset, FsSource};
use gfa_render::city::DebugView;
use gfa_sim::input::{Buttons, TickInput};
use gfa_sim::replay::level_world;
use gfa_sim::view::SimView;
use gfa_sim::world::{Doing, World};
use gfa_tool::render::{self, HOLECHECK_POSITIONS, Scene, View};
use gfa_tool::run::{player_view, render_world, world_sprites};

const SIZE: (u32, u32) = (640, 400);

fn data() -> FsSource {
    match DataPath::resolve(None) {
        Ok(p) => FsSource::new(p.root),
        Err(e) => panic!("this test needs the game data: {e}"),
    }
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn holecheck_every_map() {
    let src = data();
    for map in [
        "GTADATA/NYC.CMP",
        "GTADATA/SANB.CMP",
        "GTADATA/MIAMI.CMP",
        "GTADATA/UK/UK.CMP",
        "GTADATA/UK/MC.CMP",
        "GTADATA/UK/UKE.CMP",
    ] {
        let mut scene = Scene::load(&src, map, None, SIZE).unwrap();
        scene.show_sprites = false;
        for &at in &HOLECHECK_POSITIONS {
            let view = View {
                at,
                height: 10.0,
                pitch: 0.0,
                yaw: 0.0,
            };
            let holes = render::holes(&scene.render(&view, DebugView::Normal, 0.0));
            assert_eq!(holes, 0, "{map} at {at:?}: {holes} undrawn pixels");
        }
        println!(
            "{map}: no holes at {} viewpoints",
            HOLECHECK_POSITIONS.len()
        );
    }
}

/// Pixels that differ between two frames, and their bounding box.
fn diff(a: &[u8], b: &[u8]) -> (usize, [u32; 4]) {
    let (mut n, mut bx) = (0, [u32::MAX, u32::MAX, 0, 0]);
    for (i, (p, q)) in a.chunks(4).zip(b.chunks(4)).enumerate() {
        if p[..3] != q[..3] {
            let (x, y) = (i as u32 % SIZE.0, i as u32 / SIZE.0);
            n += 1;
            bx = [bx[0].min(x), bx[1].min(y), bx[2].max(x), bx[3].max(y)];
        }
    }
    (n, bx)
}

/// The box is around the middle of the frame, where the camera looks.
fn centred(b: [u32; 4]) -> bool {
    let (cx, cy) = ((b[0] + b[2]) / 2, (b[1] + b[3]) / 2);
    cx.abs_diff(SIZE.0 / 2) <= 4 && cy.abs_diff(SIZE.1 / 2) <= 4
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn the_simulation_is_drawn() {
    let src = data();
    let files = level::find(&src, Dataset::Gta, 1).unwrap();
    let lv = Level::load(&src, files.clone()).unwrap();
    let mut scene = Scene::load_for_sim(&src, &files.map, Some(&lv.style_path), SIZE).unwrap();
    let fresh = || {
        let mut world: World = level_world(
            lv.map.clone(),
            lv.style.cars.clone(),
            &files.mission,
            true,
            SimView::GAME,
            Some((62, 60)),
            false,
        )
        .unwrap();
        // Only what the test puts there.
        world.traffic.clear();
        world
    };
    let mut world = fresh();
    let bare = scene.render(&player_view(&world), DebugView::Normal, 0.0);

    // The player: a standing ped, about 14 by 10 px, in the middle.
    let (n, b) = diff(&bare, &render_world(&mut scene, &world));
    println!("player: {n} pixels, box {b:?}");
    assert!(
        (40..400).contains(&n) && centred(b),
        "player not drawn where he stands"
    );

    // In a car: a saloon is 30 by 62.
    world.put_in_car(0, 62, 60, 128);
    let clean = render_world(&mut scene, &world);
    let (n, b) = diff(&bare, &clean);
    println!("car: {n} pixels, box {b:?}");
    assert!(
        n > 1000 && centred(b) && b[3] - b[1] > b[2] - b[0],
        "car not drawn, or not facing south"
    );

    // Written off: every panel's damage shows, inside the car.
    world.damage_car(100);
    world.step(Default::default());
    let (n, d) = diff(&clean, &render_world(&mut scene, &world));
    println!("dents: {n} pixels, box {d:?}");
    assert!(n > 200, "no damage drawn");
    assert!(
        d[0] >= b[0] && d[2] <= b[2] && d[1] >= b[1] && d[3] <= b[3],
        "damage outside the car"
    );

    // Over the roof: put out of a car whose door (east) is blocked by
    // another, he is drawn on top of his car. The camera stays where it
    // was, on the car.
    let mut world = fresh();
    world.put_in_car(0, 62, 60, 128);
    world.park(0, 16, 0, 128);
    let view = player_view(&world);
    let frame = |scene: &mut Scene, w: &World| {
        let extra = world_sprites(scene, w);
        scene.render_with(&view, DebugView::Normal, 0.0, &extra)
    };
    let before = frame(&mut scene, &world);
    world.step(TickInput::new(Buttons::ENTER));
    assert!(
        matches!(world.doing, Doing::Vaulting(_)),
        "{:?}",
        world.doing
    );
    let (n, v) = diff(&before, &frame(&mut scene, &world));
    println!("vault: {n} pixels, box {v:?}");
    let mid = ((v[0] + v[2]) / 2, (v[1] + v[3]) / 2);
    assert!((40..400).contains(&n), "the vaulting player not drawn");
    assert!(
        (b[0]..=b[2]).contains(&mid.0) && (b[1]..=b[3]).contains(&mid.1),
        "the vaulting player is not on his car"
    );
}
