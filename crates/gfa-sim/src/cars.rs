//! Cars in the world: the ones parked by the map, and the player getting
//! into one, driving it and getting out.
//!
//! Ported from AmiGTA1's main loop (`gta_main.c`, MIT): the door point and
//! door-side rules, the walk to the door and the ten-step get-in, the
//! nine-step get-out with the car still rolling, the ramp climb judged at
//! the nose, the jump off a ramp with nothing beyond it, bodywork damage
//! from walls, and the vault over a car's roof. Parked cars come from the map's startup objects, as in
//! the original (Carnage3D `CreateStartupObjects`).

use crate::hash::{SimHash, StateHasher};
use crate::map::{SimMap, ground};
use crate::vehicle::{CLASS_BIKE, EXIT_MAX_SPEED, Pedals, Veh, veh_layer};
use gfa_fixed::trig::{cos, sin};
use gfa_formats::cmp::StartupObject;
use gfa_formats::style::CarInfo;

/// How far the player reaches for a car: 48 world px from its centre.
pub const REACH: i32 = 48;
/// How close the walker may come to a car's body, in px.
pub const PED_CAR_MARGIN: i32 = 3;
/// Get-in steps and ticks per step: ped frames 26 26 26 25 25 29..33.
pub const ENTER_SEQ: [u16; 10] = [26, 26, 26, 25, 25, 29, 30, 31, 32, 33];
pub const ENTER_TICKS: i32 = 4;
/// Get-out: ped frames 16..24, four ticks each.
pub const EXIT_FIRST: u16 = 16;
pub const EXIT_FRAMES: i32 = 9;
pub const EXIT_TICKS: i32 = 4;
/// A bike: mount 80..83, ride 84, dismount 85..88.
pub const BIKE_ENTER_FIRST: u16 = 80;
pub const BIKE_SIT: u16 = 84;
/// A driver seen in a convertible.
pub const SIT_IN_CAR: u16 = 97;
pub const BIKE_EXIT_FIRST: u16 = 85;
pub const BIKE_FRAMES: i32 = 4;
/// Along and across offsets from the hinge through the nine exit steps.
const EXIT_ALONG: [i64; 9] = [0, 0, 0, 0, 0, -2, -2, -2, -2];
const EXIT_LAT: [i64; 9] = [-2, -2, -2, -1, 1, 0, 0, 0, 1];
/// The jump: lift and gravity a tick, and the height cap (five blocks).
const LIFT: i64 = 1 << 16;
const GRAVITY: i64 = 1 << 16;
const Z_MAX: i64 = 160 << 16;
/// The vault: a state lasts four ticks (the get-in's clock), he may spend
/// at most 15 states on a roof before he is put down, and a vehicle must
/// stand lower than 20 (the car table's `vert`: 10 for a saloon, 24 for a
/// bus) to be vaulted rather than slid under.
pub const VAULT_TICKS: i32 = 4;
pub const VAULT_HOLD_MAX: i32 = 15;
pub const VAULT_MAX_VERT: i16 = 20;
/// Damage at which a car is a write-off, and how long it then burns.
pub const WRECKED: i32 = 100;
pub const FUSE: i32 = 120;
/// The style's damage deltas on a car sprite: front right, back left,
/// middle left, front left, back right, middle right, and the windscreen.
pub const DELTA_FR: u32 = 0;
pub const DELTA_BL: u32 = 1;
pub const DELTA_ML: u32 = 2;
pub const DELTA_FL: u32 = 3;
pub const DELTA_BR: u32 = 4;
pub const DELTA_MR: u32 = 5;
pub const DELTA_WINDSCREEN: u32 = 10;
/// Every damage delta: what a write-off shows.
pub const DAMAGE_MASK: u32 = 0x3f | (1 << DELTA_WINDSCREEN);
/// The door's four opening deltas start here.
pub const DELTA_DOOR: u32 = 6;
/// The door's frame through the nine exit steps (0 shut, 1..4 opening).
const EXIT_DOOR: [u32; 9] = [2, 3, 4, 4, 4, 3, 2, 1, 0];
/// How long the door takes to open and close on the way in, in ticks.
pub const DOOR_TICKS: i32 = 40;

/// The door delta `tick` ticks into the get-in, if it is open: four frames
/// opening and three closing, five ticks each (AmiGTA1 `door_delta`). It is
/// still closing for a moment after the player is seated.
pub fn door_delta(tick: i32) -> Option<u32> {
    let f = tick / 5;
    if f <= 0 || f >= 8 {
        return None;
    }
    let f = if f > 4 { 8 - f } else { f } as u32;
    Some(DELTA_DOOR + f - 1)
}

