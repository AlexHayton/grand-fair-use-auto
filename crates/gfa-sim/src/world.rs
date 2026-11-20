//! The whole simulation state and its tick.

use crate::cars::{
    self, BIKE_ENTER_FIRST, BIKE_EXIT_FIRST, BIKE_FRAMES, BIKE_SIT, Driven, ENTER_SEQ, ENTER_TICKS,
    EXIT_FIRST, EXIT_FRAMES, EXIT_TICKS, PED_CAR_MARGIN, Parked, REACH, body_hit, door_point,
    door_pos, lerp_angle, lerp_fp,
};
use crate::hash::{SimHash, StateHasher};
use crate::input::{Buttons, TickInput};
use crate::map::{SimMap, ground};
use crate::player::{Anim, Player, Stick};
use crate::vehicle::{CLASS_BIKE, Pedals, Veh};
use crate::view::SimView;
use gfa_fixed::fx::{BLOCK_SHIFT, FP, block_centre};
use gfa_fixed::trig::{cos, sin};
use gfa_formats::style::CarInfo;

/// Where the player starts: a block, and optionally a heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Start {
    pub bx: i32,
    pub by: i32,
    /// 0..256, 0 north. `None` faces south, the way the art is drawn.
    pub angle: Option<i32>,
}

impl Start {
    /// From a `PLAYER` declaration: `(x, y)` in blocks and the second
    /// trailing number a 1024-step heading (AmiGTA1
    /// `gta_script_player_start`).
    pub fn from_decl(pos: [i32; 3], nums: &[i32]) -> Self {
        // AmiGTA1 stores the numbers as `short` and divides with C's
        // truncation, which Rust's `/` matches.
        let b = nums.get(1).map_or(0, |&b| i32::from(b as i16));
        Start {
            bx: pos[0],
            by: pos[1],
            angle: Some((b / 4) & 255),
        }
    }

    /// From a mission section's first `PLAYER` declaration.
    pub fn from_section(section: &gfa_formats::mission::Section) -> Option<Self> {
        let d = section.decls.iter().find(|d| d.kind == "PLAYER")?;
        Some(Start::from_decl(d.pos, &d.nums))
    }
}

/// A line for the run log, stamped with the tick it happened on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub tick: u32,
    pub text: String,
}

/// Walking to a car's door and getting in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entering {
    pub car: Parked,
    /// Where he walks to, and where he walked from (with his heading).
    pub door: (i64, i64),
    pub from: (i64, i64, i32),
    pub walk_len: i32,
    pub walk_t: i32,
    pub step: i32,
    pub tick: i32,
    pub bike: bool,
}

/// Getting out; the car keeps rolling, with nobody at the controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exiting {
    /// The car's heading when he opened the door.
    pub face: i32,
    /// Where he started (a bike's rider slides off from here).
    pub from: (i64, i64),
    pub door: (i64, i64),
    pub step: i32,
    pub tick: i32,
    pub bike: bool,
}

/// A car as the renderer draws it: 16.16 world pixels (z 32 to a layer),
/// heading 0..256, the deltas shown, and the fuse when it burns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShownCar {
    pub model: usize,
    pub remap: u8,
    pub pos: [i32; 3],
    pub face: i32,
    pub deltas: u32,
    pub fuse: i32,
}

/// What the player is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doing {
    OnFoot,
    Entering(Entering),
    Driving,
    Exiting(Exiting),
}

/// The simulation.
#[derive(Debug, Clone)]
pub struct World {
    /// Ticks completed.
    pub tick: u32,
    pub map: SimMap,
    /// The style's car table.
    pub cars: Vec<CarInfo>,
    pub view: SimView,
    pub player: Player,
    pub doing: Doing,
    /// The player's car, while he drives it or gets out of it.
    pub driven: Option<Driven>,
    /// Vehicles standing in the streets.
    pub parked: Vec<Parked>,
    /// Last tick's buttons, to find presses.
    prev: Buttons,
    log: Vec<LogLine>,
}

