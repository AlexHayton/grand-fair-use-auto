//! The player on foot.
//!
//! Ported from AmiGTA1 `gta_player.c` (MIT). Collision is read off the
//! map's ground types ([`SimMap::walkable`]), not inferred from walls:
//! every corner of a small square body must be in a walkable block on the
//! player's layer, and fences ([`SimMap::wall_cross`]) stop him at their
//! edge. A blocked move tries each axis alone, so he slides along walls.
//!
//! Layers change on ramps only: walking up a ramp off its top lands one
//! layer higher, stepping onto a ramp from its high end one lower. Kerbs
//! and steps fall back to the nearest walkable layer. AmiGTA1 settled this
//! on NYC's bridge at (19,44), which has road on layers 2 and 3.
//!
//! Speeds, turn rate and animation timing are AmiGTA1's, per 50 Hz tick.
//! GTA has no walk key: the player always runs, and walking is an extra.

use crate::hash::{SimHash, StateHasher};
use crate::map::{LAYERS, SimMap};
use gfa_fixed::fx::{BLOCK_SHIFT, FP, block_centre};
use gfa_fixed::trig::{cos, sin, step_dir};

/// Walking pace: 0.6 world px a tick (0.94 blocks a second).
pub const WALK_SPEED: i32 = 39321;
/// Running pace: 1.3 world px a tick (2.03 blocks a second).
pub const RUN_SPEED: i32 = 85197;
/// Angle steps per tick: a full turn in just over a second.
pub const TURN_RATE: i32 = 5;
/// Half-width of the square collision body: 3 world px. Square, so the
/// collision does not depend on which way he faces.
pub const BODY_R: i32 = 3 << FP;
/// Ticks per walking frame.
pub const WALK_TICKS: u8 = 7;
/// Ticks per running frame.
pub const RUN_TICKS: u8 = 4;
/// Frames in the walk and run cycles.
pub const CYCLE_FRAMES: u8 = 8;
/// The street layer: where a column with nothing walkable puts him.
pub const STREET_LAYER: i32 = 2;
/// The heading at which the ped art is unrotated: south.
pub const ART_FACES: i32 = 128;

/// Ped sprite frames, within the style's ped sprite category, from
/// Carnage3D's `ped_animations.json` (MIT).
pub mod frames {
    pub const WALK_FIRST: u16 = 0;
    pub const RUN_FIRST: u16 = 8;
    pub const STAND: u16 = 98;
    /// Going over a car's roof: one frame a vault state (AmiGTA1
    /// `GTA_PED_VAULT_FIRST`; the last two never play).
    pub const VAULT_FIRST: u16 = 91;
    pub const VAULT_FRAMES: u16 = 6;
    /// Lying on his back under a vehicle too tall to vault.
    pub const SLIDE_UNDER: u16 = 41;
}

/// What the player is doing, for the sprite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Anim {
    Stand,
    Walk,
    Run,
    /// Over a car's roof; `frame` is the vault state.
    Vault,
    /// Sliding under a tall vehicle.
    SlideUnder,
}

/// The player's controls for one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stick {
    /// -1 left, 0 straight, +1 right.
    pub turn: i32,
    /// -1 back, 0 still, +1 forward.
    pub forward: i32,
    /// Walk instead of run.
    pub walk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Player {
    /// 16.16 world pixels, 32 to a block.
    pub x: i32,
    pub y: i32,
    /// The map layer he occupies.
    pub layer: i32,
    /// 0..256, 0 north, clockwise.
    pub angle: i32,
    pub anim: Anim,
    pub frame: u8,
    pub frame_tick: u8,
    /// Diagnostics from the last update.
    pub blocked_x: bool,
    pub blocked_y: bool,
    /// Ground type where he stands.
    pub ground: u8,
}

impl Player {
    /// Stand in the middle of block `(bx, by)`, on the lowest walkable layer
    /// of that column, facing south. The flag is false when the column has
    /// no walkable layer (he is put on the street layer anyway).
    pub fn place(map: &SimMap, bx: i32, by: i32) -> (Self, bool) {
        let found = (0..LAYERS).find(|&z| map.walkable(bx, by, z));
        let layer = found.unwrap_or(STREET_LAYER);
        let p = Player {
            x: block_centre(bx),
            y: block_centre(by),
            layer,
            angle: ART_FACES,
            anim: Anim::Stand,
            frame: 0,
            frame_tick: 0,
            blocked_x: false,
            blocked_y: false,
            ground: map.ground_type(bx, by, layer),
        };
        (p, found.is_some())
    }

    /// The block he is in.
    pub fn block(&self) -> (i32, i32) {
        (self.x >> (FP + BLOCK_SHIFT), self.y >> (FP + BLOCK_SHIFT))
    }

