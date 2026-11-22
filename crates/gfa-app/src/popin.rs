//! The pop-in check: nobody and no car comes or goes where the player can
//! see it.
//!
//! The holecheck's counterpart for the simulation. The game runs as the app
//! runs it: 60 frames a second, the follow camera over the simulation's
//! lead at its speed zoom, zoomed out as far as the cap lets it. Between
//! any two frames, no person or car may appear or vanish inside the ground
//! the picture shows, measured on the lowest layer anything stands on, at
//! 21:9 (and wider, pillarboxed), 16:9 and 4:3. Free look and flying are
//! debug views and are not checked.
//!
//! Run with the data tests (`cargo data-test`).

use std::collections::HashMap;

use gfa_assets::level::{self, Level};
use gfa_assets::{DataPath, Dataset, FsSource};
use gfa_render::camera::{FOV_Y_DEGREES, FollowCamera, MAX_ASPECT, max_depth};
use gfa_render::glam::Vec3;
use gfa_sim::input::{Buttons, TickInput};
use gfa_sim::world::Doing;

use crate::controls::MIN_EYE_Z;
use crate::game::Game;

const ASPECTS: [(&str, f32); 3] = [
    ("21:9", MAX_ASPECT),
    ("16:9", 16.0 / 9.0),
    ("4:3", 4.0 / 3.0),
];
const FRAME_US: u64 = 16_667;
/// A person's sprite, from the middle, in map pixels (a sixth of a block).
const PED_RADIUS: f32 = 11.0;

/// Something that can come and go: where it is (map pixels) and how far it
/// reaches from there.
#[derive(Debug, Clone)]
struct Thing {
    at: Vec3,
    reach: f32,
    what: &'static str,
    note: String,
}

/// Everybody and every car but the player's, by a key that stays the same
/// while it is the same thing: a car's serial, or a person's slot and the
/// tick he came on.
fn things(game: &Game, born: &mut [u32], last: &mut [Option<Vec3>]) -> HashMap<(u8, u64), Thing> {
    let w = &game.world;
    let px = |x: i64, y: i64| Vec3::new(x as f32, y as f32, 0.0) / 65536.0 * 2.0;
    let mut out = HashMap::new();
    for c in w.traffic.live() {
        let ci = &w.cars[c.model];
        let reach = f32::from(ci.length).hypot(f32::from(ci.width)) / 2.0;
        let what = if c.wrecked { "wreck" } else { "car" };
        let note = format!(
            "serial {} hold {} wait {} speed {} abandoned {} resident {} swap {}",
            c.serial,
            c.hold,
            c.wait,
            c.speed >> 16,
            c.abandoned,
            c.resident,
            c.swap
        );
        out.insert(
            (0, c.serial),
            Thing {
                at: px(c.x, c.y),
                reach,
                what,
                note,
            },
        );
    }
    for (i, p) in w.peds.p.iter().enumerate() {
        if !p.shown() {
            last[i] = None;
            continue;
        }
        let at = px(p.x, p.y);
        // A slot that jumps further than anybody walks between frames is
        // somebody new in it.
        if last[i].is_none_or(|l| (l - at).abs().max_element() > 16.0) {
            born[i] = w.tick;
        }
        last[i] = Some(at);
        let what = if p.corpse { "body" } else { "person" };
        let key = (1, (i as u64) << 32 | u64::from(born[i]));
        let note = format!("slot {i} cop {} mission {}", p.cop, p.mission);
        out.insert(
            key,
            Thing {
                at,
                reach: PED_RADIUS,
                what,
                note,
            },
        );
    }
    out
}

/// The ground a straight-down camera shows on the plane `z`: centre and
/// half-extents.
fn shown(eye: Vec3, aspect: f32, z: f32) -> (Vec3, f32, f32) {
    let h = (eye.z - z) * (FOV_Y_DEGREES.to_radians() / 2.0).tan();
    (eye, h * aspect, h)
}

fn inside(t: &Thing, (c, hw, hh): (Vec3, f32, f32)) -> bool {
    (t.at.x - c.x).abs() < hw + t.reach && (t.at.y - c.y).abs() < hh + t.reach
}

