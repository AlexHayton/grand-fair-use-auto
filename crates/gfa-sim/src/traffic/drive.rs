//! One car's tick: where it is heading, whether it may enter the junction
//! ahead, its speed, its turn, and its move.
//!
//! After AmiGTA1 (MIT), `gta_traffic.c` (`drive_one` and the steps it
//! calls). See the module above for the switches fixed at AmiGTA1's
//! defaults.

use gfa_fixed::trig::{cos, dir16, sin, step_dir};
use gfa_formats::style::CarInfo;

use super::roads::{self, heading_step, step_angle};
use super::*;
use crate::map::ground;

/// Why the last booking failed, for the fallbacks after it.
#[derive(Debug, Clone, Copy, Default)]
struct ClaimWhy {
    why: i32,
    side: i32,
}

/// The angle in a 16.16 heading difference, wrapped to -128..128 steps.
#[inline]
fn wrap24(d: i64) -> i64 {
    let d = d & 0xFF_FFFF;
    if d > 0x80_0000 { d - 0x100_0000 } else { d }
}

#[inline]
fn cos_fp(a_fp: i64) -> i64 {
    let i = ((a_fp >> FP) & 255) as i32;
    let f = a_fp & 0xFFFF;
    let (c0, c1) = (i64::from(cos(i)), i64::from(cos(i + 1)));
    c0 + (((c1 - c0) * f) >> FP)
}

#[inline]
fn sin_fp(a_fp: i64) -> i64 {
    cos_fp(a_fp - (64 << FP))
}

impl Traffic {
    /// The distance from the centre to the edge of block `(bx, by)` ahead.
    fn to_block_edge(c: &Car, bx: i32, by: i32) -> i64 {
        let v = match c.angle {
            0 => c.y - (i64::from(by * 32) << FP),
            128 => (i64::from((by + 1) * 32) << FP) - c.y,
            192 => c.x - (i64::from(bx * 32) << FP),
            _ => (i64::from((bx + 1) * 32) << FP) - c.x,
        };
        v.max(0)
    }

    /// The route's way out of `(bx, by)`; steps past nodes already reached.
    fn route_dir(c: &mut Car, bx: i32, by: i32) -> i32 {
        while c.path_i < c.path_n() {
            let (nx, ny) = (i32::from(c.path[c.path_i].x), i32::from(c.path[c.path_i].y));
            if nx == bx && ny == by {
                c.path_i += 1;
                continue;
            }
            return match (nx - bx, ny - by) {
                (0, -1) => 0,
                (1, 0) => 64,
                (0, 1) => 128,
                (-1, 0) => 192,
                _ => -1,
            };
        }
        -1
    }

    /// Blocks of road ahead, up to `maxb`: along the route if there is
    /// one, otherwise along the arrows.
    #[allow(clippy::too_many_arguments)]
    fn road_clear(
        map: &SimMap,
        c: &Car,
        bx: i32,
        by: i32,
        mut z: i32,
        angle: i32,
        maxb: i32,
    ) -> i32 {
        if c.path_i < c.path_n() {
            for i in 0..maxb {
                let k = c.path_i + i as usize;
                if k >= c.path_n() {
                    return maxb;
                }
                let p = c.path[k];
                if roads::nav_dirs(map, i32::from(p.x), i32::from(p.y), z) == 0 {
                    return i;
                }
            }
            return maxb;
        }
        const BIT4: [u8; 4] = [nav::N, nav::S, nav::W, nav::E];
        const DIR_OF: [i32; 4] = [0, 128, 192, 64];
        let (mut cx, mut cy, mut dir) = (bx, by, angle);
        for i in 0..maxb {
            let mut nz = roads::nav_step_layer(map, cx, cy, z, dir);
            let (mut dx, mut dy) = heading_step(dir);
            if nz.is_none() {
                let here = roads::nav_dirs(map, cx, cy, z);
                let back = (dir + 128) & 255;
                let mut found = None;
                for k in 0..4 {
                    let nd = DIR_OF[k];
                    if nd == back || nd == dir || here & BIT4[k] == 0 {
                        continue;
                    }
                    if let Some(kz) = roads::nav_step_layer(map, cx, cy, z, nd) {
                        found = Some(nd);
                        nz = Some(kz);
                        break;
                    }
                }
                let Some(f) = found else {
                    return i;
                };
                dir = f;
                (dx, dy) = heading_step(dir);
            }
            cx += dx;
            cy += dy;
            z = nz.unwrap_or(z);
        }
        maxb
    }

    /// A way out of `(bx, by)` by its arrows, not back the way it came,
    /// preferring ways that lead on; straight on three times in four when
    /// that is one of them.
    #[allow(clippy::too_many_arguments)]
    fn choose_heading(
        &mut self,
        map: &SimMap,
        bx: i32,
        by: i32,
        z: i32,
        cur: i32,
        length: i32,
    ) -> i32 {
        let Some(d) = roads::raw_dirs(map, bx, by, z) else {
            return -1;
        };
        let back = (cur + 128) & 255;
        let all: Vec<i32> = [(0x01, 0), (0x08, 64), (0x02, 128), (0x04, 192)]
            .iter()
            .filter(|(bit, _)| d & bit != 0)
            .map(|&(_, a)| a)
            .collect();
        let (mut cand, mut poor) = (Vec::new(), Vec::new());
        for &a in &all {
            if a == back {
                continue;
            }
            let (dx, dy) = heading_step(a);
            let Some(nz) = roads::nav_step_layer(map, bx, by, z, a) else {
                continue;
            };
            if !roads::fits_entering(map, bx + dx, by + dy, nz, a, length) {
                continue;
            }
            if roads::has_exit(map, bx + dx, by + dy, nz, a, length) {
                cand.push(a);
            } else {
                poor.push(a);
            }
        }
        if cand.is_empty() {
            cand = poor;
        }
        if cand.is_empty() {
            let (dx, dy) = heading_step(back);
            let axis = if back & 127 == 64 { 64 } else { 0 };
            if roads::dir_allowed(map, bx, by, z, back)
                && roads::drivable(map, bx + dx, by + dy, z)
                && roads::dir_allowed(map, bx + dx, by + dy, z, back)
                && roads::fits(map, bx, by, z, axis, length)
                && roads::fits(map, bx + dx, by + dy, z, axis, length)
            {
                return back;
            }
            return -1;
        }
        if self.goal_on {
            let mut best = cand[0];
            let mut best_d = i32::MAX;
            for &a in &cand {
                let (dx, dy) = heading_step(a);
                let d = (bx + dx - self.goal_x).abs() + (by + dy - self.goal_y).abs();
                if d < best_d || (d == best_d && a == cur) {
                    best_d = d;
                    best = a;
                }
            }
            return best;
        }
        for &a in &cand {
            if a == cur && self.next_rand() & 3 != 0 {
                return cur;
            }
        }
        cand[(self.next_rand() % cand.len() as u64) as usize]
    }

    /// Point car `idx`'s lane line along block `(bx, by)` heading `dir`.
    fn line_from_block(&mut self, map: &SimMap, idx: usize, bx: i32, by: i32, dir: i32) {
        let layer = self.cars[idx].layer;
        let tgt = roads::lane_target(map, &self.boxes, bx, by, layer, dir);
        let c = &mut self.cars[idx];
        c.lane_axis = i32::from(dir & 127 == 64);
        c.lane_off = i64::from((if c.lane_axis != 0 { by } else { bx }) * 32 + tgt) << FP;
        c.lane_dir = dir & 255;
        c.lane_set = true;
        c.lane_target = tgt;
    }

    /// Steer towards a point on the lane line ahead (pure pursuit), turning
    /// at a rate that grows with speed. Returns the new face and whether
    /// the turn still wanted is sharp enough to slow for.
    fn steer_along_line(c: &mut Car) -> (i32, bool) {
        if !c.lane_set {
            return (c.face, false);
        }
        let line = Self::lane_line(c);
        let l = ((c.speed * PURSUIT_K) >> FP).clamp(i64::from(PURSUIT_MIN), i64::from(PURSUIT_MAX));
        let (tx, ty) = if c.lane_axis != 0 {
            (c.x + ((if c.lane_dir == 64 { l } else { -l }) << FP), line)
        } else {
            (line, c.y + ((if c.lane_dir == 128 { l } else { -l }) << FP))
        };
        let want16 = i64::from(dir16(((tx - c.x) >> 8) as i32, ((ty - c.y) >> 8) as i32));
        if ((c.face16 >> FP) & 255) != i64::from(c.face) {
            c.face16 = i64::from(c.face) << FP;
        }
        let mut d = wrap24(want16 - c.face16);
        let slow = !(-(STEER_SLOW << FP)..=(STEER_SLOW << FP)).contains(&d);
        let w =
            (((c.speed >> 6) * STEER_W) / i64::from(TURN_RADIUS)).clamp(STEER_W_MIN, STEER_W_MAX);
        d = d.clamp(-w, w);
        c.face16 = (c.face16 + d) & 0xFF_FFFF;
        c.face = ((c.face16 >> FP) & 255) as i32;
        // On the line and nearly square to it: snap square.
        let err = (if c.lane_axis != 0 { c.y } else { c.x } - line).abs();
        let dd = wrap24((i64::from(c.lane_dir) << FP) - c.face16);
        if err <= (1 << FP) && dd < (1 << FP) && dd > -(1 << FP) {
            c.face16 = i64::from(c.lane_dir) << FP;
            c.face = c.lane_dir;
        }
        (c.face, slow)
    }