impl World {
    pub fn new(map: SimMap, cars: Vec<CarInfo>, view: SimView, start: Start) -> Self {
        let (mut player, found) = Player::place(&map, start.bx, start.by);
        if let Some(a) = start.angle {
            player.angle = a & 255;
        }
        let parked = cars::parked_from_map(&map, &cars);
        let mut world = World {
            tick: 0,
            map,
            cars,
            view,
            player,
            doing: Doing::OnFoot,
            driven: None,
            parked,
            prev: Buttons::NONE,
            log: Vec::new(),
        };
        let p = &world.player;
        let text = format!(
            "player: start ({},{}) layer {} angle {} ground {}{}",
            start.bx,
            start.by,
            p.layer,
            p.angle,
            ground_name(p.ground),
            if found { "" } else { " (no walkable layer)" }
        );
        world.say(text);
        world
    }

    /// Advance one tick.
    pub fn step(&mut self, input: TickInput) {
        let b = input.buttons;
        if b.pressed_since(self.prev).contains(Buttons::ENTER) {
            match self.doing {
                Doing::OnFoot => self.try_enter(),
                Doing::Driving => self.try_exit(),
                Doing::Entering(_) | Doing::Exiting(_) => {}
            }
        }
        match self.doing {
            Doing::OnFoot => self.walk(b),
            Doing::Entering(e) => self.entering(e),
            Doing::Exiting(e) => self.exiting(e),
            Doing::Driving => {}
        }
        if self.doing == Doing::Driving {
            let pedals = Pedals {
                throttle: b.contains(Buttons::UP),
                brake: b.contains(Buttons::DOWN),
                steer: i32::from(b.contains(Buttons::RIGHT)) - i32::from(b.contains(Buttons::LEFT)),
                handbrake: b.contains(Buttons::HANDBRAKE),
            };
            self.drive(pedals);
        }
        self.prev = b;
        self.tick += 1;
    }

    fn walk(&mut self, b: Buttons) {
        let axis = |plus: Buttons, minus: Buttons| {
            i32::from(b.contains(plus)) - i32::from(b.contains(minus))
        };
        let stick = Stick {
            turn: axis(Buttons::RIGHT, Buttons::LEFT),
            forward: axis(Buttons::UP, Buttons::DOWN),
            walk: b.contains(Buttons::WALK),
        };
        let before = self.player.layer;
        let (ox, oy) = (self.player.x, self.player.y);
        self.player.update(&self.map, stick);
        self.car_block(ox, oy);
        if self.player.layer != before {
            let (bx, by) = self.player.block();
            self.say(format!(
                "player: layer {before} -> {} at ({bx},{by})",
                self.player.layer
            ));
        }
    }

    /// Is a parked car's body at `(px, py)` on `layer`, grown by `margin`?
    fn car_at(&self, px: i64, py: i64, layer: i32, margin: i32) -> bool {
        self.parked.iter().any(|c| {
            let ci = &self.cars[c.model];
            c.layer == layer
                && body_hit(
                    c.x.into(),
                    c.y.into(),
                    c.face,
                    i32::from(ci.length >> 1) / 2,
                    i32::from(ci.width >> 1) / 2,
                    px,
                    py,
                    margin,
                )
        })
    }

    /// A walker who has stepped into a car goes back along whichever axis
    /// put him there (AmiGTA1 `ped_car_block`).
    fn car_block(&mut self, ox: i32, oy: i32) {
        let p = &self.player;
        let (nx, ny, z) = (i64::from(p.x), i64::from(p.y), p.layer);
        let (ox64, oy64) = (i64::from(ox), i64::from(oy));
        let m = PED_CAR_MARGIN;
        if !self.car_at(nx, ny, z, m) || self.car_at(ox64, oy64, z, m) {
            return;
        }
        if !self.car_at(nx, oy64, z, m) {
            self.player.y = oy;
        } else if !self.car_at(ox64, ny, z, m) {
            self.player.x = ox;
        } else {
            self.player.x = ox;
            self.player.y = oy;
        }
    }

