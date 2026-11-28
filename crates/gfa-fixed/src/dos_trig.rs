//! GTA8.EXE's own trigonometry: 1024 steps a turn, 16.16 values.
//!
//! An angle of 0 points north (up the screen), 256 west, 512 south and 768
//! east: the angle grows counter-clockwise as seen on screen, and GTA8.EXE
//! turns a sprite's art counter-clockwise by it. A unit step along angle `a`
//! is `(-sin a, -cos a)` in screen axes (x right, y down). This is not
//! [`crate::trig`]'s circle, which is AmiGTA1's (256 steps, clockwise).
//!
//! The tables are built at run time by the original. Both are generated
//! here, not copied, and were checked against a RAM dump of GTA8.EXE in
//! play (`tools/dosprobe/scripts/mission_arrow.probe`):
//! - sine (0x1934a4, 1280 entries so that the cosine is the sine read a
//!   quarter turn on): `trunc(sin(2*pi*i/1024) * 65536)`. It is symmetric,
//!   so a quarter wave is kept;
//! - tangent (0x1948a4, 256 entries over a quarter turn):
//!   `trunc(tan(pi*i/512) * 65536)`, except that entry 128 is exactly 1.0.
//!
//! The tests rebuild both with floats and compare.

/// Steps in a full turn.
pub const TURN: i32 = 1024;
/// 1.0 in 16.16.
pub const ONE: i32 = 1 << 16;

/// `trunc(sin(2*pi*i/1024) * 65536)` for i in 0..=256.
#[rustfmt::skip]
const QUARTER_SIN: [i32; 257] = [
    0, 402, 804, 1206, 1608, 2010, 2412, 2814,
    3215, 3617, 4018, 4420, 4821, 5222, 5622, 6023,
    6423, 6823, 7223, 7623, 8022, 8421, 8819, 9218,
    9616, 10013, 10410, 10807, 11204, 11600, 11995, 12390,
    12785, 13179, 13573, 13966, 14359, 14751, 15142, 15533,
    15923, 16313, 16702, 17091, 17479, 17866, 18253, 18638,
    19024, 19408, 19792, 20175, 20557, 20938, 21319, 21699,
    22078, 22456, 22833, 23210, 23586, 23960, 24334, 24707,
    25079, 25450, 25820, 26189, 26557, 26925, 27291, 27656,
    28020, 28383, 28745, 29105, 29465, 29824, 30181, 30538,
    30893, 31247, 31600, 31952, 32302, 32651, 32999, 33346,
    33692, 34036, 34379, 34721, 35061, 35400, 35738, 36074,
    36409, 36743, 37075, 37406, 37736, 38064, 38390, 38716,
    39039, 39362, 39682, 40002, 40319, 40636, 40950, 41263,
    41575, 41885, 42194, 42501, 42806, 43110, 43412, 43712,
    44011, 44308, 44603, 44897, 45189, 45480, 45768, 46055,
    46340, 46624, 46906, 47186, 47464, 47740, 48015, 48288,
    48558, 48828, 49095, 49360, 49624, 49886, 50146, 50403,
    50660, 50914, 51166, 51416, 51665, 51911, 52155, 52398,
    52639, 52877, 53114, 53348, 53581, 53811, 54040, 54266,
    54491, 54713, 54933, 55152, 55368, 55582, 55794, 56004,
    56212, 56417, 56621, 56822, 57022, 57219, 57414, 57606,
    57797, 57986, 58172, 58356, 58538, 58718, 58895, 59070,
    59243, 59414, 59583, 59749, 59913, 60075, 60235, 60392,
    60547, 60700, 60850, 60998, 61144, 61288, 61429, 61568,
    61705, 61839, 61971, 62100, 62228, 62353, 62475, 62596,
    62714, 62829, 62942, 63053, 63162, 63268, 63371, 63473,
    63571, 63668, 63762, 63854, 63943, 64030, 64115, 64197,
    64276, 64353, 64428, 64501, 64571, 64638, 64703, 64766,
    64826, 64884, 64939, 64992, 65043, 65091, 65136, 65179,
    65220, 65258, 65294, 65327, 65358, 65386, 65412, 65436,
    65457, 65475, 65491, 65505, 65516, 65524, 65531, 65534,
    65536,
];