    /// The line car `c` steers along: its lane, or while it is pulled over,
    /// the kerb in the same block, on its right (right-hand traffic) or,
    /// where that is road, its left.
    fn lane_line(c: &Car) -> i64 {
        if c.pull == 0 {
            return c.lane_off;
        }
        // Heading north or east, the right is the far side of the block.
        let plus = (c.lane_dir == 0 || c.lane_dir == 64) != c.pull_left;
        let off = LANE_TARGET + if plus { PULL_KERB } else { -PULL_KERB };
        ((c.lane_off >> BS) * 32 + i64::from(off)) << FP
    }

    /// Room to back out two blocks?
    fn back_out_clear(&self, map: &SimMap, idx: usize, bx: i32, by: i32) -> bool {
        let c = &self.cars[idx];
        let (dx, dy) = heading_step((c.angle + 128) & 255);
        for k in 1..=2 {
            let (nx, ny) = (bx + dx * k, by + dy * k);
            if !roads::drivable(map, nx, ny, c.layer) {
                return false;
            }
            for (i, o) in self.cars[..self.n].iter().enumerate() {
                if i != idx && !o.done && o.layer == c.layer && o.block() == (nx, ny) {
                    return false;
                }
            }
        }
        true
    }

    /// A collision is carrying the car: slide and spin it, damping, and
    /// when it settles put it back on a lane.
    fn knock_step(&mut self, map: &SimMap, cars: &[CarInfo], idx: usize) {
        let com = (i64::from(cars[self.cars[idx].model].cy) << 16) / 2;
        let umul = |t: i64, v: i64| (t * (v >> 8)) >> 6;
        let c = &mut self.cars[idx];
        c.x += c.kvx;
        c.y += c.kvy;
        let cmx = c.x + umul(i64::from(sin(c.face)), com);
        let cmy = c.y - umul(i64::from(cos(c.face)), com);
        c.face16 = (c.face16 + c.komega) & 0xFF_FFFF;
        c.face = ((c.face16 >> 16) & 255) as i32;
        c.x = cmx - umul(i64::from(sin(c.face)), com);
        c.y = cmy + umul(i64::from(cos(c.face)), com);
        c.angle = c.face;
        {
            let (fx, fy) = (i64::from(sin(c.face)), -i64::from(cos(c.face)));
            let along = ((c.kvx >> 8) * (fx >> 6)) + ((c.kvy >> 8) * (fy >> 6));
            let across = ((c.kvx >> 8) * (-fy >> 6)) + ((c.kvy >> 8) * (fx >> 6));
            let along = (along >> 8) * KNOCK_DAMP;
            let across = (across >> 8) * KNOCK_SIDE_DAMP;
            c.kvx = ((along >> 8) * (fx >> 6)) + ((across >> 8) * (-fy >> 6));
            c.kvy = ((along >> 8) * (fy >> 6)) + ((across >> 8) * (fx >> 6));
        }
        c.komega = (c.komega >> 8) * KNOCK_SPIN;
        let sp = c.kvx.abs() + c.kvy.abs();
        c.knock -= 1;
        if c.knock > 0 && sp >= KNOCK_REST {
            return;
        }
        c.knock = 0;
        c.kvx = 0;
        c.kvy = 0;
        c.komega = 0;
        if c.turn != 0 || c.arc_s > 0 {
            c.turn = 0;
            c.turn_accum = 0;
            c.arc_s = 0;
            c.arc_len = 0;
            c.turn_lock = 0;
        }
        c.angle = (((c.face + 32) >> 6) << 6) & 255;
        c.speed = sp.min(c.top);
        c.hold = hold::NONE;
        c.recover = 0;
        let (kbx, kby) = c.block();
        let (angle, layer) = (c.angle, c.layer);
        let (sdx, sdy) = heading_step((angle + TURN_QUARTER) & 255);
        for k in 0..3 {
            let tx = kbx
                + if k == 1 {
                    sdx
                } else if k == 2 {
                    -sdx
                } else {
                    0
                };
            let ty = kby
                + if k == 1 {
                    sdy
                } else if k == 2 {
                    -sdy
                } else {
                    0
                };
            if roads::ground_at(map, tx, ty, layer) != ground::ROAD {
                continue;
            }
            if !roads::dir_allowed(map, tx, ty, layer, angle) {
                continue;
            }
            self.line_from_block(map, idx, tx, ty, angle);
            break;
        }
    }

    /// The block car `idx` drives by: the one under its middle, unless it
    /// has been shoved out of its lane. Then, if the block under it does not
    /// go its way (the lane beside going the other way, or the kerb) and a
    /// block beside it does (the nearer side first), that one: its route, its
    /// junctions and its lane line are that lane's, and the lane keeper
    /// steers it back. Not for a car turning, changing lanes, reversing,
    /// crossing a junction or on police business, whose heading and lane
    /// may part for a while by design.
    ///
    /// Ours (decision log, 2026-11-21; traffic_rejoin.pl): AmiGTA1 drives
    /// by the block under the middle, so a car pushed a pixel over a lane
    /// line in a collision has nowhere legal to go and stands at a "dead
    /// end" in the wrong lane or on the pavement.
    fn lane_block(&self, map: &SimMap, idx: usize) -> (i32, i32) {
        let c = &self.cars[idx];
        let (bx, by) = c.block();
        let (z, angle) = (c.layer, c.angle);
        if c.turn != 0 || c.swap != 0 || c.reverse != 0 || c.cop != 0 || c.crossing {
            return (bx, by);
        }
        if self.is_junction(bx, by, z)
            || (roads::drivable(map, bx, by, z) && roads::dir_allowed(map, bx, by, z, angle))
        {
            return (bx, by);
        }
        // The nearer side, across the way it is heading, then the other:
        // pushed past the middle of the block beside its lane, its lane is
        // on the far side.
        let along_x = angle & 127 == 64;
        let across = if along_x { c.y } else { c.x };
        let s = if (across >> FP) & 31 >= 16 { 1 } else { -1 };
        for s in [s, -s] {
            let (nx, ny) = if along_x { (bx, by + s) } else { (bx + s, by) };
            if roads::drivable(map, nx, ny, z)
                && roads::dir_allowed(map, nx, ny, z, angle)
                && !self.is_junction(nx, ny, z)
            {
                return (nx, ny);
            }
        }
        (bx, by)
    }

    /// Turn round into the oncoming lane on the left, if the road is wide
    /// enough and the way back is free.
    fn uturn_try(&mut self, map: &SimMap, idx: usize, bx: i32, by: i32) -> bool {
        let (angle, layer) = (self.cars[idx].angle, self.cars[idx].layer);
        let back = (angle + 128) & 255;
        let (fdx, fdy) = heading_step(angle);
        if self.is_junction(bx, by, layer) {
            return false;
        }
        let (sdx, sdy) = heading_step((angle - TURN_QUARTER) & 255);
        let mut tb = None;
        for k in 1..=UTURN_SCAN {
            let (qx, qy) = (bx + sdx * k, by + sdy * k);
            if roads::ground_at(map, qx, qy, layer) != ground::ROAD {
                break;
            }
            if roads::dir_allowed(map, qx, qy, layer, back) {
                tb = Some((qx, qy));
                break;
            }
        }
        let Some((tbx, tby)) = tb else {
            return false;
        };
        for k in 0..3 {
            let (qx, qy) = (tbx - fdx * k, tby - fdy * k);
            if roads::ground_at(map, qx, qy, layer) != ground::ROAD
                || !roads::dir_allowed(map, qx, qy, layer, back)
                || self.body_on_sq(idx, qx, qy, layer, false, 0)
            {
                return false;
            }
        }
        let wide = (1..=3)
            .take_while(|&k| {
                roads::ground_at(map, bx + sdx * k, by + sdy * k, layer) == ground::ROAD
            })
            .count() as i32;
        if wide * 32 < 2 * TURN_RADIUS {
            return false;
        }
        self.line_from_block(map, idx, tbx, tby, back);
        let c = &mut self.cars[idx];
        c.clear_path();
        c.want_route = true;
        c.swap_wait = 0;
        c.swap = 1;
        c.swap_ticks = 0;
        c.swap_bx = tbx;
        c.swap_by = tby;
        c.swap_sdx = sdx;
        c.swap_sdy = sdy;
        true
    }