    /// RETURN on foot: walk to the nearest car's door within reach.
    fn try_enter(&mut self) {
        let p = &self.player;
        let reach = i64::from(REACH) << 16;
        let mut best: Option<(usize, i64)> = None;
        for (i, c) in self.parked.iter().enumerate() {
            if c.layer != p.layer || c.wrecked || !cars::enterable(&self.cars[c.model]) {
                continue;
            }
            let d = (i64::from(c.x) - i64::from(p.x))
                .abs()
                .max((i64::from(c.y) - i64::from(p.y)).abs());
            if d < best.map_or(reach, |b| b.1) {
                best = Some((i, d));
            }
        }
        let Some((i, _)) = best else {
            self.say("car: no car within reach".into());
            return;
        };
        let car = self.parked.remove(i);
        let ci = &self.cars[car.model];
        let door = door_point(ci, car.x.into(), car.y.into(), car.face);
        let (px, py) = (i64::from(self.player.x), i64::from(self.player.y));
        let px_dist = ((door.0 - px).abs() + (door.1 - py).abs()) >> 16;
        let e = Entering {
            car,
            door,
            from: (px, py, self.player.angle),
            walk_len: ((px_dist * 3) / 4).clamp(0, 60) as i32,
            walk_t: 0,
            step: 0,
            tick: 0,
            bike: ci.vtype == CLASS_BIKE,
        };
        self.say(format!(
            "car: going for model {} at ({},{})",
            ci.model,
            car.x >> 21,
            car.y >> 21
        ));
        self.doing = Doing::Entering(e);
    }

    fn entering(&mut self, mut e: Entering) {
        let p = &mut self.player;
        if e.walk_t < e.walk_len {
            p.anim = Anim::Walk;
            p.frame = ((e.walk_t / 3) % 8) as u8;
            p.x = lerp_fp(e.from.0, e.door.0, e.walk_t, e.walk_len) as i32;
            p.y = lerp_fp(e.from.1, e.door.1, e.walk_t, e.walk_len) as i32;
            p.angle = lerp_angle(e.from.2, e.car.face, e.walk_t, e.walk_len);
            e.walk_t += 1;
            self.doing = Doing::Entering(e);
            return;
        }
        let steps = if e.bike {
            BIKE_FRAMES
        } else {
            ENTER_SEQ.len() as i32
        };
        let total = steps * ENTER_TICKS;
        let t = e.step * ENTER_TICKS + e.tick;
        // He stands at the door, then slides in over the last two steps.
        let slide = (steps - 2) * ENTER_TICKS;
        let (cx, cy) = (i64::from(e.car.x), i64::from(e.car.y));
        if t <= slide {
            p.x = e.door.0 as i32;
            p.y = e.door.1 as i32;
        } else {
            p.x = lerp_fp(e.door.0, cx, t - slide, total - slide) as i32;
            p.y = lerp_fp(e.door.1, cy, t - slide, total - slide) as i32;
        }
        p.angle = e.car.face;
        e.tick += 1;
        if e.tick >= ENTER_TICKS {
            e.tick = 0;
            e.step += 1;
        }
        if e.step < steps {
            self.doing = Doing::Entering(e);
            return;
        }
        let car = e.car;
        let mut veh = Veh::new(&self.cars[car.model], car.model, cx, cy, car.face);
        veh.remap = (car.remap > 0).then_some(car.remap);
        veh.damage = car.damage;
        veh.dmg_bits = car.dmg_bits;
        self.driven = Some(Driven {
            veh,
            layer: car.layer,
            air: None,
            // The door is still swinging shut as he sits down.
            door_tick: if e.bike { -1 } else { total },
        });
        self.player.anim = Anim::Stand;
        self.player.frame = 0;
        self.doing = Doing::Driving;
        self.say(format!(
            "car: in - model {} at ({},{})",
            self.cars[car.model].model,
            car.x >> 21,
            car.y >> 21
        ));
    }

