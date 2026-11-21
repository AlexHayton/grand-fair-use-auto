//! Traffic: the fleet of driven cars around the player.
//!
//! Ported from AmiGTA1 (MIT), `gta_traffic.c`, `gta_traffic.h`. A car is
//! given a route of blocks (`crate::route`), keeps to its lane line by
//! pure pursuit, turns through junctions on circular arcs, books the
//! blocks of a crossing before entering it, follows the car ahead at a
//! gap, stops at red lights, and is pushed about by collisions with its
//! neighbours. Cars are added in a ring outside the view and removed when
//! they fall far behind it.
//!
//! Units are AmiGTA1's: 16.16 world pixels at 32 to a block, angles of 256
//! steps, a 50 Hz tick. Arithmetic is `i64`, as in AmiGTA1's 64-bit host
//! build, which is the oracle (`tools/oracle/traffictrace.c`); so is the
//! generator's seed (see [`crate::route::lcg_step`]).
//!
//! AmiGTA1 keeps many experimental switches; this port fixes them at
//! AmiGTA1's defaults and leaves out the branches they turn off. Police
//! chases, roadblocks, missions and hunters arrive with M6 and M7: their
//! state is here, but nothing sets it yet.

mod contact;
mod drive;
pub mod geom;
pub mod roads;

pub use contact::{Ram, Taken};

use gfa_fixed::trig::{cos, sin};
use gfa_formats::style::CarInfo;

use crate::hash::{SimHash, StateHasher};
use crate::map::{SimMap, nav};
use crate::route::{self, Node, Router};
use geom::{box_hit, box_hit_block, box_mtv};
use roads::{Boxes, LANE_KERB, LANE_TARGET, heading_step};

const FP: u32 = 16;
/// Shift from 16.16 world px to blocks.
const BS: u32 = FP + 5;

/// Cars the fleet holds at most.
pub const MAX_CARS: usize = 192;
/// Cars the fleet tops up to by default.
pub const FLEET_CAP: usize = 20;

pub(crate) const SPAWN_TICKS: i32 = 25;
const HALFRATE_MARGIN: i32 = 3;
const CRUISE_MAX: i64 = 3;
const CRUISE_LOOK: i32 = 3;

pub(crate) const LOOKAHEAD: i64 = 128 << 16;
pub(crate) const FOLLOW_GAP: i64 = 56 << 16;
pub(crate) const MIN_GAP: i64 = 24 << 16;
pub(crate) const GAP_SHIFT: u32 = 5;
pub(crate) const ACCEL: i64 = 3000;
pub(crate) const BRAKE: i64 = 22000;
pub(crate) const ACCEL_UNIT: i64 = 120;
pub(crate) const BRAKE_UNIT: i64 = 880;
pub(crate) const TURN_QUARTER: i32 = 64;
pub(crate) const ARC_QLEN: i64 = 102_944;
pub(crate) const ARC_K: i64 = 5215;
pub(crate) const ARC_KSHIFT: u32 = 7;
pub(crate) const TURN_RADIUS: i32 = 29;
pub(crate) const TURN_RADIUS_MIN: i32 = 3;
pub(crate) const AFTER_TURN: i32 = 40;
pub(crate) const PURSUIT_K: i64 = 24;
pub(crate) const PURSUIT_MIN: i32 = 24;
pub(crate) const PURSUIT_MAX: i32 = 64;
pub(crate) const STEER_W: i64 = 2608;
pub(crate) const STEER_W_MIN: i64 = 1 << 14;
pub(crate) const STEER_W_MAX: i64 = 3 << 16;
pub(crate) const STEER_SLOW: i64 = 16;
const KNOCK_HARD: i64 = 2 << 16;
const KNOCK_KEEP: i64 = 128;
const HIT_SCALE: i64 = 42;
const HIT_MASS_CAP: i64 = 20;
const HIT_LIGHT: i64 = 15;
const HIT_SPIN: i64 = 41721;
const KNOCK_SPIN_MAX: i64 = 3 << 16;
const HIT_LATCH: u8 = 6;
const SEP_SLOP: i64 = 1 << 14;
const SEP_DEEP: i64 = 2 << 14;
const KNOCK_TICKS: i32 = 75;
pub(crate) const KNOCK_DAMP: i64 = 246;
pub(crate) const KNOCK_SIDE_DAMP: i64 = 200;
pub(crate) const KNOCK_SPIN: i64 = 235;
pub(crate) const KNOCK_REST: i64 = 13107;
/// One of the original's speed units, 16.16 px a tick.
pub const SPEED_UNIT: i64 = 19660;
/// The style's `max_speed` to 16.16 px a tick.
const STYLE_SPEED_UNIT: i64 = 3495;
pub(crate) const SPEED_CRUISE: i64 = 6;
pub(crate) const SPEED_TURN_TIGHT: i64 = 2;
pub(crate) const SPEED_TURN_WIDE: i64 = 5;
pub(crate) const SPEED_ARC_MIN: i64 = 2;
pub(crate) const TURN_AGAIN: i64 = 48;
pub(crate) const SWEEP_PATIENCE: i32 = 25;
pub(crate) const ARC_CREEP: i32 = 1;
pub(crate) const CLEAR_EASE: i32 = 6;
pub(crate) const CLEAR_HARD: i32 = 4;
pub(crate) const CLEAR_STOP: i32 = 3;
const ROUTE_TARGET_LO: i32 = 10;
const ROUTE_TARGET_HI: i32 = 20;
pub(crate) const FOLLOW_SIDE: i64 = 12 << 16;
pub(crate) const PATIENCE: i32 = 150;
const CLAIM_MAX: usize = MAX_CARS * 12;
pub(crate) const PATH_MAX: i32 = 4;
const OCC_MAX: usize = MAX_CARS * 9;
pub(crate) const LONG_CAR: i32 = 44;
const ARC_CELLS: usize = 12;
const CLAIM_TTL: i16 = 500;
pub(crate) const BOX_LOOK: i64 = 16 << 16;
pub(crate) const REVERSE: i32 = 50;
pub(crate) const SPEED_REVERSE: i64 = 4;
pub(crate) const GIVEUP: i32 = 300;
pub(crate) const ABANDON: i32 = 1500;
pub(crate) const STUCK: i32 = 150;

// Pulling over for the player's horn or siren: the original's
// `car+0x11f == 1`, which AmiGTA1 records (`gta_main.c` `special_key`,
// `gta_traffic.h`'s lane offsets) and does not port. The special key held in
// a car looks up to five blocks ahead for a car, and that car moves to the
// kerb: lane offset 7 or 0x37 of the original's 0x40 against the middle's
// 0x1f, 12 of our px either side of 16. How far to the side the look
// reaches, how slowly the car moves over, that it stops, and for how long
// are ours: (verify) under DOSBox-X (M6b).
/// How far ahead of his nose the look reaches, px: five blocks.
pub(crate) const PULL_LOOK: i64 = 5 * 32;
/// How far either side of his line, px: half a block. (verify)
pub(crate) const PULL_SIDE: i64 = 16;
/// The kerb line, px off the middle of the block.
pub(crate) const PULL_KERB: i32 = 12;
/// Within this of the kerb line (px), it stops.
pub(crate) const PULL_BAND: i64 = 2;
/// Ticks it stays over after the look last found it: three seconds.
/// (verify)
pub(crate) const PULL_TICKS: i32 = 150;
/// Its speed while it moves over, in [`SPEED_UNIT`]s. (verify)
pub(crate) const SPEED_PULL: i64 = 2;
pub(crate) const HEADON: i32 = 100;
pub(crate) const ESCAPE: i32 = 150;
const JUMP_MAX: i32 = 10;
const WRECK_KEEP_BLOCKS: i32 = 20;
/// AmiGTA1 keeps 16, more than its 12 peds; ours holds a full game-view
/// pool, so every walker counts.
const MAX_WALKERS: usize = 64;
pub(crate) const LANE_SWAP_WAIT: i32 = 100;
pub(crate) const LANE_SWAP_COOL: i32 = 150;
pub(crate) const LANE_SWAP_MAX: i32 = 200;
pub(crate) const LANE_SWAP_LOOK: i32 = 3;
pub(crate) const UTURN_SCAN: i32 = 3;
const RECOVER_RINGS: i32 = 6;
const COP_MODEL: u8 = 4;
const HELICOPTER: u8 = 88;

/// Why a car is not moving.
pub mod hold {
    pub const NONE: u8 = 0;
    pub const QUEUE: u8 = 1;
    pub const LIGHT: u8 = 2;
    pub const BOX: u8 = 3;
    pub const MERGE: u8 = 4;
    pub const DEADEND: u8 = 5;
    pub const ROAD: u8 = 6;
    pub const GAP: u8 = 7;
    pub const STUCK: u8 = 8;
    /// Pulled over for the player's horn or siren.
    pub const PULL: u8 = 9;
}

