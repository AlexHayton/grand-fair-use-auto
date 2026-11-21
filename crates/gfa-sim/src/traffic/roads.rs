//! What traffic asks of the map: which blocks are road, which ways they let
//! cars leave, where the junction boxes are, which lane to keep to, and
//! the traffic lights' clock.
//!
//! After AmiGTA1 (MIT), `gta_traffic.c`. Everything here reads the
//! navigation grid ([`SimMap::nav`]) for ground and arrows, except where
//! AmiGTA1 reads the raw block's arrows ([`raw_dirs`]): those include
//! railway blocks, which the grid zeroes.

use crate::map::{SimMap, ground, nav};

/// Which way one block step goes for a road angle.
#[inline]
pub fn heading_step(angle: i32) -> (i32, i32) {
    match angle & 255 {
        0 => (0, -1),
        64 => (1, 0),
        128 => (0, 1),
        _ => (-1, 0),
    }
}

/// The angle of a unit block step.
#[inline]
pub fn step_angle(dx: i32, dy: i32) -> i32 {
    if dy < 0 {
        0
    } else if dy > 0 {
        128
    } else if dx > 0 {
        64
    } else {
        192
    }
}

/// The ground type traffic sees: the nav grid's (railway is air).
#[inline]
pub fn ground_at(map: &SimMap, bx: i32, by: i32, z: i32) -> u8 {
    map.nav_ground(bx, by, z)
}

/// Road a car may drive on.
#[inline]
pub fn drivable(map: &SimMap, bx: i32, by: i32, z: i32) -> bool {
    ground_at(map, bx, by, z) == ground::ROAD
}

/// The arrows of the block in the file, `None` where there is none.
#[inline]
pub fn raw_dirs(map: &SimMap, bx: i32, by: i32, z: i32) -> Option<u8> {
    if !(0..256).contains(&bx) || !(0..256).contains(&by) {
        return None;
    }
    map.block(bx, by, z).map(|b| (b.type_map & 0x0F) as u8)
}

/// The nav grid's arrows, 0 off the map.
#[inline]
pub fn nav_dirs(map: &SimMap, bx: i32, by: i32, z: i32) -> u8 {
    nav::dirs(map.nav(bx, by, z))
}

/// The lowest layer of road in a column.
pub fn road_layer(map: &SimMap, bx: i32, by: i32) -> Option<i32> {
    (0..6).find(|&z| {
        ground_at(map, bx, by, z) == ground::ROAD
            && !map.block(bx, by, z).is_some_and(|b| b.is_railway())
    })
}

/// Does the block's own arrow allow leaving at `angle`?
pub fn dir_allowed(map: &SimMap, bx: i32, by: i32, z: i32, angle: i32) -> bool {
    let Some(d) = raw_dirs(map, bx, by, z) else {
        return false;
    };
    match angle & 255 {
        0 => d & 0x01 != 0,
        128 => d & 0x02 != 0,
        192 => d & 0x04 != 0,
        64 => d & 0x08 != 0,
        _ => false,
    }
}

/// Whichever way the road runs across `(bx, by)`: east-west (64) when more
/// road lies either side along x than along y, otherwise north-south (0).
pub fn road_axis(map: &SimMap, bx: i32, by: i32, z: i32) -> i32 {
    let run = |dx: i32, dy: i32| {
        (1..=3)
            .take_while(|&i| ground_at(map, bx + dx * i, by + dy * i, z) == ground::ROAD)
            .count()
    };
    let nx = run(-1, 0) + run(1, 0);
    let ny = run(0, -1) + run(0, 1);
    if nx > ny { 64 } else { 0 }
}

/// Does a vehicle `length` world px long fit at `(bx, by)` along `axis`
/// (0 north-south, 64 east-west)? Its block must be road, and the blocks
/// its ends reach road or pavement.
pub fn fits(map: &SimMap, bx: i32, by: i32, z: i32, axis: i32, length: i32) -> bool {
    let over = length / 2 - 16;
    let reach = if over > 0 { (over + 31) / 32 } else { 0 };
    if ground_at(map, bx, by, z) != ground::ROAD {
        return false;
    }
    (-reach..=reach).all(|i| {
        let (cx, cy) = if axis == 0 {
            (bx, by + i)
        } else {
            (bx + i, by)
        };
        let g = ground_at(map, cx, cy, z);
        g == ground::ROAD || g == ground::PAVEMENT
    })
}

