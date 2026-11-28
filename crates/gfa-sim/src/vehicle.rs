//! The driven vehicle: a rigid body on two tyres, in fixed point.
//!
//! Ported from AmiGTA1 `gta_vehphys.c` (MIT), which reconstructs the
//! original's player-car model. In short:
//!
//! - **Two tyres on the centreline**, the steered front and the driven
//!   rear. Each resists the velocity of its own contact point, along and
//!   across its axis, in proportion to it. The sums give a force and a
//!   torque about the centre of mass, integrated with semi-implicit Euler at
//!   dt = 1. The car turns because the front tyre pulls sideways at a lever
//!   arm; there is no turn rate anywhere.
//! - **The feel is three numbers**: the rear tyre's grip is 1.5 times the
//!   front's (gentle understeer); the handbrake swaps the rear's lateral 1.5
//!   for 0.6 (the handbrake turn); the steering wheel is an absolute world
//!   heading that winds on at the model's turn ratio, stops at a 50 degree
//!   lock and unwinds by itself when released.
//! - **No foot brake**: "brake" is the reverse gear at half force (a
//!   seventh for bikes), which gives the long stopping distance.
//! - **The road straightens the car**: hands off, going forward on a road
//!   block, it is pulled onto the nearest compass point.
//! - **It runs at the original's 23.333 Hz** on its own 7/15 accumulator,
//!   because its yaw damping is a property of the discrete step.
//! - **Lengths are halved** to the simulation's 32 px blocks (the moment of
//!   inertia quarters); the style's 16.16 physics values are used raw.
//!
//! The body against the world: a ten-point outline tested against ground a
//! car may stand on, fences and posts, resolved per axis so cars slide along
//! walls; the heading is undone if the car turned into something; a car
//! wedged for half a second is eased back out; and no tick ends with the
//! body in a wall (it returns to the last clear transform).
//!
//! Arithmetic is `i64`, matching AmiGTA1's 64-bit host build, which is the
//! oracle. Its fixed-point helpers were chosen to stay inside 32 bits on the
//! Amiga, so the two agree wherever AmiGTA1 is right.

use crate::hash::{SimHash, StateHasher};
use crate::map::{SimMap, nav};
use gfa_fixed::trig::{cos, cos16, sin, sin16, step_dir};
use gfa_formats::style::CarInfo;

/// The original's 23.333 Hz over our 50.
const TIME_NUM: i32 = 7;
const TIME_DEN: i32 = 15;
/// One of the car table's speed units: half a world pixel a step, 16.16.
pub const SPEED_UNIT: i64 = 32768;
/// The only model whose engine cuts out at its top speed.
pub const TANK_MODEL: u8 = 0x25;
/// The bus: grip 3.0 on both rear coefficients, and no handbrake slide.
pub const BUS_MODEL: u8 = 9;
/// Too fast to get out: the original's speed 5.
pub const EXIT_MAX_SPEED: i64 = 5 * SPEED_UNIT;
/// Ticks wedged against a wall before the car is eased back out.
pub const WALL_STUCK: i32 = 25;
/// The vehicle class byte of a motorcycle.
pub const CLASS_BIKE: u8 = 3;

/// Steering lock, 50 degrees, in 16.16 steps.
const STEER_LOCK: i64 = 2_331_278;
/// The wheel's return to centre: dead band and rate, 7.96 degrees.
const CENTRE_BAND: i64 = 371_150;
const CENTRE_RATE: i64 = 371_150;
/// Road snap: window 11.25 degrees, "exact" within a quarter step, nudge
/// 1.59 degrees a step, above 3 px a step.
const SNAP_BAND: i64 = 524_288;
const SNAP_EXACT: i64 = 16_384;
const SNAP_NUDGE: i64 = 74_244;
const SNAP_SPEED: i64 = 196_608;
/// Below these, with no power, the car is at rest (fixed point cannot decay
/// the last few units the way the original's floats did).
const REST_SPEED: i64 = 2849;
const REST_OMEGA: i64 = 4000;
/// Rear grip 1.5, handbrake slide 0.6, the bus's 3.0, all Q16.
const REAR_GRIP: i64 = 98_304;
const HAND_SLIDE: i64 = 39_322;
const BUS_GRIP: i64 = 196_608;
/// The skid threshold before dividing by mass: 10.0 in 16.16.
const SKID_NUM: i64 = 655_360;
const ANGLE_MASK: i64 = 0xFF_FFFF;
const QUARTER: i64 = 64 << 16;