/// Which models the fleet draws, with weights, per GTA style (AmiGTA1's
/// reading of each city's traffic).
const MIX: [&[(u8, u8)]; 3] = [
    &[
        (0, 1),
        (1, 5),
        (2, 1),
        (3, 6),
        (6, 1),
        (9, 4),
        (14, 2),
        (17, 2),
        (18, 11),
        (19, 5),
        (21, 1),
        (22, 19),
        (25, 2),
        (26, 2),
        (27, 5),
        (28, 1),
        (29, 1),
        (31, 3),
        (34, 4),
        (35, 1),
        (41, 2),
        (43, 2),
        (44, 2),
        (45, 2),
        (46, 2),
        (58, 4),
        (70, 4),
        (72, 2),
        (76, 2),
        (80, 1),
    ],
    &[
        (1, 8),
        (3, 4),
        (6, 1),
        (7, 2),
        (9, 3),
        (19, 7),
        (22, 16),
        (31, 5),
        (35, 1),
        (41, 2),
        (43, 2),
        (44, 1),
        (45, 1),
        (46, 1),
        (50, 2),
        (53, 1),
        (54, 10),
        (55, 2),
        (58, 3),
        (61, 2),
        (62, 2),
        (63, 1),
        (64, 5),
        (65, 4),
        (66, 1),
        (70, 5),
        (74, 1),
        (76, 1),
        (79, 5),
        (86, 1),
    ],
    &[
        (1, 8),
        (3, 8),
        (6, 2),
        (7, 2),
        (9, 3),
        (19, 6),
        (22, 13),
        (26, 2),
        (29, 1),
        (35, 1),
        (41, 3),
        (43, 3),
        (44, 2),
        (45, 1),
        (46, 2),
        (54, 10),
        (62, 2),
        (65, 3),
        (70, 6),
        (72, 2),
        (73, 1),
        (74, 1),
        (75, 4),
        (76, 2),
        (77, 1),
        (78, 2),
        (79, 2),
        (80, 2),
        (81, 1),
        (82, 1),
        (83, 1),
        (87, 2),
    ],
];

/// Half the length of a model, world px.
#[inline]
pub(crate) fn half_len(ci: &CarInfo) -> i32 {
    (i32::from(ci.length) >> 1) / 2
}

/// Half the width of a model, world px.
#[inline]
pub fn half_wid(ci: &CarInfo) -> i32 {
    (i32::from(ci.width) >> 1) / 2
}

/// Length of a model, world px.
#[inline]
pub fn world_len(ci: &CarInfo) -> i32 {
    i32::from(ci.length) >> 1
}

/// Can the fleet use this model? It needs a door, a known class (car, bus
/// or bike), and is never the helicopter.
fn parkable(ci: &CarInfo) -> bool {
    !ci.doors.is_empty() && ci.model != HELICOPTER && matches!(ci.vtype, 0 | 3 | 4)
}

/// One car of the fleet. Field names follow AmiGTA1's `gta_car`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Car {
    /// Centre, 16.16 world px.
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    /// The road direction: 0 N, 64 E, 128 S, 192 W.
    pub angle: i32,
    /// Index into the style's car table.
    pub model: usize,
    /// Colour remap, -1 for none.
    pub remap: i32,
    /// 16.16 px a tick.
    pub speed: i64,
    pub top: i64,
    pub accel: i64,
    pub brake: i64,
    /// Which way the body points, 0..256.
    pub face: i32,
    pub turn: i32,
    pub turn_accum: i32,
    pub turn_from: i32,
    pub since_turn: i32,
    pub turn_aim_tgt: i32,
    pub turn_aim_bx: i32,
    pub turn_aim_by: i32,
    pub turn_radius: i32,
    pub arc_cx: i64,
    pub arc_cy: i64,
    pub arc_s: i64,
    pub arc_len: i64,
    pub arc_line: i64,
    pub bend_bx: i32,
    pub bend_by: i32,
    pub bend_dir: i32,
    pub book_ax: i32,
    pub book_ay: i32,
    /// The convoy (cars crossing a junction on one booking) this car is in.
    pub convoy: u64,
    pub book_lx: i32,
    pub book_ly: i32,
    pub damage: i32,
    pub dmg_bits: u32,
    pub fuse: i32,
    pub ram_cool: u8,
    pub hit_latch: u8,
    pub mission: bool,
    /// No driver: parked, left by the player, or a wreck.
    pub abandoned: bool,
    /// One of the map's parked cars, kept however far away the player goes
    /// (ours; AmiGTA1 has none).
    pub resident: bool,
    /// The doors are locked (a mission's).
    pub locked: bool,
    /// Takes no damage (a mission's).
    pub invuln: bool,
    pub driver: bool,
    pub wrecked: bool,
    /// 0 none, 1 patrol, 2 chasing, 3 out on foot, 4 roadblock.
    pub cop: u8,
    pub hunt: u8,
    pub swap_wait: i32,
    pub swap: i32,
    pub swap_sdx: i32,
    pub swap_sdy: i32,
    pub swap_bx: i32,
    pub swap_by: i32,
    pub swap_ticks: i32,
    pub swap_cool: i32,
    /// What is ahead: 0 nothing, 1 the player or a walker, 2 a stopped car,
    /// 3 a moving one.
    pub lead_kind: i32,
    pub lead_idx: i32,
    pub honk: u8,
    pub honk_pat: u8,
    /// Ticks left of being knocked about by a collision.
    pub knock: i32,
    pub recover: i32,
    pub kvx: i64,
    pub kvy: i64,
    pub komega: i64,
    /// `face` in 16.16, for the steering's fractions.
    pub face16: i64,
    pub allow_turn: bool,
    pub lane_bx: i32,
    pub lane_by: i32,
    pub appr_bx: i32,
    pub appr_by: i32,
    pub odo: i64,
    pub turn_free_at: i64,
    pub boxlock_ticks: i32,
    pub turn_step: i32,
    /// Holding a booking through a junction.
    pub crossing: bool,
    pub turn_lock: i32,
    pub lane_fix: i32,
    pub lane_target: i32,
    /// Lane line: across x (0) or y (1), its 16.16 offset, its direction.
    pub lane_axis: i32,
    pub lane_off: i64,
    pub lane_dir: i32,
    pub lane_set: bool,
    pub path: Vec<Node>,
    pub path_i: usize,
    pub dest_x: i32,
    pub dest_y: i32,
    pub want_route: bool,
    pub route_cool: i32,
    pub hint_bx: i32,
    pub hint_by: i32,
    pub hint_val: u8,
    pub last_bx: i32,
    pub last_by: i32,
    pub wait: i32,
    pub reverse: i32,
    pub at_light: bool,
    pub hold: u8,
    pub done: bool,
    pub offroad: bool,
    pub turn_want: bool,
    pub turn_want_bx: i32,
    pub turn_want_by: i32,
    pub turn_want_from: i32,
    pub serial: u64,
    pub st_x: i64,
    pub st_y: i64,
    pub st_face: i32,
    pub still: bool,
    pub occ_ser: u64,
    pub occ_mask: u16,
    pub cruise: i32,
    /// Ticks left pulled over to the kerb for the player's horn or siren;
    /// 0 driving ([`PULL_TICKS`]).
    pub pull: i32,
}

