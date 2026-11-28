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
            rpx: 24,
            object: 9,
            delta: 7,
        }],
    }
}

/// A bus: as tall as the table's buses (24), too tall to vault.
fn bus() -> CarInfo {
    CarInfo {
        width: 40,
        length: 120,
        vert: 24,
        model: 1,
        doors: vec![CarDoor {
            rpy: 50,
            rpx: 27,
            object: 9,
            delta: 7,
        }],
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
fn a_door_wall_stops_walkers_until_it_opens() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 10..20, 10..20);
    b.set(15, 15, 2, block(BUILDING));
    let mut w = world(&b, south(15, 11));
    // A shut door on the north edge of (15,14), held through an edit next
    // to it, which works the grids round it out again.
    w.map.set_door_wall(15, 14, 2, true, true);
    w.map.set_block(16, 14, 2, block(ROAD));
    run(&mut w, Buttons::UP, 200);
    assert_eq!(w.player.block().1, 13, "held at the door");
    // Open, and the building beyond made field: he walks into it.
    w.map.set_door_wall(15, 14, 2, true, false);
    assert!(!w.map.walkable(15, 15, 2));
    w.map.set_block(15, 15, 2, block(FIELD));
    assert!(w.map.walkable(15, 15, 2));
    run(&mut w, Buttons::UP, 200);
    assert!(w.player.block().1 > 14);
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
    let (x, y) = (i64::from(w.player.x), i64::from(w.player.y));
    w.step(press(Buttons::ENTER));
    assert!(log_has(
        &mut w,
        "car: door blocked by a car - over the roof, heading 128"
    ));
    assert!(matches!(w.doing, Doing::Vaulting(_)));
    assert_eq!(w.traffic.standing().count(), 2, "his car is parked first");
    assert!(vault_frame(&w));
    run(&mut w, Buttons::NONE, 80);
    assert_eq!(w.doing, Doing::OnFoot);
    assert!(log_has(&mut w, "player: landed after state"));
    assert!(
        i64::from(w.player.y) > y + (15 << 16),
        "down ahead of the front bumper, clear of the car"
    );
    // Back on his feet with the controls.
    let y0 = w.player.y;
    run(&mut w, Buttons::UP, 20);
    assert!(w.player.x != x as i32 || w.player.y != y0);
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

#[test]
fn drowning_player_can_fire() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..15);
    for x in 5..25 {
        for y in 15..20 {
            b.set(x, y, 2, block(0x10));
        }
    }
    let mut w = world(&b, south(15, 13));
    run(&mut w, Buttons::UP, 80);
    assert!(matches!(w.doing, Doing::Drowning(_)));
    w.arms.give(1, 5);
    run(&mut w, Buttons::FIRE, 5);
    assert!(w.arms.punch_left > 0 || !w.weapons.shots.is_empty());
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
        // A Hare Krishna line may take slots beyond the crowd's.
        last.resize(w.peds.p.len(), None);
        turning.resize(w.peds.p.len(), 0);
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

#[test]
fn heavy_bodies_take_less_from_a_bullet_and_any_finishes_at_75() {
    use gfa_sim::weapons::bullet_damage;
    let car = saloon();
    assert_eq!(bullet_damage(&car, 0), 5);
    assert_eq!(bullet_damage(&car, 80), 20);
    let bus = CarInfo { vtype: 0, ..bus() };
    assert_eq!(bullet_damage(&bus, 50), 2);
    assert_eq!(bullet_damage(&bus, 75), 25);
}

#[test]
fn the_top_up_gives_every_gun_99_rounds_once_a_press() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..40);
    let mut w = world(&b, south(15, 8));
    w.give_weapon(1, 3);
    run(&mut w, Buttons::ALL_WEAPONS, 5);
    assert_eq!(w.arms.ammo, [0, 99, 99, 99, 99]);
    assert_eq!(w.arms.weapon, 1, "the gun in hand stays");
    // Held, it does not count as pressed again.
    w.arms.ammo[1] = 10;
    run(&mut w, Buttons::ALL_WEAPONS, 3);
    assert_eq!(w.arms.ammo[1], 10);
    let log = w.drain_log();
    assert_eq!(
        log.iter()
            .filter(|l| l.text.starts_with("cheat: keypad-*"))
            .count(),
        1
    );
}

#[test]
fn crates_open_by_shot_car_or_blast_and_powerups_obey_caps() {
    use gfa_sim::pickups::{PickupState, kind};
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..30, 5..30);
    let mut w = world(&b, south(15, 15));

    // Place a pistol crate 1 block south and shoot it open.
    w.drop_crate(0, 1, kind::PISTOL, 0);
    assert_eq!(w.pickups.p[0].state, PickupState::Crate);
    w.give_weapon(1, 5);
    run(&mut w, Buttons::FIRE, 10);
    assert_eq!(w.pickups.p[0].state, PickupState::Open);
    assert!(
        w.pickups
            .smashes
            .iter()
            .any(|s| s.frame().is_some_and(|f| (1..=12).contains(&f))),
        "shooting a crate should play the smashed crate animation"
    );
    // Advance until the smash animation finishes.
    run(
        &mut w,
        Buttons::NONE,
        gfa_sim::pickups::CRATE_SMASH_LIFE as u32,
    );
    assert!(
        w.pickups.smashes.iter().all(|s| s.frame().is_none()),
        "smash animation should finish and clear"
    );

    // Refused when ammo is already 99; taken when below 99.
    w.arms.ammo[1] = 99;
    run(&mut w, Buttons::UP, 24);
    assert_eq!(w.pickups.p[0].state, PickupState::Open);
    w.arms.ammo[1] = 50;
    run(&mut w, Buttons::NONE, 2);
    assert_eq!(w.pickups.p[0].state, PickupState::Gone);
    assert_eq!(w.arms.ammo[1], 70);

    // Armour, multiplier, life, jail-free, bribe, speed, and frenzy.
    let (bx, by) = w.player.block();
    for (k, amt) in [
        (kind::ARMOUR, 5),
        (kind::MULTIPLIER, 1),
        (kind::LIFE, 2),
        (kind::JAILFREE, 1),
        (kind::SPEED, 1),
        (kind::BRIBE, 1),
    ] {
        w.pickups.p.clear();
        w.drop_crate(0, 0, k, amt);
        w.pickups.p[0].state = PickupState::Open;
        if k == kind::BRIBE {
            w.score.force_level(2);
        }
        run(&mut w, Buttons::NONE, 1);
        assert_eq!(
            w.pickups.p[0].state,
            PickupState::Gone,
            "failed to take kind {k} at ({bx},{by})"
        );
    }
    assert_eq!(w.armour, 5);
    assert_eq!(w.score.multiplier, 2);
    assert_eq!(w.lives, 6);
    assert!(w.jail_free);
    assert!(w.player_speed > 0);
    assert_eq!(w.score.level, 0);

    // Endless weapon from kill frenzy restores previous ammo on expiry.
    w.arms.give(1, 12);
    w.arms.give_endless(2, 5);
    assert_eq!(w.arms.weapon, 2);
    assert_eq!(w.arms.ammo[2], 99);
    run(&mut w, Buttons::FIRE, 3);
    assert_eq!(w.arms.ammo[2], 99);
    run(&mut w, Buttons::NONE, 5);
    assert_eq!(w.arms.inf_ticks, 0);
    assert_eq!(w.arms.weapon, 1);
    assert_eq!(w.arms.ammo[1], 12);
}

#[test]
fn running_over_a_crate_plays_smash_animation_even_when_taken_immediately() {
    use gfa_sim::pickups::{CRATE_SMASH_LIFE, PickupState, kind};
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..30, 5..30);
    let mut w = world(&b, south(15, 15));
    w.park(0, -20, 0, 128);
    w.step(press(Buttons::ENTER));
    run(&mut w, Buttons::NONE, 100);
    assert_eq!(w.doing, Doing::Driving);

    // Place a pistol crate 1 block south of the car and drive over it.
    w.drop_crate(0, 1, kind::PISTOL, 0);
    assert_eq!(w.pickups.p[0].state, PickupState::Crate);
    let mut saw_smash = false;
    for _ in 0..80 {
        w.step(press(Buttons::UP));
        if w.pickups.p[0].state == PickupState::Gone {
            saw_smash = w.pickups.smashes.iter().any(|s| s.frame() == Some(1));
            break;
        }
    }
    assert_eq!(w.pickups.p[0].state, PickupState::Gone);
    assert!(
        saw_smash,
        "running over a crate should start the smash animation at frame 1"
    );
    // Advance until the smash animation completes.
    run(&mut w, Buttons::NONE, CRATE_SMASH_LIFE as u32);
    assert!(
        w.pickups.smashes.iter().all(|s| s.frame().is_none()),
        "smash animation should finish after CRATE_SMASH_LIFE ticks"
    );
}

