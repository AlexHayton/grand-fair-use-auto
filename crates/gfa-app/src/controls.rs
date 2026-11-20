//! The M2 debug camera: fly over a city before there is a player to follow.
//!
//! - Arrows or WASD pan; Shift is faster. The follow camera trails the point
//!   being panned, with its lead-ahead, as it will trail the player in M3.
//! - Scroll wheel, `+`/`-` or PageUp/PageDown zoom (camera height).
//! - `F` toggles free look: drag with the right mouse button to tilt and
//!   turn. `R` returns to straight down.
//!
//! Render-side only; nothing here reaches the simulation.

use std::collections::HashSet;

use gfa_render::camera::{Camera, FollowCamera};
use gfa_render::city_mesh::BLOCK_PX;
use gfa_render::glam::{Vec2, Vec3};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

const PX: f32 = BLOCK_PX as f32;
/// Top of the tallest column: the eye never goes below the rooftops.
const ROOF_Z: f32 = 6.0 * PX;

pub struct Controls {
    held: HashSet<KeyCode>,
    pub follow: FollowCamera,
    /// The point being panned around, map pixels.
    pub target: Vec3,
    pub free_look: bool,
    pub pitch: f32,
    pub yaw: f32,
    dragging: bool,
    last_cursor: Option<Vec2>,
    /// Cursor in window pixels, for the inspector.
    pub cursor: Option<Vec2>,
    velocity: Vec3,
}

impl Controls {
    pub fn new(target: Vec3) -> Self {
        let mut follow = FollowCamera::new(target);
        follow.snap(ROOF_Z + PX);
        Controls {
            held: HashSet::new(),
            follow,
            target,
            free_look: false,
            pitch: 0.0,
            yaw: 0.0,
            dragging: false,
            last_cursor: None,
            cursor: None,
            velocity: Vec3::ZERO,
        }
    }

    pub fn jump_to(&mut self, target: Vec3) {
        self.target = target;
        self.follow.target = target;
        self.follow.snap(ROOF_Z + PX);
    }

    /// Feed a window event. Returns whether it was used.
    pub fn event(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return false;
                };
                match event.state {
                    ElementState::Pressed => {
                        let first = self.held.insert(code);
                        if first {
                            self.pressed(code);
                        }
                    }
                    ElementState::Released => {
                        self.held.remove(&code);
                    }
                }
                true
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.follow.zoom(0.9f32.powf(lines));
                true
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Right,
                ..
            } => {
                self.dragging = *state == ElementState::Pressed && self.free_look;
                self.last_cursor = None;
                true
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = Vec2::new(position.x as f32, position.y as f32);
                self.cursor = Some(p);
                if self.dragging {
                    if let Some(last) = self.last_cursor {
                        let d = p - last;
                        self.yaw += d.x * 0.005;
                        self.pitch = (self.pitch + d.y * 0.005).clamp(0.0, 1.3);
                    }
                    self.last_cursor = Some(p);
                }
                false
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                false
            }
            WindowEvent::Focused(false) => {
                self.held.clear();
                self.dragging = false;
                false
            }
            _ => false,
        }
    }

    fn pressed(&mut self, code: KeyCode) {
        match code {
            KeyCode::KeyF => {
                self.free_look = !self.free_look;
                if !self.free_look {
                    self.dragging = false;
                }
            }
            KeyCode::KeyR => {
                self.pitch = 0.0;
                self.yaw = 0.0;
            }
            KeyCode::Equal | KeyCode::NumpadAdd | KeyCode::PageUp => self.follow.zoom(0.8),
            KeyCode::Minus | KeyCode::NumpadSubtract | KeyCode::PageDown => self.follow.zoom(1.25),
            _ => {}
        }
    }

    fn axis(&self, neg: [KeyCode; 2], pos: [KeyCode; 2]) -> f32 {
        let held = |ks: [KeyCode; 2]| ks.iter().any(|k| self.held.contains(k));
        f32::from(u8::from(held(pos))) - f32::from(u8::from(held(neg)))
    }

    /// Advance by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        let x = self.axis(
            [KeyCode::ArrowLeft, KeyCode::KeyA],
            [KeyCode::ArrowRight, KeyCode::KeyD],
        );
        let y = self.axis(
            [KeyCode::ArrowUp, KeyCode::KeyW],
            [KeyCode::ArrowDown, KeyCode::KeyS],
        );
        let fast =
            self.held.contains(&KeyCode::ShiftLeft) || self.held.contains(&KeyCode::ShiftRight);
        // Pan speed follows the zoom, so the screen moves at a steady rate.
        let speed = self.follow.height * if fast { 3.0 } else { 1.0 };
        let dir = Vec2::new(x, y).normalize_or_zero();
        // Screen directions turn with the free-look heading.
        let (s, c) = self.yaw.sin_cos();
        let world = Vec2::new(dir.x * c - dir.y * s, dir.x * s + dir.y * c);
        self.velocity = Vec3::new(world.x, world.y, 0.0) * speed;
        self.target += self.velocity * dt;
        let max = 256.0 * PX;
        self.target.x = self.target.x.clamp(0.0, max);
        self.target.y = self.target.y.clamp(0.0, max);
        self.follow
            .update(self.target, self.velocity, dt, ROOF_Z + PX);
    }

    pub fn camera(&self) -> Camera {
        let mut cam = self.follow.camera();
        if self.free_look || self.pitch != 0.0 || self.yaw != 0.0 {
            // Orbit: keep looking at the target from the same distance.
            let dist = cam.eye.z - self.target.z;
            cam.pitch = self.pitch;
            cam.yaw = self.yaw;
            let back = Vec2::new(self.yaw.sin(), -self.yaw.cos()) * -(dist * self.pitch.sin());
            cam.eye = Vec3::new(
                self.target.x + back.x,
                self.target.y + back.y,
                self.target.z + dist * self.pitch.cos(),
            )
            .max(Vec3::new(f32::MIN, f32::MIN, ROOF_Z + PX));
        }
        cam
    }
}
