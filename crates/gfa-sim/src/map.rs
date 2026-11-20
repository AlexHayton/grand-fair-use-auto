//! The city as the simulation sees it: who may stand where, which way the
//! ramps climb, where the fences are, and how high the ground is.
//!
//! Ported from AmiGTA1 (MIT): `gta_map.c` (ground types, ramps) and
//! `gta_nav.c` (the navigation byte grid and thin walls). The rules, in
//! short:
//!
//! - **The ground type is on the block a person occupies**, not the block
//!   under their feet: a road's lid is on layer 1 and the walkable road
//!   block is the empty-looking layer 2 above it.
//! - **A block missing from the file inside a surface is pavement**, when
//!   the block below has a lid and at least two neighbours on the same
//!   layer are road, pavement or field. Without this rule there are
//!   invisible walls on pavements (AmiGTA1 found one at NYC 25,17).
//! - **Fences are thin walls**: a flat block's west or north face whose
//!   tile is in the style's short list of solid tiles blocks a person
//!   crossing that edge.
//!
//! The grids are computed once from the map. Map edits by the mission
//! script (M7) will have to refresh the edited cell and its neighbours.

use gfa_fixed::fx::{BLOCK_SHIFT, FP};
use gfa_formats::cmp::{Block, Cmp, Face, MAP_DIM, MAP_LAYERS};

/// Blocks along each side of the map.
pub const DIM: i32 = MAP_DIM as i32;
/// Layers in the map.
pub const LAYERS: i32 = MAP_LAYERS as i32;

/// Ground types: bits 4..6 of a block's type word (names from Carnage3D).
pub mod ground {
    pub const AIR: u8 = 0;
    pub const WATER: u8 = 1;
    pub const ROAD: u8 = 2;
    pub const PAVEMENT: u8 = 3;
    pub const FIELD: u8 = 4;
    pub const BUILDING: u8 = 5;

    /// Road, pavement and field: where a person can stand.
    pub fn walkable(g: u8) -> bool {
        (ROAD..=FIELD).contains(&g)
    }
}

/// Bits of the navigation byte.
pub mod nav {
    /// Traffic may leave northwards.
    pub const N: u8 = 0x01;
    pub const S: u8 = 0x02;
    pub const W: u8 = 0x04;
    pub const E: u8 = 0x08;
    /// The block is a ramp.
    pub const SLOPE: u8 = 0x80;

    pub fn dirs(v: u8) -> u8 {
        v & 0x0F
    }
    pub fn ground(v: u8) -> u8 {
        (v >> 4) & 7
    }
}

/// A thin wall on a block's west edge.
pub const WALL_W: u8 = 1;
/// A thin wall on a block's north edge.
pub const WALL_N: u8 = 2;
/// A post in the middle of a block's west edge (stops cars only).
pub const POST_W: u8 = 4;
/// A post in the middle of a block's north edge.
pub const POST_N: u8 = 8;

/// The flat-face tiles that stop a person, for one style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallTiles {
    pub solid: &'static [u8],
    /// Flat-face tiles that are posts: an 8×8 px square in the middle of the
    /// edge that stops a car, and nothing else.
    pub post: &'static [u8],
}

impl WallTiles {
    /// No thin walls: fences can be walked through.
    pub const NONE: WallTiles = WallTiles {
        solid: &[],
        post: &[],
    };

    /// The lists for GTA's three cities, by the map's style number. From
    /// AmiGTA1's research into the original (`gta_nav.c`, MIT).
    ///
    /// London's styles have no list yet (see `docs/PARITY.md`), so a London
    /// map should be given [`WallTiles::NONE`].
    pub fn gta(style: u8) -> WallTiles {
        const STYLE1: &[u8] = &[46, 51, 55, 56, 108, 114, 168, 187, 190, 194];
        const STYLE2: &[u8] = &[6, 48, 51, 108, 111, 112, 126, 142, 160, 193];
        const STYLE3: &[u8] = &[
            69, 70, 100, 101, 102, 106, 124, 125, 155, 156, 159, 160, 161, 162, 163, 164, 169, 170,
            184, 185, 186, 197,
        ];
        let (solid, post): (&[u8], &[u8]) = match style {
            2 => (STYLE2, &[21, 180]),
            3 => (STYLE3, &[140]),
            _ => (STYLE1, &[97]),
        };
        WallTiles { solid, post }
    }