#[test]
fn police_arrest_on_foot_and_jail_free_key_saves_weapons_and_multiplier() {
    use gfa_sim::world::Card;
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..30, 5..30);
    let mut w = world(&b, south(15, 15));
    w.peds.clear();
    w.score.force_level(1);
    w.score.multiplier = 4;
    w.armour = 5;
    w.give_weapon(1, 20);
    w.jail_free = true;

    // Spawn a cop on foot right next to the player with no arrest wait.
    let (px, py, pl) = (i64::from(w.player.x), i64::from(w.player.y), w.player.layer);
    assert!(w.peds.spawn_cop(px + (4 << 16), py, pl, 192));
    let ci = w.peds.last.unwrap();
    w.peds.p[ci].arrest_wait = 0;
    w.peds.p[ci].exit_wait = 0;

    // Without fire held, the cop arrests him on foot -> FOOT_ARREST_TICKS -> BUSTED.
    run(&mut w, Buttons::NONE, 5);
    assert!(w.foot_arrest > 0 || w.card == Card::Busted);
    run(&mut w, Buttons::NONE, 20);
    assert_eq!(w.card, Card::Busted);
    // Jail free key preserved his multiplier, armour, and pistol!
    assert_eq!(w.score.multiplier, 4);
    assert_eq!(w.armour, 5);
    assert_eq!(w.arms.ammo[1], 20);
    run(&mut w, Buttons::NONE, gfa_sim::world::BUST_TICKS as u32 + 2);
    assert_eq!(w.card, Card::None);
    assert!(!w.jail_free, "key is consumed after the bust");

    // Second bust without the key halves multiplier and strips weapons & armour.
    w.score.force_level(1);
    assert!(w.peds.spawn_cop(
        i64::from(w.player.x) + (4 << 16),
        i64::from(w.player.y),
        w.player.layer,
        192
    ));
    let ci = w.peds.last.unwrap();
    w.peds.p[ci].arrest_wait = 0;
    w.peds.p[ci].exit_wait = 0;
    run(&mut w, Buttons::NONE, 25);
    assert_eq!(w.card, Card::Busted);
    assert_eq!(w.score.multiplier, 2);
    assert_eq!(w.armour, 0);
    assert_eq!(w.arms.ammo[1], 0);
}

#[test]
fn wasted_card_respawns_at_hospital_and_last_life_is_game_over() {
    use gfa_formats::cmp::BlockPos;
    use gfa_sim::world::{BUST_TICKS, Card};
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..35, 5..35);
    let mut cmp = Cmp::parse(&b.build()).unwrap();
    cmp.locations[1][0] = BlockPos { x: 25, y: 25, z: 2 };
    let mut w = World::new(
        SimMap::new(cmp, WallTiles::gta(1)),
        vec![saloon()],
        SimView::GAME,
        south(10, 10),
    );
    w.lives = 2;
    w.hurt(100);
    w.step(press(Buttons::NONE));
    assert_eq!(w.card, Card::Wasted);
    assert_eq!(w.bust_timer, BUST_TICKS);
    run(&mut w, Buttons::NONE, BUST_TICKS as u32);
    assert_eq!(w.card, Card::None);
    assert_eq!(w.lives, 1);
    assert_eq!(w.player.block(), (25, 25), "respawned at the hospital");

    // Losing the last life triggers GAME OVER.
    w.hurt(100);
    w.step(press(Buttons::NONE));
    assert_eq!(w.card, Card::Wasted);
    run(&mut w, Buttons::NONE, BUST_TICKS as u32);
    assert_eq!(w.lives, 0);
    assert!(w.game_over);
    assert_eq!(w.card, Card::GameOver);
}

#[test]
fn fire_key_latch_clears_across_wasted_until_released() {
    use gfa_formats::cmp::BlockPos;
    use gfa_sim::world::{BUST_TICKS, Card};

    // Live-city WASTED card + hospital respawn: holding FIRE continuously
    // through death and respawn stays disarmed until released and re-pressed.
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..35, 5..35);
    let mut cmp = Cmp::parse(&b.build()).unwrap();
    cmp.locations[1][0] = BlockPos { x: 25, y: 25, z: 2 };
    let mut w = World::new(
        SimMap::new(cmp, WallTiles::gta(1)),
        vec![saloon()],
        SimView::GAME,
        south(10, 10),
    );
    w.step(press(Buttons::FIRE));
    assert!(w.arms.fire_held);
    w.hurt(100);
    w.step(press(Buttons::FIRE));
    assert_eq!(w.card, Card::Wasted);
    assert!(!w.arms.fire_held);
    run(&mut w, Buttons::FIRE, BUST_TICKS as u32 + 5);
    assert_eq!(w.card, Card::None);
    assert_eq!(w.player.block(), (25, 25));
    assert!(!w.arms.fire_held);
    assert_eq!(w.arms.punch_left, 0);

    w.step(press(Buttons::NONE));
    w.step(press(Buttons::FIRE));
    assert!(w.arms.fire_held);
    assert!(w.arms.punch_left > 0);

    // Immediate (!live_city) respawn: same latch clearing until release.
    let mut b2 = CmpBuilder::new(1);
    street(&mut b2, 5..35, 5..35);
    let cmp2 = Cmp::parse(&b2.build()).unwrap();
    let mut w = World::with_map_cars(
        SimMap::new(cmp2, WallTiles::gta(1)),
        vec![saloon()],
        SimView::GAME,
        south(10, 10),
        false,
    );
    w.arms.give(1, 5);
    w.step(press(Buttons::FIRE));
    assert!(w.arms.fire_held);
    w.hurt(100);
    run(&mut w, Buttons::FIRE, 5);
    assert_eq!(w.player.block(), (10, 10));
    assert!(!w.arms.fire_held);
    assert_eq!(w.arms.punch_left, 0);

    w.step(press(Buttons::NONE));
    w.step(press(Buttons::FIRE));
    assert!(w.arms.fire_held);
    assert!(w.arms.punch_left > 0);
}

#[test]
fn ambulance_dispatches_and_medic_revives_dead_ped() {
    let mut b = CmpBuilder::new(1);
    let floor = Block {
        faces: [0, 0, 0, 0, 1],
        ..Block::default()
    };
    let mut road_s = block(ROAD);
    road_s.type_map |= 0x03; // N | S arrows
    for x in 10..25 {
        for y in 5..40 {
            b.set(x, y, 1, floor).set(x, y, 2, road_s);
        }
    }
    let cmp = Cmp::parse(&b.build()).unwrap();
    let amb_info = CarInfo {
        model: 5,
        ..saloon()
    };
    let mut w = World::new(
        SimMap::new(cmp, WallTiles::gta(1)),
        vec![saloon(), amb_info],
        SimView::GAME,
        south(15, 20),
    );
    w.peds.clear();
    w.kill_ped_decl(999);
    let dead_slot = w.peds.p.iter().position(|p| p.alive && p.corpse).unwrap();

    // Step so service scans the body and dispatches an ambulance.
    run(&mut w, Buttons::NONE, 5);
    assert_eq!(w.service.stat_amb_sent, 1);
    // Bring the ambulance within 2 blocks of the body so medics deploy.
    let ser = w.service.amb[0].car_serial;
    if let Some(c) = w.traffic.car_mut(ser) {
        c.x = w.peds.p[dead_slot].x;
        c.y = w.peds.p[dead_slot].y - (2 << 21);
        c.arrived = true;
    }
    run(&mut w, Buttons::NONE, 200);
    assert_eq!(w.service.stat_revived, 1);
    assert!(!w.peds.p[dead_slot].corpse, "ped was revived by the medic");
}

#[test]
fn fire_engine_dispatches_and_extinguishes_burning_wreck() {
    let mut b = CmpBuilder::new(1);
    let floor = Block {
        faces: [0, 0, 0, 0, 1],
        ..Block::default()
    };
    let mut road_s = block(ROAD);
    road_s.type_map |= 0x03;
    for x in 10..25 {
        for y in 5..40 {
            b.set(x, y, 1, floor).set(x, y, 2, road_s);
        }
    }
    let cmp = Cmp::parse(&b.build()).unwrap();
    let fire_info = CarInfo {
        model: 42,
        ..saloon()
    };
    let mut w = World::new(
        SimMap::new(cmp, WallTiles::gta(1)),
        vec![saloon(), fire_info],
        SimView::GAME,
        south(15, 20),
    );
    w.traffic.clear();
    w.wreck_car_decl(999);
    run(&mut w, Buttons::NONE, 10);
    assert_eq!(w.service.stat_fire_sent, 1);
    let ser = w.service.eng[0].car_serial;
    let (fx, fy) = (w.service.eng[0].fire_x, w.service.eng[0].fire_y);
    if let Some(c) = w.traffic.car_mut(ser) {
        c.x = fx;
        c.y = fy - (2 << 21);
        c.arrived = true;
    }
    run(&mut w, Buttons::NONE, 120);
    assert!(
        w.service.stat_fires_out >= 1,
        "fire engine extinguished the fire"
    );
}

