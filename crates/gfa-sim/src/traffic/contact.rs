//! The fleet and everything outside it: cars put into the fleet standing
//! (the map's parked cars, the player's car when he leaves it, wrecks), the
//! player taking one out of it, his car pressing against it and ramming
//! it, walkers stopped by it, and write-offs burning down.
//!
//! After AmiGTA1 (MIT): `gta_traffic.c` (`car_place`, `gta_traffic_abandon`,
//! `gta_traffic_leave_wreck`, `gta_traffic_grab_car`,
//! `gta_traffic_rename_car`, `gta_traffic_box_free`, `gta_traffic_sweep_box`,
//! `gta_traffic_ram`), `gta_main.c` (`fleet_car_at`) and `gta_weapon.c`
//! (`wreck_sweep`; the blast is the weapons').

use gfa_fixed::trig::{cos, sin};
use gfa_formats::style::CarInfo;

use super::geom::{box_hit, box_mtv};
use super::{
    ACCEL, ACCEL_UNIT, AFTER_TURN, BRAKE, BRAKE_UNIT, Car, FP, HIT_LATCH, HIT_LIGHT, HIT_MASS_CAP,
    HIT_SCALE, HIT_SPIN, KNOCK_HARD, KNOCK_KEEP, KNOCK_SPIN_MAX, KNOCK_TICKS, LANE_TARGET,
    MAX_CARS, SEP_DEEP, SEP_SLOP, SPEED_CRUISE, SPEED_UNIT, STYLE_SPEED_UNIT, TURN_RADIUS, Traffic,
    half_len, half_wid, hold,
};
use crate::cars::{self, body_hit, panel_delta};

/// The player's box is tested two pixels larger, so cars that only touch
/// still count as a contact (both tests work in whole pixels).
const TOUCH_PX: i32 = 2;
/// Most a struck car is moved apart in one tick, px.
const SEP_MAX_PX: i64 = 2;
/// Most the striker is moved back out of an overlap in one tick, px.
const SEP_STRIKER_PX: i64 = 3;
/// Closing speed above which a ram costs bodywork (1 px a step).
const RAM_HARD: i64 = 65536;
/// Ticks before the same car can be charged for a ram again.
const RAM_COOL: u8 = 25;
/// A striker at more than this many speed units shoves a stopped car on.
const HIT_SHOVE_MIN: i64 = 12;
/// AmiGTA1's police car model id.
const COP_MODEL: u8 = 4;

/// What a car taken out of the fleet was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taken {
    pub model: usize,
    pub x: i64,
    pub y: i64,
    pub face: i32,
    pub layer: i32,
    /// The fleet's colour remap, -1 for none.
    pub remap: i32,
    pub damage: i32,
    pub dmg_bits: u32,
    /// Somebody was at the wheel: a moving car, or a parked one with a
    /// driver.
    pub had_driver: bool,
    pub serial: u64,
    pub mission: bool,
    pub cop: bool,
}

/// The player's share of one tick of ramming, for his car's physics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ram {
    /// Cars hit hard enough to cost bodywork.
    pub hits: i32,
    /// Velocity change, 16.16 px a step.
    pub dvx: i64,
    pub dvy: i64,
    /// Turn, in `ang16` units.
    pub dyaw: i64,
    /// Push back out of the overlap, 16.16 px.
    pub dpx: i64,
    pub dpy: i64,
    /// Damage to his car from the hits.
    pub damage: i32,
}

/// `v` (16.16) times a Q14 unit axis component, back in 16.16.
#[inline]
fn nram(v: i64, n: i64) -> i64 {
    ((v >> 7) * n) >> 7
}

/// A car's own velocity on its rails: speed along its face, 16.16.
fn rail_velocity(c: &Car) -> (i64, i64) {
    (
        (i64::from(sin(c.face)) >> 6) * (c.speed >> 8),
        (-i64::from(cos(c.face)) >> 6) * (c.speed >> 8),
    )
}

