//! The simulation on small maps built in memory: no game data needed, so
//! these run with plain `cargo test`.
//!
//! The behaviour itself is checked against AmiGTA1 by `cargo oracle-test`;
//! these tests guard the rules on maps whose every block is known.

use gfa_formats::cmp::{Block, Cmp, CmpBuilder};
use gfa_formats::style::{CarDoor, CarInfo};
use gfa_sim::input::{Buttons, InputLog, TickInput};
use gfa_sim::map::{SimMap, WallTiles};
use gfa_sim::replay::{self, RunOptions};
use gfa_sim::script::Script;
use gfa_sim::vehicle::body_blocked;
use gfa_sim::view::SimView;
use gfa_sim::world::{Doing, Start, World};

const ROAD: u16 = 0x20;
const PAVEMENT: u16 = 0x30;
const FIELD: u16 = 0x40;
const BUILDING: u16 = 0x50;
const FLAT: u16 = 0x80;
const RAILWAY: u8 = 0x80;

fn block(type_map: u16) -> Block {
    Block {
        type_map,
        ..Block::default()
    }
}

/// A floor on layer 1 under a road on layer 2, over a rectangle of blocks.
fn street(b: &mut CmpBuilder, xs: std::ops::Range<u8>, ys: std::ops::Range<u8>) {
    let floor = Block {
        faces: [0, 0, 0, 0, 1],
        ..Block::default()
    };
    for x in xs {
        for y in ys.clone() {
            b.set(x, y, 1, floor).set(x, y, 2, block(ROAD));
        }
    }
}

/// A saloon with plausible numbers: thrust 15 over mass 10, one door.
fn saloon() -> CarInfo {
    CarInfo {
        width: 30,
        length: 62,
        vert: 10,
        sprite: 0,
        weight: 0,
        max_speed: 30,
        min_speed: -10,
        accel: 0,
        braking: 0,
        grip: 0,
        handling: 0,
        hls_remaps: [[0; 3]; 12],
        remap8: [0; 12],
        vtype: 4,
        model: 0,
        turning: 0,
        damagable: 1,
        value: [0; 4],
        cx: 0,
        cy: -5,
        moment: 1360,
        mass: 10 << 16,
        thrust: 15 << 16,
        tyre_adhesion_x: 9175,
        tyre_adhesion_y: 60000,
        handbrake_friction: 30000,
        footbrake_friction: 0,
        front_brake_bias: 0,
        turn_ratio: 12,
        drive_wheel_offset: -25,
        steering_wheel_offset: 25,
        back_end_slide: 0,
        handbrake_slide: 0,
        convertible: 0,
        engine: 0,
        radio: 0,
        horn: 0,
        sound_function: 0,
        fast_change: 0,
        doors: vec![CarDoor {
            rpy: -6,
            rpx: 0,
            object: 0,
            delta: 6,
        }],
    }
}

/// A bus: as tall as the table's buses (24), too tall to vault.
fn bus() -> CarInfo {
    CarInfo {
        length: 120,
        vert: 24,
        model: 1,
        ..saloon()
    }
}

/// The car table: a saloon (model 0) and a bus (model 1).
fn world(b: &CmpBuilder, start: Start) -> World {
    let cmp = Cmp::parse(&b.build()).unwrap();
    let mut w = World::new(
        SimMap::new(cmp, WallTiles::gta(1)),
        vec![saloon(), bus()],
        SimView::GAME,
        start,
    );
    w.drain_log();
    w
}

/// The `i`th car standing in the street.
fn standing(w: &World, i: usize) -> gfa_sim::traffic::Car {
    w.traffic.standing().nth(i).unwrap().clone()
}

fn press(b: Buttons) -> TickInput {
    TickInput::new(b)
}

fn run(w: &mut World, b: Buttons, ticks: u32) {
    for _ in 0..ticks {
        w.step(press(b));
    }
}

fn log_has(w: &mut World, text: &str) -> bool {
    w.drain_log().iter().any(|l| l.text.contains(text))
}

fn south(bx: i32, by: i32) -> Start {
    Start {
        bx,
        by,
        angle: None,
    }
}