    /// The list for a map: GTA's by style number; none for London's maps
    /// (`gta` false), whose lists are not known yet.
    pub fn for_map(style: u8, gta: bool) -> WallTiles {
        if gta {
            WallTiles::gta(style)
        } else {
            WallTiles::NONE
        }
    }

    fn is_solid(&self, tile: u8) -> bool {
        tile != 0 && self.solid.contains(&tile)
    }

    /// A solid tile is a wall, not a post, even where the lists meet.
    fn is_post(&self, tile: u8) -> bool {
        tile != 0 && !self.is_solid(tile) && self.post.contains(&tile)
    }
}

/// Is `(x, y, z)` inside the map?
#[inline]
pub fn in_map(x: i32, y: i32, z: i32) -> bool {
    (0..DIM).contains(&x) && (0..DIM).contains(&y) && (0..LAYERS).contains(&z)
}

#[inline]
fn cell(x: i32, y: i32, z: i32) -> usize {
    ((z as usize) << 16) | ((y as usize) << 8) | x as usize
}

fn raw_ground(b: &Block) -> u8 {
    ((b.type_map >> 4) & 7) as u8
}

/// The map plus the grids derived from it.
#[derive(Debug, Clone)]
pub struct SimMap {
    cmp: Cmp,
    walls_from: WallTiles,
    /// Ground type per cell, including inferred pavement.
    ground: Vec<u8>,
    /// AmiGTA1's navigation byte per cell.
    nav: Vec<u8>,
    /// [`WALL_W`], [`WALL_N`], [`POST_W`] and [`POST_N`] per cell.
    walls: Vec<u8>,
}

impl SimMap {
    pub fn new(cmp: Cmp, walls_from: WallTiles) -> Self {
        let n = MAP_DIM * MAP_DIM * MAP_LAYERS;
        let mut map = SimMap {
            cmp,
            walls_from,
            ground: vec![0; n],
            nav: vec![0; n],
            walls: vec![0; n],
        };
        // Ground first: the nav byte of a missing block reads it.
        for z in 0..LAYERS {
            for y in 0..DIM {
                for x in 0..DIM {
                    map.ground[cell(x, y, z)] = map.compute_ground(x, y, z);
                }
            }
        }
        for z in 0..LAYERS {
            for y in 0..DIM {
                for x in 0..DIM {
                    let i = cell(x, y, z);
                    map.nav[i] = map.compute_nav(x, y, z);
                    map.walls[i] = map.compute_walls(x, y, z);
                }
            }
        }
        map
    }

    /// The parsed map file.
    pub fn cmp(&self) -> &Cmp {
        &self.cmp
    }

    /// The block at `(x, y, z)`; `None` for air and off the map.
    pub fn block(&self, x: i32, y: i32, z: i32) -> Option<&Block> {
        if !in_map(x, y, z) {
            return None;
        }
        self.cmp.block(x as usize, y as usize, z as usize)
    }

    /// The ground type at `(x, y, z)`, 0 (air) off the map. Everything that
    /// asks "may somebody be here" goes through this.
    #[inline]
    pub fn ground_type(&self, x: i32, y: i32, z: i32) -> u8 {
        if in_map(x, y, z) {
            self.ground[cell(x, y, z)]
        } else {
            ground::AIR
        }
    }

    /// Can a person stand in block `(x, y, z)`?
    #[inline]
    pub fn walkable(&self, x: i32, y: i32, z: i32) -> bool {
        ground::walkable(self.ground_type(x, y, z))
    }

    /// The navigation byte at `(x, y, z)`, 0 off the map.
    #[inline]
    pub fn nav(&self, x: i32, y: i32, z: i32) -> u8 {
        if in_map(x, y, z) {
            self.nav[cell(x, y, z)]
        } else {
            0
        }
    }

    /// The thin-wall bits at `(x, y, z)`, 0 off the map.
    #[inline]
    pub fn wall(&self, x: i32, y: i32, z: i32) -> u8 {
        if in_map(x, y, z) {
            self.walls[cell(x, y, z)] & (WALL_W | WALL_N)
        } else {
            0
        }
    }