    /// RETURN in a car: open the door, if it is slow enough and the door is
    /// clear.
    fn try_exit(&mut self) {
        let Some(d) = &self.driven else {
            return;
        };
        let v = &d.veh;
        if cars::too_fast_to_exit(v) {
            self.say("car: too fast to get out".into());
            return;
        }
        let ci = &self.cars[v.model];
        let a = v.angle();
        let outside = door_pos(ci, v.ox, v.oy, a, 0, 2);
        let blocked = if self.car_at(outside.0, outside.1, d.layer, 2) {
            Some("a car")
        } else if self
            .map
            .nav_ground((outside.0 >> 21) as i32, (outside.1 >> 21) as i32, d.layer)
            == ground::BUILDING
        {
            Some("a building")
        } else {
            None
        };
        if let Some(what) = blocked {
            // AmiGTA1 climbs out over the roof; not built yet (M4).
            self.say(format!("car: door blocked by {what}"));
            return;
        }
        let door = door_point(ci, v.ox, v.oy, a);
        self.doing = Doing::Exiting(Exiting {
            face: a,
            from: (v.ox, v.oy),
            door,
            step: 0,
            tick: 0,
            bike: ci.vtype == CLASS_BIKE,
        });
    }

    fn exiting(&mut self, mut e: Exiting) {
        let Some(d) = &self.driven else {
            self.doing = Doing::OnFoot;
            return;
        };
        let v = &d.veh;
        let ci = &self.cars[v.model];
        let steps = if e.bike { BIKE_FRAMES } else { EXIT_FRAMES };
        let total = steps * EXIT_TICKS;
        let t = e.step * EXIT_TICKS + e.tick;
        let (x, y) = if e.bike {
            (
                lerp_fp(e.from.0, e.door.0, t, total),
                lerp_fp(e.from.1, e.door.1, t, total),
            )
        } else {
            let (along, lat) = cars::exit_offsets(e.step);
            door_pos(ci, v.ox, v.oy, e.face, along, lat)
        };
        self.player.x = x as i32;
        self.player.y = y as i32;
        self.player.angle = e.face;
        self.player.layer = d.layer;
        e.tick += 1;
        if e.tick >= EXIT_TICKS {
            e.tick = 0;
            e.step += 1;
        }
        if e.step < steps {
            self.doing = Doing::Exiting(e);
            // The car rolls on with nobody at the wheel.
            self.drive(Pedals::default());
            return;
        }
        // Out: beside the door, turned away from the car.
        let side = if ci.doors.is_empty() { 1 } else { -1 };
        if !e.bike {
            let (x, y) = door_pos(ci, v.ox, v.oy, e.face, -2, 1);
            self.player.x = x as i32;
            self.player.y = y as i32;
        }
        self.player.angle = (e.face + side * if e.bike { 64 } else { 32 }) & 255;
        let car = Parked {
            model: v.model,
            x: v.ox as i32,
            y: v.oy as i32,
            face: e.face,
            layer: d.layer,
            remap: v.remap.unwrap_or(0),
            damage: v.damage,
            dmg_bits: v.dmg_bits,
            wrecked: false,
        };
        self.parked.push(car);
        self.driven = None;
        self.player.anim = Anim::Stand;
        self.player.frame = 0;
        let (bx, by) = self.player.block();
        self.player.ground = self.map.ground_type(bx, by, self.player.layer);
        self.doing = Doing::OnFoot;
        self.say(format!(
            "car: out at ({},{}) facing {}",
            self.player.x >> 16,
            self.player.y >> 16,
            self.player.angle
        ));
    }

    fn drive(&mut self, pedals: Pedals) {
        let Some(d) = self.driven.as_mut() else {
            return;
        };
        if d.door_tick >= 0 {
            d.door_tick += 1;
            if d.door_tick >= cars::DOOR_TICKS {
                d.door_tick = -1;
            }
        }
        let mut lines = Vec::new();
        d.tick(&self.map, &self.cars[d.veh.model], pedals, &mut |s| {
            lines.push(s)
        });
        if self.doing == Doing::Driving {
            self.player.x = d.veh.ox as i32;
            self.player.y = d.veh.oy as i32;
            self.player.layer = d.layer;
            self.player.angle = d.veh.angle();
        }
        for l in lines {
            self.say(l);
        }
        self.burn();
    }

