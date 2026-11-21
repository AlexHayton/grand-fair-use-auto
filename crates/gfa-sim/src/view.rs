//! The logical sim view: the area around the player that counts as "on
//! screen" to the simulation.
//!
//! AmiGTA1 derives its spawn rings and on-screen checks from the window
//! width and camera zoom, so a wider window or a zoomed-out camera changes
//! what spawns where, and replays stop matching. Here the simulation keeps
//! its own view (Principle 4): a rectangle of blocks round a centre, which
//! leads a moving player and widens as he speeds up ([`SimCamera`]), from
//! his position and speed only. Gameplay never reads the window, the zoom
//! or the HD setting.
//!
//! It works the other way round too: in normal play the renderer never
//! shows ground outside this view (it caps its zoom-out and pillarboxes
//! wide windows), so nothing comes or goes where the player can see it.
//! The spawn rings sit outside the view: peds a block out, the fleet two to
//! six blocks out, and both are kept until well past it.

use gfa_fixed::fx::{BLOCK_SHIFT, FP};

/// Half-extents of the view, in blocks, around its centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimView {
    pub half_w: i32,
    pub half_h: i32,
    /// Whether the view leads a moving player and widens with his speed
    /// (the game), or sits on him at one size (AmiGTA1, whose camera has
    /// no lead and zooms only on a key: for the oracle).
    pub follows: bool,
}

impl SimView {
    /// What AmiGTA1 uses at its default zoom (256 px wide at 32 px a
    /// block): half-width 5, half-height 4. Use it to compare with the
    /// AmiGTA1 oracle.
    pub const AMIGTA1: SimView = SimView {
        half_w: 5,
        half_h: 4,
        follows: false,
    };

    /// The game's view at rest. The follow camera's default (8 blocks
    /// above a street on layer 2, 55 degree field of view) shows 10.9
    /// blocks either side of a 21:9 window and 4.7 above and below, measured
    /// on layer 1, the lowest ground anything stands on in the three
    /// cities. A block more for the car drawn up to a physics step ahead of
    /// its last tick, rounded up; and the height has room for a 16:9 window
    /// to zoom out as far as a 21:9 one.
    pub const GAME: SimView = SimView {
        half_w: 12,
        half_h: 7,
        follows: true,
    };

    /// The view at `zoom` (16.16, 1.0 at rest): each half-extent scaled and
    /// rounded up.
    pub fn at(&self, zoom: i32) -> SimView {
        let scale = |h: i32| ((i64::from(h) * i64::from(zoom) + 0xFFFF) >> FP) as i32;
        SimView {
            half_w: scale(self.half_w),
            half_h: scale(self.half_h),
            follows: self.follows,
        }
    }

    /// Is block `(bx, by)` inside the view centred on the 16.16 point
    /// `(cx, cy)`?
    pub fn contains_block(&self, cx: i32, cy: i32, bx: i32, by: i32) -> bool {
        let (ox, oy) = (cx >> (FP + BLOCK_SHIFT), cy >> (FP + BLOCK_SHIFT));
        (bx - ox).abs() <= self.half_w && (by - oy).abs() <= self.half_h
    }
}

/// 1.0 in 16.16: the zoom at rest.
pub const ZOOM_ONE: i32 = 1 << FP;
/// The zoom at full speed: half as much again of the city each way.
pub const ZOOM_MAX: i32 = ZOOM_ONE * 3 / 2;
/// The speed at which the zoom is all the way out (16.16 px a tick): a
/// fast car's top speed (the sports cars make about 10).
const ZOOM_FULL_SPEED: i64 = 10 << FP;
/// Ticks of travel the view leads by: Carnage3D's 0.35 seconds.
const LEAD_TICKS: i64 = 17;
/// The lead is at most this fraction of the view's half-extent, so a fast
/// car still has two thirds of the screen ahead of it and a third behind.
const LEAD_SHARE: i64 = 3;
/// Each tick the lead closes an eighth of the way to what it wants, and the
/// zoom a thirty-second: about a sixth and two thirds of a second.
const LEAD_EASE: i64 = 8;
const ZOOM_EASE: i64 = 32;

