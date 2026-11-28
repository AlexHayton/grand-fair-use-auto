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
//! | M7        | `arrow*`, `car_on`, `crane_*`, `alarm`, `robbery`, frenzies, text, `level_end` |
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

/// The root player's view as GTA8.EXE keeps it in his record (0x3ba478), in
/// the original's pixels (64 to a block): what the arrow (0xff0e0) and
/// `GENERAL_ONSCREEN` (0xa583b) read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerView {
    /// His position, or his car's, 16.16 (0xcd9e0).
    pub at: (i32, i32),
    /// The view rectangle (+4, 0xcd760): left, right, top and bottom, whole
    /// pixels.
    pub rect: [i32; 4],
    /// The camera's depth (+0x30, 0x1cc1dc): 78 standing on the ground.
    pub depth: i32,
}

impl PlayerView {
    /// The rectangle round `centre` (whole pixels) for a camera at `depth`
    /// and `plane` (0x736d0): `(depth + 384) × 160 / plane` either side,
    /// three quarters of that above and below, no edge below 0. At rest,
    /// 520 by 390 pixels.
    pub fn rect_round(centre: (i32, i32), depth: i32, plane: i32) -> [i32; 4] {
        let half = (depth + 0x180) * 160 / plane.max(1);
        let tall = half * 3 / 4;
        let (cx, cy) = centre;
        [cx - half, cx + half, cy - tall, cy + tall].map(|e| e.max(0))
    }

    /// Is pixel `(x, y)` inside the rectangle, edges included?
    pub fn shows(&self, x: i32, y: i32) -> bool {
        let [l, r, t, b] = self.rect;
        (l..=r).contains(&x) && (t..=b).contains(&y)
    }
}

/// Where a mission's door stands (GTA8.EXE's door record +0x10 to +0x16):
/// its declared block, which edge of it the door is on, and the floor tile
/// laid under its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorPlace {
    /// x, y and gfa-sim's layer.
    pub block: [i32; 3],
    /// The declared block's west (0), east (1), north (2) or south (3)
    /// edge.
    pub facing: u8,
    /// The lid laid on the three blocks under the door's row, one layer
    /// down, when not 0 (the third number; AmiGTA1 reads it so, and 0xe7e10
    /// lays it on those blocks).
    pub lid: u8,
}

impl DoorPlace {
    /// Blocks behind the door its doorway covers (0xe8900).
    const DOORWAY: i32 = 3;

    /// The block whose west edge (north edge, when the flag is set) holds
    /// the door, on the declared block's layer: the declared block itself
    /// for facings 0 and 2, the one east or south of it for 1 and 3.
    pub fn face(&self) -> (i32, i32, i32, bool) {
        let [x, y, z] = self.block;
        (
            x + i32::from(self.facing == 1),
            y + i32::from(self.facing == 3),
            z,
            self.facing >= 2,
        )
    }

    /// Is block `(bx, by)` in the doorway (0xe8900): the declared block or
    /// one of the three behind it, away from the door's edge?
    pub fn inside(&self, bx: i32, by: i32) -> bool {
        let [x, y, _] = self.block;
        let r = Self::DOORWAY;
        match self.facing {
            0 => by == y && (x..=x + r).contains(&bx),
            1 => by == y && (x - r..=x).contains(&bx),
            2 => bx == x && (y..=y + r).contains(&by),
            _ => bx == x && (y - r..=y).contains(&by),
        }
    }
}

pub trait ScriptWorld {
    /// The player's position: 16.16 world pixels and layer. In a car, the
    /// car's.
    fn player_pos(&self) -> (i32, i32, i32);

    /// Is the player on foot (not in a car, not getting in or out)?
    fn player_on_foot(&self) -> bool;

    /// The root player's view in the original's pixels.
    fn player_view(&self) -> PlayerView;

    /// The player's block as GTA8.EXE's trigger scan sees him (0xf3800), by
    /// his state (his record +0xb9). In a DOSBox-X probe
    /// (`tools/dosprobe/scripts/mission_incar.probe`) it was 0, in a car,
    /// only while he sat at the wheel with the door shut, and the car's
    /// block counts. It was 2, on foot, the rest of the time, getting in
    /// and out included, and his ped's block counts. The scan skips state
    /// 1, whose writers are not read. `None` while gfa-sim cannot place him.
    fn trigger_block(&self) -> Option<(i32, i32)>;

    /// The model of the car he is in (the scan's state 0), else `None`.
    /// A driver of model 47 sets off no `MPHONES` (0xf4075).
    fn trigger_car_model(&self) -> Option<u8>;

    /// His ped is in a car (its +0x66 is 7, `IS_PED_IN_CAR`'s test): in the
    /// probe, from when he sat down until he was out of it again, getting
    /// out included. The car's model.
    fn player_in_car(&self) -> Option<u8>;

    /// His health (0x7af40 for a `PLAYER`: his ped's +0x44), 100 full.
    fn player_health(&self) -> i32;

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

