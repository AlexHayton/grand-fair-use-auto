//! Routes for traffic: a breadth-first search over the navigation grid's
//! direction arrows, inside a 48-block window around the start.
//!
//! Ported from AmiGTA1 (MIT), `gta_route.c`. The search obeys the arrows a
//! block carries, never climbs a ramp from its high side, never turns
//! straight back on itself two steps after arriving, and gives up after 700
//! cells. Three uses:
//!
//! - [`Router::find`]: the way to a given block.
//! - [`Router::find_chase`]: the same, but a target outside the window, or
//!   unreachable, gives the way to the nearest block reached instead.
//! - [`Router::wander`]: a random block reached at a distance in a band,
//!   picked fairly among all those the search touches.
//!
//! The random numbers come from the caller's generator, a 64-bit
//! Numerical Recipes LCG as in AmiGTA1's host build (see [`crate::traffic`]).

use crate::map::{SimMap, nav};

/// Width of the search window, in blocks.
pub const WINDOW: i32 = 48;
/// Most nodes a route holds.
pub const MAX: usize = 40;
/// Cells the search may take from its queue before giving up.
pub const BUDGET: usize = 700;

/// One block of a route.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Node {
    pub x: u8,
    pub y: u8,
}

/// Why the last search failed, for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fail {
    Ok,
    NoStart,
    NoTarget,
    Far,
    Same,
    Budget,
    Exhausted,
}

const W: usize = WINDOW as usize;
// N, S, W, E, as the arrow bits.
const STEP_X: [i32; 4] = [0, 0, -1, 1];
const STEP_Y: [i32; 4] = [-1, 1, 0, 0];
const DIR_BIT: [u8; 4] = [nav::N, nav::S, nav::W, nav::E];
const OPPOSITE: [i32; 4] = [1, 0, 3, 2];
// The angle each step heads in, to compare with a ramp's climb.
const ANG_OF: [i32; 4] = [0, 128, 192, 64];

/// The numbers AmiGTA1's LCG draws: `seed * 1664525 + 1013904223` in 64
/// bits, as its host build keeps `unsigned long`.
#[inline]
pub fn lcg_step(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *seed
}

/// Scratch space for searches, reused between them.
#[derive(Debug, Clone)]
pub struct Router {
    came_from: Vec<u8>,
    queue: Vec<u16>,
    /// Why the last search failed.
    pub last_fail: Fail,
}

impl Default for Router {
    fn default() -> Self {
        Router {
            came_from: vec![0; W * W],
            queue: vec![0; W * W],
            last_fail: Fail::Ok,
        }
    }
}

struct Wander<'a> {
    min: i32,
    max: i32,
    seed: &'a mut u64,
}

impl Router {
    /// The route from `(sx, sy)` to `(tx, ty)` on layer `z`, without its
    /// start block, into `out`; returns its length (0: none). `ban` is an
    /// angle the first step may not take (any other value bans nothing).
    #[allow(clippy::too_many_arguments)]
    pub fn find(
        &mut self,
        map: &SimMap,
        sx: i32,
        sy: i32,
        z: i32,
        tx: i32,
        ty: i32,
        ban: i32,
        out: &mut [Node],
    ) -> usize {
        self.bfs(map, sx, sy, z, tx, ty, ban, out, false, None)
    }

    /// As [`find`](Self::find), but settles for the nearest block reached
    /// when the target is out of reach, and may turn back on itself.
    #[allow(clippy::too_many_arguments)]
    pub fn find_chase(
        &mut self,
        map: &SimMap,
        sx: i32,
        sy: i32,
        z: i32,
        tx: i32,
        ty: i32,
        ban: i32,
        out: &mut [Node],
    ) -> usize {
        self.bfs(map, sx, sy, z, tx, ty, ban, out, true, None)
    }

    /// A route to a random block between `min` and `max` blocks away
    /// (Manhattan distance), drawing from `seed`.
    #[allow(clippy::too_many_arguments)]
    pub fn wander(
        &mut self,
        map: &SimMap,
        sx: i32,
        sy: i32,
        z: i32,
        ban: i32,
        min: i32,
        max: i32,
        seed: &mut u64,
        out: &mut [Node],
    ) -> usize {
        let w = Wander { min, max, seed };
        self.bfs(map, sx, sy, z, 0, 0, ban, out, false, Some(w))
    }

