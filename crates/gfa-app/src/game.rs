//! The simulation inside the app: paced by the wall clock, fed from the
//! keyboard, and drawn between ticks.
//!
//! The simulation itself never sees the clock (Principle 4): the [`Pacer`]
//! turns elapsed wall time into a whole number of ticks, and the renderer
//! interpolates the player between the last two ticks by the leftover
//! fraction. The player's car is drawn on its own physics clock instead
//! ([`CarClock`]): it moves on 7 ticks in 15, and drawn between ticks it
//! would move on one and stand still on the next.

use gfa_assets::Dataset;
use gfa_assets::level::Level;
use gfa_render::glam::Vec3;
use gfa_sim::clock::TICK_US;
use gfa_sim::input::{InputLog, TickInput};
use gfa_sim::map::{SimMap, WallTiles};
use gfa_sim::traffic::roads::{Lamp, Light};
use gfa_sim::view::{SimCamera, SimView, ZOOM_ONE};
use gfa_sim::world::{Doing, Start, World};

/// Most ticks run for one frame. A frame that took longer (a stall, a
/// debugger stop) drops the rest rather than paying it back, which would
/// make the next frame slower still. AmiGTA1 uses 6.
pub const MAX_CATCHUP: u32 = 6;

/// Turns wall time into ticks.
#[derive(Debug, Default)]
pub struct Pacer {
    /// Microseconds owed to the simulation.
    owed: u64,
}

impl Pacer {
    /// Ticks to run after `elapsed_us` more microseconds of wall time.
    pub fn ticks(&mut self, elapsed_us: u64) -> u32 {
        let tick = u64::from(TICK_US);
        self.owed = (self.owed + elapsed_us).min(tick * u64::from(MAX_CATCHUP));
        let n = self.owed / tick;
        self.owed -= n * tick;
        n as u32
    }

    /// How far into the next tick wall time is, 0..1: for interpolation.
    pub fn alpha(&self) -> f32 {
        self.owed as f32 / TICK_US as f32
    }
}

/// Something drawn between two ticks.
#[derive(Debug, Clone, Copy)]
pub struct Pose {
    /// Render pixels (64 to a block).
    pub pos: Vec3,
    /// Radians clockwise, for the sprite (the art faces south).
    pub draw_angle: f32,
    /// Render pixels per second, for the camera's lead.
    pub velocity: Vec3,
}

/// What to draw of the simulation this frame, and what the camera follows.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    /// The player and his ped sprite frame, unless he is hidden in a car.
    pub player: Option<(Pose, u16)>,
    /// The player's car: pose, car table record and colour remap.
    pub car: Option<(Pose, usize, u8)>,
    /// What the camera centres on: the player (or his car) as drawn, plus
    /// the simulation's lead.
    pub follow: Pose,
    /// The simulation's speed zoom between the last two ticks: 1.0 at rest.
    pub zoom: f32,
    /// The simulation's view now: half-extents in blocks. The camera shows
    /// no more than this.
    pub view: (i32, i32),
}

/// Ticks between the driven car's physics steps (it steps at 23.3 Hz).
const STEP_TICKS: f64 = 15.0 / 7.0;

/// The driven car at its last two physics steps, and when the last one
/// happened, for drawing it on the physics clock: between steps it carries
/// on at the last step's velocity and turn, so it moves on every frame.
/// Render only; the simulation never reads it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct CarClock {
    /// The car's 7/15 accumulator after the last tick; `None` on foot.
    accum: Option<i32>,
    /// When the last step happened, in ticks (fractional).
    at: f64,
    /// `(x, y, ang16)` at the last step and at the step before it.
    last: (i64, i64, i64),
    before: (i64, i64, i64),
}

/// The short way round from `a` to `b`, in `ang16` units (16.16 of 256).
fn ang16_turn(a: i64, b: i64) -> i64 {
    let d = (b - a) & 0xFF_FFFF;
    if d > 0x80_0000 { d - 0x100_0000 } else { d }
}

