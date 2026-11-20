//! A style's art as the GPU sees it, built on the CPU.
//!
//! GRY and G24 colour work differently: G24 has hundreds of palettes and
//! picks one per tile and remap; GRY has one palette and 256 remap tables
//! that translate colour indices. Both become the same thing here, a list of
//! 256-colour RGBA palettes and a table of which palette each tile uses
//! under each lid remap. GRY's palettes are its remap tables applied to its
//! one palette, so palette `k` is table `k` (table 0 is the identity).
//!
//! Tiles go into one 8-bit atlas, 32 tiles across, so the GPU reads them with
//! `textureLoad` and colours them in the shader.

use gfa_formats::style::{Colour, Style, TILE_DIM, TileKind};

/// Width of every table texture.
pub const ATLAS_WIDTH: usize = 2048;
pub const ATLAS_TILES_PER_ROW: usize = ATLAS_WIDTH / TILE_DIM;
/// Palettes laid side by side in each row of the palette texture.
pub const PALETTES_PER_ROW: usize = ATLAS_WIDTH / 256;
/// Width of the small lookup tables (animation, tile palettes).
pub const TABLE_WIDTH: usize = 256;

pub type Rgba = [u8; 4];

/// The style's colours and tiles in GPU-ready form.
#[derive(Debug, Clone)]
pub struct StyleTables {
    /// 256 colours each. Alpha is always 255; colour 0's see-through-ness is
    /// decided per face, not by the palette.
    pub palettes: Vec<[Rgba; 256]>,
    /// Per linear tile, the palette for lid remaps 0..=3.
    pub tile_palettes: Vec<[u16; 4]>,
    /// The tile atlas: [`ATLAS_WIDTH`] wide, palette indices.
    pub atlas: Vec<u8>,
    pub atlas_height: usize,
    pub tiles: usize,
}

impl StyleTables {
    pub fn new(style: &Style) -> Self {
        let tiles = style.tile_total();
        let (palettes, tile_palettes) = match &style.colour {
            Colour::G24(c) => {
                let palettes = c
                    .cluts
                    .iter()
                    .map(|p| p.map(|[r, g, b, _]| [r, g, b, 255]))
                    .collect();
                let tile_palettes = (0..tiles)
                    .map(|t| std::array::from_fn(|r| c.tile_palette(t, r as u8).unwrap_or(0)))
                    .collect();
                (palettes, tile_palettes)
            }
            Colour::Gry(c) => {
                let palettes = c
                    .remaps
                    .iter()
                    .map(|table| {
                        std::array::from_fn(|i| {
                            let [r, g, b] = c.rgb8(table[i]);
                            [r, g, b, 255]
                        })
                    })
                    .collect();
                // Lid remaps come from the lid remap table; sides and aux
                // tiles are never remapped.
                let lid0 = style.tile_linear(TileKind::Lid, 0).unwrap_or(usize::MAX);
                let tile_palettes = (0..tiles)
                    .map(|t| {
                        t.checked_sub(lid0)
                            .filter(|&n| n < style.lid_tiles)
                            .and_then(|n| c.lid_remaps.get(n))
                            .map_or([0; 4], |row| row.map(u16::from))
                    })
                    .collect();
                (palettes, tile_palettes)
            }
        };

        let rows = tiles.div_ceil(ATLAS_TILES_PER_ROW).max(1);
        let atlas_height = rows * TILE_DIM;
        let mut atlas = vec![0u8; ATLAS_WIDTH * atlas_height];
        for t in 0..tiles {
            let img = style.tile(t).expect("tile in range");
            let (ox, oy) = atlas_origin(t);
            for y in 0..TILE_DIM {
                let at = (oy + y) * ATLAS_WIDTH + ox;
                atlas[at..at + TILE_DIM].copy_from_slice(img.row(y));
            }
        }
        StyleTables {
            palettes,
            tile_palettes,
            atlas,
            atlas_height,
            tiles,
        }
    }