    /// `PARKED_ON`'s car (GTA8.EXE's 0xedfd0, then 0xefd10): the original's
    /// model `model` standing at block `[bx, by, bz]`'s centre, turned
    /// `turn` (1024 steps a turn). The map loader hands a parked car's
    /// rotation to the same routine (0x19ab0) in the same place, so the turn
    /// reads as the map's own cars' does. The car is the mission's: the
    /// fleet keeps it however far away he goes. Its handle, or `None` when
    /// none could be made (the style has no such model, or the fleet is
    /// full).
    fn car_on(&mut self, model: i32, block: [i32; 3], turn: i32) -> Option<u64>;

    /// Where car `car` is now, in the original's pixels, 16.16; while he
    /// gets into it or drives it, where it is with him. `None` once it is
    /// gone.
    fn car_pos(&self, car: u64) -> Option<(i32, i32)>;

    /// How whole car `car` is (0x7af40): 100 less its damage, and 0 once
    /// it is written off or gone.
    fn car_health(&self, car: u64) -> i32;

    /// The car he is in, by his state (record +0xb9 0, +0xbd the car): at
    /// the wheel with the door shut. `STEAL`'s test (0xee930) comes to the
    /// same: the car's driver is his ped, in it, and his ped's sprite
    /// (+0x17) is 0, which in the probe it was from when the door shut.
    /// `None` on foot, while he gets in or out, and while the door swings.
    fn player_car(&self) -> Option<u64>;

    /// A door made in the map as its section loads (0xe7e10): the blocks
    /// it stands between flagged flat (0x69670 with 0x80), the face it
    /// replaces cleared (the renderer draws the door there), the floor tile
    /// under its row, and its wall, shut.
    fn door_build(&mut self, _door: &DoorPlace) {}

    /// A door's wall solid (shut, closing) or clear (opening, open).
    fn door_wall(&mut self, _door: &DoorPlace, _solid: bool) {}

    /// The declared block made field, for good, as the door opens (0x69450
    /// with 0x40).
    fn door_field(&mut self, _door: &DoorPlace) {}

    /// The garage door's sound at the door, as it comes to rest (0x15e20
    /// with 0x26).
    fn door_sound(&mut self, _door: &DoorPlace) {}

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

    /// The DOS camera's two settings come from gfa-sim's model of them
    /// ([`crate::view::SimCamera`]), which rests at 80 where GTA8.EXE has 78
    /// on foot (it takes his height off), so the rectangle is a pixel wider
    /// and the arrow stands a pixel further out.
    fn player_view(&self) -> PlayerView {
        let (x, y) = match self.in_car() {
            Some(d) => (d.veh.ox, d.veh.oy),
            None => (i64::from(self.player.x), i64::from(self.player.y)),
        };
        // gfa-sim's 16.16 pixels are half the original's.
        let (cx, cy) = self.view_centre();
        let centre = ((cx >> 15) as i32, (cy >> 15) as i32);
        let depth = self.camera.lens();
        PlayerView {
            at: ((x << 1) as i32, (y << 1) as i32),
            rect: PlayerView::rect_round(centre, depth, self.camera.plane()),
            depth,
        }
    }

    /// While he drives, the car's block, which is his too (he moves with it,
    /// the door swinging or not). `None` while he drowns, whose state is not
    /// read.
    fn trigger_block(&self) -> Option<(i32, i32)> {
        use crate::world::Doing;
        match self.doing {
            Doing::Driving => {
                let d = self.driven.as_ref()?;
                Some(((d.veh.ox >> 21) as i32, (d.veh.oy >> 21) as i32))
            }
            Doing::OnFoot
            | Doing::Vaulting(_)
            | Doing::Sliding(_)
            | Doing::Riding(_)
            | Doing::Entering(_)
            | Doing::Exiting(_) => Some((self.player.x >> 21, self.player.y >> 21)),
            Doing::Drowning(_) => None,
        }
    }

    fn trigger_car_model(&self) -> Option<u8> {
        self.at_wheel().map(|d| d.veh.model_id)
    }

    fn player_in_car(&self) -> Option<u8> {
        self.in_car().map(|d| d.veh.model_id)
    }

