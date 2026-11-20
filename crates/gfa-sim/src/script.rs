//! Test scripts: what a replay does, tick by tick.
//!
//! Two line formats from AmiGTA1 (MIT) share one file:
//!
//! - **Orders**, from its `autodrive.txt` (`gta_main.c`): `wait 50`,
//!   `run 120 1 0 0 0` (ticks, up, down, steer, handbrake), `goto 62 60`,
//!   `face 64`, `enter`, `park …` and the other fixtures. Each order lasts
//!   its tick count (at least one); `wait` releases the keys, `run` sets
//!   them, and other orders leave them as they were.
//! - **Legs**, from `gtadump walk`: `turn forward walk ticks`, e.g.
//!   `0 1 0 60` runs forward for 60 ticks, then logs where he got to.
//!
//! `start bx by` sets the starting block. Header comments carry the
//! checks, so a script is a test on its own:
//!
//! ```text
//! # section: 1
//! # expect: layer 2 -> 3
//! # reject: BLOCKED
//! ```
//!
//! `expect` text must appear in some log line, `reject` text in none.

use std::fmt;

/// One line of a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Order {
    /// Release every key.
    Wait(u32),
    /// Hold keys: up, down, steer (-1 left, +1 right), handbrake.
    Run {
        ticks: u32,
        up: bool,
        down: bool,
        steer: i32,
        handbrake: bool,
    },
    /// Hold `turn forward walk` for `ticks`, then log the player.
    Leg {
        turn: i32,
        forward: i32,
        walk: bool,
        ticks: u32,
    },
    /// Put the player on a block, optionally facing `face`.
    Goto { bx: i32, by: i32, face: Option<i32> },
    /// Turn the player to an angle.
    Face(i32),
    /// Press the enter key for a tick.
    Enter,
    /// Put the player straight into a car of table record `model`, centred
    /// on block `(bx, by)` facing `face`, on that column's lowest standable
    /// layer. Ours, for the vehicle oracle; not an AmiGTA1 order.
    Drive {
        model: i32,
        bx: i32,
        by: i32,
        face: i32,
    },
    /// Hold fire for `ticks`.
    Fire(u32),
    /// Press jump (the handbrake key) for a tick.
    Jump,
    /// Save a picture of the current frame.
    Dump,
    /// Save a picture every tick, for `n` ticks.
    Film(u32),
    /// An AmiGTA1 fixture the simulation cannot do yet (cars, peds, HUD,
    /// menus…). Kept by name so a run can say what it skipped.
    Fixture { name: String, args: Vec<i32> },
}

impl Order {
    /// Ticks this order lasts. A leg may last 0 ticks (it only logs).
    pub fn ticks(&self) -> u32 {
        match *self {
            Order::Wait(t) | Order::Run { ticks: t, .. } | Order::Fire(t) | Order::Film(t) => {
                t.max(1)
            }
            Order::Leg { ticks, .. } => ticks,
            _ => 1,
        }
    }
}

/// A script line with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub number: usize,
    pub order: Order,
}

/// A parsed script.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Script {
    /// `start bx by`.
    pub start: Option<(i32, i32)>,
    /// `# section: N`.
    pub section: Option<u32>,
    pub lines: Vec<Line>,
    /// `# expect:` texts.
    pub expect: Vec<String>,
    /// `# reject:` texts.
    pub reject: Vec<String>,
}

/// A line the parser could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub line: usize,
    pub text: String,
    pub why: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "script line {}: {} (`{}`)",
            self.line, self.why, self.text
        )
    }
}

impl std::error::Error for ParseError {}

/// AmiGTA1 fixtures and how many numbers each takes (the first count that
/// matches wins, longest first). `park` with four numbers (an empty car:
/// record, dx, dy, facing) and `damage` run; the rest wait for later
/// milestones.
const FIXTURES: &[(&str, &[usize])] = &[
    ("park", &[5, 4]),
    ("weapon", &[2]),
    ("ped", &[3]),
    ("damage", &[1]),
    ("brief", &[2, 1]),
    ("hurt", &[1]),
    ("crate", &[4]),
    ("copcar", &[3]),
    ("sound", &[2, 1]),
    ("mode", &[1]),
    ("board", &[1]),
    ("pool", &[2]),
    ("shootped", &[1]),
    ("killped", &[1]),
    ("wreck", &[1]),
    ("score", &[1]),
    ("script", &[1]),
    ("levelend", &[1]),
    ("gunman", &[3]),
    ("tab", &[1]),
    ("lives", &[1]),
    ("endkey", &[1]),
    ("pause", &[0]),
    ("quitask", &[0]),
    ("quityes", &[0]),
    ("quitno", &[0]),
    ("quittest", &[0]),
];