#[test]
fn player_stops_at_a_building() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 10..20, 10..20);
    b.set(15, 14, 2, block(BUILDING));
    let mut w = world(&b, south(15, 11));
    assert_eq!(w.player.layer, 2);
    run(&mut w, Buttons::UP, 200);
    let (bx, by) = w.player.block();
    assert_eq!(
        (bx, by),
        (15, 13),
        "stopped on the block before the building"
    );
    assert!(w.player.blocked_y);
    // Backing off works: half pace, 0.65 px a tick.
    run(&mut w, Buttons::DOWN, 100);
    assert!(w.player.block().1 < 13);
}

#[test]
fn player_slides_along_a_wall() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 10..20, 10..20);
    for x in 10..20 {
        b.set(x, 14, 2, block(BUILDING));
    }
    // Facing south-east, into a wall running east-west: he slides east.
    let mut w = world(
        &b,
        Start {
            bx: 12,
            by: 12,
            angle: Some(96),
        },
    );
    let x0 = w.player.x;
    run(&mut w, Buttons::UP, 150);
    assert!(w.player.blocked_y && !w.player.blocked_x);
    assert!(w.player.x > x0 + (20 << 16), "slid east");
    assert_eq!(w.player.block().1, 13);
}

#[test]
fn player_climbs_a_ramp_to_the_layer_above() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..30, 13..18);
    // A 7 degree ramp rising east over x 11..18 on layer 2 (slopes 33..40),
    // then road on layer 3 over the street.
    for (i, x) in (11..19).enumerate() {
        let mut r = block(ROAD);
        r.type_map |= (33 + i as u16) << 8;
        b.set(x, 15, 2, r);
    }
    for x in 19..25 {
        b.set(x, 15, 3, block(ROAD));
    }
    let mut w = world(
        &b,
        Start {
            bx: 9,
            by: 15,
            angle: Some(64),
        },
    );
    run(&mut w, Buttons::UP, 350);
    assert_eq!(w.player.layer, 3, "on the road above");
    assert!(log_has(&mut w, "player: layer 2 -> 3 at (19,15)"));
}

#[test]
fn fences_stop_walkers() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 10..20, 10..20);
    // A fence on the north edge of (15,14): style 1's solid tile 46.
    let mut fence = block(ROAD | FLAT);
    fence.faces = [0, 0, 46, 0, 0];
    b.set(15, 14, 2, fence);
    let mut w = world(&b, south(15, 11));
    run(&mut w, Buttons::UP, 200);
    assert_eq!(w.player.block().1, 13, "held at the fence");
    // Without the fence list, the same map lets him through.
    let cmp = Cmp::parse(&b.build()).unwrap();
    let mut open = World::new(
        SimMap::new(cmp, WallTiles::NONE),
        vec![],
        SimView::GAME,
        south(15, 11),
    );
    run(&mut open, Buttons::UP, 200);
    assert!(open.player.block().1 > 14);
}

#[test]
fn missing_pavement_is_pavement() {
    let mut b = CmpBuilder::new(1);
    let floor = Block {
        faces: [0, 0, 0, 0, 1],
        ..Block::default()
    };
    for x in 10..13 {
        for y in 10..13 {
            b.set(x, y, 1, floor);
            if (x, y) != (11, 11) {
                b.set(x, y, 2, block(PAVEMENT));
            }
        }
    }
    let cmp = Cmp::parse(&b.build()).unwrap();
    let map = SimMap::new(cmp, WallTiles::NONE);
    assert!(map.block(11, 11, 2).is_none());
    assert!(
        map.walkable(11, 11, 2),
        "the hole in the pavement is pavement"
    );
}

#[test]
fn car_drives_and_stops_at_a_wall() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..40);
    for x in 5..25 {
        b.set(x, 30, 2, block(BUILDING));
    }
    let mut w = world(&b, south(15, 8));
    w.put_in_car(0, 15, 8, 128);
    assert_eq!(w.doing, Doing::Driving);
    run(&mut w, Buttons::UP, 300);
    let d = w.driven.as_ref().unwrap();
    let v = &d.veh;
    assert!(v.damage > 0, "the wall cost bodywork");
    assert!(v.dmg_bits != 0, "and dented a panel");
    assert!((v.oy >> 21) < 30, "never inside the building");
    assert!(!body_blocked(
        &w.map,
        d.layer,
        v.ox,
        v.oy,
        v.angle(),
        v.len / 2,
        v.wid / 2
    ));
    assert!(log_has(&mut w, "car: wall hit"));
}

