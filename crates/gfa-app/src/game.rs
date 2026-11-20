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

/// The player as the renderer draws him.
#[derive(Debug, Clone, Copy)]
pub struct PlayerPose {
    /// Render pixels (64 to a block).
    pub pos: Vec3,
    /// Radians clockwise, for the sprite (the art faces south).
    pub draw_angle: f32,
    /// Ped sprite frame within the style's ped category.
    pub frame: u16,
    /// Render pixels per second, for the camera's lead.
    pub velocity: Vec3,
}

/// A running level.
pub struct Game {
    pub world: World,
    pacer: Pacer,
    /// The player's position and angle before the last tick.
    prev: ([i32; 3], i32),
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
        let mut world = World::new(SimMap::new(level.map.clone(), walls), SimView::GAME, start);
        for line in world.drain_log() {
            log::info!("{}", line.text);
        }
        let prev = (world.player_pos(), world.player.angle);
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
            self.prev = (self.world.player_pos(), self.world.player.angle);
            self.recording.record(self.world.tick, input);
            self.world.step(input);
            for line in self.world.drain_log() {
                log::debug!("[{}] {}", line.tick, line.text);
            }
        }
        n
    }

    /// The player between the last two ticks.
    pub fn player(&self) -> PlayerPose {
        let alpha = self.pacer.alpha();
        let p = &self.world.player;
        let now = gfa_render::sprites::from_sim(self.world.player_pos());
        let before = gfa_render::sprites::from_sim(self.prev.0);
        let (now, before) = (Vec3::from(now), Vec3::from(before));
        // Turn the short way round.
        let turn = ((p.angle - self.prev.1 + 128) & 255) - 128;
        let angle = self.prev.1 as f32 + turn as f32 * alpha;
        // The art faces south: draw half a turn round from the heading.
        let draw = (angle + 128.0) * std::f32::consts::TAU / 256.0;
        PlayerPose {
            pos: before.lerp(now, alpha),
            draw_angle: draw,
            frame: p.ped_frame(),
            velocity: (now - before) * gfa_sim::clock::SIM_HZ as f32,
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
