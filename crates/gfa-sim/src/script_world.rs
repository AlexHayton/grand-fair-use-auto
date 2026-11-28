//! The seam between the mission VM (M7) and the rest of the simulation.
//!
//! AmiGTA1 runs its mission script against a table of about 80 callbacks
//! (`gta_script_world` in `gta_script.h`) and never touches the world
//! directly. This trait is that table. It starts with what exists in M3;
//! each milestone adds the methods for what it builds:
//!
//! | Milestone | AmiGTA1 callbacks                                                     |
//! |-----------|-----------------------------------------------------------------------|
//! | M3        | player position, on foot, `onscreen`, `map_type`, `map_face`           |
//! | M4        | `car_*`, `player_car*`, `driver_out`, `car_board`, trains, `door`      |
//! | M5        | `ped_*` (spawn, send to, board, remap, speed)                         |
//! | M6        | `score*`, `drop_wanted`, `explode`, bombs, `respray`, power-ups, arrest |
//! | M7        | `arrow*`, `crane_*`, `alarm`, `robbery`, frenzies, text, `level_end`  |
//!
//! The M7 interpreter itself follows GTA8.EXE (`docs/MISSION_VM.md`), so the
//! methods it adds carry the original's routine beside AmiGTA1's name.
//!
//! Methods the world cannot do yet return [`Unsupported`], so the VM can
//! log the mission line and count it in the coverage report rather than
//! treat it as done (AmiGTA1 lets unknown commands succeed silently).

use crate::world::World;

/// A request the world cannot carry out yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unsupported(pub &'static str);

pub trait ScriptWorld {
    /// The player's position: 16.16 world pixels and layer. In a car, the
    /// car's.
    fn player_pos(&self) -> (i32, i32, i32);

    /// Is the player on foot (not in a car, not getting in or out)?
    fn player_on_foot(&self) -> bool;

    /// Is block `(bx, by)` inside the logical sim view?
    fn onscreen(&self, bx: i32, by: i32) -> bool;

    /// The player's block as GTA8.EXE's trigger scan sees him (0xf3800): on
    /// foot (its state 0) or in a car (state 2). `None` while it skips him
    /// (state 1), which reads as getting in or out of a car or being down.
    fn trigger_block(&self) -> Option<(i32, i32)>;

    /// The model of the car he drives (the scan's state 0), else `None`.
    /// A driver of model 47 sets off no `MPHONES` (0xf4075).
    fn trigger_car_model(&self) -> Option<u8>;

    /// Pay the player `points` (GTA8.EXE's 0xca190 with `100, 100, 0`),
    /// times his multiplier.
    fn pay(&mut self, points: i32);

    /// `POWERUP_ON` (GTA8.EXE's 0xd7c10): a crate of `kind` holding
    /// `amount` on block `[bx, by, bz]`; a kill frenzy (14) stands loose,
    /// uncrated. False when none was made, which the original shrugs off.
    fn powerup_on(&mut self, block: [i32; 3], kind: i32, amount: i32) -> bool;

    /// `POWERUP_OFF` (0xd7fd0): the first crate or item on block `(bx, by)`
    /// taken away, whatever its state. False when there was none.
    fn powerup_off(&mut self, bx: i32, by: i32) -> bool;

    /// `IS_POWERUP_DONE`'s test (0xd8060): is a crate or item, opened or
    /// not, still on block `(bx, by)`?
    fn powerup_there(&self, bx: i32, by: i32) -> bool;

    /// Has a player reached the level's points target (GTA8.EXE's 0xea550)?
    /// The target comes from the career data, which the world does not
    /// read yet (M8), so the default says no.
    fn target_reached(&self) -> bool {
        false
    }

    /// `CHANGE_BLOCK`: replace a block's type word (faces kept).
    fn map_type(&mut self, _x: i32, _y: i32, _z: i32, _value: u32) -> Result<(), Unsupported> {
        Err(Unsupported("map_type"))
    }

    /// `CHANGE_TYPE`: set one face's tile (0 W, 1 E, 2 N, 3 S, 4 lid).
    fn map_face(
        &mut self,
        _x: i32,
        _y: i32,
        _z: i32,
        _face: u8,
        _tile: u8,
    ) -> Result<(), Unsupported> {
        Err(Unsupported("map_face"))
    }
}

impl ScriptWorld for World {
    fn player_pos(&self) -> (i32, i32, i32) {
        (self.player.x, self.player.y, self.player.layer)
    }

    fn player_on_foot(&self) -> bool {
        self.doing == crate::world::Doing::OnFoot
    }

    fn onscreen(&self, bx: i32, by: i32) -> bool {
        self.view
            .contains_block(self.player.x, self.player.y, bx, by)
    }

    fn trigger_block(&self) -> Option<(i32, i32)> {
        use crate::world::Doing;
        match self.doing {
            Doing::Driving => {
                let d = self.driven.as_ref()?;
                Some(((d.veh.ox >> 21) as i32, (d.veh.oy >> 21) as i32))
            }
            Doing::OnFoot | Doing::Vaulting(_) | Doing::Sliding(_) | Doing::Riding(_) => {
                Some((self.player.x >> 21, self.player.y >> 21))
            }
            Doing::Entering(_) | Doing::Exiting(_) | Doing::Drowning(_) => None,
        }
    }

    fn trigger_car_model(&self) -> Option<u8> {
        match self.doing {
            crate::world::Doing::Driving => self.driven.as_ref().map(|d| d.veh.model_id),
            _ => None,
        }
    }

    fn pay(&mut self, points: i32) {
        let award = self.score.add(i64::from(points));
        self.say(format!("score: mission +{award}"));
    }

    fn powerup_on(&mut self, block: [i32; 3], kind: i32, amount: i32) -> bool {
        let [bx, by, _] = block;
        let name = crate::pickups::kind_name(kind);
        match self.pickups.power_up(&self.map, block, kind, amount) {
            Some(i) => {
                let what = match self.pickups.p[i].state {
                    crate::pickups::PickupState::Loose => "loose",
                    _ => "crate",
                };
                self.say(format!(
                    "pickups: {what} {i} at ({bx},{by}) - kind {kind} ({name}), {amount}"
                ));
                true
            }
            None => {
                self.say(format!(
                    "pickups: no crate at ({bx},{by}) - kind {kind} ({name}): no ground or no record free"
                ));
                false
            }
        }
    }

    fn powerup_off(&mut self, bx: i32, by: i32) -> bool {
        let gone = self.pickups.take_off(bx, by);
        if gone {
            self.say(format!("pickups: taken away at ({bx},{by})"));
        }
        gone
    }

    fn powerup_there(&self, bx: i32, by: i32) -> bool {
        self.pickups.first_at(bx, by).is_some()
    }
}
