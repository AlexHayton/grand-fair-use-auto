//! Rendering checks against the user's game data, on a headless GPU:
//! nothing undrawn in the city, the simulation's player, car, dents and
//! vault appear where they should, and people wear the same colours in
//! either style. Run with `cargo data-test`.

use gfa_assets::level::{self, Level};
use gfa_assets::{AssetSource, DataPath, Dataset, FsSource};
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
        world.peds.clear();
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

/// Mean colour of the pixels that differ between `a` and `b`, as drawn in
/// `b`.
fn mean_changed(a: &[u8], b: &[u8]) -> (usize, [f64; 3]) {
    let mut sum = [0.0; 3];
    let mut n = 0;
    for (p, q) in a.chunks(4).zip(b.chunks(4)) {
        if p[..3] != q[..3] {
            n += 1;
            for k in 0..3 {
                sum[k] += f64::from(q[k]);
            }
        }
    }
    (n, sum.map(|s| s / n.max(1) as f64))
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn people_wear_the_same_in_either_style() {
    let src = data();
    let files = level::find(&src, Dataset::Gta, 1).unwrap();
    let lv = Level::load(&src, files.clone()).unwrap();
    let g24 = lv.style_path.clone();
    let gry = g24.replace(".G24", ".GRY");
    assert!(gry != g24 && src.exists(&gry), "no GRY beside {g24}");
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
    world.traffic.clear();
    world.peds.clear();
    // Somebody 40 px east of the player, in each of AmiGTA1's dresses in
    // turn, drawn in both styles; the player stays out of the picture by
    // being compared against himself.
    world.drop_ped(40, 0, 64);
    let i = world
        .peds
        .p
        .iter()
        .position(|p| p.alive)
        .expect("a ped dropped");
    let mut worst = 0.0f64;
    for style in [&g24, &gry] {
        let mut scene = Scene::load_for_sim(&src, &files.map, Some(style), SIZE).unwrap();
        world.peds.p[i].remap = 0;
        let bare = {
            let mut w = world.clone();
            w.peds.clear();
            render_world(&mut scene, &w)
        };
        let own = render_world(&mut scene, &world);
        let (n, b) = diff(&bare, &own);
        println!("{style}: ped {n} pixels, box {b:?}");
        assert!((40..400).contains(&n), "the ped is not drawn");
        assert!(b[0] > SIZE.0 / 2, "the ped is not east of the player");
    }
    for &table in &gfa_sim::peds::REMAPS {
        world.peds.p[i].remap = i32::from(table);
        let mut means = Vec::new();
        for style in [&g24, &gry] {
            let mut scene = Scene::load_for_sim(&src, &files.map, Some(style), SIZE).unwrap();
            let bare = {
                let mut w = world.clone();
                w.peds.clear();
                render_world(&mut scene, &w)
            };
            means.push(mean_changed(&bare, &render_world(&mut scene, &world)));
        }
        let d = (0..3)
            .map(|k| (means[0].1[k] - means[1].1[k]).abs())
            .fold(0.0, f64::max);
        println!(
            "table {table}: G24 {:?} GRY {:?} - {d:.1} apart",
            means[0], means[1]
        );
        worst = worst.max(d);
    }
    // The G24's colours are the GRY's to within its own requantising.
    assert!(
        worst < 6.0,
        "a dress differs by {worst:.1} between the styles"
    );
}

/// The traffic lights stand on a crossing's corners, show the light the
/// cars beside them obey, and face the drivers coming into the crossing.
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn the_lights_stand_on_the_corners() {
    use gfa_render::glam::Vec3;
    use gfa_sim::traffic::roads::{Light, heading_step};
    let src = data();
    let files = level::find(&src, Dataset::Gta, 1).unwrap();
    let lv = Level::load(&src, files.clone()).unwrap();
    let mut scene = Scene::load_for_sim(&src, &files.map, Some(&lv.style_path), SIZE).unwrap();
    let world: World = level_world(
        lv.map.clone(),
        lv.style.cars.clone(),
        &files.mission,
        true,
        SimView::GAME,
        Some((62, 40)),
        false,
    )
    .unwrap();
    // A crossing in Liberty City, blocks 60 to 63 by 44 to 47.
    let lamps: Vec<_> = world
        .traffic
        .lamps(&world.map)
        .into_iter()
        .filter(|l| (59..=64).contains(&l.bx) && (43..=48).contains(&l.by))
        .collect();
    assert_eq!(lamps.len(), 8, "two at each corner");
    let view = View {
        at: (62.0, 46.0),
        height: 10.0,
        pitch: 0.0,
        yaw: 0.0,
    };
    let bare = scene.render(&view, DebugView::Normal, 0.0);
    let show = |lamp| world.traffic.lamp_light(lamp);
    let sprites: Vec<_> = lamps
        .iter()
        .filter_map(|l| {
            let (x, y) = l.pos();
            let pos = [x, y, world.map.ground_z(x, y, l.z)];
            scene.lamp_sprite(show(l).sprite(), pos, l.toward)
        })
        .collect();
    assert_eq!(sprites.len(), lamps.len());
    let lit = scene.render_with(&view, DebugView::Normal, 0.0, &sprites);
    let aspect = SIZE.0 as f32 / SIZE.1 as f32;
    let vp = view.camera().view_proj(aspect);
    let to_screen = |p: [f32; 3]| {
        let c = vp.project_point3(Vec3::from(p));
        (
            (c.x + 1.0) / 2.0 * SIZE.0 as f32,
            (1.0 - c.y) / 2.0 * SIZE.1 as f32,
        )
    };
    // Screen pixels per map pixel at the ground.
    let (ax, _) = to_screen([0.0, 0.0, 128.0]);
    let (bx, _) = to_screen([64.0, 0.0, 128.0]);
    let scale = (bx - ax) / 64.0;
    let centres: Vec<(f32, f32)> = lamps
        .iter()
        .map(|l| {
            let (x, y) = l.pos();
            to_screen(gfa_render::sprites::from_sim([
                x,
                y,
                world.map.ground_z(x, y, l.z),
            ]))
        })
        .collect();
    // Each changed pixel belongs to the nearest lamp, and the lamp is 20 by
    // 24: within 13 px of its middle.
    let reach = 13.0 * scale;
    let mut mine = vec![(0, 0, 0.0f32, 0.0f32); lamps.len()];
    for (k, (a, b)) in bare.chunks(4).zip(lit.chunks(4)).enumerate() {
        if a[..3] == b[..3] {
            continue;
        }
        let (px, py) = ((k as u32 % SIZE.0) as f32, (k as u32 / SIZE.0) as f32);
        let (i, &(sx, sy)) = centres
            .iter()
            .enumerate()
            .min_by(|a, b| {
                let d = |c: &(f32, f32)| (c.0 - px).hypot(c.1 - py);
                d(a.1).total_cmp(&d(b.1))
            })
            .unwrap();
        assert!(
            (px - sx).abs() <= reach && (py - sy).abs() <= reach,
            "pixel ({px},{py}) drawn off the corners"
        );
        // Its lit lens is the colour of its light.
        let [r, g, bl] = [b[0], b[1], b[2]].map(i32::from);
        let is_lens = match show(&lamps[i]) {
            Light::Red => r > 150 && g < 100 && bl < 100,
            Light::Green => g > 150 && r < 140 && bl < 140,
            Light::Amber => r > 200 && (60..180).contains(&g) && bl < 60,
        };
        let m = &mut mine[i];
        m.0 += 1;
        if is_lens {
            m.1 += 1;
            m.2 += px - sx;
            m.3 += py - sy;
        }
    }
    for (l, &(n, lens, lx, ly)) in lamps.iter().zip(&mine) {
        // The lens is on the side the drivers come from.
        let (dx, dy) = heading_step(l.toward);
        let ahead = (lx * dx as f32 + ly * dy as f32) / lens.max(1) as f32;
        println!(
            "lamp at ({},{}) for {:?}, toward {}, {:?}: {n} px, {lens} of lens {ahead:.1} px ahead of its middle",
            l.bx,
            l.by,
            l.road,
            l.toward,
            show(l)
        );
        assert!(n > 40, "lamp at ({},{}) not drawn", l.bx, l.by);
        assert!(lens >= 4, "lamp at ({},{}): no lit lens", l.bx, l.by);
        assert!(
            ahead < -2.0 * scale,
            "lamp at ({},{}) faces into the crossing",
            l.bx,
            l.by
        );
    }
    // One crossing, one light: the lamps for each road agree.
    for axis in [true, false] {
        let shown: Vec<_> = lamps
            .iter()
            .filter(|l| l.along_x == axis)
            .map(show)
            .collect();
        assert!(shown.windows(2).all(|w| w[0] == w[1]), "{shown:?}");
    }
}
