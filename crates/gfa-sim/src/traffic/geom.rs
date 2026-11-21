//! Oriented boxes: overlap tests, the separating push, and the time two
//! moving boxes first touch.
//!
//! After AmiGTA1 (MIT), `gta_traffic.c`. A box is a centre (16.16), an
//! angle (0..256) and half a length and width in whole px; the tests run
//! on whole-pixel offsets against the four Q14 axes of the two boxes.

use gfa_fixed::trig::{cos, sin};

const FP: u32 = 16;

fn axes(aang: i32, bang: i32) -> [[i64; 2]; 4] {
    let (sa, ca) = (i64::from(sin(aang)), i64::from(cos(aang)));
    let (sb, cb) = (i64::from(sin(bang)), i64::from(cos(bang)));
    [[sa, -ca], [ca, sa], [sb, -cb], [cb, sb]]
}

fn projections(axis: &[[i64; 2]; 4], n: [i64; 2], half: [i64; 4]) -> (i64, i64) {
    let dot = |k: usize| ((axis[k][0] * n[0] + axis[k][1] * n[1]) >> 14).abs();
    let pa = dot(0) * half[0] + dot(1) * half[1];
    let pb = dot(2) * half[2] + dot(3) * half[3];
    (pa, pb)
}

/// Do two boxes overlap?
#[allow(clippy::too_many_arguments)]
pub fn box_hit(
    ax: i64,
    ay: i64,
    aang: i32,
    ahl: i32,
    ahw: i32,
    bx: i64,
    by: i64,
    bang: i32,
    bhl: i32,
    bhw: i32,
) -> bool {
    let (dx, dy) = ((bx - ax) >> FP, (by - ay) >> FP);
    let lim = i64::from(ahl + ahw + bhl + bhw);
    if dx > lim || dx < -lim || dy > lim || dy < -lim {
        return false;
    }
    let axis = axes(aang, bang);
    let half = [ahl, ahw, bhl, bhw].map(i64::from);
    for n in axis {
        let r = (dx * n[0] + dy * n[1]).abs();
        let (pa, pb) = projections(&axis, n, half);
        if r > pa + pb {
            return false;
        }
    }
    true
}

/// Does a box overlap the 32 px block centred at `(cx, cy)`?
pub fn box_hit_block(cx: i64, cy: i64, bx: i64, by: i64, bang: i32, bhl: i32, bhw: i32) -> bool {
    let (dx, dy) = ((bx - cx) >> FP, (by - cy) >> FP);
    let (s, c) = (i64::from(sin(bang)), i64::from(cos(bang)));
    let (as_, ac) = (s.abs(), c.abs());
    let q = (s * s + c * c) >> 14;
    let r = (dx * s - dy * c).abs();
    if r > (ac + as_) * 16 + q * i64::from(bhl) {
        return false;
    }
    let r = (dx * c + dy * s).abs();
    r <= (as_ + ac) * 16 + q * i64::from(bhw)
}

/// Closing speed along an axis above which that axis is preferred for the
/// push, 1/8 px a step.
const NRAM_CLOSING: i64 = 65536 / 8;

/// The push that separates two overlapping boxes: a Q14 unit axis pointing
/// from `a` to `b` and the depth along it in Q14 px, or `None` when they do
/// not overlap. Among the axes the pair is closing along (relative velocity
/// `rv`, 16.16), the shallowest wins; failing that, the shallowest of all.
#[allow(clippy::too_many_arguments)]
pub fn box_mtv(
    ax: i64,
    ay: i64,
    aang: i32,
    ahl: i32,
    ahw: i32,
    bx: i64,
    by: i64,
    bang: i32,
    bhl: i32,
    bhw: i32,
    rvx: i64,
    rvy: i64,
) -> Option<(i64, i64, i64)> {
    let (dx, dy) = ((bx - ax) >> FP, (by - ay) >> FP);
    let lim = i64::from(ahl + ahw + bhl + bhw);
    if dx > lim || dx < -lim || dy > lim || dy < -lim {
        return None;
    }
    let axis = axes(aang, bang);
    let half = [ahl, ahw, bhl, bhw].map(i64::from);
    let (mut best, mut b) = (0x7FFF_FFFF_i64, (0, 0));
    let (mut cbest, mut cb) = (0x7FFF_FFFF_i64, (0, 0));
    for n in axis {
        let r = dx * n[0] + dy * n[1];
        let sgn = if r < 0 { -1 } else { 1 };
        let (pa, pb) = projections(&axis, n, half);
        let over = pa + pb - r.abs();
        if over <= 0 {
            return None;
        }
        let (nx, ny) = (n[0] * sgn, n[1] * sgn);
        if over < best {
            best = over;
            b = (nx, ny);
        }
        if (((rvx >> 8) * nx) + ((rvy >> 8) * ny)) >> 6 > NRAM_CLOSING && over < cbest {
            cbest = over;
            cb = (nx, ny);
        }
    }
    Some(if cbest != 0x7FFF_FFFF {
        (cb.0, cb.1, cbest)
    } else {
        (b.0, b.1, best)
    })
}

/// The first time, in steps of the relative motion `(hx, hy)` (16.16 a
/// step), at which box `a` moving that way touches box `b`, up to `maxt`;
/// `-1` if not within it.
#[allow(clippy::too_many_arguments)]
pub fn sweep_toi(
    ax: i64,
    ay: i64,
    aang: i32,
    ahl: i32,
    ahw: i32,
    bx: i64,
    by: i64,
    bang: i32,
    bhl: i32,
    bhw: i32,
    hx: i64,
    hy: i64,
    maxt: i64,
) -> i64 {
    let (dx, dy) = ((bx - ax) >> FP, (by - ay) >> FP);
    let axis = axes(aang, bang);
    let half = [ahl, ahw, bhl, bhw].map(i64::from);
    let (mut tlo, mut thi) = (0i64, maxt);
    for n in axis {
        let r = dx * n[0] + dy * n[1];
        let hn = (hx * n[0] + hy * n[1]) >> 14;
        let (pa, pb) = projections(&axis, n, half);
        let s = pa + pb;
        if hn == 0 {
            if r > s || r < -s {
                return -1;
            }
            continue;
        }
        let (mut t0, mut t1) = ((r - s) / hn, (r + s) / hn);
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }
        tlo = tlo.max(t0);
        thi = thi.min(t1);
        if tlo > thi {
            return -1;
        }
    }
    tlo
}
