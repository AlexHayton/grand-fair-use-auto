//! The score, the multiplier, and the wanted level's heat.
//!
//! Ported from AmiGTA1 (MIT): `gta_score.c`, whose rules it took from the
//! original. In short:
//!
//! - **An award is a type and a reason.** The type is what was hit (a
//!   person, a car), the reason how (run over, shot, burnt, blown up); the
//!   reason picks a base. The same type again inside the streak window
//!   (13 of the original's frames) raises the streak, and the whole is
//!   multiplied by the player's multiplier.
//! - **Heat never cools.** A crime adds heat; only a respray, a bribe, a
//!   bust, a death or the script clear it. The wanted level is not kept:
//!   it is read off the heat against the city's thresholds each time the
//!   heat changes.
//! - **The crime counters** are this life's (the pager reads them on a
//!   bust) and the level's (the end screen's record).
//!
//! The multiplier starts at 1. The pickup, a mission reward and the bust
//! (which halves it) are M6's and M7's callers.

use crate::hash::StateHasher;

/// What was hit: the original's score types that the game reaches so far.
pub mod kind {
    /// A pedestrian.
    pub const CIVILIAN: u8 = 1;
    /// A car rammed.
    pub const RAM: u8 = 0x18;
    /// A car taken off its driver.
    pub const CAR: u8 = 0x1c;
}

/// How: the reason picks the base an award is multiplied from.
pub mod reason {
    pub const RUNOVER: u8 = 1;
    pub const BLOWN: u8 = 2;
    pub const SHOT: u8 = 5;
    pub const BURNED: u8 = 8;
}

/// The crimes, numbered as the original reports them.
pub mod crime {
    /// Ramming a car while driving at speed: 2 heat.
    pub const SHUNT: usize = 2;
    /// A pedestrian hit by his car: 50.
    pub const RUNOVER: usize = 3;
    /// A train taken: 10.
    pub const HIJACK: usize = 4;
    /// A car taken off its driver: 15.
    pub const CARJACK: usize = 5;
    /// 50; the original never reports it.
    pub const GTA: usize = 6;
    /// A round into a car, or a car destroyed: 1.
    pub const FIREARM: usize = 7;
    /// A pedestrian killed by a bullet, a fire or a blast: 100.
    pub const MURDER: usize = 8;
    pub const COUNT: usize = 10;
}

/// The heat stops here.
pub const HEAT_CAP: i32 = 2000;
/// The score stops here, as the original's does.
pub const SCORE_CAP: i64 = 999_999_999;
/// The streak window: the original's 13 frames, in our 50 Hz ticks.
pub const STREAK_TICKS: i32 = 20;
/// The highest wanted level.
pub const WANTED_MAX: i32 = 4;

/// The heat at which each level begins, for Liberty City, San Andreas and
/// Vice City (the map's style number 1, 2, 3).
const THRESHOLDS: [[i32; 5]; 3] = [
    [0, 151, 251, 351, 501],
    [0, 101, 201, 251, 376],
    [0, 101, 176, 251, 351],
];

fn crime_heat(c: usize) -> i32 {
    match c {
        crime::SHUNT => 2,
        crime::RUNOVER | crime::GTA => 50,
        crime::HIJACK => 10,
        crime::CARJACK => 15,
        crime::FIREARM => 1,
        crime::MURDER | 9 => 100,
        _ => 0,
    }
}

fn kind_value(k: u8) -> i64 {
    // Every type the game reaches is worth 100 but the ram's 10.
    if k == kind::RAM { 10 } else { 100 }
}