/// Q16 coefficient times a 16.16 velocity.
fn kmul(k: i64, v: i64) -> i64 {
    (k * (v >> 8)) >> 8
}
/// Two small Q16 numbers.
fn cmul(a: i64, b: i64) -> i64 {
    ((a >> 4) * b) >> 12
}
/// A lever arm times an acceleration.
fn rmul(r: i64, a: i64) -> i64 {
    ((r >> 8) * (a >> 4)) >> 4
}
/// Mass over moment times a torque.
fn imul(k: i64, t: i64) -> i64 {
    ((k >> 4) * (t >> 8)) >> 4
}
/// A Q14 unit component times a 16.16 length.
fn umul(u: i64, len: i64) -> i64 {
    (u * (len >> 8)) >> 6
}
/// A 16.16 vector onto a Q14 unit vector.
fn vdot(vx: i64, vy: i64, ux: i64, uy: i64) -> i64 {
    ((vx >> 7) * ux + (vy >> 7) * uy) >> 7
}
fn s16(a: i64) -> i64 {
    i64::from(sin16(a as i32))
}
fn c16(a: i64) -> i64 {
    i64::from(cos16(a as i32))
}
/// The short way round, in 16.16 steps.
fn ang_diff(a: i64, b: i64) -> i64 {
    let d = (a - b) & ANGLE_MASK;
    if d > (128 << 16) { d - (256 << 16) } else { d }
}
fn isqrt(mut v: i64) -> i64 {
    if v <= 0 {
        return 0;
    }
    let (mut r, mut b) = (0i64, 1i64 << 30);
    while b > v {
        b >>= 2;
    }
    while b != 0 {
        if v >= r + b {
            v -= r + b;
            r = (r >> 1) + b;
        } else {
            r >>= 1;
        }
        b >>= 2;
    }
    r
}

/// The controls for one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pedals {
    pub throttle: bool,
    pub brake: bool,
    /// -1 left, 0 straight, +1 right.
    pub steer: i32,
    pub handbrake: bool,
}

/// A driven vehicle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Veh {
    /// Centre of mass, 16.16 world px.
    pub x: i64,
    pub y: i64,
    /// Velocity, 16.16 px per physics step.
    pub vx: i64,
    pub vy: i64,
    /// Heading, 16.16 of the 256-step circle.
    pub ang16: i64,
    /// Spin, 16.16 steps per physics step.
    pub omega: i64,
    /// The steered wheel's absolute heading, 16.16.
    pub steer16: i64,
    accum: i32,
    wall_stuck: i32,
    /// The last transform with the whole outline clear.
    safe: Option<[i64; 5]>,
    /// What the last wall resolve took off the velocity.
    pub hit_vx: i64,
    pub hit_vy: i64,
    /// Geometric centre, 16.16: what is drawn and collided.
    pub ox: i64,
    pub oy: i64,
    /// Index into the style's car table.
    pub model: usize,
    /// The table's own model id, which the special cases are keyed on.
    pub model_id: u8,
    /// Colour remap, `None` for the sprite's own colours.
    pub remap: Option<u8>,
    /// World px.
    pub len: i32,
    pub wid: i32,
    pub vmax: i64,
    pub vmin: i64,
    pub mass: i64,
    pub damage: i32,
    /// Dented panels: bit `n` set shows the sprite's delta `n`.
    pub dmg_bits: u32,
    /// Ticks of burning left before a write-off goes up; 0 when not burning.
    pub fuse: i32,
    /// Its bomb went off: the write-off blows up as the bomb's five blasts.
    pub bombed: bool,
    /// Sum of the axles' lateral forces this step, and whether that is a
    /// slide (tyre marks).
    pub skid: i64,
    pub sliding: bool,
    pub skid_level: i64,
    k_adh_x: i64,
    k_adh_y: i64,
    k_hand: i64,
    k_reverse: i64,
    k_thrust: i64,
    r_front: i64,
    r_rear: i64,
    com_y: i64,
    k_inertia: i64,
    turn_rate: i64,
}