    /// A write-off burns for [`cars::FUSE`] ticks, then blows up: the
    /// player is thrown clear to its right and it stays as a wreck
    /// (AmiGTA1's main loop; the blast's harm to others is M6).
    fn burn(&mut self) {
        let Some(d) = self.driven.as_mut() else {
            return;
        };
        let v = &mut d.veh;
        if v.damage < cars::WRECKED {
            return;
        }
        if v.fuse == 0 {
            v.fuse = cars::FUSE;
            v.dmg_bits |= cars::DAMAGE_MASK;
            self.say("car: a write-off - get out".into());
            return;
        }
        v.fuse -= 1;
        if v.fuse > 0 {
            return;
        }
        let (x, y, face, layer) = (v.ox, v.oy, v.angle(), d.layer);
        let wreck = Parked {
            model: v.model,
            x: x as i32,
            y: y as i32,
            face,
            layer,
            remap: v.remap.unwrap_or(0),
            damage: v.damage,
            dmg_bits: v.dmg_bits,
            wrecked: true,
        };
        self.parked.push(wreck);
        self.driven = None;
        // A Q14 unit times 48 is 12 px.
        self.player.x = (x + i64::from(cos(face)) * 48) as i32;
        self.player.y = (y + i64::from(sin(face)) * 48) as i32;
        self.player.angle = (face + 64) & 255;
        self.player.layer = layer;
        self.player.anim = Anim::Stand;
        self.player.frame = 0;
        self.doing = Doing::OnFoot;
        self.say(format!("car: blew up at ({},{})", x >> 16, y >> 16));
    }

    /// The cars standing still this tick: the parked ones, and the one the
    /// player is getting into, with its door.
    pub fn standing_cars(&self) -> Vec<ShownCar> {
        let shown = |c: &Parked, extra: u32| ShownCar {
            model: c.model,
            remap: c.remap,
            pos: [c.x, c.y, self.map.ground_z(c.x, c.y, c.layer)],
            face: c.face,
            deltas: c.dmg_bits | extra,
            fuse: 0,
        };
        let mut out: Vec<ShownCar> = self.parked.iter().map(|c| shown(c, 0)).collect();
        if let Doing::Entering(e) = self.doing {
            let t = e.step * ENTER_TICKS + e.tick;
            let door = if e.bike || e.walk_t < e.walk_len {
                None
            } else {
                cars::door_delta(t)
            };
            out.push(shown(&e.car, door.map_or(0, |d| 1 << d)));
        }
        out
    }

    /// The driven car's deltas (dents and an open door) and fuse.
    pub fn driven_look(&self) -> Option<(u32, i32)> {
        let d = self.driven.as_ref()?;
        let mut door = cars::door_delta(d.door_tick);
        if let Doing::Exiting(e) = self.doing {
            door = if e.bike {
                None
            } else {
                cars::exit_door_delta(e.step)
            };
        }
        Some((d.veh.dmg_bits | door.map_or(0, |d| 1 << d), d.veh.fuse))
    }

    /// The ped sprite frame to draw the player with, or `None` while he is
    /// hidden inside a car.
    pub fn player_frame(&self) -> Option<u16> {
        match self.doing {
            Doing::OnFoot => Some(self.player.ped_frame()),
            Doing::Entering(e) if e.walk_t < e.walk_len => Some(self.player.ped_frame()),
            Doing::Entering(e) if e.bike => {
                Some(BIKE_ENTER_FIRST + e.step.clamp(0, BIKE_FRAMES - 1) as u16)
            }
            Doing::Entering(e) => {
                Some(ENTER_SEQ[e.step.clamp(0, ENTER_SEQ.len() as i32 - 1) as usize])
            }
            // A rider is seen on his bike, a driver in a convertible.
            Doing::Driving => {
                let ci = &self.cars[self.driven.as_ref()?.veh.model];
                if ci.vtype == CLASS_BIKE {
                    Some(BIKE_SIT)
                } else if ci.convertible & 1 != 0 {
                    Some(cars::SIT_IN_CAR)
                } else {
                    None
                }
            }
            Doing::Exiting(e) if e.bike => {
                Some(BIKE_EXIT_FIRST + e.step.clamp(0, BIKE_FRAMES - 1) as u16)
            }
            Doing::Exiting(e) => Some(EXIT_FIRST + e.step.clamp(0, EXIT_FRAMES - 1) as u16),
        }
    }