/// The layer a car reaches stepping off `(bx, by, z)` towards `dir`, if any:
/// up a ramp that climbs that way, level, or down/up where a ramp joins.
pub fn nav_step_layer(map: &SimMap, bx: i32, by: i32, z: i32, dir: i32) -> Option<i32> {
    let (dx, dy) = heading_step(dir);
    if z + 1 < 6
        && map.slope_up_dir(bx, by, z) == Some(dir)
        && drivable(map, bx + dx, by + dy, z + 1)
    {
        return Some(z + 1);
    }
    if drivable(map, bx + dx, by + dy, z) {
        return Some(z);
    }
    let sloped = |x: i32, y: i32, l: i32| map.nav(x, y, l) & nav::SLOPE != 0;
    let here = sloped(bx, by, z);
    if z > 0 && drivable(map, bx + dx, by + dy, z - 1) && (here || sloped(bx + dx, by + dy, z - 1))
    {
        return Some(z - 1);
    }
    if z + 1 < 6
        && drivable(map, bx + dx, by + dy, z + 1)
        && (here || sloped(bx + dx, by + dy, z + 1))
    {
        return Some(z + 1);
    }
    None
}

const DIRS: [i32; 4] = [0, 64, 128, 192];

/// Can a car entering `(bx, by)` heading `inn` leave it again, other than
/// back the way it came, with room for its length?
pub fn has_exit(map: &SimMap, bx: i32, by: i32, z: i32, inn: i32, length: i32) -> bool {
    let back = (inn + 128) & 255;
    if raw_dirs(map, bx, by, z).is_none() {
        return false;
    }
    DIRS.iter().any(|&d| {
        if d == back || !dir_allowed(map, bx, by, z, d) {
            return false;
        }
        let (dx, dy) = heading_step(d);
        let Some(nz) = nav_step_layer(map, bx, by, z, d) else {
            return false;
        };
        let axis = if d & 127 == 64 { 64 } else { 0 };
        fits(map, bx, by, z, axis, length) && fits(map, bx + dx, by + dy, nz, axis, length)
    })
}

/// Blocks a spawn must reach to count as connected to the city.
pub const REACH_MIN: usize = 24;

/// Is `(bx, by)` connected to at least [`REACH_MIN`] blocks of road by the
/// arrows? Keeps cars out of dead-end pockets.
pub fn can_get_away(map: &SimMap, bx: i32, by: i32, z: i32) -> bool {
    let mut q: Vec<(i32, i32, i32)> = Vec::with_capacity(REACH_MIN);
    q.push((bx, by, z));
    let mut head = 0;
    while head < q.len() {
        let (cx, cy, cz) = q[head];
        head += 1;
        for &d in &DIRS {
            if !dir_allowed(map, cx, cy, cz, d) {
                continue;
            }
            let Some(nz) = nav_step_layer(map, cx, cy, cz, d) else {
                continue;
            };
            let (dx, dy) = heading_step(d);
            let (nx, ny) = (cx + dx, cy + dy);
            // AmiGTA1 keeps the queue in bytes and compares them with the
            // unclipped step.
            if q.iter().any(|&(x, y, z)| x == nx && y == ny && z == nz) {
                continue;
            }
            if q.len() >= REACH_MIN {
                return true;
            }
            q.push((nx & 255, ny & 255, nz));
        }
    }
    false
}

/// The junction boxes: every road block of a crossing, grown out from the
/// blocks that carry arrows on both axes, up to 8 blocks a side.
#[derive(Debug, Clone)]
pub struct Boxes {
    bits: Vec<u8>,
}

impl Boxes {
    pub fn build(map: &SimMap) -> Boxes {
        let mut b = Boxes {
            bits: vec![0; 256 * 256 * 6 / 8],
        };
        for z in 0..6 {
            for y in 0..256 {
                for x in 0..256 {
                    if seed(map, x, y, z) {
                        b.grow(map, z, x, y);
                    }
                }
            }
        }
        b
    }

    fn set(&mut self, x: i32, y: i32, z: i32) {
        let n = ((z * 256 + y) * 256 + x) as usize;
        self.bits[n >> 3] |= 1 << (n & 7);
    }

