//! The fixed tick and the slower clocks that run off it.
//!
//! The simulation steps at [`SIM_HZ`] (AmiGTA1's PAL-derived 50 Hz).
//! Subsystems that ran slower in the original keep their own divider, as
//! they do in AmiGTA1:
//!
//! | Subsystem                      | Rate      | Divider                 |
//! |--------------------------------|-----------|-------------------------|
//! | mission script                 | 25 Hz     | [`Half`], second tick   |
//! | marks (blood, fires), trains   | 25 Hz     | [`Half`], first tick    |
//! | vehicle physics                | 23.333 Hz | [`Ratio`] 7/15          |
//! | ped timers                     | 50 Hz     | [`ped_frames`] scales   |
//!
//! Vehicle physics runs at the original's own 23.333 Hz (three ticks of a
//! 70 Hz timer) rather than being rescaled, because its yaw damping is a
//! property of the discrete step (see AmiGTA1 `gta_vehphys.c`).
//!
//! How many ticks to run per rendered frame is decided outside the
//! simulation, by the app's pacer: wall-clock time never reaches here.

/// Simulation ticks per second.
pub const SIM_HZ: u32 = 50;
/// Microseconds per tick.
pub const TICK_US: u32 = 1_000_000 / SIM_HZ;

/// A subsystem that runs every other tick.
///
/// AmiGTA1 writes these as `half ^= 1; if (half) return;` (runs on the
/// second tick) or `if (!half) return;` (runs on the first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Half {
    flag: bool,
    runs_when_set: bool,
}

impl Half {
    /// `runs_first`: whether the very first tick runs.
    pub const fn new(runs_first: bool) -> Self {
        Half {
            flag: false,
            runs_when_set: runs_first,
        }
    }

    /// Advance one tick; true if the subsystem runs on it.
    pub fn step(&mut self) -> bool {
        self.flag = !self.flag;
        self.flag == self.runs_when_set
    }
}

/// A subsystem that runs `num` times every `den` ticks, spread evenly by an
/// accumulator: `acc += num; if acc >= den { acc -= den; run }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ratio {
    num: u32,
    den: u32,
    acc: u32,
}

impl Ratio {
    pub const fn new(num: u32, den: u32) -> Self {
        Ratio { num, den, acc: 0 }
    }

    /// Advance one tick; true if the subsystem steps on it. Needs
    /// `num <= den`.
    pub fn step(&mut self) -> bool {
        self.acc += self.num;
        if self.acc < self.den {
            return false;
        }
        self.acc -= self.den;
        true
    }
}

/// The vehicle physics clock: the original's 23.333 Hz against our 50.
pub const fn vehicle_physics() -> Ratio {
    Ratio::new(7, 15)
}

/// A ped timer of `n` original frames (25 Hz) in our ticks:
/// AmiGTA1's `FRAMES(n) = n * 3 / 2`.
pub const fn ped_frames(n: i32) -> i32 {
    n * 3 / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves() {
        let mut script = Half::new(false);
        let mut marks = Half::new(true);
        let s: Vec<bool> = (0..4).map(|_| script.step()).collect();
        let m: Vec<bool> = (0..4).map(|_| marks.step()).collect();
        assert_eq!(s, [false, true, false, true]);
        assert_eq!(m, [true, false, true, false]);
    }

    #[test]
    fn vehicle_physics_is_seven_in_fifteen() {
        let mut r = vehicle_physics();
        let steps: Vec<bool> = (0..15).map(|_| r.step()).collect();
        assert_eq!(steps.iter().filter(|&&s| s).count(), 7);
        // AmiGTA1's accumulator first fires on the third tick.
        assert_eq!(&steps[..5], &[false, false, true, false, true]);
        let mut r = vehicle_physics();
        assert_eq!((0..1500).filter(|_| r.step()).count(), 700);
    }
}