#[test]
fn spray_shop_and_bomb_shop_respray_and_rig_car_bomb() {
    use gfa_sim::world::{BombShop, SprayShop};
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.sprays.push(SprayShop {
        line: 1,
        bx: 15,
        by: 15,
        color: 3,
        used: false,
    });
    w.bombshops.push(BombShop {
        line: 2,
        bx: 15,
        by: 15,
        used: false,
    });
    w.score.score = 10_000;
    w.score.force_level(2);
    w.cars[0].remap8[2] = 1;
    w.put_in_car(0, 15, 15, 128);
    w.driven.as_mut().unwrap().mission = true;
    w.damage_car(45);

    run(&mut w, Buttons::NONE, 4);
    assert_eq!(w.driven.as_ref().unwrap().veh.damage, 0);
    assert_eq!(w.driven.as_ref().unwrap().veh.remap, Some(3));
    assert_eq!(w.score.level, 0);
    assert_eq!(w.score.score, 2_980);
    assert_eq!(w.bombs[0].kind, 1);

    // Arm the bomb with FIRE while driving, get out, and let the 250-tick fuse blow it up.
    run(&mut w, Buttons::FIRE, 2);
    assert_eq!(w.bombs[0].kind, 2);
    assert!(w.bombs[0].fuse > 0);
    run(&mut w, Buttons::NONE, 260);
    assert_eq!(w.bombs[0].kind, 0);
    assert!(log_has(&mut w, "car: bomb detonated"));
}

#[test]
fn spray_shop_charges_for_plates_and_preserves_state_on_refusal() {
    use gfa_sim::world::SprayShop;
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.sprays.push(SprayShop {
        bx: 15,
        by: 15,
        color: 3,
        ..SprayShop::default()
    });
    w.cars[0].value[0] = 4;
    w.cars[0].remap8[2] = 1;
    w.score.score = 8_159;
    w.score.force_level(4);
    w.put_in_car(0, 14, 15, 128);

    run(&mut w, Buttons::NONE, 4);
    assert!(!log_has(&mut w, "no_respray"));
    w.put_in_car(0, 15, 15, 128);
    w.damage_car(2);
    run(&mut w, Buttons::NONE, 4);
    assert!(log_has(&mut w, "no_respray"));
    assert_eq!(w.score.score, 8_159);
    assert_eq!(w.score.level, 4);
    assert_eq!(w.driven.as_ref().unwrap().veh.damage, 2);
    run(&mut w, Buttons::NONE, 4);
    assert!(!log_has(&mut w, "no_respray"));

    w.put_in_car(0, 14, 15, 128);
    run(&mut w, Buttons::NONE, 4);
    w.score.score += 1;
    w.put_in_car(0, 15, 15, 128);
    w.damage_car(2);
    run(&mut w, Buttons::NONE, 4);
    assert!(log_has(&mut w, "clean_done"));
    assert_eq!(w.score.score, 0);
    assert_eq!(w.score.level, 0);
    assert_eq!(w.driven.as_ref().unwrap().veh.damage, 0);
    assert_eq!(w.driven.as_ref().unwrap().veh.remap, None);
}

#[test]
fn bomb_shop_refuses_special_vehicles_and_insufficient_score() {
    use gfa_sim::world::BombShop;
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    for bx in [15, 19] {
        w.bombshops.push(BombShop {
            bx,
            by: 15,
            ..BombShop::default()
        });
    }
    w.bomb_cost = 1_000;

    for (model, score, key) in [
        (5, 2_000, "no_emerg_respray"),
        (37, 0, "no_tank_respray"),
        (47, 2_000, "no_modelcar_respray"),
        (0, 999, "no_respray"),
    ] {
        w.cars[0].model = model;
        w.score.score = score;
        w.put_in_car(0, 15, 15, 128);
        w.damage_car(5);
        run(&mut w, Buttons::NONE, 4);
        assert!(log_has(&mut w, key));
        assert_eq!(w.score.score, score);
        assert_eq!(w.driven.as_ref().unwrap().veh.damage, 5);
        assert!(w.bombs.iter().all(|bomb| bomb.kind == 0));
        w.put_in_car(0, 14, 15, 128);
        run(&mut w, Buttons::NONE, 4);
    }

    w.cars[0].model = 4;
    w.score.score = 1_000;
    w.put_in_car(0, 15, 15, 128);
    run(&mut w, Buttons::NONE, 4);
    assert_eq!(w.score.score, 0);
    assert_eq!(w.bombs[0].kind, 1);

    w.score.score = 1_000;
    w.driven.as_mut().unwrap().veh.ox += 4 << 21;
    run(&mut w, Buttons::NONE, 4);
    assert_eq!(w.score.score, 1_000);
    assert_eq!(w.bombs.iter().filter(|bomb| bomb.kind != 0).count(), 1);
}

#[test]
fn type_4_bomb_needs_damage_above_ten_and_goes_off_as_five_blasts() {
    use gfa_sim::world::CarBomb;
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.put_in_car(0, 15, 15, 128);
    let serial = 7;
    w.driven.as_mut().unwrap().serial = serial;
    w.bombs[0] = CarBomb {
        serial,
        kind: 4,
        fuse: 0,
    };

    // Ten is not enough (`GTA8.EXE` tests `car+0xf7 > 10`).
    w.damage_car(10);
    run(&mut w, Buttons::NONE, 4);
    assert_eq!(w.bombs[0].kind, 4);
    assert!(!log_has(&mut w, "car: bomb detonated"));

    // Eleven sets it off, and the write-off blows up as the bomb's five
    // blasts (four corners, then the centre), not as a wreck's.
    w.damage_car(1);
    let before = w.weapons.stats.blasts;
    run(&mut w, Buttons::NONE, 4);
    assert_eq!(w.bombs[0].kind, 0);
    assert!(log_has(&mut w, "car: bomb detonated"));
    run(&mut w, Buttons::NONE, 200);
    assert_eq!(w.weapons.stats.blasts - before, 5);
}

#[test]
fn a_car_blowing_up_pays_only_when_the_player_caused_it() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.traffic.clear();
    w.park(0, 0, 100, 0);
    w.park(0, 0, -100, 0);
    let n = w.traffic.n;
    assert!(n >= 2, "two parked cars");
    for (i, mine) in [(n - 2, false), (n - 1, true)] {
        let c = &mut w.traffic.cars[i];
        c.damage = gfa_sim::cars::WRECKED;
        c.fuse = 1;
        c.player_kill = mine;
    }
    let before = w.score.score;
    run(&mut w, Buttons::NONE, 5);
    // One wreck was his, the other somebody else's: 100 points, not 200.
    assert_eq!(w.score.score - before, 100);
}

#[test]
fn a_wreck_pays_only_within_seven_blocks_of_the_player() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..35, 5..35);
    let mut w = world(&b, south(20, 20));
    w.traffic.clear();
    w.peds.clear();
    // His own wrecks, 7 blocks off and 8 (`tank_chain`: the chain paid up
    // to the car at 7 blocks, not the one at 8), far apart in time.
    for (blocks, pays) in [(7, 100), (8, 0)] {
        w.park(0, 0, -32 * blocks, 0);
        let c = &mut w.traffic.cars[w.traffic.n - 1];
        c.damage = gfa_sim::cars::WRECKED;
        c.player_kill = true;
        let before = w.score.score;
        run(&mut w, Buttons::NONE, 60);
        assert!(w.traffic.cars[w.traffic.n - 1].wrecked, "{blocks} blocks");
        assert_eq!(w.score.score - before, pays, "{blocks} blocks");
    }
}

/// A street with car 0 made the tank, and him in it at (15,30) facing
/// north, nobody else about.
fn in_tank() -> World {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..35, 5..45);
    let mut w = world(&b, south(15, 30));
    w.peds.clear();
    w.traffic.clear();
    w.cars[0].model = gfa_sim::vehicle::TANK_MODEL;
    w.put_in_car(0, 15, 30, 0);
    w.drain_log();
    w
}