/// The lever arm for a blow from `(dx, dy)` px away: the point of the
/// struck car's box nearest the striker's centre, halved.
fn contact_arm(o: &Car, oi: &CarInfo, dx: i64, dy: i64) -> (i64, i64) {
    let (ofx, ofy) = (i64::from(sin(o.face)), -i64::from(cos(o.face)));
    let (hl, hw) = (i64::from(half_len(oi)), i64::from(half_wid(oi)));
    let along = ((-dx * ofx + -dy * ofy) >> 14).clamp(-hl, hl);
    let across = ((-dx * -ofy + -dy * ofx) >> 14).clamp(-hw, hw);
    (
        ((along * ofx - across * ofy) >> 14) / 2,
        ((along * ofy + across * ofx) >> 14) / 2,
    )
}

impl Traffic {
    /// Put a standing car with nobody in it into the fleet, in the next
    /// slot, or in place of the ambient car farthest from it when the fleet
    /// is full. The slot is cleared first. Returns the slot.
    #[allow(clippy::too_many_arguments)]
    fn car_place(
        &mut self,
        cars: &[CarInfo],
        model: usize,
        x: i64,
        y: i64,
        face: i32,
        layer: i32,
        remap: i32,
        damage: i32,
    ) -> Option<usize> {
        let slot = if self.n < MAX_CARS {
            self.n += 1;
            self.n - 1
        } else {
            let cam_x = i64::from(self.cam_bx * 32 + 16) << FP;
            let cam_y = i64::from(self.cam_by * 32 + 16) << FP;
            let mut worst = -1;
            let mut slot = None;
            for (i, c) in self.cars[..self.n].iter().enumerate() {
                if c.abandoned || c.mission {
                    continue;
                }
                let (cbx, cby) = c.block();
                if self.in_view(cbx, cby) {
                    continue;
                }
                let d = (c.x - cam_x).abs().max((c.y - cam_y).abs());
                if d > worst {
                    worst = d;
                    slot = Some(i);
                }
            }
            slot?
        };
        self.next_serial += 1;
        let serial = self.next_serial;
        let info = &cars[model];
        let (bx, by) = ((x >> (FP + 5)) as i32, (y >> (FP + 5)) as i32);
        self.cars[slot] = Car {
            model,
            x,
            y,
            face,
            angle: face,
            layer,
            remap,
            damage,
            abandoned: true,
            serial,
            top: (i64::from(info.max_speed) * STYLE_SPEED_UNIT).min(SPEED_CRUISE * SPEED_UNIT),
            accel: if info.accel != 0 {
                i64::from(info.accel) * ACCEL_UNIT
            } else {
                ACCEL
            },
            brake: if info.braking != 0 {
                i64::from(info.braking) * BRAKE_UNIT
            } else {
                BRAKE
            },
            convoy: serial,
            book_lx: -1,
            book_ly: -1,
            lane_bx: bx,
            lane_by: by,
            appr_bx: bx,
            appr_by: by,
            turn_radius: TURN_RADIUS,
            since_turn: AFTER_TURN,
            turn_from: face & 255,
            allow_turn: true,
            lane_target: LANE_TARGET,
            hold: hold::NONE,
            ..Car::default()
        };
        Some(slot)
    }

    /// Leave a car standing where the player got out of it (AmiGTA1's
    /// `gta_traffic_abandon`). Returns its serial, or `None` when every
    /// slot holds somebody's car.
    #[allow(clippy::too_many_arguments)]
    pub fn abandon(
        &mut self,
        cars: &[CarInfo],
        model: usize,
        x: i64,
        y: i64,
        face: i32,
        layer: i32,
        remap: i32,
        damage: i32,
    ) -> Option<u64> {
        let slot = self.car_place(cars, model, x, y, face, layer, remap, damage)?;
        Some(self.cars[slot].serial)
    }

    /// One of the map's parked cars. It stands in the fleet like any car
    /// left by the player, but is kept however far away the player goes,
    /// until he takes it. Ours: AmiGTA1 places none.
    pub fn place_resident(&mut self, cars: &[CarInfo], car: &cars::Parked) -> Option<u64> {
        let slot = self.car_place(
            cars,
            car.model,
            car.x.into(),
            car.y.into(),
            car.face,
            car.layer,
            i32::from(car.remap) - 1,
            car.damage,
        )?;
        let c = &mut self.cars[slot];
        c.resident = true;
        // A vehicle with no door to get in by is locked.
        c.locked = !cars::enterable(&cars[car.model]);
        Some(c.serial)
    }

