//! In-game SFX pitch/volume calculations and positional attenuation.
//!
//! Sources: AmiGTA1 `native/gta_main.c` (`engine_rate`, `covered`, siren/horn
//! loops) and `native/gta_snd.h` (MIT).

/// Sawtooth gear frequencies (Hz) for `sound_function` 0 and 1.
const ENGINE_GEAR_HZ: [f64; 13] = [
    18000.0, 33000.0, 41000.0, 48000.0, 53000.0, 58000.0, 62000.0, 66000.0, 69500.0, 72000.0,
    74000.0, 76500.0, 77000.0,
];

/// Compute the engine playback rate (Hz) and volume scale (`0.0..=1.0`) for a
/// vehicle from its `sound_function`, bank sample number (`0x2d + engine`),
/// signed forward speed in original units (half a sim pixel per physics step),
/// and whether it is airborne.
pub fn engine_rate(sound_function: u8, sample: u8, v: i32, airborne: bool) -> (f64, f32) {
    let a = v.abs();
    let mut hz = match sound_function {
        0 | 1 => {
            let g = if v == 0 {
                0
            } else if v < 0 {
                a.min(12) as usize
            } else {
                (v / 10 + v % 10 + 1).min(12) as usize
            };
            let mut h = ENGINE_GEAR_HZ[g];
            if sound_function == 1 {
                h *= 0.5;
            } else if sample == 0x36 {
                h *= 2.0;
            }
            h
        }
        2 | 3 => {
            let mut h = 22050.0 + if v < 0 { 400.0 * f64::from(a) } else { 1400.0 * f64::from(v) };
            if sound_function == 3 {
                h *= 3.0;
            }
            h
        }
        4 => 4000.0 + 2000.0 * f64::from(a),
        5 => {
            if v < 0 {
                6000.0 + 500.0 * f64::from(a)
            } else if v < 8 {
                6000.0 + 333.0 * f64::from(v)
            } else if v < 12 {
                750.0 * f64::from(v)
            } else if v < 18 {
                500.0 * f64::from(v)
            } else {
                333.0 * f64::from(v)
            }
        }
        _ => 4000.0,
    };
    if airborne {
        hz *= 3.0;
    }
    let pct = if v < 1 {
        40
    } else {
        (40 + v * 3 / 2).min(100)
    };
    // The engine is the floor of the mix (~75% max relative to one-shots).
    (hz.max(1000.0), (pct as f32 / 100.0) * 0.65)
}

/// Audible radius in sim pixels (16 blocks × 32 px/block).
pub const MAX_RANGE_PX: f32 = 512.0;

/// Compute `(gain, pan)` for a sound at 16.16 sim position `(src_x, src_y)`
/// relative to `(listener_x, listener_y)`. Returns `None` if beyond
/// [`MAX_RANGE_PX`]. `covered` halves the gain (under a roof, bridge or tunnel).
pub fn positional(
    src_x: i32,
    src_y: i32,
    listener_x: i32,
    listener_y: i32,
    covered: bool,
) -> Option<(f32, f32)> {
    let dx = ((src_x - listener_x) >> 16) as f32;
    let dy = ((src_y - listener_y) >> 16) as f32;
    let dist = dx.abs() + dy.abs();
    if dist >= MAX_RANGE_PX {
        return None;
    }
    let mut gain = (1.0 - dist / MAX_RANGE_PX).clamp(0.0, 1.0);
    if covered {
        gain *= 0.5;
    }
    if gain <= 0.01 {
        return None;
    }
    let pan = (dx / 256.0).clamp(-0.85, 0.85);
    Some((gain, pan))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_rate_follows_gear_and_airborne_rules() {
        let (idle_hz, idle_vol) = engine_rate(0, 0x2d, 0, false);
        assert_eq!(idle_hz, 18000.0);
        assert!((idle_vol - 0.26).abs() < 0.01);

        let (air_hz, _) = engine_rate(0, 0x2d, 0, true);
        assert_eq!(air_hz, 54000.0);

        let (half_hz, _) = engine_rate(1, 0x2d, 0, false);
        assert_eq!(half_hz, 9000.0);

        let (tank_hz, _) = engine_rate(4, 0x2d, 5, false);
        assert_eq!(tank_hz, 14000.0);
    }

    #[test]
    fn positional_attenuates_pans_and_halves_under_cover() {
        let (g0, p0) = positional(0, 0, 0, 0, false).unwrap();
        assert!((g0 - 1.0).abs() < 1e-5);
        assert!(p0.abs() < 1e-5);

        let (gc, _) = positional(0, 0, 0, 0, true).unwrap();
        assert!((gc - 0.5).abs() < 1e-5);

        let (_, pr) = positional(128 << 16, 0, 0, 0, false).unwrap();
        assert!(pr > 0.4);
        assert!(positional(600 << 16, 0, 0, 0, false).is_none());
    }
}