    /// The palette texture: [`PALETTES_PER_ROW`] palettes per row of
    /// [`ATLAS_WIDTH`] RGBA texels. Returns `(texels, rows)`.
    pub fn palette_texels(&self) -> (Vec<u8>, usize) {
        let rows = self.palettes.len().div_ceil(PALETTES_PER_ROW).max(1);
        let mut out = vec![0u8; ATLAS_WIDTH * 4 * rows];
        for (p, pal) in self.palettes.iter().enumerate() {
            let at = ((p / PALETTES_PER_ROW) * ATLAS_WIDTH + (p % PALETTES_PER_ROW) * 256) * 4;
            out[at..at + 1024].copy_from_slice(pal.as_flattened());
        }
        (out, rows)
    }

    /// The tile palette table as `Rgba16Uint` texels, [`TABLE_WIDTH`] wide.
    pub fn tile_palette_texels(&self) -> (Vec<u16>, usize) {
        let rows = table_rows(self.tiles);
        let mut out = vec![0u16; TABLE_WIDTH * 4 * rows];
        out[..self.tiles * 4].copy_from_slice(self.tile_palettes.as_flattened());
        (out, rows)
    }

    /// The colour of one pixel of tile `t` under remap `r`, for tests and
    /// headless checks.
    pub fn tile_rgb(&self, t: usize, r: usize, x: usize, y: usize) -> Rgba {
        let (ox, oy) = atlas_origin(t);
        let i = self.atlas[(oy + y) * ATLAS_WIDTH + ox + x];
        self.palettes[self.tile_palettes[t][r] as usize][i as usize]
    }
}

/// Top-left texel of tile `t` in the atlas.
pub fn atlas_origin(t: usize) -> (usize, usize) {
    (
        (t % ATLAS_TILES_PER_ROW) * TILE_DIM,
        (t / ATLAS_TILES_PER_ROW) * TILE_DIM,
    )
}

pub fn table_rows(entries: usize) -> usize {
    entries.div_ceil(TABLE_WIDTH).max(1)
}

/// Rate of the original's game frames, which tile animations count in.
/// AmiGTA1 reads the original's frame as 25 Hz (`gta_main.c`, doors: "15 of
/// the original's 25 Hz"); Carnage3D uses 24. Render-side only.
pub const ORIGINAL_FRAME_HZ: f64 = 25.0;

/// Animated tiles: which tile each tile shows at a given moment.
///
/// Each animation shows its own tile, then each of its aux frames, for
/// `speed` game frames apiece, and loops (Carnage3D `InitBlocksAnimations`).
#[derive(Debug, Clone, Default)]
pub struct TileAnimator {
    /// `(tile, frames including the tile itself, game frames per frame)`.
    anims: Vec<(usize, Vec<u16>, u32)>,
    table: Vec<u16>,
}

impl TileAnimator {
    pub fn new(style: &Style) -> Self {
        let anims = style
            .animations
            .iter()
            .filter_map(|a| {
                let tile = style.tile_linear(a.kind()?, a.tile as usize)?;
                let mut frames = vec![tile as u16];
                for &f in &a.frames {
                    frames.push(style.tile_linear(TileKind::Aux, f as usize)? as u16);
                }
                Some((tile, frames, u32::from(a.speed.max(1))))
            })
            .collect();
        let tiles = style.tile_total();
        let mut table = vec![0u16; TABLE_WIDTH * table_rows(tiles)];
        for (t, v) in table.iter_mut().enumerate().take(tiles) {
            *v = t as u16;
        }
        TileAnimator { anims, table }
    }

    /// Set the table for game frame `frame`. Returns whether it changed.
    pub fn set_frame(&mut self, frame: u64) -> bool {
        let mut changed = false;
        for (tile, frames, speed) in &self.anims {
            let i = (frame / u64::from(*speed)) % frames.len() as u64;
            let now = frames[i as usize];
            if self.table[*tile] != now {
                self.table[*tile] = now;
                changed = true;
            }
        }
        changed
    }

    /// `R16Uint` texels, [`TABLE_WIDTH`] wide.
    pub fn texels(&self) -> &[u16] {
        &self.table
    }

    pub fn rows(&self) -> usize {
        self.table.len() / TABLE_WIDTH
    }

    pub fn len(&self) -> usize {
        self.anims.len()
    }

    pub fn is_empty(&self) -> bool {
        self.anims.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_layout() {
        assert_eq!(atlas_origin(0), (0, 0));
        assert_eq!(atlas_origin(33), (64, 64));
        assert_eq!(table_rows(0), 1);
        assert_eq!(table_rows(257), 2);
    }
}