/// `trunc(tan(pi*i/512) * 65536)` for i in 0..256, but exactly 1.0 at 128.
#[rustfmt::skip]
const TAN: [i32; 256] = [
    0, 402, 804, 1206, 1608, 2011, 2413, 2816,
    3219, 3622, 4026, 4430, 4834, 5238, 5643, 6048,
    6454, 6861, 7267, 7675, 8083, 8491, 8900, 9310,
    9721, 10132, 10544, 10957, 11371, 11786, 12201, 12618,
    13035, 13454, 13874, 14294, 14716, 15139, 15563, 15989,
    16415, 16843, 17273, 17704, 18136, 18569, 19005, 19441,
    19880, 20320, 20761, 21205, 21650, 22097, 22545, 22996,
    23449, 23903, 24360, 24819, 25280, 25743, 26208, 26675,
    27145, 27618, 28092, 28570, 29050, 29532, 30017, 30505,
    30996, 31489, 31986, 32485, 32988, 33493, 34002, 34514,
    35029, 35548, 36070, 36596, 37125, 37658, 38195, 38736,
    39280, 39829, 40382, 40939, 41500, 42065, 42635, 43210,
    43789, 44373, 44962, 45556, 46155, 46759, 47369, 47984,
    48604, 49230, 49862, 50500, 51144, 51794, 52451, 53114,
    53784, 54460, 55143, 55834, 56531, 57236, 57949, 58669,
    59398, 60134, 60879, 61633, 62395, 63166, 63946, 64736,
    65536, 66345, 67164, 67994, 68834, 69685, 70548, 71422,
    72307, 73205, 74115, 75038, 75974, 76923, 77886, 78864,
    79855, 80862, 81884, 82922, 83976, 85047, 86135, 87241,
    88365, 89507, 90669, 91851, 93053, 94277, 95522, 96790,
    98081, 99396, 100736, 102101, 103492, 104911, 106358, 107834,
    109340, 110877, 112446, 114049, 115687, 117360, 119071, 120820,
    122609, 124439, 126313, 128232, 130197, 132211, 134275, 136392,
    138564, 140792, 143081, 145431, 147846, 150329, 152883, 155512,
    158217, 161005, 163877, 166839, 169895, 173050, 176309, 179677,
    183160, 186765, 190499, 194367, 198380, 202544, 206869, 211365,
    216043, 220913, 225990, 231286, 236816, 242597, 248647, 254986,
    261634, 268616, 275959, 283691, 291845, 300457, 309567, 319222,
    329471, 340373, 351993, 364404, 377693, 391956, 407305, 423871,
    441807, 461291, 482534, 505787, 531351, 559593, 590957, 625996,
    665398, 710035, 761030, 819849, 888449, 969498, 1066729, 1185538,
    1334015, 1524876, 1779313, 2135471, 2669640, 3559833, 5340085, 10680573,
];

/// Sine of an angle, 16.16. Any integer wraps.
pub fn sin(a: i32) -> i32 {
    let a = a & (TURN - 1);
    match a {
        0..256 => QUARTER_SIN[a as usize],
        256..512 => QUARTER_SIN[(512 - a) as usize],
        512..768 => -QUARTER_SIN[(a - 512) as usize],
        _ => -QUARTER_SIN[(1024 - a) as usize],
    }
}

/// Cosine of an angle, 16.16: the sine a quarter turn on, as GTA8.EXE reads
/// it (the table at 0x1938a4 is 0x1934a4's from entry 256).
pub fn cos(a: i32) -> i32 {
    sin(a.wrapping_add(256))
}

/// 0x1039c: the steps of a quarter turn (0 to 255) whose tangent is `x`
/// (16.16, not negative), by a binary search of the tangent table.
///
/// Without an exact match it gives the first entry above `x`, so it rounds
/// up, with the original's two edge cases. A negative `x`, which the
/// original never passes (it would read outside the table), gives 0.
fn atan_steps(x: i32) -> i32 {
    // eax and ecx: compared unsigned (`jbe`), so `hi` may wrap below 0.
    let (mut lo, mut hi) = (0u32, 255u32);
    let mut mid;
    loop {
        mid = lo.wrapping_add(hi) >> 1;
        let Some(&t) = TAN.get(mid as usize) else {
            return 0;
        };
        if x < t {
            hi = mid.wrapping_sub(1);
        } else if x == t {
            return mid as i32;
        } else {
            lo = mid + 1;
        }
        if lo > hi {
            break;
        }
    }
    if mid == 1 && x < TAN[1] {
        return 0;
    }
    if mid == 255 && x as u32 > TAN[255] as u32 {
        return 255;
    }
    lo as i32
}