/// The door delta at exit step `step`, if it is open.
pub fn exit_door_delta(step: i32) -> Option<u32> {
    match EXIT_DOOR[step.clamp(0, 8) as usize] {
        0 => None,
        f => Some(DELTA_DOOR + f - 1),
    }
}

/// Vehicle classes on rails: trains and trams have their own code.
const TRAIN: u8 = 8;
const TRAM: u8 = 9;

/// A vehicle standing in the street.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parked {
    /// Index into the style's car table.
    pub model: usize,
    /// Centre, 16.16 world px.
    pub x: i32,
    pub y: i32,
    /// Heading, 0..256.
    pub face: i32,
    pub layer: i32,
    /// Colour remap, 0 for none.
    pub remap: u8,
    pub damage: i32,
    /// Dented panels, kept when the player gets in and out.
    pub dmg_bits: u32,
    /// Burnt out: nobody gets in.
    pub wrecked: bool,
}

impl SimHash for Parked {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.u32(self.model as u32);
        h.i32(self.x);
        h.i32(self.y);
        h.i32(self.face);
        h.i32(self.layer);
        h.u8(self.remap);
        h.i32(self.damage);
        h.u32(self.dmg_bits);
        h.bool(self.wrecked);
    }
}

/// The car table record of a map object that is a vehicle the simulation
/// owns: anything with a remap byte of 128 or more, except trains and
/// trams.
pub fn sim_vehicle(o: &StartupObject, cars: &[CarInfo]) -> Option<usize> {
    if o.remap < 128 {
        return None;
    }
    let model = cars.iter().position(|c| c.model == o.kind)?;
    (!matches!(cars[model].vtype, TRAIN | TRAM)).then_some(model)
}

/// The map's parked vehicles. A startup object's position is in the
/// original's pixels (64 a block), its height counts down from the top of
/// the map and its rotation is 1024 steps clockwise from north.
pub fn parked_from_map(map: &SimMap, cars: &[CarInfo]) -> Vec<Parked> {
    map.cmp()
        .objects
        .iter()
        .filter_map(|o| {
            let model = sim_vehicle(o, cars)?;
            let height = 6 * 64 - 1 - i32::from(o.z);
            Some(Parked {
                model,
                x: i32::from(o.x) << 15,
                y: i32::from(o.y) << 15,
                face: (i32::from(o.rotation) >> 2) & 255,
                layer: (height >> 6).clamp(0, 5),
                remap: o.remap - 128,
                damage: 0,
                dmg_bits: 0,
                wrecked: false,
            })
        })
        .collect()
}

/// Can a person get into this vehicle? It needs a door, or to be a bike.
pub fn enterable(ci: &CarInfo) -> bool {
    !ci.doors.is_empty() || ci.vtype == CLASS_BIKE
}

/// Which panel a hit at `(hx, hy)` lands on, for a car centred at
/// `(cx, cy)` facing `face`: thirds of the length (front, middle, back) and
/// the side, as a delta number (AmiGTA1 `gta_car_panel_delta`).
pub fn panel_delta(ci: &CarInfo, cx: i64, cy: i64, face: i32, hx: i64, hy: i64) -> u32 {
    const PANEL: [u32; 6] = [DELTA_FL, DELTA_ML, DELTA_BL, DELTA_FR, DELTA_MR, DELTA_BR];
    let (fx, fy) = (i64::from(sin(face)), -i64::from(cos(face)));
    let (rx, ry) = (i64::from(cos(face)), i64::from(sin(face)));
    let (dx, dy) = ((hx - cx) >> 16, (hy - cy) >> 16);
    let along = (dx * fx + dy * fy) >> 14;
    let across = (dx * rx + dy * ry) >> 14;
    let half = (i64::from(ci.length >> 1) / 2).max(3);
    let third = if along > half / 3 {
        0
    } else if along < -half / 3 {
        2
    } else {
        1
    };
    let side = usize::from(across >= 0);
    PANEL[side * 3 + third]
}

/// Which flank the door is on: -1 when the model has a door record (the
/// side its door art opens on), +1 for a bike.
pub fn door_side(ci: &CarInfo) -> i64 {
    if ci.doors.is_empty() { 1 } else { -1 }
}

/// A body's half length and half width in world px.
pub fn half_size(ci: &CarInfo) -> (i32, i32) {
    (i32::from(ci.length >> 1) / 2, i32::from(ci.width >> 1) / 2)
}