impl Veh {
    /// A vehicle of table record `model` with its geometric centre at
    /// `(x, y)` (16.16) facing `angle` (0..256) (AmiGTA1 `gta_veh_init`).
    pub fn new(ci: &CarInfo, model: usize, x: i64, y: i64, angle: i32) -> Veh {
        let m = if ci.mass > 0 {
            i64::from(ci.mass)
        } else {
            10 << 16
        };
        let m8 = (m >> 8).max(1);
        let mom = if ci.moment > 0 {
            i64::from(ci.moment)
        } else {
            1360
        };
        let ang16 = i64::from(angle & 255) << 16;
        // The centre of mass sits `cy` source px along the body from the
        // centre; source px are half a world px.
        let cy = (i64::from(ci.cy) << 16) / 2;
        let coef = |c: i32| (i64::from(c) << 8) / m8;
        let k_thrust = ((i64::from(ci.thrust) / 2) << 8) / m8;
        let k_reverse = if ci.vtype == CLASS_BIKE {
            k_thrust * 15 / 100
        } else {
            k_thrust / 2
        };
        let mut k_inertia = (m / mom) * 163;
        if k_inertia < 16 {
            k_inertia = (m * 163) / mom;
        }
        Veh {
            x: x + umul(s16(ang16), cy),
            y: y - umul(c16(ang16), cy),
            vx: 0,
            vy: 0,
            ang16,
            omega: 0,
            steer16: ang16,
            accum: 0,
            wall_stuck: 0,
            safe: None,
            hit_vx: 0,
            hit_vy: 0,
            ox: x,
            oy: y,
            model,
            model_id: ci.model,
            remap: None,
            len: i32::from(ci.length >> 1).max(8),
            wid: i32::from(ci.width >> 1),
            vmax: i64::from(ci.max_speed) * SPEED_UNIT,
            vmin: i64::from(ci.min_speed) * SPEED_UNIT,
            mass: m,
            damage: 0,
            dmg_bits: 0,
            fuse: 0,
            bombed: false,
            skid: 0,
            sliding: false,
            skid_level: SKID_NUM / (m8 >> 8).max(1),
            k_adh_x: coef(ci.tyre_adhesion_x),
            k_adh_y: coef(ci.tyre_adhesion_y),
            k_hand: coef(ci.handbrake_friction),
            k_reverse,
            k_thrust,
            r_front: ((i64::from(ci.steering_wheel_offset) - i64::from(ci.cy)) << 16) / 2,
            r_rear: ((i64::from(ci.drive_wheel_offset) - i64::from(ci.cy)) << 16) / 2,
            com_y: cy,
            k_inertia: k_inertia.max(1),
            turn_rate: i64::from(ci.turn_ratio) * 65536 * 256 / 360,
        }
    }

    /// The 7/15 accumulator after the last tick, 0..15: a physics step
    /// happened on a tick that left it below 7, `accum / 7` ticks before
    /// the tick's end. For drawing the car on the physics clock.
    pub fn phase(&self) -> i32 {
        self.accum
    }

    /// The heading in whole steps.
    pub fn angle(&self) -> i32 {
        ((self.ang16 >> 16) & 255) as i32
    }

    /// Speed, 16.16 px per physics step.
    pub fn speed(&self) -> i64 {
        let (sx, sy) = (self.vx >> 8, self.vy >> 8);
        isqrt(sx * sx + sy * sy) << 8
    }

    /// The exit's speed test: the larger axis plus half the smaller.
    pub fn exit_speed(&self) -> i64 {
        let (ax, ay) = (self.vx.abs(), self.vy.abs());
        if ax > ay { ax + ay / 2 } else { ay + ax / 2 }
    }

