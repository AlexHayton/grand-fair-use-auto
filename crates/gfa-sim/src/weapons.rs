//! What leaves a gun, how it flies, and what it does when it lands: the
//! bullet, the rocket and the flame, the explosions, the fires a rocket
//! leaves on a wall, and cars blowing up.
//!
//! Ported from AmiGTA1 (MIT): `gta_weapon.c`, whose rules it measured from
//! the original. In short:
//!
//! - **Projectiles are objects with a heading and a speed**, in a pool of
//!   40 shared by every kind. A unit of speed is half a world pixel.
//! - **The bullet** (pistol and machine gun) starts just behind the
//!   shooter, moves 15 units a tick and dies after 20 ticks. Each tick it
//!   is tested first against flat faces (fences, railings, doors), then
//!   people, then cars, then the block under it, where a wall or water
//!   stops it.
//! - **The rocket** starts ahead of him, speeds up from 10 units to 15, has
//!   no range limit and bursts on the first thing it meets. Against a wall
//!   it leaves three fires.
//! - **The flame** is a puff a tick that lives ten frames. It sets alight
//!   everybody it passes through and stops only at a car or a wall.
//! - **An explosion** kills people inside 17 px, sets them alight out to
//!   30, and writes off any car within 32, whose own fuse then runs: that
//!   is the chain reaction.
//! - **A car blowing up** is one blast, three for anything heavier than a
//!   car, five for the tanker.
//!
//! The player is neither a ped nor a fleet car, so he is a [`Target`] the
//! world sets once a tick; what reached him is counted in [`Hurt`] until
//! the world reads it.

use gfa_fixed::fx::FP;
use gfa_fixed::trig::{cos, sin};
use gfa_formats::style::CarInfo;

use crate::cars;
use crate::hash::StateHasher;
use crate::map::{SimMap, ground};
use crate::peds::Peds;
use crate::score::{self, Score, crime};
use crate::traffic::{self, Traffic};

/// The weapons, in the order the cycle keys step through them.
pub mod weapon {
    pub const FIST: usize = 0;
    pub const PISTOL: usize = 1;
    pub const MACHINE_GUN: usize = 2;
    pub const ROCKET: usize = 3;
    pub const FLAME: usize = 4;
    pub const COUNT: usize = 5;
}

/// What a crate of each weapon holds: the original's numbers. (AmiGTA1
/// triples them because it has no crates; ours come with the pickups.)
pub const CRATE_AMMO: [i32; weapon::COUNT] = [0, 20, 20, 5, 10];
/// The machine gun and the flamethrower fire five shots on one unit.
pub const AMMO_PER_UNIT: i32 = 5;

/// Ticks between shots.
pub fn cooldown(w: usize) -> i32 {
    match w {
        weapon::PISTOL => 10,
        weapon::MACHINE_GUN | weapon::FLAME => 1,
        weapon::ROCKET => 20,
        _ => 0,
    }
}

/// A punch: six states of four ticks; the blow lands as the third ends.
pub const PUNCH_TICKS: i32 = 4;
pub const PUNCH_STATES: i32 = 6;
const PUNCH_LANDS_AT: i32 = PUNCH_TICKS * (PUNCH_STATES - 2);

/// The player's weapons: which he holds, his ammunition, and the fire key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arms {
    pub weapon: usize,
    /// Units of ammunition per weapon; the fist needs none.
    pub ammo: [i32; weapon::COUNT],
    /// Shots left on the current unit, for the guns that fire five a unit.
    sub: [i32; weapon::COUNT],
    /// Ticks until the gun fires again.
    cool: i32,
    /// Ticks left of a punch in the air.
    pub punch_left: i32,
    /// The fire key, held.
    pub fire_held: bool,
}

impl Default for Arms {
    fn default() -> Self {
        Arms {
            weapon: weapon::FIST,
            ammo: [0; weapon::COUNT],
            sub: [0, 0, AMMO_PER_UNIT, 0, AMMO_PER_UNIT],
            cool: 0,
            punch_left: 0,
            fire_held: false,
        }
    }
}

/// What the fire key did this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// A punch thrown: it lands a few ticks later.
    Swing,
    /// A punch landing now.
    Blow,
    /// A shot of the held weapon.
    Shot(usize),
}

impl Arms {
    /// The next weapon with ammunition (or the fist), forwards or back.
    pub fn cycle(&mut self, forward: bool) {
        let mut w = self.weapon;
        for _ in 0..weapon::COUNT {
            w = if forward {
                (w + 1) % weapon::COUNT
            } else {
                (w + weapon::COUNT - 1) % weapon::COUNT
            };
            if w == weapon::FIST || self.ammo[w] > 0 {
                break;
            }
        }
        self.weapon = w;
    }

    /// Hand him weapon `w` with `rounds` units (the scripts' order).
    pub fn give(&mut self, w: usize, rounds: i32) {
        if w < weapon::COUNT {
            self.weapon = w;
            self.ammo[w] = rounds;
        }
    }

