//! The render camera: a perspective view, straight down by default.
//!
//! Render-side only. The simulation never sees this camera (Principle 4): its
//! "on screen" tests use its own logical view (`gfa_sim::view`), so zoom,
//! aspect ratio and Retina scaling cannot change gameplay. The traffic goes
//! the other way: in normal play this camera shows no more ground than that
//! view, [`max_depth`] capping the zoom-out and [`pillarbox`] the width, so
//! nothing comes or goes in sight.
//!
//! Positions are map pixels (64 per block): `x` east, `y` south, `z` up.
//! That frame is left-handed, so the view matrix flips `y` first and works in
//! an ordinary right-handed frame from there.
//!
//! Source for the follow behaviour: Carnage3D `FollowCameraController.cpp`
//! (MIT): 55° vertical field of view, scroll zoom on height. Its lead ahead
//! of a moving car is now the simulation's. Implementation ours.

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::city_mesh::BLOCK_PX;

const PX: f32 = BLOCK_PX as f32;

/// Carnage3D's vertical field of view.
pub const FOV_Y_DEGREES: f32 = 55.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Eye position in map pixels.
    pub eye: Vec3,
    /// Heading in radians: 0 has north at the top of the screen; positive
    /// turns the view clockwise.
    pub yaw: f32,
    /// Tilt from straight down, in radians: 0 looks straight down.
    pub pitch: f32,
    pub fov_y: f32,
}

impl Camera {
    /// Looking straight down from `eye`.
    pub fn top_down(eye: Vec3) -> Self {
        Camera {
            eye,
            yaw: 0.0,
            pitch: 0.0,
            fov_y: FOV_Y_DEGREES.to_radians(),
        }
    }

    /// The camera's orientation in the right-handed render frame
    /// `(x, -y, z)`: forward and up vectors.
    fn basis(&self) -> (Vec3, Vec3) {
        let tilt = Quat::from_rotation_x(self.pitch);
        let turn = Quat::from_rotation_z(-self.yaw);
        let forward = turn * tilt * Vec3::NEG_Z;
        let up = turn * tilt * Vec3::Y;
        (forward, up)
    }

    pub fn view(&self) -> Mat4 {
        let flip = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
        let eye = flip.transform_point3(self.eye);
        let (forward, up) = self.basis();
        Mat4::look_to_rh(eye, forward, up) * flip
    }

    pub fn projection(&self, aspect: f32) -> Mat4 {
        // Near: a sixteenth of a block. Far: well past the map's diagonal at
        // any sensible height.
        Mat4::perspective_rh(self.fov_y, aspect.max(1e-3), PX / 16.0, PX * 512.0)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        self.projection(aspect) * self.view()
    }

    /// Where the ray through normalised screen point `ndc` (`-1..1`, `y` up)
    /// meets the horizontal plane `z`, if it does.
    pub fn ray_to_plane(&self, aspect: f32, ndc: [f32; 2], z: f32) -> Option<Vec3> {
        let inv = self.view_proj(aspect).inverse();
        let near = inv.project_point3(Vec3::new(ndc[0], ndc[1], 0.0));
        let far = inv.project_point3(Vec3::new(ndc[0], ndc[1], 1.0));
        let dir = far - near;
        if dir.z.abs() < 1e-6 {
            return None;
        }
        let t = (z - near.z) / dir.z;
        (t >= 0.0).then(|| near + dir * t)
    }

    /// Width and height of the ground visible at `depth` below the eye, for a
    /// straight-down camera.
    pub fn footprint(&self, aspect: f32, depth: f32) -> (f32, f32) {
        let h = 2.0 * depth * (self.fov_y / 2.0).tan();
        (h * aspect, h)
    }
}

/// Six planes, each `(normal, d)` with the inside where `n·p + d >= 0`.
#[derive(Debug, Clone, Copy)]
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    /// From a view-projection matrix with depth `0..1` (Gribb and Hartmann).
    pub fn new(view_proj: &Mat4) -> Self {
        let m = view_proj.transpose();
        let (r0, r1, r2, r3) = (m.x_axis, m.y_axis, m.z_axis, m.w_axis);
        let planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2, r3 - r2].map(|p| {
            let len = p.truncate().length();
            if len > 0.0 { p / len } else { p }
        });
        Frustum { planes }
    }

    /// Whether any part of the box may be visible.
    pub fn intersects_box(&self, min: Vec3, max: Vec3) -> bool {
        self.planes.iter().all(|p| {
            let n = p.truncate();
            // The corner furthest along the plane's normal.
            let corner = Vec3::select(n.cmpge(Vec3::ZERO), max, min);
            n.dot(corner) + p.w >= 0.0
        })
    }
}

/// Windows wider than this are pillarboxed. The simulation's view is sized
/// for 21:9; a wider picture would show ground where people and cars come
/// and go.
pub const MAX_ASPECT: f32 = 21.0 / 9.0;

/// Blocks of the simulation's view kept spare round what the camera shows:
/// the car is drawn up to a physics step (0.8 of a block at top speed) ahead
/// of its last tick, and a person's sprite pokes a little out of his block.
pub const VIEW_MARGIN_BLOCKS: f32 = 1.0;

