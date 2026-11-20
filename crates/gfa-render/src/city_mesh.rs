//! Building the city's geometry from a `.CMP` map, on the CPU.
//!
//! Sources: Carnage3D `src/GameMapHelpers.cpp` (`BuildMapMesh`,
//! `PutBlockFace`; MIT, © 2019 jericho) for the face layout, slopes, lid
//! rotation and flips; AmiGTA1 `native/gta_render.c` (`draw_block`; MIT) for
//! what a flat block is and which faces are see-through. Implementation ours.
//!
//! Units are the original's pixels: a block is 64 across and 64 tall. `x`
//! runs east, `y` south and `z` up, so a vertex is `(x, y, z)` exactly as the
//! map addresses it, times 64.
//!
//! The rules, with where they come from:
//! - A cube's side faces are drawn as seen from outside the block, so a tile
//!   reads left to right from wherever it is visible (Carnage3D). Back-face
//!   culling then shows at most one of a block's east and west faces, as the
//!   original's straight-down camera does (AmiGTA1, "fact 2").
//! - The `flip_left_right` bit mirrors the west and east faces; the
//!   `flip_top_bottom` bit mirrors the north and south faces (both).
//! - A **flat** block is a plate with no thickness (fences, shop fronts,
//!   railings): its east face is drawn on its west face's plane and its south
//!   face on its north face's plane. Both sides of a plate run the same way
//!   in the world, so a sign does not reverse when the camera passes it
//!   (AmiGTA1). If one side has no tile, the other side's tile shows from
//!   both directions (AmiGTA1).
//! - A flat block's lid sits on top of its cube, like any lid (AmiGTA1: the
//!   elevated railway proves it), and ignores the slope.
//! - Lids rotate clockwise in quarter turns (both references agree).
//! - Colour 0 is see-through on a flat block's faces and lid, and nowhere
//!   else (Carnage3D). AmiGTA1 masks colour 0 on every side face; drawn in
//!   3D that opens holes through solid walls, whose windows use colour 0 for
//!   black (seen in the first render of NYC at block 64,64).
//! - Slopes lower the top edge of the lid and the side faces (Carnage3D).
//!   Side faces keep the texture's scale and lose their top rows, rather
//!   than squashing the tile into the shorter face. This is our choice: the
//!   references differ (Carnage3D squashes; AmiGTA1 draws full-height walls).
//! - Side tile numbers past the side bank are mission doors, which the
//!   mission script assigns (AmiGTA1 `doors_build`). They are skipped until
//!   M7 and counted in [`MeshStats::door_faces`].

use gfa_formats::cmp::{Block, Cmp, Face, MAP_DIM, MAP_LAYERS};

/// Pixels per block, horizontally and vertically.
pub const BLOCK_PX: u16 = 64;
/// Blocks per side of a chunk. 256 / 16 = 16 chunks per side.
pub const CHUNK_BLOCKS: usize = 16;
pub const CHUNKS_PER_SIDE: usize = MAP_DIM / CHUNK_BLOCKS;

/// Vertex flags, in `attr[3]`.
pub mod flags {
    /// Colour 0 is see-through.
    pub const TRANSPARENT: u8 = 1 << 0;
    /// Part of a flat block: drawn a hair towards the camera so it wins
    /// against the cube face it shares a plane with.
    pub const FLAT: u8 = 1 << 1;
    /// The face, `Face as u8`, in bits 4..7 (for the inspector and debug views).
    pub const FACE_SHIFT: u8 = 4;
}

/// One corner of a face. 12 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CityVertex {
    /// `x, y, z` in pixels, then the tile, linear across the style's banks.
    pub pos: [u16; 4],
    /// `u, v` in texels (0..=64), the lid remap (0..=3), [`flags`].
    pub attr: [u8; 4],
}

/// A square of the map whose faces are contiguous in the index buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chunk {
    /// The north-west block.
    pub x: u16,
    pub y: u16,
    /// Top of the tallest column, in layers.
    pub height: u8,
    pub first_index: u32,
    pub index_count: u32,
}

