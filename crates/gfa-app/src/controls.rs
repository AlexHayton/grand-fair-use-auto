//! Keys and mouse: the player's controls, and the camera.
//!
//! The original's keys do what they did in the DOS game's default layout
//! (its README, decision log 2026-11-21), and our own keys keep off them:
//! - Arrows: run forward and back, turn (WASD too). Shift walks (ours).
//! - Return gets in and out of cars; Space is the handbrake and jump;
//!   Ctrl fires; Tab is the special key (horn, siren); Z and X change
//!   weapon; F6 pauses; Esc quits.
//! - Keypad `*` gives every gun 99 rounds, which the original did only with
//!   a name-entry cheat on; until the cheats come (M8) it is always on.
//! - Kept for what they did there, not built yet: F5 retunes the radio
//!   (M8), F7 repeats the last pager message, F9 shows the area's name,
//!   F10 repeats the last message (M7), F8 the frame limiter and F11 the
//!   screen mode (M12). F1 to F4 sent messages in network games, which are
//!   out of scope.
//!
//! Ours, on keys the original leaves free:
//! - Scroll wheel, `+`/`-` or PageUp/PageDown zoom (camera height). In
//!   normal play the zoom-out stops where the picture would show more than
//!   the simulation's view; free look and flying are not capped.
//! - `F` toggles free look: drag with the right mouse button to tilt and
//!   turn. `R` returns to straight down.
//! - `C` detaches the camera to fly over the city with the movement keys
//!   (Shift is faster); the player stands still meanwhile.
//! - `` ` `` shows and hides the debug menu (in the app's event loop); it
//!   starts hidden. Its buttons reach the simulation as presses queued here
//!   ([`Controls::tap`]), held until a tick has run, so a recording replays
//!   them.
//!
//! On a touch screen the web page draws buttons along the bottom for the
//! keys above ([`touch_button`] names them) and holds them here
//! ([`Controls::touch`]) for as long as a finger is on them.
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
/// The lowest the eye goes: a block above the rooftops.
pub(crate) const MIN_EYE_Z: f32 = ROOF_Z + PX;

pub struct Controls {
    held: HashSet<KeyCode>,
    pub follow: FollowCamera,
    /// What the camera looks at, map pixels: the player, or in fly mode a
    /// point moved with the keys.
    pub target: Vec3,
    pub free_look: bool,
    /// The camera is detached from the player.
    pub flying: bool,
    /// F6: the simulation is stopped. The camera still moves.
    pub paused: bool,
    pub pitch: f32,
    pub yaw: f32,
    dragging: bool,
    last_cursor: Option<Vec2>,
    /// Cursor in window pixels, for the inspector.
    pub cursor: Option<Vec2>,
    /// Buttons pressed from the debug menu, held until a tick has run
    /// with them, so they reach the simulation (and its recording) as a
    /// key press would.
    queued: Buttons,
    /// On-screen buttons a finger is holding.
    touched: Buttons,
}

/// The on-screen button for each key, by the name the page uses.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn touch_button(name: &str) -> Option<Buttons> {
    Some(match name {
        "up" => Buttons::UP,
        "down" => Buttons::DOWN,
        "left" => Buttons::LEFT,
        "right" => Buttons::RIGHT,
        "fire" => Buttons::FIRE,
        "enter" => Buttons::ENTER,
        "jump" => Buttons::HANDBRAKE,
        "special" => Buttons::SPECIAL,
        "prev" => Buttons::WEAPON_PREV,
        "next" => Buttons::WEAPON_NEXT,
        _ => return None,
    })
}

impl Controls {
    pub fn new(target: Vec3) -> Self {
        let mut follow = FollowCamera::new(target);
        follow.snap(MIN_EYE_Z);
        Controls {
            held: HashSet::new(),
            follow,
            target,
            free_look: false,
            flying: false,
            paused: false,
            pitch: 0.0,
            yaw: 0.0,
            dragging: false,
            last_cursor: None,
            cursor: None,
            queued: Buttons::NONE,
            touched: Buttons::NONE,
        }
    }

