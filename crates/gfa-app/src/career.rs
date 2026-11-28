//! The port's versioned player file. DOS career data is an optional first-run import.
use std::collections::BTreeMap;

use gfa_formats::career::{Career as Original, LEVELS, SLOTS};

use crate::controls::Bindings;

pub const NAMES: [&str; SLOTS] = [
    "Ulrika", "Travis", "Katie", "Mikki", "Divine", "Bubba", "Troy", "Kivlov",
];

#[derive(Clone)]
pub struct Career {
    pub current_slot: usize,
    pub options: Options,
    pub players: [Player; SLOTS],
    pub high_scores: [[Entry; 3]; LEVELS],
}

#[derive(Clone)]
pub struct Options {
    pub sound_volume: u8,
    pub music_volume: u8,
    pub text_speed: u8,
    pub music_mode: u8,
    pub keybindings: Bindings,
    pub hud_filter: gfa_render::hud::HudFilter,
}

#[derive(Clone)]
pub struct Player {
    pub name: String,
    /// Section number to best score. An absent chapter is locked.
    pub best: BTreeMap<u32, i64>,
    pub seen: Vec<u32>,
}

impl Player {
    pub fn new(slot: usize) -> Self {
        Self {
            name: NAMES[slot].into(),
            best: BTreeMap::from([(1, 0)]),
            seen: vec![],
        }
    }
}

#[derive(Clone)]
pub struct Entry {
    pub name: String,
    pub score: i64,
}

impl Career {
    pub fn from_original(original: Option<&Original>) -> Self {
        let players = std::array::from_fn(|slot| {
            let mut p = Player::new(slot);
            if let Some(c) = original {
                let name = c.name(slot);
                if !name.is_empty() {
                    p.name = name;
                }
                p.best.clear();
                for (i, section) in crate::menu::SECTIONS.into_iter().enumerate() {
                    if let Some(score) = c.best(slot, i) {
                        p.best.insert(section, score.into());
                    }
                    if c.seen(slot, i) {
                        p.seen.push(section);
                    }
                }
            }
            p
        });
        let high_scores = std::array::from_fn(|level| {
            std::array::from_fn(|row| {
                let (score, name) = original
                    .map(|c| c.high_score(level, row))
                    .unwrap_or_else(|| (row as i32 + 1, ["BILLY", "PAUL", "STEVE"][row].into()));
                Entry {
                    score: score.into(),
                    name,
                }
            })
        });
        Self {
            current_slot: original.map_or(1, Original::current_slot),
            players,
            high_scores,
            options: Options {
                sound_volume: original.map_or(7, |c| c.sound_volume().min(7)),
                music_volume: original.map_or(6, |c| c.music_volume().min(7)),
                text_speed: original.map_or(2, |c| c.text_speed().clamp(1, 3)),
                music_mode: original.map_or(0, |c| c.bytes[3].min(1)),
                keybindings: Default::default(),
                hud_filter: Default::default(),
            },
        }
    }

    pub fn record_result(&mut self, level: usize, score: i64, passed: bool) {
        let player = &mut self.players[self.current_slot];
        let section = crate::menu::SECTIONS[level];
        let best = player.best.entry(section).or_insert(score);
        *best = (*best).max(score);
        let table = &mut self.high_scores[level];
        if let Some(row) = table.iter().rposition(|entry| score > entry.score) {
            table[..=row].rotate_left(1);
            table[row] = Entry {
                score,
                name: player.name.clone(),
            };
        }
        if passed {
            if !player.seen.contains(&section) {
                player.seen.push(section);
            }
            if let Some(next) = crate::menu::SECTIONS.get(level + 1) {
                player.best.entry(*next).or_insert(0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_over_records_the_selected_players_score_and_pass_unlocks_next_chapter() {
        let mut career = Career::from_original(None);
        career.current_slot = 4;
        career.record_result(0, 42, false);
        assert_eq!(career.players[4].best[&1], 42);
        assert!(!career.players[4].best.contains_key(&2));
        assert_eq!(career.players[0].best[&1], 0);
        assert_eq!(career.high_scores[0][2].name, "Divine");
        career.record_result(0, 20, true);
        assert_eq!(career.players[4].best[&1], 42);
        assert_eq!(career.players[4].best[&2], 0);
        assert_eq!(career.players[4].seen, [1]);
        assert_eq!(
            career.high_scores[0].each_ref().map(|e| e.score),
            [3, 20, 42]
        );
    }
}