impl Script {
    pub fn parse(text: &str) -> Result<Script, ParseError> {
        let mut script = Script::default();
        for (i, raw) in text.lines().enumerate() {
            let number = i + 1;
            let line = raw.trim();
            if let Some(comment) = line.strip_prefix('#') {
                script.header(comment.trim(), number, line)?;
                continue;
            }
            if line.is_empty() {
                continue;
            }
            let err = |why| ParseError {
                line: number,
                text: line.to_string(),
                why,
            };
            let mut words = line.split_whitespace();
            let word = words.next().unwrap_or_default();
            let rest: Vec<&str> = words.collect();
            let nums: Option<Vec<i32>> = rest.iter().map(|w| w.parse().ok()).collect();
            // A leg is four bare numbers.
            if let Ok(turn) = word.parse::<i32>() {
                let n = nums.ok_or_else(|| err("a leg is four numbers"))?;
                let [forward, walk, ticks] = n[..] else {
                    return Err(err("a leg is `turn forward walk ticks`"));
                };
                let ticks = u32::try_from(ticks).map_err(|_| err("negative tick count"))?;
                script.push(
                    number,
                    Order::Leg {
                        turn,
                        forward,
                        walk: walk != 0,
                        ticks,
                    },
                );
                continue;
            }
            let order = match (word, nums.as_deref(), rest.as_slice()) {
                ("start", Some(&[bx, by]), _) => {
                    script.start = Some((bx, by));
                    continue;
                }
                ("wait", Some(&[t]), _) => {
                    Order::Wait(ticks(t).ok_or_else(|| err("negative ticks"))?)
                }
                ("run", Some(&[t, up, down, steer, hb]), _) => Order::Run {
                    ticks: ticks(t).ok_or_else(|| err("negative ticks"))?,
                    up: up != 0,
                    down: down != 0,
                    steer: steer.signum(),
                    handbrake: hb != 0,
                },
                ("goto", Some(&[bx, by]), _) => Order::Goto { bx, by, face: None },
                ("goto", None, &[bx, by, "face", face]) => {
                    let n = |s: &str| s.parse::<i32>().map_err(|_| err("goto x y face a"));
                    Order::Goto {
                        bx: n(bx)?,
                        by: n(by)?,
                        face: Some(n(face)?),
                    }
                }
                ("face", Some(&[a]), _) => Order::Face(a),
                ("enter", Some(&[]), _) => Order::Enter,
                ("drive", Some(&[model, bx, by, face]), _) => Order::Drive {
                    model,
                    bx,
                    by,
                    face,
                },
                ("fire", Some(&[t]), _) => {
                    Order::Fire(ticks(t).ok_or_else(|| err("negative ticks"))?)
                }
                ("jump", Some(&[]), _) => Order::Jump,
                ("dump", Some(&[]), _) => Order::Dump,
                ("film", Some(&[t]), _) => {
                    Order::Film(ticks(t).ok_or_else(|| err("negative ticks"))?)
                }
                (name, Some(args), _) => {
                    let Some((name, counts)) = FIXTURES.iter().find(|f| f.0 == name) else {
                        return Err(err("unknown order"));
                    };
                    if !counts.contains(&args.len()) {
                        return Err(err("wrong number of arguments"));
                    }
                    Order::Fixture {
                        name: (*name).to_string(),
                        args: args.to_vec(),
                    }
                }
                _ => return Err(err("unknown order")),
            };
            script.push(number, order);
        }
        Ok(script)
    }

    fn push(&mut self, number: usize, order: Order) {
        self.lines.push(Line { number, order });
    }

    fn header(&mut self, comment: &str, number: usize, line: &str) -> Result<(), ParseError> {
        let Some((key, value)) = comment.split_once(':') else {
            return Ok(());
        };
        let value = value.trim();
        match key.trim() {
            "expect" => self.expect.push(value.to_string()),
            "reject" => self.reject.push(value.to_string()),
            "section" => {
                self.section = Some(value.parse().map_err(|_| ParseError {
                    line: number,
                    text: line.to_string(),
                    why: "section must be a number",
                })?);
            }
            _ => {}
        }
        Ok(())
    }

    /// Ticks the whole script lasts.
    pub fn ticks(&self) -> u32 {
        self.lines.iter().map(|l| l.order.ticks()).sum()
    }
}

fn ticks(t: i32) -> Option<u32> {
    u32::try_from(t).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_formats() {
        let s = Script::parse(
            "# section: 1\n# expect: layer 2 -> 3\n# a note: not a check\nstart 9 44\n\
             -1 0 0 13\n0 1 0 40\nwait 5\nrun 20 1 0 -1 0\ngoto 62 60 face 128\n\
             goto 1 2\nface 64\nenter\npark 3 10 0 64\ndump\nfilm 0\n",
        )
        .unwrap();
        assert_eq!(s.section, Some(1));
        assert_eq!(s.start, Some((9, 44)));
        assert_eq!(s.expect, ["layer 2 -> 3"]);
        assert_eq!(s.lines.len(), 11);
        assert_eq!(
            s.lines[0].order,
            Order::Leg {
                turn: -1,
                forward: 0,
                walk: false,
                ticks: 13
            }
        );
        assert_eq!(
            s.lines[3].order,
            Order::Run {
                ticks: 20,
                up: true,
                down: false,
                steer: -1,
                handbrake: false
            }
        );
        assert_eq!(
            s.lines[4].order,
            Order::Goto {
                bx: 62,
                by: 60,
                face: Some(128)
            }
        );
        assert_eq!(
            s.lines[8].order,
            Order::Fixture {
                name: "park".into(),
                args: vec![3, 10, 0, 64]
            }
        );
        assert_eq!(s.lines[8].number, 13);
        // 13 + 40 + 5 + 20 + 1*6 + film 0 counts as 1.
        assert_eq!(s.ticks(), 13 + 40 + 5 + 20 + 6 + 1);
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        assert_eq!(Script::parse("fly 3").unwrap_err().line, 1);
        assert!(Script::parse("0 1 0").is_err());
        assert!(Script::parse("park 1 2").is_err());
        assert!(Script::parse("wait -1").is_err());
        assert!(Script::parse("# section: one").is_err());
    }
}