/// 0x1040e: the angle from `from` towards `to`, both 16.16 pixels, as
/// GTA8.EXE works it out (it is passed `from − to` on each axis).
///
/// Straight up or down the screen (less than a whole pixel across) it gives
/// 0 or 512, and straight across 256 or 768. Otherwise it divides the y
/// difference (16.16) by the whole pixels of the x difference and looks the
/// tangent up ([`atan_steps`]).
pub fn towards(from: (i32, i32), to: (i32, i32)) -> i32 {
    let a = from.1.wrapping_sub(to.1);
    let b = from.0.wrapping_sub(to.0);
    let (hy, hx) = (a >> 16, b >> 16);
    if hx == 0 {
        return if a < 0 { 0x200 } else { 0 };
    }
    if hy == 0 {
        return if hx < 0 { 0x300 } else { 0x100 };
    }
    let q = a.wrapping_div(hx);
    let t = atan_steps(if q < 0 { q.wrapping_neg() } else { q });
    match (hy >= 0, hx >= 0) {
        (true, true) => 0x100 - t,
        (true, false) if t == 0x100 => 0,
        (true, false) => t + 0x300,
        (false, true) => t + 0x100,
        (false, false) => 0x300 - t,
    }
}

#[cfg(test)]
#[allow(clippy::float_arithmetic)]
mod tests {
    extern crate std;
    use super::*;
    use core::f64::consts::PI;

    #[test]
    fn tables_are_the_formulas() {
        for i in 0..1280 {
            let want = ((2.0 * PI * f64::from(i) / 1024.0).sin() * 65536.0) as i32;
            assert_eq!(sin(i), want, "sin[{i}]");
        }
        for (i, &t) in TAN.iter().enumerate() {
            let want = match i {
                128 => ONE,
                _ => ((PI * i as f64 / 512.0).tan() * 65536.0) as i32,
            };
            assert_eq!(t, want, "tan[{i}]");
        }
    }

    #[test]
    fn compass() {
        assert_eq!(sin(256), ONE);
        assert_eq!(cos(0), ONE);
        assert_eq!(cos(512), -ONE);
        assert_eq!(sin(-256), -ONE);
        let p = (100 << 16, 100 << 16);
        assert_eq!(towards(p, (100 << 16, 50 << 16)), 0, "north");
        assert_eq!(towards(p, (50 << 16, 100 << 16)), 256, "west");
        assert_eq!(towards(p, (100 << 16, 150 << 16)), 512, "south");
        assert_eq!(towards(p, (150 << 16, 100 << 16)), 768, "east");
        assert_eq!(towards(p, (50 << 16, 50 << 16)), 128, "north-west");
    }

    #[test]
    fn a_step_along_the_angle_reaches_the_target() {
        // GTA8.EXE stands the arrow at `from - (sin a, cos a) * r`.
        let from = (6752 << 16, 7648 << 16);
        for to in [(7935, 8736), (6000, 7000), (7000, 6000), (6100, 9000)] {
            let to = (to.0 << 16, to.1 << 16);
            let a = towards(from, to);
            let (dx, dy) = (-sin(a), -cos(a));
            let (wx, wy) = ((to.0 - from.0) >> 16, (to.1 - from.1) >> 16);
            // The step and the way to the target agree in sign on both axes.
            assert!(
                i64::from(dx) * i64::from(wx) >= 0 && i64::from(dy) * i64::from(wy) >= 0,
                "{to:?}: {a}"
            );
        }
    }

    /// Liberty City's opening, from mission_arrow.probe's dump: the player
    /// at (6752, 7648), the arrow on `137 TARGET` at (7935, 8736). The arrow
    /// object stood at angle 646, 51 pixels out.
    #[test]
    fn the_dumped_arrow() {
        let (from, to) = ((6752 << 16, 7648 << 16), (7935 << 16, 8736 << 16));
        let a = towards(from, to);
        assert_eq!(a, 646);
        assert_eq!(from.0 - sin(a) * 51, 444_947_837);
        assert_eq!(from.1 - cos(a) * 51, 503_494_081);
        assert_eq!(towards((6792 << 16, 7648 << 16), to), 644);
    }
}
