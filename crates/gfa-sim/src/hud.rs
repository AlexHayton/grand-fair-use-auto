//! Pure simulation-to-HUD events.
//!
//! `World` records one-shot HUD messages (pager scrolls, bottom briefs, big
//! two-line cards, car name plaques, and bust crime tallies) in a transient
//! queue ([`World::drain_hud`]), never included in [`World::hash`], so
//! determinism and replays are unaffected. Continuous HUD indicators (score,
//! multiplier, lives, wanted level, weapon and ammo, armour, jail-free card,
//! frenzy/speed timers, BUSTED/WASTED/GAME OVER cards) are read directly from
//! `World`'s state by `gfa-app`.

use crate::score::crime;

/// Icon and priority class for the bottom-left brief box.
///
/// Kinds `0..=2` (`Phone`, `Talk`, `Mobile`) carry higher priority and
/// replace kinds `3..=5` (`Info`, `Cop`, `Skull`), matching the original and
/// AmiGTA1 (`gta_main.c:651`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BriefKind {
    Phone = 0,
    Talk = 1,
    Mobile = 2,
    Info = 3,
    Cop = 4,
    Skull = 5,
}

impl BriefKind {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Phone,
            1 => Self::Talk,
            2 => Self::Mobile,
            3 => Self::Info,
            4 => Self::Cop,
            _ => Self::Skull,
        }
    }

    pub fn is_high_priority(self) -> bool {
        (self as u8) <= (Self::Mobile as u8)
    }
}

/// Text payload for a HUD message: either an FXT table key or a literal string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudText {
    Key(String),
    Literal(String),
}

impl HudText {
    pub fn key(k: impl Into<String>) -> Self {
        Self::Key(k.into())
    }

    pub fn literal(s: impl Into<String>) -> Self {
        Self::Literal(s.into())
    }
}

/// A one-shot HUD display event emitted by the simulation during a tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudEvent {
    /// Scroll a message across the top-left pager window.
    Pager { text: HudText, secs: Option<i32> },
    /// Show a wrapped brief along the bottom edge with its icon.
    Brief { kind: BriefKind, text: HudText },
    /// Show a centred big multi-line card (e.g. GOURANGA!, MISSION COMPLETE!).
    Big { text: HudText },
    /// Show the entered vehicle's name on its top-centre plaque (`car<model>`).
    CarName { model: u8 },
    /// Scroll the player's crime tally on a bust.
    BustCrimes { crimes: [u32; crime::COUNT] },
}