impl Chunk {
    /// Bounds in pixels: `(min, max)` corners.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let px = f32::from(BLOCK_PX);
        let side = CHUNK_BLOCKS as f32 * px;
        let (x, y) = (f32::from(self.x) * px, f32::from(self.y) * px);
        (
            [x, y, 0.0],
            [x + side, y + side, f32::from(self.height) * px],
        )
    }
}

/// What went into a mesh.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MeshStats {
    pub blocks: usize,
    pub faces: usize,
    pub lids: usize,
    pub flat_blocks: usize,
    pub sloped_blocks: usize,
    /// Side faces naming a mission door (skipped until M7).
    pub door_faces: usize,
    /// Faces naming a tile the style does not have (skipped).
    pub bad_tiles: usize,
    /// Side faces with no height left after the slope (skipped).
    pub degenerate: usize,
}

/// The city's geometry, ready to upload.
#[derive(Debug, Clone, Default)]
pub struct CityMesh {
    pub vertices: Vec<CityVertex>,
    pub indices: Vec<u32>,
    /// Row-major, [`CHUNKS_PER_SIDE`] squared. Empty chunks are kept.
    pub chunks: Vec<Chunk>,
    pub stats: MeshStats,
}

/// How many tiles the style has in each bank: the mesh needs it to tell a
/// lid from a side tile and to reject faces the style cannot draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileBanks {
    pub side: usize,
    pub lid: usize,
}

/// Heights of a block's top corners above its floor, in pixels, in the
/// order north-west, north-east, south-east, south-west.
pub fn slope_corners(slope: u8) -> [u16; 4] {
    // (axis, height at the low-coordinate edge, at the high edge), in eighths
    // of a block. Axis Y: north edge, south edge. Axis X: west edge, east edge.
    // From Carnage3D `PutBlockFace` and AmiGTA1 `slope_heights`, which agree.
    #[derive(PartialEq)]
    enum Axis {
        None,
        X,
        Y,
    }
    let s = slope;
    let (axis, e0, e1) = match s {
        1..=2 => (Axis::Y, (s - 1 + 1) * 4, (s - 1) * 4),
        3..=4 => (Axis::Y, (s - 3) * 4, (s - 3 + 1) * 4),
        5..=6 => (Axis::X, (s - 5 + 1) * 4, (s - 5) * 4),
        7..=8 => (Axis::X, (s - 7) * 4, (s - 7 + 1) * 4),
        9..=16 => (Axis::Y, s - 9 + 1, s - 9),
        17..=24 => (Axis::Y, s - 17, s - 17 + 1),
        25..=32 => (Axis::X, s - 25 + 1, s - 25),
        33..=40 => (Axis::X, s - 33, s - 33 + 1),
        41 => (Axis::Y, 8, 0),
        42 => (Axis::Y, 0, 8),
        43 => (Axis::X, 8, 0),
        44 => (Axis::X, 0, 8),
        _ => (Axis::None, 8, 8),
    };
    let (e0, e1) = (u16::from(e0) * 8, u16::from(e1) * 8);
    match axis {
        Axis::None => [BLOCK_PX; 4],
        Axis::Y => [e0, e0, e1, e1],
        Axis::X => [e0, e1, e1, e0],
    }
}

const NW: usize = 0;
const NE: usize = 1;
const SE: usize = 2;
const SW: usize = 3;

struct Builder<'a> {
    banks: TileBanks,
    mesh: &'a mut CityMesh,
}

/// One quad: four corners clockwise from the top-left as seen from the
/// front, each `(x, y, z, u, v)`.
type Quad = [(u16, u16, u16, u8, u8); 4];

