use gfa_fixed::fx::{BLOCK_SHIFT, FP, block_centre};
use gfa_formats::mission::Section;

use crate::hash::StateHasher;
use crate::map::SimMap;

/// Records, as GTA8.EXE keeps (0x3baafc, 28 bytes each).
pub const MAX_PICKUPS: usize = 256;

pub const OBJ_CRATE: usize = 0x54;
pub const OBJ_CRATE_OPEN: usize = 0x55;

const BS: u32 = FP + BLOCK_SHIFT;

pub mod kind {
    pub const PISTOL: i32 = 1;
    pub const MG: i32 = 2;
    pub const ROCKET: i32 = 3;
    pub const FLAME: i32 = 4;
    pub const SPEED: i32 = 6;
    pub const BRIBE: i32 = 9;
    pub const ARMOUR: i32 = 10;
    pub const MULTIPLIER: i32 = 11;
    pub const JAILFREE: i32 = 12;
    pub const LIFE: i32 = 13;
    pub const FRENZY: i32 = 14;
}

pub fn item_object(kind: i32) -> Option<usize> {
    match kind {
        1..=4 => Some(0x4e + usize::try_from(kind - 1).ok()?),
        6..=8 => Some(0x61),
        9 => Some(0x65),
        10 => Some(0x60),
        11 => Some(0x64),
        12 => Some(0x62),
        13 | 15 => Some(0x63),
        14 => Some(0x5f),
        _ => None,
    }
}