#[test]
fn reverse_gear_stops_the_car_and_backs_it_up() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..60);
    let mut w = world(&b, south(15, 8));
    w.put_in_car(0, 15, 8, 128);
    run(&mut w, Buttons::UP, 100);
    let fast = w.driven.as_ref().unwrap().veh.vy;
    assert!(fast > 0, "moving south");
    // No foot brake: the reverse gear slows it, stops it and backs it up.
    let furthest = w.driven.as_ref().unwrap().veh.oy;
    run(&mut w, Buttons::DOWN, 150);
    let v = &w.driven.as_ref().unwrap().veh;
    assert!(v.vy < 0, "now reversing north");
    assert!(v.oy < furthest);
}

#[test]
fn get_into_a_parked_car_and_out() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    // Facing south, its door (east) faces him: a plain walk to it.
    w.park(0, -20, 0, 128);
    w.step(press(Buttons::ENTER));
    assert!(matches!(w.doing, Doing::Entering(e) if e.over.is_none()));
    run(&mut w, Buttons::NONE, 100);
    assert_eq!(w.doing, Doing::Driving);
    assert_eq!(w.traffic.standing().count(), 0);
    w.step(press(Buttons::ENTER));
    assert!(matches!(w.doing, Doing::Exiting(_)));
    run(&mut w, Buttons::NONE, 60);
    assert_eq!(w.doing, Doing::OnFoot);
    assert_eq!(w.traffic.standing().count(), 1, "the car is parked again");
    // And he cannot walk through it.
    let car = standing(&w, 0);
    w.player.x = (car.x - (40 << 16)) as i32;
    w.player.y = car.y as i32;
    w.player.angle = 64;
    run(&mut w, Buttons::UP, 60);
    assert!(
        i64::from(w.player.x) < car.x - (8 << 16),
        "held off the car's side"
    );
}

#[test]
fn nobody_gets_out_at_speed() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..60);
    let mut w = world(&b, south(15, 8));
    w.put_in_car(0, 15, 8, 128);
    run(&mut w, Buttons::UP, 80);
    w.step(press(Buttons::UP.with(Buttons::ENTER, true)));
    assert_eq!(w.doing, Doing::Driving);
    assert!(log_has(&mut w, "car: too fast to get out"));
}

#[test]
fn a_write_off_burns_then_blows_up() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.put_in_car(0, 15, 15, 128);
    w.damage_car(100);
    run(&mut w, Buttons::NONE, 5);
    assert!(w.driven.as_ref().unwrap().veh.fuse > 0, "burning");
    run(&mut w, Buttons::NONE, 130);
    assert!(w.driven.is_none());
    assert_eq!(w.doing, Doing::OnFoot);
    assert!(w.traffic.standing().any(|c| c.wrecked));
    // A wreck cannot be got into.
    w.step(press(Buttons::ENTER));
    assert!(log_has(&mut w, "car: no car within reach"));
}

/// The player's frame while he is over a roof: ped sprites 91..94.
fn vault_frame(w: &World) -> bool {
    w.player_frame().is_some_and(|f| (91..=94).contains(&f))
}

#[test]
fn getting_in_from_the_far_side_goes_over_the_roof() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    // Facing south, a saloon's door is on its east flank; he stands 20 px
    // to its west.
    w.park(0, 20, 0, 128);
    let car = standing(&w, 0);
    w.step(press(Buttons::ENTER));
    assert!(log_has(&mut w, "car: far side - will vault heading 64"));
    let mut vaulted = false;
    for _ in 0..120 {
        w.step(press(Buttons::NONE));
        vaulted |= vault_frame(&w);
        if w.doing == Doing::Driving {
            break;
        }
    }
    assert!(vaulted, "drawn going over the roof");
    assert_eq!(w.doing, Doing::Driving);
    let log: Vec<String> = w.drain_log().into_iter().map(|l| l.text).collect();
    assert!(
        log.iter().any(|l| l.starts_with("car: landed after state")),
        "{log:?}"
    );
    let v = &w.driven.as_ref().unwrap().veh;
    assert_eq!((v.ox, v.oy), (car.x, car.y), "the car has not moved");
}