/// Can the player go over this vehicle's roof? Anything lower than
/// [`VAULT_MAX_VERT`]; a taller one is slid under.
pub fn vaultable(ci: &CarInfo) -> bool {
    ci.vert < VAULT_MAX_VERT
}

/// Getting in from the far side. When the player stands on the flank away
/// from the door, clear of the body and no more than 59 degrees off
/// square-on (tan 59° is 5/3), and his straight line to the door crosses
/// the body, he runs at that crossing and goes over the roof. Returns the
/// point 2 px short of the near flank where his line meets it, and the
/// heading straight across the car. AmiGTA1 reads the original's vault
/// trigger off the geometry this way, once, at RETURN.
pub fn far_side(
    ci: &CarInfo,
    car: (i64, i64, i32),
    player: (i64, i64),
    door: (i64, i64),
) -> Option<((i64, i64), i32)> {
    let (cx, cy, face) = car;
    let (fx, fy) = (i64::from(sin(face)), -i64::from(cos(face)));
    let (rx, ry) = (i64::from(cos(face)), i64::from(sin(face)));
    // Along and across the body, in whole px from its centre.
    let local = |x: i64, y: i64| {
        let (dx, dy) = ((x - cx) >> 16, (y - cy) >> 16);
        ((dx * fx + dy * fy) >> 14, (dx * rx + dy * ry) >> 14)
    };
    let (along_p, across_p) = local(player.0, player.1);
    let (along_d, across_d) = local(door.0, door.1);
    let (hl, hw) = half_size(ci);
    let (hl, hw) = (i64::from(hl), i64::from(hw));
    let opposite = (across_p < 0) != (across_d < 0);
    if !opposite || across_p.abs() <= hw || along_p.abs() * 3 > across_p.abs() * 5 {
        return None;
    }
    let edge = if across_p < 0 { -hw } else { hw };
    // Integer division truncates toward zero, as C's does.
    let along_hit = along_p + ((along_d - along_p) * (across_p - edge)) / (across_p - across_d);
    if along_hit.abs() > hl {
        return None;
    }
    let out = if across_p < 0 { edge - 2 } else { edge + 2 };
    let point = (
        cx + (fx * along_hit + rx * out) * 4,
        cy + (fy * along_hit + ry * out) * 4,
    );
    let head = (if across_d < 0 { face - 64 } else { face + 64 }) & 255;
    Some((point, head))
}

/// Where the player stands to get in: level with the hinge along the body
/// (the door record's `rpx`, halved), half a body width plus 5 px out on the
/// door side (AmiGTA1 `car_door_point`).
pub fn door_point(ci: &CarInfo, cx: i64, cy: i64, face: i32) -> (i64, i64) {
    let (fx, fy) = (i64::from(sin(face)), -i64::from(cos(face)));
    let (rx, ry) = (i64::from(cos(face)), i64::from(sin(face)));
    let along = ci.doors.first().map_or(0, |d| i64::from(d.rpx) / 2);
    let out = i64::from(ci.width >> 1) / 2 + 5;
    let across = if ci.doors.is_empty() { out } else { -out };
    (
        cx + (fx * along + rx * across) * 4,
        cy + (fy * along + ry * across) * 4,
    )
}

/// A point near the door: `along` px from the hinge along the body, `lat`
/// px outside the body's edge on the door side (AmiGTA1 `car_door_pos`).
pub fn door_pos(ci: &CarInfo, cx: i64, cy: i64, face: i32, along: i64, lat: i64) -> (i64, i64) {
    let (fx, fy) = (i64::from(sin(face)), -i64::from(cos(face)));
    let (rx, ry) = (i64::from(cos(face)), i64::from(sin(face)));
    let along = ci.doors.first().map_or(0, |d| i64::from(d.rpx) / 2) + along;
    let across = door_side(ci) * (i64::from(ci.width >> 1) / 2 + lat);
    (
        cx + (fx * along + rx * across) * 4,
        cy + (fy * along + ry * across) * 4,
    )
}

/// Is the point inside a car body (half length `hl`, half width `hw`, whole
/// px) grown by `margin` px?
#[allow(clippy::too_many_arguments)]
pub fn body_hit(
    cx: i64,
    cy: i64,
    face: i32,
    hl: i32,
    hw: i32,
    px: i64,
    py: i64,
    margin: i32,
) -> bool {
    let (fx, fy) = (i64::from(sin(face)), -i64::from(cos(face)));
    let (rx, ry) = (i64::from(cos(face)), i64::from(sin(face)));
    let (dx, dy) = ((px - cx) >> 16, (py - cy) >> 16);
    let along = ((dx * fx + dy * fy) >> 14).abs();
    let across = ((dx * rx + dy * ry) >> 14).abs();
    along <= i64::from(hl + margin) && across <= i64::from(hw + margin)
}