impl CarClock {
    /// Call after every tick.
    fn track(&mut self, world: &World) {
        let Some(d) = &world.driven else {
            *self = CarClock::default();
            return;
        };
        let v = &d.veh;
        let pose = (v.ox, v.oy, v.ang16);
        let acc = v.phase();
        let now = f64::from(world.tick);
        match self.accum {
            None => {
                self.at = now;
                self.last = pose;
                self.before = pose;
            }
            // The accumulator wrapped: a step, `acc / 7` ticks ago.
            Some(a) if acc != a + 7 => {
                self.before = self.last;
                self.last = pose;
                self.at = now - f64::from(acc) / 7.0;
            }
            // Moved between steps (pushed by a car, eased off a wall):
            // the same velocity from where it is now.
            Some(_) => {
                let (dx, dy) = (pose.0 - self.last.0, pose.1 - self.last.1);
                let da = ang16_turn(self.last.2, pose.2);
                self.last = pose;
                self.before = (self.before.0 + dx, self.before.1 + dy, self.before.2 + da);
            }
        }
        self.accum = Some(acc);
    }

    /// The car at `t` ticks: x and y (16.16), heading (256ths) and the
    /// velocity per tick (16.16). `None` when there is no car, or the last
    /// step was a jump no car makes (a teleport).
    fn at_time(&self, t: f64) -> Option<(f64, f64, f64, f64, f64)> {
        self.accum?;
        let (vx, vy) = (self.last.0 - self.before.0, self.last.1 - self.before.1);
        if vx.abs().max(vy.abs()) > 64 << 16 {
            return None;
        }
        let turn = ang16_turn(self.before.2, self.last.2) as f64;
        // Up to a step either side of the last one: behind it is between
        // the two steps, ahead of it is where the next one will be.
        let k = ((t - self.at) / STEP_TICKS).clamp(-1.0, 1.0);
        Some((
            self.last.0 as f64 + vx as f64 * k,
            self.last.1 as f64 + vy as f64 * k,
            (self.last.2 as f64 + turn * k) / 65536.0,
            vx as f64 / STEP_TICKS,
            vy as f64 / STEP_TICKS,
        ))
    }
}

/// Positions (16.16, z included) and headings (256 steps) at a tick.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Snapshot {
    player: ([i32; 3], i32),
    car: Option<([i32; 3], i32)>,
    camera: SimCamera,
}

impl Snapshot {
    fn of(world: &World) -> Self {
        Snapshot {
            camera: world.camera,
            player: (world.player_pos(), world.player.angle),
            car: world.driven.as_ref().map(|d| {
                let v = &d.veh;
                (
                    [v.ox as i32, v.oy as i32, d.z(&world.map) as i32],
                    v.angle(),
                )
            }),
        }
    }
}

/// Between `before` and `now`, `alpha` of the way, turning the short way.
fn between(before: ([i32; 3], i32), now: ([i32; 3], i32), alpha: f32) -> Pose {
    let b = Vec3::from(gfa_render::sprites::from_sim(before.0));
    let n = Vec3::from(gfa_render::sprites::from_sim(now.0));
    let turn = ((now.1 - before.1 + 128) & 255) - 128;
    let angle = before.1 as f32 + turn as f32 * alpha;
    Pose {
        pos: b.lerp(n, alpha),
        // The art faces south: draw half a turn round from the heading.
        draw_angle: (angle + 128.0) * std::f32::consts::TAU / 256.0,
        velocity: (n - b) * gfa_sim::clock::SIM_HZ as f32,
    }
}

/// Each traffic car's serial, position (16.16, with the ground's height)
/// and face.
fn traffic_poses(world: &World) -> Vec<(u64, ([i32; 3], i32))> {
    world
        .traffic
        .live()
        .map(|c| {
            let (x, y) = (c.x as i32, c.y as i32);
            (
                c.serial,
                ([x, y, world.map.ground_z(x, y, c.layer)], c.face),
            )
        })
        .collect()
}

