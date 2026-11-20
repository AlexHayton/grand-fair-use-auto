//! Q14 trigonometry on a 256-step circle.
//!
//! Angles: a full turn is 256 steps; 0 is north (up the screen, -y), 64 east,
//! 128 south, 192 west. They grow clockwise as seen on screen, so the unit
//! vector for angle `a` is `(sin a, -cos a)`. Fractional angles are 16.16 of
//! the same circle (`Angle16`).
//!
//! After AmiGTA1 `gta_trig.c` (MIT). Both tables are generated, not copied:
//! - cosine: `round(cos(2*pi*i/256) * 16384)` for i in 0..256;
//! - arctangent: `round(atan(i/64) * 256/(2*pi) * 65536)` for i in 0..=64.
//!
//! The tests rebuild them with floats and compare.

/// Steps in a full turn.
pub const ANGLE_STEPS: i32 = 256;
/// 1.0 in Q14.
pub const ONE_Q14: i32 = 16384;

/// `cos(2*pi*i/256)` in Q14.
#[rustfmt::skip]
pub const COS_Q14: [i16; 256] = [
    16384, 16379, 16364, 16340, 16305, 16261, 16207, 16143,
    16069, 15986, 15893, 15791, 15679, 15557, 15426, 15286,
    15137, 14978, 14811, 14635, 14449, 14256, 14053, 13842,
    13623, 13395, 13160, 12916, 12665, 12406, 12140, 11866,
    11585, 11297, 11003, 10702, 10394, 10080, 9760, 9434,
    9102, 8765, 8423, 8076, 7723, 7366, 7005, 6639,
    6270, 5897, 5520, 5139, 4756, 4370, 3981, 3590,
    3196, 2801, 2404, 2006, 1606, 1205, 804, 402,
    0, -402, -804, -1205, -1606, -2006, -2404, -2801,
    -3196, -3590, -3981, -4370, -4756, -5139, -5520, -5897,
    -6270, -6639, -7005, -7366, -7723, -8076, -8423, -8765,
    -9102, -9434, -9760, -10080, -10394, -10702, -11003, -11297,
    -11585, -11866, -12140, -12406, -12665, -12916, -13160, -13395,
    -13623, -13842, -14053, -14256, -14449, -14635, -14811, -14978,
    -15137, -15286, -15426, -15557, -15679, -15791, -15893, -15986,
    -16069, -16143, -16207, -16261, -16305, -16340, -16364, -16379,
    -16384, -16379, -16364, -16340, -16305, -16261, -16207, -16143,
    -16069, -15986, -15893, -15791, -15679, -15557, -15426, -15286,
    -15137, -14978, -14811, -14635, -14449, -14256, -14053, -13842,
    -13623, -13395, -13160, -12916, -12665, -12406, -12140, -11866,
    -11585, -11297, -11003, -10702, -10394, -10080, -9760, -9434,
    -9102, -8765, -8423, -8076, -7723, -7366, -7005, -6639,
    -6270, -5897, -5520, -5139, -4756, -4370, -3981, -3590,
    -3196, -2801, -2404, -2006, -1606, -1205, -804, -402,
    0, 402, 804, 1205, 1606, 2006, 2404, 2801,
    3196, 3590, 3981, 4370, 4756, 5139, 5520, 5897,
    6270, 6639, 7005, 7366, 7723, 8076, 8423, 8765,
    9102, 9434, 9760, 10080, 10394, 10702, 11003, 11297,
    11585, 11866, 12140, 12406, 12665, 12916, 13160, 13395,
    13623, 13842, 14053, 14256, 14449, 14635, 14811, 14978,
    15137, 15286, 15426, 15557, 15679, 15791, 15893, 15986,
    16069, 16143, 16207, 16261, 16305, 16340, 16364, 16379,];

/// `atan(i/64)` in 16.16 steps of the 256-step circle, i in 0..=64.
#[rustfmt::skip]
const ATAN16: [i32; 65] = [
    0, 41718, 83416, 125073, 166669, 208185, 249600, 290894,
    332050, 373047, 413869, 454496, 494912, 535100, 575043, 614727,
    654136, 693257, 732076, 770579, 808756, 846595, 884085, 921217,
    957981, 994370, 1030375, 1065990, 1101209, 1136026, 1170436, 1204436,
    1238021, 1271189, 1303938, 1336265, 1368170, 1399652, 1430711, 1461346,
    1491559, 1521350, 1550722, 1579676, 1608214, 1636338, 1664052, 1691359,
    1718262, 1744764, 1770869, 1796582, 1821906, 1846846, 1871405, 1895590,
    1919403, 1942851, 1965938, 1988668, 2011047, 2033080, 2054772, 2076127,
    2097152,];

/// Cosine of a whole-step angle, Q14. Any integer wraps.
#[inline]
pub fn cos(a: i32) -> i32 {
    i32::from(COS_Q14[(a & 255) as usize])
}

/// Sine of a whole-step angle, Q14: the cosine table read a quarter turn on.
#[inline]
pub fn sin(a: i32) -> i32 {
    i32::from(COS_Q14[(a.wrapping_sub(64) & 255) as usize])
}

