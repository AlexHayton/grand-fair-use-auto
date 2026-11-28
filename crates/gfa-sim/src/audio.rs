//! Pure simulation-to-audio events.
//!
//! `World` records one-shot sound effects and speech lines in a transient
//! queue ([`World::drain_audio`]), never included in [`World::hash`], so
//! determinism and replays are unaffected. Continuous loops (engine note,
//! tyre skid, sirens, horns, burning wrecks, radio) are driven directly from
//! `World`'s state by `gfa-app` and `gfa-audio`.
//!
//! Sample and voice numbers come from AmiGTA1's `native/gta_snd.h` (MIT).

/// One-shot sound effects in `LEVEL001`–`003`.
pub mod snd {
    pub const DOOR: u8 = 1;
    pub const PICKUP_GUN: u8 = 2;
    pub const PICKUP_ODD: u8 = 3;
    pub const PICKUP_EVEN: u8 = 4;
    pub const CAR_START: u8 = 5;
    pub const IMPACT_LIGHT: u8 = 6;
    pub const IMPACT_MED: u8 = 7;
    pub const IMPACT_HARD: u8 = 8;
    pub const WALL: u8 = 9;
    pub const CRASH: u8 = 10;
    pub const LANDING: u8 = 0x0e;
    pub const BODY_DOWN: u8 = 0x0f;
    pub const PUNCH: u8 = 0x10;
    pub const DEATH: u8 = 0x11;
    pub const SPLASH: u8 = 0x12;
    pub const SCREAM_HIGH: u8 = 0x17;
    pub const IGNITE: u8 = 0x18;
    pub const BURN_DEATH: u8 = 0x19;
    pub const PHONE: u8 = 0x1c;
    pub const PISTOL: u8 = 0x21;
    pub const MACHINEGUN: u8 = 0x22;
    pub const FLAME: u8 = 0x23;
    pub const ROCKET: u8 = 0x24;
    pub const GARAGE_DOOR: u8 = 0x26;
    pub const EXPLOSION: u8 = 0x27;
    pub const AIR_BRAKE: u8 = 0x29;
    pub const RESPRAY: u8 = 0x2a;
    pub const BULLET_HIT: u8 = 0x2b;
    pub const ENGINE_BASE: u8 = 0x2d;
    pub const HORN_BASE: u8 = 0x3a;
    pub const SKID: u8 = 0x41;
    pub const SIREN_YELP: u8 = 0x42;
    pub const SIREN: u8 = 0x43;
    pub const WRECK_FIRE: u8 = 0x4b;
    pub const FOOT_SHOUT_BASE: u8 = 0x4c;
    pub const RADIO_BASE: u8 = 0x7e;

    /// Sample id for weapon `1..=4` (pistol, MG, rocket, flamethrower).
    pub const fn for_weapon(w: usize) -> Option<u8> {
        match w {
            1 => Some(PISTOL),
            2 => Some(MACHINEGUN),
            3 => Some(ROCKET),
            4 => Some(FLAME),
            _ => None,
        }
    }

    /// Radio snippet sample played when opening another car's door.
    pub const fn for_car_radio(radio: u8) -> Option<u8> {
        match radio {
            0 => Some(RADIO_BASE),
            1 => Some(RADIO_BASE + 1),
            3 => Some(RADIO_BASE + 2),
            4 => Some(RADIO_BASE + 3),
            5 => Some(RADIO_BASE + 4),
            _ => None,
        }
    }
}

/// Speech lines in `VOCALCOM`.
pub mod voice {
    pub const MISSION_COMPLETE: u8 = 1;
    pub const MISSION_FAILED: u8 = 2;
    pub const FRENZY: u8 = 3;
    pub const FRENZY_PASSED: u8 = 4;
    pub const FRENZY_FAILED: u8 = 5;
    pub const LOST: u8 = 6;
    pub const EXTRA_LIFE: u8 = 0x0b;
    pub const BUSTED_WASTED: u8 = 0x0c;
    pub const POWERUP_DONE: u8 = 0x11;
}

/// A one-shot audio event emitted by the simulation during a tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioEvent {
    /// A sample from the level's SFX bank (`LEVEL001`–`003`) at sim position
    /// `(x, y, layer)` (16.16 world pixels at 32 px/block).
    Sfx {
        id: u8,
        x: i32,
        y: i32,
        layer: i32,
    },
    /// A speech line from `VOCALCOM`. When `pos` is `None`, it is an announcer
    /// line at full volume; when `cut` is true, it interrupts any playing line.
    Voice {
        line: u8,
        pos: Option<(i32, i32, i32)>,
        cut: bool,
    },
    /// On-foot Tab shout (`0x4c` or `0x4d` by the voice dice).
    FootShout { x: i32, y: i32, layer: i32 },
    /// Police shout (`kind` + `(ped % 3) * 4`, lines 18..=29).
    CopShout {
        kind: u8,
        ped: u8,
        x: i32,
        y: i32,
        layer: i32,
    },
    /// Driver yelling in the street (lines 30..=50). When `chance_half` is true
    /// (a high-speed ram), the audio layer gates it on a 50% roll.
    StreetYell {
        x: i32,
        y: i32,
        layer: i32,
        chance_half: bool,
    },
    /// A ped frightened by gunfire or a tank shell (`0x17..=0x15` SFX + lines
    /// 62..=69).
    Panic {
        x: i32,
        y: i32,
        layer: i32,
        first: bool,
    },
    /// Kill frenzy cheer (lines 7..=10).
    FrenzyCheer,
}