/// Each ped slot's position (16.16, with the ground's height) and heading,
/// or `None` when nobody there is shown.
fn ped_poses(world: &World) -> Vec<Option<([i32; 3], i32)>> {
    world
        .peds
        .p
        .iter()
        .map(|p| {
            p.shown().then(|| {
                let (x, y) = (p.x as i32, p.y as i32);
                ([x, y, world.map.ground_z(x, y, p.layer)], p.angle)
            })
        })
        .collect()
}

/// A person to draw: pose between the last two ticks, sprite frame, the
/// GRY remap table he wears, and the fire's frame if he is alight.
#[derive(Debug, Clone, Copy)]
pub struct PedLook {
    pub pose: Pose,
    pub frame: u16,
    pub table: u8,
    pub burning: Option<usize>,
}

/// A fleet car to draw: its pose between the last two ticks, car table
/// record, colour remap (0 for its own colours), dents and fire.
#[derive(Debug, Clone, Copy)]
pub struct TrafficCar {
    pub pose: Pose,
    pub model: usize,
    pub remap: u8,
    pub deltas: u32,
    /// Ticks left before a burning write-off or wreck burns out; 0 when not alight.
    pub fuse: i32,
    pub wrecked: bool,
    pub cop: u8,
    pub face: i32,
    pub layer: i32,
    pub x: i64,
    pub y: i64,
}

/// A running level.
pub struct Game {
    pub world: World,
    pacer: Pacer,
    /// Where things were before the last tick.
    prev: Snapshot,
    /// The fleet before the last tick: serial, position and face.
    prev_traffic: Vec<(u64, ([i32; 3], i32))>,
    /// The crowd before the last tick, by slot.
    prev_peds: Vec<Option<([i32; 3], i32)>>,
    /// The player's car on its physics clock.
    car_clock: CarClock,
    /// Every tick's input since the level started.
    pub recording: InputLog,
    /// The city's traffic lights, found once.
    lamps: Vec<Lamp>,
}

impl Game {
    /// The level's world, with the player at the section's `PLAYER`.
    pub fn new(level: &Level) -> Self {
        let start = Start::from_section(&level.files.mission).unwrap_or_else(|| {
            log::warn!(
                "section [{}] has no PLAYER; starting mid-map",
                level.files.section
            );
            Start {
                bx: 128,
                by: 128,
                angle: None,
            }
        });
        let walls = WallTiles::for_map(level.map.style_number, level.files.dataset == Dataset::Gta);
        let mut world = World::new(
            SimMap::new(level.map.clone(), walls),
            level.style.cars.clone(),
            SimView::GAME,
            start,
        );
        world.load_section(&level.files.mission);
        for line in world.drain_log() {
            log::info!("{}", line.text);
        }
        Game::of(world)
    }

    /// A running level from a world already made.
    fn of(world: World) -> Self {
        let mut car_clock = CarClock::default();
        car_clock.track(&world);
        Game {
            prev: Snapshot::of(&world),
            prev_traffic: traffic_poses(&world),
            prev_peds: ped_poses(&world),
            lamps: world.traffic.lamps(&world.map),
            world,
            pacer: Pacer::default(),
            car_clock,
            recording: InputLog::default(),
        }
    }