    /// One tick.
    pub fn update(&mut self, map: &SimMap, stick: Stick) {
        self.blocked_x = false;
        self.blocked_y = false;

        if stick.turn != 0 {
            self.angle = (self.angle + stick.turn * TURN_RATE) & 255;
        }

        if stick.forward == 0 {
            self.anim = Anim::Stand;
            self.frame = 0;
            self.frame_tick = 0;
            self.ground = self.ground_here(map);
            return;
        }

        let mut speed = if stick.walk { WALK_SPEED } else { RUN_SPEED };
        if stick.forward < 0 {
            // Backing up is half pace.
            speed = -(speed / 2);
        }
        // Speed is shifted down before the multiply to leave headroom, as
        // AmiGTA1 does. Note the y term negates before shifting, so it
        // rounds differently from the x term: kept for bit-exactness.
        let dx = (sin(self.angle) * (speed >> 4)) >> 10;
        let dy = (-(cos(self.angle) * (speed >> 4))) >> 10;

        let (nx, ny) = (self.x + dx, self.y + dy);
        let (fx, fy) = self.block();
        let z = resolve_layer_moving(map, fx, fy, self.layer, blk(nx), blk(ny), dx, dy);
        // Only the whole move knows its direction, so only it climbs ramps;
        // the axis slides below use the plain rule.
        let wall = |p: &Player, x1: i32, y1: i32| map.wall_cross(p.layer, p.x, p.y, x1, y1);
        if let Some(z) = z
            && can_stand(map, nx, ny, z)
            && !wall(self, nx, ny)
        {
            self.x = nx;
            self.y = ny;
            self.layer = z;
        } else {
            let z = resolve_layer(map, blk(nx), blk(self.y), self.layer);
            match z {
                Some(z) if can_stand(map, nx, self.y, z) && !wall(self, nx, self.y) => {
                    self.x = nx;
                    self.layer = z;
                }
                _ => self.blocked_x = true,
            }
            // After the x slide: from the new x and layer.
            let z = resolve_layer(map, blk(self.x), blk(ny), self.layer);
            match z {
                Some(z) if can_stand(map, self.x, ny, z) && !wall(self, self.x, ny) => {
                    self.y = ny;
                    self.layer = z;
                }
                _ => self.blocked_y = true,
            }
            // Blocked on both axes he still animates: a runner frozen
            // against a wall reads as a crash.
        }

        self.anim = if stick.walk { Anim::Walk } else { Anim::Run };
        self.frame_tick += 1;
        if self.frame_tick >= if stick.walk { WALK_TICKS } else { RUN_TICKS } {
            self.frame_tick = 0;
            self.frame += 1;
            if self.frame >= CYCLE_FRAMES {
                self.frame = 0;
            }
        }
        self.ground = self.ground_here(map);
    }

    /// The ped sprite to draw, as a frame within the ped sprite category.
    pub fn ped_frame(&self) -> u16 {
        match self.anim {
            Anim::Stand => frames::STAND,
            Anim::Walk => frames::WALK_FIRST + u16::from(self.frame),
            Anim::Run => frames::RUN_FIRST + u16::from(self.frame),
            Anim::Vault => {
                frames::VAULT_FIRST + u16::from(self.frame).min(frames::VAULT_FRAMES - 1)
            }
            Anim::SlideUnder => frames::SLIDE_UNDER,
        }
    }

    /// The angle to rotate the sprite by: the art faces south.
    pub fn draw_angle(&self) -> i32 {
        (self.angle + ART_FACES) & 255
    }

    fn ground_here(&self, map: &SimMap) -> u8 {
        let (bx, by) = self.block();
        map.ground_type(bx, by, self.layer)
    }
}

impl SimHash for Player {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.i32(self.x);
        h.i32(self.y);
        h.i32(self.layer);
        h.i32(self.angle);
        h.u8(self.anim as u8);
        h.u8(self.frame);
        h.u8(self.frame_tick);
        h.bool(self.blocked_x);
        h.bool(self.blocked_y);
        h.u8(self.ground);
    }
}

#[inline]
fn blk(v: i32) -> i32 {
    v >> (FP + BLOCK_SHIFT)
}

/// The layer he lands on moving from block `(fx, fy)` on layer `fz` into
/// block `(bx, by)` along `(dx, dy)`.
#[allow(clippy::too_many_arguments)]
fn resolve_layer_moving(
    map: &SimMap,
    fx: i32,
    fy: i32,
    fz: i32,
    bx: i32,
    by: i32,
    dx: i32,
    dy: i32,
) -> Option<i32> {
    let dir = step_dir(dx, dy);
    // On a ramp heading up it: the block ahead is a layer higher. Checked
    // before preferring the current layer, which would send him under a
    // bridge he has just climbed.
    if map.slope_up_dir(fx, fy, fz) == Some(dir) && map.walkable(bx, by, fz + 1) {
        return Some(fz + 1);
    }
    // Onto a ramp from its high end: one layer down.
    if map
        .slope_up_dir(bx, by, fz - 1)
        .is_some_and(|up| (up + 128) & 255 == dir)
        && map.walkable(bx, by, fz - 1)
    {
        return Some(fz - 1);
    }
    resolve_layer(map, bx, by, fz)
}

/// The layer to stand on in column `(bx, by)`: `want`, else one below,
/// else one above.
fn resolve_layer(map: &SimMap, bx: i32, by: i32, want: i32) -> Option<i32> {
    [want, want - 1, want + 1]
        .into_iter()
        .find(|&z| map.walkable(bx, by, z))
}

/// Can the body stand at `(x, y)` on layer `z`? Every corner must be on a
/// walkable block of that layer, or over the top step of a ramp on the
/// layer below (its surface reaches this layer).
fn can_stand(map: &SimMap, x: i32, y: i32, z: i32) -> bool {
    const CORNERS: [(i32, i32); 4] = [
        (-BODY_R, -BODY_R),
        (BODY_R, -BODY_R),
        (-BODY_R, BODY_R),
        (BODY_R, BODY_R),
    ];
    CORNERS.iter().all(|&(dx, dy)| {
        let (bx, by) = (blk(x + dx), blk(y + dy));
        map.walkable(bx, by, z) || (map.slope_is_top(bx, by, z - 1) && map.walkable(bx, by, z - 1))
    })
}