impl Builder<'_> {
    fn push(&mut self, quad: Quad, tile: usize, remap: u8, flags: u8) {
        let base = self.mesh.vertices.len() as u32;
        for (x, y, z, u, v) in quad {
            self.mesh.vertices.push(CityVertex {
                pos: [x, y, z, tile as u16],
                attr: [u, v, remap, flags],
            });
        }
        self.mesh
            .indices
            .extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
        self.mesh.stats.faces += 1;
    }

    /// The linear tile for a side face, or `None` if it cannot be drawn.
    fn side_tile(&mut self, n: u8) -> Option<usize> {
        if n == 0 {
            None
        } else if (n as usize) < self.banks.side {
            Some(n as usize)
        } else {
            self.mesh.stats.door_faces += 1;
            None
        }
    }

    fn block(&mut self, b: &Block, bx: usize, by: usize, bz: usize) {
        let px = BLOCK_PX;
        let (x0, y0, zb) = (bx as u16 * px, by as u16 * px, bz as u16 * px);
        let (x1, y1) = (x0 + px, y0 + px);
        let flat = b.is_flat();
        let slope = if flat { 0 } else { b.slope() };
        let th = slope_corners(slope);
        let top = th.map(|h| zb + h);
        self.mesh.stats.blocks += 1;
        if flat {
            self.mesh.stats.flat_blocks += 1;
        }
        if slope != 0 {
            self.mesh.stats.sloped_blocks += 1;
        }

        let fl = |face: Face| {
            let mut f = (face as u8) << flags::FACE_SHIFT;
            if flat {
                f |= flags::TRANSPARENT | flags::FLAT;
            }
            f
        };
        // A side quad from its top-left corner `a` to top-right corner `b`
        // (each `(x, y, corner)`), as seen from its front.
        let side = |a: (u16, u16, usize), b: (u16, u16, usize), mirror: bool| -> Option<Quad> {
            let (ha, hb) = (th[a.2], th[b.2]);
            if ha == 0 && hb == 0 {
                return None;
            }
            // Keep texture scale: the top `64 - h` rows are cut away.
            let (va, vb) = ((px - ha) as u8, (px - hb) as u8);
            let (ua, ub) = if mirror { (64, 0) } else { (0, 64) };
            Some([
                (a.0, a.1, top[a.2], ua, va),
                (b.0, b.1, top[b.2], ub, vb),
                (b.0, b.1, zb, ub, 64),
                (a.0, a.1, zb, ua, 64),
            ])
        };

        let (lr, tb) = (b.flip_left_right(), b.flip_top_bottom());
        let emit_side = |this: &mut Self, face: Face, quad: Option<Quad>, tile: usize| match quad {
            Some(q) => this.push(q, tile, 0, fl(face)),
            None => this.mesh.stats.degenerate += 1,
        };

        if flat {
            // The plate on the west edge: one quad facing west, one east.
            let w = self.side_tile(b.face(Face::West));
            let e = self.side_tile(b.face(Face::East));
            if let Some(t) = w.or(e) {
                emit_side(self, Face::West, side((x0, y0, NW), (x0, y1, SW), lr), t);
            }
            if let Some(t) = e.or(w) {
                emit_side(self, Face::East, side((x0, y1, SW), (x0, y0, NW), !lr), t);
            }
            // The plate on the north edge: facing north, then south.
            let n = self.side_tile(b.face(Face::North));
            let s = self.side_tile(b.face(Face::South));
            if let Some(t) = n.or(s) {
                emit_side(self, Face::North, side((x1, y0, NE), (x0, y0, NW), tb), t);
            }
            if let Some(t) = s.or(n) {
                emit_side(self, Face::South, side((x0, y0, NW), (x1, y0, NE), !tb), t);
            }
        } else {
            if let Some(t) = self.side_tile(b.face(Face::West)) {
                emit_side(self, Face::West, side((x0, y0, NW), (x0, y1, SW), lr), t);
            }
            if let Some(t) = self.side_tile(b.face(Face::East)) {
                emit_side(self, Face::East, side((x1, y1, SE), (x1, y0, NE), lr), t);
            }
            if let Some(t) = self.side_tile(b.face(Face::North)) {
                emit_side(self, Face::North, side((x1, y0, NE), (x0, y0, NW), tb), t);
            }
            if let Some(t) = self.side_tile(b.face(Face::South)) {
                emit_side(self, Face::South, side((x0, y1, SW), (x1, y1, SE), tb), t);
            }
        }

        let lid = b.face(Face::Lid);
        if lid != 0 {
            if (lid as usize) < self.banks.lid {
                // Texture corners NW, NE, SE, SW; rotation r moves the
                // texture's top-left to corner r, clockwise.
                const TC: [(u8, u8); 4] = [(0, 0), (64, 0), (64, 64), (0, 64)];
                let r = b.lid_rotation() as usize;
                let tc = |corner: usize| TC[(corner + 4 - r) % 4];
                let corner = |c: usize, x: u16, y: u16| {
                    let (u, v) = tc(c);
                    (x, y, top[c], u, v)
                };
                let quad = [
                    corner(NW, x0, y0),
                    corner(NE, x1, y0),
                    corner(SE, x1, y1),
                    corner(SW, x0, y1),
                ];
                let mut f = (Face::Lid as u8) << flags::FACE_SHIFT;
                if flat {
                    f |= flags::TRANSPARENT | flags::FLAT;
                }
                self.push(quad, self.banks.side + lid as usize, b.remap(), f);
                self.mesh.stats.lids += 1;
            } else {
                self.mesh.stats.bad_tiles += 1;
            }
        }
    }
}