    /// One 50 Hz tick: a physics step on 7 ticks in 15. `on_road` turns on
    /// the road snap (AmiGTA1 `gta_veh_step`).
    pub fn step(&mut self, p: Pedals, on_road: bool) {
        self.accum += TIME_NUM;
        if self.accum < TIME_DEN {
            return;
        }
        self.accum -= TIME_DEN;

        let (sb, cb) = (s16(self.ang16), c16(self.ang16));
        // A damaged engine pulls less: from 25 points down to a quarter at 100.
        let (mut thr, mut rev) = (self.k_thrust, self.k_reverse);
        if self.damage > 25 {
            let d = i64::from(self.damage.min(100));
            thr = thr * (125 - d) / 100;
            rev = rev * (125 - d) / 100;
        }
        let mut thrust = 0;
        if p.throttle && !p.brake {
            thrust = thr;
        }
        if p.brake {
            thrust = -rev;
        }
        let spd = self.speed();
        if self.model_id == TANK_MODEL && thrust > 0 && spd > self.vmax {
            thrust = 0;
        }

        // The wheel winds on, and stops at the lock.
        if p.steer > 0 {
            self.steer16 += self.turn_rate;
        } else if p.steer < 0 {
            self.steer16 -= self.turn_rate;
        }
        self.steer16 &= ANGLE_MASK;
        let d = ang_diff(self.steer16, self.ang16);
        if d > STEER_LOCK {
            self.steer16 = (self.ang16 + STEER_LOCK) & ANGLE_MASK;
        } else if d < -STEER_LOCK {
            self.steer16 = (self.ang16 - STEER_LOCK) & ANGLE_MASK;
        }
        let (sw, cw) = (s16(self.steer16), c16(self.steer16));

        // Each tyre resists its contact point's velocity: the body's plus
        // the sweep of that point across this step's rotation (a finite
        // difference, which stays stable in a spin).
        let (s2, c2) = (s16(self.ang16 + self.omega), c16(self.ang16 + self.omega));
        let (mut ax, mut ay, mut torque, mut skid) = (0i64, 0i64, 0i64, 0i64);
        for rear in [false, true] {
            let r = if rear { self.r_rear } else { self.r_front };
            // The rear resolves on the body, the front on the steered wheel.
            let (axis_s, axis_c) = if rear { (sb, cb) } else { (sw, cw) };
            let (kx, ky) = if rear {
                let bus = self.model_id == BUS_MODEL;
                let grip = if bus { BUS_GRIP } else { REAR_GRIP };
                let slide = if bus { BUS_GRIP } else { HAND_SLIDE };
                let kx = self.k_adh_x + if p.handbrake { self.k_hand } else { 0 };
                (
                    cmul(grip, kx),
                    cmul(if p.handbrake { slide } else { grip }, self.k_adh_y),
                )
            } else {
                (self.k_adh_x, self.k_adh_y)
            };
            let wvx = self.vx + umul(s2 - sb, r);
            let wvy = self.vy - umul(c2 - cb, r);
            let vlong = vdot(wvx, wvy, axis_s, -axis_c);
            let vlat = vdot(wvx, wvy, axis_c, axis_s);
            let mut along = -kmul(kx, vlong);
            let alat = -kmul(ky, vlat);
            if rear {
                along += thrust;
            }
            skid += alat;
            let fwx = ((along >> 7) * axis_s + (alat >> 7) * axis_c) >> 7;
            let fwy = ((alat >> 7) * axis_s - (along >> 7) * axis_c) >> 7;
            ax += fwx;
            ay += fwy;
            let (rwx, rwy) = (umul(sb, r), -umul(cb, r));
            torque += rmul(rwx, fwy) - rmul(rwy, fwx);
        }
        // The game's own slide test, with its handbrake shortcut.
        self.skid = skid.abs();
        self.sliding =
            self.skid > self.skid_level || (p.handbrake && p.throttle && spd > 2 * SPEED_UNIT);

        self.vx += ax;
        self.vy += ay;
        self.omega += imul(self.k_inertia, torque);
        self.x += self.vx;
        self.y += self.vy;
        self.ang16 = (self.ang16 + self.omega) & ANGLE_MASK;
        self.ox = self.x - umul(s16(self.ang16), self.com_y);
        self.oy = self.y + umul(c16(self.ang16), self.com_y);

        if thrust == 0
            && self.vx.abs() < REST_SPEED
            && self.vy.abs() < REST_SPEED
            && self.omega.abs() < REST_OMEGA
        {
            self.vx = 0;
            self.vy = 0;
            self.omega = 0;
        }

        // Let go, and the wheel comes back to centre.
        if p.steer == 0 {
            let d = ang_diff(self.steer16, self.ang16);
            if d > CENTRE_BAND {
                self.steer16 = (self.steer16 - CENTRE_RATE) & ANGLE_MASK;
            } else if d < -CENTRE_BAND {
                self.steer16 = (self.steer16 + CENTRE_RATE) & ANGLE_MASK;
            }
        }

        // And the road pulls it straight.
        if on_road && p.steer == 0 && !p.brake && spd > SNAP_SPEED {
            let quad = self.ang16 & (QUARTER - 1);
            if !(SNAP_EXACT..=QUARTER - SNAP_EXACT).contains(&quad) {
                self.ang16 = ((self.ang16 + (32 << 16)) & !(QUARTER - 1)) & ANGLE_MASK;
                self.omega = 0;
                self.steer16 = self.ang16;
            } else if quad < SNAP_BAND {
                self.steer16 = (self.steer16 - SNAP_NUDGE) & ANGLE_MASK;
            } else if quad > QUARTER - SNAP_BAND {
                self.steer16 = (self.steer16 + SNAP_NUDGE) & ANGLE_MASK;
            }
        }
    }

