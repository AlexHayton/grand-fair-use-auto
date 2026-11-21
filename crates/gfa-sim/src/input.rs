//! Input as the simulation sees it: the buttons held on each tick.
//!
//! The simulation never reads a keyboard. The app turns keys into a
//! [`TickInput`] per tick; a replay is the list of ticks on which the
//! input changed ([`InputLog`]). Presses such as "enter car" are held
//! buttons too; the simulation finds the edges itself.

use crate::hash::{SimHash, StateHasher};
use std::fmt::Write as _;

/// Buttons held during a tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Buttons(pub u16);

impl Buttons {
    pub const NONE: Buttons = Buttons(0);
    /// Forward on foot; accelerate in a car.
    pub const UP: Buttons = Buttons(1 << 0);
    /// Back on foot; brake and reverse in a car.
    pub const DOWN: Buttons = Buttons(1 << 1);
    pub const LEFT: Buttons = Buttons(1 << 2);
    pub const RIGHT: Buttons = Buttons(1 << 3);
    /// Walk instead of run (not in the original, which only runs).
    pub const WALK: Buttons = Buttons(1 << 4);
    pub const FIRE: Buttons = Buttons(1 << 5);
    /// Get in or out of a car.
    pub const ENTER: Buttons = Buttons(1 << 6);
    /// Handbrake in a car; jump on foot.
    pub const HANDBRAKE: Buttons = Buttons(1 << 7);
    /// The original's special key (TAB): horn, siren.
    pub const SPECIAL: Buttons = Buttons(1 << 8);
    pub const WEAPON_NEXT: Buttons = Buttons(1 << 9);
    pub const WEAPON_PREV: Buttons = Buttons(1 << 10);
    /// Keypad `*`: every gun with 99 rounds. The original's key for it,
    /// which it honours only with a name-entry cheat on; ours until the
    /// cheats come (M8).
    pub const ALL_WEAPONS: Buttons = Buttons(1 << 11);

    pub fn contains(self, other: Buttons) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn set(&mut self, other: Buttons, on: bool) {
        if on {
            self.0 |= other.0;
        } else {
            self.0 &= !other.0;
        }
    }

    pub fn with(mut self, other: Buttons, on: bool) -> Buttons {
        self.set(other, on);
        self
    }

    /// Buttons held now that were not held in `before`.
    pub fn pressed_since(self, before: Buttons) -> Buttons {
        Buttons(self.0 & !before.0)
    }
}

/// Everything the player does on one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TickInput {
    pub buttons: Buttons,
}

impl TickInput {
    pub fn new(buttons: Buttons) -> Self {
        TickInput { buttons }
    }

    pub fn held(&self, b: Buttons) -> bool {
        self.buttons.contains(b)
    }
}

impl SimHash for TickInput {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.bytes(&self.buttons.0.to_le_bytes());
    }
}

/// A recording: the input from each tick on which it changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputLog {
    changes: Vec<(u32, TickInput)>,
}

/// The text format's first line.
const LOG_MAGIC: &str = "gfa-input 1";

impl InputLog {
    /// Record the input for `tick`. Ticks must not go backwards.
    pub fn record(&mut self, tick: u32, input: TickInput) {
        let current = self.changes.last().map_or(TickInput::default(), |c| c.1);
        if let Some(&(last, _)) = self.changes.last() {
            assert!(tick >= last, "input log ticks went backwards");
        }
        if input != current {
            self.changes.push((tick, input));
        }
    }

    /// The input in force on `tick`.
    pub fn at(&self, tick: u32) -> TickInput {
        let i = self.changes.partition_point(|c| c.0 <= tick);
        if i == 0 {
            TickInput::default()
        } else {
            self.changes[i - 1].1
        }
    }

    pub fn changes(&self) -> &[(u32, TickInput)] {
        &self.changes
    }

    /// `gfa-input 1`, then one `tick buttons-in-hex` line per change.
    pub fn to_text(&self) -> String {
        let mut s = String::from(LOG_MAGIC);
        s.push('\n');
        for (tick, input) in &self.changes {
            let _ = writeln!(s, "{tick} {:04x}", input.buttons.0);
        }
        s
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(LOG_MAGIC) {
            return Err(format!(
                "not an input log (first line must be `{LOG_MAGIC}`)"
            ));
        }
        let mut log = InputLog::default();
        for (n, line) in lines.enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let bad = || format!("input log line {}: `{line}`", n + 2);
            let (tick, bits) = line.split_once(' ').ok_or_else(bad)?;
            let tick: u32 = tick.parse().map_err(|_| bad())?;
            let bits = u16::from_str_radix(bits.trim(), 16).map_err(|_| bad())?;
            if log.changes.last().is_some_and(|c| c.0 >= tick) {
                return Err(bad());
            }
            log.changes.push((tick, TickInput::new(Buttons(bits))));
        }
        Ok(log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_round_trip() {
        let mut log = InputLog::default();
        let up = TickInput::new(Buttons::UP);
        log.record(0, TickInput::default());
        log.record(3, up);
        log.record(4, up);
        log.record(9, TickInput::new(Buttons::UP.with(Buttons::LEFT, true)));
        log.record(12, TickInput::default());
        assert_eq!(log.changes().len(), 3);
        assert_eq!(log.at(2), TickInput::default());
        assert_eq!(log.at(3), up);
        assert_eq!(log.at(11).buttons, Buttons(5));
        assert_eq!(log.at(500), TickInput::default());
        let text = log.to_text();
        assert_eq!(InputLog::from_text(&text).unwrap(), log);
        assert!(InputLog::from_text("nope").is_err());
    }

    #[test]
    fn edges() {
        let now = Buttons::UP.with(Buttons::ENTER, true);
        assert_eq!(now.pressed_since(Buttons::UP), Buttons::ENTER);
    }
}