/// Cosine of a 16.16 angle, linearly interpolated, Q14.
///
/// The worst interpolation error is under a thousandth of the table's
/// largest step. AmiGTA1 needs it for the car's wheels: whole steps make
/// the turning circle grow as the car slows.
pub fn cos16(a16: i32) -> i32 {
    let i = ((a16 >> 16) & 255) as usize;
    let f = a16 & 0xFFFF;
    let c0 = i32::from(COS_Q14[i]);
    let c1 = i32::from(COS_Q14[(i + 1) & 255]);
    // (c1 - c0) is at most 402 in magnitude, so the product fits in i32.
    c0 + (((c1 - c0) * f) >> 16)
}

/// Sine of a 16.16 angle, Q14.
pub fn sin16(a16: i32) -> i32 {
    cos16(a16.wrapping_sub(64 << 16))
}

/// The heading of a vector in screen axes (x right, y DOWN), as a 16.16
/// angle in `0..256 << 16`. `(0, 0)` gives 0.
///
/// Interpolates inside a 65-entry arctangent table: one divide, and finer
/// than the original's 1024-step circle.
pub fn dir16(vx: i32, vy: i32) -> i32 {
    // i64 throughout: AmiGTA1 relies on 32-bit `long` and keeps every
    // intermediate small enough; the wider type gives the same answers.
    let mut x = i64::from(vx);
    // y now points north, like the table.
    let mut y = -i64::from(vy);
    let neg_x = x < 0;
    let neg_y = y < 0;
    x = x.abs();
    y = y.abs();
    if x == 0 && y == 0 {
        return 0;
    }
    // Keep both inside 15 bits, exactly as AmiGTA1 does, so the result is
    // bit-identical to it.
    while x > 0x7FFF || y > 0x7FFF {
        x >>= 1;
        y >>= 1;
    }
    let lookup = |num: i64, den: i64| {
        let q = (num << 6) / den;
        let rem = (num << 6) - q * den;
        let mut a = i64::from(ATAN16[q as usize]);
        if q < 64 {
            let step = i64::from(ATAN16[q as usize + 1]) - a;
            a += step * rem / den;
        }
        a
    };
    let a = if x <= y {
        lookup(x, y)
    } else {
        // Mirror about the 45 degree line.
        (64 << 16) - lookup(y, x)
    };
    // `a` is 0..64 steps from north towards east; place it in its quadrant.
    let full = match (neg_x, neg_y) {
        (false, false) => a,
        (false, true) => (128 << 16) - a,
        (true, true) => (128 << 16) + a,
        (true, false) => ((256 << 16) - a) & 0xFF_FFFF,
    };
    full as i32
}

/// The compass point a movement is mostly along: 0 N, 64 E, 128 S, 192 W.
/// Ties go to the x axis.
pub fn step_dir(dx: i32, dy: i32) -> i32 {
    if dx.unsigned_abs() >= dy.unsigned_abs() {
        if dx >= 0 { 64 } else { 192 }
    } else if dy >= 0 {
        128
    } else {
        0
    }
}

#[cfg(test)]
#[allow(clippy::float_arithmetic)]
mod tests {
    extern crate std;
    use super::*;
    use core::f64::consts::TAU;

    #[test]
    fn tables_are_the_formulas() {
        for (i, &c) in COS_Q14.iter().enumerate() {
            let want = ((TAU * i as f64 / 256.0).cos() * 16384.0).round() as i16;
            assert_eq!(c, want, "cos[{i}]");
        }
        for (i, &a) in ATAN16.iter().enumerate() {
            let want = ((i as f64 / 64.0).atan() * 256.0 / TAU * 65536.0).round() as i32;
            assert_eq!(a, want, "atan[{i}]");
        }
    }

    #[test]
    fn compass() {
        assert_eq!(cos(0), ONE_Q14);
        assert_eq!(sin(64), ONE_Q14);
        assert_eq!(sin(-64), -ONE_Q14);
        assert_eq!(cos(128 + 256), -ONE_Q14);
        assert_eq!(dir16(0, -10), 0);
        assert_eq!(dir16(10, 0), 64 << 16);
        assert_eq!(dir16(0, 10), 128 << 16);
        assert_eq!(dir16(-10, 0), 192 << 16);
        assert_eq!(dir16(0, 0), 0);
    }

    #[test]
    fn dir16_inverts_the_table() {
        // Every whole step and some fractions round-trip to within a
        // hundredth of a step.
        for a16 in (0..256 << 16).step_by(4099) {
            let (vx, vy) = (sin16(a16), -cos16(a16));
            let back = dir16(vx, vy);
            let err = (back - a16 + (128 << 16)).rem_euclid(256 << 16) - (128 << 16);
            assert!(err.abs() < 700, "a16 {a16}: back {back}, err {err}");
        }
    }

    #[test]
    fn dir16_matches_atan2() {
        for &(vx, vy) in &[(3, -7), (-1000, 1), (12345, 54321), (-5, -5), (70000, -3)] {
            let want = (f64::from(vx).atan2(-f64::from(vy))).rem_euclid(TAU) / TAU * 256.0;
            let got = f64::from(dir16(vx, vy)) / 65536.0;
            assert!((got - want).abs() < 0.01, "({vx},{vy}): {got} vs {want}");
        }
    }

    #[test]
    fn fractional_matches_whole_steps() {
        for a in 0..256 {
            assert_eq!(cos16(a << 16), cos(a));
            assert_eq!(sin16(a << 16), sin(a));
        }
    }
}
