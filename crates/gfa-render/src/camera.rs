//! The render camera: a perspective view, straight down by default.
//!
//! Render-side only. The simulation never sees this camera (Principle 4): its
//! "on screen" tests use a fixed logical view, so zoom, aspect ratio and
//! Retina scaling cannot change gameplay.
//!
//! Positions are map pixels (64 per block): `x` east, `y` south, `z` up.
//! That frame is left-handed, so the view matrix flips `y` first and works in
//! an ordinary right-handed frame from there.
//!
//! Source for the follow behaviour: Carnage3D `FollowCameraController.cpp`
//! (MIT): 55° vertical field of view, scroll zoom on height, a lead ahead of
//! a moving car. Implementation ours.

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

/// The game camera: follows a target from above, zooms by height, and
/// looks ahead of a moving target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FollowCamera {
    /// Point followed, map pixels.
    pub target: Vec3,
    /// Height of the eye above the target, map pixels.
    pub height: f32,
    /// Smoothed look-ahead offset, map pixels.
    lead: Vec3,
    /// Smoothed eye position.
    eye: Vec3,
}

impl FollowCamera {
    /// On foot: about eight blocks above the target.
    pub const DEFAULT_HEIGHT: f32 = 8.0 * PX;
    pub const MIN_HEIGHT: f32 = 3.0 * PX;
    pub const MAX_HEIGHT: f32 = 40.0 * PX;
    /// Seconds of travel the camera looks ahead (Carnage3D: 0.35).
    const LEAD_SECONDS: f32 = 0.35;
    /// How quickly the eye catches up, per second (Carnage3D: 20, ×0.3 in a car).
    const CATCH_UP: f32 = 8.0;

    pub fn new(target: Vec3) -> Self {
        let eye = target + Vec3::Z * Self::DEFAULT_HEIGHT;
        FollowCamera {
            target,
            height: Self::DEFAULT_HEIGHT,
            lead: Vec3::ZERO,
            eye,
        }
    }

    /// Multiply the height (scroll wheel), within limits.
    pub fn zoom(&mut self, factor: f32) {
        self.height = (self.height * factor).clamp(Self::MIN_HEIGHT, Self::MAX_HEIGHT);
    }

    /// Move towards `target`, moving at `velocity` (pixels per second), over
    /// `dt` seconds. Never lower than the rooftops: the eye stays above
    /// `min_eye_z`.
    pub fn update(&mut self, target: Vec3, velocity: Vec3, dt: f32, min_eye_z: f32) {
        self.target = target;
        let k = 1.0 - (-Self::CATCH_UP * dt).exp();
        let lead = Vec3::new(velocity.x, velocity.y, 0.0) * Self::LEAD_SECONDS;
        self.lead += (lead - self.lead) * k;
        let want = target + self.lead + Vec3::Z * self.height;
        self.eye += (want - self.eye) * k;
        self.eye.z = self.eye.z.max(min_eye_z);
    }

    /// Jump straight to the target, without smoothing.
    pub fn snap(&mut self, min_eye_z: f32) {
        self.lead = Vec3::ZERO;
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
    fn follow_camera_leads_and_stays_above_roofs() {
        let mut fc = FollowCamera::new(Vec3::new(0.0, 0.0, 64.0));
        for _ in 0..200 {
            fc.update(
                Vec3::new(0.0, 0.0, 64.0),
                Vec3::new(1000.0, 0.0, 0.0),
                1.0 / 60.0,
                0.0,
            );
        }
        let eye = fc.camera().eye;
        assert!(
            (eye.x - 350.0).abs() < 1.0,
            "leads by 0.35 s of travel: {eye}"
        );
        fc.update(Vec3::ZERO, Vec3::ZERO, 10.0, 10_000.0);
        assert_eq!(fc.camera().eye.z, 10_000.0);
    }
}