/// A plain generator for the drive, so the check is the same every run.
struct Dice(u64);
impl Dice {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// The buttons for each tick. In a car: flat out, steering for the road
/// ahead so it gets up to speed, now and then turning off at random;
/// backing out with full lock when stuck. On foot: running, turning now
/// and then.
struct Driver {
    dice: Dice,
    left: u32,
    buttons: Buttons,
    stuck: u32,
}

impl Driver {
    fn tick(&mut self, game: &Game) -> TickInput {
        let w = &game.world;
        let Some(d) = &w.driven else {
            return self.wander();
        };
        let v = &d.veh;
        let moving = v.vx.abs().max(v.vy.abs()) > 1 << 16;
        self.stuck = if moving { 0 } else { self.stuck + 1 };
        if self.stuck > 40 {
            self.stuck = 0;
            let turn = if self.dice.below(2) == 0 {
                Buttons::LEFT
            } else {
                Buttons::RIGHT
            };
            self.buttons = Buttons(Buttons::DOWN.0 | turn.0);
            self.left = 60;
        }
        if self.left > 0 {
            self.left -= 1;
            return TickInput::new(self.buttons);
        }
        if self.dice.below(400) == 0 {
            // Off down a side street, if there is one.
            let turn = if self.dice.below(2) == 0 {
                Buttons::LEFT
            } else {
                Buttons::RIGHT
            };
            self.buttons = Buttons(Buttons::UP.0 | turn.0);
            self.left = 25;
            return TickInput::new(self.buttons);
        }
        // How much road lies along heading `a` (256ths, 0 north, clockwise).
        let road = |a: i32| -> i32 {
            let r = f64::from(a) * std::f64::consts::TAU / 256.0;
            (1..=8)
                .take_while(|&k| {
                    let dist = f64::from(k) * 0.5 * f64::from(32 << 16);
                    let x = (v.ox as f64 + r.sin() * dist) as i32 >> 21;
                    let y = (v.oy as f64 - r.cos() * dist) as i32 >> 21;
                    w.map.ground_type(x, y, d.layer) == gfa_sim::map::ground::ROAD
                })
                .count() as i32
        };
        let a = v.angle();
        let (ahead, l, r) = (road(a), road(a - 20), road(a + 20));
        let turn = if ahead >= 6 || (ahead >= l && ahead >= r) {
            Buttons::NONE
        } else if l > r {
            Buttons::LEFT
        } else {
            Buttons::RIGHT
        };
        TickInput::new(Buttons(Buttons::UP.0 | turn.0))
    }