    /// A burnt-out wreck where the player's car blew up: every panel
    /// dented, and nobody can get into it.
    #[allow(clippy::too_many_arguments)]
    pub fn leave_wreck(
        &mut self,
        cars: &[CarInfo],
        model: usize,
        x: i64,
        y: i64,
        face: i32,
        layer: i32,
        remap: i32,
    ) -> bool {
        if self
            .abandon(cars, model, x, y, face, layer, remap, cars::WRECKED)
            .is_none()
        {
            return false;
        }
        let n = self.n;
        if let Some(c) = self.cars[..n]
            .iter_mut()
            .find(|c| !c.done && c.abandoned && !c.wrecked && c.x == x && c.y == y)
        {
            c.wrecked = true;
            c.dmg_bits = cars::DAMAGE_MASK;
            c.fuse = 0;
            c.fire_ticks = 400;
            return true;
        }
        false
    }

    /// The car in the fleet with this serial.
    pub fn car_mut(&mut self, serial: u64) -> Option<&mut Car> {
        let n = self.n;
        self.cars[..n]
            .iter_mut()
            .find(|c| serial != 0 && c.serial == serial && !c.done)
    }

    /// Give the car with serial `from` the serial `to`, so a car the player
    /// drove and left is still the car a mission knows.
    pub fn rename_car(&mut self, from: u64, to: u64) -> bool {
        if from == 0 || to == 0 || from == to {
            return false;
        }
        let n = self.n;
        let Some(c) = self.cars[..n].iter_mut().find(|c| c.serial == from) else {
            return false;
        };
        if c.convoy == from {
            c.convoy = to;
        }
        c.serial = to;
        true
    }

    /// Nearest non-wrecked, unlocked car within `range` px of `(x, y)` on
    /// `layer` (without removing it from the fleet).
    pub fn nearest_enterable(&self, x: i64, y: i64, layer: i32, range: i32) -> Option<usize> {
        let mut best = None;
        let mut bd = i64::from(range) << FP;
        for (i, c) in self.cars[..self.n].iter().enumerate() {
            if c.done || c.wrecked || c.locked || c.layer != layer {
                continue;
            }
            let d = (c.x - x).abs().max((c.y - y).abs());
            if d < bd {
                bd = d;
                best = Some(i);
            }
        }
        best
    }

    /// Take fleet slot `slot` out of the street for the player to get into.
    pub fn take_car(&mut self, slot: usize) -> Taken {
        let c = &mut self.cars[slot];
        c.done = true;
        Taken {
            model: c.model,
            x: c.x,
            y: c.y,
            face: c.face,
            layer: c.layer,
            remap: c.remap,
            damage: c.damage,
            dmg_bits: c.dmg_bits,
            had_driver: !c.abandoned || c.driver,
            serial: c.serial,
            mission: c.mission,
            cop: c.cop != 0,
        }
    }

    /// Take the nearest car within `range` px of `(x, y)` on `layer` out of
    /// the fleet for the player to get into: any car, moving or not, that
    /// is not a wreck and not locked. The tick drops it from the fleet.
    pub fn grab(&mut self, x: i64, y: i64, layer: i32, range: i32) -> Option<Taken> {
        let slot = self.nearest_enterable(x, y, layer, range)?;
        Some(self.take_car(slot))
    }

    /// The fleet car whose body is at `(px, py)` on `layer`, grown by
    /// `margin` px (AmiGTA1's `fleet_car_at`).
    pub fn car_at(
        &self,
        cars: &[CarInfo],
        px: i64,
        py: i64,
        layer: i32,
        margin: i32,
    ) -> Option<usize> {
        self.car_at_except(cars, px, py, layer, margin, None)
    }

    /// Same as [`Traffic::car_at`], ignoring fleet slot `skip` if set.
    pub fn car_at_except(
        &self,
        cars: &[CarInfo],
        px: i64,
        py: i64,
        layer: i32,
        margin: i32,
        skip: Option<usize>,
    ) -> Option<usize> {
        self.cars[..self.n].iter().position(|c| {
            !c.done
                && c.layer == layer
                && !skip.is_some_and(|s| std::ptr::eq(c, &self.cars[s]))
                && body_hit(
                    c.x,
                    c.y,
                    c.face,
                    half_len(&cars[c.model]),
                    half_wid(&cars[c.model]),
                    px,
                    py,
                    margin,
                )
        })
    }

