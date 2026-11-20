//! Keys and mouse: the player's controls, and the camera.
//!
//! - Arrows or WASD: run forward and back, turn. Shift walks.
//! - Return gets in and out of cars; Space is the handbrake and jump;
//!   Ctrl fires; Tab is the special key; Z and X change weapon.
//! - Scroll wheel, `+`/`-` or PageUp/PageDown zoom (camera height).
//! - `F` toggles free look: drag with the right mouse button to tilt and
//!   turn. `R` returns to straight down.
//! - `C` detaches the camera to fly over the city with the movement keys
//!   (Shift is faster); the player stands still meanwhile.
//!
//! The keys reach the simulation only as a [`TickInput`] per tick.

use std::collections::HashSet;

use gfa_render::camera::{Camera, FollowCamera};
use gfa_render::city_mesh::BLOCK_PX;
use gfa_render::glam::{Vec2, Vec3};
use gfa_sim::input::{Buttons, TickInput};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

const PX: f32 = BLOCK_PX as f32;
/// Top of the tallest column: the eye never goes below the rooftops.
const ROOF_Z: f32 = 6.0 * PX;

pub struct Controls {
    held: HashSet<KeyCode>,
    pub follow: FollowCamera,
    /// What the camera looks at, map pixels: the player, or in fly mode a
    /// point moved with the keys.
    pub target: Vec3,
    pub free_look: bool,
    /// The camera is detached from the player.
    pub flying: bool,
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
            flying: false,
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
            KeyCode::KeyC => self.flying = !self.flying,
            KeyCode::Equal | KeyCode::NumpadAdd | KeyCode::PageUp => self.follow.zoom(0.8),
            KeyCode::Minus | KeyCode::NumpadSubtract | KeyCode::PageDown => self.follow.zoom(1.25),
            _ => {}
        }
    }

    fn any(&self, keys: &[KeyCode]) -> bool {
        keys.iter().any(|k| self.held.contains(k))
    }

    const UP: [KeyCode; 2] = [KeyCode::ArrowUp, KeyCode::KeyW];
    const DOWN: [KeyCode; 2] = [KeyCode::ArrowDown, KeyCode::KeyS];
    const LEFT: [KeyCode; 2] = [KeyCode::ArrowLeft, KeyCode::KeyA];
    const RIGHT: [KeyCode; 2] = [KeyCode::ArrowRight, KeyCode::KeyD];
    const SHIFT: [KeyCode; 2] = [KeyCode::ShiftLeft, KeyCode::ShiftRight];

    /// The player's buttons this tick. Nothing while the camera flies.
    pub fn input(&self) -> TickInput {
        if self.flying {
            return TickInput::default();
        }
        let b = Buttons::NONE
            .with(Buttons::UP, self.any(&Self::UP))
            .with(Buttons::DOWN, self.any(&Self::DOWN))
            .with(Buttons::LEFT, self.any(&Self::LEFT))
            .with(Buttons::RIGHT, self.any(&Self::RIGHT))
            .with(Buttons::WALK, self.any(&Self::SHIFT))
            .with(
                Buttons::ENTER,
                self.any(&[KeyCode::Enter, KeyCode::NumpadEnter]),
            )
            .with(Buttons::HANDBRAKE, self.any(&[KeyCode::Space]))
            .with(
                Buttons::FIRE,
                self.any(&[KeyCode::ControlLeft, KeyCode::ControlRight]),
            )
            .with(Buttons::SPECIAL, self.any(&[KeyCode::Tab]))
            .with(Buttons::WEAPON_PREV, self.any(&[KeyCode::KeyZ]))
            .with(Buttons::WEAPON_NEXT, self.any(&[KeyCode::KeyX]));
        TickInput::new(b)
    }

    /// Advance the camera by `dt` seconds, following `player` (position and
    /// velocity, map pixels) unless flying.
    pub fn update(&mut self, dt: f32, player: Option<(Vec3, Vec3)>) {
        match player {
            Some((pos, velocity)) if !self.flying => {
                self.target = pos;
                self.velocity = velocity;
            }
            _ => self.fly(dt),
        }
        self.follow
            .update(self.target, self.velocity, dt, ROOF_Z + PX);
    }

    fn fly(&mut self, dt: f32) {
        let axis = |neg: [KeyCode; 2], pos: [KeyCode; 2]| {
            f32::from(u8::from(self.any(&pos))) - f32::from(u8::from(self.any(&neg)))
        };
        let x = axis(Self::LEFT, Self::RIGHT);
        let y = axis(Self::UP, Self::DOWN);
        let fast = self.any(&Self::SHIFT);
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