fn reason_base(r: u8) -> i64 {
    match r {
        2 => 3,
        3 | 7 | 8 => 10,
        4 => 7,
        5 | 6 => 2,
        _ => 1,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score {
    pub score: i64,
    pub multiplier: i32,
    /// The last type scored, while the window is open.
    streak_kind: Option<u8>,
    streak_count: i32,
    streak_timer: i32,
    /// What the last award was worth.
    pub last_award: i64,
    /// Every award of types 1..=9: the kills a frenzy counts.
    pub kills: u32,
    pub heat: i32,
    /// 0..=4, read off the heat at every change.
    pub level: i32,
    /// The police cheat: the level reads 0 while the heat keeps rising.
    pub no_police: bool,
    /// This life's crimes, by [`crime`] number.
    pub crimes: [u32; crime::COUNT],
    /// The level's crimes.
    pub total: [u32; crime::COUNT],
    /// Which city's thresholds: 0 Liberty, 1 San Andreas, 2 Vice.
    city: usize,
}

impl Score {
    /// A fresh score for a map of style number `style`.
    pub fn new(style: u8) -> Self {
        Score {
            score: 0,
            multiplier: 1,
            streak_kind: None,
            streak_count: 0,
            streak_timer: 0,
            last_award: 0,
            kills: 0,
            heat: 0,
            level: 0,
            no_police: false,
            crimes: [0; crime::COUNT],
            total: [0; crime::COUNT],
            city: match style {
                2 => 1,
                3 => 2,
                _ => 0,
            },
        }
    }

    /// The heat at which `level` begins in this city.
    pub fn threshold(&self, level: i32) -> i32 {
        THRESHOLDS[self.city][level.clamp(0, WANTED_MAX) as usize]
    }

    /// The level `heat` gives in this city.
    pub fn level_of(&self, heat: i32) -> i32 {
        (1..=WANTED_MAX)
            .take_while(|&l| heat >= self.threshold(l))
            .last()
            .unwrap_or(0)
    }

    /// One tick: the streak window closes on its own.
    pub fn tick(&mut self) {
        if self.streak_timer > 0 {
            self.streak_timer -= 1;
            if self.streak_timer == 0 {
                self.streak_kind = None;
                self.streak_count = 0;
            }
        }
    }

    /// A flat award, times the multiplier (shooting a car is 10).
    pub fn add(&mut self, value: i64) -> i64 {
        let award = i64::from(self.multiplier) * value;
        self.score = (self.score + award).min(SCORE_CAP);
        self.last_award = award;
        award
    }

    /// Score an event of `kind` for `reason`; returns what it was worth.
    pub fn event(&mut self, kind: u8, reason: u8) -> i64 {
        let base = reason_base(reason);
        if (1..=9).contains(&kind) {
            self.kills += 1;
        }
        if self.streak_kind == Some(kind) && self.streak_timer > 0 {
            self.streak_count = (self.streak_count + 1).min(30);
        } else {
            self.streak_kind = Some(kind);
            self.streak_count = 1;
        }
        self.streak_timer = STREAK_TICKS;
        // A person: the streak times the base. Anything else with a base
        // above 1 doubles for each repeat (at most 19 times); with base 1
        // it is the streak.
        let factor = if kind == kind::CIVILIAN {
            i64::from(self.streak_count) * base
        } else if base > 1 {
            base << (self.streak_count - 1).clamp(0, 19)
        } else {
            i64::from(self.streak_count)
        };
        self.add(factor * kind_value(kind))
    }

    /// Report crime `c`: its heat, its counters, and the level read again.
    /// Returns a line when the level changed.
    pub fn crime(&mut self, c: usize) -> Option<String> {
        let before = self.level;
        if c < crime::COUNT {
            self.crimes[c] += 1;
            self.total[c] += 1;
        }
        self.heat = (self.heat + crime_heat(c)).min(HEAT_CAP);
        self.relevel();
        (self.level != before).then(|| {
            format!(
                "score: wanted level {before} -> {} (heat {}, crime {c})",
                self.level, self.heat
            )
        })
    }

    /// Raise the heat to exactly where `level` begins, when it is below
    /// (hitting a police car at level 0 makes it level 1). Never lowers.
    pub fn force_level(&mut self, level: i32) -> Option<String> {
        let before = self.level;
        self.heat = self.heat.max(self.threshold(level));
        self.relevel();
        (self.level != before).then(|| {
            format!(
                "score: wanted level {before} -> {} (forced, heat {})",
                self.level, self.heat
            )
        })
    }

    /// Heat and level to nothing: a respray, a bribe, a bust, a death. The
    /// crime counters are kept for the pager.
    pub fn clear_heat(&mut self) -> Option<String> {
        let line = (self.level != 0 || self.heat != 0).then(|| {
            format!(
                "score: wanted level cleared (was {}, heat {})",
                self.level, self.heat
            )
        });
        self.heat = 0;
        self.level = 0;
        line
    }

    /// A new life: this life's counters start again.
    pub fn new_life(&mut self) {
        self.crimes = [0; crime::COUNT];
    }

    /// How many events of the current type have been scored in this streak.
    pub fn streak_count(&self) -> i32 {
        self.streak_count
    }

    fn relevel(&mut self) {
        self.level = if self.no_police {
            0
        } else {
            self.level_of(self.heat)
        };
    }

    pub fn sim_hash(&self, h: &mut StateHasher) {
        h.bytes(&self.score.to_le_bytes());
        h.i32(self.multiplier);
        h.i32(self.streak_kind.map_or(-1, i32::from));
        h.i32(self.streak_count);
        h.i32(self.streak_timer);
        h.u32(self.kills);
        h.i32(self.heat);
        h.i32(self.level);
        h.bool(self.no_police);
        for c in self.crimes.iter().chain(&self.total) {
            h.u32(*c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_run_over_inside_the_window_is_worth_twice_the_first() {
        let mut s = Score::new(1);
        assert_eq!(s.event(kind::CIVILIAN, reason::RUNOVER), 100);
        assert_eq!(s.event(kind::CIVILIAN, reason::RUNOVER), 200);
        for _ in 0..STREAK_TICKS {
            s.tick();
        }
        assert_eq!(s.event(kind::CIVILIAN, reason::SHOT), 200);
        assert_eq!(s.score, 500);
        assert_eq!(s.kills, 3);
    }

    #[test]
    fn a_car_streak_doubles() {
        let mut s = Score::new(1);
        s.multiplier = 2;
        // The car's reason 0 has base 1: the streak counts.
        assert_eq!(s.event(kind::CAR, 0), 200);
        assert_eq!(s.event(kind::CAR, 0), 400);
        // A blown-up car (base 3) doubles each time.
        let mut s = Score::new(1);
        assert_eq!(s.event(kind::CAR, reason::BLOWN), 300);
        assert_eq!(s.event(kind::CAR, reason::BLOWN), 600);
        assert_eq!(s.event(kind::CAR, reason::BLOWN), 1200);
    }

    #[test]
    fn the_level_reads_the_cities_thresholds() {
        let mut s = Score::new(1);
        for _ in 0..3 {
            assert_eq!(s.crime(crime::RUNOVER), None);
        }
        assert!(s.crime(crime::RUNOVER).is_some());
        assert_eq!((s.heat, s.level), (200, 1));
        assert_eq!(Score::new(2).level_of(200), 1);
        assert_eq!(Score::new(2).level_of(201), 2);
        assert_eq!(Score::new(3).level_of(351), 4);
        assert_eq!(s.level_of(HEAT_CAP), 4);
    }

    #[test]
    fn forcing_a_level_never_lowers_and_clearing_keeps_the_counts() {
        let mut s = Score::new(1);
        s.force_level(1);
        assert_eq!((s.heat, s.level), (151, 1));
        s.crime(crime::MURDER);
        s.force_level(1);
        assert_eq!((s.heat, s.level), (251, 2));
        s.clear_heat();
        assert_eq!((s.heat, s.level), (0, 0));
        assert_eq!(s.crimes[crime::MURDER], 1);
        s.new_life();
        assert_eq!(s.crimes[crime::MURDER], 0);
        assert_eq!(s.total[crime::MURDER], 1);
    }

    #[test]
    fn heat_is_capped_and_the_police_cheat_hides_the_level() {
        let mut s = Score::new(1);
        s.no_police = true;
        for _ in 0..30 {
            s.crime(crime::MURDER);
        }
        assert_eq!((s.heat, s.level), (HEAT_CAP, 0));
    }
}
