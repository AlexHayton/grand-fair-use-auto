//! 16.16 fixed point in AmiGTA1's world units: 32 world pixels to a block,
//! half the original's 64.
//!
//! Values are plain `i32`, as AmiGTA1's `long` is on the Amiga, so ported
//! arithmetic reads the same. Use `i64` for intermediates that could
//! overflow, and `wrapping_*` where AmiGTA1 relies on 32-bit wrap.

/// Fraction bits.
pub const FP: u32 = 16;
/// 1.0.
pub const ONE: i32 = 1 << FP;
/// World pixels per block, as a shift.
pub const BLOCK_SHIFT: u32 = 5;
/// World pixels per block.
pub const BLOCK_PX: i32 = 1 << BLOCK_SHIFT;
/// The original's (and the renderer's) pixels per world pixel.
pub const RENDER_SCALE: i32 = 2;

/// A whole number of world pixels as 16.16.
#[inline]
pub const fn px(n: i32) -> i32 {
    n << FP
}

/// The block a 16.16 world coordinate falls in (floors negatives).
#[inline]
pub const fn block_of(v: i32) -> i32 {
    v >> (FP + BLOCK_SHIFT)
}

/// The centre of block `b`, as 16.16.
#[inline]
pub const fn block_centre(b: i32) -> i32 {
    (b * BLOCK_PX + BLOCK_PX / 2) << FP
}

/// Whole world pixels of a 16.16 value (floors negatives).
#[inline]
pub const fn whole(v: i32) -> i32 {
    v >> FP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks() {
        assert_eq!(block_of(block_centre(62)), 62);
        assert_eq!(block_of(px(-1)), -1);
        assert_eq!(block_centre(0), px(16));
        assert_eq!(whole(px(7) + 0xFFFF), 7);
    }
}