    /// A finger went down on (`down`) or came off an on-screen button.
    pub fn touch(&mut self, b: Buttons, down: bool) {
        self.touched.set(b, down);
    }

    /// Press `b` for the next tick that runs (a debug menu button).
    pub fn tap(&mut self, b: Buttons) {
        self.queued.set(b, true);
    }

    /// Is a debug menu press waiting for a tick?
    pub fn tapped(&self, b: Buttons) -> bool {
        self.queued.contains(b)
    }

    /// Ticks have run with the queued presses: let them go.
    pub fn ticked(&mut self) {
        self.queued = Buttons::NONE;
    }

    pub fn jump_to(&mut self, target: Vec3) {
        self.target = target;
        self.follow.target = target;
        self.follow.snap(MIN_EYE_Z);
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
                self.touched = Buttons::NONE;
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
            KeyCode::F6 => self.paused = !self.paused,
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

    /// The player's buttons this tick. Nothing but the debug menu's while
    /// the camera flies.
    pub fn input(&self) -> TickInput {
        if self.flying {
            return TickInput::new(self.queued);
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
            .with(Buttons::WEAPON_NEXT, self.any(&[KeyCode::KeyX]))
            .with(Buttons::ALL_WEAPONS, self.any(&[KeyCode::NumpadMultiply]));
        let b = Buttons(b.0 | self.queued.0 | self.touched.0);
        TickInput::new(b)
    }

    /// Advance the camera by `dt` seconds, following `player` (the point
    /// to centre on, map pixels, and the simulation's zoom) unless flying.
    /// `ceiling` is the highest the eye may go in normal play.
    pub fn update(&mut self, dt: f32, player: Option<(Vec3, f32)>, ceiling: Option<f32>) {
        let (zoom, ceiling) = match player {
            Some((pos, zoom)) if !self.flying => {
                self.target = pos;
                (zoom, ceiling.filter(|_| !self.free_look))
            }
            _ => {
                self.fly(dt);
                (1.0, None)
            }
        };
        self.follow
            .update(self.target, zoom, ceiling, dt, MIN_EYE_Z);
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
        self.target += Vec3::new(world.x, world.y, 0.0) * speed * dt;
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
            .max(Vec3::new(f32::MIN, f32::MIN, MIN_EYE_Z));
        }
        cam
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_press_is_held_until_a_tick_has_run() {
        let mut c = Controls::new(Vec3::ZERO);
        assert!(!c.input().held(Buttons::ALL_WEAPONS));
        c.tap(Buttons::ALL_WEAPONS);
        assert!(c.tapped(Buttons::ALL_WEAPONS));
        assert!(c.input().held(Buttons::ALL_WEAPONS));
        // Flying, the keys are not the player's, but the menu's still are.
        c.flying = true;
        assert!(c.input().held(Buttons::ALL_WEAPONS));
        c.ticked();
        assert!(!c.input().held(Buttons::ALL_WEAPONS));
    }

    #[test]
    fn a_touched_button_is_held_until_the_finger_lifts() {
        let mut c = Controls::new(Vec3::ZERO);
        let up = touch_button("up").unwrap();
        let fire = touch_button("fire").unwrap();
        c.touch(up, true);
        c.touch(fire, true);
        assert!(c.input().held(Buttons::UP) && c.input().held(Buttons::FIRE));
        // Ticks do not release it, unlike a menu press.
        c.ticked();
        assert!(c.input().held(Buttons::UP));
        c.touch(up, false);
        assert!(!c.input().held(Buttons::UP) && c.input().held(Buttons::FIRE));
        assert_eq!(touch_button("jump"), Some(Buttons::HANDBRAKE));
        assert_eq!(touch_button("nope"), None);
    }
}