    /// Every gun gone: wasted or busted.
    pub fn disarm(&mut self) {
        self.ammo = [0; weapon::COUNT];
        self.weapon = weapon::FIST;
        self.fire_held = false;
    }

    /// A gun out with the fire key held: drawn armed.
    pub fn armed(&self) -> bool {
        self.fire_held && self.weapon != weapon::FIST
    }

    /// One tick of the fire key. The cooldown and a punch in the air run
    /// every tick; `can` is whether he may start one (on his feet, not in
    /// a car or mid-animation).
    pub fn tick(&mut self, can: bool) -> Option<Trigger> {
        if self.cool > 0 {
            self.cool -= 1;
        }
        let mut out = None;
        if self.punch_left > 0 {
            self.punch_left -= 1;
            if self.punch_left == PUNCH_LANDS_AT {
                out = Some(Trigger::Blow);
            }
        }
        if !(self.fire_held && can) {
            return out;
        }
        if self.weapon == weapon::FIST {
            if self.punch_left == 0 {
                self.punch_left = PUNCH_TICKS * PUNCH_STATES;
                return Some(Trigger::Swing);
            }
            return out;
        }
        if self.cool == 0 && self.ammo[self.weapon] > 0 {
            return Some(Trigger::Shot(self.weapon));
        }
        out
    }

    /// The shot left the gun: the cooldown starts and the ammunition is
    /// spent. Returns true when it ran out, and he is back to the pistol
    /// if he has rounds for it, else his fists.
    pub fn spent(&mut self) -> bool {
        let w = self.weapon;
        self.cool = cooldown(w);
        if self.sub[w] > 0 {
            self.sub[w] -= 1;
            if self.sub[w] == 0 {
                self.sub[w] = AMMO_PER_UNIT;
                self.ammo[w] -= 1;
            }
        } else {
            self.ammo[w] -= 1;
        }
        if self.ammo[w] > 0 {
            return false;
        }
        self.ammo[w] = 0;
        self.weapon = if self.ammo[weapon::PISTOL] > 0 {
            weapon::PISTOL
        } else {
            weapon::FIST
        };
        true
    }

    pub fn sim_hash(&self, h: &mut StateHasher) {
        h.u32(self.weapon as u32);
        for v in self.ammo.iter().chain(&self.sub) {
            h.i32(*v);
        }
        h.i32(self.cool);
        h.i32(self.punch_left);
        h.bool(self.fire_held);
    }
}

/// The original's single pool, for every kind of projectile.
pub const MAX_SHOTS: usize = 40;
pub const MAX_BLASTS: usize = 4;
pub const MAX_SPLATS: usize = 16;
pub const MAX_FIRES: usize = 8;

const BULLET_SPEED: i32 = 15;
const BULLET_LIFE: i32 = 20;
const ROCKET_SPEED: i32 = 10;
const ROCKET_SPEED_MAX: i32 = 15;
const FLAME_SPEED_MAX: i32 = 7;
/// A puff lives this many frames.
pub const FLAME_FRAMES: i32 = 10;
/// The rocket's sprite cycles this many frames.
pub const ROCKET_FRAMES: i32 = 3;

/// An explosion's frames, and our ticks to a frame.
pub const BLAST_FRAMES: i32 = 12;
const BLAST_TICKS: i32 = 4;
/// An explosion's reach, world px: it kills inside the first, sets alight
/// out to the second, and writes off cars within the third.
const BLAST_KILL_PX: i64 = 17;
const BLAST_BURN_PX: i64 = 30;
const BLAST_CAR_PX: i64 = 32;

/// Where a bullet stopped: a brief splat. (The original leaves its splat
/// object on the map; AmiGTA1 shows it for a moment. How long the
/// original's stays is to be measured in M6b.)
const SPLAT_TICKS: i32 = 8;

/// A fire left on the ground burns this long.
const FIRE_TICKS: i32 = 250;
/// It sets alight anybody within this many px.
const FIRE_REACH_PX: i64 = 8;
/// A car standing in it takes 5 points every this many ticks.
const FIRE_CAR_EVERY: i32 = 4;
/// Its sprite's frames, and our ticks to a frame.
pub const FIRE_FRAMES: i32 = 8;
const FIRE_FRAME_TICKS: i32 = 6;

/// What a projectile is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Bullet,
    Rocket,
    Flame,
}

/// A projectile in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Shot {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    /// The direction of travel, 0..=255.
    pub heading: i32,
    pub kind: Kind,
    /// Units (half px) a tick.
    pub speed: i32,
    /// 1-based frame of its sprite run.
    pub frame: i32,
    /// Ticks left; 0 is no limit.
    life: i32,
    /// The ped who fired it; `None` for the player.
    pub owner: Option<usize>,
    /// Ticks before it may hit anybody, so a man firing on the run does not
    /// run into his own shot.
    grace: i32,
    pub alive: bool,
}

/// Where a bullet stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Splat {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    pub ticks: i32,
}

/// An explosion, drawn as four quarter sprites; `frame` 0 is free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Blast {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    pub frame: i32,
    tick: i32,
}

/// A fire burning on the ground; `ticks` 0 is out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fire {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    pub ticks: i32,
    pub frame: i32,
    frame_tick: i32,
}