    fn wander(&mut self) -> TickInput {
        if self.left == 0 {
            let turn = match self.dice.below(6) {
                0 => Buttons::LEFT,
                1 => Buttons::RIGHT,
                _ => Buttons::NONE,
            };
            self.buttons = Buttons(Buttons::UP.0 | turn.0);
            self.left = 10 + self.dice.below(60) as u32;
        }
        self.left -= 1;
        TickInput::new(self.buttons)
    }
}

/// The top of the longest straight two-lane road on the street layer, and
/// the heading down it.
fn longest_straight(map: &gfa_sim::map::SimMap) -> (i32, i32, i32) {
    use gfa_sim::map::ground::ROAD;
    let road = |x: i32, y: i32| map.ground_type(x, y, 2) == ROAD;
    let mut best = (0, (128, 128, 0));
    for a in 0..255 {
        let (mut run_x, mut run_y) = (0, 0);
        for b in 0..256 {
            // Down column `a`, and along row `a`.
            run_y = if road(a, b) && road(a + 1, b) {
                run_y + 1
            } else {
                0
            };
            run_x = if road(b, a) && road(b, a + 1) {
                run_x + 1
            } else {
                0
            };
            if run_y > best.0 {
                best = (run_y, (a, b - run_y + 2, 128));
            }
            if run_x > best.0 {
                best = (run_x, (b - run_x + 2, a, 64));
            }
        }
    }
    best.1
}

/// What a run found.
struct Found {
    /// What came or went in sight.
    bad: Vec<String>,
    /// How many comings and goings were checked.
    checked: usize,
    /// The furthest the speed zoom went out.
    zoom: f32,
}

/// Run `frames` frames of `section`, the player in the style's fastest car
/// (`drive`) or on foot, the camera under the zoom cap or (`capped` false,
/// the control) not.
fn check(section: u32, drive: bool, frames: u32, seed: u64, capped: bool) -> Found {
    let src = FsSource::new(DataPath::resolve(None).expect("needs the game data").root);
    let files = level::find(&src, Dataset::Gta, section).expect("section");
    let level = Level::load(&src, files).expect("level");
    let mut game = Game::new(&level);
    if drive {
        let rec = (0..game.world.cars.len())
            .filter(|&i| game.world.cars[i].vtype == 4)
            .max_by_key(|&i| game.world.cars[i].max_speed)
            .expect("a car");
        // At the top of the longest straight, to get up to full speed.
        let (bx, by, face) = longest_straight(&game.world.map);
        game.world.put_in_car(rec, bx, by, face);
        assert_eq!(game.world.doing, Doing::Driving);
    }
    let floor = game.floor_z();
    let mut cams: Vec<FollowCamera> = ASPECTS
        .iter()
        .map(|_| {
            let mut c = FollowCamera::new(game.frame().follow.pos);
            // As far out as the player can zoom: the cap decides.
            c.height = FollowCamera::MAX_HEIGHT;
            c
        })
        .collect();
    let mut driver = Driver {
        dice: Dice(seed | 1),
        left: 0,
        buttons: Buttons::NONE,
        stuck: 0,
    };
    let n = game.world.peds.p.len();
    let (mut born, mut last) = (vec![0u32; n], vec![None; n]);
    let mut before = things(&game, &mut born, &mut last);
    let mut seen_before: Vec<Option<(Vec3, f32, f32)>> = vec![None; ASPECTS.len()];
    let mut bad = Vec::new();
    let mut checked = 0;
    let mut zoom = 1.0f32;
    let car_at = |game: &Game| {
        game.world
            .driven
            .as_ref()
            .map(|d| Vec3::new(d.veh.ox as f32, d.veh.oy as f32, 0.0) / 65536.0 * 2.0)
    };
    let mut his_car = car_at(&game);
    for _ in 0..frames {
        let input = driver.tick(&game);
        game.advance(FRAME_US, input);
        let f = game.frame();
        zoom = zoom.max(f.zoom);
        let now = things(&game, &mut born, &mut last);
        // His own car handed to the fleet (a write-off burnt out under him)
        // is the same car, not a new one.
        let mut handed = None;
        if let (Some(at), None) = (his_car, car_at(&game)) {
            handed = now
                .iter()
                .find(|(k, t)| !before.contains_key(k) && (t.at - at).length() < 48.0)
                .map(|(k, _)| *k);
        }
        his_car = car_at(&game);
        for (k, (name, aspect)) in ASPECTS.iter().enumerate() {
            let ceiling = floor + max_depth(f.view.0, f.view.1, *aspect);
            let ceiling = capped.then_some(ceiling);
            cams[k].update(f.follow.pos, f.zoom, ceiling, 1.0 / 60.0, MIN_EYE_Z);
            let seen = shown(cams[k].camera().eye, *aspect, floor);
            for (key, t) in &now {
                if !before.contains_key(key) && handed != Some(*key) {
                    // A policeman or medic stepping out of a vehicle door is
                    // coming out of a car already in the scene, not popping in.
                    let from_car = key.0 == 1
                        && (t.note.contains("cop true") || t.note.contains("mission true"))
                        && now
                            .iter()
                            .any(|(ck, ct)| ck.0 == 0 && (ct.at - t.at).length() < 72.0);
                    if from_car {
                        continue;
                    }
                    checked += 1;
                    if inside(t, seen) {
                        bad.push(format!(
                            "tick {} {name}: a {} came on in sight at {:.0},{:.0} (eye {:.0},{:.0} shows ±{:.0},{:.0}; {})",
                            game.world.tick, t.what, t.at.x, t.at.y, seen.0.x, seen.0.y, seen.1, seen.2, t.note
                        ));
                    }
                }
            }
            if let Some(seen_prev) = seen_before[k]
                && (seen.0.truncate() - seen_prev.0.truncate()).length() <= 256.0
            {
                for (key, t) in &before {
                    if !now.contains_key(key) {
                        // A medic boarding his ambulance to leave the scene.
                        let into_car = key.0 == 1
                            && t.note.contains("mission true")
                            && before
                                .iter()
                                .any(|(ck, ct)| ck.0 == 0 && (ct.at - t.at).length() < 72.0);
                        if into_car {
                            continue;
                        }
                        checked += 1;
                        if inside(t, seen_prev) {
                            bad.push(format!(
                                "tick {} {name}: a {} went in sight at {:.0},{:.0} ({})",
                                game.world.tick, t.what, t.at.x, t.at.y, t.note
                            ));
                        }
                    }
                }
            }
            seen_before[k] = Some(seen);
        }
        before = now;
    }
    Found { bad, checked, zoom }
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn nothing_pops_in_or_out_in_sight() {
    let mut all = Vec::new();
    for (section, seed) in [(1u32, 1u64), (102, 2), (202, 3)] {
        for drive in [true, false] {
            let f = check(section, drive, 7_200, seed, true);
            println!(
                "[{section}] {}: {} comings and goings checked, {} in sight; zoom to {:.2}",
                if drive { "driving" } else { "on foot" },
                f.checked,
                f.bad.len(),
                f.zoom
            );
            for b in f.bad.iter().take(8) {
                println!("  {b}");
            }
            assert!(f.checked > 100, "too little came and went to say");
            if drive {
                assert!(f.zoom > 1.4, "the drive never got up to speed");
            }
            all.extend(f.bad);
        }
    }
    assert!(all.is_empty(), "{} came or went in sight", all.len());
}

/// The control: the same drive with the camera zoomed all the way out and
/// no cap, as before the cap, does see people and cars come and go.
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn without_the_cap_things_pop_in() {
    let f = check(1, true, 3_000, 1, false);
    println!("uncapped: {} of {} in sight", f.bad.len(), f.checked);
    assert!(f.bad.len() > 50, "the check should see these");
}
