//! Pedestrians: the crowd on the pavements, and everybody else on foot who
//! is not the player.
//!
//! After AmiGTA1's `gta_peds.c` (MIT), in its units and with its numbers.
//! A ped walks straight along a heading, and a few rules bend it:
//!
//! - a gait taken from a wheel all peds share (wander, brisk, jog, a short
//!   blip, or a rest), each for a random time;
//! - a wobble of up to 8 steps, its sign alternating;
//! - three quarters of a block before the pavement ends, a turn towards the
//!   pavement to one side;
//! - a road is only stepped onto with pavement a block and a half beyond
//!   it; a ped who finds himself on the road runs away from the point where
//!   he stepped on;
//! - at a block the map marks as a light, now and then, a wait at the kerb
//!   for the cars along his line to have the green, then a run across.
//!
//! People are born one block outside the view, on the side the player is
//! heading for, mostly walking in, and are let go once they have been out
//! of the view (grown by two blocks) for 45 ticks.
//!
//! AmiGTA1 departs from the original in four places so that people do not
//! pile up at corners: the corner target is spread over the pavement, close
//! neighbours sidestep and are pushed apart, nobody rests within sight of a
//! corner, and a T-junction is a coin toss per ped. We keep all four, so
//! the oracle can check us (`docs/PARITY.md`).
//!
//! Ours, beyond AmiGTA1: a ped who ends up in the water drowns as the
//! player does (AmiGTA1's peds stand on it); the pool is sized to the view
//! ([`Peds::cap_for`]); and the sidestep's quarter-turn bend comes off
//! again before his next tick and is given up once that side is walled,
//! where AmiGTA1's piled up into a spin on the spot or flicked back and
//! forth along a kerb. The oracle runs AmiGTA1's crowd with the same fix
//! (`tools/oracle/ped_sidestep.pl`).
//!
//! The police on foot, the script's own people and gunmen are AmiGTA1's
//! too; their brain is here, and what sets them going comes with M6 and M7.

use gfa_fixed::fx::{BLOCK_SHIFT, FP};
use gfa_fixed::trig::{cos, dir16, sin};
use gfa_formats::style::CarInfo;

use crate::cars;
use crate::hash::{SimHash, StateHasher};
use crate::map::{SimMap, ground};
use crate::player::{RUN_SPEED, frames};
use crate::view::SimView;

/// AmiGTA1's pool, at its view ([`SimView::AMIGTA1`]).
pub const AMIGTA1_PEDS: usize = 12;
/// The traffic and drowning seed (AmiGTA1's `gta_peds_init` call).
pub const SEED: u64 = 777;

/// One speed unit: a quarter of the player's run, 16.16 px a tick.
const UNIT: i64 = RUN_SPEED as i64 / 4;
const WALK_TICKS: i32 = 7;
const RUN_TICKS: i32 = 4;

/// The original's dress list, handed out in turn: GRY remap tables.
pub const REMAPS: [u8; 22] = [
    39, 40, 41, 132, 133, 134, 136, 137, 138, 139, 140, 177, 178, 179, 180, 181, 182, 183, 184,
    185, 186, 187,
];

/// Lying after being thrown out of a car (the generic drop).
pub const STUN_TICKS: i32 = 125;
/// A ped's brain: what he is doing.
pub mod mode {
    pub const FLEE: i32 = 1;
    pub const IDLE: i32 = 2;
    /// A policeman on foot after the player.
    pub const COP: i32 = 3;
    /// At a lit crossing: waiting, then across.
    pub const CROSS: i32 = 4;
    /// Put there by the script; stays.
    pub const MISSION: i32 = 5;
    /// The script's: after a target.
    pub const HUNT: i32 = 6;
    /// The script's: walking to a point.
    pub const SENDTO: i32 = 7;
}
/// How he walks.
pub mod sub {
    pub const WANDER: i32 = 2;
    pub const BRISK: i32 = 4;
    pub const JOG: i32 = 5;
    pub const BLIP: i32 = 7;
    pub const STAND: i32 = 8;
    pub const VEER_L: i32 = 9;
    pub const VEER_R: i32 = 10;
}

const COP_ARREST_PX: i64 = 10;
const COP_PULL_TICKS: i32 = 20 + 6 * 4;
const COP_DOOR_PX: i64 = 6;

const FIRE_FRAMES: i32 = 8;
const FIRE_FRAME_TICKS: i32 = 6;
const SHOT_STATES: i32 = 4;
const SHOT_STATE_TICKS: i32 = 4;
const FALL_TICKS: i32 = 12;
const PUNCH_DOWN_TICKS: i32 = 80;
const PULL_STATES: i32 = 6;
const PULL_TICKS: i32 = 4;
const PULL_DOWN_TICKS: i32 = 21 * PULL_TICKS;
const CORPSE_TICKS: i32 = 1500;
/// A ped set alight burns to death in this many ticks.
pub const BURN_TICKS: i32 = 100;
/// A frightened ped runs at least this long before the pavement may calm
/// him.
const PANIC_TICKS: i32 = 150;
/// Of the original's 13-step punch counter, the first 10 land; the last
/// three are the victim's (the counter-punch, M6b).
const PUNCH_WHEEL: i32 = 13;
const PUNCH_LANDS: i32 = 10;
const RETIRE_TICKS: i32 = 45;

const APART_PX: i64 = 5;
const DODGE_TICKS: i32 = 24;
const LEAVE_TICKS: i32 = 100;
const DODGE_TURN: i32 = 64;

/// Ticks a ped lasts under water: the player's 100 health at 3 every
/// other tick.
pub const DROWN_TICKS: i32 = 67;

/// The pull out of a car: along the body and out from the flank, px, per
/// state.
const PULL_ALONG: [i64; 6] = [-1, -1, -2, -3, -4, -4];
const PULL_LAT: [i64; 6] = [-3, 0, 2, 2, 2, 2];

/// Ped sprite frames beyond the player's: the fall, lying, the bodies and
/// the pull's.
pub mod frame {
    pub const FALL_FIRST: u16 = 38;
    pub const LIES: u16 = 42;
    pub const BODY: u16 = 43;
    pub const BODY_BACK: u16 = 44;
    /// The second set, a policeman's.
    pub const COP_SET: u16 = 0xbd;
}

/// One person.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ped {
    /// 16.16 world px.
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    /// Walking heading, 0..256.
    pub angle: i32,
    /// Within the walk or run cycle.
    pub frame: i32,
    pub frame_tick: i32,
    /// GRY remap table: what he wears. 0 is the sprite's own colours.
    pub remap: i32,
    pub alive: bool,
    pub mode: i32,
    /// The script's name for him, 0 for the crowd.
    pub serial: u64,
    /// The script's own: never let go.
    pub mission: bool,
    /// 2 while an ambulance is coming for him.
    pub svc_q: u8,
    hunt_pl: bool,
    gang: u8,
    gang_miss: u8,
    gang_burst: bool,
    gang_cool: i32,
    send_speed: i32,
    pub sub: i32,
    /// 0..4, a quarter of a run each.
    pub speed: i32,
    timer: i32,
    wobble: i32,
    /// Where he is walking to, 0 for nowhere.
    tx: i64,
    ty: i64,
    /// Where he is running from.
    gx: i64,
    gy: i64,
    stuck: i32,
    dodge: i32,
    dodge_side: i32,
    /// The turn the sidestep put on his heading this tick, taken off again
    /// before his next tick's aim (ours: AmiGTA1 leaves it on, and a man
    /// whose mode does not re-aim him every tick turned another quarter
    /// every tick and spun on the spot).
    bent: i32,
    /// The block he was last in and how long he has wanted out of it.
    blk_x: i32,
    blk_y: i32,
    blk_ticks: i32,
    offscreen: i32,
    flee_aim: i32,
    /// Ticks left lying, alive.
    pub down: i32,
    pub corpse: bool,
    pub bled: bool,
    /// Being pulled out of a car: the state 0..6, or -1.
    pub pull: i32,
    pull_tick: i32,
    pull_cx: i64,
    pull_cy: i64,
    pull_face: i32,
    pull_model: usize,
    /// Shot: 1 from the front, 2 from behind.
    pub shot: i32,
    pub shot_step: i32,
    shot_tick: i32,
    shot_dir: i32,
    /// Knocked down, falling back.
    pub fall: i32,
    panic: i32,
    /// Ticks left alight.
    pub burn: i32,
    pub burn_frame: i32,
    burn_tick: i32,
    pub cop: bool,
    /// The police uniform without the police brain.
    pub uniform: bool,
    pub weapon: u8,
    arrest: bool,
    cop_cool: i32,
    arrest_wait: i32,
    shoot_req: bool,
    shoot_angle: i32,
    execute: bool,
    post: bool,
    exit_wait: i32,
    pulling: i32,
    cross_axis_x: bool,
    /// Ticks under water, 0 when dry (ours).
    pub drown: i32,
}