#[test]
fn the_tank_fires_only_in_a_frenzy_a_shell_every_24_frames() {
    let mut w = in_tank();
    run(&mut w, Buttons::FIRE, 100);
    assert_eq!(w.weapons.stats.fired, 0, "no frenzy, no cannon");
    w.start_frenzy();
    assert_eq!(w.cannon.shells, gfa_sim::weapons::SHELLS);
    run(&mut w, Buttons::NONE, 2);
    // Held for 150 ticks, 70 game frames: shells on frames 0, 24 and 48.
    run(&mut w, Buttons::FIRE, 150);
    assert_eq!(w.weapons.stats.fired, 3);
    assert_eq!(w.cannon.shells, gfa_sim::weapons::SHELLS - 3);
    // The last shell, then nothing.
    w.cannon.shells = 1;
    run(&mut w, Buttons::FIRE, 150);
    assert_eq!(w.weapons.stats.fired, 4);
    assert_eq!(w.cannon.shells, 0);
}

#[test]
fn a_shell_leaves_the_muzzle_and_speeds_up_by_three_to_seventeen() {
    use gfa_sim::weapons::{Kind, MUZZLE_PX};
    let mut w = in_tank();
    w.start_frenzy();
    let y0 = w.driven.as_ref().unwrap().veh.oy;
    let mut ys = Vec::new();
    for _ in 0..30 {
        w.step(press(Buttons::FIRE));
        if let Some(s) = w
            .weapons
            .shots
            .iter()
            .find(|s| s.alive && s.kind == Kind::Shell)
            && ys.last() != Some(&s.y)
        {
            ys.push(s.y);
        }
    }
    // Spawned at the muzzle and flown its first step, 7 half-px, at once.
    assert_eq!(ys[0], y0 - (MUZZLE_PX << 16) - (7 << 15));
    let moves: Vec<i64> = ys.windows(2).map(|p| (p[0] - p[1]) >> 15).collect();
    assert_eq!(moves[..6], [10, 13, 16, 17, 17, 17], "{moves:?}");
}

#[test]
fn tab_with_left_or_right_turns_the_turret_eight_a_frame_and_not_the_hull() {
    let mut w = in_tank();
    let hull = w.driven.as_ref().unwrap().veh.angle();
    // 15 ticks are 7 game frames: 56 clockwise, and the hull stays put.
    run(&mut w, Buttons::SPECIAL.with(Buttons::RIGHT, true), 15);
    let d = w.driven.as_ref().unwrap();
    assert_eq!(d.turret, 56);
    assert_eq!(d.veh.angle(), hull);
    // Back the other way, past ahead.
    run(&mut w, Buttons::SPECIAL.with(Buttons::LEFT, true), 30);
    let d = w.driven.as_ref().unwrap();
    assert_eq!(d.turret, (56 - 14 * 8) & 255);
    assert_eq!(d.veh.angle(), hull);
    // A shell goes where the turret points, not the hull.
    w.start_frenzy();
    run(&mut w, Buttons::FIRE, 3);
    assert!(log_has(
        &mut w,
        &format!("shell fired at angle {}", (56 - 14 * 8) & 255)
    ));
}

#[test]
fn a_shot_car_blows_up_a_frame_later_and_one_in_its_blast_ten_frames_after() {
    let mut w = in_tank();
    // The target 3 blocks ahead; a second car beside and beyond it, inside
    // the target's blast but not the shell's own burst on its tail.
    w.park(0, 0, -96, 0);
    w.park(0, 32, -124, 0);
    let (a, b) = (w.traffic.n - 2, w.traffic.n - 1);
    w.start_frenzy();
    w.drain_log();
    let before = w.score.score;
    let (mut burst, mut up_a, mut up_b) = (None, None, None);
    for t in 0..120 {
        w.step(press(if t < 3 { Buttons::FIRE } else { Buttons::NONE }));
        if burst.is_none() && log_has(&mut w, "shell burst on car") {
            burst = Some(t);
        }
        if up_a.is_none() && w.traffic.cars[a].wrecked {
            up_a = Some(t);
        }
        if up_b.is_none() && w.traffic.cars[b].wrecked {
            up_b = Some(t);
        }
    }
    let (burst, up_a, up_b) = (burst.unwrap(), up_a.unwrap(), up_b.unwrap());
    // One game frame is 2 or 3 ticks; ten are 21 or 22.
    assert!((1..=3).contains(&(up_a - burst)), "{burst} -> {up_a}");
    assert!((21..=22).contains(&(up_b - up_a)), "{up_a} -> {up_b}");
    // The hit pays nothing of its own; the two wrecks 100, then 200.
    assert_eq!(w.score.score - before, 300);
}

/// The tank drives straight through a bus and a car: each it touches is
/// written off, blows up on the next game frame and pays as his wreck, and
/// nothing stops, pushes or dents it (`GTA8.EXE`, the owner's play).
#[test]
fn the_tank_writes_off_every_car_it_touches_and_drives_on() {
    let mut w = in_tank();
    w.cars.push(saloon());
    // A bus across the road 3 blocks ahead, and a saloon 4 blocks past it,
    // out of the bus's blast.
    w.park(1, 0, -96, 64);
    w.park(2, 0, -224, 64);
    let (bus, car) = (w.traffic.n - 2, w.traffic.n - 1);
    w.drain_log();
    let before = w.score.score;
    let (mut crushed, mut up) = (Vec::new(), [None, None]);
    let mut speeds = Vec::new();
    // Through both, and stopped short of the wall at the street's end.
    for t in 0..90u32 {
        w.step(press(Buttons::UP));
        let v = &w.driven.as_ref().expect("wasted in the tank").veh;
        speeds.push(v.vx.abs() + v.vy.abs());
        if w.drain_log()
            .iter()
            .any(|l| l.text.contains("car: the tank crushed 1"))
        {
            crushed.push(t);
        }
        for (k, i) in [bus, car].into_iter().enumerate() {
            if up[k].is_none() && w.traffic.cars[i].wrecked {
                up[k] = Some(t);
            }
        }
    }
    assert_eq!(crushed.len(), 2, "one crush a car: {crushed:?}");
    for (k, at) in crushed.iter().enumerate() {
        let gone = up[k].expect("never blew up");
        // On the first game frame from the touch: the same tick if it is
        // one (the sweep comes after the drive), else within 3 ticks.
        assert!((0..=3).contains(&(gone - at)), "{at} -> {gone}");
    }
    let d = w.driven.as_ref().expect("still in the tank");
    assert_eq!(d.veh.damage, 0, "the tank took no dent");
    // The blasts against it did him no harm.
    assert_eq!(w.health, 100);
    let y = d.veh.oy >> 16;
    assert!(
        y < (w.traffic.cars[car].y >> 16) - 64,
        "drove on through the wrecks: at {y}"
    );
    // Not slowed by either: no more than the engine cut's dither at the
    // top speed over the tick it touched.
    for &t in &crushed {
        let (was, now) = (speeds[t as usize - 1], speeds[t as usize + 2]);
        assert!(now * 10 >= was * 9, "slowed at {t}: {was} -> {now}");
    }
    // Each pays as his wreck, the second doubled in the same streak.
    assert_eq!(w.score.score - before, 300);
}

/// The tank runs over the people in its way (`GTA8.EXE`, the owner's
/// play). Slow as it is, it is past the speed a car needs to kill within a
/// few ticks of pulling away, so the car's rule gives it.
#[test]
fn the_tank_runs_people_over() {
    let mut w = in_tank();
    let (x, y) = (w.player.x as i64, w.player.y as i64);
    assert!(w.peds.drop_ped(x, y - (60 << 16), 2, 128, -1, 0));
    w.drain_log();
    let mut ran_over = false;
    for _ in 0..100 {
        w.step(press(Buttons::UP));
        if log_has(&mut w, "car: ran over 1") {
            ran_over = true;
            break;
        }
    }
    assert!(ran_over, "the man in its path lived");
    assert!(w.peds.p.iter().any(|p| p.corpse));
}

/// A rocket writes off a tank as it does any car (ours: `GTA8.EXE` makes
/// the tank vanish and then stops with `ADDING TWICE`, a crash not copied).
#[test]
fn a_rocket_writes_off_a_tank_like_any_car() {
    use gfa_sim::weapons::weapon;
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..35, 5..45);
    let mut w = world(&b, south(15, 15));
    w.peds.clear();
    w.traffic.clear();
    w.cars[0].model = gfa_sim::vehicle::TANK_MODEL;
    w.park(0, 0, 64, 64);
    let tank = w.traffic.n - 1;
    w.give_weapon(weapon::ROCKET as i32, 1);
    w.drain_log();
    for t in 0..80 {
        w.step(press(if t == 0 { Buttons::FIRE } else { Buttons::NONE }));
    }
    assert!(w.traffic.cars[tank].wrecked, "the tank stood the rocket");
}