impl Car {
    pub(crate) fn path_n(&self) -> usize {
        self.path.len()
    }
    pub(crate) fn clear_path(&mut self) {
        self.path.clear();
        self.path_i = 0;
    }
    /// The block of the centre.
    pub fn block(&self) -> (i32, i32) {
        ((self.x >> BS) as i32, (self.y >> BS) as i32)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct Claim {
    x: u8,
    y: u8,
    z: i8,
    car: u64,
    ttl: i16,
    seen: bool,
    on: bool,
    on_ser: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct Occ {
    x: u8,
    y: u8,
    z: i8,
    car: u64,
    next: u16,
}

/// The fleet.
#[derive(Debug, Clone)]
pub struct Traffic {
    /// AmiGTA1's fixed array of slots; the fleet is `cars[..n]`. A new car
    /// keeps the fields AmiGTA1 does not set from the slot's last car.
    pub cars: Vec<Car>,
    pub n: usize,
    boxes: Boxes,
    router: Router,
    /// AmiGTA1's traffic generator, 64-bit as in its host build.
    pub seed: u64,
    spawn_wait: i32,
    pub tick: u64,
    /// Blocks from the camera to the view's edge, across.
    pub view_blocks: i32,
    cam_bx: i32,
    cam_by: i32,
    pub fleet_cap: usize,
    route_turn: usize,
    next_serial: u64,
    claims: Vec<Claim>,
    claim_top: usize,
    occ: Vec<Occ>,
    occ_head: [u16; 16],
    // The player, as an obstacle.
    pl_active: bool,
    pl_x: i64,
    pl_y: i64,
    pl_speed: i64,
    pl_layer: i32,
    pl_hl: i32,
    // Pedestrians, as obstacles.
    walk: Vec<(i64, i64, i32)>,
    pub wanted: i32,
    n_cop_patrol: i32,
    n_cop_chasing: i32,
    honk_rng: u64,
    /// A driver yelled at someone this tick (for the peds).
    pub yell: Option<(i64, i64, i32)>,
    /// Ticks of the player's car touching a police car (AmiGTA1's
    /// `stat_ram_cop`; the police read it from M6).
    pub rammed_cops: u32,
}

impl Traffic {
    /// An empty fleet on `map`, with its junction boxes built.
    pub fn new(map: &SimMap, seed: u64) -> Traffic {
        Traffic {
            cars: vec![Car::default(); MAX_CARS],
            n: 0,
            boxes: Boxes::build(map),
            router: Router::default(),
            seed: if seed == 0 { 1 } else { seed },
            spawn_wait: 0,
            tick: 0,
            view_blocks: 5,
            cam_bx: 0,
            cam_by: 0,
            fleet_cap: FLEET_CAP,
            route_turn: 0,
            next_serial: 0,
            claims: vec![Claim::default(); CLAIM_MAX],
            claim_top: 0,
            occ: Vec::with_capacity(OCC_MAX),
            occ_head: [0; 16],
            pl_active: false,
            pl_x: 0,
            pl_y: 0,
            pl_speed: 0,
            pl_layer: 0,
            pl_hl: 0,
            walk: Vec::new(),
            wanted: 0,
            n_cop_patrol: 0,
            n_cop_chasing: 0,
            honk_rng: 0,
            yell: None,
            rammed_cops: 0,
        }
    }

    /// The view's half-width in blocks; clamped to 3..=24.
    pub fn set_view_blocks(&mut self, blocks: i32) {
        self.view_blocks = blocks.clamp(3, 24);
    }

    /// The player, for following gaps: centre, speed, layer and half
    /// length.
    #[allow(clippy::too_many_arguments)]
    pub fn set_player(&mut self, active: bool, x: i64, y: i64, speed: i64, layer: i32, hl: i32) {
        self.pl_active = active;
        self.pl_x = x;
        self.pl_y = y;
        self.pl_speed = speed;
        self.pl_layer = layer;
        self.pl_hl = hl;
    }

    /// Pedestrians standing in the road, as obstacles; at most 64.
    pub fn set_walkers(&mut self, walkers: impl Iterator<Item = (i64, i64, i32)>) {
        self.walk.clear();
        self.walk.extend(walkers.take(MAX_WALKERS));
    }

    fn next_rand(&mut self) -> u64 {
        (route::lcg_step(&mut self.seed) >> 16) & 0x7FFF
    }

    #[inline]
    /// The player's special key held in his car, centred at `(x, y)`
    /// (16.16) on `layer`, facing `face`, `hl` px from centre to nose: the
    /// nearest car whose centre is up to five blocks ahead of his nose
    /// and within half a block of his line pulls over to the kerb, if it
    /// is driving along a plain stretch of road. Returns its serial on the
    /// tick it is first told.
    pub fn pull_over(
        &mut self,
        map: &SimMap,
        x: i64,
        y: i64,
        layer: i32,
        face: i32,
        hl: i32,
    ) -> Option<u64> {
        let (s, co) = (i64::from(sin(face)), i64::from(cos(face)));
        let hl = i64::from(hl);
        let mut best: Option<(i64, usize)> = None;
        for (i, c) in self.cars[..self.n].iter().enumerate() {
            if c.done || c.layer != layer {
                continue;
            }
            let (dx, dy) = ((c.x - x) >> FP, (c.y - y) >> FP);
            let along = (dx * s - dy * co) >> 14;
            let side = (dx * co + dy * s) >> 14;
            if along <= hl || along > hl + PULL_LOOK || side.abs() > PULL_SIDE {
                continue;
            }
            if best.is_none_or(|(d, _)| along < d) {
                best = Some((along, i));
            }
        }
        let (_, i) = best?;
        let first = self.cars[i].pull == 0;
        if first && !self.can_pull_over(map, i) {
            return None;
        }
        let c = &mut self.cars[i];
        c.pull = PULL_TICKS;
        c.cruise = 0;
        first.then_some(c.serial)
    }

    /// Is car `i` ordinary traffic driving along its lane, with the block
    /// it is on and the next two plain road, so it can move over and stop
    /// clear of a junction?
    fn can_pull_over(&self, map: &SimMap, i: usize) -> bool {
        let c = &self.cars[i];
        if c.cop != 0
            || c.mission
            || c.abandoned
            || c.wrecked
            || c.knock > 0
            || c.reverse > 0
            || c.turn != 0
            || c.swap != 0
            || c.crossing
            || !c.lane_set
            || c.lane_dir & 63 != 0
        {
            return false;
        }
        let (bx, by) = c.block();
        let (dx, dy) = heading_step(c.lane_dir);
        (0..=2).all(|k| {
            let (x, y) = (bx + k * dx, by + k * dy);
            roads::drivable(map, x, y, c.layer) && !self.is_junction(x, y, c.layer)
        })
    }

    pub fn is_junction(&self, bx: i32, by: i32, z: i32) -> bool {
        self.boxes.is_junction(bx, by, z)
    }

    /// Is the light green at `(bx, by)` for traffic along x or y? The
    /// light is the crossing's ([`roads::Boxes::crossing`]).
    pub fn light_green(&self, bx: i32, by: i32, along_x: bool) -> bool {
        let (kx, ky) = self.boxes.crossing(bx, by);
        roads::light_green(self.tick as u32, kx, ky, along_x)
    }

    /// The tick the traffic lights' clock reads.
    pub fn light_tick(&self) -> u32 {
        self.tick as u32
    }

    /// Every traffic light in the city, for drawing: the map's, found once.
    pub fn lamps(&self, map: &SimMap) -> Vec<roads::Lamp> {
        roads::lamps(map, &self.boxes)
    }

    /// Fill the fleet around `(bx, by)` out to `radius` blocks, up to
    /// `want` cars: the level start (AmiGTA1 `gta_traffic_park`).
    pub fn park(
        &mut self,
        map: &SimMap,
        cars: &[CarInfo],
        bx: i32,
        by: i32,
        radius: i32,
        want: usize,
    ) -> usize {
        self.park_band(map, cars, bx, by, 1, radius, want)
    }

    /// Place up to `want` cars on road blocks in the square rings
    /// `ring_lo..=ring_hi` around `(bx, by)`.
    #[allow(clippy::too_many_arguments)]
    fn park_band(
        &mut self,
        map: &SimMap,
        cars: &[CarInfo],
        bx: i32,
        by: i32,
        ring_lo: i32,
        ring_hi: i32,
        want: usize,
    ) -> usize {
        if cars.is_empty() {
            return 0;
        }
        let driven = self.cars[..self.n]
            .iter()
            .filter(|c| !c.done && !c.abandoned)
            .count() as i32;
        let mut want = want as i32;
        {
            const SHARE: [i32; 5] = [7, 6, 4, 3, 2];
            let lv = self.wanted.clamp(0, 4);
            let mut cap = self.fleet_cap as i32 * SHARE[lv as usize] / 7;
            if lv > 0 {
                cap += self.cars[..self.n]
                    .iter()
                    .filter(|c| !c.done && c.cop != 0)
                    .count() as i32;
            }
            cap = cap.max(2);
            want = want.min(cap - driven);
        }
        if want <= 0 {
            return 0;
        }
        let style = (map.cmp().style_number as usize).clamp(1, 3);
        let mut models: Vec<usize> = Vec::with_capacity(100);
        for &(id, w) in MIX[style - 1] {
            let Some(rec) = cars.iter().position(|c| c.model == id) else {
                continue;
            };
            if !parkable(&cars[rec]) {
                continue;
            }
            for _ in 0..w {
                if models.len() < 100 {
                    models.push(rec);
                }
            }
        }
        if models.is_empty() {
            return 0;
        }

        let mut placed = 0;
        let mut ring = ring_lo;
        while ring <= ring_hi && placed < want {
            for side in 0..4 {
                if placed >= want {
                    break;
                }
                for step in -ring..=ring {
                    if placed >= want {
                        break;
                    }
                    let (cx, cy) = match side {
                        0 => (bx + step, by - ring),
                        1 => (bx + step, by + ring),
                        2 => (bx - ring, by + step),
                        _ => (bx + ring, by + step),
                    };
                    if self.n >= MAX_CARS {
                        return placed as usize;
                    }
                    if self.place_at(map, cars, &models, cx, cy) {
                        placed += 1;
                    }
                }
            }
            ring += 1;
        }
        placed as usize
    }

    /// Would a car of model `ci` put at `(x, y)` heading `ang` touch the
    /// body of a car already there? Its box at the block's middle, wide
    /// enough for the lane beside a kerb, with 2 px spare at the ends.
    ///
    /// Ours (decision log, 2026-11-21; traffic_spawn.pl): AmiGTA1 asks only
    /// whether its lane has room, so a car could be put down on the nose
    /// of one waiting at a corner or one changing into that lane.
    fn body_in_way(
        &self,
        cars: &[CarInfo],
        x: i64,
        y: i64,
        z: i32,
        ang: i32,
        ci: &CarInfo,
    ) -> bool {
        let (hl, hw) = (half_len(ci) + 2, half_wid(ci) + LANE_KERB);
        self.cars[..self.n].iter().any(|o| {
            let oi = &cars[o.model];
            !o.done
                && o.layer == z
                && geom::box_hit(
                    x,
                    y,
                    ang,
                    hl,
                    hw,
                    o.x,
                    o.y,
                    o.face,
                    half_len(oi),
                    half_wid(oi),
                )
        })
    }

    /// Try to put a car on block `(cx, cy)`.
    fn place_at(
        &mut self,
        map: &SimMap,
        cars: &[CarInfo],
        models: &[usize],
        cx: i32,
        cy: i32,
    ) -> bool {
        let Some(z) = roads::road_layer(map, cx, cy) else {
            return false;
        };
        if self.next_rand() & 3 != 0 {
            return false;
        }
        if self.is_junction(cx, cy, z) {
            return false;
        }
        let (ang, axis) = match self.road_heading(map, cx, cy, z) {
            Some(a) => (a, if a & 127 == 64 { 64 } else { 0 }),
            None => {
                let axis = roads::road_axis(map, cx, cy, z);
                (axis + ((self.next_rand() & 1) << 7) as i32, axis)
            }
        };
        let px = i64::from(cx * 32 + 16) << FP;
        let py = i64::from(cy * 32 + 16) << FP;
        let mut mi = None;
        let mut reach: Option<bool> = None;
        for _ in 0..4 {
            let cand = models[(self.next_rand() % models.len() as u64) as usize];
            let len = world_len(&cars[cand]);
            if !roads::fits(map, cx, cy, z, axis, len)
                || !roads::has_exit(map, cx, cy, z, ang, len)
                || !self.room_for(cars, None, px, py, z, ang, len, MIN_GAP)
                || self.claim_any(cx, cy, z)
                || self.body_in_way(cars, px, py, z, ang, &cars[cand])
            {
                continue;
            }
            let r = *reach.get_or_insert_with(|| roads::can_get_away(map, cx, cy, z));
            if !r {
                break;
            }
            mi = Some(cand);
            break;
        }
        let Some(mi) = mi else {
            return false;
        };
        let info = &cars[mi];
        // The lane: the middle, or beside the kerb.
        let (lo, hi) = if axis == 64 {
            (
                roads::ground_at(map, cx, cy - 1, z) == crate::map::ground::ROAD,
                roads::ground_at(map, cx, cy + 1, z) == crate::map::ground::ROAD,
            )
        } else {
            (
                roads::ground_at(map, cx - 1, cy, z) == crate::map::ground::ROAD,
                roads::ground_at(map, cx + 1, cy, z) == crate::map::ground::ROAD,
            )
        };
        let mut tgt = LANE_TARGET;
        if !lo && hi {
            tgt += LANE_KERB;
        } else if lo && !hi {
            tgt -= LANE_KERB;
        }
        let (ox, oy) = if axis == 64 {
            (0, i64::from(tgt - LANE_TARGET) << FP)
        } else {
            (i64::from(tgt - LANE_TARGET) << FP, 0)
        };

        self.next_serial += 1;
        let serial = self.next_serial;
        let top = (i64::from(info.max_speed) * STYLE_SPEED_UNIT).min(SPEED_CRUISE * SPEED_UNIT);
        let lane_axis = i32::from(ang & 127 == 64);
        // AmiGTA1 writes these fields into the next slot and leaves the
        // rest as the slot's last car had them.
        let n = self.n;
        let car = &mut self.cars[n];
        car.honk = 0;
        car.honk_pat = 0;
        car.hunt = 0;
        car.mission = false;
        car.cop = 0;
        car.driver = false;
        car.serial = serial;
        car.convoy = serial;
        car.book_lx = -1;
        car.book_ly = -1;
        car.x = px + ox;
        car.y = py + oy;
        car.layer = z;
        car.angle = ang & 255;
        car.model = mi;
        car.done = false;
        car.top = top;
        car.speed = top;
        car.accel = if info.accel != 0 {
            i64::from(info.accel) * ACCEL_UNIT
        } else {
            ACCEL
        };
        car.brake = if info.braking != 0 {
            i64::from(info.braking) * BRAKE_UNIT
        } else {
            BRAKE
        };
        car.face = ang & 255;
        car.turn = 0;
        car.pull = 0;
        car.turn_accum = 0;
        car.turn_radius = TURN_RADIUS;
        car.since_turn = AFTER_TURN;
        car.turn_from = ang & 255;
        car.turn_lock = 0;
        car.crossing = false;
        car.abandoned = false;
        car.wrecked = false;
        car.damage = 0;
        car.dmg_bits = 0;
        car.fuse = 0;
        car.knock = 0;
        car.recover = 0;
        car.allow_turn = true;
        car.lane_bx = cx;
        car.lane_by = cy;
        car.appr_bx = cx;
        car.appr_by = cy;
        car.odo = 0;
        car.turn_free_at = 0;
        car.arc_s = 0;
        car.arc_len = 0;
        car.lane_fix = 0;
        car.lane_target = tgt;
        car.lane_axis = lane_axis;
        car.lane_off = i64::from((if lane_axis != 0 { cy } else { cx }) * 32 + tgt) << FP;
        car.lane_dir = ang & 255;
        car.lane_set = true;
        car.wait = 0;
        car.hold = hold::NONE;
        car.at_light = false;
        car.reverse = 0;
        car.clear_path();
        car.want_route = true;
        car.route_cool = 0;
        car.hint_bx = -1;
        car.hint_by = -1;
        car.hint_val = 0;
        car.last_bx = cx;
        car.last_by = cy;
        car.dest_x = cx;
        car.dest_y = cy;
        car.remap = -1;
        self.n += 1;
        // One patrol car on the streets when there is none.
        if self.n_cop_patrol + self.n_cop_chasing < 1
            && self.next_rand() & 3 == 0
            && let Some(cm) = cars.iter().position(|c| c.model == COP_MODEL)
        {
            self.cars[n].model = cm;
            self.cars[n].cop = 1;
            self.n_cop_patrol += 1;
        }
        let ok: Vec<i32> = (0..info.remap8.len() as i32)
            .filter(|&r| info.remap8[r as usize] != 0)
            .collect();
        if !ok.is_empty() {
            self.cars[n].remap = ok[(self.next_rand() % ok.len() as u64) as usize];
        }
        true
    }

    /// A random arrow of the block, `None` if it has none.
    fn road_heading(&mut self, map: &SimMap, bx: i32, by: i32, z: i32) -> Option<i32> {
        let d = roads::raw_dirs(map, bx, by, z)?;
        let mut angles = [0; 4];
        let mut n = 0;
        for (bit, a) in [(0x01, 0), (0x08, 64), (0x02, 128), (0x04, 192)] {
            if d & bit != 0 {
                angles[n] = a;
                n += 1;
            }
        }
        if n == 0 {
            return None;
        }
        Some(angles[(self.next_rand() % n as u64) as usize])
    }

    /// Room for a vehicle `length` long at `(x, y)` facing `angle`, with
    /// `slack` more between it and any car in the same lane?
    #[allow(clippy::too_many_arguments)]
    fn room_for(
        &self,
        cars: &[CarInfo],
        skip: Option<usize>,
        x: i64,
        y: i64,
        layer: i32,
        angle: i32,
        length: i32,
        slack: i64,
    ) -> bool {
        let ns = angle & 127 == 0;
        for (i, o) in self.cars[..self.n].iter().enumerate() {
            if Some(i) == skip || o.done || o.layer != layer {
                continue;
            }
            let along = if ns { o.y - y } else { o.x - x }.abs();
            let side = if ns { o.x - x } else { o.y - y }.abs();
            if side > (16 << FP) && along > (16 << FP) {
                continue;
            }
            let need = (i64::from(length / 2 + world_len(&cars[o.model]) / 2) << FP) + slack;
            if side <= (16 << FP) && along < need {
                return false;
            }
        }
        true
    }

    fn claim_any(&self, bx: i32, by: i32, z: i32) -> bool {
        self.claims[..self.claim_top].iter().any(|c| {
            c.ttl > 0 && i32::from(c.x) == bx && i32::from(c.y) == by && i32::from(c.z) == z
        })
    }

    /// Is any car (other than `skip`), the player or a walker on block
    /// `(bx, by, z)`? `stopped_only` counts only those standing; cars of
    /// convoy `exempt` do not count.
    #[allow(clippy::too_many_arguments)]
    fn body_on_sq(
        &self,
        skip: usize,
        bx: i32,
        by: i32,
        z: i32,
        stopped_only: bool,
        exempt: u64,
    ) -> bool {
        for (i, o) in self.cars[..self.n].iter().enumerate() {
            if i == skip || o.done || o.layer != z {
                continue;
            }
            if stopped_only && o.speed > 0 {
                continue;
            }
            if exempt != 0 && (o.serial == exempt || o.convoy == exempt) {
                continue;
            }
            if o.block() == (bx, by) {
                return true;
            }
        }
        if self.pl_active
            && self.pl_layer == z
            && !(stopped_only && self.pl_speed > 0)
            && (self.pl_x >> BS) as i32 == bx
            && (self.pl_y >> BS) as i32 == by
        {
            return true;
        }
        self.walk
            .iter()
            .any(|&(x, y, l)| l == z && (x >> BS) as i32 == bx && (y >> BS) as i32 == by)
    }

    /// Is another car riding a booking made by car `id`?
    fn convoy_riding(&self, id: u64) -> bool {
        self.cars[..self.n]
            .iter()
            .any(|c| !c.done && c.crossing && c.serial != id && c.convoy == id)
    }

    /// Would car `idx` facing `ang` at `(px, py)` overlap no other car?
    fn car_free_at(&self, cars: &[CarInfo], idx: usize, px: i64, py: i64, ang: i32) -> bool {
        let c = &self.cars[idx];
        let info = &cars[c.model];
        let (ahl, ahw) = (half_len(info), half_wid(info));
        for (i, o) in self.cars[..self.n].iter().enumerate() {
            if i == idx || o.done || o.layer != c.layer {
                continue;
            }
            if ((px - o.x) >> FP).abs() > 80 || ((py - o.y) >> FP).abs() > 80 {
                continue;
            }
            let oi = &cars[o.model];
            if box_hit(
                px,
                py,
                ang,
                ahl,
                ahw,
                o.x,
                o.y,
                o.face,
                half_len(oi),
                half_wid(oi),
            ) {
                return false;
            }
        }
        true
    }

    /// Is the next 4 px of car `idx`'s arc clear of other cars?
    fn arc_step_free(&self, cars: &[CarInfo], idx: usize) -> bool {
        let c = &self.cars[idx];
        let info = &cars[c.model];
        let (ahl, ahw) = (half_len(info), half_wid(info));
        if c.turn == 0 || c.turn_radius <= 0 {
            return true;
        }
        let s = (c.arc_s + (4 << FP)).min(c.arc_len);
        let phi = (((s / i64::from(c.turn_radius)) * ARC_K) >> ARC_KSHIFT)
            .min(i64::from(TURN_QUARTER) << FP);
        let ang = (((i64::from(c.turn_from) << FP) + i64::from(c.turn) * phi) >> FP) as i32 & 255;
        let rd = (ang - c.turn * TURN_QUARTER) & 255;
        let px = c.arc_cx + ((i64::from(c.turn_radius) * i64::from(sin(rd))) << 2);
        let py = c.arc_cy - ((i64::from(c.turn_radius) * i64::from(cos(rd))) << 2);
        for (i, o) in self.cars[..self.n].iter().enumerate() {
            if i == idx || o.done || o.layer != c.layer {
                continue;
            }
            if ((px - o.x) >> FP).abs() > 80 || ((py - o.y) >> FP).abs() > 80 {
                continue;
            }
            let oi = &cars[o.model];
            if box_hit(
                px,
                py,
                ang,
                ahl,
                ahw,
                o.x,
                o.y,
                o.face,
                half_len(oi),
                half_wid(oi),
            ) {
                return false;
            }
        }
        true
    }

    /// Is a stopped car (other than `idx`) centred on block `(bx, by)`?
    fn block_full(&self, idx: usize, bx: i32, by: i32, layer: i32) -> bool {
        self.cars[..self.n].iter().enumerate().any(|(i, o)| {
            i != idx && !o.done && o.layer == layer && o.speed <= 0 && o.block() == (bx, by)
        })
    }

    /// Does car `c`'s convoy hold a booking on `(x, y)`?
    fn claim_mine(&self, c: &Car, x: i32, y: i32) -> bool {
        self.claims[..self.claim_top].iter().any(|k| {
            k.ttl > 0
                && k.car == c.convoy
                && i32::from(k.x) == x
                && i32::from(k.y) == y
                && i32::from(k.z) == c.layer
        })
    }

    /// Is a car on block `(bx, by)` crossing it on the other axis?
    fn box_busy(&self, idx: usize, bx: i32, by: i32, layer: i32, ew: bool) -> bool {
        self.cars[..self.n].iter().enumerate().any(|(i, o)| {
            i != idx
                && !o.done
                && o.layer == layer
                && o.block() == (bx, by)
                && ((o.angle & 127) == 64) != ew
        })
    }

    /// Book block `(rx, ry, z)` for car `serial`; false if another holds it.
    fn junction_claim(&mut self, rx: i32, ry: i32, z: i32, serial: u64) -> bool {
        let mut free_slot = None;
        for i in 0..self.claim_top {
            let k = &mut self.claims[i];
            if k.ttl <= 0 {
                if free_slot.is_none() {
                    free_slot = Some(i);
                }
                continue;
            }
            if k.x == rx as u8 && k.y == ry as u8 && k.z == z as i8 {
                if k.car != serial {
                    return false;
                }
                k.ttl = CLAIM_TTL;
                return true;
            }
        }
        let slot = match free_slot {
            Some(s) => s,
            None if self.claim_top < CLAIM_MAX => self.claim_top,
            None => return true,
        };
        if slot >= self.claim_top {
            self.claim_top = slot + 1;
        }
        self.claims[slot] = Claim {
            x: rx as u8,
            y: ry as u8,
            z: z as i8,
            car: serial,
            ttl: CLAIM_TTL,
            seen: false,
            on: false,
            on_ser: 0,
        };
        true
    }

    /// Who holds block `(bx, by, z)` in the occupancy table (0: nobody).
    fn occ_owner(&self, bx: i32, by: i32, z: i32) -> u64 {
        let mut i = self.occ_head[(bx & 15) as usize];
        while i != 0 {
            let k = &self.occ[usize::from(i) - 1];
            if k.x == bx as u8 && k.y == by as u8 && k.z == z as i8 {
                return k.car;
            }
            i = k.next;
        }
        0
    }

    /// The occupancy table: the blocks each car's body covers.
    fn occ_rebuild(&mut self, cars: &[CarInfo]) {
        self.occ.clear();
        self.occ_head = [0; 16];
        for i in 0..self.n {
            if self.cars[i].done {
                continue;
            }
            let (bx, by) = self.cars[i].block();
            let replay = {
                let o = &self.cars[i];
                o.still && o.occ_ser == o.serial
            };
            if !replay {
                self.cars[i].occ_mask = 0;
                self.cars[i].occ_ser = 0;
            }
            let o = self.cars[i].clone();
            let oi = &cars[o.model];
            let (hl, hw) = (half_len(oi), half_wid(oi));
            let (s, co) = (i64::from(sin(o.face)).abs(), i64::from(cos(o.face)).abs());
            let rx = ((s * i64::from(hl) + co * i64::from(hw)) >> 14) + 16;
            let ry = ((co * i64::from(hl) + s * i64::from(hw)) >> 14) + 16;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (cx, cy) = (bx + dx, by + dy);
                    let bit = 1u16 << ((dy + 1) * 3 + (dx + 1));
                    if self.occ.len() >= OCC_MAX {
                        return;
                    }
                    if replay {
                        if o.occ_mask & bit == 0 {
                            continue;
                        }
                    } else {
                        let ddx = (o.x >> FP) - i64::from(cx * 32 + 16);
                        let ddy = (o.y >> FP) - i64::from(cy * 32 + 16);
                        if ddx > rx || ddx < -rx || ddy > ry || ddy < -ry {
                            continue;
                        }
                        if !box_hit_block(
                            i64::from(cx * 32 + 16) << FP,
                            i64::from(cy * 32 + 16) << FP,
                            o.x,
                            o.y,
                            o.face,
                            hl,
                            hw,
                        ) {
                            continue;
                        }
                        self.cars[i].occ_mask |= bit;
                    }
                    if self.occ_owner(cx, cy, o.layer) != 0 {
                        continue;
                    }
                    let h = (cx & 15) as usize;
                    self.occ.push(Occ {
                        x: cx as u8,
                        y: cy as u8,
                        z: o.layer as i8,
                        car: o.serial,
                        next: self.occ_head[h],
                    });
                    self.occ_head[h] = self.occ.len() as u16;
                }
            }
            self.cars[i].occ_ser = o.serial;
        }
    }

    /// Check (and with `commit`, book) the blocks a turn's arc sweeps,
    /// from quarter `s0` of the turn to its end.
    #[allow(clippy::too_many_arguments)]
    fn arc_reserve(
        &mut self,
        idx: usize,
        cx: i64,
        cy: i64,
        r_px: i32,
        from_ang: i32,
        dir: i32,
        long_car: bool,
        commit: bool,
        s0: i32,
    ) -> bool {
        let (layer, serial) = (self.cars[idx].layer, self.cars[idx].serial);
        let mut cells: Vec<(i32, i32)> = Vec::with_capacity(ARC_CELLS);
        for s in s0..=4 {
            let phi = s * (TURN_QUARTER / 4);
            let ang = (from_ang + dir * phi) & 255;
            let rd = (ang - dir * TURN_QUARTER) & 255;
            let px = cx + ((i64::from(r_px) * i64::from(sin(rd))) << 2);
            let py = cy - ((i64::from(r_px) * i64::from(cos(rd))) << 2);
            let (bx, by) = ((px >> BS) as i32, (py >> BS) as i32);
            for extra in 0..=i32::from(long_car && s == 2) {
                let (mut ex, mut ey) = (bx, by);
                if extra != 0 {
                    let (odx, ody) = heading_step((ang - dir * TURN_QUARTER) & 255);
                    ex -= odx;
                    ey -= ody;
                }
                if cells.contains(&(ex, ey)) || cells.len() >= ARC_CELLS {
                    continue;
                }
                cells.push((ex, ey));
            }
        }
        for &(x, y) in &cells {
            let who = self.occ_owner(x, y, layer);
            if who != 0 && who != serial {
                return false;
            }
            if self.claims[..self.claim_top].iter().any(|k| {
                k.ttl > 0
                    && k.x == x as u8
                    && k.y == y as u8
                    && k.z == layer as i8
                    && k.car != serial
            }) {
                return false;
            }
        }
        if commit {
            for &(x, y) in &cells {
                self.junction_claim(x, y, layer, serial);
            }
        }
        true
    }

    /// The gap to whatever is nearest ahead of car `idx` in its lane (a
    /// car, the player or a walker), 16.16 px, with that thing's speed and
    /// index (`-1` for the player or a walker); `-1` if nothing.
    /// How many blocks on along its heading car `c`'s route turns: 0 at the
    /// block it is in, `None` if it goes straight on for the next four (or
    /// it is turning already).
    fn turn_ahead(c: &Car) -> Option<i32> {
        if c.turn != 0 {
            return None;
        }
        let (ax, ay) = roads::heading_step(c.angle);
        let (mut px, mut py) = c.block();
        let mut k = 0;
        for n in c.path.iter().skip(c.path_i).take(4) {
            let (nx, ny) = (i32::from(n.x), i32::from(n.y));
            if (nx, ny) == (px, py) {
                continue;
            }
            if (nx, ny) != (px + ax, py + ay) {
                return Some(k);
            }
            (px, py) = (nx, ny);
            k += 1;
        }
        None
    }

    fn gap_ahead(&self, cars: &[CarInfo], idx: usize) -> (i64, i64, i32) {
        let c = &self.cars[idx];
        let (fx, fy) = (i64::from(sin(c.face)), -i64::from(cos(c.face)));
        // Ours: where its route turns, a car straight on past the corner is
        // not in its way, and waiting for one closed rings of queues round
        // the one-way loops (decision log, 2026-11-21; traffic_follow.pl).
        let corner = Self::turn_ahead(c);
        let (cbx, cby) = c.block();
        let (hax, hay) = roads::heading_step(c.angle);
        let my_half = i64::from(world_len(&cars[c.model]) / 2);
        let (mut best, mut lead, mut lead_i) = (-1i64, 0i64, -1i32);
        let along_side = |ox: i64, oy: i64| {
            let along = ((ox >> 8) * fx + (oy >> 8) * fy) >> 6;
            let side = (((ox >> 8) * -fy + (oy >> 8) * fx) >> 6).abs();
            (along, side)
        };
        let in_box = |ox: i64, oy: i64| {
            (-LOOKAHEAD..=LOOKAHEAD).contains(&ox) && (-LOOKAHEAD..=LOOKAHEAD).contains(&oy)
        };
        // Ours: halfway round a turn the nose points across the corner, and
        // a car ahead in the street it is turning into falls outside the
        // strip along it; so while it turns, it looks along that street too
        // (decision log, 2026-11-21; traffic_follow.pl).
        let exit = (c.turn != 0).then(|| {
            let a = (c.turn_from + if c.turn > 0 { 64 } else { -64 }) & 255;
            (i64::from(sin(a)), -i64::from(cos(a)))
        });
        for (i, o) in self.cars[..self.n].iter().enumerate() {
            if i == idx || o.done || o.layer != c.layer {
                continue;
            }
            let (ox, oy) = (o.x - c.x, o.y - c.y);
            if !in_box(ox, oy) {
                continue;
            }
            let (mut along, mut side) = along_side(ox, oy);
            if let Some((gx, gy)) = exit
                && (along <= 0 || side > FOLLOW_SIDE)
            {
                along = ((ox >> 8) * gx + (oy >> 8) * gy) >> 6;
                side = (((ox >> 8) * -gy + (oy >> 8) * gx) >> 6).abs();
            }
            if along <= 0 || side > FOLLOW_SIDE {
                continue;
            }
            // Two stopped cars on one junction block, crossing: the later
            // one does not wait for the earlier.
            if o.speed == 0
                && c.speed == 0
                && i > idx
                && ((o.angle & 127) == 64) != ((c.angle & 127) == 64)
                && o.block() == c.block()
            {
                continue;
            }
            if along > LOOKAHEAD {
                continue;
            }
            if let Some(kt) = corner {
                let (obx, oby) = o.block();
                if (obx - cbx) * hax + (oby - cby) * hay > kt {
                    continue;
                }
            }
            let gap = (along - ((my_half + i64::from(world_len(&cars[o.model]) / 2)) << FP)).max(0);
            if best < 0 || gap < best {
                best = gap;
                lead = o.speed;
                lead_i = i as i32;
            }
        }
        if c.cop != 2 && self.pl_active && self.pl_layer == c.layer {
            let (ox, oy) = (self.pl_x - c.x, self.pl_y - c.y);
            if in_box(ox, oy) {
                let (along, side) = along_side(ox, oy);
                if along > 0 && side <= FOLLOW_SIDE && along <= LOOKAHEAD {
                    let pgap = (along - ((my_half + i64::from(self.pl_hl)) << FP)).max(0);
                    if best < 0 || pgap < best {
                        best = pgap;
                        lead = self.pl_speed;
                        lead_i = -1;
                    }
                }
            }
        }
        for &(wx, wy, wl) in &self.walk {
            if wl != c.layer {
                continue;
            }
            let (ox, oy) = (wx - c.x, wy - c.y);
            if !in_box(ox, oy) {
                continue;
            }
            let (along, side) = along_side(ox, oy);
            if along <= 0 || along > LOOKAHEAD || side > FOLLOW_SIDE / 2 {
                continue;
            }
            let wgap = (along - ((my_half + 4) << FP)).max(0);
            if best < 0 || wgap < best {
                best = wgap;
                lead = 0;
                lead_i = -1;
            }
        }
        (best, lead, lead_i)
    }

    /// Is block `(bx, by)` inside the despawn square, where the player may
    /// see it?
    fn in_view(&self, bx: i32, by: i32) -> bool {
        let r = self.view_blocks + 9;
        bx - self.cam_bx < r && self.cam_bx - bx < r && by - self.cam_by < r && self.cam_by - by < r
    }

    /// Head to head with the car ahead, which is waiting on us too: should
    /// car `idx` be the one to back off?
    fn headon_yield(&self, idx: usize) -> bool {
        let c = &self.cars[idx];
        if c.lead_idx < 0 || c.lead_idx as usize >= self.n {
            return false;
        }
        let o = &self.cars[c.lead_idx as usize];
        if o.done || o.layer != c.layer || o.lead_idx != idx as i32 {
            return false;
        }
        let d = (c.face - o.face) & 255;
        if !(96..=160).contains(&d) {
            return false;
        }
        c.serial > o.serial || o.wait > 2 * HEADON
    }

    fn escape_place(c: &mut Car, nx: i32, ny: i32, ang: i32) {
        c.x = i64::from(nx * 32 + LANE_TARGET) << FP;
        c.y = i64::from(ny * 32 + LANE_TARGET) << FP;
        c.angle = ang;
        c.face = ang;
        c.turn = 0;
        c.turn_lock = 0;
        c.clear_path();
        c.want_route = true;
        c.wait = 0;
        c.last_bx = nx;
        c.last_by = ny;
    }

    /// Stuck out of sight: jump ahead along the road, up to 10 blocks.
    fn escape_jump(
        &mut self,
        map: &SimMap,
        cars: &[CarInfo],
        idx: usize,
        bx: i32,
        by: i32,
    ) -> bool {
        let (ang, layer) = {
            let c = &self.cars[idx];
            (((c.face + 32) & 255) & 0xC0, c.layer)
        };
        let (dx, dy) = heading_step(ang);
        let mut best = 0;
        for k in 1..=JUMP_MAX {
            let (nx, ny) = (bx + dx * k, by + dy * k);
            if !roads::drivable(map, nx, ny, layer) {
                break;
            }
            if k < 2 || !roads::dir_allowed(map, nx, ny, layer, ang) || self.in_view(nx, ny) {
                continue;
            }
            let px = i64::from(nx * 32 + LANE_TARGET) << FP;
            let py = i64::from(ny * 32 + LANE_TARGET) << FP;
            if self.car_free_at(cars, idx, px, py, ang) {
                best = k;
            }
        }
        if best == 0 {
            return false;
        }
        let c = &mut self.cars[idx];
        Self::escape_place(c, bx + dx * best, by + dy * best, ang);
        c.speed = c.top;
        true
    }

    /// Stuck out of sight: move to any free road with a long straight run,
    /// up to 8 blocks away.
    fn escape_hop(&mut self, map: &SimMap, cars: &[CarInfo], idx: usize, bx: i32, by: i32) -> bool {
        let layer = self.cars[idx].layer;
        for r in 1..=8 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx > -r && dx < r && dy > -r && dy < r {
                        continue;
                    }
                    let (nx, ny) = (bx + dx, by + dy);
                    if !roads::drivable(map, nx, ny, layer) || self.in_view(nx, ny) {
                        continue;
                    }
                    for a in [0, 64, 128, 192] {
                        if !roads::dir_allowed(map, nx, ny, layer, a) {
                            continue;
                        }
                        let (sx, sy) = heading_step(a);
                        if !(1..=6).all(|k| roads::drivable(map, nx + sx * k, ny + sy * k, layer)) {
                            continue;
                        }
                        let px = i64::from(nx * 32 + LANE_TARGET) << FP;
                        let py = i64::from(ny * 32 + LANE_TARGET) << FP;
                        if !self.car_free_at(cars, idx, px, py, a) {
                            continue;
                        }
                        let c = &mut self.cars[idx];
                        Self::escape_place(c, nx, ny, a);
                        c.speed = 0;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Stuck behind a car out of sight: swap places with it.
    fn escape_swap(&mut self, cars: &[CarInfo], idx: usize) -> bool {
        let li = self.cars[idx].lead_idx;
        if li < 0 || li as usize >= self.n {
            return false;
        }
        let li = li as usize;
        {
            let (c, o) = (&self.cars[idx], &self.cars[li]);
            let (obx, oby) = o.block();
            if o.done
                || o.layer != c.layer
                || o.cop != 0
                || o.mission
                || o.abandoned
                || self.in_view(obx, oby)
            {
                return false;
            }
        }
        let (cx, cy) = (self.cars[idx].x, self.cars[idx].y);
        let (ox, oy) = (self.cars[li].x, self.cars[li].y);
        self.cars[idx].x = ox;
        self.cars[idx].y = oy;
        self.cars[li].x = cx;
        self.cars[li].y = cy;
        let (cd, od) = (self.cars[idx].done, self.cars[li].done);
        self.cars[li].done = true;
        let mut ok = {
            let c = &self.cars[idx];
            self.car_free_at(cars, idx, c.x, c.y, c.face)
        };
        self.cars[li].done = od;
        self.cars[idx].done = true;
        ok = ok && {
            let o = &self.cars[li];
            self.car_free_at(cars, li, o.x, o.y, o.face)
        };
        self.cars[idx].done = cd;
        if !ok {
            self.cars[idx].x = cx;
            self.cars[idx].y = cy;
            self.cars[li].x = ox;
            self.cars[li].y = oy;
            return false;
        }
        for k in [idx, li] {
            let c = &mut self.cars[k];
            c.wait = 0;
            c.clear_path();
            c.want_route = true;
            let (bx, by) = c.block();
            c.last_bx = bx;
            c.last_by = by;
        }
        true
    }

    /// Off the road and out of sight: put the car on the nearest road.
    fn recover_offroad(&mut self, map: &SimMap, idx: usize) -> bool {
        let (bx, by) = self.cars[idx].block();
        let layer = self.cars[idx].layer;
        if self.in_view(bx, by) {
            return false;
        }
        for r in 1..=RECOVER_RINGS {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx > -r && dx < r && dy > -r && dy < r {
                        continue;
                    }
                    let (nx, ny) = (bx + dx, by + dy);
                    if !roads::drivable(map, nx, ny, layer) {
                        continue;
                    }
                    let Some(dir) = self.road_heading(map, nx, ny, layer) else {
                        continue;
                    };
                    let c = &mut self.cars[idx];
                    c.x = i64::from(nx * 32 + LANE_TARGET) << FP;
                    c.y = i64::from(ny * 32 + LANE_TARGET) << FP;
                    c.angle = dir;
                    c.face = dir;
                    c.turn = 0;
                    c.turn_lock = 0;
                    c.speed = 0;
                    c.clear_path();
                    c.want_route = true;
                    c.wait = 0;
                    c.offroad = false;
                    return true;
                }
            }
        }
        false
    }

    /// Is the centre off every road (this layer or the ones beside it)?
    fn offroad_check(&mut self, map: &SimMap, cars: &[CarInfo], idx: usize) {
        let c = &self.cars[idx];
        let info = &cars[c.model];
        if world_len(info) / 2 <= 0 || (i32::from(info.width) >> 1) / 2 <= 0 || c.abandoned {
            return;
        }
        let (bx, by) = c.block();
        let on = roads::drivable(map, bx, by, c.layer)
            || (c.layer > 0 && roads::drivable(map, bx, by, c.layer - 1))
            || (c.layer + 1 < 6 && roads::drivable(map, bx, by, c.layer + 1));
        self.cars[idx].offroad = !on;
    }

    /// One car pushes another: separate them along the shallowest axis,
    /// and on a hard enough hit knock `b` away spinning.
    fn fleet_collide(&mut self, cars: &[CarInfo], ia: usize, ib: usize) {
        let (a, b) = (&self.cars[ia], &self.cars[ib]);
        let (ai, bi) = (&cars[a.model], &cars[b.model]);
        let (dx, dy) = ((b.x - a.x) >> FP, (b.y - a.y) >> FP);
        let (ahl, ahw) = (half_len(ai), half_wid(ai));
        let (bhl, bhw) = (half_len(bi), half_wid(bi));
        let vel = |c: &Car| {
            if c.knock > 0 {
                (c.kvx, c.kvy)
            } else {
                (
                    (i64::from(sin(c.face)) >> 6) * (c.speed >> 8),
                    (i64::from(-cos(c.face)) >> 6) * (c.speed >> 8),
                )
            }
        };
        let (avx, avy) = vel(a);
        let (bvx, bvy) = vel(b);
        let (rvx, rvy) = (avx - bvx, avy - bvy);
        let Some((nx, ny, depth)) = box_mtv(
            a.x, a.y, a.face, ahl, ahw, b.x, b.y, b.face, bhl, bhw, rvx, rvy,
        ) else {
            return;
        };
        let vrel = (((rvx >> 8) * nx) + ((rvy >> 8) * ny)) >> 6;
        let am = i64::from(ai.mass >> 16).max(1);
        let bm = i64::from(bi.mass >> 16).max(1);
        let nram = |v: i64, n: i64| ((v >> 7) * n) >> 7;
        {
            let sep = if depth > SEP_SLOP {
                depth - SEP_SLOP
            } else {
                0
            };
            let sep = (sep << 1) / (am + bm);
            let cap = 2i64 << FP;
            let mut mvx = nram(sep * am, nx);
            let mut mvy = nram(sep * am, ny);
            if depth > SEP_DEEP {
                let extra = (depth - SEP_DEEP) << 1;
                mvx += nram(extra, nx);
                mvy += nram(extra, ny);
            }
            let mvx = mvx.clamp(-cap, cap);
            let mvy = mvy.clamp(-cap, cap);
            let ax_ = nram(sep * bm, nx).clamp(-cap, cap);
            let ay_ = nram(sep * bm, ny).clamp(-cap, cap);
            self.cars[ib].x += mvx;
            self.cars[ib].y += mvy;
            self.cars[ia].x -= ax_;
            self.cars[ia].y -= ay_;
        }
        if vrel <= KNOCK_HARD || self.cars[ib].hit_latch != 0 {
            return;
        }
        let m = am.min(HIT_MASS_CAP);
        let pushx = (((avx >> FP) + dx) * m) / HIT_SCALE;
        let pushy = (((avy >> FP) + dy) * m) / HIT_SCALE;
        let b = &mut self.cars[ib];
        if b.knock == 0 {
            b.kvx = bvx;
            b.kvy = bvy;
            if bm < HIT_LIGHT {
                b.kvx = (b.kvx >> 8) * KNOCK_KEEP;
                b.kvy = (b.kvy >> 8) * KNOCK_KEEP;
            }
            b.face16 = i64::from(b.face) << 16;
        }
        b.kvx += (pushx << FP) / bm;
        b.kvy += (pushy << FP) / bm;
        let (rx, ry) = {
            let (bfx, bfy) = (i64::from(sin(b.face)), -i64::from(cos(b.face)));
            let along = ((-dx * bfx + -dy * bfy) >> 14).clamp(-i64::from(bhl), i64::from(bhl));
            let across = ((-dx * -bfy + -dy * bfx) >> 14).clamp(-i64::from(bhw), i64::from(bhw));
            (
                ((along * bfx - across * bfy) >> 14) / 2,
                ((along * bfy + across * bfx) >> 14) / 2,
            )
        };
        let torque = (rx * pushy - ry * pushx) * 4;
        let inertia = i64::from(bi.moment).max(1);
        b.komega += ((torque * HIT_SPIN) / inertia) << 6;
        b.komega = b.komega.clamp(-KNOCK_SPIN_MAX, KNOCK_SPIN_MAX);
        if b.speed < SPEED_UNIT {
            b.speed = 4 * SPEED_UNIT;
        }
        b.knock = KNOCK_TICKS;
        b.hit_latch = HIT_LATCH;
        b.hold = hold::NONE;
        self.cars[ia].speed >>= 1;
    }

    fn fleet_collisions(&mut self, cars: &[CarInfo]) {
        let cr: Vec<i64> = self.cars[..self.n]
            .iter()
            .map(|c| i64::from(half_len(&cars[c.model]) + half_wid(&cars[c.model])))
            .collect();
        for i in 0..self.n {
            if self.cars[i].done {
                continue;
            }
            for j in i + 1..self.n {
                let (a, b) = (&self.cars[i], &self.cars[j]);
                if b.done || b.layer != a.layer {
                    continue;
                }
                // The map parks some rows of cars a few pixels into each
                // other; standing there they stay as the map put them
                // (ours: AmiGTA1 has no map cars).
                if a.resident && b.resident && a.knock == 0 && b.knock == 0 {
                    continue;
                }
                let (dx, dy) = ((b.x - a.x) >> FP, (b.y - a.y) >> FP);
                let lim = cr[i] + cr[j];
                if dx > lim || dx < -lim || dy > lim || dy < -lim {
                    continue;
                }
                self.fleet_collide(cars, i, j);
                self.fleet_collide(cars, j, i);
            }
        }
    }

    /// Find routes: one car a tick, round robin, that wants one.
    fn route_tick(&mut self, map: &SimMap) {
        let n = self.n;
        if n == 0 {
            return;
        }
        for _ in 0..n {
            let idx = self.route_turn % n;
            self.route_turn = (self.route_turn + 1) % n;
            let c = &mut self.cars[idx];
            if c.route_cool > 0 {
                c.route_cool -= 1;
                continue;
            }
            if !c.want_route || c.done {
                continue;
            }
            let (bx, by) = c.block();
            let (layer, back) = (c.layer, (c.angle + 128) & 255);
            let mut buf = [Node::default(); route::MAX];
            let len = self.router.wander(
                map,
                bx,
                by,
                layer,
                back,
                ROUTE_TARGET_LO,
                ROUTE_TARGET_HI,
                &mut self.seed,
                &mut buf,
            );
            let c = &mut self.cars[idx];
            c.path.clear();
            c.path.extend_from_slice(&buf[..len]);
            c.path_i = 0;
            let (tx, ty) = c
                .path
                .last()
                .map_or((bx, by), |p| (i32::from(p.x), i32::from(p.y)));
            if len == 0 {
                c.route_cool = 50;
            }
            c.dest_x = tx;
            c.dest_y = ty;
            c.want_route = false;
            return;
        }
    }

    /// One tick of the fleet, with the camera (the centre of the view) at
    /// `(cam_x, cam_y)`, 16.16.
    pub fn step(&mut self, map: &SimMap, cars: &[CarInfo], cam_x: i64, cam_y: i64) {
        self.tick += 1;
        self.yell = None;
        for c in &mut self.cars[..self.n] {
            c.still = c.x == c.st_x && c.y == c.st_y && c.face == c.st_face;
            c.st_x = c.x;
            c.st_y = c.y;
            c.st_face = c.face;
        }
        self.count_cops();
        self.convoy_orphans();
        self.age_claims(cars);

        let (camx, camy) = ((cam_x >> BS) as i32, (cam_y >> BS) as i32);
        self.cam_bx = camx;
        self.cam_by = camy;
        self.occ_rebuild(cars);

        for i in 0..self.n {
            let (px, py) = (self.cars[i].x, self.cars[i].y);
            {
                let c = &mut self.cars[i];
                c.ram_cool = c.ram_cool.saturating_sub(1);
                c.hit_latch = c.hit_latch.saturating_sub(1);
            }
            let c = &self.cars[i];
            let odd = (self.tick ^ c.serial) & 1 != 0;
            // Far cars drive every other tick, the farthest every fourth.
            if !c.done && c.cop == 0 && !c.mission && (self.tick ^ c.serial) & 3 != 0 {
                let rx = self.view_blocks + HALFRATE_MARGIN;
                let ry = (self.view_blocks * 5) / 8 + HALFRATE_MARGIN;
                let dx = (px >> BS) as i32 - self.cam_bx;
                let dy = (py >> BS) as i32 - self.cam_by;
                let far2 = dx > 2 * rx || dx < -2 * rx || dy > 2 * ry || dy < -2 * ry;
                if far2 || (odd && (dx > rx || dx < -rx || dy > ry || dy < -ry)) {
                    continue;
                }
            }
            // On a clear straight, decide only every other tick and coast
            // between.
            let (bx, by) = ((px >> BS) as i32, (py >> BS) as i32);
            if !c.done
                && c.cop == 0
                && !c.mission
                && c.knock == 0
                && !c.abandoned
                && c.cruise == 0
                && odd
                && c.lane_set
                && c.turn == 0
                && !c.crossing
                && c.swap == 0
                && c.recover == 0
                && c.lane_fix == 0
                && c.reverse == 0
                && map.nav(bx, by, c.layer) & nav::SLOPE == 0
            {
                let c = &mut self.cars[i];
                if c.speed > 0 {
                    c.cruise = 1;
                } else {
                    c.wait += 1;
                    continue;
                }
            }
            let c = &mut self.cars[i];
            if c.cruise > 0 && !c.done && c.knock == 0 && !c.abandoned {
                c.x += (i64::from(sin(c.face)) * (c.speed >> 4)) >> 10;
                c.y += -(i64::from(cos(c.face)) * (c.speed >> 4)) >> 10;
                c.cruise -= 1;
                continue;
            }
            c.cruise = 0;
            self.drive_one(map, cars, i);
            self.offroad_check(map, cars, i);
            if self.cars[i].offroad {
                self.recover_offroad(map, i);
            }
            let c = &mut self.cars[i];
            c.odo += ((c.x - px).abs() + (c.y - py).abs()) >> FP;
        }

        self.boxlock_count();
        self.route_tick(map);
        self.fleet_collisions(cars);

        // Drop cars far from the camera; wrecks stay a while longer.
        let r = self.view_blocks + 9;
        let (cbx, cby) = (camx, camy);
        let mut keep = 0;
        for i in 0..self.n {
            let c = &self.cars[i];
            let (bx, by) = c.block();
            let rr = if c.wrecked {
                WRECK_KEEP_BLOCKS
            } else if c.cop == 2 {
                r + 24
            } else {
                r
            };
            let kept = ((c.mission || c.resident) && !c.done)
                || !((bx - cbx).abs() > rr || (by - cby).abs() > rr || c.done);
            if kept {
                if keep != i {
                    self.cars[keep] = self.cars[i].clone();
                }
                keep += 1;
            }
        }
        self.n = keep;

        if self.spawn_wait > 0 {
            self.spawn_wait -= 1;
        } else if self.n < MAX_CARS {
            self.spawn_wait = SPAWN_TICKS;
            let (lo, hi) = (self.view_blocks + 2, self.view_blocks + 6);
            let cap = self.fleet_cap;
            self.park_band(map, cars, camx, camy, lo, hi, cap);
        }
    }

    /// The police tally AmiGTA1's dispatch keeps; with nobody wanted it
    /// only counts the patrol cars (the spawn adds one when there is none).
    fn count_cops(&mut self) {
        let mut patrol = 0;
        let mut chasing = 0;
        for c in &mut self.cars[..self.n] {
            if c.done || c.cop == 0 {
                continue;
            }
            if c.wrecked || (c.abandoned && c.cop != 4) {
                c.cop = 0;
                continue;
            }
            if c.cop == 4 {
                continue;
            }
            if c.cop >= 2 {
                chasing += 1;
            } else {
                patrol += 1;
            }
        }
        self.n_cop_chasing = chasing;
        self.n_cop_patrol = patrol;
    }

    /// A convoy whose leader is gone passes its bookings to the member
    /// farthest from its exit.
    fn convoy_orphans(&mut self) {
        for i in 0..self.n {
            let (done, convoy, serial) = {
                let m = &self.cars[i];
                (m.done, m.convoy, m.serial)
            };
            if done || convoy == serial {
                continue;
            }
            if self.cars[..self.n]
                .iter()
                .any(|k| !k.done && k.serial == convoy)
            {
                continue;
            }
            let mut best = -1i64;
            let mut tail = None;
            for o in &self.cars[..self.n] {
                if o.done || o.convoy != convoy || o.book_lx < 0 {
                    continue;
                }
                let d = ((o.x >> FP) - i64::from(o.book_lx * 32 + 16)).abs()
                    + ((o.y >> FP) - i64::from(o.book_ly * 32 + 16)).abs();
                if d > best {
                    best = d;
                    tail = Some(o.serial);
                }
            }
            let Some(tail) = tail else {
                continue;
            };
            for k in &mut self.claims[..self.claim_top] {
                if k.ttl > 0 && k.car == convoy {
                    k.car = tail;
                    k.seen = false;
                }
            }
            for c in &mut self.cars[..self.n] {
                if !c.done && c.convoy == convoy {
                    c.convoy = tail;
                }
            }
        }
    }

    /// Bookings end when their car has driven over the block and off it,
    /// or is gone, or (unseen and not crossing) after their time runs out.
    fn age_claims(&mut self, cars: &[CarInfo]) {
        let mut top2 = 0;
        for i in 0..self.claim_top {
            if self.claims[i].ttl <= 0 {
                continue;
            }
            let k = self.claims[i];
            let o = self.cars[..self.n].iter().position(|c| c.serial == k.car);
            match o {
                Some(oi) if !self.cars[oi].done && self.cars[oi].layer == i32::from(k.z) => {
                    let o = &self.cars[oi];
                    let on = if o.still && k.on_ser == o.serial {
                        k.on
                    } else {
                        let on = car_on_block(cars, o, i32::from(k.x), i32::from(k.y));
                        self.claims[i].on_ser = o.serial;
                        self.claims[i].on = on;
                        on
                    };
                    if on {
                        self.claims[i].seen = true;
                    } else if k.seen {
                        self.claims[i].ttl = 0;
                    } else if !o.crossing && !self.convoy_riding(o.serial) {
                        self.claims[i].ttl = (self.claims[i].ttl - 1).max(0);
                    }
                }
                _ => self.claims[i].ttl = 0,
            }
            if self.claims[i].ttl > 0 {
                top2 = i + 1;
            }
        }
        self.claim_top = top2;
    }

    /// Count how long stopped cars sit in a junction with two others.
    fn boxlock_count(&mut self) {
        for i in 0..self.n {
            let c = &self.cars[i];
            let (bx, by) = c.block();
            if c.done || c.speed > 0 || !self.is_junction(bx, by, c.layer) {
                self.cars[i].boxlock_ticks = 0;
                continue;
            }
            let root = self.boxes.root(bx, by, c.layer);
            let mut n = 1;
            for (k, o) in self.cars[..self.n].iter().enumerate() {
                if k == i || o.done || o.speed > 0 || o.layer != c.layer {
                    continue;
                }
                let (ox, oy) = o.block();
                if !self.is_junction(ox, oy, o.layer) {
                    continue;
                }
                if self.boxes.root(ox, oy, o.layer) == root {
                    n += 1;
                }
            }
            let c = &mut self.cars[i];
            if n >= 3 {
                if c.boxlock_ticks < 30000 {
                    c.boxlock_ticks += 1;
                }
            } else {
                c.boxlock_ticks = 0;
            }
        }
    }

    /// The fleet in the traffic oracle's format, one line a car:
    /// `f tick slot serial x y layer face angle speed model`.
    pub fn trace_lines(&self, tick: u64) -> impl Iterator<Item = String> + '_ {
        self.cars[..self.n].iter().enumerate().map(move |(i, c)| {
            format!(
                "f {tick} {i} {} {} {} {} {} {} {} {}",
                c.serial, c.x, c.y, c.layer, c.face, c.angle, c.speed, c.model
            )
        })
    }