impl Ped {
    /// The ped sheet frame to draw him with, before the policeman's set.
    pub fn sprite_frame(&self) -> u16 {
        let f = if self.pull >= 0 {
            match self.pull {
                0..=2 => 80,
                3 => 46,
                _ => 45,
            }
        } else if self.corpse {
            if self.shot == 2 {
                frame::BODY_BACK
            } else {
                frame::BODY
            }
        } else if self.shot != 0 {
            match (self.shot, self.shot_step < 3) {
                (2, true) => 14,
                (2, false) => 41,
                (_, true) => 38,
                (_, false) => 39,
            }
        } else if self.fall > 0 {
            frame::FALL_FIRST
        } else if self.down > 0 {
            frame::LIES
        } else if self.speed == 0 {
            frames::STAND
        } else {
            let first = if self.speed >= 3 {
                frames::RUN_FIRST
            } else {
                frames::WALK_FIRST
            };
            first + self.frame as u16
        };
        if self.cop || self.uniform {
            f + frame::COP_SET
        } else {
            f
        }
    }

    /// Is he in the middle of a sidestep?
    pub fn dodging(&self) -> bool {
        self.dodge > 0
    }

    /// Is he drawn? Not while under water.
    pub fn shown(&self) -> bool {
        self.alive && self.drown == 0
    }
}

/// Counts for the log and the tests; nothing reads them back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PedStats {
    pub spawned: u64,
    pub runover: u64,
    pub killed: u64,
    pub crossings: u64,
    pub crossed: u64,
    pub drowned: u64,
    pub cops_killed: u64,
    pub shot: u64,
    pub punched: u64,
}

/// The pool, and what every ped shares.
#[derive(Debug, Clone)]
pub struct Peds {
    pub p: Vec<Ped>,
    next_serial: u64,
    rng: u64,
    view_hw: i32,
    view_hh: i32,
    player_angle: i32,
    player_moving: bool,
    gait_wheel: i32,
    spawn_edge: i32,
    spawn_dir: i32,
    remap_next: usize,
    fidget: bool,
    spawned_since_retire: bool,
    punch_wheel: i32,
    // The player, for the police on foot.
    pl_x: i64,
    pl_y: i64,
    pl_layer: i32,
    pl_in_car: bool,
    pl_door_x: i64,
    pl_door_y: i64,
    pl_slow: bool,
    pl_hl: i64,
    pl_hw: i64,
    pl_face: i32,
    /// The slot the last pull, knock-off or spawn used.
    pub last: Option<usize>,
    cross_rr: i32,
    cop_shoot: i32,
    gang_aim: i32,
    gang_side: bool,
    pub stats: PedStats,
}

/// What a tick needs from the world.
pub struct TickCtx<'a> {
    pub map: &'a SimMap,
    pub cars: &'a [CarInfo],
    /// The view's centre: the player, or his car.
    pub cam: (i64, i64),
    /// Is the light at `(bx, by)` green for traffic along x (or y)?
    pub green: &'a dyn Fn(i32, i32, bool) -> bool,
}

fn ground_at(map: &SimMap, x: i64, y: i64, z: i32) -> u8 {
    let (bx, by) = (x >> 21, y >> 21);
    if !(0..256).contains(&bx) || !(0..256).contains(&by) {
        return ground::BUILDING;
    }
    map.nav_ground(bx as i32, by as i32, z)
}

fn pavement_at(map: &SimMap, x: i64, y: i64, z: i32) -> bool {
    ground_at(map, x, y, z) == ground::PAVEMENT
}

/// Road, pavement or field: where a push may put him.
fn walkable(map: &SimMap, x: i64, y: i64, z: i32) -> bool {
    (ground::ROAD..=ground::FIELD).contains(&ground_at(map, x, y, z))
}

/// The point `px` world px along heading `a` from `(x, y)`.
fn ahead(x: i64, y: i64, a: i32, px: i64) -> (i64, i64) {
    (
        x + i64::from(sin(a)) * px * 4,
        y - i64::from(cos(a)) * px * 4,
    )
}

fn snap_cardinal(a: i32) -> i32 {
    ((a + 32) & 255) & !63
}

fn angle_to(fx: i64, fy: i64, tx: i64, ty: i64) -> i32 {
    (dir16((tx - fx) as i32, (ty - fy) as i32) >> 16) & 255
}

/// The traffic hint of the block under `(x, y)` on layer `z`; 1 is a light.
fn hint_at(map: &SimMap, x: i64, y: i64, z: i32) -> u8 {
    map.block((x >> 21) as i32, (y >> 21) as i32, z)
        .map_or(0, |b| b.traffic_hint())
}

/// A step `dist` px that way is not into a building, the air or water.
fn step_free(map: &SimMap, p: &Ped, ang: i32, dist: i64) -> bool {
    let (nx, ny) = ahead(p.x, p.y, ang, dist);
    !matches!(
        ground_at(map, nx, ny, p.layer),
        ground::BUILDING | ground::AIR | ground::WATER
    )
}

/// Where somebody being pulled out of a car stands in pull state `p.pull`.
fn pull_place(cars: &[CarInfo], p: &mut Ped) {
    let ci = &cars[p.pull_model];
    let s = p.pull.clamp(0, PULL_STATES - 1) as usize;
    let (fx, fy) = (i64::from(sin(p.pull_face)), -i64::from(cos(p.pull_face)));
    let (rx, ry) = (i64::from(cos(p.pull_face)), i64::from(sin(p.pull_face)));
    let along = ci.doors.first().map_or(0, |d| i64::from(d.rpx) / 2) + PULL_ALONG[s];
    let across = cars::door_side(ci) * (i64::from(ci.width >> 1) / 2 + PULL_LAT[s]);
    p.x = p.pull_cx + (fx * along + rx * across) * 4;
    p.y = p.pull_cy + (fy * along + ry * across) * 4;
}

impl Peds {
    /// A pool of `cap` slots, all empty.
    pub fn new(seed: u64, cap: usize) -> Self {
        Peds {
            p: vec![Ped::default(); cap],
            next_serial: 0,
            rng: if seed != 0 { seed } else { 1 },
            view_hw: 6,
            view_hh: 4,
            player_angle: 0,
            player_moving: false,
            gait_wheel: 0,
            spawn_edge: 0,
            spawn_dir: 0,
            remap_next: 0,
            fidget: false,
            spawned_since_retire: false,
            punch_wheel: 0,
            pl_x: 0,
            pl_y: 0,
            pl_layer: 0,
            pl_in_car: false,
            pl_door_x: 0,
            pl_door_y: 0,
            pl_slow: false,
            pl_hl: 0,
            pl_hw: 0,
            pl_face: 0,
            last: None,
            cross_rr: 0,
            cop_shoot: 0,
            gang_aim: 0,
            gang_side: false,
            stats: PedStats::default(),
        }
    }

    /// The pool for a view: AmiGTA1's twelve at its view, and as many more
    /// as keep the same number of people to a block of view.
    pub fn cap_for(view: SimView) -> usize {
        let area = |v: SimView| (2 * v.half_w + 1) as usize * (2 * v.half_h + 1) as usize;
        (AMIGTA1_PEDS * area(view) / area(SimView::AMIGTA1)).max(AMIGTA1_PEDS)
    }

    fn rand(&mut self) -> u64 {
        self.rng = self.rng.wrapping_mul(1_103_515_245).wrapping_add(12345);
        (self.rng >> 16) & 0x7FFF
    }

    /// The view in blocks either side of its centre, and the player's
    /// heading and whether he is moving (the spawner's edge).
    pub fn set_view(&mut self, half_w: i32, half_h: i32, angle: i32, moving: bool) {
        self.view_hw = half_w;
        self.view_hh = half_h;
        self.player_angle = angle & 255;
        self.player_moving = moving;
    }

    /// Where the player is, for the police: in a car, its half extents
    /// and heading (px), and its driver's door.
    #[allow(clippy::too_many_arguments)]
    pub fn set_player(
        &mut self,
        x: i64,
        y: i64,
        layer: i32,
        car: Option<(i32, i32, i32)>,
        door: (i64, i64),
        slow: bool,
    ) {
        self.pl_x = x;
        self.pl_y = y;
        self.pl_layer = layer;
        self.pl_in_car = car.is_some();
        let (hl, hw, face) = car.unwrap_or((0, 0, 0));
        self.pl_hl = i64::from(hl);
        self.pl_hw = i64::from(hw);
        self.pl_face = face;
        self.pl_door_x = door.0;
        self.pl_door_y = door.1;
        self.pl_slow = slow && car.is_some();
    }