pub fn kind_name(kind: i32) -> &'static str {
    match kind {
        kind::PISTOL => "pistol",
        kind::MG => "machine gun",
        kind::ROCKET => "rocket",
        kind::FLAME => "flame",
        6..=8 => "speed",
        kind::BRIBE => "bribe",
        kind::ARMOUR => "armour",
        kind::MULTIPLIER => "multiplier",
        kind::JAILFREE => "jail free",
        kind::LIFE | 15 => "life",
        kind::FRENZY => "kill frenzy",
        _ => "powerup",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PickupState {
    #[default]
    Gone = 0,
    Crate = 1,
    /// The item, standing in its opened crate.
    Open = 2,
    /// The item with no crate under it: a kill frenzy the script turns on
    /// (GTA8.EXE's 0xd7c10 makes object 0x5f in its state 2 and no 0x55).
    Loose = 3,
}

impl PickupState {
    /// An item the player can take.
    pub fn takeable(self) -> bool {
        matches!(self, PickupState::Open | PickupState::Loose)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pickup {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    pub kind: i32,
    pub amount: i32,
    pub state: PickupState,
}

#[derive(Debug, Clone, Default)]
pub struct Pickups {
    pub p: Vec<Pickup>,
    pub stat_opened: u64,
    pub stat_taken: u64,
}

impl Pickups {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_section(section: &Section, map: &SimMap) -> Self {
        let mut out = Self::new();
        for d in &section.decls {
            if d.kind == "POWERUP" {
                let bx = d.pos[0];
                let by = d.pos[1];
                let kind = d.nums.first().copied().unwrap_or(0);
                let amount = d.nums.get(1).copied().unwrap_or(0);
                out.add_block(map, bx, by, kind, amount);
            }
        }
        out
    }

    pub fn add(&mut self, x: i64, y: i64, layer: i32, kind: i32, amount: i32) -> bool {
        if item_object(kind).is_none() {
            return false;
        }
        let entry = Pickup {
            x,
            y,
            layer,
            kind,
            amount,
            state: PickupState::Crate,
        };
        if let Some(slot) = self.p.iter_mut().find(|p| p.state == PickupState::Gone) {
            *slot = entry;
            return true;
        }
        if self.p.len() >= MAX_PICKUPS {
            return false;
        }
        self.p.push(entry);
        true
    }

    pub fn add_block(&mut self, map: &SimMap, bx: i32, by: i32, kind: i32, amount: i32) -> bool {
        if !(0..256).contains(&bx) || !(0..256).contains(&by) {
            return false;
        }
        if self.at_block(bx, by) {
            return false;
        }
        let Some(layer) = map.stand_layer(bx, by) else {
            return false;
        };
        self.add(
            i64::from(block_centre(bx)),
            i64::from(block_centre(by)),
            layer,
            kind,
            amount,
        )
    }

    /// `POWERUP_ON`'s crate (GTA8.EXE's 0xd7c10): `kind` holding `amount`
    /// on block `(bx, by)`, in the first free record. A kill frenzy (14)
    /// stands loose at once, with no crate. Unlike [`Pickups::add_block`]
    /// it does not look for one already there: a second `POWERUP_ON` makes
    /// a second record, as there.
    ///
    /// The original makes nothing for a block outside 1 to 255 or a height
    /// `bz` outside 1 to 5 (the object routine, 0xae040), or with all 256
    /// records in use. gfa-sim also needs ground to stand it on. GTA8.EXE
    /// passes the block's corner; the probe arena (`weapons_ammo`) shows the
    /// crate in line with the player, who starts at the block's centre, so
    /// it is put there.
    pub fn power_up(
        &mut self,
        map: &SimMap,
        [bx, by, bz]: [i32; 3],
        kind: i32,
        amount: i32,
    ) -> Option<usize> {
        if !(1..=255).contains(&bx) || !(1..=255).contains(&by) || !(1..=5).contains(&bz) {
            return None;
        }
        let layer = map.stand_layer(bx, by)?;
        let entry = Pickup {
            x: i64::from(block_centre(bx)),
            y: i64::from(block_centre(by)),
            layer,
            kind,
            amount,
            state: if kind == kind::FRENZY {
                PickupState::Loose
            } else {
                PickupState::Crate
            },
        };
        if let Some(i) = self.p.iter().position(|p| p.state == PickupState::Gone) {
            self.p[i] = entry;
            return Some(i);
        }
        if self.p.len() >= MAX_PICKUPS {
            return None;
        }
        self.p.push(entry);
        Some(self.p.len() - 1)
    }

    /// The first crate or item standing exactly on block `(bx, by)`'s
    /// place, whatever its state (GTA8.EXE's search in 0xd7fd0 and 0xd8060).
    pub fn first_at(&self, bx: i32, by: i32) -> Option<usize> {
        let (x, y) = (i64::from(block_centre(bx)), i64::from(block_centre(by)));
        self.p
            .iter()
            .position(|p| p.state != PickupState::Gone && p.x == x && p.y == y)
    }

    /// `POWERUP_OFF` (GTA8.EXE's 0xd7fd0): the first crate or item on
    /// block `(bx, by)` taken away. False when there was none.
    pub fn take_off(&mut self, bx: i32, by: i32) -> bool {
        match self.first_at(bx, by) {
            Some(i) => {
                self.p[i].state = PickupState::Gone;
                true
            }
            None => false,
        }
    }

    pub fn at_block(&self, bx: i32, by: i32) -> bool {
        self.p.iter().any(|p| {
            p.state != PickupState::Gone && ((p.x >> BS) as i32) == bx && ((p.y >> BS) as i32) == by
        })
    }

    pub fn remove_block(&mut self, bx: i32, by: i32) -> usize {
        let mut n = 0;
        for p in &mut self.p {
            if p.state != PickupState::Gone
                && ((p.x >> BS) as i32) == bx
                && ((p.y >> BS) as i32) == by
            {
                p.state = PickupState::Gone;
                n += 1;
            }
        }
        n
    }

    pub fn open_at(
        &mut self,
        x: i64,
        y: i64,
        layer: i32,
        radius_px: i32,
        say: &mut dyn FnMut(String),
    ) -> usize {
        let r = i64::from(radius_px) << FP;
        let mut n = 0;
        for (i, p) in self.p.iter_mut().enumerate() {
            if p.state != PickupState::Crate || p.layer != layer {
                continue;
            }
            if (p.x - x).abs() <= r && (p.y - y).abs() <= r {
                p.state = PickupState::Open;
                self.stat_opened += 1;
                n += 1;
                say(format!(
                    "pickups: crate {i} opened at ({},{}) - kind {} ({}), {}",
                    p.x >> BS,
                    p.y >> BS,
                    p.kind,
                    kind_name(p.kind),
                    p.amount
                ));
            }
        }
        n
    }

    pub fn peek_from(
        &self,
        x: i64,
        y: i64,
        layer: i32,
        radius_px: i32,
        from: usize,
    ) -> Option<(usize, i32, i32)> {
        let r = i64::from(radius_px) << FP;
        for (i, p) in self.p.iter().enumerate().skip(from) {
            if !p.state.takeable() || p.layer != layer {
                continue;
            }
            if (p.x - x).abs() <= r && (p.y - y).abs() <= r {
                return Some((i, p.kind, p.amount));
            }
        }
        None
    }

    pub fn take_index(&mut self, index: usize) -> Option<(i32, i32)> {
        let p = self.p.get_mut(index)?;
        if !p.state.takeable() {
            return None;
        }
        let out = (p.kind, p.amount);
        p.state = PickupState::Gone;
        self.stat_taken += 1;
        Some(out)
    }

    pub fn sim_hash(&self, h: &mut StateHasher) {
        h.u32(self.p.len() as u32);
        for p in &self.p {
            h.bytes(&p.x.to_le_bytes());
            h.bytes(&p.y.to_le_bytes());
            h.i32(p.layer);
            h.i32(p.kind);
            h.i32(p.amount);
            h.u8(p.state as u8);
        }
        h.bytes(&self.stat_opened.to_le_bytes());
        h.bytes(&self.stat_taken.to_le_bytes());
    }
}