impl CityMesh {
    /// Build the whole map, chunk by chunk.
    pub fn build(map: &Cmp, banks: TileBanks) -> Self {
        let mut mesh = CityMesh::default();
        for cy in 0..CHUNKS_PER_SIDE {
            for cx in 0..CHUNKS_PER_SIDE {
                let first_index = mesh.indices.len() as u32;
                let mut height = 0;
                let mut b = Builder {
                    banks,
                    mesh: &mut mesh,
                };
                for by in cy * CHUNK_BLOCKS..(cy + 1) * CHUNK_BLOCKS {
                    for bx in cx * CHUNK_BLOCKS..(cx + 1) * CHUNK_BLOCKS {
                        let h = map.column_height(bx, by);
                        height = height.max(h);
                        for bz in 0..h.min(MAP_LAYERS) {
                            if let Some(block) = map.block(bx, by, bz) {
                                b.block(block, bx, by, bz);
                            }
                        }
                    }
                }
                mesh.chunks.push(Chunk {
                    x: (cx * CHUNK_BLOCKS) as u16,
                    y: (cy * CHUNK_BLOCKS) as u16,
                    height: height as u8,
                    first_index,
                    index_count: mesh.indices.len() as u32 - first_index,
                });
            }
        }
        mesh
    }

    /// Faces of one block alone, for tests and the inspector.
    pub fn of_block(block: &Block, banks: TileBanks, bx: usize, by: usize, bz: usize) -> Self {
        let mut mesh = CityMesh::default();
        Builder {
            banks,
            mesh: &mut mesh,
        }
        .block(block, bx, by, bz);
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANKS: TileBanks = TileBanks { side: 10, lid: 10 };

    fn block(type_map: u16, ext: u8, faces: [u8; 5]) -> Block {
        Block {
            type_map,
            type_map_ext: ext,
            faces,
        }
    }

    type Corners = Vec<(u16, u16, u16, u8, u8)>;

    /// The quad for one face, as `(x, y, z, u, v)` corners.
    fn quads(m: &CityMesh) -> Vec<Corners> {
        m.vertices
            .chunks(4)
            .map(|q| {
                q.iter()
                    .map(|v| (v.pos[0], v.pos[1], v.pos[2], v.attr[0], v.attr[1]))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn slopes_match_both_references() {
        assert_eq!(slope_corners(0), [64; 4]);
        // 26°, north high: the lower half rises from 0 (south) to 32 (north).
        assert_eq!(slope_corners(1), [32, 32, 0, 0]);
        assert_eq!(slope_corners(2), [64, 64, 32, 32]);
        assert_eq!(slope_corners(4), [32, 32, 64, 64]);
        // 26°, west high.
        assert_eq!(slope_corners(5), [32, 0, 0, 32]);
        // 7°: eighths.
        assert_eq!(slope_corners(9), [8, 8, 0, 0]);
        assert_eq!(slope_corners(40), [56, 64, 64, 56]);
        // 45°.
        assert_eq!(slope_corners(41), [64, 64, 0, 0]);
        assert_eq!(slope_corners(44), [0, 64, 64, 0]);
    }

    #[test]
    fn a_cube_has_outward_opaque_faces() {
        // W=1 E=2 N=3 S=4 lid=5, at block (1, 2, 0).
        let m = CityMesh::of_block(&block(0, 0, [1, 2, 3, 4, 5]), BANKS, 1, 2, 0);
        assert_eq!(m.stats.faces, 5);
        let q = quads(&m);
        // West face seen from the west: north on the left.
        assert_eq!(
            q[0],
            [
                (64, 128, 64, 0, 0),
                (64, 192, 64, 64, 0),
                (64, 192, 0, 64, 64),
                (64, 128, 0, 0, 64)
            ]
        );
        // East face seen from the east: south on the left.
        assert_eq!(q[1][0], (128, 192, 64, 0, 0));
        // Lid: the north-west corner has the texture's top-left.
        assert_eq!(q[4][0], (64, 128, 64, 0, 0));
        assert_eq!(
            m.vertices[16].pos[3],
            10 + 5,
            "lid tiles follow the side bank"
        );
        assert!(
            m.vertices
                .iter()
                .all(|v| v.attr[3] & flags::TRANSPARENT == 0)
        );
    }

    #[test]
    fn lids_rotate_clockwise_and_carry_the_remap() {
        // rotation 1 (bits 14..15), remap 2 (ext bits 3..4).
        let m = CityMesh::of_block(&block(1 << 14, 2 << 3, [0, 0, 0, 0, 5]), BANKS, 0, 0, 0);
        let q = quads(&m);
        // NE corner now holds the texture's top-left.
        assert_eq!((q[0][1].3, q[0][1].4), (0, 0));
        assert_eq!((q[0][0].3, q[0][0].4), (0, 64));
        assert_eq!(m.vertices[0].attr[2], 2);
    }

    #[test]
    fn flat_blocks_are_plates_that_read_the_same_both_ways() {
        // Flat (0x80), only a west tile: it shows from both sides, on x0.
        let m = CityMesh::of_block(&block(0x80, 0, [3, 0, 0, 0, 7]), BANKS, 1, 0, 0);
        let q = quads(&m);
        assert_eq!(m.stats.faces, 3);
        assert!(
            q[0].iter().chain(&q[1]).all(|c| c.0 == 64),
            "both on the west plane"
        );
        // Same world orientation: the north end has u = 0 on both quads.
        let u_at_north = |quad: &Corners| quad.iter().find(|c| c.1 == 0 && c.2 == 64).unwrap().3;
        assert_eq!(u_at_north(&q[0]), 0);
        assert_eq!(u_at_north(&q[1]), 0);
        // The lid is on top and see-through.
        assert!(q[2].iter().all(|c| c.2 == 64));
        assert_ne!(m.vertices[8].attr[3] & flags::TRANSPARENT, 0);
        assert!(m.vertices.iter().all(|v| v.attr[3] & flags::FLAT != 0));
    }

    #[test]
    fn slopes_cut_side_faces_and_skip_empty_ones() {
        // Slope 41: north edge at full height, south edge at 0.
        let m = CityMesh::of_block(&block(41 << 8, 0, [1, 2, 3, 4, 5]), BANKS, 0, 0, 0);
        assert_eq!(m.stats.degenerate, 1, "the south face has no height");
        let q = quads(&m);
        // West face: full height at the north end, nothing at the south end;
        // the texture is cut, not squashed.
        assert_eq!(q[0][0], (0, 0, 64, 0, 0));
        assert_eq!(q[0][1], (0, 64, 0, 64, 64));
    }

    #[test]
    fn doors_and_missing_tiles_are_counted_not_drawn() {
        let m = CityMesh::of_block(&block(0, 0, [12, 0, 0, 0, 11]), BANKS, 0, 0, 0);
        assert_eq!(m.stats.faces, 0);
        assert_eq!(m.stats.door_faces, 1);
        assert_eq!(m.stats.bad_tiles, 1);
    }
}