    /// Lane changes: finish one under way, or start one after waiting long
    /// enough behind a stopped car (failing that, turn round).
    fn lane_swap_step(&mut self, map: &SimMap, idx: usize, bx: i32, by: i32) {
        let layer = self.cars[idx].layer;
        let mut blocked = {
            let c = &self.cars[idx];
            c.speed == 0 && c.hold != hold::LIGHT && (c.lead_kind == 1 || c.lead_kind == 2)
        };
        if blocked && self.cars[idx].lead_kind == 2 {
            let (adx, ady) = heading_step(self.cars[idx].angle);
            if self.is_junction(bx + adx, by + ady, layer) {
                blocked = false;
            }
        }
        if self.cars[idx].swap_cool > 0 {
            self.cars[idx].swap_cool -= 1;
        }
        if self.cars[idx].swap != 0 {
            let mut done = {
                let c = &mut self.cars[idx];
                let derr = wrap24((i64::from(c.lane_dir) << FP) - c.face16);
                let cross_now = if c.lane_axis != 0 { by } else { bx };
                let cross_want = if c.lane_axis != 0 {
                    c.swap_by
                } else {
                    c.swap_bx
                };
                let mut done = cross_now == cross_want
                    && derr < (STEER_SLOW << FP)
                    && derr > -(STEER_SLOW << FP);
                if !done {
                    c.swap_ticks += 1;
                    if c.swap_ticks > LANE_SWAP_MAX {
                        done = true;
                    }
                }
                done
            };
            let (sdx, sdy) = (self.cars[idx].swap_sdx, self.cars[idx].swap_sdy);
            if !done
                && (sdx != 0 || sdy != 0)
                && self.body_on_sq(idx, bx + sdx, by + sdy, layer, false, 0)
            {
                let a = self.cars[idx].angle;
                self.line_from_block(map, idx, bx, by, a);
                done = true;
            }
            if done {
                let c = &mut self.cars[idx];
                c.swap = 0;
                c.swap_ticks = 0;
                c.swap_cool = LANE_SWAP_COOL;
            }
        } else if self.cars[idx].turn == 0
            && blocked
            && self.cars[idx].swap_cool == 0
            && !self.is_junction(bx, by, layer)
        {
            self.cars[idx].swap_wait += 1;
            if self.cars[idx].swap_wait >= LANE_SWAP_WAIT {
                self.cars[idx].swap_wait = 0;
                let angle = self.cars[idx].angle;
                for t in 0..2 {
                    if self.cars[idx].swap != 0 {
                        break;
                    }
                    let sd = if t == 0 { -TURN_QUARTER } else { TURN_QUARTER };
                    let (sdx, sdy) = heading_step((angle + sd) & 255);
                    let (adx, ady) = heading_step(angle);
                    let (nbx, nby) = (bx + sdx, by + sdy);
                    let g = roads::ground_at(map, nbx, nby, layer) == ground::ROAD;
                    let da = g && roads::dir_allowed(map, nbx, nby, layer, angle);
                    let bo = da
                        && (0..LANE_SWAP_LOOK).any(|q| {
                            self.body_on_sq(idx, nbx + adx * q, nby + ady * q, layer, false, 0)
                        });
                    if !g || !da || bo {
                        continue;
                    }
                    self.line_from_block(map, idx, nbx, nby, angle);
                    let c = &mut self.cars[idx];
                    c.swap = 1;
                    c.swap_ticks = 0;
                    c.swap_bx = nbx;
                    c.swap_by = nby;
                    c.swap_sdx = sdx;
                    c.swap_sdy = sdy;
                }
                if self.cars[idx].swap == 0 {
                    self.uturn_try(map, idx, bx, by);
                }
            }
        } else if !blocked {
            self.cars[idx].swap_wait = 0;
        }
    }

    /// Keep the car's layer on the road it is over: up off a ramp's top,
    /// down (or, with `up_fallback`, up) where its layer has no road.
    #[allow(clippy::too_many_arguments)]
    fn car_layer_follow(
        &mut self,
        map: &SimMap,
        idx: usize,
        bx: i32,
        by: i32,
        from_bx: i32,
        from_by: i32,
        up_fallback: bool,
    ) {
        let c = &mut self.cars[idx];
        let was = c.layer;
        if (from_bx != bx || from_by != by)
            && c.layer + 1 < 6
            && roads::drivable(map, bx, by, c.layer + 1)
        {
            let dir = step_dir(bx - from_bx, by - from_by);
            if map.slope_up_dir(from_bx, from_by, c.layer) == Some(dir) {
                c.layer += 1;
            }
        }
        if !roads::drivable(map, bx, by, c.layer) {
            if c.layer > 0 && roads::drivable(map, bx, by, c.layer - 1) {
                c.layer -= 1;
            } else if up_fallback && c.layer + 1 < 6 && roads::drivable(map, bx, by, c.layer + 1) {
                c.layer += 1;
            }
        }
        if c.layer != was {
            c.clear_path();
            c.want_route = true;
        }
    }

    fn car_layer_track(&mut self, map: &SimMap, idx: usize) {
        let (bx, by) = self.cars[idx].block();
        let (lx, ly) = (self.cars[idx].last_bx, self.cars[idx].last_by);
        if bx == lx && by == ly {
            return;
        }
        self.car_layer_follow(map, idx, bx, by, lx, ly, false);
        self.cars[idx].last_bx = bx;
        self.cars[idx].last_by = by;
    }

    /// Where the route leaves the junction it enters at `(nx, ny)`: 0
    /// straight on, 1 left, 2 right, -1 unknown; and the exit block,
    /// shifted to the car's own lane.
    fn route_exit_side(&self, c: &Car, nx: i32, ny: i32, dxs: i32, dys: i32) -> (i32, i32, i32) {
        if c.path_i >= c.path_n() {
            return (-1, -1, -1);
        }
        let n = c.path_n();
        let mut k0 = None;
        let start = c
            .path_i
            .saturating_sub(2)
            .min(if c.path_i > 1 { c.path_i - 2 } else { 0 });
        let mut k = start;
        while k < n && k < c.path_i + 5 {
            let ox = i32::from(c.path[k].x) - nx;
            let oy = i32::from(c.path[k].y) - ny;
            let lat = ox * dys - oy * dxs;
            if ox * dxs + oy * dys == 0 && (-1..=1).contains(&lat) {
                k0 = Some((k, -ox, -oy));
                break;
            }
            k += 1;
        }
        let Some((k0, ox0, oy0)) = k0 else {
            return (-1, -1, -1);
        };
        let (mut lastx, mut lasty) = (nx, ny);
        let mut k = k0;
        while k < n
            && k < k0 + 12
            && self.is_junction(i32::from(c.path[k].x), i32::from(c.path[k].y), c.layer)
        {
            lastx = i32::from(c.path[k].x);
            lasty = i32::from(c.path[k].y);
            k += 1;
        }
        if k >= n
            || k >= k0 + 12
            || self.is_junction(i32::from(c.path[k].x), i32::from(c.path[k].y), c.layer)
        {
            return (-1, -1, -1);
        }
        let (px, py) = (i32::from(c.path[k].x), i32::from(c.path[k].y));
        let (ex, ey) = (px - lastx, py - lasty);
        let (olx, oly) = (px + ox0, py + oy0);
        let side = if ex == dxs && ey == dys {
            0
        } else if ex == dys && ey == -dxs {
            2
        } else if ex == -dys && ey == dxs {
            1
        } else {
            -1
        };
        (side, olx, oly)
    }