/// Where the simulation's view is: ahead of a moving player, and wider the
/// faster he goes. Ours (decision log, 2026-11-21): AmiGTA1 centres on the
/// player at a fixed size; the lead is Carnage3D's, and the zoom-out with
/// speed the original's, with a curve still to measure against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimCamera {
    /// Offset of the view's centre from the player, 16.16 px.
    pub lead: (i64, i64),
    /// Size against the view at rest, 16.16: [`ZOOM_ONE`] to [`ZOOM_MAX`].
    pub zoom: i32,
}

impl Default for SimCamera {
    fn default() -> Self {
        SimCamera::REST
    }
}

impl SimCamera {
    pub const REST: SimCamera = SimCamera {
        lead: (0, 0),
        zoom: ZOOM_ONE,
    };

    /// One tick, the player moving at `(vx, vy)` (16.16 px a tick). A view
    /// that does not follow stays at rest.
    pub fn follow(&mut self, view: SimView, vx: i64, vy: i64) {
        if !view.follows {
            *self = SimCamera::REST;
            return;
        }
        let speed = vx.abs().max(vy.abs()).min(ZOOM_FULL_SPEED);
        let want = i64::from(ZOOM_ONE) + i64::from(ZOOM_MAX - ZOOM_ONE) * speed / ZOOM_FULL_SPEED;
        self.zoom += ((want - i64::from(self.zoom)) / ZOOM_EASE) as i32;
        let now = view.at(self.zoom);
        let cap = |half: i32| (i64::from(half) << (FP + BLOCK_SHIFT)) / LEAD_SHARE;
        let ease = |lead: i64, v: i64, half: i32| {
            let want = (v * LEAD_TICKS).clamp(-cap(half), cap(half));
            lead + (want - lead) / LEAD_EASE
        };
        self.lead = (
            ease(self.lead.0, vx, now.half_w),
            ease(self.lead.1, vy, now.half_h),
        );
    }

    /// The view's centre for a player at `(x, y)` (16.16).
    pub fn centre(&self, x: i64, y: i64) -> (i64, i64) {
        (x + self.lead.0, y + self.lead.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gfa_fixed::fx::block_centre;

    #[test]
    fn contains() {
        let (cx, cy) = (block_centre(100), block_centre(50));
        let v = SimView::AMIGTA1;
        assert!(v.contains_block(cx, cy, 105, 46));
        assert!(!v.contains_block(cx, cy, 106, 50));
        assert!(!v.contains_block(cx, cy, 100, 55));
    }

    #[test]
    fn zoom_scales_and_rounds_up() {
        assert_eq!(SimView::GAME.at(ZOOM_ONE), SimView::GAME);
        let wide = SimView::GAME.at(ZOOM_MAX);
        assert_eq!((wide.half_w, wide.half_h), (18, 11));
        assert_eq!(SimView::GAME.at(ZOOM_ONE + 1).half_w, 13);
    }

    #[test]
    fn amigta1s_view_stays_on_him() {
        let mut c = SimCamera::REST;
        for _ in 0..100 {
            c.follow(SimView::AMIGTA1, 10 << FP, 0);
        }
        assert_eq!(c, SimCamera::REST);
    }

    #[test]
    fn the_view_leads_and_widens_at_speed_then_settles() {
        let mut c = SimCamera::REST;
        let v = SimView::GAME;
        // Flat out south for five seconds.
        for _ in 0..250 {
            c.follow(v, 0, 10 << FP);
        }
        assert!(c.zoom > ZOOM_MAX - ZOOM_ONE / 64, "zoom {}", c.zoom);
        let half_h = i64::from(v.at(c.zoom).half_h) << (FP + BLOCK_SHIFT);
        assert_eq!(c.lead.0, 0);
        // Capped at a third of the half-height: the car stays on screen.
        assert!(
            c.lead.1 > 0 && c.lead.1 <= half_h / 3,
            "lead {}",
            c.lead.1 >> FP
        );
        assert!(c.lead.1 > half_h / 3 - (4 << FP));
        // Stopped: back to rest, the lead to within a pixel.
        for _ in 0..500 {
            c.follow(v, 0, 0);
        }
        assert!(c.zoom - ZOOM_ONE < 64, "zoom {}", c.zoom);
        assert!(c.lead.1.abs() < 1 << FP, "lead {}", c.lead.1);
    }
}
