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
        true
    }

    fn onscreen(&self, bx: i32, by: i32) -> bool {
        self.view
            .contains_block(self.player.x, self.player.y, bx, by)
    }
}