/// `a + (b - a) * t / n`, exactly as AmiGTA1 rounds it.
pub fn lerp_fp(a: i64, b: i64, t: i32, n: i32) -> i64 {
    if n <= 0 || t >= n {
        return b;
    }
    if t <= 0 {
        return a;
    }
    let (d, t, n) = (b - a, i64::from(t), i64::from(n));
    a + (d / n) * t + ((d % n) * t) / n
}

/// The short way round from `a` to `b`, `t` of `n` of the way.
pub fn lerp_angle(a: i32, b: i32, t: i32, n: i32) -> i32 {
    if n <= 0 || t >= n {
        return b & 255;
    }
    if t <= 0 {
        return a & 255;
    }
    let mut d = (b - a) & 255;
    if d > 128 {
        d -= 256;
    }
    (a + (d * t) / n) & 255
}

/// A car in the air off a ramp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Air {
    /// Height above the layer it left, 16.16 px.
    pub z: i64,
    pub vz: i64,
    /// Ticks of climb left.
    pub lift: i32,
    /// Distance flown, 16.16 px (for the log).
    pub flown: i64,
}

/// The player's car while he is in it or getting out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driven {
    pub veh: Veh,
    pub layer: i32,
    pub air: Option<Air>,
    /// Ticks since the door began to open on the way in; -1 when shut.
    pub door_tick: i32,
    /// The fleet serial it had before he got in, given back when he
    /// leaves it (0 for a car that was never in the fleet), and whether a
    /// mission had marked it.
    pub serial: u64,
    pub mission: bool,
}

impl SimHash for Driven {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.veh.sim_hash(h);
        h.i32(self.layer);
        h.i32(self.door_tick);
        h.bytes(&self.serial.to_le_bytes());
        h.bool(self.mission);
        match self.air {
            Some(a) => {
                h.bytes(&a.z.to_le_bytes());
                h.bytes(&a.vz.to_le_bytes());
                h.i32(a.lift);
                h.bytes(&a.flown.to_le_bytes());
            }
            None => h.u8(0),
        }
    }
}

impl Driven {
    /// One tick of driving: the physics, a take-off or the flight, the
    /// layer from the nose, then the walls and their damage. Lines for the
    /// run log go to `say`. Returns the layer of the water the car fell
    /// into, if a jump fell short over water.
    pub fn tick(
        &mut self,
        map: &SimMap,
        ci: &CarInfo,
        pedals: Pedals,
        say: &mut impl FnMut(String),
    ) -> Option<i32> {
        let mut water = None;
        let v = &mut self.veh;
        let on_road = map.nav_ground((v.ox >> 21) as i32, (v.oy >> 21) as i32, self.layer) == 2;
        let (x0, y0, ox0, oy0, ang0) = (v.x, v.y, v.ox, v.oy, v.ang16);
        v.step(pedals, on_road);

        if self.air.is_none() {
            self.take_off(map, say);
        }
        if let Some(air) = self.air.as_mut() {
            let v = &self.veh;
            air.z += air.vz;
            air.flown += (v.ox - ox0).abs() + (v.oy - oy0).abs();
            if air.lift > 0 {
                air.lift -= 1;
            } else {
                air.vz -= GRAVITY;
            }
            if air.z > Z_MAX {
                air.z = Z_MAX;
                air.vz = 0;
            }
            if air.z <= 0 && air.vz <= 0 {
                let (bx, by) = ((v.ox >> 21) as i32, (v.oy >> 21) as i32);
                match map.stand_layer(bx, by) {
                    // Any surface at or below the layer it left.
                    Some(lz) if lz <= self.layer => {
                        say(format!(
                            "car: landed at ({bx},{by}) layer {lz} after {} px",
                            air.flown >> 16
                        ));
                        self.layer = lz;
                        self.air = None;
                    }
                    _ if air.z < -(32 << 16) => {
                        say(format!("car: jump fell short at ({bx},{by})"));
                        self.air = None;
                        // What it fell into answers: the first surface
                        // under the layer it left.
                        let below = (0..=self.layer)
                            .rev()
                            .find(|&z| map.nav_ground(bx, by, z) != ground::AIR);
                        if let Some(z) = below
                            && map.nav_ground(bx, by, z) == ground::WATER
                        {
                            say(format!("car: into the water at ({bx},{by}) layer {z}"));
                            water = Some(z);
                        }
                    }
                    _ => {}
                }
            }
        }

        if self.air.is_none() {
            let v = &self.veh;
            let (nx0, ny0) = v.nose(ox0, oy0, ang0);
            let (nx1, ny1) = v.nose(v.ox, v.oy, v.ang16);
            let blk = |p: i64| (p >> 21) as i32;
            let nz = veh_layer(
                map,
                self.layer,
                blk(nx0),
                blk(ny0),
                blk(nx1),
                blk(ny1),
                nx1 - nx0,
                ny1 - ny0,
            );
            if nz != self.layer {
                say(format!(
                    "car: layer {} -> {nz} at ({},{})",
                    self.layer,
                    blk(v.ox),
                    blk(v.oy)
                ));
                self.layer = nz;
            }
            let hit = self.veh.wall(map, self.layer, x0, y0, ox0, oy0, ang0);
            // A wall costs the impact speed less a grace pixel, and dents
            // the panel that met it.
            let dmg = hit - 1;
            if dmg > 0 {
                let v = &mut self.veh;
                v.damage += dmg;
                let (hx, hy) = (v.ox + v.hit_vx * 4, v.oy + v.hit_vy * 4);
                v.dmg_bits |= 1 << panel_delta(ci, v.ox, v.oy, v.angle(), hx, hy);
                say(format!(
                    "car: wall hit at {hit} px/step - damage {}",
                    v.damage
                ));
            }
        }
        water
    }