    fn player_health(&self) -> i32 {
        self.health.max(0)
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

    /// The car stands on the block's lowest ground, as a crate does. The
    /// original's 0xefd10 first clears the place of the people, objects
    /// and other cars in the way (0xefbf0, 0xefb60, 0xef8b0), which is not
    /// ported.
    fn car_on(&mut self, model: i32, block: [i32; 3], turn: i32) -> Option<u64> {
        let [bx, by, _] = block;
        let Some(rec) = self.cars.iter().position(|c| i32::from(c.model) == model) else {
            self.say(format!(
                "car: the mission wants model {model}, which this style lacks"
            ));
            return None;
        };
        let layer = self.map.stand_layer(bx, by).unwrap_or(0);
        // gfa-sim's pixels: 32 to a block.
        let x = i64::from(bx * 32 + 16) << 16;
        let y = i64::from(by * 32 + 16) << 16;
        let face = (turn >> 2) & 255;
        let Some(car) = self
            .traffic
            .abandon(&self.cars, rec, x, y, face, layer, -1, 0)
        else {
            self.say(format!(
                "car: no room in the fleet for the mission's model {model}"
            ));
            return None;
        };
        self.traffic.set_mission(car, true);
        self.say(format!(
            "car: the mission's model {model} parked at ({bx},{by}) facing {face}"
        ));
        Some(car)
    }

    fn car_pos(&self, car: u64) -> Option<(i32, i32)> {
        // gfa-sim's 16.16 pixels are half the original's.
        let orig = |x: i64, y: i64| Some(((x << 1) as i32, (y << 1) as i32));
        if let crate::world::Doing::Entering(e) = &self.doing
            && e.serial == car
        {
            return orig(e.car.x.into(), e.car.y.into());
        }
        if let Some(d) = self.driven.as_ref().filter(|d| d.serial == car) {
            return orig(d.veh.ox, d.veh.oy);
        }
        let (x, y, ..) = self.traffic.find_car(car)?;
        orig(x, y)
    }

    fn car_health(&self, car: u64) -> i32 {
        let damage = if let crate::world::Doing::Entering(e) = &self.doing
            && e.serial == car
        {
            Some(e.car.damage)
        } else if let Some(d) = self.driven.as_ref().filter(|d| d.serial == car) {
            Some(d.veh.damage)
        } else {
            let n = self.traffic.n;
            self.traffic.cars[..n]
                .iter()
                .find(|c| !c.done && c.serial == car)
                .map(|c| {
                    if c.wrecked {
                        crate::cars::WRECKED
                    } else {
                        c.damage
                    }
                })
        };
        damage.map_or(0, |d| (crate::cars::WRECKED - d).max(0))
    }

    fn player_car(&self) -> Option<u64> {
        self.at_wheel().map(|d| d.serial).filter(|&s| s != 0)
    }

    /// The faces the original sets to the door's animated tile are left
    /// empty here: the renderer draws the door on that edge from
    /// [`crate::mission::Door`]'s frame.
    fn door_build(&mut self, door: &DoorPlace) {
        use gfa_formats::cmp::Face;
        let [x, y, z] = door.block;
        let flat = |b: &mut gfa_formats::cmp::Block| b.type_map |= FLAT;
        match door.facing {
            0 => self.edit_block(x, y, z, |b| {
                flat(b);
                b.faces[Face::East as usize] = 0;
            }),
            1 => {
                self.edit_block(x, y, z, |b| {
                    flat(b);
                    b.faces[Face::East as usize] = 0;
                });
                self.edit_block(x + 1, y, z, |b| {
                    flat(b);
                    b.faces[Face::West as usize] = 0;
                });
            }
            2 => self.edit_block(x, y, z, |b| {
                flat(b);
                b.faces[Face::South as usize] = 0;
            }),
            _ => {
                self.edit_block(x, y, z, |b| {
                    flat(b);
                    b.faces[Face::South as usize] = 0;
                });
                self.edit_block(x, y + 1, z, |b| {
                    flat(b);
                    b.faces[Face::North as usize] = 0;
                });
            }
        }
        if door.lid != 0 && z > 0 {
            for k in -1..=1 {
                let (bx, by) = if door.facing >= 2 { (x + k, y) } else { (x, y + k) };
                self.edit_block(bx, by, z - 1, |b| b.faces[Face::Lid as usize] = door.lid);
            }
        }
        self.door_wall(door, true);
    }

    fn door_wall(&mut self, door: &DoorPlace, solid: bool) {
        let (x, y, z, north) = door.face();
        self.map.set_door_wall(x, y, z, north, solid);
    }

    fn door_field(&mut self, door: &DoorPlace) {
        let [x, y, z] = door.block;
        self.edit_block(x, y, z, |b| b.type_map = (b.type_map & !0x70) | FIELD);
    }

    fn door_sound(&mut self, door: &DoorPlace) {
        let (x, y, z, _) = door.face();
        let centre = |b: i32| (b * 32 + 16) << 16;
        self.emit_sfx(crate::audio::snd::GARAGE_DOOR, centre(x), centre(y), z);
    }
}

/// A block's flat bit (the type word's 0x80).
const FLAT: u16 = 0x80;
/// The field ground type, in the type word's bits 4 to 6.
const FIELD: u16 = 0x40;

impl World {
    /// Change the block at `(x, y, z)`: an empty one where there is none.
    fn edit_block(&mut self, x: i32, y: i32, z: i32, f: impl FnOnce(&mut gfa_formats::cmp::Block)) {
        let mut b = self.map.block(x, y, z).copied().unwrap_or_default();
        f(&mut b);
        self.map.set_block(x, y, z, b);
    }
    /// His car while he sits at its wheel with the door shut: GTA8.EXE's
    /// player state 0.
    fn at_wheel(&self) -> Option<&crate::cars::Driven> {
        self.driven
            .as_ref()
            .filter(|d| self.doing == crate::world::Doing::Driving && d.door_tick < 0)
    }
}