    /// Remove every car (tests that want an empty street).
    pub fn clear(&mut self) {
        self.n = 0;
        self.claim_top = 0;
    }

    /// The cars still in the fleet.
    pub fn live(&self) -> impl Iterator<Item = &Car> {
        self.cars[..self.n].iter().filter(|c| !c.done)
    }
}

/// Does car `o`'s body cover any of block `(bx, by)`?
fn car_on_block(cars: &[CarInfo], o: &Car, bx: i32, by: i32) -> bool {
    let oi = &cars[o.model];
    let cx = i64::from(bx * 32 + 16) << FP;
    let cy = i64::from(by * 32 + 16) << FP;
    let (dx, dy) = ((o.x - cx) >> FP, (o.y - cy) >> FP);
    if !(-64..=64).contains(&dx) || !(-64..=64).contains(&dy) {
        return false;
    }
    let (hl, hw) = (half_len(oi), half_wid(oi));
    let (s, co) = (i64::from(sin(o.face)).abs(), i64::from(cos(o.face)).abs());
    let rx = (s * i64::from(hl) + co * i64::from(hw)) >> 14;
    let ry = (co * i64::from(hl) + s * i64::from(hw)) >> 14;
    if dx > rx + 16 || dx < -(rx + 16) || dy > ry + 16 || dy < -(ry + 16) {
        return false;
    }
    box_hit_block(cx, cy, o.x, o.y, o.face, hl, hw)
}

impl SimHash for Traffic {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.bytes(&self.seed.to_le_bytes());
        h.bytes(&self.tick.to_le_bytes());
        h.u32(self.n as u32);
        for c in &self.cars[..self.n] {
            h.bytes(&c.serial.to_le_bytes());
            h.bytes(&c.x.to_le_bytes());
            h.bytes(&c.y.to_le_bytes());
            h.bytes(&c.speed.to_le_bytes());
            h.i32(c.layer);
            h.i32(c.face);
            h.i32(c.angle);
            h.u8(c.hold);
            h.i32(c.wait);
            h.i32(c.damage);
            h.u32(c.dmg_bits);
            h.i32(c.fuse);
            h.i32(c.knock);
            h.i32(c.pull);
            h.u8(u8::from(c.abandoned) | u8::from(c.wrecked) << 1 | u8::from(c.resident) << 2);
        }
        h.u32(self.claim_top as u32);
    }
}