    /// Book the blocks car `idx` will cross through the junction entered at
    /// `(nx, ny)` heading `(dxs, dys)`, and the exit beyond it; or join the
    /// booking of a car ahead taking the same way through.
    /// `force_mode`: 0 follow the route, 1 straight on, 2 turn left, 3 turn
    /// right. `join_only`: succeed only by joining.
    #[allow(clippy::too_many_arguments)]
    fn claim_route(
        &mut self,
        map: &SimMap,
        cars: &[CarInfo],
        idx: usize,
        nx: i32,
        ny: i32,
        dxs: i32,
        dys: i32,
        force_mode: i32,
        join_only: bool,
        why: &mut ClaimWhy,
    ) -> bool {
        let force_straight = force_mode == 1;
        let (layer, serial) = (self.cars[idx].layer, self.cars[idx].serial);
        let jn = |s: &Self, x: i32, y: i32| s.is_junction(x, y, layer);
        let road_to = |x: i32, y: i32, a: i32| {
            roads::ground_at(map, x, y, layer) == ground::ROAD
                && roads::dir_allowed(map, x, y, layer, a)
        };
        const CAP: usize = 2 * PATH_MAX as usize + 3;
        let mut p: Vec<(i32, i32)> = Vec::with_capacity(CAP);
        let (mut bbx, mut bby, mut bdir) = (-1, -1, 255);
        let (mut cbx, mut cby) = (-1, -1);
        let (mut lx, mut ly) = (-1, -1);
        let (mut exd, mut eyd) = (dxs, dys);
        let mut did_chain = false;
        why.why = 0;

        if force_mode == 2 || force_mode == 3 {
            let side = if force_mode == 2 { 1 } else { 2 };
            exd = if side == 2 { dys } else { -dys };
            eyd = if side == 2 { -dxs } else { dxs };
            let (mut x, mut y) = (nx, ny);
            let mut ok = false;
            while p.len() < 2 * PATH_MAX as usize && jn(self, x, y) {
                p.push((x, y));
                bbx = x;
                bby = y;
                if side == 1 {
                    break;
                }
                x += dxs;
                y += dys;
            }
            if !p.is_empty() {
                x = bbx + exd;
                y = bby + eyd;
                while p.len() < 2 * PATH_MAX as usize + 1 && jn(self, x, y) {
                    p.push((x, y));
                    x += exd;
                    y += eyd;
                }
                if !jn(self, x, y) {
                    let ang3 = step_angle(exd, eyd);
                    if road_to(x, y, ang3) {
                        lx = x;
                        ly = y;
                        bdir = ang3;
                        cbx = bbx - dxs + exd;
                        cby = bby - dys + eyd;
                        ok = true;
                    }
                }
            }
            if !ok {
                why.why = 17;
                return false;
            }
        } else if !force_straight {
            let (mut side, rlx, rly) = {
                let c = &self.cars[idx];
                self.route_exit_side(c, nx, ny, dxs, dys)
            };
            why.side = side;
            if side == 1 || side == 2 {
                // A turn the lane beside us could take instead: go straight
                // on when the way straight on leads to road too.
                let (svx, svy) = if side == 1 { (-dys, dxs) } else { (dys, -dxs) };
                let (ax2, ay2) = (nx - dxs, ny - dys);
                let ang2 = step_angle(dxs, dys);
                if road_to(ax2 + svx, ay2 + svy, ang2) {
                    let (mut wx, mut wy, mut steps2) = (nx, ny, 0);
                    while steps2 < 2 * PATH_MAX && jn(self, wx, wy) {
                        wx += dxs;
                        wy += dys;
                        steps2 += 1;
                    }
                    if road_to(wx, wy, ang2) {
                        side = 0;
                    }
                }
            }
            if (side == 1 || side == 2) && rlx >= 0 {
                let mut ok = false;
                exd = if side == 2 { dys } else { -dys };
                eyd = if side == 2 { -dxs } else { dxs };
                bbx = if dxs != 0 { rlx } else { nx };
                bby = if dxs != 0 { ny } else { rly };
                let (mut x, mut y) = (nx, ny);
                while p.len() < 2 * PATH_MAX as usize && jn(self, x, y) {
                    p.push((x, y));
                    if x == bbx && y == bby {
                        ok = true;
                        break;
                    }
                    x += dxs;
                    y += dys;
                }
                if ok {
                    x = bbx + exd;
                    y = bby + eyd;
                    while p.len() < 2 * PATH_MAX as usize + 1
                        && !(x == rlx && y == rly)
                        && jn(self, x, y)
                    {
                        p.push((x, y));
                        x += exd;
                        y += eyd;
                    }
                    if x == rlx && y == rly {
                        lx = rlx;
                        ly = rly;
                        bdir = step_angle(exd, eyd);
                        cbx = bbx - dxs + exd;
                        cby = bby - dys + eyd;
                    } else {
                        ok = false;
                    }
                }
                if !ok {
                    p.clear();
                    (bbx, bby, bdir) = (-1, -1, 255);
                    (cbx, cby) = (-1, -1);
                    (exd, eyd) = (dxs, dys);
                    (lx, ly) = (-1, -1);
                }
            }
        }
        if p.is_empty() {
            let (mut x, mut y) = (nx, ny);
            while p.len() < 2 * PATH_MAX as usize && jn(self, x, y) {
                p.push((x, y));
                x += dxs;
                y += dys;
            }
            if p.is_empty() {
                return true;
            }
            if !jn(self, x, y) {
                lx = x;
                ly = y;
            }
            let ang = step_angle(dxs, dys);
            if lx >= 0 {
                if !road_to(lx, ly, ang) {
                    why.why = 17;
                    return false;
                }
            } else if force_straight {
                why.why = 19;
                return false;
            }
        }
        if (0..256).contains(&cbx) && (0..256).contains(&cby) {
            p.push((cbx, cby));
        }
        if lx >= 0 {
            let sq = (world_len(&cars[self.cars[idx].model]) + (MIN_GAP >> FP) as i32 + 31) / 32;
            let (mut qx, mut qy) = (lx, ly);
            let mut chained = false;
            let mut q = 0;
            while q < sq {
                if jn(self, qx, qy) {
                    chained = true;
                    break;
                }
                if self.block_full(idx, qx, qy, layer) {
                    why.why = 18;
                    return false;
                }
                qx += exd;
                qy += eyd;
                q += 1;
            }
            p.push((lx, ly));
            if chained {
                did_chain = true;
                let ang2 = step_angle(exd, eyd);
                for q2 in 1..q {
                    if p.len() >= CAP {
                        why.why = 18;
                        return false;
                    }
                    p.push((lx + q2 * exd, ly + q2 * eyd));
                }
                let (mut hops, mut done2) = (0, false);
                while hops < 2 && !done2 {
                    let mut steps2 = 0;
                    while steps2 < 2 * PATH_MAX && jn(self, qx, qy) {
                        if p.len() >= CAP {
                            why.why = 18;
                            return false;
                        }
                        p.push((qx, qy));
                        qx += exd;
                        qy += eyd;
                        steps2 += 1;
                    }
                    if jn(self, qx, qy) {
                        why.why = 18;
                        return false;
                    }
                    if !road_to(qx, qy, ang2) {
                        why.why = 17;
                        return false;
                    }
                    let mut gap2 = 0;
                    while gap2 < sq && !jn(self, qx + gap2 * exd, qy + gap2 * eyd) {
                        gap2 += 1;
                    }
                    if gap2 >= sq {
                        for r2 in 0..sq {
                            if self.block_full(idx, qx + r2 * exd, qy + r2 * eyd, layer) {
                                why.why = 18;
                                return false;
                            }
                        }
                        if p.len() >= CAP {
                            why.why = 18;
                            return false;
                        }
                        p.push((qx, qy));
                        done2 = true;
                    } else {
                        for _ in 0..gap2 {
                            if p.len() >= CAP {
                                why.why = 18;
                                return false;
                            }
                            p.push((qx, qy));
                            qx += exd;
                            qy += eyd;
                        }
                        hops += 1;
                    }
                }
                if !done2 {
                    why.why = 18;
                    return false;
                }
            }
        }

        // Someone else's bookings on our blocks: we may join one convoy.
        let mut joinid = 0u64;
        for &(x, y) in &p {
            for k in &self.claims[..self.claim_top] {
                if k.ttl > 0
                    && k.x == x as u8
                    && k.y == y as u8
                    && k.z == layer as i8
                    && k.car != serial
                {
                    if joinid == 0 {
                        joinid = k.car;
                    } else if k.car != joinid {
                        why.why = 15;
                        return false;
                    }
                }
            }
        }
        if joinid != 0 && did_chain {
            why.why = 15;
            return false;
        }
        let mut jtail_crossing = false;
        if joinid != 0 {
            let Some(jt) = self.cars[..self.n]
                .iter()
                .find(|c| !c.done && c.serial == joinid)
            else {
                why.why = 15;
                return false;
            };
            if lx < 0
                || jt.book_lx != lx
                || jt.book_ly != ly
                || jt.bend_bx != bbx
                || jt.bend_by != bby
                || jt.bend_dir != bdir
            {
                why.why = 15;
                return false;
            }
            jtail_crossing = jt.crossing;
            let my_angle = self.cars[idx].angle;
            for (i, w) in self.cars[..self.n].iter().enumerate() {
                if i == idx || w.done || w.layer != layer {
                    continue;
                }
                if w.serial == joinid || w.convoy == joinid {
                    continue;
                }
                if w.speed > 0 || w.hold != hold::BOX || w.angle == my_angle {
                    continue;
                }
                let (wbx, wby) = w.block();
                if (wbx - nx).abs() + (wby - ny).abs() <= 8 {
                    why.why = 15;
                    return false;
                }
            }
        } else if join_only {
            return false;
        }
        for &(x, y) in &p {
            if self.body_on_sq(idx, x, y, layer, true, joinid)
                || (!jn(self, x, y) && self.body_on_sq(idx, x, y, layer, false, joinid))
            {
                why.why = 16;
                return false;
            }
        }
        for &(x, y) in &p {
            self.junction_claim(x, y, layer, serial);
        }
        if joinid != 0 {
            if jtail_crossing {
                for k in &mut self.claims[..self.claim_top] {
                    if k.ttl > 0 && k.car == joinid {
                        k.car = serial;
                        k.seen = false;
                    }
                }
                for c in &mut self.cars[..self.n] {
                    if !c.done && (c.serial == joinid || c.convoy == joinid) {
                        c.convoy = serial;
                    }
                }
            } else {
                for k in &mut self.claims[..self.claim_top] {
                    if k.ttl <= 0 || k.car != joinid {
                        continue;
                    }
                    if p.iter()
                        .any(|&(x, y)| k.x == x as u8 && k.y == y as u8 && k.z == layer as i8)
                    {
                        k.car = serial;
                        k.seen = false;
                    }
                }
            }
        }
        let c = &mut self.cars[idx];
        c.convoy = serial;
        c.book_lx = lx;
        c.book_ly = ly;
        c.bend_bx = bbx;
        c.bend_by = bby;
        c.bend_dir = bdir;
        true
    }