    #[allow(clippy::too_many_arguments)]
    fn bfs(
        &mut self,
        map: &SimMap,
        sx: i32,
        sy: i32,
        z: i32,
        mut tx: i32,
        mut ty: i32,
        ban: i32,
        out: &mut [Node],
        allow_reverse: bool,
        mut wander: Option<Wander>,
    ) -> usize {
        let half = WINDOW / 2;
        let (ox, oy) = (sx - half, sy - half);
        let max = out.len();
        let idx = |x: i32, y: i32| ((y - oy) as usize) * W + (x - ox) as usize;

        self.last_fail = Fail::NoStart;
        if max == 0 || nav::dirs(map.nav(sx, sy, z)) == 0 {
            return 0;
        }
        let mut best_d = 0x7fff;
        let (mut bpx, mut bpy) = (-1, -1);
        if wander.is_none() {
            self.last_fail = Fail::NoTarget;
            if !allow_reverse {
                if nav::dirs(map.nav(tx, ty, z)) == 0 {
                    return 0;
                }
                self.last_fail = Fail::Far;
                if tx - ox < 0 || tx - ox >= WINDOW || ty - oy < 0 || ty - oy >= WINDOW {
                    return 0;
                }
            }
            self.last_fail = Fail::Same;
            if sx == tx && sy == ty {
                return 0;
            }
            best_d = (sx - tx).abs() + (sy - ty).abs();
        }
        let (lox, loy) = (ox.max(0), oy.max(0));
        let (hix, hiy) = ((ox + WINDOW).min(256), (oy + WINDOW).min(256));
        let ban_i: i32 = match ban {
            0 => 0,
            128 => 1,
            192 => 2,
            64 => 3,
            _ => -1,
        };

        self.came_from.fill(0);
        self.came_from[idx(sx, sy)] = 0xff;
        let (mut head, mut tail) = (0usize, 0usize);
        self.queue[tail] = (((sy - oy) << 8) | (sx - ox)) as u16;
        tail += 1;
        let (mut wpx, mut wpy) = (-1, -1);
        let mut wseen: u64 = 0;

        let found = 'search: {
            while head < tail {
                let cell = self.queue[head] as i32;
                head += 1;
                if head > BUDGET {
                    if wander.is_some() && wpx >= 0 {
                        (tx, ty) = (wpx, wpy);
                        break 'search true;
                    }
                    if wander.is_none() && allow_reverse && bpx >= 0 {
                        (tx, ty) = (bpx, bpy);
                        break 'search true;
                    }
                    self.last_fail = Fail::Budget;
                    return 0;
                }
                let cx = ox + (cell & 0xff);
                let cy = oy + (cell >> 8);
                let here = map.nav(cx, cy, z);

                // The step two cells back, reversed: the way that would turn
                // straight back.
                let mut back2 = -1;
                {
                    let (mut px, mut py) = (cx, cy);
                    for k in 0..2 {
                        let a = self.came_from[idx(px, py)] as i32 - 1;
                        if !(0..=3).contains(&a) {
                            break;
                        }
                        px -= STEP_X[a as usize];
                        py -= STEP_Y[a as usize];
                        if px < lox || px >= hix || py < loy || py >= hiy {
                            break;
                        }
                        if k == 1 {
                            back2 = OPPOSITE[a as usize];
                        }
                    }
                }
                // Keep going the way we arrived first, then the others in
                // order.
                let mut order = [0usize, 1, 2, 3];
                let arrival = self.came_from[idx(cx, cy)] as i32 - 1;
                if (0..4).contains(&arrival) {
                    order[0] = arrival as usize;
                    let mut k = 1;
                    for i in 0..4 {
                        if i != arrival as usize {
                            order[k] = i;
                            k += 1;
                        }
                    }
                }

                for &i in &order {
                    if nav::dirs(here) & DIR_BIT[i] == 0 {
                        continue;
                    }
                    if i as i32 == ban_i && cx == sx && cy == sy {
                        continue;
                    }
                    if i as i32 == back2 && !allow_reverse {
                        continue;
                    }
                    if here & nav::SLOPE != 0 && map.slope_up_dir(cx, cy, z) == Some(ANG_OF[i]) {
                        continue;
                    }
                    let (nx, ny) = (cx + STEP_X[i], cy + STEP_Y[i]);
                    if nx < lox || nx >= hix || ny < loy || ny >= hiy {
                        continue;
                    }
                    if map.nav(nx, ny, z) & 0x0f == 0 {
                        continue;
                    }
                    let ncell = idx(nx, ny);
                    if self.came_from[ncell] != 0 {
                        continue;
                    }
                    self.came_from[ncell] = 1 + i as u8;
                    self.queue[tail] = (((ny - oy) << 8) | (nx - ox)) as u16;
                    tail += 1;

                    if let Some(w) = wander.as_mut() {
                        let md = (nx - sx).abs() + (ny - sy).abs();
                        if md >= w.min && md <= w.max {
                            wseen += 1;
                            lcg_step(w.seed);
                            if (*w.seed >> 16) % wseen == 0 {
                                (wpx, wpy) = (nx, ny);
                            }
                        }
                    } else if nx == tx && ny == ty {
                        break 'search true;
                    } else if allow_reverse {
                        let dd = (nx - tx).abs() + (ny - ty).abs();
                        if dd < best_d {
                            best_d = dd;
                            (bpx, bpy) = (nx, ny);
                        }
                    }
                }
            }
            if wander.is_none() && allow_reverse && bpx >= 0 {
                (tx, ty) = (bpx, bpy);
                break 'search true;
            }
            if wander.is_some() && wpx >= 0 {
                (tx, ty) = (wpx, wpy);
                break 'search true;
            }
            false
        };
        if !found {
            self.last_fail = Fail::Exhausted;
            return 0;
        }
        self.last_fail = Fail::Ok;