    /// Is a box at `(x, y)` facing `face` clear of every fleet car?
    #[allow(clippy::too_many_arguments)]
    pub fn box_free(
        &self,
        cars: &[CarInfo],
        x: i64,
        y: i64,
        face: i32,
        hl: i32,
        hw: i32,
        layer: i32,
    ) -> bool {
        !self.cars[..self.n].iter().any(|o| {
            if o.done || o.layer != layer {
                return false;
            }
            let (dx, dy) = (((x - o.x) >> FP).abs(), ((y - o.y) >> FP).abs());
            if dx > 96 || dy > 96 {
                return false;
            }
            let oi = &cars[o.model];
            box_hit(
                x,
                y,
                face,
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

    /// The player's car moved from `(x0, y0, ang0)` to `pos` this tick. If
    /// that ends inside a fleet car but did not start in one, bring it back
    /// to the last clear point along the move, found by eight halvings, and
    /// return true. Angles are `ang16`: 16.16 of a 256-step turn.
    #[allow(clippy::too_many_arguments)]
    pub fn sweep_box(
        &self,
        cars: &[CarInfo],
        x0: i64,
        y0: i64,
        ang0: i64,
        pos: &mut (i64, i64, i64),
        hl: i32,
        hw: i32,
        layer: i32,
    ) -> bool {
        let face = |a: i64| ((a >> 16) & 255) as i32;
        if self.box_free(cars, pos.0, pos.1, face(pos.2), hl, hw, layer)
            || !self.box_free(cars, x0, y0, face(ang0), hl, hw, layer)
        {
            return false;
        }
        let (dx, dy) = (pos.0 - x0, pos.1 - y0);
        let mut da = (pos.2 - ang0) & 0xFF_FFFF;
        if da > 0x80_0000 {
            da -= 0x100_0000;
        }
        let (mut lo, mut hi) = (0i64, 256i64);
        for _ in 0..8 {
            let mid = (lo + hi) / 2;
            let tx = x0 + dx * mid / 256;
            let ty = y0 + dy * mid / 256;
            let ta = (ang0 + da * mid / 256) & 0xFF_FFFF;
            if self.box_free(cars, tx, ty, face(ta), hl, hw, layer) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        *pos = (
            x0 + dx * lo / 256,
            y0 + dy * lo / 256,
            (ang0 + da * lo / 256) & 0xFF_FFFF,
        );
        true
    }

    /// One tick of the player's car against the fleet (AmiGTA1's
    /// `gta_traffic_ram`). The struck cars take their share here: pushed
    /// apart, knocked loose with a spin, dented and charged. The player's
    /// share comes back for his car's physics. `pmass` is the car table's
    /// 16.16 mass, `phl`/`phw` half his car's length and width.
    #[allow(clippy::too_many_arguments)]
    pub fn ram(
        &mut self,
        cars: &[CarInfo],
        px: i64,
        py: i64,
        pface: i32,
        phl: i32,
        phw: i32,
        pvx: i64,
        pvy: i64,
        pmass: i64,
        layer: i32,
    ) -> Ram {
        let mut out = Ram::default();
        let pm = (pmass >> 16).max(1);
        let pspd = pvx.abs() + pvy.abs();
        let (plx, ply) = (self.pl_x, self.pl_y);
        for i in 0..self.n {
            let o = &self.cars[i];
            if o.done || o.layer != layer {
                continue;
            }
            let (dx, dy) = ((o.x - px) >> FP, (o.y - py) >> FP);
            if !(-96..=96).contains(&dx) || !(-96..=96).contains(&dy) {
                continue;
            }
            let oi = &cars[o.model];
            let (ovx, ovy) = rail_velocity(o);
            let Some((nx, ny, depth)) = box_mtv(
                px,
                py,
                pface,
                phl + TOUCH_PX,
                phw + TOUCH_PX,
                o.x,
                o.y,
                o.face,
                half_len(oi),
                half_wid(oi),
                pvx - ovx,
                pvy - ovy,
            ) else {
                continue;
            };
            // The touch margin is not overlap.
            let depth = (depth - (i64::from(TOUCH_PX) << 14)).max(0);
            if oi.model == COP_MODEL {
                self.rammed_cops += 1;
            }
            let om = i64::from(oi.mass >> 16).max(1);

            // Apart first, closing or not: each by the other's share of
            // the mass, the struck car by at most SEP_MAX_PX a tick and the
            // striker backing out of the rest.
            let sep = if depth > SEP_SLOP {
                depth - SEP_SLOP
            } else {
                0
            };
            let sep = (sep << 1) / (pm + om);
            let mut mvx = nram(sep * pm, nx);
            let mut mvy = nram(sep * pm, ny);
            if depth > SEP_DEEP {
                let extra = (depth - SEP_DEEP) << 1;
                mvx += nram(extra, nx);
                mvy += nram(extra, ny);
            }
            let cap = SEP_MAX_PX << FP;
            let o = &mut self.cars[i];
            o.x += mvx.clamp(-cap, cap);
            o.y += mvy.clamp(-cap, cap);
            let mut sx = nram(sep * om, nx);
            let mut sy = nram(sep * om, ny);
            if depth > SEP_DEEP {
                let extra = (depth - SEP_DEEP) << 2;
                sx += nram(extra, nx);
                sy += nram(extra, ny);
            }
            let cap = SEP_STRIKER_PX << FP;
            out.dpx -= sx.clamp(-cap, cap);
            out.dpy -= sy.clamp(-cap, cap);

            // An impulse only for a pair still closing, and once a contact.
            let vrel = ((((pvx - ovx) >> 8) * nx) + (((pvy - ovy) >> 8) * ny)) >> 6;
            if vrel <= 0 || o.hit_latch != 0 {
                continue;
            }
            let share = (vrel + (vrel >> 2)) / (pm + om);
            let (jp, jt) = (share * om, share * pm);
            out.dvx -= nram(jp, nx);
            out.dvy -= nram(jp, ny);
            {
                let (fx, fy) = (i64::from(sin(pface)), -i64::from(cos(pface)));
                let along = ((dx << 8) * (fx >> 6) + (dy << 8) * (fy >> 6)) >> 8;
                let side = ((dx << 8) * (-fy >> 6) + (dy << 8) * (fx >> 6)) >> 8;
                let kick = (if side > 0 { jp } else { -jp }) >> 4;
                if along > 0 {
                    out.dyaw -= kick;
                } else {
                    out.dyaw += kick;
                }
            }
            // The struck car's rails velocity as AmiGTA1 seeds it here.
            let seed = |o: &Car| {
                let (sfx, sfy) = (i64::from(sin(o.face)), -i64::from(cos(o.face)));
                (
                    ((sfx >> 6) * (o.speed >> 8)) >> 8,
                    ((sfy >> 6) * (o.speed >> 8)) >> 8,
                )
            };
            let inertia = i64::from(oi.moment).max(1);
            if vrel > KNOCK_HARD {
                // A crash: the original's push, the striker's step plus the
                // line between them, scaled by its capped mass.
                let am = pm.min(HIT_MASS_CAP);
                let pushx = (((pvx >> FP) + dx) * am) / HIT_SCALE;
                let pushy = (((pvy >> FP) + dy) * am) / HIT_SCALE;
                if o.knock == 0 {
                    (o.kvx, o.kvy) = seed(o);
                    if om < HIT_LIGHT {
                        o.kvx = (o.kvx >> 8) * KNOCK_KEEP;
                        o.kvy = (o.kvy >> 8) * KNOCK_KEEP;
                    }
                }
                o.kvx += (pushx << FP) / om;
                o.kvy += (pushy << FP) / om;
                let (rx, ry) = contact_arm(o, oi, dx, dy);
                let torque = (rx * pushy - ry * pushx) * 4;
                o.komega += ((torque * HIT_SPIN) / inertia) << 6;
                o.komega = o.komega.clamp(-KNOCK_SPIN_MAX, KNOCK_SPIN_MAX);
                if o.speed < SPEED_UNIT && pspd > HIT_SHOVE_MIN * SPEED_UNIT {
                    o.speed = 4 * SPEED_UNIT;
                }
                if o.knock == 0 {
                    o.face16 = i64::from(o.face) << 16;
                }
                o.knock = KNOCK_TICKS;
                o.hit_latch = HIT_LATCH;
                out.dvx -= pvx >> 1;
                out.dvy -= pvy >> 1;
            } else {
                // A lean: the struck car takes its share of the impulse and
                // rolls loose, with no latch and no damage.
                if o.knock == 0 {
                    (o.kvx, o.kvy) = seed(o);
                    o.face16 = i64::from(o.face) << 16;
                }
                o.kvx += nram(jt, nx);
                o.kvy += nram(jt, ny);
                let (rx, ry) = contact_arm(o, oi, dx, dy);
                let torque = (rx * (nram(jt, ny) >> FP) - ry * (nram(jt, nx) >> FP)) * 4;
                o.komega += ((torque * HIT_SPIN) / inertia) << 6;
                o.komega = o.komega.clamp(-KNOCK_SPIN_MAX, KNOCK_SPIN_MAX);
                o.knock = KNOCK_TICKS;
                o.hold = hold::NONE;
            }

            // Bodywork for a real impact only, once in RAM_COOL ticks: the
            // momentum difference, shared out against each car's own mass.
            if vrel > RAM_HARD && o.ram_cool == 0 {
                let (pux, puy) = (pvx >> 13, pvy >> 13);
                let (oux, ouy) = (ovx >> 13, ovy >> 13);
                let ax = (pm * pux - om * oux).abs();
                let ay = (pm * puy - om * ouy).abs();
                let sev = (ax + ay) / 16;
                let dmg_o = (sev / (2 * om)).clamp(1, 100) as i32;
                let dmg_p = (sev / (2 * pm)).clamp(1, 100) as i32;
                if !o.invuln {
                    o.damage += dmg_o;
                    o.player_kill = true;
                }
                out.damage += dmg_p;
                o.dmg_bits |= 1 << panel_delta(oi, o.x, o.y, o.face, plx, ply);
                o.ram_cool = RAM_COOL;
                out.hits += 1;
            }
        }
        out
    }

    /// One car's turn in the wreck sweep: a write-off catches fire and,
    /// [`cars::FUSE`] ticks later, blows up and stays where it stood as a
    /// wreck nobody drives (AmiGTA1's `wreck_sweep`). Returns the car's
    /// model, centre, heading and layer on the tick it blows up, for the
    /// blast.
    pub fn wreck_step(&mut self, i: usize, say: &mut dyn FnMut(String)) -> Option<WreckStep> {
        let c = &mut self.cars[i];
        if c.done || c.wrecked || c.damage < cars::WRECKED {
            return None;
        }
        if c.fuse == 0 {
            c.fuse = cars::FUSE;
            c.dmg_bits |= cars::DAMAGE_MASK;
            say(format!(
                "traffic: car {i} (model {}) is a write-off - burning",
                c.model
            ));
            return None;
        }
        c.fuse -= 1;
        if c.fuse > 0 {
            return None;
        }
        say(format!(
            "traffic: car {i} (model {}) blew up at ({},{})",
            c.model,
            c.x >> 16,
            c.y >> 16
        ));
        c.wrecked = true;
        c.abandoned = true;
        c.speed = 0;
        c.knock = 0;
        c.fuse = 0;
        c.fire_ticks = 400;
        c.cop = 0;
        c.svc = 0;
        c.hold = hold::NONE;
        Some(WreckStep {
            model: c.model,
            x: c.x,
            y: c.y,
            face: c.face,
            layer: c.layer,
            bombed: std::mem::take(&mut c.bombed),
            player: std::mem::take(&mut c.player_kill),
        })
    }

    /// The cars standing with nobody in them: parked, left or wrecked.
    pub fn standing(&self) -> impl Iterator<Item = &Car> {
        self.cars[..self.n]
            .iter()
            .filter(|c| !c.done && c.abandoned)
    }
}

/// A fleet car blowing up in the wreck sweep: what and where, and whether
/// its bomb went off and whether the player caused it.
#[derive(Debug, Clone, Copy)]
pub struct WreckStep {
    pub model: usize,
    pub x: i64,
    pub y: i64,
    pub face: i32,
    pub layer: i32,
    pub bombed: bool,
    pub player: bool,
}