#[test]
fn a_blocked_door_puts_him_out_over_the_roof() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.put_in_car(0, 15, 15, 128);
    // Another car hard against the door (east) flank.
    w.park(0, 16, 0, 128);
    let x = i64::from(w.player.x);
    w.step(press(Buttons::ENTER));
    assert!(log_has(
        &mut w,
        "car: door blocked by a car - over the roof, heading 192"
    ));
    assert!(matches!(w.doing, Doing::Vaulting(_)));
    assert_eq!(w.traffic.standing().count(), 2, "his car is parked first");
    assert!(vault_frame(&w));
    run(&mut w, Buttons::NONE, 80);
    assert_eq!(w.doing, Doing::OnFoot);
    assert!(log_has(&mut w, "player: landed after state"));
    assert!(
        i64::from(w.player.x) < x - (10 << 16),
        "down on the west side, clear of the car"
    );
    // Back on his feet with the controls.
    let y = w.player.y;
    run(&mut w, Buttons::UP, 20);
    assert!(w.player.x != x as i32 || w.player.y != y);
}

#[test]
fn jump_while_running_vaults_a_low_car() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.player.angle = 64;
    // Facing north, 30 px east of him: he runs into its flank.
    w.park(0, 30, 0, 0);
    let car = standing(&w, 0);
    run(&mut w, Buttons::UP, 40);
    assert!(
        i64::from(w.player.x) < car.x - (9 << 16),
        "held off the flank"
    );
    // Jump does nothing while standing.
    run(&mut w, Buttons::NONE, 1);
    w.step(press(Buttons::HANDBRAKE));
    assert_eq!(w.doing, Doing::OnFoot);
    assert!(!log_has(&mut w, "player: jump"));
    run(&mut w, Buttons::UP, 2);
    w.step(press(Buttons::UP.with(Buttons::HANDBRAKE, true)));
    assert!(log_has(&mut w, "player: jump - vaulting car 0"));
    run(&mut w, Buttons::UP, 60);
    assert_eq!(w.doing, Doing::OnFoot);
    assert!(log_has(&mut w, "player: landed after state 3"));
    assert!(
        i64::from(w.player.x) > car.x + (7 << 16),
        "over to the far side"
    );
}

#[test]
fn jump_under_a_bus_slides_him_out() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.player.angle = 64;
    // A bus parked over him, facing north: he is under its middle.
    w.park(1, 0, 0, 0);
    let bus = standing(&w, 0);
    run(&mut w, Buttons::UP, 1);
    w.step(press(Buttons::UP.with(Buttons::HANDBRAKE, true)));
    assert!(log_has(&mut w, "player: jump - sliding under car 0"));
    assert_eq!(w.player_frame(), Some(41));
    run(&mut w, Buttons::NONE, 80);
    assert_eq!(w.doing, Doing::OnFoot);
    assert!(log_has(&mut w, "player: out from under"));
    assert!(
        i64::from(w.player.x) > bus.x + (8 << 16),
        "out on the east side"
    );
}

#[test]
fn walking_into_water_drowns_him() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..15);
    // Open water south of the street, on the street's layer.
    for x in 5..25 {
        for y in 15..20 {
            b.set(x, y, 2, block(0x10));
        }
    }
    let mut w = world(&b, south(15, 13));
    run(&mut w, Buttons::UP, 80);
    assert!(matches!(w.doing, Doing::Drowning(_)), "{:?}", w.doing);
    assert!(log_has(&mut w, "player: in the water at (15,15)"));
    assert_eq!(w.player_frame(), None, "gone under");
    assert!(w.health < 100);
    run(&mut w, Buttons::NONE, 80);
    assert!(log_has(&mut w, "player: wasted - drowned"));
    assert_eq!(w.doing, Doing::OnFoot);
    assert_eq!(w.health, 100);
    assert_eq!(w.player.block(), (15, 13), "back at the start");
}

/// A rectangle of railway on layer 2, x 10..=40 and y 10..=20, with a
/// spawning station (hint 7) on the top side and a plain one (6) on the
/// bottom.
fn rail_loop() -> CmpBuilder {
    let mut b = CmpBuilder::new(1);
    let rail = |b: &mut CmpBuilder, x: u8, y: u8, hint: u8| {
        let mut r = block(FIELD);
        r.type_map_ext = RAILWAY | hint;
        b.set(x, y, 2, r);
    };
    for x in 10..=40 {
        rail(&mut b, x, 10, if x == 25 { 7 } else { 0 });
        rail(&mut b, x, 20, if x == 25 { 6 } else { 0 });
    }
    for y in 11..20 {
        rail(&mut b, 10, y, 0);
        rail(&mut b, 40, y, 0);
    }
    b
}