#[test]
fn the_tenth_punch_that_lands_is_countered_and_puts_the_player_down_two_seconds() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.peds.clear();
    let mut counter_at = None;
    for n in 1..=12 {
        // A fresh man walking up to him, 16 px ahead, so that he is in the
        // fist's box when the blow lands eight ticks on; then time to
        // get up again.
        w.drop_ped(0, 16, 0);
        run(&mut w, Buttons::FIRE, 30);
        run(&mut w, Buttons::NONE, 130);
        if log_has(&mut w, "punched back") && counter_at.is_none() {
            counter_at = Some(n);
        }
    }
    assert_eq!(counter_at, Some(10), "tenth contact hits back");
    // The wheel starts again: the eleventh and twelfth blows land.
    assert!(
        w.peds.stats.punched >= 11,
        "punched {}",
        w.peds.stats.punched
    );
}

#[test]
fn friendly_fire_groups_and_gouranga_bonus() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..30);
    let mut w = world(&b, south(15, 10));
    w.peds.clear();

    // Two cops in a line: a shot owned by cop 0 cannot hit cop 1.
    let (px, py, pl) = (i64::from(w.player.x), i64::from(w.player.y), w.player.layer);
    assert!(w.peds.spawn_cop(px, py + (10 << 16), pl, 128));
    let c0 = w.peds.last.unwrap();
    assert!(w.peds.spawn_cop(px, py + (16 << 16), pl, 128));
    let c1 = w.peds.last.unwrap();
    assert_eq!(
        w.peds
            .hit_segment(px, py + (10 << 16), 128, 20, 2, pl, Some(c0)),
        None,
        "cops do not hit other cops"
    );
    assert_eq!(
        w.peds
            .hit_segment(px, py + (10 << 16), 128, 20, 2, pl, None),
        Some(c0),
        "player shots still hit cops"
    );
    let _ = c1;

    // GOURANGA! is the 7th Hare Krishna in a streak (GTA8.EXE), not any 4
    // people: 4 passers-by run over pay none.
    w.peds.clear();
    w.put_in_car(0, 15, 8, 128);
    run(&mut w, Buttons::UP, 60);
    let v = w.driven.as_ref().unwrap().veh.clone();
    for k in 0..4 {
        w.peds
            .drop_ped(v.ox, v.oy + ((36 + k * 4) << 16), 2, 0, -1, 0);
    }
    run(&mut w, Buttons::UP, 25);
    let log = w.drain_log();
    assert!(log.iter().any(|l| l.text.contains("car: ran over")));
    assert!(!log.iter().any(|l| l.text.contains("GOURANGA")));
}

#[test]
fn a_hare_krishna_line_run_over_pays_25000_with_gouranga() {
    use gfa_sim::peds::KRISHNA_REMAP;
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..30);
    let mut w = world(&b, south(15, 10));
    w.peds.clear();
    w.put_in_car(0, 15, 8, 128);
    run(&mut w, Buttons::UP, 60);
    let v = w.driven.as_ref().unwrap().veh.clone();
    assert_eq!(w.score.score, 0);
    let leader = w.peds.spawn_line(v.ox, v.oy + (40 << 16), 2, 0).unwrap();
    assert_eq!(w.peds.leader, Some(leader));
    let line: Vec<_> = w.peds.p.iter().filter(|p| p.in_line()).collect();
    assert_eq!(line.len(), 7);
    assert!(line.iter().all(|p| p.remap == KRISHNA_REMAP));
    assert!(
        w.peds.spawn_line(v.ox, v.oy, 2, 0).is_none(),
        "one line at a time"
    );
    run(&mut w, Buttons::UP, 30);
    let log = w.drain_log();
    let said = |t: &str| log.iter().any(|l| l.text.contains(t));
    assert!(said("score: GOURANGA! +12300"), "{log:#?}");
    assert!(said("the line scatters"));
    // 100, 200, 400 … 6400, and 12,300 more.
    assert_eq!(w.score.score, 25_000);
    assert_eq!((w.peds.leader, w.peds.made), (None, 0));
}

/// A walled plaza of pavement 14..27 square in a sea of road: the crowd's
/// spawn points, a view's width away, all fall on the road.
fn plaza() -> CmpBuilder {
    let mut b = CmpBuilder::new(1);
    let floor = Block {
        faces: [0, 0, 0, 0, 1],
        ..Block::default()
    };
    for x in 0..60 {
        for y in 0..60 {
            let kind = if (14..27).contains(&x) && (14..27).contains(&y) {
                PAVEMENT
            } else {
                ROAD
            };
            b.set(x, y, 1, floor).set(x, y, 2, block(kind));
        }
    }
    b
}

#[test]
fn the_hundredth_passer_by_starts_a_hare_krishna_line() {
    use gfa_sim::peds::{KRISHNA_REMAP, line, mode};
    use gfa_sim::score::kind;
    let mut w = world(&pavements(), south(20, 25));
    w.peds.made = 98;
    let made = (0..300).find(|_| {
        w.step(TickInput::default());
        w.peds.leader.is_some()
    });
    assert!(made.is_some(), "no line by the 100th");
    let l = w.peds.leader.unwrap();
    assert_eq!(w.peds.made, 100);
    // One chain, each after the one made before him, back to the leader.
    let members: Vec<usize> = (0..w.peds.p.len())
        .filter(|&i| w.peds.p[i].in_line())
        .collect();
    assert_eq!(members.len(), 7);
    for &i in &members {
        let p = &w.peds.p[i];
        assert_eq!(p.remap, KRISHNA_REMAP);
        assert_eq!(p.score_kind(), kind::KRISHNA);
        let mut at = i;
        for _ in 0..7 {
            if at == l {
                break;
            }
            at = w.peds.p[at].ahead as usize;
        }
        assert_eq!(at, l, "follower {i} is not in the leader's chain");
    }
    // No second line while he lives, and the line is never let go.
    run(&mut w, Buttons::NONE, 400);
    assert_eq!(w.peds.leader, Some(l));
    assert!(w.peds.made > 100);
    let line_now = w.peds.p.iter().filter(|p| p.in_line()).count();
    assert_eq!(line_now, 7);
    // With the leader dead the rest scatter, still Krishnas, and the count
    // starts again.
    w.peds.kill(l);
    w.step(TickInput::default());
    assert_eq!(w.peds.leader, None);
    assert!(w.peds.made < 5);
    let scattered: Vec<_> = w
        .peds
        .p
        .iter()
        .filter(|p| p.alive && p.line == line::LEADERLESS)
        .collect();
    assert_eq!(scattered.len(), 6);
    assert!(scattered.iter().all(|p| p.mode == mode::FLEE));
    assert!(scattered.iter().all(|p| p.score_kind() == kind::KRISHNA));
}

/// A staircase of pavement one block wide in a sea of road, turning every
/// three blocks, right and left in turn (the `corners` arena's): from
/// (10,10) east, south, east … to (22,19).
fn staircase() -> CmpBuilder {
    let mut b = CmpBuilder::new(1);
    let floor = Block {
        faces: [0, 0, 0, 0, 1],
        ..Block::default()
    };
    for x in 0..70 {
        for y in 0..70 {
            b.set(x, y, 1, floor).set(x, y, 2, block(ROAD));
        }
    }
    for k in 0..4u8 {
        let (x0, y0) = (10 + 3 * k, 10 + 3 * k);
        for d in 0..=3 {
            b.set(x0 + d, y0, 2, block(PAVEMENT));
            if k < 3 {
                b.set(x0 + 3, y0 + d, 2, block(PAVEMENT));
            }
        }
    }
    b
}

#[test]
fn a_hare_krishna_line_keeps_to_its_leaders_path() {
    // The player well away, so nobody else comes on the path.
    let mut w = world(&staircase(), south(55, 55));
    w.peds.clear();
    let at = |b: i64| (b * 32 + 16) << 16;
    let l = w.peds.spawn_line(at(11), at(10), 2, 64).unwrap();
    let mut worst_gap = 0;
    // Every block the leader has stood on: the followers go nowhere else
    // (the owner's K9: no corner cut, on the road or into a building).
    let mut trail = std::collections::BTreeSet::new();
    let mut corners = 0;
    let mut heading = w.peds.p[l].angle & !63;
    for t in 0..3000 {
        w.step(TickInput::default());
        assert_eq!(w.peds.leader, Some(l), "the leader was lost at tick {t}");
        let q = &w.peds.p[l];
        trail.insert((q.x >> 21, q.y >> 21));
        let h = (q.angle + 32) & 255 & !63;
        if h != heading {
            corners += 1;
            heading = h;
        }
        for (i, p) in w.peds.p.iter().enumerate() {
            if !p.in_line() || p.line != gfa_sim::peds::line::FOLLOWER {
                continue;
            }
            assert!(
                trail.contains(&(p.x >> 21, p.y >> 21)),
                "Krishna {i} at ({},{}) on a block the leader never stood on, tick {t}",
                p.x >> 16,
                p.y >> 16
            );
            if t > 100 && p.ahead >= 0 {
                let a = &w.peds.p[p.ahead as usize];
                let gap = ((a.x - p.x) >> 16).abs().max(((a.y - p.y) >> 16).abs());
                worst_gap = worst_gap.max(gap);
            }
        }
    }
    // Round a good few of the corners, and nobody left behind.
    assert!(
        trail.len() >= 10 && corners >= 4,
        "the leader turned {corners} times on {trail:?}"
    );
    assert_eq!(w.peds.p.iter().filter(|p| p.in_line()).count(), 7);
    assert!(worst_gap <= 24, "a follower fell {worst_gap} px behind");
}

