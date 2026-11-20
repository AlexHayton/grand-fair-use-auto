//! The logical sim view: the area around the player that counts as "on
//! screen" to the simulation.
//!
//! AmiGTA1 derives its spawn rings and on-screen checks from the window
//! width and camera zoom, so a wider window or a zoomed-out camera changes
//! what spawns where, and replays stop matching. Here the simulation uses a
//! fixed rectangle instead (Principle 4). The renderer may show more or
//! less than this; gameplay never reads the window, the zoom or the HD
//! setting.
//!
//! A later milestone may make the view depend on sim state (the original
//! zooms out with the car's speed), but never on the renderer.

use gfa_fixed::fx::{BLOCK_SHIFT, FP};

/// Half-extents of the view, in blocks, around its centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimView {
    pub half_w: i32,
    pub half_h: i32,
}

impl SimView {
    /// What AmiGTA1 uses at its default zoom (256 px wide at 32 px a
    /// block): half-width 5, half-height 4. Use it to compare with the
    /// AmiGTA1 oracle.
    pub const AMIGTA1: SimView = SimView {
        half_w: 5,
        half_h: 4,
    };

    /// The game's view. It covers a 21:9 window at the follow camera's
    /// default height (8 blocks, 55 degree field of view: 9.7 blocks either
    /// side and 4.2 above and below), rounded up.
    pub const GAME: SimView = SimView {
        half_w: 10,
        half_h: 6,
    };

    /// Is block `(bx, by)` inside the view centred on the 16.16 point
    /// `(cx, cy)`?
    pub fn contains_block(&self, cx: i32, cy: i32, bx: i32, by: i32) -> bool {
        let (ox, oy) = (cx >> (FP + BLOCK_SHIFT), cy >> (FP + BLOCK_SHIFT));
        (bx - ox).abs() <= self.half_w && (by - oy).abs() <= self.half_h
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gfa_fixed::fx::block_centre;

    #[test]
    fn contains() {
        let (cx, cy) = (block_centre(100), block_centre(50));
        let v = SimView::AMIGTA1;
        assert!(v.contains_block(cx, cy, 105, 46));
        assert!(!v.contains_block(cx, cy, 106, 50));
        assert!(!v.contains_block(cx, cy, 100, 55));
    }
}
