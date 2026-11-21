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

/// Does a vehicle `length` world px long fit coming into `(bx, by)` heading
/// `dir`? Lying along `dir`, where the block lets it go straight on. At a
/// corner, where it does not, the car turns before its nose reaches the far
/// side, so what has to take it is the street it turns into: some way out
/// of the corner, not back, where it [`fits`] lying along that way. A
/// corner with no way out is judged along `dir`, as before.
///
/// This is the whole test of a move, of a car or of a way out of a block:
/// the car is already where it is.
///
/// Ours (decision log, 2026-11-21; traffic_fits.pl): AmiGTA1 asks [`fits`]
/// along the way the car leaves, of the block it is in and of the next. So
/// a bus coming to a corner whose far side is a building or grass is
/// refused the move, and one that has turned in the corner, its tail still
/// over the verge behind, is refused the way out; either stops for good and
/// blocks its street.
pub fn fits_entering(map: &SimMap, bx: i32, by: i32, z: i32, dir: i32, length: i32) -> bool {
    let axis = |d: i32| if d & 127 == 64 { 64 } else { 0 };
    if dir_allowed(map, bx, by, z, dir) {
        return fits(map, bx, by, z, axis(dir), length);
    }
    let back = (dir + 128) & 255;
    let exits: Vec<i32> = DIRS
        .iter()
        .copied()
        .filter(|&d| d != dir && d != back && dir_allowed(map, bx, by, z, d))
        .collect();
    if exits.is_empty() || ground_at(map, bx, by, z) != ground::ROAD {
        return fits(map, bx, by, z, axis(dir), length);
    }
    exits.into_iter().any(|d| {
        let (dx, dy) = heading_step(d);
        nav_step_layer(map, bx, by, z, d)
            .is_some_and(|nz| fits(map, bx + dx, by + dy, nz, axis(d), length))
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
        let Some(nz) = nav_step_layer(map, bx, by, z, d) else {
            return false;
        };
        let (dx, dy) = heading_step(d);
        fits_entering(map, bx + dx, by + dy, nz, d, length)
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

    /// The crossing a light at `(bx, by)` belongs to, named by its junction
    /// box's root: the first junction block within two blocks, nearest
    /// first (rings from the north-west, then the layers up). A block with
    /// none near is its own name. A car at a stop line asks about the box
    /// ahead of it, a lamp about its corner's, so neither is misled by a
    /// second crossing close behind; a person waiting to cross asks about
    /// where he stands.
    ///
    /// Ours: AmiGTA1 names a light by its own block, and since a crossing's
    /// stop lines lie either side of its box they often fall in different
    /// 8×8 neighbourhoods; then two approaches across each other are green
    /// at once, which twenty of Liberty City's twenty-four lit crossings
    /// did (decision log, 2026-11-21; `tools/oracle/traffic_lights.pl`).
    pub fn crossing(&self, bx: i32, by: i32) -> (i32, i32) {
        for r in 0..=2i32 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    for z in 0..6 {
                        if self.is_junction(bx + dx, by + dy, z) {
                            return self.root(bx + dx, by + dy, z);
                        }
                    }
                }
            }
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

/// Is the light green on tick `tick` for traffic along x (`along_x`) or y
/// at the crossing named `(kx, ky)` ([`Boxes::crossing`])?
pub fn light_green(tick: u32, kx: i32, ky: i32, along_x: bool) -> bool {
    light(tick, kx, ky, along_x) == Light::Green
}

/// What a traffic light shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Light {
    Red,
    Green,
    /// The end of a green: traffic stops, as at red.
    Amber,
}

impl Light {
    /// The lamp's sprite within the style's traffic-light sprites.
    pub fn sprite(self) -> usize {
        match self {
            Light::Red => 0,
            Light::Green => 1,
            Light::Amber => 2,
        }
    }
}

/// The light on tick `tick` for traffic along x or y at the crossing named
/// `(kx, ky)` (AmiGTA1 `gta_traffic_light_state`): green for one axis while
/// the other is red, amber for the last [`LIGHT_AMBER`] ticks of each green.
/// Crossings in alternate 8×8 neighbourhoods run half a cycle apart, so the
/// city does not change in unison.
pub fn light(tick: u32, kx: i32, ky: i32, along_x: bool) -> Light {
    let cycle = LIGHT_PHASE * 2;
    let off = (((kx >> 3) + (ky >> 3)) as u32 & 1) * LIGHT_PHASE;
    let p = tick.wrapping_add(off) % cycle;
    let ew = p < LIGHT_PHASE;
    if along_x != ew {
        Light::Red
    } else if p % LIGHT_PHASE >= LIGHT_PHASE - LIGHT_AMBER {
        Light::Amber
    } else {
        Light::Green
    }
}

/// A traffic light on the corner of a lit crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lamp {
    /// The pavement block it stands on.
    pub bx: i32,
    pub by: i32,
    pub z: i32,
    /// The road block beside it that it is for: one block from the corner.
    pub road: (i32, i32),
    /// That road runs along x.
    pub along_x: bool,
    /// The heading into the crossing along that road: the lamp faces the
    /// drivers coming the other way.
    pub toward: i32,
    /// The crossing whose light it shows ([`Boxes::crossing`]).
    pub key: (i32, i32),
}

impl Lamp {
    /// Where it is drawn, 16.16 sim px: ten pixels from the corner's middle
    /// toward its road (AmiGTA1).
    pub fn pos(&self) -> (i32, i32) {
        let (dx, dy) = (self.road.0 - self.bx, self.road.1 - self.by);
        let at = |b: i32, d: i32| (b * 32 + 16 + d * 10) << 16;
        (at(self.bx, dx), at(self.by, dy))
    }