    /// Leave the ground if the car is driving up a ramp, fast enough, into
    /// a block with nothing to drive on that is not more ramp.
    fn take_off(&mut self, map: &SimMap, say: &mut impl FnMut(String)) {
        let v = &self.veh;
        let (bx, by) = ((v.ox >> 21) as i32, (v.oy >> 21) as i32);
        let Some(up) = map.slope_up_dir(bx, by, self.layer) else {
            return;
        };
        const DX: [i32; 4] = [0, 1, 0, -1];
        const DY: [i32; 4] = [-1, 0, 1, 0];
        let q = ((up >> 6) & 3) as usize;
        let speed = (v.vx.abs().max(v.vy.abs()) >> 16) as i32;
        let going = (DX[q] > 0 && v.vx > 0)
            || (DX[q] < 0 && v.vx < 0)
            || (DY[q] > 0 && v.vy > 0)
            || (DY[q] < 0 && v.vy < 0);
        let (ax, ay) = (bx + DX[q], by + DY[q]);
        let solid = map.stand_layer(ax, ay).is_some_and(|z| z >= self.layer);
        let ahead_ramp = map.slope_up_dir(ax, ay, self.layer).is_some();
        if going && !solid && !ahead_ramp && speed >= 4 {
            self.air = Some(Air {
                z: 0,
                vz: LIFT,
                lift: speed * 2,
                flown: 0,
            });
            say(format!(
                "car: jump from ({bx},{by}) layer {} at {speed} px/step",
                self.layer
            ));
        }
    }

    /// Height of the car's base for drawing: the ground under its centre,
    /// plus its height in the air. 16.16, 32 px a layer.
    pub fn z(&self, map: &SimMap) -> i64 {
        let ground = i64::from(map.ground_z(self.veh.ox as i32, self.veh.oy as i32, self.layer));
        ground + self.air.map_or(0, |a| a.z)
    }
}

/// The get-out speed limit, for callers that want to say why.
pub fn too_fast_to_exit(v: &Veh) -> bool {
    v.exit_speed() >= EXIT_MAX_SPEED
}

/// Exit step `s`'s offsets from the hinge.
pub fn exit_offsets(s: i32) -> (i64, i64) {
    let s = s.clamp(0, EXIT_FRAMES - 1) as usize;
    (EXIT_ALONG[s], EXIT_LAT[s])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerps() {
        assert_eq!(lerp_fp(0, 100, 1, 3), 33);
        assert_eq!(lerp_fp(0, -100, 2, 3), -66);
        assert_eq!(lerp_fp(5, 9, 0, 4), 5);
        assert_eq!(lerp_fp(5, 9, 7, 4), 9);
        assert_eq!(lerp_angle(250, 6, 1, 2), 0);
        assert_eq!(lerp_angle(6, 250, 1, 2), 0);
        assert_eq!(lerp_angle(0, 128, 1, 2), 64);
    }
}