    /// The front of the car for a centre and heading: what decides when a
    /// car has reached the next layer (AmiGTA1 `gta_veh_nose`).
    pub fn nose(&self, ox: i64, oy: i64, ang16: i64) -> (i64, i64) {
        let a = ((ang16 >> 16) & 255) as i32;
        let hl = i64::from(self.len / 2);
        (
            ox + i64::from(sin(a)) * hl * 4,
            oy - i64::from(cos(a)) * hl * 4,
        )
    }

    /// Resolve the step just taken against the world, given the transform
    /// before it. Returns the impact speed in whole px a step (AmiGTA1
    /// `gta_veh_wall`).
    #[allow(clippy::too_many_arguments)]
    pub fn wall(
        &mut self,
        map: &SimMap,
        layer: i32,
        x0: i64,
        y0: i64,
        ox0: i64,
        oy0: i64,
        ang0: i64,
    ) -> i32 {
        let (mut dx, mut dy) = (self.x - x0, self.y - y0);
        let (odx, ody) = (dx, dy);
        let ang = self.angle();
        let (hl, hw) = (self.len / 2, self.wid / 2);
        let mut hit = false;
        let (vx0, vy0) = (self.vx, self.vy);
        self.hit_vx = 0;
        self.hit_vy = 0;
        let blocked = |x: i64, y: i64, a: i32| body_blocked(map, layer, x, y, a, hl, hw);

        // X first, then Y on what X kept. The blocked component is zeroed.
        if dx != 0 && blocked(ox0 + dx, oy0, ang) {
            dx = 0;
            self.vx = 0;
            hit = true;
        }
        if dy != 0 && blocked(ox0 + dx, oy0 + dy, ang) {
            dy = 0;
            self.vy = 0;
            hit = true;
        }
        // It may have turned into something: put the heading back.
        if blocked(ox0 + dx, oy0 + dy, ang) {
            let a0 = ((ang0 >> 16) & 255) as i32;
            if !blocked(ox0 + dx, oy0 + dy, a0) {
                self.ang16 = ang0;
                self.omega = 0;
                hit = true;
            }
        }

        if !hit {
            // Only a car that moved is not stuck: the physics steps on 7
            // ticks in 15 and the others move nothing.
            if dx != 0 || dy != 0 {
                self.wall_stuck = 0;
            }
            self.settle(map, layer, hl, hw);
            return self.impact(vx0, vy0);
        }

        if dx == 0 && dy == 0 {
            self.wall_stuck += 1;
            if self.wall_stuck > WALL_STUCK {
                // Back out the way it came, a pixel.
                let m = odx.abs() + ody.abs();
                let (bx, by) = if m < 1 {
                    let a = self.angle();
                    (-i64::from(sin(a)) * 4, i64::from(cos(a)) * 4)
                } else {
                    (-(odx * 65536) / m, -(ody * 65536) / m)
                };
                if !blocked(ox0 + bx, oy0 + by, ang) {
                    dx = bx;
                    dy = by;
                }
                self.wall_stuck = 0;
            }
        } else {
            self.wall_stuck = 0;
        }
        self.x = x0 + dx;
        self.y = y0 + dy;
        self.ox = ox0 + dx;
        self.oy = oy0 + dy;
        self.settle(map, layer, hl, hw);
        self.impact(vx0, vy0)
    }