#[test]
fn the_krishna_leader_follows_the_player_who_stands_by_him() {
    let mut w = world(&plaza(), south(20, 20));
    w.peds.clear();
    let at = |b: i64| (b * 32 + 16) << 16;
    let l = w.peds.spawn_line(at(17), at(17), 2, 64).unwrap();
    run(&mut w, Buttons::NONE, 120);
    assert!(!w.peds.p[l].on_player);
    // The player steps up to him, nearer than the first behind him.
    let p = w.peds.p[l];
    w.player.x = (p.x + (5 << 16)) as i32;
    w.player.y = p.y as i32;
    w.player.layer = p.layer;
    w.step(TickInput::default());
    assert!(
        w.peds.p[l].on_player,
        "the leader did not take up after him"
    );
    assert!(log_has(&mut w, "follows the player"));
    // He walks off; the leader keeps up.
    w.player.x += 3 << 21;
    run(&mut w, Buttons::NONE, 200);
    let p = w.peds.p[l];
    let d = ((i64::from(w.player.x) - p.x) >> 16)
        .abs()
        .max(((i64::from(w.player.y) - p.y) >> 16).abs());
    assert!(d <= 16, "the leader is {d} px off");
    // Into a car: he lets the player go.
    w.put_in_car(0, 22, 22, 0);
    run(&mut w, Buttons::NONE, 2);
    assert!(!w.peds.p[l].on_player);
    assert!(log_has(&mut w, "lets the player go"));
}

#[test]
fn weapon_fire_rates_projectile_speed_and_splat_lifetime_match_original() {
    use gfa_sim::weapons::weapon;

    let open = || {
        let mut b = CmpBuilder::new(1);
        street(&mut b, 5..45, 5..45);
        let mut w = world(&b, south(10, 10));
        w.peds.clear();
        w.traffic.clear();
        w
    };

    // Pistol: 7 shots over 150 ticks (3.0 s = 70 weapon steps at 23.333 Hz);
    // bullet travels 7.5 wpx per weapon step and expires after 20 weapon steps (~43 ticks).
    let mut w = open();
    w.give_weapon(weapon::PISTOL as i32, 20);
    let mut positions = Vec::new();
    let mut bullet_alive_ticks = 0u32;
    let mut bullet_done = false;
    for t in 0..150 {
        w.step(press(Buttons::FIRE));
        if w.weapons.stats.fired >= 1 && !bullet_done {
            if w.weapons.shots[0].alive {
                bullet_alive_ticks += 1;
                positions.push((t, w.weapons.shots[0].y));
            } else {
                bullet_done = true;
            }
        }
    }
    assert_eq!(w.arms.ammo[weapon::PISTOL], 13);
    assert_eq!(w.weapons.stats.fired, 7);
    assert!(
        (40..=45).contains(&bullet_alive_ticks),
        "pistol bullet lived {bullet_alive_ticks} sim ticks (expected 40 = visible across 20 @ 23.333 Hz steps)"
    );
    let weapon_moves: Vec<i64> = positions
        .windows(2)
        .filter_map(|p| (p[1].1 != p[0].1).then_some((p[1].1 - p[0].1) >> 15))
        .collect();
    // Step 0 spawns & flies (life 20->19); steps 1..18 record 18 more moves; step 19 expires it.
    assert_eq!(weapon_moves.len(), 18);
    assert!(weapon_moves.iter().all(|&d| d == 15), "{weapon_moves:?}");

    // Machine gun: 70 rounds in 150 ticks (23.333/s, 5 rounds per ammo unit),
    // never spawning two bullets on the same tick.
    let mut w = open();
    w.give_weapon(weapon::MACHINE_GUN as i32, 20);
    let mut prev_fired = 0;
    for _ in 0..150 {
        w.step(press(Buttons::FIRE));
        let delta = w.weapons.stats.fired - prev_fired;
        assert!(delta <= 1, "at most 1 MG bullet spawned per tick ({delta})");
        prev_fired = w.weapons.stats.fired;
    }
    assert_eq!(w.weapons.stats.fired, 70);
    assert_eq!(w.arms.ammo[weapon::MACHINE_GUN], 6);

    // Rocket launcher: 4 rockets in 150 ticks (weapon steps 0, 20, 40, 60 = ~1.17/s).
    let mut w = open();
    w.give_weapon(weapon::ROCKET as i32, 20);
    run(&mut w, Buttons::FIRE, 150);
    assert_eq!(w.weapons.stats.fired, 4);
    assert_eq!(w.arms.ammo[weapon::ROCKET], 16);

    // Flamethrower: single puff is visible for 9 weapon steps (~19 sim ticks / 380 ms)
    // and expires on the 10th step, matching `fl_single` (9 game frames / 385 ms).
    let mut w = open();
    w.give_weapon(weapon::FLAME as i32, 20);
    let mut flame_ticks = 0u32;
    for _ in 0..60 {
        let btn = if w.weapons.stats.fired == 0 {
            Buttons::FIRE
        } else {
            Buttons::NONE
        };
        w.step(press(btn));
        if w.weapons.alive() > 0 {
            flame_ticks += 1;
        } else if flame_ticks > 0 {
            break;
        }
    }
    assert_eq!(w.weapons.stats.fired, 1);
    assert!(
        (18..=22).contains(&flame_ticks),
        "flame puff lived {flame_ticks} sim ticks (expected ~19 = 9 visible steps @ 23.333 Hz)"
    );

    // Shots fly on over water on their own layer, to the wall beyond it.
    let over_water = || {
        let mut b = CmpBuilder::new(1);
        street(&mut b, 5..20, 5..20);
        for x in 5..20 {
            for y in 12..14 {
                b.set(x, y, 2, block(0x10));
            }
        }
        b.set(10, 14, 2, block(BUILDING));
        let mut w = world(&b, south(10, 10));
        w.peds.clear();
        w.traffic.clear();
        w
    };
    for (gun, hit) in [
        (weapon::ROCKET, "rocket burst on a wall at block (10,14,"),
        (weapon::PISTOL, "hit a wall at"),
    ] {
        let mut w = over_water();
        w.give_weapon(gun as i32, 5);
        let mut seen = Vec::new();
        for t in 0..60 {
            w.step(press(if t == 0 { Buttons::FIRE } else { Buttons::NONE }));
            seen.extend(w.drain_log().into_iter().map(|l| l.text));
        }
        assert!(
            seen.iter()
                .any(|l| l.contains(hit) && l.contains("(10,14,")),
            "weapon {gun}: {seen:?}"
        );
        assert!(!seen.iter().any(|l| l.contains("water")), "{seen:?}");
    }

    // A rocket into a wall leaves one fire on this side of it, 10 px back
    // along its line, which burns for 1100 ticks (GTA8.EXE, the owner's
    // play; AmiGTA1 lights three for 250).
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..20, 5..20);
    b.set(10, 14, 2, block(BUILDING));
    let mut w = world(&b, south(10, 10));
    w.peds.clear();
    w.traffic.clear();
    w.give_weapon(weapon::ROCKET as i32, 1);
    let burning = |w: &gfa_sim::world::World| {
        w.weapons
            .fires
            .iter()
            .filter(|f| f.ticks > 0)
            .copied()
            .collect::<Vec<_>>()
    };
    let mut lit = None;
    for t in 0..1300u32 {
        w.step(press(if t == 0 { Buttons::FIRE } else { Buttons::NONE }));
        let fires = burning(&w);
        match lit {
            None if !fires.is_empty() => {
                assert_eq!(fires.len(), 1, "one fire, not AmiGTA1's three");
                let f = fires[0];
                assert_eq!(f.ticks, 1100);
                assert_eq!(f.layer, 2);
                assert!(
                    ((f.x >> 16) - (10 * 32 + 16)).abs() <= 1,
                    "on the rocket's line: x {}",
                    f.x >> 16
                );
                let back = 14 * 32 - (f.y >> 16);
                // 10 px back from where the rocket was before the wall,
                // up to one 7.5 px step short of it.
                assert!((10..=18).contains(&back), "{back} px short of the wall");
                lit = Some(t);
            }
            Some(at) if fires.is_empty() => {
                assert_eq!(t - at, 1100, "burnt for {} ticks", t - at);
                lit = None;
                break;
            }
            _ => {}
        }
    }
    assert!(lit.is_none(), "the fire never lit or never went out");

    // A rocket's blast that sets a man alight without killing him pays when
    // he has burnt to death, as burnt (GTA8.EXE, the owner's play): 1000 for
    // a passer-by, nothing before.
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..20, 5..20);
    b.set(10, 14, 2, block(BUILDING));
    let mut w = world(&b, south(10, 10));
    w.peds.clear();
    w.traffic.clear();
    // Lying 24 px to the side of the wall's face: past the blast's 17 px,
    // inside its 30.
    let at = ((336 + 24) << 16, 440 << 16);
    assert!(w.peds.drop_ped(at.0, at.1, 2, 0, -1, 0));
    let i = w
        .peds
        .p
        .iter()
        .position(|p| p.alive && (p.x, p.y) == at)
        .unwrap();
    w.peds.p[i].down = 10_000;
    w.give_weapon(weapon::ROCKET as i32, 5);
    let mut alight = None;
    let mut paid = None;
    for t in 0..300 {
        w.step(press(if t == 0 { Buttons::FIRE } else { Buttons::NONE }));
        let log = w.drain_log();
        if alight.is_none() && w.peds.p[i].burn > 0 {
            alight = Some(t);
            assert_eq!(w.score.score, 0, "paid as he caught fire");
        }
        if log
            .iter()
            .any(|l| l.text.contains("burnt to death, 1000 points"))
        {
            paid = Some(t);
            break;
        }
    }
    let (alight, paid) = (
        alight.expect("never caught fire"),
        paid.expect("never paid"),
    );
    assert!(w.peds.p[i].corpse);
    assert_eq!(paid - alight, gfa_sim::peds::BURN_TICKS as u32);
    assert_eq!(w.score.score, 1000);

    // Bullet splat against a building wall lasts 5 weapon steps (~10-11 sim ticks / ~214 ms).
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..20, 5..20);
    b.set(10, 12, 2, block(BUILDING));
    let mut w = world(&b, south(10, 10));
    w.peds.clear();
    w.traffic.clear();
    w.give_weapon(weapon::PISTOL as i32, 5);
    let mut splat_ticks = 0u32;
    let mut saw_initial_five = false;
    for _ in 0..60 {
        let btn = if w.weapons.stats.fired == 0 {
            Buttons::FIRE
        } else {
            Buttons::NONE
        };
        w.step(press(btn));
        let ticks = w.weapons.splats[0].ticks;
        if ticks > 0 {
            if splat_ticks == 0 {
                saw_initial_five = ticks == 5;
            }
            splat_ticks += 1;
        } else if splat_ticks > 0 {
            break;
        }
    }
    assert!(saw_initial_five, "initial splat lifetime should be 5 steps");
    assert!(
        (9..=13).contains(&splat_ticks),
        "splat visible for {splat_ticks} sim ticks (expected ~10-11 = 5 @ 23.333 Hz)"
    );
}