        // Count the steps back to the start.
        let mut n = 0usize;
        let (mut gx, mut gy) = (tx, ty);
        while !(gx == sx && gy == sy) {
            let d = self.came_from[idx(gx, gy)] as i32 - 1;
            if !(0..=3).contains(&d) {
                return 0;
            }
            n += 1;
            gx -= STEP_X[d as usize];
            gy -= STEP_Y[d as usize];
            if n > W * W {
                return 0;
            }
        }
        // Too long: keep the first `max` steps.
        (gx, gy) = (tx, ty);
        if n > max {
            for _ in 0..n - max {
                let d = (self.came_from[idx(gx, gy)] - 1) as usize;
                gx -= STEP_X[d];
                gy -= STEP_Y[d];
            }
            n = max;
        }
        for i in (0..n).rev() {
            out[i] = Node {
                x: gx as u8,
                y: gy as u8,
            };
            let d = self.came_from[idx(gx, gy)] as i32 - 1;
            if !(0..=3).contains(&d) {
                break;
            }
            gx -= STEP_X[d as usize];
            gy -= STEP_Y[d as usize];
        }
        n
    }
}

/// A random road block `min..=max` blocks from `(bx, by)` (Manhattan), up
/// to 16 tries.
pub fn pick_target(
    map: &SimMap,
    bx: i32,
    by: i32,
    z: i32,
    min: i32,
    max: i32,
    seed: &mut u64,
) -> Option<(i32, i32)> {
    for _ in 0..16 {
        let r = min + ((lcg_step(seed) >> 16) % (max - min + 1) as u64) as i32;
        let ang = ((lcg_step(seed) >> 16) & 3) as i32;
        let mut dx = ((lcg_step(seed) >> 16) % (r + 1) as u64) as i32;
        let mut dy = r - dx;
        if ang & 1 != 0 {
            dx = -dx;
        }
        if ang & 2 != 0 {
            dy = -dy;
        }
        if nav::dirs(map.nav(bx + dx, by + dy, z)) != 0 {
            return Some((bx + dx, by + dy));
        }
    }
    None
}