    /// Everybody on his feet, as obstacles for the fleet: alive and not
    /// lying down. AmiGTA1 lists the bodies too, and so do we.
    pub fn walkers(&self) -> impl Iterator<Item = (i64, i64, i32)> + '_ {
        self.p
            .iter()
            .filter(|p| p.alive && p.down == 0 && p.drown == 0)
            .map(|p| (p.x, p.y, p.layer))
    }

    /// How many are alive (bodies included).
    pub fn count(&self) -> usize {
        self.p.iter().filter(|p| p.alive).count()
    }

    fn reset(&mut self, i: usize, x: i64, y: i64, layer: i32, angle: i32, remap: i32) {
        let frame = (self.rand() & 7) as i32;
        let remap = if remap >= 0 {
            remap
        } else {
            let r = REMAPS[self.remap_next % REMAPS.len()];
            self.remap_next += 1;
            i32::from(r)
        };
        // What a slot keeps from its last occupant (the block clock, the
        // gunman's settings, the cop's clocks) it keeps, as AmiGTA1's does.
        let p = &mut self.p[i];
        p.x = x;
        p.y = y;
        p.layer = layer;
        p.angle = angle & 255;
        p.svc_q = 0;
        p.frame = frame;
        p.frame_tick = 0;
        p.remap = remap;
        p.alive = true;
        p.mission = false;
        p.hunt_pl = false;
        p.serial = 0;
        p.mode = mode::IDLE;
        p.sub = sub::WANDER;
        p.speed = 1;
        p.timer = 0;
        p.wobble = 0;
        p.tx = 0;
        p.ty = 0;
        p.gx = 0;
        p.gy = 0;
        p.stuck = 0;
        p.dodge = 0;
        p.dodge_side = 1;
        p.bent = 0;
        p.offscreen = 0;
        p.flee_aim = 0;
        p.down = 0;
        p.corpse = false;
        p.bled = false;
        p.pull = -1;
        p.pull_tick = 0;
        p.shot = 0;
        p.shot_step = 0;
        p.shot_tick = 0;
        p.shot_dir = 0;
        p.fall = 0;
        p.panic = 0;
        p.burn = 0;
        p.burn_frame = 0;
        p.burn_tick = 0;
        p.cop = false;
        p.uniform = false;
        p.weapon = 0;
        p.arrest = false;
        p.cop_cool = 0;
        p.shoot_req = false;
        p.shoot_angle = 0;
        p.execute = false;
        p.post = false;
        p.cross_axis_x = false;
        p.drown = 0;
    }

    /// An empty slot, or failing that the body out of sight longest that
    /// no ambulance is coming for.
    fn free_slot(&self) -> Option<usize> {
        if let Some(i) = self.p.iter().position(|p| !p.alive) {
            return Some(i);
        }
        let mut best = None;
        let mut off = -1;
        for (i, p) in self.p.iter().enumerate() {
            if p.corpse && !p.mission && p.svc_q != 2 && p.offscreen > off {
                best = Some(i);
                off = p.offscreen;
            }
        }
        best
    }

    /// A slot even if somebody has to give one up: the living ped out of
    /// sight longest goes, so somebody the player is watching can arrive.
    fn free_slot_forced(&self) -> Option<usize> {
        if let Some(i) = self.free_slot() {
            return Some(i);
        }
        let mut best = None;
        let mut off = -1;
        for (i, p) in self.p.iter().enumerate() {
            if p.alive && !p.mission && p.offscreen > off {
                best = Some(i);
                off = p.offscreen;
            }
        }
        best
    }

    /// One try at a birth on the edge of the view ahead of the player.
    fn spawn_one(&mut self, map: &SimMap, cbx: i32, cby: i32) -> bool {
        let Some(i) = self.free_slot() else {
            return false;
        };
        let (x0, x1) = (cbx - self.view_hw - 1, cbx + self.view_hw + 1);
        let (y0, y1) = (cby - self.view_hh - 1, cby + self.view_hh + 1);
        // The edge he is heading for, or each edge in turn when he stands.
        let edge = if self.player_moving {
            match self.player_angle {
                a if !(32..224).contains(&a) => 3,
                a if a < 96 => 1,
                a if a < 160 => 2,
                _ => 0,
            }
        } else {
            let e = self.spawn_edge;
            self.spawn_edge = (self.spawn_edge + 1) & 3;
            e
        };
        // Eight in ten walk into the view.
        self.spawn_dir = (self.spawn_dir + 1) % 10;
        let inward = self.spawn_dir < 8;
        let (bx, by, heading) = match edge {
            0 => {
                let by = y0 + (self.rand() % (y1 - y0 + 1) as u64) as i32;
                (x0, by, if inward { 64 } else { 192 })
            }
            1 => {
                let by = y0 + (self.rand() % (y1 - y0 + 1) as u64) as i32;
                (x1, by, if inward { 192 } else { 64 })
            }
            2 => {
                let bx = x0 + (self.rand() % (x1 - x0 + 1) as u64) as i32;
                (bx, y1, if inward { 0 } else { 128 })
            }
            _ => {
                let bx = x0 + (self.rand() % (x1 - x0 + 1) as u64) as i32;
                (bx, y0, if inward { 128 } else { 0 })
            }
        };
        if !(1..=254).contains(&bx) || !(1..=254).contains(&by) {
            return false;
        }
        let x = (i64::from(bx) * 32 + 4 + (self.rand() % 24) as i64) << 16;
        let y = (i64::from(by) * 32 + 4 + (self.rand() % 24) as i64) << 16;
        if !pavement_at(map, x, y, 2) {
            return false;
        }
        let taken = self.p.iter().enumerate().any(|(j, o)| {
            o.alive && j != i && {
                let (dx, dy) = ((o.x - x) >> 16, (o.y - y) >> 16);
                dx > -4 && dx < 4 && dy > -4 && dy < 4
            }
        });
        if taken {
            return false;
        }
        self.reset(i, x, y, 2, heading, -1);
        self.stats.spawned += 1;
        self.spawned_since_retire = true;
        true
    }

    /// The driver of the car at `(cx, cy)` is dragged out of its door: put
    /// in the seat and walked through the six pull states against the car,
    /// then left lying beside it, then up and off. Takes the slot of
    /// somebody out of sight if it has to. `remap` -1 picks one.
    #[allow(clippy::too_many_arguments)]
    pub fn pull(
        &mut self,
        cars: &[CarInfo],
        cx: i64,
        cy: i64,
        face: i32,
        model: usize,
        layer: i32,
        remap: i32,
    ) -> Option<usize> {
        let i = self.free_slot_forced();
        self.last = i;
        let i = i?;
        let ci = &cars[model];
        let side = cars::door_side(ci) as i32;
        self.reset(i, cx, cy, layer, face + side * 64, remap);
        let p = &mut self.p[i];
        p.speed = 0;
        p.pull = 0;
        p.pull_tick = 0;
        p.pull_cx = cx;
        p.pull_cy = cy;
        p.pull_face = face & 255;
        p.pull_model = model;
        pull_place(cars, p);
        self.stats.spawned += 1;
        Some(i)
    }

    /// A bike's rider knocked off at `(x, y)` facing `angle`: the fall,
    /// lying, then up.
    pub fn knock_off(
        &mut self,
        x: i64,
        y: i64,
        layer: i32,
        angle: i32,
        remap: i32,
    ) -> Option<usize> {
        let i = self.free_slot_forced();
        self.last = i;
        let i = i?;
        self.reset(i, x, y, layer, angle, remap);
        let p = &mut self.p[i];
        p.fall = FALL_TICKS;
        p.speed = 0;
        p.tx = 0;
        p.ty = 0;
        self.stats.spawned += 1;
        Some(i)
    }

    /// Somebody put down at `(x, y)` facing `angle`, lying for `stun` ticks.
    /// Only into a free slot (or a body's).
    pub fn drop_ped(
        &mut self,
        x: i64,
        y: i64,
        layer: i32,
        angle: i32,
        remap: i32,
        stun: i32,
    ) -> bool {
        let Some(i) = self.free_slot() else {
            return false;
        };
        self.reset(i, x, y, layer, angle, remap);
        self.p[i].speed = 0;
        self.p[i].down = stun;
        self.stats.spawned += 1;
        true
    }

    /// Nobody about (tests that want an empty street).
    pub fn clear(&mut self) {
        for p in &mut self.p {
            p.alive = false;
        }
    }

    /// Killed where he stands.
    pub fn kill(&mut self, i: usize) {
        let Some(p) = self.p.get_mut(i) else {
            return;
        };
        if !p.alive || p.corpse {
            return;
        }
        p.corpse = true;
        if p.cop {
            self.stats.cops_killed += 1;
        }
        p.burn = 0;
        p.speed = 0;
        p.down = 0;
        p.fall = 0;
        p.shot = 0;
        p.tx = 0;
        p.ty = 0;
        self.stats.killed += 1;
    }

    /// The first ped a shot's box meets: the box runs `len_px` along
    /// `heading` from `(x, y)` and `hw_px` either side, each grown by a
    /// two-pixel body. Nearest along the line wins; `skip` is the shooter.
    /// Somebody already down, shot or being pulled out is not hit again.
    #[allow(clippy::too_many_arguments)]
    pub fn hit_segment(
        &self,
        x: i64,
        y: i64,
        heading: i32,
        len_px: i64,
        hw_px: i64,
        layer: i32,
        skip: Option<usize>,
    ) -> Option<usize> {
        let (fx, fy) = (i64::from(sin(heading)), -i64::from(cos(heading)));
        let (rx, ry) = (i64::from(cos(heading)), i64::from(sin(heading)));
        let mut best: Option<(usize, i64)> = None;
        for (i, p) in self.p.iter().enumerate() {
            if !p.alive || p.corpse || p.pull >= 0 || p.shot != 0 || p.layer != layer {
                continue;
            }
            if skip == Some(i) {
                continue;
            }
            let (dx, dy) = ((p.x - x) >> FP, (p.y - y) >> FP);
            if dx.abs() > 64 || dy.abs() > 64 {
                continue;
            }
            let along = (dx * fx + dy * fy) >> 14;
            let side = ((dx * rx + dy * ry) >> 14).abs();
            if along < -2 || along > len_px + 2 || side > hw_px + 2 {
                continue;
            }
            if best.is_none_or(|(_, b)| along < b) {
                best = Some((i, along));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Ped `i` takes a bullet travelling along `heading`. Within a quarter
    /// turn of it he was shot in the back: he keeps the bullet's heading
    /// and falls forward. Otherwise he turns to face the shooter and falls
    /// back. Either way the body is carried along the bullet's line.
    pub fn shoot(&mut self, i: usize, heading: i32) {
        let Some(p) = self.p.get_mut(i) else {
            return;
        };
        if !p.alive || p.corpse || p.shot != 0 {
            return;
        }
        let d = (p.angle - heading) & 255;
        if !(64..=192).contains(&d) {
            p.shot = 2;
            p.angle = heading & 255;
        } else {
            p.shot = 1;
            p.angle = (heading + 128) & 255;
        }
        p.shot_dir = heading & 255;
        p.shot_step = 0;
        p.shot_tick = 0;
        p.speed = 0;
        p.down = 0;
        p.fall = 0;
        p.tx = 0;
        p.ty = 0;
        self.stats.shot += 1;
    }

    /// A punch from `(x, y)` along `angle`: the original's 6×6-unit box
    /// 8 units ahead (3 px either way, 4 px ahead). The first ped standing
    /// in it is knocked down facing the puncher. Returns `Ok(i)` for him,
    /// `Err(true)` when the punch met him but missed (the counter-punch's
    /// turn on the original's wheel), `Err(false)` when it met nobody.
    pub fn punch(&mut self, x: i64, y: i64, angle: i32, layer: i32) -> Result<usize, bool> {
        let (ax, ay) = ahead(x, y, angle, 4);
        let hit = self.p.iter().position(|p| {
            p.alive
                && !p.corpse
                && p.pull < 0
                && p.shot == 0
                && p.fall == 0
                && p.down == 0
                && p.layer == layer
                && ((p.x - ax) >> FP).abs() <= 3
                && ((p.y - ay) >> FP).abs() <= 3
        });
        let i = hit.ok_or(false)?;
        self.punch_wheel = (self.punch_wheel + 1) % PUNCH_WHEEL;
        if self.punch_wheel >= PUNCH_LANDS {
            return Err(true);
        }
        let p = &mut self.p[i];
        p.angle = (angle + 128) & 255;
        p.fall = FALL_TICKS;
        p.speed = 0;
        p.tx = 0;
        p.ty = 0;
        p.mode = mode::IDLE;
        p.panic = 0;
        self.stats.punched += 1;
        Ok(i)
    }

    /// A shot at `(x, y)` frightens everybody on their feet in its two-block
    /// cell and the eight around it (AmiGTA1 reads the original's cell
    /// list): they run straight away from it, at full speed, for a while.
    pub fn panic(&mut self, x: i64, y: i64, layer: i32) {
        let cell = |v: i64| v >> (FP + BLOCK_SHIFT + 1);
        for i in 0..self.p.len() {
            let p = &self.p[i];
            if !p.alive
                || p.corpse
                || p.pull >= 0
                || p.shot != 0
                || p.fall != 0
                || p.down != 0
                || p.layer != layer
                || p.speed >= 4
            {
                continue;
            }
            if (cell(p.x) - cell(x)).abs() > 1 || (cell(p.y) - cell(y)).abs() > 1 {
                continue;
            }
            let still = (p.x - x) >> FP == 0 && (p.y - y) >> FP == 0;
            let angle = if still {
                (self.rand() & 255) as i32
            } else {
                angle_to(x, y, p.x, p.y)
            };
            let p = &mut self.p[i];
            p.mode = mode::FLEE;
            p.gx = x;
            p.gy = y;
            p.flee_aim = 0;
            p.tx = 0;
            p.ty = 0;
            p.speed = 4;
            p.panic = PANIC_TICKS;
            p.angle = angle;
        }
    }

    /// Ped `i` is set alight by a fire at `(fx, fy)`: he runs from it and
    /// burns to death in [`BURN_TICKS`].
    pub fn burn(&mut self, i: usize, fx: i64, fy: i64) {
        let Some(p) = self.p.get_mut(i) else {
            return;
        };
        if !p.alive || p.corpse || p.shot != 0 || p.pull >= 0 || p.burn > 0 {
            return;
        }
        p.burn = BURN_TICKS;
        p.burn_frame = 0;
        p.burn_tick = 0;
        p.mode = mode::FLEE;
        p.gx = fx;
        p.gy = fy;
        p.flee_aim = 0;
        p.panic = PANIC_TICKS;
        p.tx = 0;
        p.ty = 0;
        p.speed = 4;
        p.down = 0;
        p.fall = 0;
        if (p.x >> FP) != (fx >> FP) || (p.y >> FP) != (fy >> FP) {
            p.angle = angle_to(fx, fy, p.x, p.y);
        }
    }

    /// The player's car against the crowd, centred at `(px, py)` facing
    /// `pface` with half extents in px. At `speed` five units or more
    /// (16.16 px a physics step) a ped in the box is killed where he
    /// stands; slower, he is shoved along his own heading. Returns how
    /// many were killed standing.
    #[allow(clippy::too_many_arguments)]
    pub fn ram(
        &mut self,
        px: i64,
        py: i64,
        pface: i32,
        phl: i32,
        phw: i32,
        layer: i32,
        speed: i64,
    ) -> u32 {
        let (fx, fy) = (i64::from(sin(pface)), -i64::from(cos(pface)));
        let (rx, ry) = (i64::from(cos(pface)), i64::from(sin(pface)));
        let (phl, phw) = (i64::from(phl), i64::from(phw));
        let fast = speed >= 5 * 32768 || speed <= -5 * 32768;
        let mut hits = 0;
        for p in &mut self.p {
            if !p.alive || p.corpse || p.pull >= 0 || p.layer != layer || p.drown > 0 {
                continue;
            }
            // A cop at his door holds the car.
            if p.pulling > 0 {
                continue;
            }
            let (dx, dy) = ((p.x - px) >> 16, (p.y - py) >> 16);
            if !(-64..=64).contains(&dx) || !(-64..=64).contains(&dy) {
                continue;
            }
            let along = ((dx * (fx >> 6) + dy * (fy >> 6)) >> 8).abs();
            let side = ((dx * (rx >> 6) + dy * (ry >> 6)) >> 8).abs();
            // A man lying flat has no 3 px of body and must be under the
            // car proper.
            if p.down > 0 && (along > phl - 2 || side > phw - 2) {
                continue;
            }
            if along > phl + 3 || side > phw + 3 {
                continue;
            }
            if fast {
                p.corpse = true;
                if p.cop {
                    self.stats.cops_killed += 1;
                }
                if p.down == 0 {
                    p.speed = 0;
                    p.tx = 0;
                    p.ty = 0;
                    self.stats.killed += 1;
                    self.stats.runover += 1;
                    hits += 1;
                }
            } else if p.down == 0 {
                p.x += i64::from(sin(p.angle)) * 2;
                p.y -= i64::from(cos(p.angle)) * 2;
                p.speed = 0;
                p.timer = 3 * 3 / 2;
            }
        }
        hits
    }

    /// The gait wheel's pick, when a ped's gait runs out. No rest within
    /// sight of a corner.
    fn pick_gait(&mut self, p: &mut Ped, corner_ahead: bool) {
        let w = self.gait_wheel;
        let frames = |n: u64| (n * 3 / 2) as i32;
        if w <= 8 {
            p.sub = sub::WANDER;
            p.speed = 1;
            p.timer = frames(self.rand() % 400);
        } else if w <= 14 {
            p.sub = sub::BRISK;
            p.speed = 2;
            p.timer = frames(self.rand() % 200);
        } else if w == 15 {
            p.sub = sub::JOG;
            p.speed = 3;
            p.timer = frames(self.rand() % 200);
        } else if w <= 19 {
            if p.sub == sub::BLIP {
                p.sub = sub::WANDER;
                p.speed = 1;
                p.timer = frames(self.rand() % 400);
            } else {
                p.sub = sub::BLIP;
                p.speed = 1;
                p.timer = frames(self.rand() % 0x40);
            }
        } else if corner_ahead {
            p.sub = sub::WANDER;
            p.speed = 1;
            p.timer = frames(60);
        } else {
            p.sub = sub::STAND;
            p.speed = 0;
            p.timer = frames(self.rand() % 200);
        }
    }

    /// Up to 8 steps either way, never the same sign twice running.
    fn wobble(&mut self, p: &mut Ped) {
        let mut w = (self.rand() % 16) as i32 - 8;
        if (p.wobble < 0 && w < 0) || (p.wobble > 0 && w > 0) {
            w = -w;
        }
        p.wobble = w;
        p.angle = (p.angle + w) & 255;
    }

    /// The pavement ends ahead: aim at a spread point on the pavement to
    /// one side. False when there is none either side.
    fn corner(&mut self, map: &SimMap, p: &mut Ped) -> bool {
        let a = snap_cardinal(p.angle);
        let (lx, ly) = ahead(p.x, p.y, (a - 64) & 255, 32);
        let (rx, ry) = ahead(p.x, p.y, (a + 64) & 255, 32);
        let left = pavement_at(map, lx, ly, p.layer);
        let right = pavement_at(map, rx, ry, p.layer);
        if !left && !right {
            return false;
        }
        let side = if left && right {
            if self.rand() & 1 != 0 { -1 } else { 1 }
        } else if left {
            -1
        } else {
            1
        };
        let (sx, sy) = ahead(p.x, p.y, (a + side * 64) & 255, 32);
        let fwd = (self.rand() % 21) as i64;
        let lat = (self.rand() % 21) as i64 - 10;
        let (sx, sy) = ahead(sx, sy, a, fwd);
        let (sx, sy) = ahead(sx, sy, (a + 64) & 255, lat);
        p.tx = sx;
        p.ty = sy;
        p.angle = a;
        true
    }

    /// Somebody within 5 px ahead: lean away from him. True if anybody is.
    fn separation(&self, i: usize, p: &mut Ped) -> bool {
        let (fx, fy) = (i64::from(sin(p.angle)), -i64::from(cos(p.angle)));
        let mut crowded = false;
        for (k, o) in self.p.iter().enumerate() {
            if k == i || !o.alive || o.corpse || o.pull >= 0 || o.layer != p.layer {
                continue;
            }
            let (dx, dy) = ((o.x - p.x) >> 16, (o.y - p.y) >> 16);
            if !(-6..=6).contains(&dx) || !(-6..=6).contains(&dy) {
                continue;
            }
            let along = (dx * fx + dy * fy) >> 14;
            let side = (dx * fy - dy * fx) >> 14;
            if !(0..=5).contains(&along) {
                continue;
            }
            crowded = true;
            p.angle = (p.angle + if side >= 0 { -6 } else { 6 }) & 255;
        }
        crowded
    }

    fn shove(map: &SimMap, p: &mut Ped, px: i64, py: i64) {
        if walkable(map, p.x + px, p.y + py, p.layer) {
            p.x += px;
            p.y += py;
        } else if walkable(map, p.x + px, p.y, p.layer) {
            p.x += px;
        } else if walkable(map, p.x, p.y + py, p.layer) {
            p.y += py;
        }
    }

    /// Nobody stands inside anybody: every overlapping pair is pushed
    /// apart along the line between them, a pixel when deep, half when
    /// shallow, and slot order splits two on the same point.
    fn relax(&mut self, map: &SimMap) {
        let n = self.p.len();
        for a in 0..n {
            if !self.p[a].alive || self.p[a].corpse || self.p[a].pull >= 0 {
                continue;
            }
            for b in a + 1..n {
                let (pa, pb) = (self.p[a], self.p[b]);
                if !pb.alive || pb.corpse || pb.pull >= 0 || pb.layer != pa.layer {
                    continue;
                }
                let (dx, dy) = ((pb.x - pa.x) >> 16, (pb.y - pa.y) >> 16);
                if !(-APART_PX..=APART_PX).contains(&dx) || !(-APART_PX..=APART_PX).contains(&dy) {
                    continue;
                }
                let d2 = dx * dx + dy * dy;
                if d2 >= APART_PX * APART_PX {
                    continue;
                }
                let step: i64 = if d2 <= 4 { 1 << 16 } else { 1 << 15 };
                let (px, py) = if dx == 0 && dy == 0 {
                    (step, 0)
                } else {
                    let big = dx.abs().max(dy.abs());
                    (step * dx / big, step * dy / big)
                };
                Self::shove(map, &mut self.p[b], px, py);
                Self::shove(map, &mut self.p[a], -px, -py);
            }
        }
    }

    /// The block clock behind the walk-away rule: how long a ped who
    /// wants to move has been in the same block.
    fn block_clock(p: &mut Ped) {
        let (bx, by) = ((p.x >> 21) as i32, (p.y >> 21) as i32);
        if !p.alive || p.corpse || p.pull >= 0 || p.down > 0 || p.fall > 0 {
            p.blk_ticks = 0;
            p.blk_x = bx;
            p.blk_y = by;
            return;
        }
        if bx != p.blk_x || by != p.blk_y {
            p.blk_x = bx;
            p.blk_y = by;
            p.blk_ticks = 0;
            return;
        }
        if p.speed > 0 {
            p.blk_ticks += 1;
        }
    }

    /// How far a cop is from the player: to the nearest bodywork when he is
    /// in a car, and whether the cop is under it.
    fn dist_to_player(&self, p: &Ped) -> (i64, bool) {
        let (dx, dy) = ((self.pl_x - p.x) >> 16, (self.pl_y - p.y) >> 16);
        if !self.pl_in_car {
            return (dx.abs().max(dy.abs()), false);
        }
        let (fx, fy) = (i64::from(sin(self.pl_face)), -i64::from(cos(self.pl_face)));
        let (rx, ry) = (i64::from(cos(self.pl_face)), i64::from(sin(self.pl_face)));
        let along = ((dx * (fx >> 6) + dy * (fy >> 6)) >> 8).abs();
        let side = ((dx * (rx >> 6) + dy * (ry >> 6)) >> 8).abs();
        let inside = along < self.pl_hl && side < self.pl_hw;
        let along = (along - self.pl_hl).max(0);
        let side = (side - self.pl_hw).max(0);
        (along.max(side), inside)
    }

    /// One tick for everybody: the pulls, the bodies, the brains, then the
    /// spawner and the push apart.
    pub fn tick(&mut self, ctx: &TickCtx, say: &mut dyn FnMut(String)) {
        let (cbx, cby) = ((ctx.cam.0 >> 21) as i32, (ctx.cam.1 >> 21) as i32);
        let mut alive = 0;
        for i in 0..self.p.len() {
            if !self.p[i].alive {
                continue;
            }
            alive += 1;
            let mut p = self.p[i];
            let (bx, by) = ((p.x >> 21) as i32, (p.y >> 21) as i32);
            let (hw, hh) = (self.view_hw, self.view_hh);
            let in_view = bx >= cbx - hw - 2
                && bx <= cbx + hw + 2
                && by >= cby - hh - 2
                && by <= cby + hh + 2;
            if in_view {
                p.offscreen = 0;
            } else {
                p.offscreen += 1;
                let far = bx < cbx - 24 || bx > cbx + 24 || by < cby - 24 || by > cby + 24;
                if !p.mission
                    && ((p.corpse && p.offscreen > CORPSE_TICKS)
                        || (!p.corpse && p.pull < 0 && p.offscreen > RETIRE_TICKS)
                        || far)
                {
                    p.alive = false;
                    self.spawned_since_retire = false;
                    self.p[i] = p;
                    continue;
                }
            }
            self.gait_wheel = self.gait_wheel % 20 + 1;
            let p = self.think(i, p, ctx, say);
            self.p[i] = p;
        }
        if alive < self.p.len() && !self.spawn_one(ctx.map, cbx, cby) && !self.spawned_since_retire
        {
            self.spawn_one(ctx.map, cbx, cby);
        }
        self.relax(ctx.map);
    }

    /// One ped's tick after the view check.
    fn think(&mut self, i: usize, mut p: Ped, ctx: &TickCtx, say: &mut dyn FnMut(String)) -> Ped {
        let map = ctx.map;
        // Last tick's sidestep bend: off again below, once he is on his
        // feet; a tick spent down, pulled or burning keeps the heading.
        let bent = std::mem::take(&mut p.bent);
        if p.pull >= 0 {
            p.pull_tick += 1;
            if p.pull_tick >= PULL_TICKS {
                p.pull_tick = 0;
                p.pull += 1;
                if p.pull == 2 {
                    p.angle = p.pull_face;
                }
                if p.pull >= PULL_STATES {
                    let side = cars::door_side(&ctx.cars[p.pull_model]) as i32;
                    p.pull = -1;
                    p.down = PULL_DOWN_TICKS;
                    p.angle = (p.pull_face + side * 64) & 255;
                    return p;
                }
            }
            pull_place(ctx.cars, &mut p);
            return p;
        }
        if p.corpse {
            return p;
        }
        if p.drown > 0 {
            // Gone under: nothing more to do but last.
            p.drown += 1;
            if p.drown > DROWN_TICKS {
                p.alive = false;
                self.stats.drowned += 1;
                self.stats.killed += 1;
                say(format!("ped: {i} drowned"));
            }
            return p;
        }
        // The script's own stands where he was put.
        if p.mode == mode::MISSION && p.burn <= 0 && p.down == 0 {
            p.speed = 0;
            return p;
        }
        if p.burn > 0 {
            p.burn_tick += 1;
            if p.burn_tick >= FIRE_FRAME_TICKS {
                p.burn_tick = 0;
                p.burn_frame = (p.burn_frame + 1) % FIRE_FRAMES;
            }
            p.burn -= 1;
            if p.burn == 0 {
                p.corpse = true;
                p.bled = true;
                if p.cop {
                    self.stats.cops_killed += 1;
                }
                p.speed = 0;
                p.tx = 0;
                p.ty = 0;
                self.stats.killed += 1;
                return p;
            }
            if p.down == 0 && p.fall == 0 && p.pull < 0 {
                p.speed = 4;
            }
        }
        if p.shot != 0 {
            // Carried along the bullet's line for three states (a wall
            // stops it), then a body.
            if p.shot_step < SHOT_STATES - 1 {
                let nx = p.x + i64::from(sin(p.shot_dir)) * 3;
                let ny = p.y - i64::from(cos(p.shot_dir)) * 3;
                if ground_at(map, nx, ny, p.layer) != ground::BUILDING {
                    p.x = nx;
                    p.y = ny;
                }
            }
            p.shot_tick += 1;
            if p.shot_tick >= SHOT_STATE_TICKS {
                p.shot_tick = 0;
                p.shot_step += 1;
                if p.shot_step >= SHOT_STATES {
                    p.corpse = true;
                    if p.cop {
                        self.stats.cops_killed += 1;
                    }
                    p.speed = 0;
                    p.down = 0;
                    p.tx = 0;
                    p.ty = 0;
                    self.stats.killed += 1;
                }
            }
            return p;
        }
        if p.fall > 0 {
            // Knocked back half a px a tick (away from his heading: he
            // faces whoever hit him), then lying.
            let nx = p.x - i64::from(sin(p.angle)) * 2;
            let ny = p.y + i64::from(cos(p.angle)) * 2;
            if ground_at(map, nx, ny, p.layer) != ground::BUILDING {
                p.x = nx;
                p.y = ny;
            }
            p.fall -= 1;
            if p.fall == 0 {
                p.down = PUNCH_DOWN_TICKS;
            }
            return p;
        }
        if ground_at(map, p.x, p.y, p.layer) == ground::WATER {
            // In the water: he goes under (ours; AmiGTA1's stand on it).
            p.drown = 1;
            p.speed = 0;
            p.down = 0;
            p.tx = 0;
            p.ty = 0;
            say(format!(
                "ped: {i} in the water at ({},{})",
                p.x >> 21,
                p.y >> 21
            ));
            return p;
        }
        if p.panic > 0 {
            p.panic -= 1;
        }
        if p.down > 0 {
            // Up when the count runs out, a quarter turn round.
            p.down -= 1;
            if p.down == 0 {
                p.angle = (p.angle + 64) & 255;
                p.mode = mode::IDLE;
                p.sub = sub::WANDER;
                p.speed = 1;
                p.timer = 0;
            }
            return p;
        }

        // Back to the heading the sidestep bent, so a mode that keeps his
        // heading from tick to tick sees his own and not the bent one.
        p.angle = (p.angle - bent) & 255;
        let here = ground_at(map, p.x, p.y, p.layer);
        if p.cop {
            if !self.cop_brain(&mut p, say) {
                return p;
            }
        } else if p.mode == mode::HUNT || p.mode == mode::SENDTO {
            self.script_brain(i, &mut p, map, say);
        } else if p.mode == mode::CROSS {
            let (dx, dy) = ((p.tx - p.x) >> 16, (p.ty - p.y) >> 16);
            let d = dx.abs().max(dy.abs());
            if p.sub == sub::STAND {
                let (bx, by) = ((p.x >> 21) as i32, (p.y >> 21) as i32);
                if (ctx.green)(bx, by, p.cross_axis_x) {
                    p.sub = sub::JOG;
                    p.speed = 4;
                    self.stats.crossings += 1;
                    say(format!(
                        "ped: {i} sets off across at ({},{}) to ({},{})",
                        p.x >> 16,
                        p.y >> 16,
                        p.tx >> 16,
                        p.ty >> 16
                    ));
                }
            } else {
                p.angle = angle_to(p.x, p.y, p.tx, p.ty);
                p.speed = if d > 6 { 4 } else { 2 };
                if d <= 4 {
                    p.mode = mode::IDLE;
                    p.sub = sub::WANDER;
                    p.speed = 1;
                    p.timer = 0;
                    p.tx = 0;
                    p.ty = 0;
                    p.angle = snap_cardinal(p.angle);
                    self.stats.crossed += 1;
                    say(format!("ped: {i} crossed"));
                }
            }
        } else if here == ground::ROAD && p.mode != mode::FLEE {
            // On the road: run from where he stepped on.
            p.mode = mode::FLEE;
            p.gx = p.x;
            p.gy = p.y;
            p.flee_aim = 0;
            p.tx = 0;
            p.ty = 0;
        }
        if p.mode == mode::FLEE {
            let (dgx, dgy) = ((p.x - p.gx) >> 16, (p.y - p.gy) >> 16);
            if here == ground::PAVEMENT
                && p.panic == 0
                && (!(-32..=32).contains(&dgx) || !(-32..=32).contains(&dgy))
            {
                p.mode = mode::IDLE;
                p.sub = sub::WANDER;
                p.speed = 1;
                p.timer = 0;
                p.angle = snap_cardinal(p.angle);
            } else {
                p.speed = 4;
                p.flee_aim += 1;
                if p.flee_aim >= 5 * 3 / 2 {
                    p.flee_aim = 0;
                    if dgx != 0 || dgy != 0 {
                        p.angle = angle_to(p.gx, p.gy, p.x, p.y);
                    }
                }
            }
        }

        // A lit crossing: one tick in eleven, an idle ped on a light block
        // tries the next of the four ways round. The next block that way
        // must be a light on the road, with pavement within eight blocks.
        if p.mode == mode::IDLE
            && p.tx == 0
            && p.down == 0
            && self.rand().is_multiple_of(11)
            && hint_at(map, p.x, p.y, p.layer) == 1
        {
            const SX: [i32; 4] = [0, 1, 0, -1];
            const SY: [i32; 4] = [-1, 0, 1, 0];
            let d = (self.cross_rr & 3) as usize;
            let (bx, by) = ((p.x >> 21) as i32, (p.y >> 21) as i32);
            self.cross_rr = (self.cross_rr + 1) & 3;
            let at = |k: i32| {
                (
                    i64::from(bx + SX[d] * k) << 21,
                    i64::from(by + SY[d] * k) << 21,
                )
            };
            let (nx, ny) = at(1);
            if hint_at(map, nx, ny, p.layer) == 1 && ground_at(map, nx, ny, p.layer) == ground::ROAD
            {
                let mut found = 0;
                for k in 2..=8 {
                    let (kx, ky) = at(k);
                    let g = ground_at(map, kx, ky, p.layer);
                    if g == ground::PAVEMENT {
                        found = k;
                        break;
                    } else if g != ground::ROAD {
                        break;
                    }
                }
                if found != 0 {
                    p.mode = mode::CROSS;
                    p.sub = sub::STAND;
                    p.speed = 0;
                    p.cross_axis_x = SX[d] != 0;
                    p.tx = (i64::from(bx + SX[d] * found) * 32 + 16) << 16;
                    p.ty = (i64::from(by + SY[d] * found) * 32 + 16) << 16;
                    p.angle = (d as i32 * 64) & 255;
                }
            }
        }

        if p.mode == mode::IDLE {
            let (ax, ay) = ahead(p.x, p.y, snap_cardinal(p.angle), 24);
            let corner_ahead = !pavement_at(map, ax, ay, p.layer);
            if p.tx == 0 {
                p.timer -= 1;
                if p.timer <= 0 {
                    self.pick_gait(&mut p, corner_ahead);
                }
                if p.sub == sub::STAND {
                    // Shifting on his feet, through a flag they all share.
                    if !self.fidget {
                        let r = self.rand();
                        if r < 6000 {
                            self.fidget = true;
                            p.angle = (p.angle - 8) & 255;
                        } else if r > 26000 {
                            self.fidget = true;
                            p.angle = (p.angle + 8) & 255;
                        }
                    } else if self.rand() < 16000 {
                        self.fidget = false;
                    }
                } else if p.sub == sub::VEER_L || p.sub == sub::VEER_R {
                    // Four frames back towards the pavement, then straight.
                    if p.timer <= 100 * 3 / 2 - 4 * 3 / 2 {
                        p.angle = snap_cardinal(p.angle);
                        p.sub = sub::WANDER;
                        p.speed = 1;
                    }
                } else {
                    if p.sub == sub::BLIP || self.rand() > 24000 {
                        self.wobble(&mut p);
                    }
                    if corner_ahead {
                        // Nowhere to the side either: carry on, and let the
                        // step decide.
                        self.corner(map, &mut p);
                    } else if p.angle & 63 != 0 {
                        // Drifting at a slant: pavement's edge 6 px to a side
                        // turns him 45 degrees back.
                        let (qx, qy) = ahead(p.x, p.y, (p.angle - 64) & 255, 6);
                        if !pavement_at(map, qx, qy, p.layer) {
                            p.sub = sub::VEER_R;
                            p.angle = (p.angle + 32) & 255;
                            p.timer = 100 * 3 / 2;
                        } else {
                            let (qx, qy) = ahead(p.x, p.y, (p.angle + 64) & 255, 6);
                            if !pavement_at(map, qx, qy, p.layer) {
                                p.sub = sub::VEER_L;
                                p.angle = (p.angle - 32) & 255;
                                p.timer = 100 * 3 / 2;
                            }
                        }
                    }
                }
            } else {
                // To the corner target; within 9 px it is reached.
                let (dx, dy) = ((p.tx - p.x) >> 16, (p.ty - p.y) >> 16);
                if dx > -9 && dx < 9 && dy > -9 && dy < 9 {
                    p.tx = 0;
                    p.ty = 0;
                    p.angle = snap_cardinal(p.angle);
                } else {
                    p.angle = angle_to(p.x, p.y, p.tx, p.ty);
                }
                if p.speed == 0 {
                    p.speed = 1;
                }
            }
        }

        // Keeping apart, before the step: a shuffle and a sidestep, never a
        // stop.
        let crowded = self.separation(i, &mut p);
        if crowded && p.speed > 1 && p.mode == mode::IDLE {
            p.speed = 1;
        }
        let free_to_dodge = crowded && p.dodge == 0 && p.speed > 0 && p.pull < 0 && !p.arrest;
        if free_to_dodge
            && p.blk_ticks > LEAVE_TICKS
            && p.mode == mode::IDLE
            && p.tx == 0
            && p.ty == 0
        {
            // Stuck in a crowd for two seconds with nowhere to be: turn
            // his back on the nearest body and walk off.
            let mut best = None;
            let mut bd = 0;
            for (k, o) in self.p.iter().enumerate() {
                if k == i || !o.alive || o.corpse || o.layer != p.layer {
                    continue;
                }
                let (ddx, ddy) = ((o.x - p.x) >> 16, (o.y - p.y) >> 16);
                let d = ddx * ddx + ddy * ddy;
                if best.is_none() || d < bd {
                    best = Some(k);
                    bd = d;
                }
            }
            if let Some(b) = best {
                const TURN: [i32; 4] = [0, 64, -64, 128];
                let o = self.p[b];
                let away = snap_cardinal(angle_to(o.x, o.y, p.x, p.y));
                for t in TURN {
                    if step_free(map, &p, (away + t) & 255, 12) {
                        p.angle = (away + t) & 255;
                        p.dodge = DODGE_TICKS * 2;
                        p.dodge_side = 0;
                        break;
                    }
                }
            }
        }
        if crowded && p.dodge == 0 && p.speed > 0 && p.pull < 0 && !p.arrest {
            if step_free(map, &p, (p.angle + DODGE_TURN) & 255, 8) {
                p.dodge = DODGE_TICKS;
                p.dodge_side = 1;
            } else if step_free(map, &p, (p.angle - DODGE_TURN) & 255, 8) {
                p.dodge = DODGE_TICKS;
                p.dodge_side = -1;
            }
        }
        if p.dodge > 0 {
            let a = (p.angle + DODGE_TURN * p.dodge_side) & 255;
            p.dodge -= 1;
            if p.dodge_side != 0 && p.speed > 0 {
                if step_free(map, &p, a, 8) {
                    p.angle = a;
                    p.bent = DODGE_TURN * p.dodge_side;
                } else {
                    // Walled on that side: his own heading for the rest of
                    // the sidestep (ours: AmiGTA1 tries the side again
                    // next tick, and a man along a kerb turned a quarter
                    // and back on alternate ticks).
                    p.dodge_side = 0;
                }
            }
            if p.speed == 1 {
                p.speed = 2;
            }
        }

        if p.speed > 0 {
            self.step(&mut p, here, map, i, say);
        }
        Self::block_clock(&mut p);
        // The cadence: the run cycle from speed 3.
        if p.speed > 0 {
            p.frame_tick += 1;
            if p.frame_tick >= if p.speed >= 3 { RUN_TICKS } else { WALK_TICKS } {
                p.frame_tick = 0;
                p.frame = (p.frame + 1) & 7;
            }
        }
        p
    }

    /// The step along his heading, and what refuses it.
    fn step(&mut self, p: &mut Ped, here: u8, map: &SimMap, i: usize, say: &mut dyn FnMut(String)) {
        let a0 = p.angle;
        self.step_on(p, here, map, i, say);
        // Turned away from a wall or out of a jam: a new heading of his
        // own, not a bent one.
        if p.angle != a0 {
            p.bent = 0;
        }
    }

    fn step_on(
        &mut self,
        p: &mut Ped,
        here: u8,
        map: &SimMap,
        i: usize,
        say: &mut dyn FnMut(String),
    ) {
        let v = (UNIT * i64::from(p.speed)) >> 8;
        let nx = p.x + ((i64::from(sin(p.angle)) * v) >> 6);
        let ny = p.y - ((i64::from(cos(p.angle)) * v) >> 6);
        let mut there = ground_at(map, nx, ny, p.layer);
        // A fence or a shut door is a wall to a man on foot.
        if map.wall_cross(p.layer, p.x as i32, p.y as i32, nx as i32, ny as i32) {
            there = ground::BUILDING;
        }
        let steered = p.cop || p.mode == mode::HUNT || p.mode == mode::SENDTO;
        let blocked = if matches!(there, ground::BUILDING | ground::AIR | ground::WATER)
            || (there == ground::FIELD && here != ground::FIELD && !steered)
        {
            true
        } else if there == ground::ROAD
            && here != ground::ROAD
            && p.mode != mode::FLEE
            && !steered
            && p.mode != mode::CROSS
        {
            // A road only with pavement a block and a half beyond it.
            let (fx, fy) = ahead(p.x, p.y, p.angle, 48);
            !pavement_at(map, fx, fy, p.layer)
        } else {
            false
        };
        if blocked && p.mode == mode::FLEE {
            p.angle = (p.angle + 64) & 255;
        } else if blocked && steered {
            // Round it, the other way next time.
            p.angle = (p.angle + if p.stuck & 1 != 0 { 48 } else { -48 }) & 255;
            p.stuck += 1;
        } else if blocked && p.mode == mode::CROSS {
            say(format!(
                "ped: {i} crossing abandoned at ({},{}): ground {there} ahead",
                p.x >> 16,
                p.y >> 16
            ));
            p.mode = mode::IDLE;
            p.sub = sub::WANDER;
            p.speed = 1;
            p.timer = 3 * 3 / 2;
            p.tx = 0;
            p.ty = 0;
            p.angle = (p.angle + 128) & 255;
        } else if blocked {
            // 135 degrees round, and a new gait in three frames.
            p.angle = (p.angle - 96) & 255;
            p.tx = 0;
            p.ty = 0;
            p.timer = 3 * 3 / 2;
            p.stuck = 0;
        } else {
            if nx == p.x && ny == p.y {
                p.stuck += 1;
                if p.stuck > 8 * 3 / 2 {
                    p.angle = (p.angle + 80) & 255;
                    p.stuck = 0;
                }
            } else {
                p.stuck = 0;
            }
            p.x = nx;
            p.y = ny;
        }
    }

    /// The policeman on foot. False when his tick ends here.
    fn cop_brain(&mut self, p: &mut Ped, say: &mut dyn FnMut(String)) -> bool {
        let (d, under) = self.dist_to_player(p);
        p.mode = mode::COP;
        if !self.pl_in_car {
            p.pulling = 0;
        }
        if p.arrest_wait > 0 {
            p.arrest_wait -= 1;
        }
        if under && p.pulling == 0 {
            // Out from under the car first; no arrest from there.
            p.angle = angle_to(self.pl_x, self.pl_y, p.x, p.y);
            p.speed = 2;
            p.arrest = false;
            return false;
        }
        if p.post && d > 192 {
            p.speed = 0;
            p.arrest = false;
            return false;
        }
        if p.exit_wait > 0 {
            p.exit_wait -= 1;
            p.speed = 0;
            p.arrest = false;
            return false;
        }
        if p.cop_cool > 0 {
            p.cop_cool -= 1;
        }
        if self.cop_shoot != 0 && self.pl_layer == p.layer && d <= 96 {
            p.angle = angle_to(p.x, p.y, self.pl_x, self.pl_y);
            p.speed = if d > 40 { 2 } else { 0 };
            p.arrest = false;
            if p.cop_cool == 0 && d > 8 {
                p.shoot_req = true;
                p.shoot_angle = p.angle;
                p.cop_cool = 15;
            }
            if self.cop_shoot >= 2 && d <= COP_ARREST_PX {
                p.execute = true;
            }
        } else if self.pl_in_car && self.pl_layer == p.layer {
            // A driver is taken at his door, once the car has all but
            // stopped; BUSTED is the end of the pull.
            let de = ((self.pl_door_x - p.x) >> 16)
                .abs()
                .max(((self.pl_door_y - p.y) >> 16).abs());
            if p.pulling > 0 {
                p.x = self.pl_door_x;
                p.y = self.pl_door_y;
                p.angle = angle_to(p.x, p.y, self.pl_x, self.pl_y);
                p.speed = 0;
                p.pulling += 1;
                if p.pulling >= COP_PULL_TICKS && !p.arrest {
                    p.arrest = true;
                    say(format!(
                        "police: pulled out of the car after {} ticks",
                        p.pulling
                    ));
                }
            } else if de <= COP_DOOR_PX && self.pl_slow && p.arrest_wait == 0 {
                p.pulling = 1;
                p.x = self.pl_door_x;
                p.y = self.pl_door_y;
                p.speed = 0;
                say("police: the cop opens his door".into());
            } else {
                p.angle = angle_to(p.x, p.y, self.pl_door_x, self.pl_door_y);
                p.speed = if de > 96 { 3 } else { 2 };
                if de <= COP_DOOR_PX {
                    p.x = self.pl_door_x;
                    p.y = self.pl_door_y;
                    p.speed = 0;
                }
                p.arrest = false;
            }
        } else if self.pl_layer == p.layer && d <= COP_ARREST_PX && p.arrest_wait == 0 {
            if !p.arrest {
                p.arrest = true;
                say(format!("police: the cop has him, {d} px away"));
            }
            p.speed = 0;
        } else {
            p.angle = angle_to(p.x, p.y, self.pl_x, self.pl_y);
            p.speed = if d > 96 { 3 } else { 2 };
            p.arrest = false;
        }
        true
    }

    /// The script's own person with a target: straight at it over
    /// whatever ground, turning up to a right angle round what is in the
    /// way.
    fn script_brain(&mut self, i: usize, p: &mut Ped, map: &SimMap, say: &mut dyn FnMut(String)) {
        let (tx, ty) = if p.hunt_pl {
            (self.pl_x, self.pl_y)
        } else {
            (p.tx, p.ty)
        };
        let d = ((tx - p.x) >> 16).abs().max(((ty - p.y) >> 16).abs());
        let mut want = angle_to(p.x, p.y, tx, ty);
        if !step_free(map, p, want, 12) {
            const OFF: [i32; 6] = [32, 64, 96, -32, -64, -96];
            let base = if p.wobble < 0 { 3 } else { 0 };
            for k in 0..6 {
                let o = OFF[(base + k) % 6];
                if step_free(map, p, (want + o) & 255, 12) {
                    want = (want + o) & 255;
                    p.wobble = if o < 0 { -1 } else { 1 };
                    break;
                }
            }
        }
        p.angle = want;
        if p.mode == mode::SENDTO {
            p.speed = if d > 8 {
                if p.send_speed != 0 { p.send_speed } else { 2 }
            } else {
                0
            };
            if d <= 8 {
                p.mode = mode::MISSION;
                p.speed = 0;
                p.tx = 0;
                p.ty = 0;
            }
        } else if p.gang != 0 && p.hunt_pl {
            // A gunman: AmiGTA1's counts at 50 Hz.
            let (sdx, sdy) = ((self.pl_x - p.x) >> 16, (self.pl_y - p.y) >> 16);
            let on_screen =
                sdx > -190 && sdx < 190 && sdy > -120 && sdy < 120 && self.pl_layer == p.layer;
            let near = d < if self.pl_in_car { 48 } else { 16 };
            p.speed = if p.gang == 1 || near || p.gang_burst {
                0
            } else {
                3
            };
            if p.gang_cool > 0 {
                p.gang_cool -= 1;
            }
            if !on_screen {
                p.gang_cool = if p.gang == 1 { 0 } else { 100 };
                p.gang_burst = false;
            } else if p.gang_cool == 0 {
                let lim = if p.gang_miss == 8 { 10 } else { 5 };
                let mut turn = 0;
                if !self.pl_in_car && p.gang_miss != 0 {
                    if self.gang_aim > lim {
                        self.gang_aim = 0;
                    } else {
                        let miss = i64::from(p.gang_miss);
                        let r = if d > 32 {
                            miss
                        } else if d > 16 {
                            miss / 2
                        } else {
                            0
                        };
                        if r > 0 && d > 0 {
                            turn = ((r * 41 + d - 1) / d) as i32;
                        }
                        if self.gang_side {
                            turn = -turn;
                        }
                        self.gang_aim += 1;
                    }
                    self.gang_side = !self.gang_side;
                }
                p.shoot_req = true;
                p.shoot_angle = (angle_to(p.x, p.y, self.pl_x, self.pl_y) + turn) & 255;
                p.angle = p.shoot_angle;
                say(format!("ped: gunman {i} fires at {d} px"));
                if p.gang == 1 {
                    if !p.gang_burst {
                        p.gang_burst = true;
                        p.gang_cool = 30;
                    } else {
                        p.gang_burst = false;
                        p.gang_cool = 100;
                    }
                } else {
                    p.gang_cool = if near { 32 } else { 430 };
                    p.gang_burst = false;
                }
            }
        } else {
            // A hunter without a gun closes in and stands over him.
            p.speed = if d > 96 {
                3
            } else if d > 20 {
                2
            } else {
                0
            };
        }
    }

    /// The pool as trace lines, one per live ped:
    /// `p <tick> <slot> <x> <y> <layer> <angle> <mode> <sub> <speed> <frame> <corpse> <down> <pull>`.
    pub fn trace_lines(&self, tick: u32) -> Vec<String> {
        self.p
            .iter()
            .enumerate()
            .filter(|(_, p)| p.alive)
            .map(|(i, p)| {
                format!(
                    "p {tick} {i} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
                    p.x,
                    p.y,
                    p.layer,
                    p.angle,
                    p.mode,
                    p.sub,
                    p.speed,
                    p.frame,
                    i32::from(p.corpse),
                    p.down,
                    p.pull,
                    p.shot,
                    p.fall,
                    p.burn
                )
            })
            .collect()
    }
}

impl SimHash for Ped {
    fn sim_hash(&self, h: &mut StateHasher) {
        for v in [
            self.x,
            self.y,
            self.tx,
            self.ty,
            self.gx,
            self.gy,
            self.pull_cx,
            self.pull_cy,
        ] {
            h.bytes(&v.to_le_bytes());
        }
        for v in [
            self.layer,
            self.angle,
            self.frame,
            self.frame_tick,
            self.remap,
            self.mode,
            self.gang_cool,
            self.send_speed,
            self.sub,
            self.speed,
            self.timer,
            self.wobble,
            self.stuck,
            self.dodge,
            self.dodge_side,
            self.bent,
            self.blk_x,
            self.blk_y,
            self.blk_ticks,
            self.offscreen,
            self.flee_aim,
            self.down,
            self.pull,
            self.pull_tick,
            self.pull_face,
            self.pull_model as i32,
            self.shot,
            self.shot_step,
            self.shot_tick,
            self.shot_dir,
            self.fall,
            self.panic,
            self.burn,
            self.burn_frame,
            self.burn_tick,
            self.cop_cool,
            self.arrest_wait,
            self.shoot_angle,
            self.exit_wait,
            self.pulling,
            self.drown,
        ] {
            h.i32(v);
        }
        h.bytes(&self.serial.to_le_bytes());
        for v in [self.svc_q, self.gang, self.gang_miss, self.weapon] {
            h.u8(v);
        }
        for v in [
            self.alive,
            self.mission,
            self.hunt_pl,
            self.gang_burst,
            self.corpse,
            self.bled,
            self.cop,
            self.uniform,
            self.arrest,
            self.shoot_req,
            self.execute,
            self.post,
            self.cross_axis_x,
        ] {
            h.bool(v);
        }
    }
}

impl SimHash for Peds {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.u32(self.p.len() as u32);
        for p in &self.p {
            p.sim_hash(h);
        }
        for v in [self.next_serial, self.rng, self.remap_next as u64] {
            h.bytes(&v.to_le_bytes());
        }
        for v in [
            self.view_hw,
            self.view_hh,
            self.player_angle,
            self.gait_wheel,
            self.spawn_edge,
            self.spawn_dir,
            self.punch_wheel,
            self.pl_layer,
            self.pl_face,
            self.cross_rr,
            self.cop_shoot,
            self.gang_aim,
        ] {
            h.i32(v);
        }
        for v in [
            self.pl_x,
            self.pl_y,
            self.pl_door_x,
            self.pl_door_y,
            self.pl_hl,
            self.pl_hw,
        ] {
            h.bytes(&v.to_le_bytes());
        }
        for v in [
            self.player_moving,
            self.fidget,
            self.spawned_since_retire,
            self.pl_in_car,
            self.pl_slow,
            self.gang_side,
        ] {
            h.bool(v);
        }
        h.i32(self.last.map_or(-1, |i| i as i32));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pool_keeps_amigta1_density() {
        assert_eq!(Peds::cap_for(SimView::AMIGTA1), 12);
        assert_eq!(Peds::cap_for(SimView::GAME), 45);
    }

    #[test]
    fn the_generator_is_amigta1s() {
        // AmiGTA1's rng_next from seed 777, worked by hand.
        let mut p = Peds::new(SEED, 1);
        let s = 777u64.wrapping_mul(1_103_515_245).wrapping_add(12345);
        assert_eq!(p.rand(), (s >> 16) & 0x7FFF);
    }

    #[test]
    fn frames_follow_the_state() {
        let mut p = Ped {
            alive: true,
            pull: -1,
            speed: 1,
            frame: 3,
            ..Ped::default()
        };
        assert_eq!(p.sprite_frame(), 3);
        p.speed = 3;
        assert_eq!(p.sprite_frame(), 11);
        p.speed = 0;
        assert_eq!(p.sprite_frame(), 98);
        p.down = 5;
        assert_eq!(p.sprite_frame(), frame::LIES);
        p.corpse = true;
        assert_eq!(p.sprite_frame(), frame::BODY);
        p.pull = 3;
        assert_eq!(p.sprite_frame(), 46);
        p.pull = -1;
        p.corpse = false;
        p.down = 0;
        p.cop = true;
        assert_eq!(p.sprite_frame(), 98 + frame::COP_SET);
    }
}