#[test]
fn a_train_runs_round_its_loop_and_stops_at_stations() {
    let b = rail_loop();
    let mut w = world(&b, south(5, 5));
    assert_eq!(w.trains.trains.len(), 1);
    assert!(!w.trains.trains[0].tram);
    let s0 = w.trains.trains[0].s;
    let mut stops = 0;
    for _ in 0..12_000 {
        w.step(TickInput::default());
        stops += w
            .drain_log()
            .iter()
            .filter(|l| l.text.contains("stops at"))
            .count();
    }
    assert!(stops >= 2, "it stopped at both stations ({stops} stops)");
    assert_ne!(w.trains.trains[0].s, s0);
}

#[test]
fn an_open_line_has_no_trains() {
    let mut b = rail_loop();
    // Break the loop.
    b.set(10, 15, 2, Block::default());
    let w = world(&b, south(5, 5));
    assert!(w.trains.trains.is_empty());
}

#[test]
fn same_script_same_hashes() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..40, 5..40);
    b.set(20, 20, 2, block(BUILDING));
    let script = Script::parse(
        "0 1 0 50\n1 1 0 30\n-1 1 1 40\npark 0 30 0 64\nenter\nwait 80\nrun 100 1 0 1 1\nrun 60 0 1 0 0\n",
    )
    .unwrap();
    let opts = RunOptions {
        ticks: None,
        hash_every: Some(25),
    };
    let a = replay::run(&mut world(&b, south(10, 10)), &script, opts, |_, _| {});
    let b2 = replay::run(&mut world(&b, south(10, 10)), &script, opts, |_, _| {});
    assert!(a.hashes.len() > 10);
    assert_eq!(a.hashes, b2.hashes);
    assert!(a.log.iter().any(|l| l.text.contains("car: in")));
}

#[test]
fn a_recording_replays_the_run() {
    use gfa_formats::cmp::StartupObject;
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..40, 5..40);
    // A car parked by the map (not by a script fixture), beside the start:
    // original px (64 a block), height 383 - 128 for layer 2, facing south.
    b.object(StartupObject {
        x: 11 * 64 + 32,
        y: 10 * 64 + 32,
        z: 383 - 128,
        kind: 0,
        remap: 128,
        rotation: 512,
        pitch: 0,
        roll: 0,
    });
    let script = Script::parse(
        "0 1 0 5\nenter\nwait 80\nrun 120 1 0 1 0\nrun 40 1 0 0 1\nrun 80 0 1 -1 0\n",
    )
    .unwrap();
    let opts = RunOptions {
        ticks: None,
        hash_every: Some(50),
    };
    let mut live = world(&b, south(10, 10));
    assert_eq!(live.traffic.standing().count(), 1);
    let mut rec = InputLog::default();
    let report = replay::run_recorded(&mut live, &script, opts, &mut rec, |_, _| {});
    assert!(report.log.iter().any(|l| l.text.contains("car: in")));
    let mut played = world(&b, south(10, 10));
    let hashes = replay::play(&mut played, &rec, report.ticks, Some(50));
    assert_eq!(hashes, report.hashes, "the recording reproduces every hash");
}

/// Pavement both sides of a north-south road: blocks x 5..45, y 5..45, the
/// road at x 24 and 25.
fn pavements() -> CmpBuilder {
    let mut b = CmpBuilder::new(1);
    let floor = Block {
        faces: [0, 0, 0, 0, 1],
        ..Block::default()
    };
    for x in 5..45 {
        for y in 5..45 {
            let kind = if (24..26).contains(&x) {
                ROAD
            } else {
                PAVEMENT
            };
            b.set(x, y, 1, floor).set(x, y, 2, block(kind));
        }
    }
    b
}

#[test]
fn the_crowd_comes_on_at_the_pavements() {
    use gfa_sim::map::ground;
    let mut w = world(&pavements(), south(20, 25));
    let mut was: Vec<bool> = w.peds.p.iter().map(|p| p.alive).collect();
    let mut seen = 0;
    for _ in 0..600 {
        w.step(TickInput::default());
        for (i, p) in w.peds.p.iter().enumerate() {
            if p.alive && !was[i] {
                seen += 1;
                let g = w
                    .map
                    .nav_ground((p.x >> 21) as i32, (p.y >> 21) as i32, p.layer);
                assert_eq!(
                    g,
                    ground::PAVEMENT,
                    "ped {i} came on at {:?}",
                    (p.x >> 21, p.y >> 21)
                );
            }
            was[i] = p.alive;
        }
    }
    let about = w.peds.count();
    assert!(seen >= 10 && about >= 10, "{seen} came on, {about} about");
}