    /// The traffic lights near the simulation's view (two blocks past it)
    /// and what each shows: the lamp, where it is drawn (map pixels) and
    /// its heading.
    pub fn lamps(&self) -> impl Iterator<Item = (Light, [f32; 3], f32)> + '_ {
        let w = &self.world;
        let view = w.view_now();
        let (cx, cy) = w.view_centre();
        let (cbx, cby) = ((cx >> 21) as i32, (cy >> 21) as i32);
        let tick = w.traffic.light_tick();
        self.lamps
            .iter()
            .filter(move |l| {
                (l.bx - cbx).abs() <= view.half_w + 2 && (l.by - cby).abs() <= view.half_h + 2
            })
            .map(move |l| {
                let (x, y) = l.pos();
                let z = w.map.ground_z(x, y, l.z);
                let pos = gfa_render::sprites::from_sim([x, y, z]);
                // The sprite's front is its lens: it faces the drivers
                // coming the other way (see `Pose::draw_angle`).
                let angle = gfa_render::sprites::sim_angle(l.toward);
                (l.light(tick), pos, angle)
            })
    }

    /// Run the ticks due after `elapsed_us` of wall time, all with `input`.
    /// Returns how many ran.
    pub fn advance(&mut self, elapsed_us: u64, input: TickInput) -> u32 {
        let n = self.pacer.ticks(elapsed_us);
        for _ in 0..n {
            self.prev = Snapshot::of(&self.world);
            self.prev_traffic = traffic_poses(&self.world);
            self.prev_peds = ped_poses(&self.world);
            self.recording.record(self.world.tick, input);
            self.world.step(input);
            self.car_clock.track(&self.world);
            for line in self.world.drain_log() {
                log::debug!("[{}] {}", line.tick, line.text);
            }
        }
        n
    }

    /// The fleet between the last two ticks. A car added this tick is drawn
    /// where it is.
    pub fn traffic(&self) -> Vec<TrafficCar> {
        let alpha = self.pacer.alpha();
        let now = traffic_poses(&self.world);
        self.world
            .traffic
            .live()
            .zip(now)
            .map(|(c, (serial, pose))| {
                let before = self
                    .prev_traffic
                    .iter()
                    .find(|p| p.0 == serial)
                    .map_or(pose, |p| p.1);
                let lights = if c.cop >= 2 || c.svc == 2 {
                    1 << (gfa_sim::cars::DELTA_LIGHT0
                        + (self.world.tick / gfa_sim::cars::LIGHT_FLASH_TICKS) % 2)
                } else {
                    0
                };
                TrafficCar {
                    pose: between(before, pose, alpha),
                    model: c.model,
                    remap: (c.remap + 1).clamp(0, 255) as u8,
                    deltas: c.dmg_bits | lights,
                    fuse: if c.fuse > 0 { c.fuse } else { c.fire_ticks },
                    wrecked: c.wrecked,
                    cop: c.cop,
                    face: c.face,
                    layer: c.layer,
                    x: c.x,
                    y: c.y,
                }
            })
            .collect()
    }

    /// The crowd between the last two ticks. Somebody new in a slot (a
    /// spawn, or a jump no walk makes in a tick) is drawn where he is.
    pub fn peds(&self) -> Vec<PedLook> {
        const JUMP: i32 = 8 << 16;
        let alpha = self.pacer.alpha();
        let now = ped_poses(&self.world);
        self.world
            .peds
            .p
            .iter()
            .zip(now)
            .enumerate()
            .filter_map(|(i, (p, pose))| {
                let pose = pose?;
                let before = self
                    .prev_peds
                    .get(i)
                    .copied()
                    .flatten()
                    .filter(|b| {
                        (b.0[0] - pose.0[0]).abs() < JUMP && (b.0[1] - pose.0[1]).abs() < JUMP
                    })
                    .unwrap_or(pose);
                Some(PedLook {
                    pose: between(before, pose, alpha),
                    frame: p.sprite_frame(),
                    table: p.remap.clamp(0, 255) as u8,
                    burning: (p.burn > 0).then_some(p.burn_frame.max(0) as usize),
                })
            })
            .collect()
    }

    /// The player and his car between the last two ticks.
    pub fn frame(&self) -> Frame {
        let alpha = self.pacer.alpha();
        let now = Snapshot::of(&self.world);
        let player = between(self.prev.player, now.player, alpha);
        let car = now.car.map(|c| {
            // Just got in: no previous car pose to come from.
            let before = self.prev.car.unwrap_or(c);
            let d = self
                .world
                .driven
                .as_ref()
                .map(|d| (d.veh.model, d.veh.remap.unwrap_or(0)));
            let (model, remap) = d.unwrap_or_default();
            // Height between ticks; place, heading and velocity on the
            // physics clock, at the time everything else is drawn at.
            let mut pose = between(before, c, alpha);
            let t = f64::from(self.world.tick) - 1.0 + f64::from(alpha);
            if let Some((x, y, angle, vx, vy)) = self.car_clock.at_time(t) {
                let at = |v: f64| (v / 65536.0) as f32 * gfa_render::sprites::SIM_SCALE;
                pose.pos.x = at(x);
                pose.pos.y = at(y);
                pose.draw_angle = (angle as f32 + 128.0) * std::f32::consts::TAU / 256.0;
                let hz = gfa_sim::clock::SIM_HZ as f32;
                pose.velocity = Vec3::new(at(vx) * hz, at(vy) * hz, 0.0);
            }
            (pose, model, remap)
        });
        // A rider on his bike, or a driver seen in a convertible, sits at
        // the car's origin in the simulation: drawn with the car, on its
        // clock, or he trails the seat by up to a step (and stays on the
        // ground under a jump).
        let player = match (self.world.doing, car) {
            (Doing::Driving, Some((pose, _, _))) => pose,
            _ => player,
        };
        // The view's lead and zoom between ticks, like everything else.
        let (was, cam) = (self.prev.camera, now.camera);
        let lerp = |a: i64, b: i64| a as f32 + (b - a) as f32 * alpha;
        let lead = Vec3::new(
            lerp(was.lead.0, cam.lead.0) / 65536.0 * gfa_render::sprites::SIM_SCALE,
            lerp(was.lead.1, cam.lead.1) / 65536.0 * gfa_render::sprites::SIM_SCALE,
            0.0,
        );
        let mut follow = car.map_or(player, |c| c.0);
        follow.pos += lead;
        let view = self.world.view_now();
        Frame {
            player: self.world.player_frame().map(|f| (player, f)),
            car,
            follow,
            zoom: lerp(i64::from(was.zoom), i64::from(cam.zoom)) / ZOOM_ONE as f32,
            view: (view.half_w, view.half_h),
        }
    }

    /// The height of the lowest ground anything stands on (map pixels): the
    /// camera's cap is measured there.
    pub fn floor_z(&self) -> f32 {
        self.world.map.floor_layer() as f32 * gfa_render::city_mesh::BLOCK_PX as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gfa_formats::cmp::{Block, Cmp, CmpBuilder};
    use gfa_formats::style::{CarDoor, CarInfo};
    use gfa_sim::input::Buttons;

    /// A saloon like the style's model 0, enough to drive.
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

    /// A long straight road running south, and the player in `car` at its
    /// top, facing down it.
    fn south_road(car: CarInfo) -> World {
        let mut b = CmpBuilder::new(1);
        let floor = Block {
            faces: [0, 0, 0, 0, 1],
            ..Block::default()
        };
        let road = Block {
            type_map: 0x20,
            ..Block::default()
        };
        for x in 10..14u8 {
            for y in 5..250u8 {
                b.set(x, y, 1, floor).set(x, y, 2, road);
            }
        }
        let cmp = Cmp::parse(&b.build()).unwrap();
        let start = Start {
            bx: 12,
            by: 8,
            angle: Some(128),
        };
        let mut world = World::new(
            SimMap::new(cmp, WallTiles::gta(1)),
            vec![car],
            SimView::GAME,
            start,
        );
        world.put_in_car(0, 12, 8, 128);
        world
    }

    /// The driven car's drawn y on every frame of a 60 Hz display, the old
    /// way (between ticks) and on the physics clock, while it drives south
    /// down a long straight road at speed.
    fn drawn_ys() -> (Vec<f32>, Vec<f32>) {
        let mut world = south_road(saloon());
        let mut clock = CarClock::default();
        let up = TickInput::new(Buttons::UP);
        // Up to speed first.
        for _ in 0..150 {
            world.step(up);
            clock.track(&world);
        }
        let (mut old, mut new) = (Vec::new(), Vec::new());
        let mut prev = Snapshot::of(&world);
        let mut owed = 0u64;
        for _ in 0..120 {
            owed += 16_667;
            while owed >= u64::from(TICK_US) {
                owed -= u64::from(TICK_US);
                prev = Snapshot::of(&world);
                world.step(up);
                clock.track(&world);
            }
            let alpha = owed as f32 / TICK_US as f32;
            let now = Snapshot::of(&world);
            old.push(between(prev.car.unwrap(), now.car.unwrap(), alpha).pos.y);
            let t = f64::from(world.tick) - 1.0 + f64::from(alpha);
            new.push((clock.at_time(t).unwrap().1 / 65536.0) as f32);
        }
        (old, new)
    }

    /// Smallest and largest move from one frame to the next.
    fn steps(ys: &[f32]) -> (f32, f32) {
        let d: Vec<f32> = ys.windows(2).map(|w| w[1] - w[0]).collect();
        let lo = d.iter().copied().fold(f32::MAX, f32::min);
        let hi = d.iter().copied().fold(f32::MIN, f32::max);
        (lo, hi)
    }

    #[test]
    fn the_car_moves_on_every_frame() {
        let (old, new) = drawn_ys();
        let (lo, hi) = steps(&old);
        // Between ticks it stood still on some frames: the stutter.
        assert!(lo < 0.05 * hi, "between ticks: {lo} .. {hi}");
        let (lo, hi) = steps(&new);
        assert!(lo > 0.0, "it never stops");
        assert!(lo > 0.9 * hi, "on the physics clock, even: {lo} .. {hi}");
    }

    /// A bike: the saloon, narrow, in the bike class.
    fn bike() -> CarInfo {
        CarInfo {
            width: 12,
            length: 40,
            vtype: gfa_sim::vehicle::CLASS_BIKE,
            ..saloon()
        }
    }

    /// Reported from play: riding a bike, the rider trailed the seat by a
    /// frame. He was drawn between ticks and the bike on its physics clock.
    #[test]
    fn the_rider_sits_on_his_bike_on_every_frame() {
        let mut game = Game::of(south_road(bike()));
        let up = TickInput::new(Buttons::UP);
        let mut worst_between = 0.0f32;
        for i in 0..270 {
            game.advance(16_667, up);
            if i < 150 {
                // Up to speed first.
                continue;
            }
            let f = game.frame();
            let ((rider, sprite), (car, _, _)) = (f.player.unwrap(), f.car.unwrap());
            assert_eq!(sprite, gfa_sim::cars::BIKE_SIT);
            assert_eq!(rider.pos, car.pos, "frame {i}");
            assert_eq!(rider.draw_angle, car.draw_angle, "frame {i}");
            // How far off the seat he was drawn the old way.
            let old = between(
                game.prev.player,
                Snapshot::of(&game.world).player,
                game.pacer.alpha(),
            );
            worst_between = worst_between.max(old.pos.distance(car.pos));
        }
        // The old way was off the seat: the test would have caught it.
        assert!(worst_between > 0.5, "between ticks, off by {worst_between}");
    }

    /// On a street (layer 2) the default camera fits the simulation's view
    /// at 21:9 measured on layer 1, at rest and at full speed, so the cap
    /// only bites when the player zooms out or climbs.
    #[test]
    fn the_default_camera_fits_the_view() {
        use gfa_render::camera::{FollowCamera, MAX_ASPECT, max_depth};
        let block = gfa_render::city_mesh::BLOCK_PX as f32;
        for zoom in [ZOOM_ONE, gfa_sim::view::ZOOM_MAX] {
            let v = SimView::GAME.at(zoom);
            let cap = block + max_depth(v.half_w, v.half_h, MAX_ASPECT);
            let eye = 2.0 * block + FollowCamera::DEFAULT_HEIGHT * zoom as f32 / ZOOM_ONE as f32;
            assert!(eye <= cap, "zoom {zoom}: eye {eye} over the cap {cap}");
        }
    }

    #[test]
    fn pacer() {
        let mut p = Pacer::default();
        assert_eq!(p.ticks(10_000), 0);
        assert!((p.alpha() - 0.5).abs() < 1e-6);
        assert_eq!(p.ticks(10_000), 1);
        assert_eq!(p.ticks(45_000), 2);
        // A long stall pays at most MAX_CATCHUP ticks.
        assert_eq!(p.ticks(10_000_000), MAX_CATCHUP);
        assert_eq!(p.ticks(0), 0);
    }
}
