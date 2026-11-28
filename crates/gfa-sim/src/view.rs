//! The logical sim view: the area around the player that counts as "on
//! screen" to the simulation.
//!
//! AmiGTA1 derives its spawn rings and on-screen checks from the window
//! width and camera zoom, so a wider window or a zoomed-out camera changes
//! what spawns where, and replays stop matching. Here the simulation keeps
//! its own view (Principle 4): a rectangle of blocks round a centre, which
//! leads and widens with the driven car's heading and speed
//! ([`SimCamera`]). Gameplay never reads the window, the player's manual
//! zoom or the HD setting.
//!
//! It works the other way round too: in normal play the renderer never
//! shows ground outside this view (it caps its zoom-out and pillarboxes
//! wide windows), so nothing comes or goes where the player can see it.
//! The spawn rings sit outside the view: peds a block out, the fleet two to
//! six blocks out, and both are kept until well past it.

use crate::hash::{SimHash, StateHasher};
use gfa_fixed::fx::{BLOCK_SHIFT, FP};
use gfa_fixed::trig::{ONE_Q14, cos16, sin16};

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
/// The ground footprint multiplier at the DOS camera's maximum zoom-out.
pub const ZOOM_MAX: i32 = ground_zoom(240, 204);
const LENS_REST: i32 = 80;
const PLANE_REST: i32 = 284;
const HALF_PX: i64 = 1 << (FP - 1);
const LEAD_MAX: i64 = 257 * HALF_PX;

/// Convert the two DOS zoom settings to relative ground coverage. The
/// renderer uses one height multiplier at a fixed field of view.
const fn ground_zoom(lens: i32, plane: i32) -> i32 {
    ((256 + lens) as i64 * PLANE_REST as i64 * ZOOM_ONE as i64 / ((256 + LENS_REST) * plane) as i64)
        as i32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ZoomSetting {
    value: i32,
    speed: i32,
}

impl ZoomSetting {
    const fn rest(value: i32) -> Self {
        Self { value, speed: 0 }
    }

    /// Use the prior frame's speed, then accelerate toward the setting.
    /// Both DOS settings move faster when decreasing than increasing.
    fn step(&mut self, target: i32) {
        let delta = target - self.value;
        let direction = delta.signum();
        if self.speed.signum() != direction {
            self.speed = 0;
        }
        let speed = self.speed.abs();
        let distance = if delta < 0 { speed } else { (speed + 1) / 2 };
        self.value += direction * distance.min(8).min(delta.abs());
        if self.value == target {
            *self = Self::rest(target);
        } else {
            self.speed = (self.speed + direction).clamp(-16, 16);
        }
    }
}

/// The view widens with the car's speed and leads toward its bonnet.
/// AmiGTA1's oracle view stays centred at a fixed scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SimCamera {
    /// Offset of the view's centre from the player, 16.16 px.
    pub lead: (i64, i64),
    /// Size against the view at rest, 16.16: [`ZOOM_ONE`] to [`ZOOM_MAX`].
    pub zoom: i32,
    lens: ZoomSetting,
    plane: ZoomSetting,
    lead_rate: (i64, i64),
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
        lens: ZoomSetting::rest(LENS_REST),
        plane: ZoomSetting::rest(PLANE_REST),
        lead_rate: (0, 0),
    };

    /// One DOS frame (7/15 of the 50 Hz ticks). The car supplies its speed
    /// in 16.16 px per physics step and heading in 16.16 of 256 steps.
    pub fn follow(&mut self, view: SimView, car: Option<(i64, i32)>) {
        if !view.follows {
            *self = SimCamera::REST;
            return;
        }
        let (speed, heading) = car.unwrap_or_default();
        let speed_half_px = (speed / HALF_PX).min(16) as i32;
        let lens = (48 + 12 * speed_half_px).max(LENS_REST);
        self.lens.step(lens);
        self.plane.step(324 - lens / 2);
        self.zoom = ground_zoom(self.lens.value, self.plane.value);

        // Fit the horizontal lead to the DOS straight-run samples. Its
        // direction comes from the hull, so it still points forward in reverse.
        let ahead = speed * 16;
        let project =
            |axis: i32| (ahead * i64::from(axis) / i64::from(ONE_Q14)).clamp(-LEAD_MAX, LEAD_MAX);
        step_lead(
            &mut self.lead.0,
            &mut self.lead_rate.0,
            project(sin16(heading)),
        );
        step_lead(
            &mut self.lead.1,
            &mut self.lead_rate.1,
            project(-cos16(heading)),
        );
    }

    /// The view's centre for a player at `(x, y)` (16.16).
    pub fn centre(&self, x: i64, y: i64) -> (i64, i64) {
        (x + self.lead.0, y + self.lead.1)
    }

    /// The DOS camera's first setting now: the player record's +0x30, which
    /// GTA8.EXE copies to 0x1cc1dc every frame and calls the camera's depth
    /// (`docs/MISSION_VM.md`, "The arrow"). 80 at rest.
    pub fn lens(&self) -> i32 {
        self.lens.value
    }

    /// The DOS camera's second setting now: the player record's +0x3c and
    /// +0x40. 284 at rest.
    pub fn plane(&self) -> i32 {
        self.plane.value
    }
}