    /// A test fixture: put the player on block `(bx, by)`, on its lowest
    /// walkable layer, optionally turned to `face`. In a car, the car goes
    /// with him (AmiGTA1's `goto` order).
    pub fn teleport(&mut self, bx: i32, by: i32, face: Option<i32>) {
        if let Some(d) = self.driven.as_mut() {
            let (x, y) = (i64::from(block_centre(bx)), i64::from(block_centre(by)));
            let ci = &self.cars[d.veh.model];
            let angle = face.unwrap_or(d.veh.angle());
            let (remap, damage) = (d.veh.remap, d.veh.damage);
            d.veh = Veh::new(ci, d.veh.model, x, y, angle);
            d.veh.remap = remap;
            d.veh.damage = damage;
            d.air = None;
            d.layer = self.map.stand_layer(bx, by).unwrap_or(d.layer);
            let layer = d.layer;
            self.player.x = x as i32;
            self.player.y = y as i32;
            self.player.layer = layer;
            self.say(format!("player: goto ({bx},{by}) layer {layer} in the car"));
            return;
        }
        let angle = face.unwrap_or(self.player.angle);
        let (mut player, found) = Player::place(&self.map, bx, by);
        player.angle = angle & 255;
        self.player = player;
        self.doing = Doing::OnFoot;
        let layer = self.player.layer;
        self.say(format!(
            "player: goto ({bx},{by}) layer {layer}{}",
            if found { "" } else { " (no walkable layer)" }
        ));
    }

    /// A test fixture: the player straight into a new car of table record
    /// `model`, centred on block `(bx, by)` facing `face`, on the column's
    /// lowest standable layer (the street layer if it has none).
    pub fn put_in_car(&mut self, model: usize, bx: i32, by: i32, face: i32) {
        let Some(ci) = self.cars.get(model) else {
            self.say(format!("car: drive - no model {model}"));
            return;
        };
        let (x, y) = (block_centre(bx), block_centre(by));
        let layer = self
            .map
            .stand_layer(bx, by)
            .unwrap_or(crate::player::STREET_LAYER);
        self.driven = Some(Driven {
            veh: Veh::new(ci, model, x.into(), y.into(), face),
            layer,
            air: None,
            door_tick: -1,
        });
        self.player.x = x;
        self.player.y = y;
        self.player.layer = layer;
        self.player.angle = face & 255;
        self.doing = Doing::Driving;
        self.say(format!(
            "car: in - model {} at ({bx},{by}) layer {layer}",
            ci.model
        ));
    }

    /// A test fixture: `points` of damage on the player's car (AmiGTA1's
    /// `damage` order).
    pub fn damage_car(&mut self, points: i32) {
        match self.driven.as_mut() {
            Some(d) => {
                d.veh.damage += points;
                let total = d.veh.damage;
                self.say(format!("car: damage {total}"));
            }
            None => self.say("car: damage - not in a car".into()),
        }
    }

    /// A test fixture: park car table record `model` `(dx, dy)` world px
    /// from the player, facing `face` (AmiGTA1's `park` order).
    pub fn park(&mut self, model: usize, dx: i32, dy: i32, face: i32) {
        if model >= self.cars.len() {
            self.say(format!("car: park - no model {model}"));
            return;
        }
        let car = Parked {
            model,
            x: self.player.x + (dx << 16),
            y: self.player.y + (dy << 16),
            face: face & 255,
            layer: self.player.layer,
            remap: 0,
            damage: 0,
            dmg_bits: 0,
            wrecked: false,
        };
        self.say(format!(
            "car: parked model {} at ({},{}) facing {}",
            self.cars[model].model,
            car.x >> 16,
            car.y >> 16,
            car.face
        ));
        self.parked.push(car);
    }