    /// The post bits at `(x, y, z)`, as [`WALL_W`] and [`WALL_N`].
    #[inline]
    pub fn post(&self, x: i32, y: i32, z: i32) -> u8 {
        if in_map(x, y, z) {
            self.walls[cell(x, y, z)] >> 2
        } else {
            0
        }
    }

    /// The ground type in the nav byte at `(x, y, z)`: like
    /// [`ground_type`](Self::ground_type), but railway blocks are air and
    /// dirt roads are road. What vehicles go by.
    #[inline]
    pub fn nav_ground(&self, x: i32, y: i32, z: i32) -> u8 {
        nav::ground(self.nav(x, y, z))
    }

    /// The lowest layer of column `(x, y)` with road, pavement or field in
    /// its nav byte, if any (AmiGTA1 `stand_layer`).
    pub fn stand_layer(&self, x: i32, y: i32) -> Option<i32> {
        (0..LAYERS).find(|&z| ground::walkable(self.nav_ground(x, y, z)))
    }

    /// Does a car's box touch a post on layer `z`? The box: centre `(cx, cy)`
    /// (16.16), unit heading `f` and right `r` in Q14, half length `hl` and
    /// half width `hw` in whole pixels (AmiGTA1 `gta_nav_post_hit`).
    #[allow(clippy::too_many_arguments)]
    pub fn post_hit(
        &self,
        z: i32,
        cx: i32,
        cy: i32,
        f: (i32, i32),
        r: (i32, i32),
        hl: i32,
        hw: i32,
    ) -> bool {
        let (px, py) = (cx >> FP, cy >> FP);
        let reach = hl.max(hw) + 4;
        let inside = |qx: i32, qy: i32| {
            let (dx, dy) = (i64::from(qx - px), i64::from(qy - py));
            let a = ((dx * i64::from(f.0) + dy * i64::from(f.1)) >> 14).abs();
            let b = ((dx * i64::from(r.0) + dy * i64::from(r.1)) >> 14).abs();
            a <= i64::from(hl + 2) && b <= i64::from(hw + 2)
        };
        let block = 1 << BLOCK_SHIFT;
        for y in ((py - reach) >> BLOCK_SHIFT)..=((py + reach) >> BLOCK_SHIFT) + 1 {
            for x in ((px - reach) >> BLOCK_SHIFT)..=((px + reach) >> BLOCK_SHIFT) + 1 {
                let w = self.post(x, y, z);
                if w & WALL_W != 0 && inside(x * block, y * block + block / 2) {
                    return true;
                }
                if w & WALL_N != 0 && inside(x * block + block / 2, y * block) {
                    return true;
                }
            }
        }
        false
    }

    /// The slope value (0 flat, 1..=44 ramps) at `(x, y, z)`.
    pub fn slope(&self, x: i32, y: i32, z: i32) -> u8 {
        self.block(x, y, z).map_or(0, Block::slope)
    }

    /// Which way the ramp at `(x, y, z)` climbs (0 N, 64 E, 128 S, 192 W),
    /// or `None` if it is not a ramp.
    pub fn slope_up_dir(&self, x: i32, y: i32, z: i32) -> Option<i32> {
        slope_up_dir(self.slope(x, y, z))
    }

    /// Is the ramp at `(x, y, z)` at its highest step, whose surface reaches
    /// the top of its layer? Only there may a person stand on the layer
    /// above while still over the ramp block.
    pub fn slope_is_top(&self, x: i32, y: i32, z: i32) -> bool {
        matches!(
            self.slope(x, y, z),
            2 | 4 | 6 | 8 | 16 | 24 | 32 | 40 | 41..=44
        )
    }