    fn impact(&mut self, vx0: i64, vy0: i64) -> i32 {
        self.hit_vx = vx0 - self.vx;
        self.hit_vy = vy0 - self.vy;
        ((self.hit_vx.abs() + self.hit_vy.abs()) >> 16) as i32
    }

    /// No tick ends inside a wall: remember a clear transform, or push out,
    /// or go back to the last clear one and stop.
    fn settle(&mut self, map: &SimMap, layer: i32, hl: i32, hw: i32) {
        if !body_blocked(map, layer, self.ox, self.oy, self.angle(), hl, hw) {
            self.safe = Some([self.x, self.y, self.ox, self.oy, self.ang16]);
            return;
        }
        self.unstick(map, layer);
        if !body_blocked(map, layer, self.ox, self.oy, self.angle(), hl, hw) {
            return;
        }
        if let Some([x, y, ox, oy, a]) = self.safe {
            self.x = x;
            self.y = y;
            self.ox = ox;
            self.oy = oy;
            self.ang16 = a;
            self.vx = 0;
            self.vy = 0;
            self.omega = 0;
        }
    }

    /// Push a wedged body towards its centre, away from the outline points
    /// on forbidden ground: 2 px when stopped, 1 while moving.
    fn unstick(&mut self, map: &SimMap, layer: i32) -> bool {
        let a = self.angle();
        let (fx, fy) = (i64::from(sin(a)), -i64::from(cos(a)));
        let (rx, ry) = (i64::from(cos(a)), i64::from(sin(a)));
        let (hl, hw) = (i64::from(self.len / 2), i64::from(self.wid / 2));
        let (mut sx, mut sy, mut n) = (0i64, 0i64, 0);
        for [along, across] in OUTLINE {
            let al = hl * along / 1000;
            let si = hw * across / 1000;
            let px = self.ox + (fx * al + rx * si) * 4;
            let py = self.oy + (fy * al + ry * si) * 4;
            if !car_ground_ok(map.nav_ground((px >> 21) as i32, (py >> 21) as i32, layer)) {
                sx -= fx * al + rx * si;
                sy -= fy * al + ry * si;
                n += 1;
            }
        }
        if n == 0 {
            return false;
        }
        let mut m = sx.abs() + sy.abs();
        if m < 1 {
            sx = -fx;
            sy = -fy;
            m = fx.abs() + fy.abs();
            if m < 1 {
                return false;
            }
        }
        let step = if self.vx == 0 && self.vy == 0 { 2 } else { 1 };
        let (mx, my) = ((sx * step * 65536) / m, (sy * step * 65536) / m);
        self.x += mx;
        self.y += my;
        self.ox += mx;
        self.oy += my;
        self.vx = 0;
        self.vy = 0;
        self.omega = 0;
        true
    }
}

impl SimHash for Veh {
    fn sim_hash(&self, h: &mut StateHasher) {
        for v in [
            self.x,
            self.y,
            self.vx,
            self.vy,
            self.ang16,
            self.omega,
            self.steer16,
            self.ox,
            self.oy,
            self.hit_vx,
            self.hit_vy,
            self.skid,
        ] {
            h.bytes(&v.to_le_bytes());
        }
        h.i32(self.accum);
        h.i32(self.wall_stuck);
        h.i32(self.damage);
        h.u32(self.dmg_bits);
        h.i32(self.fuse);
        if self.bombed {
            h.u8(1);
        }
        h.bool(self.sliding);
        h.u32(self.model as u32);
        h.u8(self.remap.unwrap_or(255));
        match self.safe {
            Some(s) => s.iter().for_each(|v| h.bytes(&v.to_le_bytes())),
            None => h.u8(0),
        }
    }
}

/// The ten outline points, as thousandths of the half length and width:
/// the corners, the thirds of each long side and the middle of each end.
const OUTLINE: [[i64; 2]; 10] = [
    [-1000, -1000],
    [-1000, 0],
    [-1000, 1000],
    [-333, -1000],
    [-333, 1000],
    [333, -1000],
    [333, 1000],
    [1000, -1000],
    [1000, 0],
    [1000, 1000],
];

