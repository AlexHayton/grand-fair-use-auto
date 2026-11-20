//! The simulation inside the app: paced by the wall clock, fed from the
//! keyboard, and drawn between ticks.
//!
//! The simulation itself never sees the clock (Principle 4): the [`Pacer`]
//! turns elapsed wall time into a whole number of ticks, and the renderer
//! interpolates the player between the last two ticks by the leftover
//! fraction.

use gfa_assets::Dataset;
use gfa_assets::level::Level;
use gfa_render::glam::Vec3;
use gfa_sim::clock::TICK_US;
use gfa_sim::input::{InputLog, TickInput};
use gfa_sim::map::{SimMap, WallTiles};
use gfa_sim::view::SimView;
use gfa_sim::world::{Start, World};

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
    pub follow: Pose,
}

/// Positions (16.16, z included) and headings (256 steps) at a tick.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Snapshot {
    player: ([i32; 3], i32),
    car: Option<([i32; 3], i32)>,
}

impl Snapshot {
    fn of(world: &World) -> Self {
        Snapshot {
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

/// A running level.
pub struct Game {
    pub world: World,
    pacer: Pacer,
    /// Where things were before the last tick.
    prev: Snapshot,
    /// Every tick's input since the level started.
    pub recording: InputLog,
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
        for line in world.drain_log() {
            log::info!("{}", line.text);
        }
        let prev = Snapshot::of(&world);
        Game {
            world,
            pacer: Pacer::default(),
            prev,
            recording: InputLog::default(),
        }
    }

    /// Run the ticks due after `elapsed_us` of wall time, all with `input`.
    /// Returns how many ran.
    pub fn advance(&mut self, elapsed_us: u64, input: TickInput) -> u32 {
        let n = self.pacer.ticks(elapsed_us);
        for _ in 0..n {
            self.prev = Snapshot::of(&self.world);
            self.recording.record(self.world.tick, input);
            self.world.step(input);
            for line in self.world.drain_log() {
                log::debug!("[{}] {}", line.tick, line.text);
            }
        }
        n
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
            (between(before, c, alpha), model, remap)
        });
        Frame {
            player: self.world.player_frame().map(|f| (player, f)),
            car,
            follow: car.map_or(player, |c| c.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