    /// Add a line to the run log.
    pub fn say(&mut self, text: String) {
        self.log.push(LogLine {
            tick: self.tick,
            text,
        });
    }

    /// Take the log lines written since the last call.
    pub fn drain_log(&mut self) -> Vec<LogLine> {
        std::mem::take(&mut self.log)
    }

    /// A hash of everything that decides future ticks.
    pub fn hash(&self) -> u64 {
        let mut h = StateHasher::default();
        h.u32(self.tick);
        self.player.sim_hash(&mut h);
        h.bytes(&self.prev.0.to_le_bytes());
        match self.doing {
            Doing::OnFoot => h.u8(0),
            Doing::Driving => h.u8(2),
            Doing::Entering(e) => {
                h.u8(1);
                e.car.sim_hash(&mut h);
                for v in [e.door.0, e.door.1, e.from.0, e.from.1] {
                    h.bytes(&v.to_le_bytes());
                }
                for v in [e.from.2, e.walk_len, e.walk_t, e.step, e.tick] {
                    h.i32(v);
                }
            }
            Doing::Exiting(e) => {
                h.u8(3);
                for v in [e.from.0, e.from.1, e.door.0, e.door.1] {
                    h.bytes(&v.to_le_bytes());
                }
                for v in [e.face, e.step, e.tick] {
                    h.i32(v);
                }
            }
        }
        match &self.driven {
            Some(d) => d.sim_hash(&mut h),
            None => h.u8(0),
        }
        h.u32(self.parked.len() as u32);
        for c in &self.parked {
            c.sim_hash(&mut h);
        }
        h.finish()
    }

    /// The player's position for the renderer: 16.16 world pixels, with
    /// the height of the ground under him (32 px to a layer).
    pub fn player_pos(&self) -> [i32; 3] {
        let p = &self.player;
        [p.x, p.y, self.map.ground_z(p.x, p.y, p.layer)]
    }

    /// The gtadump-style one-line summary of the player, or of his car.
    pub fn player_summary(&self) -> String {
        let p = &self.player;
        let part = |v: i32| {
            let px = (v >> FP) & ((1 << BLOCK_SHIFT) - 1);
            let hundredths = ((v & 0xFFFF) * 100) >> FP;
            format!("{px:2}.{hundredths:02}")
        };
        let (bx, by) = p.block();
        let mut s = format!(
            "block ({bx:3},{by:3}) +{},{}  layer {}  angle {:3}  ground {:<8}{}{}",
            part(p.x),
            part(p.y),
            p.layer,
            p.angle,
            ground_name(p.ground),
            if p.blocked_x { "  BLOCKED-X" } else { "" },
            if p.blocked_y { "  BLOCKED-Y" } else { "" },
        );
        if let (Doing::Driving, Some(d)) = (self.doing, &self.driven) {
            let v = &d.veh;
            s = format!(
                "block ({bx:3},{by:3}) +{},{}  layer {}  angle {:3}  speed {}  damage {}",
                part(p.x),
                part(p.y),
                d.layer,
                v.angle(),
                part_wide(v.speed()),
                v.damage
            );
        }
        s
    }
}

/// A 16.16 value as `whole.hundredths`.
fn part_wide(v: i64) -> String {
    format!("{}.{:02}", v >> FP, ((v & 0xFFFF) * 100) >> FP)
}

/// The centre of a block as a 16.16 point.
pub fn block_point(bx: i32, by: i32) -> (i32, i32) {
    (block_centre(bx), block_centre(by))
}

/// A ground type's name, as AmiGTA1's tools print it.
pub fn ground_name(g: u8) -> &'static str {
    const NAMES: [&str; 8] = [
        "air", "water", "road", "pavement", "field", "building", "?6", "?7",
    ];
    NAMES[usize::from(g & 7)]
}