#[test]
fn door_geometry_and_enter_exit_sequences() {
    use gfa_sim::cars::{ENTER_SEQ, PED_CAR_MARGIN, door_point};
    use gfa_sim::vehicle::CLASS_BIKE;

    let sal = saloon();
    let coach = bus();
    // Coach SOUTH (128): front cab door is +25 world px south (`d.rpy / 2`),
    // +13 east (`d.rpx / 2`).
    let (dx, dy) = door_point(&coach, 0, 0, 128);
    assert_eq!((dx >> 16, dy >> 16), (13, 25));
    let (sx, sy) = door_point(&sal, 0, 0, 128);
    assert_eq!((sx >> 16, sy >> 16), (12, -3));

    // Car enter & exit on Coach: cab station, ENTER_SEQ with 27, face - 32 turn,
    // player drawn under car, exit 16..=23 clear of body.
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.peds.clear();
    w.park(1, -20, -25, 128);
    let coach_y = standing(&w, 0).y;
    let cab_y = coach_y + (25 << 16);
    w.step(press(Buttons::ENTER));
    let mut frames = Vec::new();
    let mut saw_under = false;
    let mut saw_pull_angle = false;
    for _ in 0..100 {
        w.step(press(Buttons::NONE));
        if w.doing == Doing::Driving {
            break;
        }
        if let Doing::Entering(e) = w.doing
            && e.walk_t >= e.walk_len
        {
            saw_under |= w.player_under_car();
            let f = w.player_frame().unwrap();
            if frames.last() != Some(&f) {
                frames.push(f);
            }
            // Must stay by the front cab doorway rather than sliding back to cy.
            assert!(
                (i64::from(w.player.y) - cab_y).abs() <= (3 << 16),
                "player dragged along Coach body: y={}, cab={cab_y}",
                w.player.y >> 16
            );
            if f == 27 {
                saw_pull_angle |= w.player.angle == 96;
            }
        } else {
            assert!(!w.player_under_car());
        }
    }
    assert_eq!(w.doing, Doing::Driving);
    assert!(saw_under, "car must draw over entering player");
    assert!(saw_pull_angle, "angle turns face - 32 at door pull (27)");
    assert_eq!(frames, vec![26, 27, 25, 29, 30, 31, 32, 33]);
    assert!(ENTER_SEQ.contains(&27));

    w.step(press(Buttons::ENTER));
    let mut exit_frames = Vec::new();
    let mut exit_under = false;
    for _ in 0..50 {
        w.step(press(Buttons::NONE));
        if w.doing == Doing::OnFoot {
            break;
        }
        exit_under |= w.player_under_car();
        let f = w.player_frame().unwrap();
        if exit_frames.last() != Some(&f) {
            exit_frames.push(f);
        }
    }
    assert_eq!(w.doing, Doing::OnFoot);
    assert!(exit_under);
    assert_eq!(exit_frames, vec![16, 17, 18, 19, 20, 21, 22, 23]);
    assert_eq!(w.player.angle, 96);
    assert!(
        w.traffic
            .car_at(
                &w.cars,
                w.player.x.into(),
                w.player.y.into(),
                w.player.layer,
                PED_CAR_MARGIN,
            )
            .is_none(),
        "exited clear of car margin"
    );

    // Bike mount (81..=83) and dismount (86..=88 facing face - 64 immediately),
    // never drawn under the bike.
    let bike = CarInfo {
        width: 16,
        length: 38,
        vert: 14,
        vtype: CLASS_BIKE,
        model: 2,
        doors: vec![CarDoor {
            rpy: 2,
            rpx: 8,
            object: 0,
            delta: 0,
        }],
        ..saloon()
    };
    let cmp = Cmp::parse(&b.build()).unwrap();
    let mut bw = World::new(
        SimMap::new(cmp, WallTiles::gta(1)),
        vec![sal, coach, bike],
        SimView::GAME,
        south(15, 15),
    );
    bw.drain_log();
    bw.peds.clear();
    bw.park(2, 0, 12, 64);
    bw.step(press(Buttons::ENTER));
    let mut b_in = Vec::new();
    for _ in 0..50 {
        bw.step(press(Buttons::NONE));
        if bw.doing == Doing::Driving {
            break;
        }
        assert!(!bw.player_under_car());
        if let Doing::Entering(e) = bw.doing
            && e.walk_t >= e.walk_len
        {
            let f = bw.player_frame().unwrap();
            if b_in.last() != Some(&f) {
                b_in.push(f);
            }
        }
    }
    assert_eq!(b_in, vec![81, 82, 83]);
    assert_eq!(bw.player_frame(), Some(84));

    bw.step(press(Buttons::ENTER));
    let mut b_out = Vec::new();
    for _ in 0..30 {
        bw.step(press(Buttons::NONE));
        if bw.doing == Doing::OnFoot {
            break;
        }
        assert!(!bw.player_under_car());
        assert_eq!(bw.player.angle, 0, "bike dismount faces face - 64");
        let f = bw.player_frame().unwrap();
        if b_out.last() != Some(&f) {
            b_out.push(f);
        }
    }
    assert_eq!(bw.doing, Doing::OnFoot);
    assert_eq!(b_out, vec![86, 87, 88]);
    assert_eq!(bw.player.angle, 0);
}