    /// One car's tick (AmiGTA1 `drive_one`).
    pub(super) fn drive_one(&mut self, map: &SimMap, cars: &[CarInfo], idx: usize) {
        let info = &cars[self.cars[idx].model];
        let len = world_len(info);

        if self.cars[idx].knock > 0 {
            self.knock_step(map, cars, idx);
            let c = &mut self.cars[idx];
            if c.abandoned && c.knock == 0 {
                c.speed = 0;
                c.recover = 0;
                c.angle = c.face;
            }
            self.car_layer_track(map, idx);
            return;
        }
        if self.cars[idx].abandoned {
            self.car_layer_track(map, idx);
            return;
        }
        if self.cars[idx].done {
            return;
        }
        let (bx, by) = self.lane_block(map, idx);
        // Pulled over: held at its speed by the kerb, which is not a jam, so
        // no waiting counts; a turn, a lane change or a booking ends it.
        if self.cars[idx].pull > 0 {
            let c = &mut self.cars[idx];
            c.pull = if c.turn != 0 || c.swap != 0 || c.crossing {
                0
            } else {
                c.pull - 1
            };
            c.wait = 0;
        }
        if self.cars[idx].cop == 3 || self.cars[idx].cop == 4 {
            let c = &mut self.cars[idx];
            c.speed = 0;
            c.hold = hold::QUEUE;
            return;
        }
        if (self.cars[idx].cop == 2 || self.cars[idx].hunt != 0) && self.pl_active {
            let pbx = (self.pl_x >> BS) as i32;
            let pby = (self.pl_y >> BS) as i32;
            let md = (bx - pbx).abs() + (by - pby).abs();
            let c = &mut self.cars[idx];
            if c.cop_re > 0 {
                c.cop_re -= 1;
            }
            if c.uturn_cool > 0 {
                c.uturn_cool -= 1;
            }
            let drift = (pbx - c.cop_dest_x).abs() + (pby - c.cop_dest_y).abs();
            if c.cop_re == 0 && (drift >= 2 || c.path_i >= c.path_n() || md <= COP_ROUTE_REACH) {
                c.want_route = true;
                c.cop_re = if md <= 8 {
                    8
                } else if md <= COP_ROUTE_REACH {
                    15
                } else {
                    COP_REROUTE
                };
            }
            let (angle, layer, turn, swap, uturn_cool) =
                (c.angle, c.layer, c.turn, c.swap, c.uturn_cool);
            if turn == 0 && swap == 0 && uturn_cool == 0 && layer == self.pl_layer {
                let (fdx, fdy) = heading_step(angle);
                let behind = (bx - pbx) * fdx + (by - pby) * fdy;
                let side = ((pbx - bx) * fdy - (pby - by) * fdx).abs();
                if (1..=10).contains(&behind) && side <= 2 && self.uturn_try(map, idx, bx, by) {
                    self.cars[idx].uturn_cool = 75;
                    self.cars[idx].cop_re = 0;
                    self.stat_cop_uturns += 1;
                }
            }
            let stop_foot = !self.wanted_in_car && md <= COP_NEAR_FOOT;
            let stop_car = self.wanted_in_car
                && md <= COP_STOP_BLOCKS
                && (self.pl_speed < 2 * SPEED_UNIT || self.pl_still >= PL_STILL_TICKS);
            let c = &mut self.cars[idx];
            if c.cop == 2 && c.hunt == 0 && c.layer == self.pl_layer && (stop_foot || stop_car) {
                if c.speed > c.brake {
                    c.speed -= c.brake;
                } else {
                    c.speed = 0;
                }
                c.hold = hold::QUEUE;
                c.cop_wait += 1;
                if c.cop_wait >= COP_OUT_TICKS && self.cop_out_req.is_none() {
                    let ox = c.x - i64::from(cos(c.face)) * 18 * 4;
                    let oy = c.y - i64::from(sin(c.face)) * 18 * 4;
                    self.cop_out_req = Some((ox, oy, c.layer, c.face));
                    self.cars[idx].cop = 3;
                    self.cars[idx].abandoned = true;
                }
                return;
            }
            if c.hunt == 1 && c.layer == self.pl_layer && md <= 3 {
                c.speed = 0;
                c.hold = hold::QUEUE;
                c.arrived = true;
                return;
            }
            c.cop_wait = 0;
        }
        if self.cars[idx].mdest_on {
            let ax = (bx - i32::from(self.cars[idx].mdest_x)).abs();
            let ay = (by - i32::from(self.cars[idx].mdest_y)).abs();
            if ax <= 1 && ay <= 1 {
                Self::mission_arrive(&mut self.cars[idx]);
                return;
            }
        }
        if self.cars[idx].reverse > 0 {
            self.cars[idx].reverse -= 1;
            let (rdx, rdy) = heading_step((self.cars[idx].angle + 128) & 255);
            let step = SPEED_REVERSE * SPEED_UNIT / 2;
            if self.back_out_clear(map, idx, bx, by) {
                self.cars[idx].x += i64::from(rdx) * step;
                self.cars[idx].y += i64::from(rdy) * step;
            } else {
                self.cars[idx].reverse = 0;
            }
            let c = &mut self.cars[idx];
            c.speed = 0;
            c.hold = hold::STUCK;
            c.wait = 0;
            return;
        }

        // Stuck: back off a head-to-head, jump out of sight, back out, or
        // give up.
        let (from_bx, from_by) = (self.cars[idx].last_bx, self.cars[idx].last_by);
        let (wait, cop, reverse) = (
            self.cars[idx].wait,
            self.cars[idx].cop,
            self.cars[idx].reverse,
        );
        if bx != from_bx || by != from_by {
            let c = &mut self.cars[idx];
            c.last_bx = bx;
            c.last_by = by;
            c.wait = 0;
        } else if wait > HEADON
            && reverse == 0
            && cop != 2
            && self.headon_yield(idx)
            && self.back_out_clear(map, idx, bx, by)
        {
            let c = &mut self.cars[idx];
            c.reverse = REVERSE;
            c.wait = 0;
            c.turn = 0;
            c.hold = hold::STUCK;
            return;
        } else if wait > ESCAPE
            && self.cars[idx].mdest_on
            && !self.in_view(bx, by)
            && self.cars[idx].path_i < self.cars[idx].path_n()
        {
            let mut jump = self.cars[idx].path_i;
            while jump + 1 < self.cars[idx].path_n() && jump < self.cars[idx].path_i + 6 {
                let p = self.cars[idx].path[jump + 1];
                if self.in_view(i32::from(p.x), i32::from(p.y)) {
                    break;
                }
                jump += 1;
            }
            let (nx, ny) = (
                i32::from(self.cars[idx].path[jump].x),
                i32::from(self.cars[idx].path[jump].y),
            );
            let ang = if jump + 1 < self.cars[idx].path_n() {
                let np = self.cars[idx].path[jump + 1];
                step_angle(i32::from(np.x) - nx, i32::from(np.y) - ny)
            } else {
                self.cars[idx].angle
            };
            let c = &mut self.cars[idx];
            c.x = i64::from(nx * 32 + LANE_TARGET) << FP;
            c.y = i64::from(ny * 32 + LANE_TARGET) << FP;
            c.angle = ang;
            c.face = ang;
            c.turn = 0;
            c.turn_lock = 0;
            c.path_i = jump;
            c.wait = 0;
            c.last_bx = nx;
            c.last_by = ny;
            return;
        } else if wait > ESCAPE
            && cop != 2
            && !self.cars[idx].abandoned
            && !self.in_view(bx, by)
            && (self.escape_jump(map, cars, idx, bx, by)
                || self.escape_swap(cars, idx)
                || self.escape_hop(map, cars, idx, bx, by))
        {
            return;
        } else if wait > (if cop == 2 { STUCK } else { ABANDON })
            && (cop == 2 || self.in_view(bx, by))
            && reverse == 0
            && self.back_out_clear(map, idx, bx, by)
        {
            let c = &mut self.cars[idx];
            c.reverse = REVERSE;
            c.wait = 0;
            c.turn = 0;
            c.hold = hold::STUCK;
            return;
        } else if wait > ABANDON && !self.in_view(bx, by) {
            self.cars[idx].done = true;
            return;
        }

        self.car_layer_follow(map, idx, bx, by, from_bx, from_by, true);
        let layer = self.cars[idx].layer;

        if self.cars[idx].turn == 0 && !self.is_junction(bx, by, layer) {
            self.cars[idx].turn_lock = 0;
        }

        // A booking ends when the car leaves the blocks it holds. (AmiGTA1
        // reads its heading here before setting it; its oracle is built
        // with that zeroed, which leaves only the first test.)
        if self.cars[idx].crossing {
            let (dxs, dys) = (0, 0);
            let convoy = self.cars[idx].convoy;
            let mut held = bx == self.cars[idx].book_ax && by == self.cars[idx].book_ay;
            if !held && self.is_junction(bx, by, layer) {
                held = self.claims[..self.claim_top].iter().any(|k| {
                    k.ttl > 0 && k.car == convoy && i32::from(k.x) == bx && i32::from(k.y) == by
                });
            }
            if !held
                && !self.is_junction(bx, by, layer)
                && (self.is_junction(bx + dxs, by + dys, layer)
                    || self.is_junction(bx + 2 * dxs, by + 2 * dys, layer))
                && self.claims[..self.claim_top]
                    .iter()
                    .any(|k| k.ttl > 0 && k.car == convoy)
            {
                self.cars[idx].book_ax = bx;
                self.cars[idx].book_ay = by;
                held = true;
            }
            if !held {
                let serial = self.cars[idx].serial;
                if !self.convoy_riding(serial) {
                    for k in &mut self.claims[..self.claim_top] {
                        if k.ttl > 0 && k.car == serial && !k.seen {
                            k.ttl = 0;
                        }
                    }
                }
                self.cars[idx].crossing = false;
            }
        }

        if !self.is_junction(bx, by, layer) {
            self.cars[idx].allow_turn = true;
        }
        if self.cars[idx].turn == 0 {
            let c = &mut self.cars[idx];
            c.lane_bx = bx;
            c.lane_by = by;
            if let Some(ad) = roads::raw_dirs(map, bx, by, layer)
                && ad != 0
                && ad & (ad - 1) == 0
            {
                c.appr_bx = bx;
                c.appr_by = by;
            }
        }

        let mut move_face: i32 = -1;
        // Through the arc of a turn.
        if self.cars[idx].turn != 0 {
            let c = &mut self.cars[idx];
            let arc_s_was = c.arc_s;
            c.arc_s = (c.arc_s + c.speed).min(c.arc_len);
            let phi = (((c.arc_s / i64::from(c.turn_radius)) * ARC_K) >> ARC_KSHIFT)
                .min(i64::from(TURN_QUARTER) << FP);
            let a_fp = (i64::from(c.turn_from) << FP) + i64::from(c.turn) * phi;
            let rd_fp = a_fp - i64::from(c.turn) * (i64::from(TURN_QUARTER) << FP);
            c.face = ((a_fp >> FP) & 255) as i32;
            c.x = c.arc_cx + ((i64::from(c.turn_radius) * sin_fp(rd_fp)) << 2);
            c.y = c.arc_cy - ((i64::from(c.turn_radius) * cos_fp(rd_fp)) << 2);
            c.turn_accum = (phi >> FP) as i32;
            move_face = -2;
            if c.arc_s >= c.arc_len && c.arc_s != arc_s_was {
                c.face = (c.turn_from
                    + if c.turn > 0 {
                        TURN_QUARTER
                    } else {
                        -TURN_QUARTER
                    })
                    & 255;
                let line = c.arc_line;
                let p = if c.face & 127 == 64 {
                    &mut c.y
                } else {
                    &mut c.x
                };
                if (*p - line).abs() <= (1 << FP) {
                    *p = line;
                }
                c.turn_accum = TURN_QUARTER;
                c.lane_target = c.turn_aim_tgt;
                // AmiGTA1 sets the lane from `angle`, still the old road
                // direction at this point.
                c.lane_axis = i32::from(c.angle & 127 == 64);
                c.lane_off = i64::from(
                    (if c.lane_axis != 0 {
                        c.turn_aim_by
                    } else {
                        c.turn_aim_bx
                    }) * 32
                        + c.turn_aim_tgt,
                ) << FP;
                c.lane_dir = c.angle;
                c.lane_set = true;
            }
            if c.turn_accum >= TURN_QUARTER && ((c.face + 32) & 0xC0) != c.turn_from {
                c.face = (c.turn_from + if c.turn > 0 { 64 } else { -64 }) & 255;
                c.turn = 0;
                c.turn_accum = 0;
                c.since_turn = 0;
            }
        }
        {
            let c = &mut self.cars[idx];
            if c.since_turn < AFTER_TURN {
                c.since_turn += 1;
            }
            c.angle = (c.face + 32) & 0xC0;
        }
        let (mut dxs, mut dys) = heading_step(self.cars[idx].angle);

        if !self.is_junction(bx, by, layer) {
            let a = self.cars[idx].angle;
            self.cars[idx].lane_target = roads::lane_target(map, &self.boxes, bx, by, layer, a);
            if self.cars[idx].swap == 0 {
                self.line_from_block(map, idx, bx, by, a);
            }
        }
        {
            let c = &mut self.cars[idx];
            if c.turn_want && (bx != c.turn_want_bx || by != c.turn_want_by) {
                c.turn_want = false;
            }
        }

        let mut want_dir = Self::route_dir(&mut self.cars[idx], bx, by);
        if (self.cars[idx].cop == 2 || self.cars[idx].hunt != 0) && self.pl_active {
            let pbx = (self.pl_x >> BS) as i32;
            let pby = (self.pl_y >> BS) as i32;
            let md = (bx - pbx).abs() + (by - pby).abs();
            if md <= COP_ARROWS_NEAR && !self.cars[idx].crossing {
                self.goal_on = true;
                self.goal_x = pbx;
                self.goal_y = pby;
                let (layer, angle) = (self.cars[idx].layer, self.cars[idx].angle);
                let gd = self.choose_heading(map, bx, by, layer, angle, len);
                self.goal_on = false;
                if gd >= 0 {
                    want_dir = gd;
                }
            }
        }
        {
            let c = &self.cars[idx];
            if c.crossing {
                want_dir = if c.bend_dir != 255 && bx == c.bend_bx && by == c.bend_by {
                    c.bend_dir
                } else {
                    c.angle
                };
            }
        }
        {
            let c = &mut self.cars[idx];
            // Out of route: ask for another (AmiGTA1's refill margin is 0).
            if c.path_i >= c.path_n() {
                c.want_route = true;
            }
            if want_dir >= 0 && want_dir == ((c.angle + 128) & 255) {
                c.clear_path();
                want_dir = -1;
            }
        }
        if want_dir >= 0 {
            let (tdx, tdy) = heading_step(want_dir);
            if !roads::fits_entering(map, bx + tdx, by + tdy, layer, want_dir, len) {
                self.cars[idx].clear_path();
                want_dir = -1;
            }
        }
        if want_dir < 0 {
            self.cars[idx].want_route = true;
            let a = self.cars[idx].angle;
            let chasing = (self.cars[idx].cop == 2 || self.cars[idx].hunt != 0) && self.pl_active;
            if chasing {
                self.goal_on = true;
                self.goal_x = (self.pl_x >> BS) as i32;
                self.goal_y = (self.pl_y >> BS) as i32;
            }
            want_dir = self.choose_heading(map, bx, by, layer, a, len);
            self.goal_on = false;
        }
        if want_dir < 0 {
            // Nowhere to go: a long vehicle in a street it cannot leave.
            // It gives up once it is out of sight (ours: AmiGTA1 deletes it
            // in view too, after six seconds, and the pop-in check saw a
            // bus vanish; the oracle carries the change, traffic_giveup.pl).
            let in_view = self.in_view(bx, by);
            let c = &mut self.cars[idx];
            c.speed = 0;
            c.hold = hold::DEADEND;
            c.wait += 1;
            if c.wait > GIVEUP && !in_view {
                c.done = true;
            }
            return;
        }

        let edge = Self::to_block_edge(&self.cars[idx], bx, by);
        let mut ahead = false;
        {
            let c = &self.cars[idx];
            if c.turn == 0 && want_dir == c.angle && !self.is_junction(bx, by, layer) {
                if c.crossing {
                    let (hx2, hy2) = heading_step(c.angle);
                    if c.bend_dir != 255 && bx + hx2 == c.bend_bx && by + hy2 == c.bend_by {
                        want_dir = c.bend_dir;
                        ahead = true;
                    }
                } else if c.path_i + 1 < c.path_n() {
                    let (ax, ay) = (i32::from(c.path[c.path_i].x), i32::from(c.path[c.path_i].y));
                    let (nx2, ny2) = (
                        i32::from(c.path[c.path_i + 1].x),
                        i32::from(c.path[c.path_i + 1].y),
                    );
                    let d2 = match (nx2 - ax, ny2 - ay) {
                        (0, -1) => 0,
                        (1, 0) => 64,
                        (0, 1) => 128,
                        (-1, 0) => 192,
                        _ => -1,
                    };
                    if d2 >= 0 && ((d2 - c.angle) & 255) != 0 && ((d2 - c.angle) & 255) != 128 {
                        want_dir = d2;
                        ahead = true;
                    }
                }
            }
        }

        let mut give_way = false;
        let mut exit_busy = false;
        if self.cars[idx].turn == 0 && want_dir != self.cars[idx].angle {
            let angle = self.cars[idx].angle;
            let d = (want_dir - angle) & 255;
            {
                let c = &mut self.cars[idx];
                c.turn_want = true;
                c.turn_want_bx = bx;
                c.turn_want_by = by;
                c.turn_want_from = angle;
            }
            if d == 128 {
                if self.is_junction(bx, by, layer)
                    || self.cars[idx].turn_lock != 0
                    || !roads::dir_allowed(map, bx, by, layer, want_dir)
                {
                    want_dir = angle;
                } else {
                    self.line_from_block(map, idx, bx, by, want_dir);
                    let c = &mut self.cars[idx];
                    c.swap = 1;
                    c.swap_ticks = 0;
                    c.swap_bx = bx;
                    c.swap_by = by;
                    c.swap_sdx = 0;
                    c.swap_sdy = 0;
                    c.turn_lock = 0;
                    (dxs, dys) = heading_step(want_dir);
                }
            } else {
                let (twx, twy) = heading_step(want_dir);
                // The lane line of the street the turn leads into.
                let (abx, aby) = (
                    bx + if ahead { dxs } else { 0 },
                    by + if ahead { dys } else { 0 },
                );
                let mut k = 1;
                while k < 4 && self.is_junction(abx + k * twx, aby + k * twy, layer) {
                    k += 1;
                }
                let (gx, gy) = (abx + k * twx, aby + k * twy);
                let tgt = roads::lane_target(map, &self.boxes, gx, gy, layer, want_dir);
                let line = if angle & 127 == 0 {
                    i64::from((aby + twy) * 32 + tgt) << FP
                } else {
                    i64::from((abx + twx) * 32 + tgt) << FP
                };
                let (cx_, cy_) = (self.cars[idx].x, self.cars[idx].y);
                let dist = match angle {
                    0 => cy_ - line,
                    128 => line - cy_,
                    192 => cx_ - line,
                    _ => line - cx_,
                };
                let mut ready = dist <= (i64::from(TURN_RADIUS) << FP);
                if ready && roads::ground_at(map, gx, gy, layer) != ground::ROAD {
                    ready = false;
                }
                let dist_look = dist;
                let aim_r = ((dist >> FP) as i32).max(TURN_RADIUS_MIN);
                let (aim_tgt, aim_line) = (tgt, line);
                // The street the turn leads into: beside the turning block,
                // which for a turn begun a block early is the next one.
                // Ours: AmiGTA1 looks beside the car's own block, the far
                // lane of the crossing street, and refused corners for cars
                // coming the other way (traffic_corner.pl).
                let (nx, ny) = (abx + twx, aby + twy);
                {
                    let c = &self.cars[idx];
                    if ready && c.turn_lock != 0 && c.path_i >= c.path_n() {
                        ready = false;
                    }
                    if ready && !c.allow_turn {
                        ready = false;
                    }
                }
                // A lane beside us that also goes our way should make the
                // turn instead.
                if ready {
                    let c = &self.cars[idx];
                    let side = (angle + if d == 64 { TURN_QUARTER } else { -TURN_QUARTER }) & 255;
                    let (sdx, sdy) = heading_step(side);
                    if roads::ground_at(map, c.appr_bx + sdx, c.appr_by + sdy, layer)
                        == ground::ROAD
                        && roads::dir_allowed(map, c.appr_bx + sdx, c.appr_by + sdy, layer, angle)
                    {
                        ready = false;
                    }
                }
                if ready {
                    let slack = if self.cars[idx].wait > PATIENCE {
                        0
                    } else {
                        MIN_GAP
                    };
                    exit_busy = !self.room_for(
                        cars,
                        Some(idx),
                        i64::from(nx * 32 + 16) << FP,
                        i64::from(ny * 32 + 16) << FP,
                        layer,
                        want_dir,
                        len,
                        slack,
                    );
                    // Ours: where the corner leaves no way straight on, a
                    // car refused its turn waits for it at the corner rather
                    // than running on off the road, and for as long as the
                    // street round the corner is full, not AmiGTA1's half
                    // second before forcing its way in (decision log,
                    // 2026-11-21; traffic_corner.pl). AmiGTA1 has only a car
                    // on a long route wait.
                    let (cx, cy) = (abx, aby);
                    let on = roads::dir_allowed(map, cx, cy, layer, angle)
                        && roads::drivable(map, cx + dxs, cy + dys, layer);
                    if exit_busy && (self.cars[idx].wait <= SWEEP_PATIENCE || !on) {
                        ready = false;
                        if !on {
                            give_way = true;
                        }
                    }
                }
                {
                    let c = &self.cars[idx];
                    if c.crossing
                        && c.bend_dir != 255
                        && bx == c.bend_bx
                        && by == c.bend_by
                        && want_dir == c.bend_dir
                        && dist_look <= (i64::from(TURN_RADIUS) << FP)
                    {
                        ready = true;
                        give_way = false;
                    }
                }
                {
                    let c = &self.cars[idx];
                    let dsq = wrap24((i64::from(c.angle) << FP) - c.face16);
                    // Ours: at a corner with no way on, a turn the car must
                    // make starts though it is not yet straight; nothing
                    // straightens it inside a junction box, so waiting for
                    // that sent it out of the far side (decision log,
                    // 2026-11-21; traffic_fits.pl).
                    let way_on = roads::dir_allowed(map, abx, aby, layer, angle)
                        && roads::drivable(map, abx + dxs, aby + dys, layer);
                    if !(-(STEER_SLOW << FP)..=(STEER_SLOW << FP)).contains(&dsq) && way_on {
                        ready = false;
                    }
                }
                if ready {
                    let lane_target = self.cars[idx].lane_target;
                    let c = &mut self.cars[idx];
                    c.turn = if d == 64 { 1 } else { -1 };
                    c.turn_from = c.angle;
                    c.turn_accum = 0;
                    c.turn_lock = c.turn;
                    c.turn_aim_tgt = aim_tgt;
                    c.turn_aim_bx = nx;
                    c.turn_aim_by = ny;
                    c.turn_radius = aim_r;
                    {
                        let p = if c.angle & 127 == 64 {
                            &mut c.y
                        } else {
                            &mut c.x
                        };
                        let ctr = (((*p >> BS) * 32) + i64::from(lane_target)) << FP;
                        let d2 = *p - ctr;
                        if (-(8 << FP)..=(8 << FP)).contains(&d2) {
                            *p = ctr;
                        }
                    }
                    let side = (c.angle + if d == 64 { TURN_QUARTER } else { -TURN_QUARTER }) & 255;
                    c.arc_cx = c.x + ((i64::from(aim_r) * i64::from(sin(side))) << 2);
                    c.arc_cy = c.y - ((i64::from(aim_r) * i64::from(cos(side))) << 2);
                    c.arc_len = i64::from(aim_r) * ARC_QLEN;
                    c.arc_s = 0;
                    c.arc_line = aim_line;
                    let (acx, acy, afrom, aturn) = (c.arc_cx, c.arc_cy, c.angle, c.turn);
                    self.arc_reserve(idx, acx, acy, aim_r, afrom, aturn, len > LONG_CAR, true, 0);
                    let c = &mut self.cars[idx];
                    c.allow_turn = false;
                    c.turn_free_at = c.odo + TURN_AGAIN;
                    c.turn_step = (c.speed >> (FP - 1)) as i32;
                    let cap = (if exit_busy {
                        SPEED_TURN_TIGHT
                    } else {
                        SPEED_TURN_WIDE
                    }) * SPEED_UNIT;
                    if c.speed > cap {
                        c.speed = cap;
                    }
                }
            }
        }

        let (nx, ny) = (bx + dxs, by + dys);
        let clear = Self::road_clear(map, &self.cars[idx], bx, by, layer, want_dir, 10);
        let (mut gap, lead, lead_i) = self.gap_ahead(cars, idx);
        {
            let lead_kind = if gap < 0 {
                0
            } else if lead_i < 0 {
                if lead == 0 { 1 } else { 3 }
            } else if self.cars[lead_i as usize].speed == 0 && self.cars[lead_i as usize].knock == 0
            {
                2
            } else {
                3
            };
            let c = &mut self.cars[idx];
            c.lead_idx = if gap >= 0 && lead_i >= 0 { lead_i } else { -1 };
            c.lead_kind = lead_kind;
        }
        self.honk(idx, gap);
        // Inside our own booking, a car ahead that is not on it does not
        // hold us up.
        if self.cars[idx].crossing && gap >= 0 && lead_i >= 0 {
            let lo = &self.cars[lead_i as usize];
            let (obx, oby) = lo.block();
            let convoy = self.cars[idx].convoy;
            let held2 = self.claims[..self.claim_top].iter().any(|k| {
                k.ttl > 0
                    && k.car == convoy
                    && i32::from(k.x) == obx
                    && i32::from(k.y) == oby
                    && i32::from(k.z) == lo.layer
            });
            if !held2 {
                gap = -1;
            }
        }

        // The speed it wants.
        let c = &self.cars[idx];
        let mut want = c.top;
        let mut new_hold = None;
        if c.turn != 0 {
            want = (if clear >= CLEAR_EASE {
                SPEED_TURN_WIDE
            } else {
                SPEED_TURN_TIGHT
            }) * SPEED_UNIT;
            want = want.min(c.top);
        }
        if clear < CLEAR_EASE && want > 4 * SPEED_UNIT {
            want = 4 * SPEED_UNIT;
        }
        if clear < CLEAR_HARD && want > 2 * SPEED_UNIT {
            want = 2 * SPEED_UNIT;
        }
        if clear < CLEAR_STOP {
            want = 0;
            new_hold = Some(hold::ROAD);
        }
        if give_way {
            want = 0;
            new_hold = Some(hold::MERGE);
        }
        if gap >= 0 {
            if gap <= MIN_GAP {
                want = 0;
                new_hold = Some(hold::GAP);
            } else if gap < FOLLOW_GAP {
                let room = (gap - MIN_GAP) >> FP;
                let base = lead.min(want);
                let eased = (base * room) >> GAP_SHIFT;
                want = want.min(eased);
            }
        }
        // Pulled over: on along the kerb line at the speed it had, its
        // throttle cut (`GTA8.EXE` `0x1e3a5`): no faster, and no stop of its
        // own; what is in its way still stops it.
        if c.pull > 0 {
            want = want.min(c.speed.max(0));
        }
        if let Some(h) = new_hold {
            self.cars[idx].hold = h;
        }

        // The junction ahead: a red light, or the gate into the box.
        let (mut jbx, mut jby, mut ej) = (-1, -1, 0i64);
        if self.is_junction(nx, ny, layer) {
            (jbx, jby, ej) = (nx, ny, edge);
        } else if self.is_junction(nx + dxs, ny + dys, layer) {
            (jbx, jby, ej) = (nx + dxs, ny + dys, edge + (32 << FP));
        }
        let mut stopr = BOX_LOOK + (i64::from(len / 2) << FP);
        {
            let c = &self.cars[idx];
            if c.brake > 0 {
                stopr += (c.speed * (c.speed / c.brake)) >> 1;
            }
        }
        self.cars[idx].at_light = false;
        if self.cars[idx].hint_bx != bx || self.cars[idx].hint_by != by {
            let hv = map
                .block(bx, by, layer)
                .map_or(0, |b| b.type_map_ext & 0x07);
            let c = &mut self.cars[idx];
            c.hint_bx = bx;
            c.hint_by = by;
            c.hint_val = hv;
        }
        let at_stop_line = {
            let c = &self.cars[idx];
            c.hint_val == 1 && {
                let (dx, dy) = heading_step(c.angle);
                self.is_junction(bx + dx, by + dy, layer)
            }
        };
        let (crossing, cop) = (self.cars[idx].crossing, self.cars[idx].cop);
        let ew = (self.cars[idx].angle & 127) == 64;
        // Ours: the light holds a car once its nose would reach the box, as
        // the box's own gate does, where the block before the box is a stop
        // line. AmiGTA1 asks only when its middle is on the stop line, and a
        // bus is then half a block into the crossing, where the traffic
        // across clips it (decision log, 2026-11-21; traffic_stopline.pl).
        let light_ahead = jbx >= 0
            && ej < stopr
            && map
                .block(jbx - dxs, jby - dys, layer)
                .is_some_and(|b| b.type_map_ext & 0x07 == 1);
        let red = (at_stop_line || light_ahead) && !self.light_green(ew);
        if cop != 2 && !crossing && red {
            want = 0;
            let c = &mut self.cars[idx];
            c.at_light = true;
            c.hold = hold::LIGHT;
        } else if (!crossing
            || (!self.is_junction(bx, by, layer) && !self.claim_mine(&self.cars[idx], jbx, jby)))
            && jbx >= 0
            && ej < stopr
        {
            // AmiGTA1 keeps why a booking failed in globals that each
            // attempt overwrites; one record shared by the attempts.
            let mut cw = ClaimWhy::default();
            if self.claim_route(map, cars, idx, jbx, jby, dxs, dys, 0, true, &mut cw) {
                let c = &mut self.cars[idx];
                c.crossing = true;
                c.book_ax = bx;
                c.book_ay = by;
            } else {
                let ew = (self.cars[idx].angle & 127) == 64;
                let (mut jx, mut jy, mut steps, mut blocked) = (jbx, jby, 0, false);
                while steps < roads::BOX_SCAN && self.is_junction(jx, jy, layer) {
                    if self.box_busy(idx, jx, jy, layer, ew) {
                        blocked = true;
                    }
                    jx += dxs;
                    jy += dys;
                    steps += 1;
                }
                let mut found_exit = !self.is_junction(jx, jy, layer);
                let (mut exx, mut exy) = (jx, jy);
                if !blocked {
                    let c = &self.cars[idx];
                    let mut k = c.path_i;
                    while k < c.path_n() && k < c.path_i + 12 {
                        let (px, py) = (i32::from(c.path[k].x), i32::from(c.path[k].y));
                        if !self.is_junction(px, py, layer) {
                            (exx, exy) = (px, py);
                            found_exit = true;
                            break;
                        }
                        k += 1;
                    }
                }
                if !blocked {
                    let mut who = self.occ_owner(jbx, jby, layer);
                    if who == 0 {
                        // AmiGTA1 scans every slot here, not only the live
                        // ones.
                        if let Some(k) = self.claims.iter().find(|k| {
                            k.ttl > 0 && k.x == jbx as u8 && k.y == jby as u8 && k.z == layer as i8
                        }) {
                            who = k.car;
                        }
                    }
                    if who != 0 && who != self.cars[idx].serial {
                        blocked = true;
                    }
                }
                if !blocked && found_exit && self.block_full(idx, exx, exy, layer) {
                    blocked = true;
                }
                {
                    let (mut dblk, mut jjx, mut jjy) = (0, jbx, jby);
                    while dblk < PATH_MAX && self.is_junction(jjx, jjy, layer) {
                        dblk += 1;
                        jjx += dxs;
                        jjy += dys;
                    }
                    let need = (i64::from(dblk * 32 + len) << FP) + MIN_GAP;
                    if !blocked && gap >= 0 && gap < need {
                        blocked = true;
                    }
                }
                if !blocked
                    && !self.claim_route(map, cars, idx, jbx, jby, dxs, dys, 0, false, &mut cw)
                {
                    let side = {
                        let c = &self.cars[idx];
                        self.route_exit_side(c, jbx, jby, dxs, dys).0
                    };
                    // Straight on instead of a right turn, or either turn
                    // when straight on leads nowhere.
                    if (side == 2
                        && self.claim_route(map, cars, idx, jbx, jby, dxs, dys, 1, false, &mut cw))
                        || (cw.why == 17
                            && (self.claim_route(
                                map, cars, idx, jbx, jby, dxs, dys, 2, false, &mut cw,
                            ) || self.claim_route(
                                map, cars, idx, jbx, jby, dxs, dys, 3, false, &mut cw,
                            )))
                    {
                        let c = &mut self.cars[idx];
                        c.path_i = c.path_n();
                        c.want_route = true;
                    } else {
                        blocked = true;
                        if cw.why == 17 && cw.side < 0 {
                            let c = &mut self.cars[idx];
                            c.clear_path();
                            c.want_route = true;
                        }
                    }
                }
                if self.cars[idx].cop == 2 && blocked && self.cars[idx].wait > 40 {
                    blocked = false;
                    self.stat_cop_box_pushed += 1;
                }
                if blocked {
                    want = 0;
                    self.cars[idx].hold = hold::BOX;
                } else {
                    let c = &mut self.cars[idx];
                    c.crossing = true;
                    c.book_ax = bx;
                    c.book_ay = by;
                }
            }
        }

        // Creep on through a turn rather than stop in it.
        if self.cars[idx].turn != 0
            && want < SPEED_ARC_MIN * SPEED_UNIT
            && (!(0..=MIN_GAP).contains(&gap)
                || (self.cars[idx].wait > ARC_CREEP && self.arc_step_free(cars, idx)))
        {
            want = SPEED_ARC_MIN * SPEED_UNIT;
            let c = &mut self.cars[idx];
            if c.hold == hold::BOX || c.hold == hold::QUEUE {
                c.hold = hold::NONE;
            }
        }
        {
            let c = &mut self.cars[idx];
            if c.crossing
                && (c.hold == hold::NONE || c.hold == hold::QUEUE)
                && want < SPEED_TURN_TIGHT * SPEED_UNIT
            {
                want = SPEED_TURN_TIGHT * SPEED_UNIT;
            }
            if (c.cop == 2 || c.hunt != 0) && self.pl_active && c.layer == self.pl_layer {
                let pbx = (self.pl_x >> BS) as i32;
                let pby = (self.pl_y >> BS) as i32;
                let md = (bx - pbx).abs() + (by - pby).abs();
                if md <= 5 && want < 4 * SPEED_UNIT {
                    want = 4 * SPEED_UNIT;
                    if c.hold == hold::BOX || c.hold == hold::QUEUE || c.hold == hold::GAP {
                        c.hold = hold::NONE;
                    }
                } else if c.hold == hold::BOX && c.wait > 100 && want < 3 * SPEED_UNIT {
                    want = 3 * SPEED_UNIT;
                    c.hold = hold::NONE;
                }
            }
            // Mid lane change, keep rolling.
            if c.swap != 0 {
                let dman = wrap24((i64::from(c.lane_dir) << FP) - c.face16);
                if !(-(STEER_SLOW << FP)..=(STEER_SLOW << FP)).contains(&dman) {
                    want = want.max(SPEED_TURN_TIGHT * SPEED_UNIT);
                    if c.hold == hold::BOX || c.hold == hold::QUEUE || c.hold == hold::GAP {
                        c.hold = hold::NONE;
                    }
                }
            }
            // Speed up or brake towards it.
            if c.speed < want {
                c.speed = (c.speed + c.accel).min(want);
            } else if c.speed > want {
                c.speed = (c.speed - c.brake).max(want);
            }
        }
        if self.cars[idx].speed <= 0 {
            let c = &mut self.cars[idx];
            c.speed = 0;
            if c.hold == hold::NONE {
                c.hold = hold::QUEUE;
            }
            c.wait += 1;
            self.lane_swap_step(map, idx, bx, by);
            return;
        }
        self.cars[idx].hold = hold::NONE;

        // Move: along the arc (done above), or steering along the lane.
        if move_face != -2 {
            if move_face < 0 {
                let c = &mut self.cars[idx];
                if (c.cop == 2 || c.hunt != 0)
                    && self.pl_active
                    && c.layer == self.pl_layer
                    && c.lane_set
                {
                    let (fdx, fdy) = heading_step(c.lane_dir);
                    let along = ((self.pl_x - c.x) >> FP) * i64::from(fdx)
                        + ((self.pl_y - c.y) >> FP) * i64::from(fdy);
                    let pl_cross = if c.lane_axis != 0 {
                        self.pl_y
                    } else {
                        self.pl_x
                    };
                    let my_cross = if c.lane_axis != 0 { c.y } else { c.x };
                    if (0..=5 * 32).contains(&along) && ((pl_cross - my_cross) >> FP).abs() <= 32 {
                        c.lane_off = pl_cross;
                    }
                }
                move_face = if !self.is_junction(bx, by, layer) {
                    Self::steer_along_line(&mut self.cars[idx]).0
                } else {
                    self.cars[idx].face
                };
            }
            let c = &mut self.cars[idx];
            c.x += (i64::from(sin(move_face)) * (c.speed >> 4)) >> 10;
            c.y += -(i64::from(cos(move_face)) * (c.speed >> 4)) >> 10;
        }
        self.lane_swap_step(map, idx, bx, by);

        // On a clear straight at full speed, coast to near the block's end
        // before deciding again.
        self.cars[idx].cruise = 0;
        let c = &self.cars[idx];
        if c.cop == 0
            && !c.mission
            && c.speed > 0
            && c.lane_set
            && c.swap == 0
            && c.turn == 0
            && c.recover == 0
            && !c.crossing
            && c.lane_fix == 0
            && c.lead_kind == 0
            && want == c.top
            && c.speed == c.top
            && c.face == c.lane_dir
            && move_face == c.face
            && map.nav(bx, by, layer) & nav::SLOPE == 0
        {
            let ddx = if c.lane_dir == 64 {
                1
            } else if c.lane_dir == 192 {
                -1
            } else {
                0
            };
            let ddy = if c.lane_dir == 128 {
                1
            } else if c.lane_dir == 0 {
                -1
            } else {
                0
            };
            let ok = (0..=CRUISE_LOOK).all(|k| {
                !self.is_junction(bx + k * ddx, by + k * ddy, layer)
                    && roads::drivable(map, bx + k * ddx, by + k * ddy, layer)
            });
            if ok {
                let pos = if c.lane_axis != 0 { c.x } else { c.y };
                let cell = i64::from(if c.lane_axis != 0 { bx } else { by });
                let mut edge = if ddx + ddy > 0 {
                    ((cell + 1) << BS) - pos
                } else {
                    pos - (cell << BS)
                };
                edge -= i64::from(len / 2) << FP;
                let n = (edge / c.speed - 1).min(CRUISE_MAX);
                if n > 0 {
                    self.cars[idx].cruise = n as i32;
                }
            }
        }
    }

    /// Horns and yells at whatever holds a car up close ahead.
    fn honk(&mut self, idx: usize, gap: i64) {
        let c = &mut self.cars[idx];
        if c.honk > 0 {
            c.honk -= 1;
        }
        if c.cop != 0 || c.abandoned || gap < 0 || c.speed <= 0 {
            return;
        }
        let d = (gap >> BS) as i32;
        let v = (c.speed / SPEED_UNIT) as i32;
        if d <= 3 && ((v > 10 && d < 4) || (v > 4 && d < 2) || (v > 0 && d == 0)) {
            self.honk_rng = self
                .honk_rng
                .wrapping_mul(1_103_515_245)
                .wrapping_add(12345);
            let r = (self.honk_rng >> 16) % 100;
            let c = &mut self.cars[idx];
            if r > 50 {
                c.honk = 40;
                if c.honk_pat == 0 {
                    c.honk_pat = (1 + (self.honk_rng >> 8) % 10) as u8;
                }
            } else if r > 25 && self.yell.is_none() {
                self.yell = Some((c.x, c.y, c.layer));
            }
        }
    }
}