    fn grow(&mut self, map: &SimMap, z: i32, sx: i32, sy: i32) {
        let road = |x: i32, y: i32| map.nav_ground(x, y, z) == ground::ROAD;
        let dirs = |x: i32, y: i32| nav_dirs2(map, x, y, z);
        let (mut x0, mut x1, mut y0, mut y1) = (sx, sx, sy, sy);
        let mut changed = true;
        let mut guard = 0;
        while changed && {
            guard += 1;
            guard < 16
        } {
            changed = false;
            if x1 - x0 < 7 {
                let mut x = x0 - 1;
                while x <= x1 + 1 {
                    let hasns = (y0 - 1..=y1 + 1).any(|y| road(x, y) && dirs(x, y) & 0x03 != 0);
                    let hasroad = (y0..=y1).any(|y| road(x, y));
                    if hasns && hasroad {
                        if x < x0 {
                            x0 = x;
                        } else {
                            x1 = x;
                        }
                        changed = true;
                    }
                    x += x1 - x0 + 2;
                }
            }
            if y1 - y0 < 7 {
                let mut y = y0 - 1;
                while y <= y1 + 1 {
                    let hasew = (x0 - 1..=x1 + 1).any(|x| road(x, y) && dirs(x, y) & 0x0C != 0);
                    let hasroad = (x0..=x1).any(|x| road(x, y));
                    if hasew && hasroad {
                        if y < y0 {
                            y0 = y;
                        } else {
                            y1 = y;
                        }
                        changed = true;
                    }
                    y += y1 - y0 + 2;
                }
            }
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                if road(x, y) {
                    self.set(x, y, z);
                }
            }
        }
    }

    /// Is `(bx, by, z)` part of a junction box?
    #[inline]
    pub fn is_junction(&self, bx: i32, by: i32, z: i32) -> bool {
        if !(0..256).contains(&bx) || !(0..256).contains(&by) || !(0..6).contains(&z) {
            return false;
        }
        let n = ((z << 16) | (by << 8) | bx) as usize;
        (self.bits[n >> 3] >> (bx & 7)) & 1 != 0
    }

    /// The north-west corner of the junction box around `(bx, by)`.
    pub fn root(&self, mut bx: i32, mut by: i32, z: i32) -> (i32, i32) {
        for _ in 0..BOX_SCAN {
            if !self.is_junction(bx - 1, by, z) {
                break;
            }
            bx -= 1;
        }
        for _ in 0..BOX_SCAN {
            if !self.is_junction(bx, by - 1, z) {
                break;
            }
            by -= 1;
        }
        (bx, by)
    }
}

/// How far [`Boxes::root`] walks.
pub const BOX_SCAN: i32 = 5;

fn nav_dirs2(map: &SimMap, x: i32, y: i32, z: i32) -> u8 {
    if !(0..256).contains(&x) || !(0..256).contains(&y) {
        return 0;
    }
    nav_dirs(map, x, y, z)
}

fn seed(map: &SimMap, x: i32, y: i32, z: i32) -> bool {
    let d = nav_dirs2(map, x, y, z);
    map.nav_ground(x, y, z) == ground::ROAD && d & 0x03 != 0 && d & 0x0C != 0
}

/// The lane centre, px into the block across the road, for a car heading
/// `dir` at `(bx, by)`: the middle, or 4 px off it towards the side with
/// road when there is a kerb on one side only. Inside a junction it looks
/// ahead up to 4 blocks to the street it is entering.
pub fn lane_target(map: &SimMap, boxes: &Boxes, mut bx: i32, mut by: i32, z: i32, dir: i32) -> i32 {
    let mut t = LANE_TARGET;
    let ew = dir & 127 == 64;
    let (dx, dy) = heading_step(dir);
    for _ in 0..LANE_STREET_SCAN {
        let (lx, ly, hx, hy) = if ew {
            (bx, by - 1, bx, by + 1)
        } else {
            (bx - 1, by, bx + 1, by)
        };
        if !boxes.is_junction(bx, by, z)
            && !boxes.is_junction(lx, ly, z)
            && !boxes.is_junction(hx, hy, z)
        {
            break;
        }
        if !drivable(map, bx + dx, by + dy, z) {
            break;
        }
        bx += dx;
        by += dy;
    }
    let (lo, hi) = if ew {
        (drivable(map, bx, by - 1, z), drivable(map, bx, by + 1, z))
    } else {
        (drivable(map, bx - 1, by, z), drivable(map, bx + 1, by, z))
    };
    if !lo && hi {
        t += LANE_KERB;
    } else if lo && !hi {
        t -= LANE_KERB;
    }
    t
}

/// The lane line: px into the block, the middle.
pub const LANE_TARGET: i32 = 16;
/// How far from the middle a lane beside a kerb runs.
pub const LANE_KERB: i32 = 4;
const LANE_STREET_SCAN: i32 = 4;

/// Ticks of green for one axis.
pub const LIGHT_PHASE: u32 = 250;
/// The last ticks of a green that count as amber.
pub const LIGHT_AMBER: u32 = 30;

/// Is the light green for traffic along x (`along_x`) or y at `(bx, by)` on
/// tick `tick`? Neighbourhoods of 8×8 blocks alternate their phase.
pub fn light_green(tick: u32, bx: i32, by: i32, along_x: bool) -> bool {
    let cycle = LIGHT_PHASE * 2;
    let off = (((bx >> 3) + (by >> 3)) as u32 & 1) * LIGHT_PHASE;
    let p = tick.wrapping_add(off) % cycle;
    let ew = p < LIGHT_PHASE;
    if p % LIGHT_PHASE >= LIGHT_PHASE - LIGHT_AMBER {
        return false;
    }
    if along_x { ew } else { !ew }
}