#[test]
fn a_car_runs_a_ped_over() {
    let mut w = world(&pavements(), south(25, 8));
    w.peds.clear();
    // Lying in the road 4 blocks ahead: killed under the car, but not
    // counted as run over (AmiGTA1's ram).
    w.drop_ped(0, 128, 0);
    let i = w.peds.p.iter().position(|p| p.alive).unwrap();
    w.peds.p[i].down = 1000;
    w.put_in_car(0, 25, 8, 128);
    // Looked at as it happens: a body left behind out of sight is cleared
    // and its slot given to somebody new.
    let under = (0..100).find(|_| {
        w.step(press(Buttons::UP));
        w.peds.p[i].corpse
    });
    assert!(under.is_some(), "drove over him and he lives");
    assert!(
        !log_has(&mut w, "car: ran over"),
        "a man lying down counted"
    );
    // Standing in the road just ahead of the moving car: no time to get
    // out of the way, and he counts.
    let v = w.driven.as_ref().unwrap().veh.clone();
    assert!(v.vy > 4 << 16, "not up to speed");
    let at = (v.ox, v.oy + (48 << 16));
    assert!(w.peds.drop_ped(at.0, at.1, 2, 64, -1, 0));
    let j = w
        .peds
        .p
        .iter()
        .position(|p| p.alive && (p.x, p.y) == at)
        .unwrap();
    let hit = (0..10).find(|_| {
        w.step(press(Buttons::UP));
        log_has(&mut w, "car: ran over 1 - ")
    });
    assert!(hit.is_some(), "never hit him");
    assert!(w.peds.p[j].corpse);
}

#[test]
fn taking_a_car_drags_its_driver_out() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.peds.clear();
    w.park(0, -20, 0, 128);
    // Somebody at the wheel of it.
    let k = w.traffic.cars.iter().position(|c| !c.done).unwrap();
    w.traffic.cars[k].driver = true;
    w.step(press(Buttons::ENTER));
    assert!(log_has(&mut w, "car: somebody is at the wheel"));
    run(&mut w, Buttons::NONE, 100);
    assert_eq!(w.doing, Doing::Driving);
    assert!(log_has(&mut w, "car: dragged the driver out (ped "));
    // Pulled out of the door and left lying beside the car...
    let car = w.driven.as_ref().unwrap().veh.clone();
    let near = |p: &gfa_sim::peds::Ped| {
        (p.x - car.ox).abs() < (48 << 16) && (p.y - car.oy).abs() < (48 << 16)
    };
    let i = w
        .peds
        .p
        .iter()
        .position(|p| p.alive && p.down > 0 && near(p))
        .expect("the driver lying by the car");
    // ...then up and walking.
    run(&mut w, Buttons::NONE, 150);
    let p = &w.peds.p[i];
    assert!(p.alive && !p.corpse && p.down == 0, "{p:?}");
}

#[test]
fn a_ped_in_the_water_drowns() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..15);
    for x in 5..25 {
        for y in 15..20 {
            b.set(x, y, 2, block(0x10));
        }
    }
    let mut w = world(&b, south(15, 13));
    w.peds.clear();
    w.drop_ped(0, 80, 0);
    let i = w.peds.p.iter().position(|p| p.alive).unwrap();
    w.step(TickInput::default());
    assert!(log_has(
        &mut w,
        &format!("ped: {i} in the water at (15,16)")
    ));
    assert!(!w.peds.p[i].shown(), "gone under");
    run(&mut w, Buttons::NONE, gfa_sim::peds::DROWN_TICKS as u32 + 2);
    assert!(log_has(&mut w, &format!("ped: {i} drowned")));
    assert_eq!(w.peds.stats.drowned, 1);
}

