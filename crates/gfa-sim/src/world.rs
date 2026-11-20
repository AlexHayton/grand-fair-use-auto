//! The whole simulation state and its tick.

use crate::hash::{SimHash, StateHasher};
use crate::input::{Buttons, TickInput};
use crate::map::SimMap;
use crate::player::{Player, Stick};
use crate::view::SimView;
use gfa_fixed::fx::{BLOCK_SHIFT, FP, block_centre};

/// Where the player starts: a block, and optionally a heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Start {
    pub bx: i32,
    pub by: i32,
    /// 0..256, 0 north. `None` faces south, the way the art is drawn.
    pub angle: Option<i32>,
}

impl Start {
    /// From a `PLAYER` declaration: `(x, y)` in blocks and the second
    /// trailing number a 1024-step heading (AmiGTA1
    /// `gta_script_player_start`).
    pub fn from_decl(pos: [i32; 3], nums: &[i32]) -> Self {
        // AmiGTA1 stores the numbers as `short` and divides with C's
        // truncation, which Rust's `/` matches.
        let b = nums.get(1).map_or(0, |&b| i32::from(b as i16));
        Start {
            bx: pos[0],
            by: pos[1],
            angle: Some((b / 4) & 255),
        }
    }

    /// From a mission section's first `PLAYER` declaration.
    pub fn from_section(section: &gfa_formats::mission::Section) -> Option<Self> {
        let d = section.decls.iter().find(|d| d.kind == "PLAYER")?;
        Some(Start::from_decl(d.pos, &d.nums))
    }
}

/// A line for the run log, stamped with the tick it happened on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub tick: u32,
    pub text: String,
}

/// The simulation.
#[derive(Debug, Clone)]
pub struct World {
    /// Ticks completed.
    pub tick: u32,
    pub map: SimMap,
    pub view: SimView,
    pub player: Player,
    /// Last tick's buttons, to find presses.
    prev: Buttons,
    log: Vec<LogLine>,
}

impl World {
    pub fn new(map: SimMap, view: SimView, start: Start) -> Self {
        let (mut player, found) = Player::place(&map, start.bx, start.by);
        if let Some(a) = start.angle {
            player.angle = a & 255;
        }
        let mut world = World {
            tick: 0,
            map,
            view,
            player,
            prev: Buttons::NONE,
            log: Vec::new(),
        };
        let p = &world.player;
        let text = format!(
            "player: start ({},{}) layer {} angle {} ground {}{}",
            start.bx,
            start.by,
            p.layer,
            p.angle,
            ground_name(p.ground),
            if found { "" } else { " (no walkable layer)" }
        );
        world.say(text);
        world
    }

    /// Advance one tick.
    pub fn step(&mut self, input: TickInput) {
        let b = input.buttons;
        let axis = |plus: Buttons, minus: Buttons| {
            i32::from(b.contains(plus)) - i32::from(b.contains(minus))
        };
        let stick = Stick {
            turn: axis(Buttons::RIGHT, Buttons::LEFT),
            forward: axis(Buttons::UP, Buttons::DOWN),
            walk: b.contains(Buttons::WALK),
        };
        let before = self.player.layer;
        self.player.update(&self.map, stick);
        if self.player.layer != before {
            let (bx, by) = self.player.block();
            self.say(format!(
                "player: layer {before} -> {} at ({bx},{by})",
                self.player.layer
            ));
        }
        self.prev = b;
        self.tick += 1;
    }

    /// A test fixture: put the player on block `(bx, by)`, on its lowest
    /// walkable layer, optionally turned to `face`.
    pub fn teleport(&mut self, bx: i32, by: i32, face: Option<i32>) {
        let angle = face.unwrap_or(self.player.angle);
        let (mut player, found) = Player::place(&self.map, bx, by);
        player.angle = angle & 255;
        self.player = player;
        let layer = self.player.layer;
        self.say(format!(
            "player: goto ({bx},{by}) layer {layer}{}",
            if found { "" } else { " (no walkable layer)" }
        ));
    }

    /// Add a line to the run log.
    pub fn say(&mut self, text: String) {
        self.log.push(LogLine {
            tick: self.tick,
            text,
        });
    }

    /// Take the log lines written since the last call.
    pub fn drain_log(&mut self) -> Vec<LogLine> {
        std::mem::take(&mut self.log)
    }

    /// A hash of everything that decides future ticks.
    pub fn hash(&self) -> u64 {
        let mut h = StateHasher::default();
        h.u32(self.tick);
        self.player.sim_hash(&mut h);
        h.bytes(&self.prev.0.to_le_bytes());
        h.finish()
    }

    /// The player's position for the renderer: 16.16 world pixels, with
    /// the height of the ground under him (32 px to a layer).
    pub fn player_pos(&self) -> [i32; 3] {
        let p = &self.player;
        [p.x, p.y, self.map.ground_z(p.x, p.y, p.layer)]
    }

    /// The gtadump-style one-line summary of the player.
    pub fn player_summary(&self) -> String {
        let p = &self.player;
        let part = |v: i32| {
            let px = (v >> FP) & ((1 << BLOCK_SHIFT) - 1);
            let hundredths = ((v & 0xFFFF) * 100) >> FP;
            format!("{px:2}.{hundredths:02}")
        };
        let (bx, by) = p.block();
        format!(
            "block ({bx:3},{by:3}) +{},{}  layer {}  angle {:3}  ground {:<8}{}{}",
            part(p.x),
            part(p.y),
            p.layer,
            p.angle,
            ground_name(p.ground),
            if p.blocked_x { "  BLOCKED-X" } else { "" },
            if p.blocked_y { "  BLOCKED-Y" } else { "" },
        )
    }
}

/// The centre of a block as a 16.16 point.
pub fn block_point(bx: i32, by: i32) -> (i32, i32) {
    (block_centre(bx), block_centre(by))
}

/// A ground type's name, as AmiGTA1's tools print it.
pub fn ground_name(g: u8) -> &'static str {
    const NAMES: [&str; 8] = [
        "air", "water", "road", "pavement", "field", "building", "?6", "?7",
    ];
    NAMES[usize::from(g & 7)]
}
