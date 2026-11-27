//! Simulation checks against the user's own game data. Run with
//! `cargo data-test`.

mod common;

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
                    true,
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

/// Every script in tools/scripts/ passes its own `# expect:` and
/// `# reject:` checks, and runs the same twice.
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_script_passes() {
    use gfa_sim::replay::{self, RunOptions};
    use gfa_sim::script::Script;
    let src = data();
    let opts = RunOptions {
        ticks: None,
        hash_every: Some(50),
    };
    let mut failed = Vec::new();
    for (name, text) in common::scripts() {
        let script = Script::parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let lv = common::level(&src, script.section.unwrap_or(1));
        let report = replay::run(
            &mut common::world(&lv, &script, false),
            &script,
            opts,
            |_, _| {},
        );
        let again = replay::run(
            &mut common::world(&lv, &script, false),
            &script,
            opts,
            |_, _| {},
        );
        assert_eq!(report.hashes, again.hashes, "{name}: not deterministic");
        let checks = script.expect.len() + script.reject.len();
        if report.failures.is_empty() {
            println!("{name}: {} ticks, {checks} checks pass", report.ticks);
        } else {
            for f in &report.failures {
                println!("{name}: FAIL {f}");
            }
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "scripts failed: {failed:?}");
}

/// Every traffic light stands on a crossing's corner, faces the drivers
/// coming into it, and shows what the cars on its road obey; and at every
/// crossing one road has the green at a time.
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn the_lamps_face_their_crossings() {
    use gfa_sim::traffic::roads::{Light, heading_step};
    use std::collections::HashMap;
    let src = data();
    for id in [1, 102, 202] {
        let lv = common::level(&src, id);
        let mut world = level_world(
            lv.map.clone(),
            lv.cars.clone(),
            &lv.files.mission,
            true,
            SimView::GAME,
            None,
            true,
        )
        .unwrap();
        let lamps = world.traffic.lamps(&world.map);
        // The crossing each lamp faces: its junction box, flood-filled.
        let mut crossing = HashMap::new();
        let mut of = Vec::new();
        let mut unfaced = Vec::new();
        for l in &lamps {
            let tr = &world.traffic;
            // It names its crossing by the box's root, ahead of it along
            // its road, or off to the side past a T-junction's kerb lane.
            let j = l.key;
            let (dx, dy) = heading_step(l.toward);
            let ahead = |k: i32| (l.road.0 + dx * k, l.road.1 + dy * k);
            let past = |p: (i32, i32)| {
                if l.along_x {
                    (p.0 - l.road.0) * dx > 0
                } else {
                    (p.1 - l.road.1) * dy > 0
                }
            };
            if !tr.is_junction(j.0, j.1, l.z) && !(0..6).any(|z| tr.is_junction(j.0, j.1, z)) {
                unfaced.push((l.bx, l.by));
                of.push(None);
                continue;
            }
            // The box is the way it faces.
            let near = (-2..=2)
                .flat_map(|a| (-2..=2).map(move |b| (a, b)))
                .map(|(a, b)| (l.road.0 + a, l.road.1 + b))
                .any(|p| past(p) && tr.is_junction(p.0, p.1, l.z));
            assert!(
                near || tr.is_junction(ahead(1).0, ahead(1).1, l.z),
                "the lamp at ({},{}) faces away from its crossing",
                l.bx,
                l.by
            );
            if !crossing.contains_key(&j) {
                let n = crossing.len();
                let z = (0..6).find(|&z| tr.is_junction(j.0, j.1, z)).unwrap();
                let mut stack = vec![j];
                while let Some((x, y)) = stack.pop() {
                    if crossing.contains_key(&(x, y)) || !tr.is_junction(x, y, z) {
                        continue;
                    }
                    crossing.insert((x, y), n);
                    stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
                }
            }
            of.push(Some(crossing[&j]));
        }
        let lit: std::collections::BTreeSet<_> = of.iter().flatten().collect();
        println!(
            "section {id}: {} lamps at {} crossings; {} face none: {unfaced:?}",
            lamps.len(),
            lit.len(),
            unfaced.len()
        );
        assert!(lamps.len() > 100 && lit.len() > 20);
        assert!(unfaced.is_empty());
        // Two cycles of the lights, with the city running.
        for _ in 0..1600 {
            world.step(TickInput::default());
            let tr = &world.traffic;
            let tick = tr.tick;
            let mut shown: HashMap<(usize, bool), Light> = HashMap::new();
            for (l, c) in lamps.iter().zip(&of) {
                let light = tr.lamp_light(l);
                // A car on its road at the stop line, heading in, asks
                // about the box ahead. (Past a T-junction's box a kerb lane
                // has none ahead, and does not stop.)
                let (dx, dy) = heading_step(l.toward);
                let (ax, ay) = (l.road.0 + dx, l.road.1 + dy);
                if tr.is_junction(ax, ay, l.z) {
                    assert_eq!(
                        light == Light::Green,
                        tr.light_green(l.along_x),
                        "tick {tick}: the lamp at ({},{}) is not what its cars obey",
                        l.bx,
                        l.by
                    );
                }
                let Some(c) = *c else { continue };
                let was = *shown.entry((c, l.along_x)).or_insert(light);
                assert_eq!(
                    was, light,
                    "tick {tick}: one road, two lights at ({},{})",
                    l.bx, l.by
                );
            }
            for (&(c, along_x), &light) in &shown {
                if along_x && light == Light::Green {
                    assert_ne!(
                        shown.get(&(c, false)),
                        Some(&Light::Green),
                        "tick {tick}: both roads green at crossing {c}"
                    );
                }
            }
        }
    }
}

/// The fleet's view in AmiGTA1's traffic gate (`gtadump drive`), blocks.
const GATE_VIEW: i32 = 6;
/// Ticks a car may stand before the gate calls it stuck: a minute.
const GATE_STUCK: u32 = 3000;
/// The length of each gate run.
const GATE_TICKS: u32 = 12_000;
/// Ticks after a turn when the lane keeper has done its work (AmiGTA1
/// `GTA_AFTER_TURN`).
const AFTER_TURN_TICKS: i32 = 40;

/// The traffic gate (AmiGTA1's `fullgate.sh`, the four sites by seeds 1 to
/// 3 for 12,000 ticks, with its fleet and view): every car on the road and
/// none inside another on every tick, none vanishing from in front of the
/// camera, the fleet moving, and no car stuck. `oracle_traffic` shows the
/// same runs match the patched C harness car for car with DOS light timing.
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn traffic_gate() {
    use gfa_fixed::trig::{cos, sin};
    use gfa_sim::map::ground;
    use gfa_sim::traffic::{Car, Traffic, half_wid, world_len};
    use std::collections::HashMap;

    let ticks: u32 = std::env::var("GFA_TRAFFIC_TICKS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12_000);
    let src = data();
    let lv = common::level(&src, 1);
    let script = gfa_sim::script::Script::parse("# section: 1\n").unwrap();
    let w = common::world(&lv, &script, true);
    let (map, cars) = (&w.map, &w.cars);
    const PX: i64 = 1 << 16;

    // Is the body on the road? Sampled two pixels in from the artwork every
    // 16 px along its real heading: its centre line over road or pavement,
    // and its flanks out of buildings and water (a bus is wider than its
    // lane, and hangs over the kerb wherever it goes). A car on a ramp is
    // between layers: empty at its own, road or pavement above or below.
    // AmiGTA1's gate test (`count_offroad`).
    let on_road = |c: &Car| {
        let ci = &cars[c.model];
        let (half_l, half_w) = (
            i64::from(world_len(ci) / 2 - 2),
            i64::from(half_wid(ci) - 2),
        );
        let (fx, fy) = (i64::from(sin(c.face)), -i64::from(cos(c.face)));
        let mut a = -half_l;
        loop {
            for k in -1..=1i64 {
                let ox = (a * fx + k * half_w * -fy) >> 14;
                let oy = (a * fy + k * half_w * fx) >> 14;
                let (qx, qy) = (
                    ((c.x + ox * PX) >> 21) as i32,
                    ((c.y + oy * PX) >> 21) as i32,
                );
                let g = |z: i32| {
                    map.block(qx, qy, z)
                        .map_or(0, |b| ((b.type_map >> 4) & 7) as u8)
                };
                let paved = |g: u8| g == ground::ROAD || g == ground::PAVEMENT;
                let mut under = g(c.layer);
                if under == ground::AIR
                    && let Some(z) = [c.layer - 1, c.layer + 1]
                        .into_iter()
                        .find(|&z| paved(g(z)))
                {
                    under = g(z);
                }
                let fine =
                    paved(under) || (k != 0 && under != ground::WATER && under != ground::BUILDING);
                if !fine {
                    return false;
                }
            }
            if a >= half_l {
                return true;
            }
            a = (a + 16).min(half_l);
        }
    };
    // Two cars inside each other: their bodies, as boxes on their real
    // headings, overlap by more than a hair (2 px at the ends, 1 at the
    // sides), the fleet's own collision test (`geom::box_hit`). AmiGTA1's
    // gate measures along the first car's nose instead, which halfway round
    // a turn takes a car passing in the next lane for one in front.
    let inside = |a: &Car, b: &Car| {
        if a.layer != b.layer {
            return false;
        }
        let (ia, ib) = (&cars[a.model], &cars[b.model]);
        gfa_sim::traffic::geom::box_hit(
            a.x,
            a.y,
            a.face,
            world_len(ia) / 2 - 2,
            half_wid(ia) - 1,
            b.x,
            b.y,
            b.face,
            world_len(ib) / 2 - 2,
            half_wid(ib) - 1,
        )
    };

    let mut failed = Vec::new();
    for seed in [1u64, 2, 3] {
        for (bx, by) in [(64, 64), (61, 52), (108, 228), (106, 114)] {
            let mut t = Traffic::new(map, seed);
            // The gate's view: half its 320-pixel picture at 32 px a block,
            // and one more.
            t.set_view_blocks(GATE_VIEW);
            let cap = t.fleet_cap;
            t.park(map, cars, bx, by, 8, cap);
            let (cam_x, cam_y) = (i64::from(bx * 32 + 16) << 16, i64::from(by * 32 + 16) << 16);
            // The camera's picture, AmiGTA1's (half 5 by 4 blocks), less a
            // block for cars leaving it at the edge.
            let shown = |c: &(i64, i64)| {
                (c.0 - cam_x).abs() <= 4 * 32 * PX && (c.1 - cam_y).abs() <= 3 * 32 * PX
            };
            // By serial: where it was, and how long it has stood.
            let mut was: HashMap<u64, (i64, i64, u32)> = HashMap::new();
            let (mut moved, mut moving_sum, mut moving_ticks) = (0i64, 0u64, 0u64);
            let (mut stood_worst, mut stood_at, mut stood_by) = (0u32, (0, 0), (0u64, 0u8));
            // Ticks with a fault of each kind, and the first of each.
            let (mut off, mut ov, mut gone) = (0u32, 0u32, 0u32);
            let mut first: Vec<String> = Vec::new();
            let mut note = |n: &mut u32, what: String| {
                if *n == 0 {
                    first.push(what);
                }
                *n += 1;
            };
            for tick in 1..=ticks {
                let before: Vec<_> = t
                    .live()
                    .filter(|c| shown(&(c.x, c.y)))
                    .map(|c| (c.serial, c.x, c.y))
                    .collect();
                t.step(map, cars, cam_x, cam_y);
                let live: Vec<&Car> = t.live().collect();
                if let Some(c) = live.iter().find(|c| !on_road(c)) {
                    let at = (c.x >> 21, c.y >> 21);
                    note(
                        &mut off,
                        format!("tick {tick}: car {} off the road at {at:?}", c.serial),
                    );
                }
                let pair = live
                    .iter()
                    .enumerate()
                    .find_map(|(i, a)| live[i + 1..].iter().find(|b| inside(a, b)).map(|b| (a, b)));
                if let Some((a, b)) = pair {
                    let at = (a.x >> 21, a.y >> 21);
                    note(
                        &mut ov,
                        format!(
                            "tick {tick}: cars {} and {} inside each other at {at:?}",
                            a.serial, b.serial
                        ),
                    );
                }
                // Nothing in the picture before the tick is gone after it.
                if let Some(&(serial, x, y)) = before
                    .iter()
                    .find(|(serial, _, _)| !live.iter().any(|c| c.serial == *serial))
                {
                    let at = (x >> 21, y >> 21);
                    note(
                        &mut gone,
                        format!("tick {tick}: car {serial} vanished in view at {at:?}"),
                    );
                }
                let mut now = HashMap::new();
                let mut movers = 0;
                for c in &live {
                    let stood = match was.get(&c.serial) {
                        Some(&(x, y, s)) => {
                            let d = (c.x - x).abs() + (c.y - y).abs();
                            if d > 0 {
                                movers += 1;
                                moved += d >> 16;
                                0
                            } else if c.abandoned {
                                0
                            } else {
                                s + 1
                            }
                        }
                        None => 0,
                    };
                    if stood > stood_worst {
                        (stood_worst, stood_at, stood_by) = (
                            stood,
                            ((c.x >> 21) as i32, (c.y >> 21) as i32),
                            (c.serial, c.hold),
                        );
                    }
                    now.insert(c.serial, (c.x, c.y, stood));
                }
                moving_sum += movers;
                moving_ticks += live.len() as u64;
                was = now;
            }
            let flow = (moving_sum * 100 / moving_ticks.max(1)) as u32;
            // Stuck: a car standing a minute and more, nothing having moved
            // it, is a jam that will not clear (a queue at a light may
            // take a few cycles).
            let stuck = stood_worst >= GATE_STUCK;
            let ok = off == 0 && ov == 0 && gone == 0 && moved > 0 && !stuck;
            println!(
                "gate ({bx},{by}) seed {seed}: {} | {} cars, {flow}% moving, {} blocks; longest stood {}.{} s at {stood_at:?} (car {}, hold {}) | ticks off the road {off}, with cars inside each other {ov}, with a car vanished in view {gone}",
                if ok { "PASSED" } else { "FAILED" },
                t.n,
                moved / 32,
                stood_worst / 50,
                (stood_worst % 50) / 5,
                stood_by.0,
                stood_by.1
            );
            for f in &first {
                println!("    first: {f}");
            }
            if !ok {
                failed.push(format!("({bx},{by}) seed {seed}"));
            }
        }
    }
    assert!(failed.is_empty(), "the traffic gate failed at {failed:?}");
}

/// No car drives straight on off the road from a block whose arrows do
/// not go that way: at a corner it turns, or waits for the street it turns
/// into. AmiGTA1's cars ran past the corners of Liberty City's one-way loop
/// by (106,114) when the street round the corner was busy, and stood on the
/// pavement (decision log, 2026-11-21). The traffic gate's runs.
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn no_car_runs_past_a_corner() {
    use gfa_sim::traffic::Traffic;
    use gfa_sim::traffic::roads::{dir_allowed, drivable, heading_step};
    use std::collections::HashMap;
    let src = data();
    let lv = common::level(&src, 1);
    let script = gfa_sim::script::Script::parse("# section: 1\n").unwrap();
    let w = common::world(&lv, &script, true);
    let (map, cars) = (&w.map, &w.cars);
    let mut ran = Vec::new();
    for seed in [1u64, 2, 3] {
        for (bx, by) in [(64, 64), (61, 52), (108, 228), (106, 114)] {
            let mut t = Traffic::new(map, seed);
            t.set_view_blocks(GATE_VIEW);
            let cap = t.fleet_cap;
            t.park(map, cars, bx, by, 8, cap);
            let (cx, cy) = (i64::from(bx * 32 + 16) << 16, i64::from(by * 32 + 16) << 16);
            // Where each car was, by serial: block, layer and heading.
            let mut was: HashMap<u64, (i32, i32, i32, i32)> = HashMap::new();
            for tick in 1..=12_000u32 {
                t.step(map, cars, cx, cy);
                for c in t.live() {
                    let now = ((c.x >> 21) as i32, (c.y >> 21) as i32, c.layer, c.angle);
                    if let Some((px, py, pz, pa)) = was.insert(c.serial, now) {
                        let (dx, dy) = heading_step(pa);
                        let past = (now.0, now.1) == (px + dx, py + dy);
                        if past
                            && now.2 == pz
                            && drivable(map, px, py, pz)
                            && !dir_allowed(map, px, py, pz, pa)
                            && !drivable(map, now.0, now.1, pz)
                        {
                            ran.push(format!(
                                "({bx},{by}) seed {seed} tick {tick}: car {} ran past the corner at ({px},{py})",
                                c.serial
                            ));
                        }
                    }
                }
            }
        }
    }
    for r in &ran {
        println!("{r}");
    }
    assert!(ran.is_empty(), "{} cars ran past a corner", ran.len());
}

/// A car shoved out of its lane in a collision, into the lane beside going
/// the other way or onto the kerb, steers back into its own: no car driving
/// stands ten seconds where its block does not go its way, for want of
/// anywhere to go (outside a junction, not waiting behind the car ahead,
/// and not mid-turn, changing lanes, reversing, crossing or on police
/// business). AmiGTA1's stood there until out of sight (decision log,
/// 2026-11-21). The traffic gate's runs.
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn no_car_stands_in_the_wrong_lane() {
    use gfa_sim::traffic::roads::{dir_allowed, drivable};
    use gfa_sim::traffic::{Traffic, hold};
    use std::collections::HashMap;
    let src = data();
    let lv = common::level(&src, 1);
    let script = gfa_sim::script::Script::parse("# section: 1\n").unwrap();
    let w = common::world(&lv, &script, true);
    let (map, cars) = (&w.map, &w.cars);
    let mut stranded = Vec::new();
    for seed in [1u64, 2, 3] {
        for (bx, by) in [(64, 64), (61, 52), (108, 228), (106, 114)] {
            let mut t = Traffic::new(map, seed);
            t.set_view_blocks(GATE_VIEW);
            let cap = t.fleet_cap;
            t.park(map, cars, bx, by, 8, cap);
            let (cx, cy) = (i64::from(bx * 32 + 16) << 16, i64::from(by * 32 + 16) << 16);
            // By serial: ticks stood in a wrong-way block, and where.
            let mut wrong: HashMap<u64, (u32, (i32, i32))> = HashMap::new();
            for tick in 1..=12_000u32 {
                t.step(map, cars, cx, cy);
                let mut now = HashMap::new();
                for c in t.live() {
                    let (qx, qy) = ((c.x >> 21) as i32, (c.y >> 21) as i32);
                    let driving = !(c.abandoned
                        || c.wrecked
                        || c.turn != 0
                        || c.swap != 0
                        || c.reverse != 0
                        || c.cop != 0
                        || c.crossing);
                    // Waiting behind the car ahead is a queue, not lost.
                    let queued = matches!(c.hold, hold::GAP | hold::QUEUE);
                    let astray = driving
                        && c.speed == 0
                        && !queued
                        && !t.is_junction(qx, qy, c.layer)
                        && !(drivable(map, qx, qy, c.layer)
                            && dir_allowed(map, qx, qy, c.layer, c.angle));
                    if astray {
                        let n = wrong.get(&c.serial).map_or(0, |w| w.0) + 1;
                        if n == 500 {
                            stranded.push(format!("({bx},{by}) seed {seed} tick {tick}: car {} stranded at ({qx},{qy}) heading {}", c.serial, c.angle));
                        }
                        now.insert(c.serial, (n, (qx, qy)));
                    }
                }
                wrong = now;
            }
        }
    }
    for s in &stranded {
        println!("{s}");
    }
    assert!(
        stranded.is_empty(),
        "{} cars stranded out of their lanes",
        stranded.len()
    );
}

/// A schematic film of one of the traffic gate's runs, for looking at what
/// the fleet does: the map as ground types and arrows, the traffic lights,
/// and every car as its own box, coloured by what it is doing. Writes PPM
/// frames for ffmpeg. `FILM="bx by seed x0 y0 blocks from to step"` (the
/// gate site, its seed, the square of map shown, and the ticks); `OUT` the
/// folder.
#[test]
#[ignore = "needs game data; makes a film when FILM and OUT are set"]
fn film_gate() {
    use gfa_sim::map::ground;
    use gfa_sim::traffic::roads::Light;
    use gfa_sim::traffic::{Traffic, half_wid, world_len};
    use std::collections::HashMap;
    use std::io::Write;
    let (Ok(film), Ok(out)) = (std::env::var("FILM"), std::env::var("OUT")) else {
        println!("film_gate: set FILM and OUT to make a film");
        return;
    };
    let spec: Vec<i32> = film
        .split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect();
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let (bx, by, seed, x0, y0, n, from, to, step) = (
        spec[0],
        spec[1],
        spec[2] as u64,
        spec[3],
        spec[4],
        spec[5],
        spec[6] as u32,
        spec[7] as u32,
        spec[8] as u32,
    );
    let src = data();
    let lv = common::level(&src, 1);
    let script = gfa_sim::script::Script::parse("# section: 1\n").unwrap();
    let w = common::world(&lv, &script, true);
    let (map, cars) = (&w.map, &w.cars);
    let mut t = Traffic::new(map, seed);
    t.set_view_blocks(GATE_VIEW);
    let cap = t.fleet_cap;
    t.park(map, cars, bx, by, 8, cap);
    let lamps = t.lamps(map);
    let (cx, cy) = (i64::from(bx * 32 + 16) << 16, i64::from(by * 32 + 16) << 16);
    const BAR: usize = 6;
    let side = (n * 32) as usize;
    let (wpx, hpx) = (side, side + BAR);

    // The ground, drawn once: layer 2 (the gate's streets), else layer 1.
    let mut base = vec![[18u8, 18, 22]; wpx * hpx];
    for y in 0..n {
        for x in 0..n {
            let (mx, my) = (x0 + x, y0 + y);
            let pick = [2, 1].into_iter().find_map(|z| {
                map.block(mx, my, z)
                    .map(|b| (z, ((b.type_map >> 4) & 7) as u8, (b.type_map & 15) as u8))
                    .filter(|b| b.1 != 0)
            });
            let Some((z, g, d)) = pick else { continue };
            let mut col: [u8; 3] = match g {
                ground::ROAD if t.is_junction(mx, my, z) => [112, 102, 84],
                ground::ROAD => [88, 88, 92],
                ground::PAVEMENT => [158, 156, 146],
                ground::FIELD => [58, 96, 52],
                ground::BUILDING => [44, 42, 58],
                ground::WATER => [32, 62, 120],
                _ => [18, 18, 22],
            };
            if z == 1 {
                col = col.map(|c| c * 3 / 4);
            }
            for py in 0..32 {
                for px in 0..32 {
                    // A hairline grid, so blocks can be counted.
                    let edge = px == 0 || py == 0;
                    let c = if edge {
                        col.map(|c| c.saturating_sub(14))
                    } else {
                        col
                    };
                    base[(y as usize * 32 + py) * wpx + x as usize * 32 + px] = c;
                }
            }
            // The arrows: a line from the middle toward each way out.
            if g == ground::ROAD {
                for (bit, (ax, ay)) in [(1, (0, -1)), (2, (0, 1)), (4, (-1, 0)), (8, (1, 0))] {
                    if d & bit == 0 {
                        continue;
                    }
                    for k in 0..11i32 {
                        let (px, py) = (16 + ax * k, 16 + ay * k);
                        base[(y as usize * 32 + py as usize) * wpx
                            + x as usize * 32
                            + px as usize] = [200, 200, 205];
                    }
                    // The head: two pixels either side at the tip.
                    for s in [-2i32, -1, 1, 2] {
                        let k = 10 - s.abs();
                        let (px, py) = (16 + ax * k + ay * s, 16 + ay * k + ax * s);
                        base[(y as usize * 32 + py as usize) * wpx
                            + x as usize * 32
                            + px as usize] = [200, 200, 205];
                    }
                }
            }
        }
    }

    // The gate's rule for the centre line: every 16 px along the car's
    // heading, over road or pavement (or a ramp's empty block beside one).
    let off_road = |c: &gfa_sim::traffic::Car| {
        let ci = &cars[c.model];
        let half_l = i64::from(world_len(ci) / 2 - 2);
        let (fx, fy) = (
            i64::from(gfa_fixed::trig::sin(c.face)),
            -i64::from(gfa_fixed::trig::cos(c.face)),
        );
        let mut a = -half_l;
        loop {
            let (qx, qy) = (
                ((c.x + (((a * fx) >> 14) << 16)) >> 21) as i32,
                ((c.y + (((a * fy) >> 14) << 16)) >> 21) as i32,
            );
            let g = |z: i32| {
                map.block(qx, qy, z)
                    .map_or(0, |b| ((b.type_map >> 4) & 7) as u8)
            };
            let paved = |g: u8| g == ground::ROAD || g == ground::PAVEMENT;
            let ok = paved(g(c.layer))
                || (g(c.layer) == ground::AIR && (paved(g(c.layer - 1)) || paved(g(c.layer + 1))));
            if !ok {
                return true;
            }
            if a >= half_l {
                return false;
            }
            a = (a + 16).min(half_l);
        }
    };
    let mut stood: HashMap<u64, (i64, i64, u32)> = HashMap::new();
    let mut frame = 0;
    for tick in 1..=to {
        t.step(map, cars, cx, cy);
        for c in t.live() {
            let e = stood.entry(c.serial).or_insert((c.x, c.y, 0));
            if (e.0, e.1) == (c.x, c.y) {
                e.2 += 1
            } else {
                *e = (c.x, c.y, 0)
            }
        }
        if tick < from || (tick - from) % step != 0 {
            continue;
        }
        let mut img = base.clone();
        let mut put = |x: i64, y: i64, c: [u8; 3]| {
            if (0..wpx as i64).contains(&x) && (0..side as i64).contains(&y) {
                img[y as usize * wpx + x as usize] = c;
            }
        };
        // The lights: a lamp is a 5-pixel dot of its colour.
        for l in &lamps {
            let (lx, ly) = l.pos();
            let (sx, sy) = (
                i64::from((lx >> 16) - x0 * 32),
                i64::from((ly >> 16) - y0 * 32),
            );
            let col = match t.lamp_light(l) {
                Light::Red => [235, 40, 40],
                Light::Green => [40, 220, 70],
                Light::Amber => [250, 180, 20],
            };
            for dy in -2..=2 {
                for dx in -2..=2 {
                    put(sx + dx, sy + dy, col);
                }
            }
        }
        // The cars, as boxes on their real heading, a pale band at the nose.
        for c in t.live() {
            let ci = &cars[c.model];
            let (hl, hw) = (f64::from(world_len(ci)) / 2.0, f64::from(half_wid(ci)));
            let s = stood[&c.serial].2;
            let fill = if c.wrecked || c.abandoned {
                [70, 70, 70]
            } else if s >= 500 {
                [230, 30, 30]
            } else if s >= 150 {
                [255, 150, 0]
            } else if s > 0 {
                [235, 235, 235]
            } else {
                [70, 150, 255]
            };
            let rim = if off_road(c) {
                [255, 0, 255]
            } else {
                [10, 10, 10]
            };
            let a = f64::from(c.face) * std::f64::consts::TAU / 256.0;
            let (fx, fy) = (a.sin(), -a.cos());
            let (px0, py0) = (
                (c.x as f64) / 65536.0 - f64::from(x0 * 32),
                (c.y as f64) / 65536.0 - f64::from(y0 * 32),
            );
            let r = hl.max(hw) as i64 + 2;
            for dy in -r..=r {
                for dx in -r..=r {
                    let (ox, oy) = (dx as f64, dy as f64);
                    let along = ox * fx + oy * fy;
                    let across = ox * -fy + oy * fx;
                    if along.abs() > hl || across.abs() > hw {
                        continue;
                    }
                    let border = along.abs() > hl - 1.5 || across.abs() > hw - 1.5;
                    let col = if border {
                        rim
                    } else if along > hl - 5.0 {
                        fill.map(|v| v / 2 + 128)
                    } else {
                        fill
                    };
                    put(px0.round() as i64 + dx, py0.round() as i64 + dy, col);
                }
            }
        }
        // Time: a bar along the bottom.
        let done = ((tick - from) as usize * wpx) / (to - from).max(1) as usize;
        for y in side..hpx {
            for x in 0..wpx {
                img[y * wpx + x] = if x <= done {
                    [220, 220, 220]
                } else {
                    [40, 40, 40]
                };
            }
        }
        let mut f = std::io::BufWriter::new(
            std::fs::File::create(out.join(format!("f{frame:04}.ppm"))).unwrap(),
        );
        write!(f, "P6\n{wpx} {hpx}\n255\n").unwrap();
        for p in &img {
            f.write_all(p).unwrap();
        }
        frame += 1;
    }
    println!("{frame} frames of {wpx}x{hpx} in {}", out.display());
}

/// The traffic gate's fleet at one of its sites, parked and ready to run.
fn gate_fleet(
    map: &gfa_sim::map::SimMap,
    cars: &[gfa_formats::style::CarInfo],
    (bx, by): (i32, i32),
    seed: u64,
) -> (gfa_sim::traffic::Traffic, (i64, i64)) {
    let mut t = gfa_sim::traffic::Traffic::new(map, seed);
    t.set_view_blocks(GATE_VIEW);
    let cap = t.fleet_cap;
    t.park(map, cars, bx, by, 8, cap);
    (
        t,
        (i64::from(bx * 32 + 16) << 16, i64::from(by * 32 + 16) << 16),
    )
}

/// Liberty City's loop of one-way streets by (106,114) keeps moving. Its
/// cars once ran off the road past its corners, and once queued behind cars
/// beyond the corners they were turning at, until the queues closed into a
/// ring and nothing moved (8 to 25% of the fleet moving, a car standing up
/// to two minutes; decision log, 2026-11-21).
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn the_one_way_loop_keeps_moving() {
    let src = data();
    let lv = common::level(&src, 1);
    let script = gfa_sim::script::Script::parse("# section: 1\n").unwrap();
    let w = common::world(&lv, &script, true);
    let (map, cars) = (&w.map, &w.cars);
    for seed in [1u64, 2, 3] {
        let (mut t, (cx, cy)) = gate_fleet(map, cars, (106, 114), seed);
        let mut was = std::collections::HashMap::new();
        let (mut moving, mut present, mut worst) = (0u64, 0u64, 0u32);
        for _ in 0..12_000 {
            t.step(map, cars, cx, cy);
            let mut now = std::collections::HashMap::new();
            for c in t.live() {
                let stood = match was.get(&c.serial) {
                    Some(&(x, y, s)) if (x, y) == (c.x, c.y) && !c.abandoned => s + 1,
                    Some(_) => {
                        moving += 1;
                        0
                    }
                    None => 0,
                };
                worst = worst.max(stood);
                present += 1;
                now.insert(c.serial, (c.x, c.y, stood));
            }
            was = now;
        }
        let flow = moving * 100 / present.max(1);
        println!(
            "seed {seed}: {flow}% moving, longest stood {:.1} s",
            f64::from(worst) / 50.0
        );
        assert!(flow >= 40, "seed {seed}: only {flow}% of the fleet moving");
        assert!(worst < 1500, "seed {seed}: a car stood {} ticks", worst);
    }
}

/// No car stands ten seconds at a dead end, with nowhere it may go, in the
/// traffic gate's runs. Long vehicles were refused corners whose far side is
/// a building or grass and stood there for good, blocking their streets
/// (decision log, 2026-11-21).
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn no_car_stops_at_a_dead_end() {
    use gfa_sim::traffic::hold;
    let src = data();
    let lv = common::level(&src, 1);
    let script = gfa_sim::script::Script::parse("# section: 1\n").unwrap();
    let w = common::world(&lv, &script, true);
    let (map, cars) = (&w.map, &w.cars);
    let mut stuck = Vec::new();
    for seed in [1u64, 2, 3] {
        for site in [(64, 64), (61, 52), (108, 228), (106, 114)] {
            let (mut t, (cx, cy)) = gate_fleet(map, cars, site, seed);
            let mut held = std::collections::HashMap::new();
            for tick in 1..=12_000u32 {
                t.step(map, cars, cx, cy);
                let mut now = std::collections::HashMap::new();
                for c in t.live().filter(|c| c.hold == hold::DEADEND) {
                    let n = held.get(&c.serial).copied().unwrap_or(0) + 1;
                    if n == 500 {
                        stuck.push(format!(
                            "{site:?} seed {seed} tick {tick}: car {} at a dead end at ({},{})",
                            c.serial,
                            c.x >> 21,
                            c.y >> 21
                        ));
                    }
                    now.insert(c.serial, n);
                }
                held = now;
            }
        }
    }
    for s in &stuck {
        println!("{s}");
    }
    assert!(stuck.is_empty(), "{} cars stood at a dead end", stuck.len());
}

/// The special key held in a car behind a fleet car on a Liberty City
/// street: the car moves over to the kerb and drives on without stopping,
/// and once the key has been let go a while, it goes back to its lane (the
/// original's pull-over, `GTA8.EXE` and the owner's P1-P6, M6b).
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn the_car_ahead_pulls_over_for_the_horn() {
    use gfa_sim::input::{Buttons, TickInput};
    let src = data();
    let lv = common::level(&src, 1);
    let script = gfa_sim::script::Script::parse("# section: 1\n").unwrap();
    let mut base = common::world(&lv, &script, true);
    for _ in 0..300 {
        base.step(TickInput::default());
    }
    let plain = |w: &World, bx: i32, by: i32, z: i32| {
        gfa_sim::traffic::roads::drivable(&w.map, bx, by, z) && !w.traffic.is_junction(bx, by, z)
    };
    // Cars driving straight along a plain street, with plain road behind
    // them for his car.
    let candidates: Vec<u64> = base
        .traffic
        .live()
        .filter(|c| {
            let (bx, by) = c.block();
            let (dx, dy) = gfa_sim::traffic::roads::heading_step(c.lane_dir);
            c.speed > 0
                && c.cop == 0
                && c.turn == 0
                && c.swap == 0
                && !c.crossing
                && c.lane_set
                && c.lane_dir & 63 == 0
                && (-3..=3).all(|k| plain(&base, bx + k * dx, by + k * dy, c.layer))
        })
        .map(|c| c.serial)
        .collect();
    assert!(!candidates.is_empty(), "no car on a plain street");
    let mut tried = 0;
    for serial in candidates {
        let mut w = base.clone();
        let car = |w: &World| w.traffic.live().find(|c| c.serial == serial).cloned();
        let c = car(&w).unwrap();
        let (bx, by) = c.block();
        let (dx, dy) = gfa_sim::traffic::roads::heading_step(c.lane_dir);
        w.put_in_car(0, bx - 2 * dx, by - 2 * dy, c.lane_dir);
        w.drain_log();
        let tab = TickInput::new(Buttons::SPECIAL);
        // Held until the car is on the kerb line, or two seconds.
        let mut told = false;
        let mut over = None;
        for _ in 0..100 {
            w.step(tab);
            told |= w
                .drain_log()
                .iter()
                .any(|l| l.text == format!("traffic: car {serial} pulls over"));
            let Some(c) = car(&w).filter(|c| c.pull > 0) else {
                continue;
            };
            // The kerb on its right, 28 px into the block heading north or
            // east, 4 heading south or west; the other side where that is
            // road.
            let across = if c.lane_dir & 127 == 64 { c.y } else { c.x };
            let into = ((across >> 16) & 31) as i32;
            let far = (c.lane_dir == 0 || c.lane_dir == 64) != c.pull_left;
            if (into - if far { 28 } else { 4 }).abs() <= 2 {
                over = Some(c);
                break;
            }
        }
        tried += 1;
        // A car with another close in front may be held short of it.
        let Some(c) = over else {
            continue;
        };
        assert!(told, "car {serial} pulled over without being told");
        assert!(c.speed > 0, "car {serial} stopped at the kerb");
        // Let go: a while on, it is back to its lane.
        let mut back = false;
        for _ in 0..300 {
            w.step(TickInput::default());
            let Some(c) = car(&w) else {
                break;
            };
            if c.pull == 0 {
                back = true;
                break;
            }
        }
        assert!(back, "car {serial} never went back to its lane");
        println!("car {serial} pulled over and drove on (tried {tried})");
        return;
    }
    panic!("no car pulled over in {tried} tries");
}

/// Through the junctions of the gate's twelve runs, cars keep their lanes:
/// none turns twice in one crossing (AmiGTA1's junction wobble, which came
/// from cars with no route re-rolling their heading every tick), a car
/// going straight across stays within a pixel of its line, and a turn
/// lands within a pixel of the lane it turns into, still there 40 ticks on
/// (AmiGTA1's corner landing; decision log, 2026-11-21). A crossing or
/// landing with a lane change, a knock or a reverse in it is not counted.
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn cars_keep_their_lanes_through_junctions() {
    use std::collections::{HashMap, HashSet};
    let src = data();
    let lv = common::level(&src, 1);
    let script = gfa_sim::script::Script::parse("# section: 1\n").unwrap();
    let w = common::world(&lv, &script, true);
    let (map, cars) = (&w.map, &w.cars);
    let (mut crossings, mut twice) = (0u32, Vec::new());
    let (mut straight, mut straight_off) = (0u32, 0u32);
    let (mut landed, mut landed_off) = (0u32, 0u32);
    for seed in [1u64, 2, 3] {
        for site in [(64, 64), (61, 52), (108, 228), (106, 114)] {
            let (mut t, (cx, cy)) = gate_fleet(map, cars, site, seed);
            // serial -> (angle in, its line, drift, turns, a turn begun, not clean)
            let mut inside: HashMap<u64, (i32, i64, i64, u32, bool, bool)> = HashMap::new();
            // serial -> (line, east-west, ticks left, not clean)
            let mut after: HashMap<u64, (i64, bool, i32, bool)> = HashMap::new();
            let mut prev: HashMap<u64, (i32, i32)> = HashMap::new();
            for tick in 1..=GATE_TICKS {
                t.step(map, cars, cx, cy);
                let mut seen = HashSet::new();
                for c in t.live().filter(|c| !c.abandoned) {
                    seen.insert(c.serial);
                    let rough = c.swap != 0 || c.knock > 0 || c.reverse > 0;
                    let (bx, by) = c.block();
                    let ew = c.angle & 127 == 64;
                    let across = if ew { c.y } else { c.x };
                    let (turn_was, angle_was) = prev
                        .insert(c.serial, (c.turn, c.angle))
                        .unwrap_or((c.turn, c.angle));
                    let began = turn_was == 0 && c.turn != 0;
                    if t.is_junction(bx, by, c.layer) {
                        let e = inside
                            .entry(c.serial)
                            .or_insert((c.angle, across, 0, 0, false, false));
                        if c.angle != angle_was {
                            e.3 += 1;
                        }
                        if c.angle == e.0 && c.turn == 0 {
                            e.2 = e.2.max((across - e.1).abs());
                        }
                        e.4 |= began;
                        e.5 |= rough;
                    } else if let Some((angle_in, _, drift, turns, turned, rough_in)) =
                        inside.remove(&c.serial)
                    {
                        crossings += 1;
                        if turns >= 2 {
                            twice.push(format!(
                                "{site:?} seed {seed} tick {tick}: car {} turned {turns} times",
                                c.serial
                            ));
                        }
                        if c.angle == angle_in && !turned && !rough_in {
                            straight += 1;
                            straight_off += u32::from(drift > 1 << 16);
                        }
                    }
                    if turn_was != 0 && c.turn == 0 {
                        after.insert(c.serial, (across, ew, AFTER_TURN_TICKS, rough));
                    }
                    if let Some(a) = after.get_mut(&c.serial) {
                        a.3 |= rough || c.turn != 0;
                        a.2 -= 1;
                        if a.2 == 0 {
                            let (line, ew, _, rough_after) = after.remove(&c.serial).unwrap();
                            if !rough_after {
                                let now = if ew { c.y } else { c.x };
                                landed += 1;
                                landed_off += u32::from((now - line).abs() > 1 << 16);
                            }
                        }
                    }
                }
                inside.retain(|s, _| seen.contains(s));
                after.retain(|s, _| seen.contains(s));
                prev.retain(|s, _| seen.contains(s));
            }
        }
    }
    println!(
        "{crossings} crossings; {straight} straight across, {straight_off} more than a pixel off their line; \
         {landed} turns, {landed_off} more than a pixel off 40 ticks on"
    );
    for l in &twice {
        println!("{l}");
    }
    assert!(
        twice.is_empty(),
        "{} cars turned twice in one crossing",
        twice.len()
    );
    assert!(straight > 1000 && landed > 1000, "too few to judge");
    assert!(
        straight_off * 100 <= straight,
        "{straight_off} of {straight} drifted across"
    );
    assert!(
        landed_off * 100 <= landed,
        "{landed_off} of {landed} slid after a turn"
    );
}