    /// Does the segment between two 16.16 points cross a thin wall on layer
    /// `z`? An edge counts when the segment crosses its line inside the
    /// edge's span, both ends included (AmiGTA1 `seg_cross`).
    pub fn wall_cross(&self, z: i32, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
        let (ax, ay) = (x0 >> FP, y0 >> FP);
        let (qx, qy) = (x1 >> FP, y1 >> FP);
        if ax == qx && ay == qy {
            return false;
        }
        let block = 1 << BLOCK_SHIFT;
        // West edges: vertical lines at x = c * 32.
        let (lo, hi) = (ax.min(qx) >> BLOCK_SHIFT, ax.max(qx) >> BLOCK_SHIFT);
        for c in lo..=hi + 1 {
            let edge = c * block;
            if (ax < edge) == (qx < edge) {
                continue;
            }
            let y = ay + (i64::from(qy - ay) * i64::from(edge - ax) / i64::from(qx - ax)) as i32;
            let r = y >> BLOCK_SHIFT;
            if self.wall(c, r, z) & WALL_W != 0 {
                return true;
            }
            if y & (block - 1) == 0 && self.wall(c, r - 1, z) & WALL_W != 0 {
                return true;
            }
        }
        // North edges: horizontal lines at y = c * 32.
        let (lo, hi) = (ay.min(qy) >> BLOCK_SHIFT, ay.max(qy) >> BLOCK_SHIFT);
        for c in lo..=hi + 1 {
            let edge = c * block;
            if (ay < edge) == (qy < edge) {
                continue;
            }
            let x = ax + (i64::from(qx - ax) * i64::from(edge - ay) / i64::from(qy - ay)) as i32;
            let r = x >> BLOCK_SHIFT;
            if self.wall(r, c, z) & WALL_N != 0 {
                return true;
            }
            if x & (block - 1) == 0 && self.wall(r - 1, c, z) & WALL_N != 0 {
                return true;
            }
        }
        false
    }

    /// The height of the ground under a 16.16 point on `layer`, as 16.16
    /// world pixels (32 to a layer). On a ramp it rises with the slope.
    pub fn ground_z(&self, x: i32, y: i32, layer: i32) -> i32 {
        let base = layer << (FP + BLOCK_SHIFT);
        let (bx, by) = (x >> (FP + BLOCK_SHIFT), y >> (FP + BLOCK_SHIFT));
        let slope = self.slope(bx, by, layer);
        let mask = (1 << (FP + BLOCK_SHIFT)) - 1;
        base + slope_height(slope, x & mask, y & mask)
    }

    fn compute_ground(&self, x: i32, y: i32, z: i32) -> u8 {
        if let Some(b) = self.block(x, y, z) {
            return raw_ground(b);
        }
        if z == 0 {
            return ground::AIR;
        }
        match self.block(x, y, z - 1) {
            Some(b) if b.face(Face::Lid) != 0 => {}
            // No floor: a hole, not a pavement.
            _ => return ground::AIR,
        }
        let surface = [(0, -1), (0, 1), (-1, 0), (1, 0)]
            .iter()
            .filter(|(dx, dy)| {
                self.block(x + dx, y + dy, z)
                    .is_some_and(|b| ground::walkable(raw_ground(b)))
            })
            .count();
        if surface >= 2 {
            ground::PAVEMENT
        } else {
            ground::AIR
        }
    }

    fn compute_nav(&self, x: i32, y: i32, z: i32) -> u8 {
        let Some(b) = self.block(x, y, z) else {
            // A missing block inside a surface: pavement, with no direction
            // bits, so nothing is routed through it.
            return if self.ground[cell(x, y, z)] == ground::PAVEMENT {
                0x30
            } else {
                0
            };
        };
        // AmiGTA1 departs from the original here: railway blocks are road
        // to the original, which parks cars on the tracks.
        if b.is_railway() {
            return 0;
        }
        let tm = (b.type_map & 0x7F) as u8;
        let slope = if b.slope() != 0 { nav::SLOPE } else { 0 };
        if tm & 0x70 == 0x40 && tm & 0x0F != 0 {
            // A field with direction arrows is a dirt road (San Andreas).
            return (tm & !0x70) | 0x20 | slope;
        }
        tm | slope
    }