#[test]
fn nobody_spins_on_the_spot() {
    // A crowd on pavements between a road and a wall of buildings, for a
    // minute. A sidestep bends a man's heading a quarter turn; it must not
    // pile up into a spin, nor flick back and forth along a kerb.
    let mut b = pavements();
    for x in 30..34 {
        for y in 5..45 {
            b.set(x, y, 2, block(BUILDING));
        }
    }
    let mut w = world(&b, south(27, 20));
    w.park(0, 40, 0, 0);
    let n = w.peds.p.len();
    let mut last: Vec<Option<i32>> = vec![None; n];
    let mut turning = vec![0; n];
    let mut worst = (0, 0, 0);
    let mut crowded = 0;
    for t in 0..3000 {
        w.step(TickInput::default());
        for (i, p) in w.peds.p.iter().enumerate() {
            let walking = p.alive && !p.corpse && p.down == 0 && p.speed > 0;
            let a = walking.then_some(p.angle);
            match (last[i], a) {
                (Some(l), Some(a)) => {
                    let d = (a - l) & 255;
                    if d.min(256 - d) >= 32 {
                        turning[i] += 1;
                        if turning[i] > worst.0 {
                            worst = (turning[i], i, t);
                        }
                    } else {
                        turning[i] = 0;
                    }
                }
                _ => turning[i] = 0,
            }
            last[i] = a;
        }
        crowded += w.peds.p.iter().filter(|p| p.alive && p.dodging()).count();
    }
    assert!(crowded > 100, "nobody sidestepped ({crowded}): not a test");
    assert!(
        worst.0 <= 8,
        "ped {} turned hard on {} ticks running, to tick {}",
        worst.1,
        worst.0,
        worst.2
    );
}

/// A press and release of the special key, `hold` ticks down.
fn tab(w: &mut World, hold: u32) {
    run(w, Buttons::SPECIAL, hold);
    run(w, Buttons::NONE, 5);
}

#[test]
fn the_special_key_works_the_siren_then_sounds_it() {
    use gfa_sim::cars::{DELTA_LIGHT0, Siren};
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..40);
    let cmp = Cmp::parse(&b.build()).unwrap();
    let police = CarInfo {
        model: 4,
        ..saloon()
    };
    let mut w = World::new(
        SimMap::new(cmp, WallTiles::gta(1)),
        vec![saloon(), police],
        SimView::GAME,
        south(15, 8),
    );
    w.put_in_car(1, 15, 8, 128);
    w.drain_log();
    let siren = |w: &World| w.driven.as_ref().unwrap().siren;
    let lights = |w: &World| w.driven_look().unwrap().0 & (3 << DELTA_LIGHT0);
    assert_eq!(lights(&w), 0, "dark until the siren is on");

    // The first press puts it on, and holding that press does not sound it.
    tab(&mut w, 30);
    assert_eq!(siren(&w), Siren::On);
    assert!(log_has(&mut w, "special key - siren on (model 4)"));
    // The lights take turns, ten ticks each.
    let a = lights(&w);
    run(&mut w, Buttons::NONE, 10);
    let b2 = lights(&w);
    assert!(a.count_ones() == 1 && b2.count_ones() == 1 && a != b2);

    // A press after that sounds it while held; letting go goes back to on.
    run(&mut w, Buttons::SPECIAL, 20);
    assert_eq!(siren(&w), Siren::Sounding);
    assert!(log_has(&mut w, "special key - siren sounding (model 4)"));
    run(&mut w, Buttons::NONE, 1);
    assert_eq!(siren(&w), Siren::On);
    assert!(log_has(&mut w, "special key - siren on (model 4)"));

    // Getting out puts it out.
    run(&mut w, Buttons::ENTER, 1);
    run(&mut w, Buttons::NONE, 1);
    assert!(matches!(w.doing, Doing::Exiting(_)), "{:?}", w.doing);
    assert_eq!(siren(&w), Siren::Off);
}

#[test]
fn the_special_key_is_a_horn_in_a_car_and_a_shout_on_foot() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..40);
    let mut w = world(&b, south(15, 8));
    tab(&mut w, 3);
    assert!(log_has(&mut w, "special key - a shout"));
    w.put_in_car(0, 15, 8, 128);
    run(&mut w, Buttons::SPECIAL, 50);
    assert!(w.driven.as_ref().unwrap().horn, "held down, it sounds");
    run(&mut w, Buttons::NONE, 1);
    assert!(!w.driven.as_ref().unwrap().horn);
    let log = w.drain_log();
    assert!(
        log.iter()
            .any(|l| l.text == "special key - horn down (model 0)")
    );
    assert!(
        log.iter()
            .any(|l| l.text == "special key - horn up (model 0)")
    );
    assert!(!log.iter().any(|l| l.text.contains("siren")));
}