/// The part of a `width` by `height` window the scene is drawn in, as
/// `[x, y, w, h]`: all of it, or a centred 21:9 band with bars either side.
pub fn pillarbox(width: u32, height: u32) -> [u32; 4] {
    let max_w = ((height as f32 * MAX_ASPECT).floor() as u32).max(1);
    if width <= max_w {
        [0, 0, width, height]
    } else {
        [(width - max_w) / 2, 0, max_w, height]
    }
}

/// The highest the eye may be above the lowest ground (map pixels) for a
/// straight-down picture of `aspect` to show no more than the simulation's
/// view, `half_w` by `half_h` blocks round the point it centres on, with
/// [`VIEW_MARGIN_BLOCKS`] spare.
pub fn max_depth(half_w: i32, half_h: i32, aspect: f32) -> f32 {
    let t = (FOV_Y_DEGREES.to_radians() / 2.0).tan();
    let aspect = aspect.clamp(1e-3, MAX_ASPECT);
    let w = (half_w as f32 - VIEW_MARGIN_BLOCKS) / (t * aspect);
    let h = (half_h as f32 - VIEW_MARGIN_BLOCKS) / t;
    w.min(h).max(0.0) * PX
}

/// The game camera: straight down over a target, at a height the player
/// zooms.
///
/// The target already carries the simulation's lead, and the zoom passed
/// in is the simulation's speed zoom, so the camera adds no smoothing of
/// its own across the ground: whatever it lagged by would show ground the
/// simulation does not count as on screen. Only the height eases.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FollowCamera {
    /// Point followed, map pixels.
    pub target: Vec3,
    /// The player's choice: the eye's height above the target at rest, map
    /// pixels.
    pub height: f32,
    /// The highest `height` the cap let through on the last update.
    room: f32,
    eye: Vec3,
}

impl FollowCamera {
    /// On foot: about eight blocks above the target.
    pub const DEFAULT_HEIGHT: f32 = 8.0 * PX;
    pub const MIN_HEIGHT: f32 = 3.0 * PX;
    pub const MAX_HEIGHT: f32 = 40.0 * PX;
    /// How quickly the height eases, per second.
    const EASE: f32 = 8.0;

    pub fn new(target: Vec3) -> Self {
        FollowCamera {
            target,
            height: Self::DEFAULT_HEIGHT,
            room: Self::MAX_HEIGHT,
            eye: target + Vec3::Z * Self::DEFAULT_HEIGHT,
        }
    }

    /// Multiply the height (scroll wheel), within limits. Zooming out
    /// stops where the cap did on the last update, so scrolling back in
    /// answers at once.
    pub fn zoom(&mut self, factor: f32) {
        let mut h = self.height * factor;
        if factor > 1.0 {
            h = h.min(self.room.max(self.height));
        }
        self.height = h.clamp(Self::MIN_HEIGHT, Self::MAX_HEIGHT);
    }

    /// Over `target` (map pixels) at `zoom` times the player's height,
    /// eased over `dt` seconds. The eye stays below `ceiling` when there is
    /// one (the zoom cap, in normal play) and above `min_eye_z` (the
    /// roofs).
    pub fn update(
        &mut self,
        target: Vec3,
        zoom: f32,
        ceiling: Option<f32>,
        dt: f32,
        min_eye_z: f32,
    ) {
        self.target = target;
        let zoom = zoom.max(1e-3);
        let k = 1.0 - (-Self::EASE * dt).exp();
        let want = target.z + self.height * zoom;
        let mut z = self.eye.z + (want - self.eye.z) * k;
        self.room = Self::MAX_HEIGHT;
        if let Some(c) = ceiling {
            z = z.min(c);
            self.room = ((c - target.z) / zoom).max(Self::MIN_HEIGHT);
        }
        self.eye = Vec3::new(target.x, target.y, z.max(min_eye_z));
    }

    /// Jump straight to the target, without easing.
    pub fn snap(&mut self, min_eye_z: f32) {
        self.eye = self.target + Vec3::Z * self.height;
        self.eye.z = self.eye.z.max(min_eye_z);
    }

    pub fn camera(&self) -> Camera {
        Camera::top_down(self.eye)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-2
    }

    #[test]
    fn top_down_puts_north_up_and_east_right() {
        let cam = Camera::top_down(Vec3::new(1000.0, 1000.0, 600.0));
        let vp = cam.view_proj(1.0);
        let centre = vp.project_point3(Vec3::new(1000.0, 1000.0, 0.0));
        let east = vp.project_point3(Vec3::new(1100.0, 1000.0, 0.0));
        let north = vp.project_point3(Vec3::new(1000.0, 900.0, 0.0));
        assert!(centre.x.abs() < 1e-4 && centre.y.abs() < 1e-4);
        assert!(east.x > 0.0 && east.y.abs() < 1e-4, "east is right");
        assert!(north.y > 0.0 && north.x.abs() < 1e-4, "north is up");
        // Higher things are nearer: smaller depth.
        let roof = vp.project_point3(Vec3::new(1000.0, 1000.0, 300.0));
        assert!(roof.z < centre.z);
    }