/// The outline taken round the perimeter, for the fence test.
const RING: [usize; 11] = [0, 1, 2, 4, 6, 9, 8, 7, 5, 3, 0];

/// Can a car stand on this nav ground type? Road, pavement, field; not
/// air, water or buildings.
pub fn car_ground_ok(g: u8) -> bool {
    g >= 2 && g != 5
}

/// Is any of the car's outline on ground a car may not be on, across a
/// fence, or against a post? While any part of the car is over a drivable
/// ramp, every part may stand on the layer above or below as well
/// (AmiGTA1 `veh_body_blocked`).
pub fn body_blocked(
    map: &SimMap,
    layer: i32,
    cx: i64,
    cy: i64,
    ang: i32,
    hl: i32,
    hw: i32,
) -> bool {
    let (fx, fy) = (i64::from(sin(ang)), -i64::from(cos(ang)));
    let (rx, ry) = (i64::from(cos(ang)), i64::from(sin(ang)));
    let (hl64, hw64) = (i64::from(hl), i64::from(hw));
    let points = OUTLINE.map(|[along, across]| {
        let al = hl64 * along / 1000;
        let si = hw64 * across / 1000;
        (cx + (fx * al + rx * si) * 4, cy + (fy * al + ry * si) * 4)
    });
    let blocks = points.map(|(px, py)| ((px >> 21) as i32, (py >> 21) as i32));
    let drivable_ramp = |v: u8| v & nav::SLOPE != 0 && car_ground_ok(nav::ground(v));
    let ramp = blocks.iter().any(|&(bx, by)| {
        drivable_ramp(map.nav(bx, by, layer)) || drivable_ramp(map.nav(bx, by, layer - 1))
    });
    for &(bx, by) in &blocks {
        if car_ground_ok(map.nav_ground(bx, by, layer)) {
            continue;
        }
        if ramp
            && (car_ground_ok(map.nav_ground(bx, by, layer - 1))
                || car_ground_ok(map.nav_ground(bx, by, layer + 1)))
        {
            continue;
        }
        return true;
    }
    for w in RING.windows(2) {
        let (a, b) = (points[w[0]], points[w[1]]);
        if map.wall_cross(layer, a.0 as i32, a.1 as i32, b.0 as i32, b.1 as i32) {
            return true;
        }
    }
    map.post_hit(
        layer,
        cx as i32,
        cy as i32,
        (fx as i32, fy as i32),
        (rx as i32, ry as i32),
        hl,
        hw,
    )
}

/// The layer a car is on after a move from block `(fbx, fby)` to `(bx, by)`
/// along `(dx, dy)`, judged at the nose. Only ramps change it: leaving one
/// in its ascent direction goes up, entering one from its high side goes
/// down (AmiGTA1 `gta_veh_layer`).
#[allow(clippy::too_many_arguments)]
pub fn veh_layer(
    map: &SimMap,
    layer: i32,
    fbx: i32,
    fby: i32,
    bx: i32,
    by: i32,
    dx: i64,
    dy: i64,
) -> i32 {
    if (dx == 0 && dy == 0) || (fbx == bx && fby == by) {
        return layer;
    }
    let dir = step_dir(dx as i32, dy as i32);
    if map.slope_up_dir(fbx, fby, layer) == Some(dir)
        && car_ground_ok(map.nav_ground(bx, by, layer + 1))
    {
        return layer + 1;
    }
    if map
        .slope_up_dir(bx, by, layer - 1)
        .is_some_and(|up| (up + 128) & 255 == dir)
        && car_ground_ok(map.nav_ground(bx, by, layer - 1))
    {
        return layer - 1;
    }
    layer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(1 << 20), 1 << 10);
        assert_eq!(isqrt(99), 9);
        assert_eq!(ang_diff(1 << 16, 255 << 16), 2 << 16);
        assert_eq!(ang_diff(255 << 16, 1 << 16), -(2 << 16));
        // 12 degrees of steering a step is 8.53 of our steps.
        assert_eq!(12i64 * 65536 * 256 / 360, 559_240);
    }
}