/// The player as the weapons see him: on foot a six-pixel body, in a car
/// its box squared off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    /// Half length and width of his car, when he is in one.
    pub car: Option<(i32, i32)>,
}

/// What reached the player since the world last looked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Hurt {
    /// Bullets that hit him on foot.
    pub bullets: i32,
    /// Bullets that hit his car.
    pub car_hits: i32,
    /// Blasts he was inside the killing ring of.
    pub blast: i32,
    /// Ticks of fire.
    pub burn: i32,
}

/// Counts for the log and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub fired: u64,
    pub ped: u64,
    pub car: u64,
    pub wall: u64,
    pub expired: u64,
    pub blasts: u64,
    pub fires: u64,
}

/// The world the projectiles fly through, for one call.
pub struct Street<'a> {
    pub map: &'a SimMap,
    pub cars: &'a [CarInfo],
    pub peds: &'a mut Peds,
    pub traffic: &'a mut Traffic,
    pub score: &'a mut Score,
    pub say: &'a mut dyn FnMut(String),
}

impl Street<'_> {
    fn crime(&mut self, c: usize) {
        if let Some(l) = self.score.crime(c) {
            (self.say)(l);
        }
    }

    fn force_level(&mut self, level: i32) {
        if let Some(l) = self.score.force_level(level) {
            (self.say)(l);
        }
    }

    /// The fleet car whose body `(x, y)` is in, with a pixel of margin.
    fn car_at(&self, x: i64, y: i64, layer: i32) -> Option<usize> {
        self.traffic.car_at(self.cars, x, y, layer, 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weapons {
    pub shots: Vec<Shot>,
    pub splats: [Splat; MAX_SPLATS],
    splat_next: usize,
    pub blasts: [Blast; MAX_BLASTS],
    pub fires: [Fire; MAX_FIRES],
    /// Where the player was when the world last said.
    pub target: Option<Target>,
    pub hurt: Hurt,
    pub stats: Stats,
}

impl Default for Weapons {
    fn default() -> Self {
        Weapons {
            shots: vec![Shot::default(); MAX_SHOTS],
            splats: [Splat::default(); MAX_SPLATS],
            splat_next: 0,
            blasts: [Blast::default(); MAX_BLASTS],
            fires: [Fire::default(); MAX_FIRES],
            target: None,
            hurt: Hurt::default(),
            stats: Stats::default(),
        }
    }
}

/// Q14 forward and right unit vectors for an angle.
fn axes(a: i32) -> (i64, i64, i64, i64) {
    (
        i64::from(sin(a)),
        -i64::from(cos(a)),
        i64::from(cos(a)),
        i64::from(sin(a)),
    )
}

/// What one bullet costs a vehicle: 5 points, or 2 for the heavy bodies
/// (buses and both halves of an articulated lorry). At 75 or more the next
/// hit finishes it.
pub fn bullet_damage(ci: &CarInfo, damage: i32) -> i32 {
    if damage >= 75 {
        return cars::WRECKED - damage;
    }
    if ci.vtype <= CLASS_JUGG_BACK { 2 } else { 5 }
}

/// Vehicle classes, as the style stores them.
const CLASS_JUGG_BACK: u8 = 2;
const CLASS_TRAIN: u8 = 8;
const CLASS_TRAM: u8 = 9;
const CLASS_BOAT: u8 = 13;
const CLASS_TANK: u8 = 14;

impl Weapons {
    /// Projectiles in flight.
    pub fn alive(&self) -> usize {
        self.shots.iter().filter(|s| s.alive).count()
    }

    /// The player's position for the next tick's hits.
    pub fn set_target(&mut self, t: Target) {
        self.target = Some(t);
    }

    /// What reached the player since the last call, cleared.
    pub fn take_hurt(&mut self) -> Hurt {
        std::mem::take(&mut self.hurt)
    }

    /// Is the player within `r` px of `(x, y)` on `layer`?
    fn target_within(&self, x: i64, y: i64, layer: i32, r: i64) -> bool {
        let Some(t) = self.target else {
            return false;
        };
        if t.layer != layer {
            return false;
        }
        let body = t.car.map_or(6, |(hl, hw)| i64::from(hl.max(hw)));
        let reach = body + r;
        ((t.x - x) >> FP).abs() <= reach && ((t.y - y) >> FP).abs() <= reach
    }

    /// Did a shot's step from `(x0, y0)` to `(x1, y1)` pass through the
    /// player on foot? Points no more than 4 px apart along the step, each
    /// against his body plus a pixel for the bullet.
    fn target_on_path(&self, x0: i64, y0: i64, x1: i64, y1: i64, layer: i32) -> bool {
        let (ddx, ddy) = (x1 - x0, y1 - y0);
        let n = (ddx.abs().max(ddy.abs()) >> 18) + 1;
        (1..=n).any(|k| self.target_within(x0 + ddx / n * k, y0 + ddy / n * k, layer, 1))
    }

    fn splat(&mut self, x: i64, y: i64, layer: i32) {
        self.splats[self.splat_next] = Splat {
            x,
            y,
            layer,
            ticks: SPLAT_TICKS,
        };
        self.splat_next = (self.splat_next + 1) % MAX_SPLATS;
    }

    /// A fire on the ground: the first slot out, or else the one with the
    /// least left to burn.
    fn light(&mut self, x: i64, y: i64, layer: i32) {
        let mut pick = 0;
        for (i, f) in self.fires.iter().enumerate() {
            if f.ticks == 0 {
                pick = i;
                break;
            }
            if f.ticks < self.fires[pick].ticks {
                pick = i;
            }
        }
        self.fires[pick] = Fire {
            x,
            y,
            layer,
            ticks: FIRE_TICKS,
            frame: 0,
            frame_tick: 0,
        };
        self.stats.fires += 1;
    }

    /// One shot of weapon `w` from a shooter at `(x, y)` facing `angle`;
    /// `running` picks the muzzle and, for the flame, his speed. `owner` is
    /// the ped who fired, `None` for the player. False when nothing left
    /// the gun (the fist, or the pool is full).
    #[allow(clippy::too_many_arguments)]
    pub fn fire(
        &mut self,
        w: usize,
        x: i64,
        y: i64,
        layer: i32,
        angle: i32,
        running: bool,
        owner: Option<usize>,
    ) -> bool {
        if !(weapon::PISTOL..weapon::COUNT).contains(&w) {
            return false;
        }
        let Some(slot) = self.shots.iter().position(|s| !s.alive) else {
            return false;
        };
        let (fx, fy, rx, ry) = axes(angle);
        // Offsets in Q14 multiples: 4 is a pixel, 2 a unit.
        let (kind, speed, life, along, aside, grace) = match (w, running) {
            (weapon::PISTOL | weapon::MACHINE_GUN, false) => {
                (Kind::Bullet, BULLET_SPEED, BULLET_LIFE, -4, 0, 0)
            }
            (weapon::PISTOL | weapon::MACHINE_GUN, true) => {
                (Kind::Bullet, BULLET_SPEED, BULLET_LIFE, -6, 8, 2)
            }
            (weapon::ROCKET, false) => (Kind::Rocket, ROCKET_SPEED, 0, 24, 4, 0),
            (weapon::ROCKET, true) => (Kind::Rocket, ROCKET_SPEED, 0, 4, 12, 1),
            // The flame starts at the shooter's own speed plus four.
            (_, false) => (Kind::Flame, 5, 0, 24, 0, 0),
            (_, true) => (Kind::Flame, 8, 0, 36, 12, 0),
        };
        self.shots[slot] = Shot {
            x: x + fx * along + rx * aside,
            y: y + fy * along + ry * aside,
            layer,
            heading: angle & 255,
            kind,
            speed,
            frame: 1,
            life,
            owner,
            grace,
            alive: true,
        };
        self.stats.fired += 1;
        true
    }

    /// A blast at `(x, y)`: a rocket's, a car's or the script's. People
    /// close in die, further out they burn; the player likewise; cars in
    /// reach are written off and their own fuses run. `by_player` scores it.
    pub fn explode(&mut self, st: &mut Street, x: i64, y: i64, layer: i32, by_player: bool) {
        if let Some(b) = self.blasts.iter_mut().find(|b| b.frame == 0) {
            *b = Blast {
                x,
                y,
                layer,
                frame: 1,
                tick: 0,
            };
        }
        self.stats.blasts += 1;
        for i in 0..st.peds.p.len() {
            let p = &st.peds.p[i];
            if !p.alive || p.corpse || p.layer != layer {
                continue;
            }
            let (dx, dy) = (((p.x - x) >> FP).abs(), ((p.y - y) >> FP).abs());
            if dx > BLAST_BURN_PX || dy > BLAST_BURN_PX {
                continue;
            }
            if dx <= BLAST_KILL_PX && dy <= BLAST_KILL_PX {
                st.peds.kill(i);
                if by_player {
                    st.score.event(score::kind::CIVILIAN, score::reason::BLOWN);
                    st.crime(crime::MURDER);
                }
            } else {
                st.peds.burn(i, x, y);
            }
        }
        if self.target_within(x, y, layer, BLAST_KILL_PX) {
            self.hurt.blast += 1;
        } else if self.target_within(x, y, layer, BLAST_BURN_PX) {
            self.hurt.burn += 20;
        }
        for c in &mut st.traffic.cars[..st.traffic.n] {
            if c.done || c.layer != layer || c.damage >= cars::WRECKED {
                continue;
            }
            if ((c.x - x) >> FP).abs() <= BLAST_CAR_PX && ((c.y - y) >> FP).abs() <= BLAST_CAR_PX {
                c.damage = cars::WRECKED;
                c.dmg_bits |= cars::DAMAGE_MASK;
            }
        }
    }

    /// The script's explosion on one face of block `(bx, by)` (0 west,
    /// 1 east, 2 north, 3 south): the blast just outside that side, and two
    /// fires along the wall either side of it. Returns where it went off.
    #[allow(clippy::too_many_arguments)]
    pub fn explode_face(
        &mut self,
        st: &mut Street,
        bx: i32,
        by: i32,
        layer: i32,
        face: i32,
        by_player: bool,
    ) -> (i64, i64) {
        let cx = (i64::from(bx) * 32 + 16) << FP;
        let cy = (i64::from(by) * 32 + 16) << FP;
        let (out, side) = (20i64 << FP, 10i64 << FP);
        let (ox, oy, sx, sy) = match face & 3 {
            0 => (-out, 0, 0, side),
            1 => (out, 0, 0, side),
            2 => (0, -out, side, 0),
            _ => (0, out, side, 0),
        };
        let (x, y) = (cx + ox, cy + oy);
        self.explode(st, x, y, layer, by_player);
        self.light(x + sx, y + sy, layer);
        self.light(x - sx, y - sy, layer);
        (x, y)
    }

    /// A vehicle coming apart: one blast at its centre, or for anything
    /// heavier than a car (and trains, trams, boats and the tank) three,
    /// at the centre and both ends; the tanker (a bus-class body 110 px or
    /// longer) five, adding a pair a quarter of its length either side.
    #[allow(clippy::too_many_arguments)]
    pub fn wreck_car(
        &mut self,
        st: &mut Street,
        ci: &CarInfo,
        cx: i64,
        cy: i64,
        face: i32,
        layer: i32,
        by_player: bool,
    ) {
        let (fx, fy, _, _) = axes(face);
        let hl = i64::from(traffic::world_len(ci) / 2);
        let heavy = ci.vtype <= CLASS_JUGG_BACK
            || matches!(ci.vtype, CLASS_TRAIN | CLASS_TRAM | CLASS_BOAT | CLASS_TANK);
        let n = match (heavy, ci.vtype <= CLASS_JUGG_BACK && hl * 2 >= 110) {
            (false, _) => 1,
            (true, true) => 5,
            (true, false) => 3,
        };
        self.explode(st, cx, cy, layer, by_player);
        for q in 1..n {
            let mut a = if q & 1 != 0 { hl } else { -hl };
            if q >= 3 {
                a /= 2;
            }
            self.explode(st, cx + fx * (a * 4), cy + fy * (a * 4), layer, by_player);
        }
    }

    /// One tick: splats fade, fires burn, explosions play on, write-offs in
    /// the fleet burn and blow up, and every projectile moves and is tested
    /// against what it meets.
    pub fn tick(&mut self, st: &mut Street) {
        for s in &mut self.splats {
            if s.ticks > 0 {
                s.ticks -= 1;
            }
        }
        self.burn_fires(st);
        for b in &mut self.blasts {
            if b.frame > 0 {
                b.tick += 1;
                if b.tick >= BLAST_TICKS {
                    b.tick = 0;
                    b.frame += 1;
                    if b.frame > BLAST_FRAMES {
                        b.frame = 0;
                    }
                }
            }
        }
        self.wreck_sweep(st);
        for i in 0..MAX_SHOTS {
            if self.shots[i].alive {
                self.fly(st, i);
            }
        }
    }

    fn burn_fires(&mut self, st: &mut Street) {
        for k in 0..MAX_FIRES {
            let f = &mut self.fires[k];
            if f.ticks == 0 {
                continue;
            }
            f.ticks -= 1;
            f.frame_tick += 1;
            if f.frame_tick >= FIRE_FRAME_TICKS {
                f.frame_tick = 0;
                f.frame = (f.frame + 1) % FIRE_FRAMES;
            }
            let f = *f;
            for i in 0..st.peds.p.len() {
                let p = &st.peds.p[i];
                if !p.alive || p.corpse || p.burn > 0 || p.layer != f.layer {
                    continue;
                }
                if ((p.x - f.x) >> FP).abs() <= FIRE_REACH_PX
                    && ((p.y - f.y) >> FP).abs() <= FIRE_REACH_PX
                {
                    st.peds.burn(i, f.x, f.y);
                }
            }
            if self.target_within(f.x, f.y, f.layer, FIRE_REACH_PX) {
                self.hurt.burn += 1;
            }
            if f.ticks % FIRE_CAR_EVERY == 0
                && let Some(ci) = st.car_at(f.x, f.y, f.layer)
            {
                let c = &mut st.traffic.cars[ci];
                if !c.wrecked && !c.invuln {
                    c.damage += 5;
                }
            }
        }
    }

    /// Write-offs in the fleet burn their fuse and blow up, each blast
    /// inline so a neighbour it writes off joins the sweep.
    fn wreck_sweep(&mut self, st: &mut Street) {
        let mut i = 0;
        while i < st.traffic.n {
            if let Some((model, x, y, face, layer)) = st.traffic.wreck_step(i, &mut *st.say) {
                let cars = st.cars;
                self.wreck_car(st, &cars[model], x, y, face, layer, false);
                // AmiGTA1 pays 100 for every car blowing up, whoever wrote
                // it off.
                st.score.add(100);
            }
            i += 1;
        }
    }

    /// Projectile `i`'s tick.
    fn fly(&mut self, st: &mut Street, i: usize) {
        let s = &mut self.shots[i];
        let (ox, oy) = (s.x, s.y);
        // A unit is half a pixel: Q14 times 2.
        s.x += i64::from(sin(s.heading)) * i64::from(s.speed * 2);
        s.y -= i64::from(cos(s.heading)) * i64::from(s.speed * 2);
        // The original speeds it up after the step.
        match s.kind {
            Kind::Rocket if s.speed < ROCKET_SPEED_MAX => s.speed += 3,
            Kind::Flame if s.speed < FLAME_SPEED_MAX => s.speed += 2,
            _ => {}
        }
        let s = *s;
        let (bx, by) = ((s.x >> 21) as i32, (s.y >> 21) as i32);
        if !(1..=254).contains(&bx) || !(1..=254).contains(&by) {
            self.shots[i].alive = false;
            self.stats.expired += 1;
            return;
        }
        let by_player = s.owner.is_none();

        // A fence, a railing or a door stops it before anything behind.
        if st
            .map
            .shot_cross(s.layer, ox as i32, oy as i32, s.x as i32, s.y as i32)
        {
            match s.kind {
                Kind::Rocket => self.explode(st, ox, oy, s.layer, by_player),
                Kind::Bullet => self.splat(ox, oy, s.layer),
                Kind::Flame => {}
            }
            (st.say)(format!(
                "weapons: shot {i} stopped by a flat face at ({},{})",
                s.x >> FP,
                s.y >> FP
            ));
            self.shots[i].alive = false;
            self.stats.wall += 1;
            return;
        }

        // A shot fired on the run skips its shooter for a tick or two, but
        // not a car he is standing beside.
        let (mx, my) = (ox + ((s.x - ox) >> 1), oy + ((s.y - oy) >> 1));
        if s.grace > 0
            && (st.car_at(mx, my, s.layer).is_some() || st.car_at(s.x, s.y, s.layer).is_some())
        {
            self.shots[i].grace = 0;
        }
        let s = self.shots[i];
        if s.grace > 0 {
            self.shots[i].grace -= 1;
        } else if s.owner.is_some() && s.kind != Kind::Flame && self.hits_target(&s, ox, oy) {
            // Somebody else's shot on the player: on foot it hits him, in
            // a car the car.
            if self.target.is_some_and(|t| t.car.is_some()) {
                self.hurt.car_hits += 1;
            } else {
                self.hurt.bullets += 1;
            }
            if s.kind == Kind::Rocket {
                self.explode(st, s.x, s.y, s.layer, false);
            } else {
                self.splat(s.x, s.y, s.layer);
            }
            self.shots[i].alive = false;
            return;
        } else if s.kind == Kind::Flame {
            if self.flame_hits(st, i, ox, oy) {
                return;
            }
        } else if self.bullet_hits(st, i, ox, oy) {
            return;
        }

        // The block under it: a wall or water stops anything.
        let g = st.map.nav_ground(bx, by, s.layer);
        if g == ground::BUILDING || g == ground::WATER {
            match s.kind {
                Kind::Rocket => {
                    self.explode(st, s.x, s.y, s.layer, by_player);
                    (st.say)(format!(
                        "weapons: rocket burst on {} at block ({bx},{by},{})",
                        if g == ground::WATER {
                            "water"
                        } else {
                            "a wall"
                        },
                        s.layer
                    ));
                    // Three fires this side of the wall: a step back along
                    // its line, and a step either way across it.
                    if g == ground::BUILDING {
                        let (fx, fy, rx, ry) = axes(s.heading);
                        let (bx, by) = (ox - fx * 40, oy - fy * 40);
                        self.light(bx, by, s.layer);
                        self.light(bx + rx * 40, by + ry * 40, s.layer);
                        self.light(bx - rx * 40, by - ry * 40, s.layer);
                    }
                }
                Kind::Bullet => {
                    self.splat(s.x, s.y, s.layer);
                    (st.say)(format!(
                        "weapons: bullet {i} hit {} at ({},{}) block ({bx},{by},{})",
                        if g == ground::WATER {
                            "water"
                        } else {
                            "a wall"
                        },
                        s.x >> FP,
                        s.y >> FP,
                        s.layer
                    ));
                }
                Kind::Flame => {}
            }
            self.shots[i].alive = false;
            self.stats.wall += 1;
            return;
        }

        // Ageing: a flame counts frames, a bullet ticks, a rocket only
        // turns its sprite over.
        let s = &mut self.shots[i];
        match s.kind {
            Kind::Flame => {
                s.frame += 1;
                if s.frame > FLAME_FRAMES {
                    s.alive = false;
                    self.stats.expired += 1;
                }
            }
            Kind::Rocket => {
                s.frame += 1;
                if s.frame > ROCKET_FRAMES {
                    s.frame = 1;
                }
            }
            Kind::Bullet => {
                if s.life > 0 {
                    s.life -= 1;
                    if s.life <= 0 {
                        s.alive = false;
                        self.stats.expired += 1;
                    }
                }
            }
        }
    }

    /// Did somebody else's shot reach the player this step?
    fn hits_target(&self, s: &Shot, ox: i64, oy: i64) -> bool {
        match self.target {
            Some(t) if t.car.is_some() => {
                self.target_within(s.x, s.y, s.layer, i64::from(s.speed / 2 + 2))
            }
            Some(_) => self.target_on_path(ox, oy, s.x, s.y, s.layer),
            None => false,
        }
    }

    /// A puff through the crowd: everybody it meets catches; a car stops
    /// it. True when it is spent.
    fn flame_hits(&mut self, st: &mut Street, i: usize, ox: i64, oy: i64) -> bool {
        let s = self.shots[i];
        let by_player = s.owner.is_none();
        let len = i64::from(s.speed / 2 + 2);
        for _ in 0..st.peds.p.len() {
            let Some(pi) = st
                .peds
                .hit_segment(ox, oy, s.heading, len, 2, s.layer, s.owner)
            else {
                break;
            };
            if st.peds.p[pi].burn > 0 {
                break;
            }
            st.peds.burn(pi, ox, oy);
            if by_player {
                st.score.event(score::kind::CIVILIAN, score::reason::BURNED);
                st.crime(crime::MURDER);
            }
            (st.say)(format!("weapons: flame caught ped {pi}"));
        }
        let Some(ci) = st.car_at(s.x, s.y, s.layer) else {
            return false;
        };
        let cars = st.cars;
        let c = &mut st.traffic.cars[ci];
        if c.invuln {
            return false;
        }
        c.damage += 15;
        let info = &cars[c.model];
        c.dmg_bits |= 1 << cars::panel_delta(info, c.x, c.y, c.face, s.x, s.y);
        let cop = info.model == traffic::COP_MODEL;
        if by_player {
            st.score.add(10);
            st.crime(crime::FIREARM);
            // A round into a police car is a wanted level on its own.
            if cop {
                st.force_level(1);
            }
        }
        self.shots[i].alive = false;
        self.stats.car += 1;
        true
    }

    /// A bullet or a rocket against the crowd, then the fleet. True when
    /// it is spent.
    fn bullet_hits(&mut self, st: &mut Street, i: usize, ox: i64, oy: i64) -> bool {
        let s = self.shots[i];
        let by_player = s.owner.is_none();
        let len = i64::from(s.speed / 2 + 2);
        if let Some(pi) = st
            .peds
            .hit_segment(ox, oy, s.heading, len, 1, s.layer, s.owner)
        {
            if s.kind == Kind::Rocket {
                self.explode(st, s.x, s.y, s.layer, by_player);
                (st.say)(format!(
                    "weapons: rocket burst on ped {pi} at ({},{})",
                    s.x >> FP,
                    s.y >> FP
                ));
            } else {
                st.peds.shoot(pi, s.heading);
                let p = &st.peds.p[pi];
                let (px, py, back) = (p.x, p.y, p.shot == 2);
                self.splat(px, py, s.layer);
                if by_player {
                    st.score.event(score::kind::CIVILIAN, score::reason::SHOT);
                    st.crime(crime::MURDER);
                }
                (st.say)(format!(
                    "weapons: bullet {i} hit ped {pi} at ({},{}) from {}",
                    px >> FP,
                    py >> FP,
                    if back { "behind" } else { "the front" }
                ));
            }
            self.shots[i].alive = false;
            self.stats.ped += 1;
            return true;
        }
        // Then the cars, half way along the step as well: a shot fired
        // from beside a car can cross its whole width in one step.
        let (mx, my) = (ox + ((s.x - ox) >> 1), oy + ((s.y - oy) >> 1));
        let Some(ci) = st
            .car_at(mx, my, s.layer)
            .or_else(|| st.car_at(s.x, s.y, s.layer))
        else {
            return false;
        };
        let (model, invuln) = (st.traffic.cars[ci].model, st.traffic.cars[ci].invuln);
        let cars = st.cars;
        let info = &cars[model];
        let cop = info.model == traffic::COP_MODEL;
        if s.kind == Kind::Rocket {
            let c = &mut st.traffic.cars[ci];
            c.damage = cars::WRECKED;
            c.dmg_bits |= cars::DAMAGE_MASK;
            self.explode(st, s.x, s.y, s.layer, by_player);
            if by_player {
                st.score.add(100);
                st.crime(crime::FIREARM);
                if cop {
                    st.force_level(1);
                }
            }
            (st.say)(format!(
                "weapons: rocket burst on car {ci} (model {})",
                model
            ));
        } else if !invuln {
            let c = &mut st.traffic.cars[ci];
            c.damage += bullet_damage(info, c.damage);
            c.dmg_bits |= 1 << cars::panel_delta(info, c.x, c.y, c.face, s.x, s.y);
            let (damage, bits) = (c.damage, c.dmg_bits);
            self.splat(s.x, s.y, s.layer);
            if by_player {
                st.score.add(10);
                st.crime(crime::FIREARM);
                if cop {
                    st.force_level(1);
                }
            }
            (st.say)(format!(
                "weapons: bullet {i} hit car {ci} (model {model}) at ({},{}), damage {damage}, panels {bits:04x}",
                s.x >> FP,
                s.y >> FP
            ));
        }
        self.shots[i].alive = false;
        self.stats.car += 1;
        true
    }

    /// Every shot in flight, explosion and fire after tick `tick`, in the
    /// oracle's format.
    pub fn trace_lines(&self, tick: u32) -> Vec<String> {
        let shots = self
            .shots
            .iter()
            .enumerate()
            .filter(|(_, s)| s.alive)
            .map(|(i, s)| {
                format!(
                    "w {tick} {i} {} {} {} {} {} {} {}",
                    s.x, s.y, s.layer, s.heading, s.kind as i32, s.speed, s.frame
                )
            });
        let blasts = self
            .blasts
            .iter()
            .enumerate()
            .filter(|(_, b)| b.frame != 0)
            .map(|(i, b)| format!("x {tick} {i} {} {} {}", b.x, b.y, b.frame));
        let fires = self
            .fires
            .iter()
            .enumerate()
            .filter(|(_, f)| f.ticks != 0)
            .map(|(i, f)| format!("h {tick} {i} {} {} {}", f.x, f.y, f.ticks));
        shots.chain(blasts).chain(fires).collect()
    }

    pub fn sim_hash(&self, h: &mut StateHasher) {
        for s in &self.shots {
            h.bool(s.alive);
            if s.alive {
                h.bytes(&s.x.to_le_bytes());
                h.bytes(&s.y.to_le_bytes());
                for v in [
                    s.layer,
                    s.heading,
                    s.kind as i32,
                    s.speed,
                    s.frame,
                    s.life,
                    s.grace,
                ] {
                    h.i32(v);
                }
                h.i32(s.owner.map_or(-1, |o| o as i32));
            }
        }
        for s in &self.splats {
            h.i32(s.ticks);
        }
        h.u32(self.splat_next as u32);
        for b in &self.blasts {
            h.i32(b.frame);
            h.i32(b.tick);
        }
        for f in &self.fires {
            h.bytes(&f.x.to_le_bytes());
            h.bytes(&f.y.to_le_bytes());
            for v in [f.layer, f.ticks, f.frame, f.frame_tick] {
                h.i32(v);
            }
        }
        let t = self.target;
        h.bool(t.is_some());
        if let Some(t) = t {
            h.bytes(&t.x.to_le_bytes());
            h.bytes(&t.y.to_le_bytes());
            h.i32(t.layer);
        }
        let u = self.hurt;
        for v in [u.bullets, u.car_hits, u.blast, u.burn] {
            h.i32(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cycle_skips_empty_guns_both_ways() {
        let mut a = Arms::default();
        a.cycle(true);
        assert_eq!(a.weapon, weapon::FIST);
        a.ammo[weapon::ROCKET] = 2;
        a.ammo[weapon::FLAME] = 1;
        a.cycle(true);
        assert_eq!(a.weapon, weapon::ROCKET);
        a.cycle(true);
        assert_eq!(a.weapon, weapon::FLAME);
        a.cycle(true);
        assert_eq!(a.weapon, weapon::FIST);
        a.cycle(false);
        assert_eq!(a.weapon, weapon::FLAME);
    }

    #[test]
    fn the_machine_gun_fires_five_shots_a_unit_then_falls_back() {
        let mut a = Arms::default();
        a.give(weapon::PISTOL, 1);
        a.give(weapon::MACHINE_GUN, 2);
        a.fire_held = true;
        let mut shots = 0;
        for _ in 0..40 {
            if let Some(Trigger::Shot(_)) = a.tick(true) {
                shots += 1;
                if a.spent() {
                    break;
                }
            }
        }
        assert_eq!(shots, 10);
        assert_eq!(a.weapon, weapon::PISTOL);
        assert_eq!(a.ammo[weapon::MACHINE_GUN], 0);
    }

    #[test]
    fn a_punch_lands_eight_ticks_after_it_is_thrown_and_repeats_while_held() {
        let mut a = Arms {
            fire_held: true,
            ..Arms::default()
        };
        let mut events = Vec::new();
        for t in 0..60 {
            if let Some(e) = a.tick(t < 30) {
                events.push((t, e));
            }
        }
        assert_eq!(
            events,
            [
                (0, Trigger::Swing),
                (8, Trigger::Blow),
                (24, Trigger::Swing),
                (32, Trigger::Blow),
            ]
        );
    }

    #[test]
    fn a_shot_needs_a_gun_and_a_free_slot() {
        let mut w = Weapons::default();
        assert!(!w.fire(weapon::FIST, 0, 0, 2, 0, false, None));
        for _ in 0..MAX_SHOTS {
            assert!(w.fire(weapon::PISTOL, 0, 0, 2, 0, false, None));
        }
        assert!(!w.fire(weapon::PISTOL, 0, 0, 2, 0, false, None));
        assert_eq!(w.alive(), MAX_SHOTS);
        // The standing bullet starts a pixel behind him: facing north,
        // a pixel south.
        assert_eq!((w.shots[0].x, w.shots[0].y), (0, 1 << 16));
    }
}