    fn compute_walls(&self, x: i32, y: i32, z: i32) -> u8 {
        let Some(b) = self.block(x, y, z) else {
            return 0;
        };
        if !b.is_flat() {
            return 0;
        }
        let (west, north) = (b.face(Face::West), b.face(Face::North));
        let mut w = 0;
        if self.walls_from.is_solid(west) {
            w |= WALL_W;
        }
        if self.walls_from.is_solid(north) {
            w |= WALL_N;
        }
        if self.walls_from.is_post(west) {
            w |= POST_W;
        }
        if self.walls_from.is_post(north) {
            w |= POST_N;
        }
        w
    }
}

/// Which way a slope value climbs (0 N, 64 E, 128 S, 192 W).
///
/// Values 1..=8 are 26 degree ramps over two blocks, 9..=40 7 degree ramps
/// over eight, 41..=44 45 degree ramps in one; each group runs N, S, W, E.
/// The letter is the direction of ascent (AmiGTA1 checked it against the
/// map: NYC's ramp at x 11..18, y 44 has values 33..40 and rises east).
pub fn slope_up_dir(slope: u8) -> Option<i32> {
    Some(match slope {
        1..=2 | 9..=16 | 41 => 0,
        3..=4 | 17..=24 | 42 => 128,
        5..=6 | 25..=32 | 43 => 192,
        7..=8 | 33..=40 | 44 => 64,
        _ => return None,
    })
}

/// Height of a ramp's surface above its block's floor at a point inside the
/// block, all 16.16 world pixels (a block is 32 across and 32 high).
///
/// The same geometry as the renderer's `slope_corners`: the surface is a
/// plane rising along one axis, given as heights in eighths of a block at
/// the low- and high-coordinate edges.
pub fn slope_height(slope: u8, fx: i32, fy: i32) -> i32 {
    let s = slope;
    // (along y?, height at the low edge, at the high edge), in eighths.
    let (along_y, e0, e1) = match s {
        1..=2 => (true, s * 4, (s - 1) * 4),
        3..=4 => (true, (s - 3) * 4, (s - 2) * 4),
        5..=6 => (false, (s - 4) * 4, (s - 5) * 4),
        7..=8 => (false, (s - 7) * 4, (s - 6) * 4),
        9..=16 => (true, s - 8, s - 9),
        17..=24 => (true, s - 17, s - 16),
        25..=32 => (false, s - 24, s - 25),
        33..=40 => (false, s - 33, s - 32),
        41 => (true, 8, 0),
        42 => (true, 0, 8),
        43 => (false, 8, 0),
        44 => (false, 0, 8),
        _ => return 0,
    };
    // An eighth of a 32 px block is 4 px.
    let (e0, e1) = (i32::from(e0) * 4, i32::from(e1) * 4);
    let f = if along_y { fy } else { fx };
    // (e1 - e0) is at most 32 and f under 2^21: the product fits.
    (e0 << FP) + (((e1 - e0) * f) >> BLOCK_SHIFT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramps_rise_the_way_they_say() {
        let top = 32 << FP;
        let end = top - 1;
        for s in 1..=44u8 {
            let up = slope_up_dir(s).unwrap();
            // Height at the north, south, west and east edges' midpoints.
            let mid = 16 << FP;
            let n = slope_height(s, mid, 0);
            let so = slope_height(s, mid, end);
            let w = slope_height(s, 0, mid);
            let e = slope_height(s, end, mid);
            let rises = match up {
                0 => n > so,
                128 => so > n,
                192 => w > e,
                64 => e > w,
                _ => unreachable!(),
            };
            assert!(
                rises,
                "slope {s} should rise towards {up}: n {n} s {so} w {w} e {e}"
            );
            assert!((0..=top).contains(&n) && (0..=top).contains(&e));
        }
        assert_eq!(slope_up_dir(0), None);
        assert_eq!(slope_up_dir(45), None);
        assert_eq!(slope_height(0, 5, 5), 0);
    }

    #[test]
    fn eight_step_ramp_is_continuous() {
        // Values 33..=40 climb east one eighth per block: each block's east
        // edge meets the next block's west edge.
        for s in 33..40u8 {
            let east = slope_height(s, 32 << FP, 0);
            let next_west = slope_height(s + 1, 0, 0);
            assert_eq!(east, next_west, "slope {s}");
        }
        assert_eq!(slope_height(40, 32 << FP, 0), 32 << FP);
    }
}