fn step_lead(lead: &mut i64, rate: &mut i64, target: i64) {
    let delta = target - *lead;
    let step = if delta.signum() == rate.signum() {
        rate.abs().min(delta.abs()) * delta.signum()
    } else {
        0
    };
    *lead += step;
    *rate = if *lead == target {
        0
    } else {
        (*rate + delta.signum() * 2 * HALF_PX).clamp(-8 * HALF_PX, 8 * HALF_PX)
    };
}

impl SimHash for SimCamera {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.bytes(&self.lead.0.to_le_bytes());
        h.bytes(&self.lead.1.to_le_bytes());
        h.i32(self.zoom);
        h.i32(self.lens.value);
        h.i32(self.lens.speed);
        h.i32(self.plane.value);
        h.i32(self.plane.speed);
        h.bytes(&self.lead_rate.0.to_le_bytes());
        h.bytes(&self.lead_rate.1.to_le_bytes());
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
        assert_eq!((wide.half_w, wide.half_h), (25, 15));
        assert_eq!(SimView::GAME.at(ZOOM_ONE + 1).half_w, 13);
    }

    #[test]
    fn amigta1s_view_stays_on_him() {
        let mut c = SimCamera::REST;
        for _ in 0..100 {
            c.follow(SimView::AMIGTA1, Some((10 << FP, 0)));
        }
        assert_eq!(c, SimCamera::REST);
    }

    #[test]
    fn the_zoom_matches_the_dos_runway_frames() {
        let mut c = SimCamera::REST;
        let v = SimView::GAME;
        // Whole half-pixels per DOS frame, truncated from the measured
        // police car's positions. The first sample is the first moving frame.
        let acceleration = [
            1, 2, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 21, 22, 23, 23,
            24, 25, 25, 26, 27, 27, 28,
        ];
        for (frame, speed) in acceleration.into_iter().enumerate() {
            c.follow(v, Some((speed * HALF_PX, 64 << FP)));
            match frame {
                3 => assert_eq!((c.lens.value, c.plane.value), (81, 283)),
                13 => assert_eq!((c.lens.value, c.plane.value), (116, 224)),
                16 => assert_eq!((c.lens.value, c.plane.value), (136, 204)),
                29 => assert_eq!(c.zoom, ZOOM_MAX),
                _ => {}
            }
        }
        // During the coast the lens closes faster and the plane returns
        // more slowly. The first step below the top band begins the ease.
        for _ in 0..10 {
            c.follow(v, Some((15 * HALF_PX, 64 << FP)));
        }
        assert_eq!((c.lens.value, c.plane.value), (228, 210));
    }

    #[test]
    fn the_view_leads_the_bonnet_and_returns_to_rest() {
        let mut c = SimCamera::REST;
        let v = SimView::GAME;
        // Flat out with the bonnet south. The caller supplies speed even
        // when the velocity points the other way.
        for _ in 0..36 {
            c.follow(v, Some((10 << FP, 128 << FP)));
        }
        assert_eq!(c.zoom, ZOOM_MAX);
        assert_eq!(c.lead, (0, LEAD_MAX));
        for _ in 0..36 {
            c.follow(v, None);
        }
        assert_eq!(c, SimCamera::REST);
    }
}