    #[test]
    fn rays_hit_the_ground_under_the_cursor() {
        let cam = Camera::top_down(Vec3::new(500.0, 700.0, 640.0));
        let hit = cam.ray_to_plane(1.5, [0.0, 0.0], 64.0).unwrap();
        assert!(approx(hit, Vec3::new(500.0, 700.0, 64.0)));
        let (w, h) = cam.footprint(1.5, 576.0);
        let edge = cam.ray_to_plane(1.5, [1.0, 1.0], 64.0).unwrap();
        let want = Vec3::new(500.0 + w / 2.0, 700.0 - h / 2.0, 64.0);
        assert!((edge - want).length() < 0.5, "{edge} vs {want}");
    }

    #[test]
    fn frustum_culls_boxes_off_screen() {
        let cam = Camera::top_down(Vec3::new(1000.0, 1000.0, 600.0));
        let f = Frustum::new(&cam.view_proj(1.0));
        let b = |x: f32, y: f32| {
            f.intersects_box(Vec3::new(x, y, 0.0), Vec3::new(x + 64.0, y + 64.0, 384.0))
        };
        assert!(b(980.0, 980.0));
        assert!(!b(3000.0, 1000.0));
        assert!(!b(1000.0, -2000.0));
        // Above the eye is behind the near plane.
        assert!(!f.intersects_box(
            Vec3::new(990.0, 990.0, 700.0),
            Vec3::new(1010.0, 1010.0, 800.0)
        ));
    }

    #[test]
    fn wide_windows_are_pillarboxed() {
        assert_eq!(pillarbox(1920, 1080), [0, 0, 1920, 1080]);
        assert_eq!(pillarbox(2520, 1080), [0, 0, 2520, 1080]);
        assert_eq!(pillarbox(5120, 1440), [880, 0, 3360, 1440]);
        // Tall windows are not letterboxed: they show less across.
        assert_eq!(pillarbox(800, 1600), [0, 0, 800, 1600]);
    }

    #[test]
    fn the_cap_keeps_the_picture_inside_the_view() {
        for aspect in [0.5, 4.0 / 3.0, 16.0 / 9.0, MAX_ASPECT] {
            let d = max_depth(12, 7, aspect);
            let cam = Camera::top_down(Vec3::new(0.0, 0.0, d));
            let (w, h) = cam.footprint(aspect, d);
            assert!(
                w / 2.0 <= (12.0 - VIEW_MARGIN_BLOCKS) * PX + 0.01,
                "{aspect}: {w}"
            );
            assert!(
                h / 2.0 <= (7.0 - VIEW_MARGIN_BLOCKS) * PX + 0.01,
                "{aspect}: {h}"
            );
        }
        // Wider than 21:9 is drawn at 21:9.
        assert_eq!(max_depth(12, 7, 4.0), max_depth(12, 7, MAX_ASPECT));
    }

    #[test]
    fn follow_camera_sits_over_the_target_under_the_cap() {
        let mut fc = FollowCamera::new(Vec3::new(0.0, 0.0, 128.0));
        let step = |fc: &mut FollowCamera, x: f32, zoom: f32, cap: Option<f32>| {
            for _ in 0..120 {
                fc.update(Vec3::new(x, 0.0, 128.0), zoom, cap, 1.0 / 60.0, 0.0);
            }
            fc.camera().eye
        };
        // Straight over the target, at the player's height: no lag.
        let eye = step(&mut fc, 500.0, 1.0, None);
        assert_eq!((eye.x, eye.y), (500.0, 0.0));
        assert!((eye.z - (128.0 + FollowCamera::DEFAULT_HEIGHT)).abs() < 1.0);
        // The speed zoom raises it; the cap stops it.
        let eye = step(&mut fc, 500.0, 1.5, None);
        assert!((eye.z - (128.0 + 1.5 * FollowCamera::DEFAULT_HEIGHT)).abs() < 1.0);
        let eye = step(&mut fc, 500.0, 1.5, Some(700.0));
        assert_eq!(eye.z, 700.0);
        // Under the cap, zooming out does nothing to the player's choice.
        fc.zoom(1.25);
        assert_eq!(fc.height, FollowCamera::DEFAULT_HEIGHT);
        // With room above, it goes no further than the cap allows...
        step(&mut fc, 500.0, 1.5, Some(1100.0));
        let room = (1100.0 - 128.0) / 1.5;
        for _ in 0..20 {
            fc.zoom(1.25);
        }
        assert!((fc.height - room).abs() < 0.01, "{}", fc.height);
        // ...and zooming in answers at once.
        fc.zoom(0.8);
        assert!((fc.height - room * 0.8).abs() < 0.01);
        // Never below the roofs.
        fc.update(Vec3::ZERO, 1.0, Some(100.0), 10.0, 10_000.0);
        assert_eq!(fc.camera().eye.z, 10_000.0);
    }
}