#[test]
fn far_side_enter_on_bus_walks_around_body() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.peds.clear();
    // Coach facing SOUTH 20 px east: player is on the non-door (west) flank.
    w.park(1, 20, 0, 128);
    w.step(press(Buttons::ENTER));
    assert!(matches!(w.doing, Doing::Entering(e) if e.over.is_none() && e.via_n == 2));
    for _ in 0..160 {
        w.step(press(Buttons::NONE));
        if w.doing == Doing::Driving {
            break;
        }
        if let Doing::Entering(e) = w.doing
            && (e.walk_t < e.walk_len || e.via_n > 0)
        {
            let ci = &w.cars[e.car.model];
            let (hl, hw) = gfa_sim::cars::half_size(ci);
            assert!(
                !gfa_sim::cars::body_hit(
                    e.car.x.into(),
                    e.car.y.into(),
                    e.car.face,
                    hl,
                    hw,
                    w.player.x.into(),
                    w.player.y.into(),
                    0,
                ),
                "player clipped through Coach body while walking round"
            );
        }
    }
    assert_eq!(w.doing, Doing::Driving);
}

#[test]
fn jump_at_bus_from_outside_slides_under() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.peds.clear();
    w.player.angle = 64;
    // Coach facing SOUTH, west flank 26 px east of him (center +26, hw=10 -> flank at +16).
    w.park(1, 26, 0, 128);
    let bus = standing(&w, 0);
    run(&mut w, Buttons::UP, 20);
    assert!(
        i64::from(w.player.x) < bus.x - (10 << 16),
        "held outside west flank"
    );
    w.step(press(Buttons::UP.with(Buttons::HANDBRAKE, true)));
    assert!(matches!(w.doing, Doing::Sliding(_)));
    assert_eq!(w.player_frame(), Some(41));
    assert!(w.player_under_car());
    run(&mut w, Buttons::NONE, 80);
    assert_eq!(w.doing, Doing::OnFoot);
    assert!(log_has(&mut w, "player: out from under"));
    assert!(
        i64::from(w.player.x) > bus.x + (10 << 16),
        "slid all the way out onto the east flank"
    );
}

#[test]
fn flush_wall_enter_refused_and_vault_turns_off_roof() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    for y in 5..25 {
        b.set(16, y, 2, block(BUILDING));
    }
    let mut w = world(&b, south(15, 15));
    w.peds.clear();
    // Flush east against block 16 (wall at x = 512 px); saloon hw = 7 -> center at +9 px = 505 px.
    w.park(0, 9, 0, 128);
    let car_y = standing(&w, 0).y;
    w.step(press(Buttons::ENTER));
    assert_eq!(w.doing, Doing::OnFoot);
    assert!(log_has(
        &mut w,
        "door blocked by a building - cannot get in"
    ));
    assert_eq!(w.traffic.standing().count(), 1);

    // Run east from the clear west side and press SPACE: vault turns off north onto clear road.
    w.player.x = 488 << 16;
    w.player.angle = 64;
    run(&mut w, Buttons::UP, 10);
    w.step(press(Buttons::UP.with(Buttons::HANDBRAKE, true)));
    assert!(matches!(w.doing, Doing::Vaulting(_)));
    run(&mut w, Buttons::NONE, 80);
    assert_eq!(w.doing, Doing::OnFoot);
    let (bx, by) = w.player.block();
    assert!(bx < 16, "never dropped into east building column {bx}");
    assert_ne!(
        w.map.nav_ground(bx, by, w.player.layer),
        BUILDING as u8 >> 4
    );
    assert!(
        i64::from(w.player.y) < car_y - (10 << 16),
        "turned north along roof off the wall edge"
    );
}

#[test]
fn blocked_door_exit_forward_on_north_car_bus_and_l_corner() {
    // 1. NORTH saloon (face = 0): door on west flank blocked -> vaults straight NORTH (heading 0).
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.peds.clear();
    w.put_in_car(0, 15, 15, 0);
    w.park(0, -16, 0, 0);
    let y0 = i64::from(w.player.y);
    w.step(press(Buttons::ENTER));
    assert!(log_has(
        &mut w,
        "car: door blocked by a car - over the roof, heading 0"
    ));
    run(&mut w, Buttons::NONE, 80);
    assert_eq!(w.doing, Doing::OnFoot);
    assert!(i64::from(w.player.y) < y0 - (15 << 16));

    // 2. SOUTH Coach (model 1, length = 120, cab door at dy = +25): door
    //    blocked -> clears full roof along heading 128.
    let mut bw = world(&b, south(15, 15));
    bw.peds.clear();
    bw.put_in_car(1, 15, 15, 128);
    bw.park(0, 17, 25, 128);
    let by0 = i64::from(bw.player.y);
    bw.step(press(Buttons::ENTER));
    assert!(log_has(
        &mut bw,
        "car: door blocked by a car - over the roof, heading 128"
    ));
    run(&mut bw, Buttons::NONE, 100);
    assert_eq!(bw.doing, Doing::OnFoot);
    assert!(
        i64::from(bw.player.y) > by0 + (28 << 16),
        "cleared front bumper of 120-px Coach instead of stopping mid-roof"
    );

    // 3. SOUTH saloon pinned flush in an L-corner (walls at x = 16 and y = 16).
    let mut lb = CmpBuilder::new(1);
    street(&mut lb, 5..25, 5..25);
    for y in 5..25 {
        lb.set(16, y, 2, block(BUILDING));
    }
    for x in 5..25 {
        lb.set(x, 16, 2, block(BUILDING));
    }
    let mut lw = world(&lb, south(15, 15));
    lw.peds.clear();
    lw.put_in_car(0, 15, 15, 128);
    {
        let v = &mut lw.driven.as_mut().unwrap().veh;
        v.x = 505 << 16;
        v.ox = 505 << 16;
        v.y = 497 << 16;
        v.oy = 497 << 16;
    }
    lw.step(press(Buttons::ENTER));
    assert!(matches!(lw.doing, Doing::Vaulting(_)));
    run(&mut lw, Buttons::NONE, 100);
    assert_eq!(lw.doing, Doing::OnFoot);
    let (bx, by) = lw.player.block();
    assert!(bx < 16 && by < 16, "exited onto clear road at ({bx},{by})");
    assert_ne!(
        lw.map.nav_ground(bx, by, lw.player.layer),
        BUILDING as u8 >> 4
    );
}

#[test]
fn shooting_a_ped_leaves_a_blood_splat_and_pool() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 15));
    w.peds.clear();
    // Place a ped directly south of the player (facing north).
    w.drop_ped(0, 24, 0);
    w.arms.give(gfa_sim::weapons::weapon::PISTOL, 10);

    // Fire until the bullet hits the ped.
    for _ in 0..20 {
        w.step(press(Buttons::FIRE));
        if w.peds.stats.shot > 0 {
            break;
        }
    }
    assert_eq!(w.peds.stats.shot, 1, "bullet should hit the ped");
    assert_eq!(
        w.marks.stat_splats, 1,
        "shooting a ped should immediately leave a blood splat"
    );
    assert!(
        w.marks
            .ground_sprites()
            .iter()
            .any(|m| m.object == gfa_sim::marks::OBJ_SPLAT),
        "OBJ_SPLAT should be in ground_sprites right after the shot"
    );

    // Let the ped finish falling into a corpse so the blood pool also appears.
    run(&mut w, Buttons::NONE, 30);
    assert!(
        w.marks.stat_pools >= 1,
        "shot corpse should also bleed a blood pool"
    );
    assert!(
        w.marks
            .ground_sprites()
            .iter()
            .any(|m| m.object == gfa_sim::marks::OBJ_POOL),
        "OBJ_POOL should be in ground_sprites after the ped dies"
    );
}

#[test]
fn running_over_a_ped_leaves_a_blood_splat_and_bloody_tyre_marks() {
    let mut b = CmpBuilder::new(1);
    street(&mut b, 5..25, 5..25);
    let mut w = world(&b, south(15, 10));
    w.peds.clear();
    w.put_in_car(0, 15, 10, 128);

    // Accelerate straight south (not sliding) so the car is at run-over speed.
    run(&mut w, Buttons::UP, 25);
    assert_eq!(w.marks.stat_bloody, 0);
    assert_eq!(w.marks.stat_splats, 0);

    // Drop a ped right ahead of the moving car and drive straight through.
    w.drop_ped(0, 24, 64);
    run(&mut w, Buttons::UP, 30);

    assert_eq!(w.peds.stats.runover, 1, "car should run over the ped");
    assert_eq!(
        w.marks.stat_splats, 1,
        "running over a ped should leave a blood splat"
    );
    assert!(
        w.marks.stat_bloody >= 4,
        "wheels should leave multiple red bloody tyre marks after running over a ped (got {})",
        w.marks.stat_bloody
    );
    let sprites = w.marks.ground_sprites();
    assert!(
        sprites
            .iter()
            .any(|m| m.object == gfa_sim::marks::OBJ_SPLAT),
        "OBJ_SPLAT should be drawn on the ground"
    );
    assert!(
        sprites
            .iter()
            .any(|m| m.object == gfa_sim::marks::OBJ_BLOODY),
        "OBJ_BLOODY tyre marks should be drawn on the ground"
    );
}