    /// What it shows on tick `tick`: the light the cars on its road obey.
    pub fn light(&self, tick: u32) -> Light {
        light(tick, self.key.0, self.key.1, self.along_x)
    }
}

/// Every traffic light in the city (AmiGTA1 `gta_traffic_lights_scan`).
///
/// The map marks hint 1 on every block round a lit crossing: the stop-line
/// blocks of every lane and the pavement beside them. The lamps stand on
/// the pavement corners, a corner being a hinted pavement block with hinted
/// road beside it; it gets a lamp for each such road, coloured for that
/// road's axis, so a driver sees the lamp for his own carriageway.
///
/// Ours: AmiGTA1 draws every lamp facing south. Here its lens (the
/// sprite's front) faces the drivers coming into the crossing at its
/// corner, and it shows that crossing's light.
pub fn lamps(map: &SimMap, boxes: &Boxes) -> Vec<Lamp> {
    const SIDES: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
    let hinted = |bx: i32, by: i32, z: i32, ground: u8| {
        map.block(bx, by, z)
            .filter(|b| b.traffic_hint() == 1 && ((b.type_map >> 4) & 7) as u8 == ground)
    };
    let mut out = Vec::new();
    for by in 1..255 {
        for bx in 1..255 {
            for z in 0..6 {
                if hinted(bx, by, z, ground::PAVEMENT).is_none() {
                    continue;
                }
                // The roads beside the corner, and their arrows.
                let sides: Vec<(i32, i32, u8)> = SIDES
                    .iter()
                    .filter_map(|&(dx, dy)| {
                        let road = hinted(bx + dx, by + dy, z, ground::ROAD)?;
                        let dirs = (road.type_map & 0x0F) as u8;
                        (dirs != 0).then_some((dx, dy, dirs))
                    })
                    .collect();
                for &(dx, dy, dirs) in &sides {
                    let (rx, ry) = (bx + dx, by + dy);
                    let along_x = dirs & 0x0C != 0;
                    // The corner's crossing: along the road from the block
                    // beside it, on the side of the corner's other road if
                    // there are boxes both ways.
                    let along: [(i32, i32); 2] = if along_x {
                        [(1, 0), (-1, 0)]
                    } else {
                        [(0, 1), (0, -1)]
                    };
                    let at_corner = along
                        .into_iter()
                        .filter(|&(ex, ey)| boxes.is_junction(rx + ex, ry + ey, z))
                        .min_by_key(|&(ex, ey)| !sides.iter().any(|s| (s.0, s.1) == (ex, ey)));
                    let (toward, entry) = match at_corner {
                        Some((ex, ey)) => (step_angle(ex, ey), Some((rx + ex, ry + ey))),
                        None => into_crossing(boxes, rx, ry, z, along_x, dirs),
                    };
                    let (kx, ky) = entry.unwrap_or((rx, ry));
                    out.push(Lamp {
                        bx,
                        by,
                        z,
                        road: (rx, ry),
                        along_x,
                        toward,
                        key: boxes.crossing(kx, ky),
                    });
                }
            }
        }
    }
    out
}

/// The heading along the road at `(bx, by)` that reaches a junction box
/// soonest (two blocks at most), and the box's first block. Failing that,
/// the nearest box off to one side, which is where a kerb lane runs past a
/// T-junction's box (San Andreas has twenty-three such lamps): the heading
/// toward it along the road. Failing that, the way the arrows point.
fn into_crossing(
    boxes: &Boxes,
    bx: i32,
    by: i32,
    z: i32,
    along_x: bool,
    dirs: u8,
) -> (i32, Option<(i32, i32)>) {
    let (fwd, back) = if along_x { (64, 192) } else { (128, 0) };
    for k in 1..=2 {
        for angle in [fwd, back] {
            let (dx, dy) = heading_step(angle);
            let (jx, jy) = (bx + dx * k, by + dy * k);
            if boxes.is_junction(jx, jy, z) {
                return (angle, Some((jx, jy)));
            }
        }
    }
    for r in 1..=2i32 {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dy.abs() != r {
                    continue;
                }
                let (jx, jy) = (bx + dx, by + dy);
                let along = if along_x { dx } else { dy };
                if along != 0 && boxes.is_junction(jx, jy, z) {
                    let angle = match (along_x, along > 0) {
                        (true, true) => 64,
                        (true, false) => 192,
                        (false, true) => 128,
                        (false, false) => 0,
                    };
                    return (angle, Some((jx, jy)));
                }
            }
        }
    }
    let angle = match dirs {
        d if d & 0x08 != 0 && along_x => 64,
        d if d & 0x04 != 0 && along_x => 192,
        d if d & 0x02 != 0 => 128,
        _ => 0,
    };
    (angle, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_axis_at_a_time_with_amber_before_red() {
        for (bx, by) in [(3, 3), (9, 3)] {
            let mut was = (light(0, bx, by, true), light(0, bx, by, false));
            for t in 1..LIGHT_PHASE * 4 {
                let now = (light(t, bx, by, true), light(t, bx, by, false));
                assert!(
                    now.0 == Light::Red || now.1 == Light::Red,
                    "tick {t}: {now:?}"
                );
                for (a, b) in [(was.0, now.0), (was.1, now.1)] {
                    if a != b {
                        // Green, amber, red, green.
                        let next = match a {
                            Light::Green => Light::Amber,
                            Light::Amber => Light::Red,
                            Light::Red => Light::Green,
                        };
                        assert_eq!(b, next, "tick {t}");
                    }
                }
                assert_eq!(light_green(t, bx, by, true), now.0 == Light::Green);
                was = now;
            }
        }
        // Neighbouring neighbourhoods run half a cycle apart.
        assert_eq!(light(10, 3, 3, true), light(10, 9, 3, false));
    }
}
