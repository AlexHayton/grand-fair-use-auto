//! The whole simulation state and its tick.

use crate::cars::{
    self, BIKE_ENTER_FIRST, BIKE_EXIT_FIRST, BIKE_FRAMES, BIKE_SIT, DOOR_OPEN, Driven, ENTER_SEQ,
    ENTER_TICKS,
    EXIT_FIRST, EXIT_FRAMES, EXIT_TICKS, PED_CAR_MARGIN, Parked, REACH, body_hit, door_point,
    door_pos, lerp_angle, lerp_fp, panel_delta,
};
use crate::hash::{SimHash, StateHasher};
use crate::input::{Buttons, TickInput};
use crate::map::{SimMap, ground};
use crate::peds::{self, Peds, TickCtx};
use crate::player::{Anim, Player, RUN_SPEED, Stick};
use crate::traffic::{self, Traffic};
use crate::trains::{Carriage, TrainEvent, Trains};
use crate::vehicle::{CLASS_BIKE, Pedals, Veh};
use crate::view::SimView;
use gfa_fixed::fx::{BLOCK_SHIFT, FP, block_centre};
use gfa_fixed::trig::{cos, sin};
use gfa_formats::style::CarInfo;

/// Health at the start and after a respawn.
pub const FULL_HEALTH: i32 = 100;

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
    /// Coming from the far side: over the roof first.
    pub over: Option<Over>,
    /// The car's fleet serial, whether a mission had marked it, and
    /// whether somebody was at the wheel.
    pub serial: u64,
    pub mission: bool,
    pub driver: bool,
}

/// Going over a car's roof at running pace along a fixed heading. Every
/// four ticks is a state, and at each state's end he looks a pixel ahead:
/// still car, and he stays up (the frame advances to the fourth and holds);
/// clear, and he lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vault {
    pub head: i32,
    pub step: i32,
    pub tick: i32,
    /// States spent with car still ahead, capped by [`cars::VAULT_HOLD_MAX`].
    pub hold: i32,
}

impl Vault {
    pub fn new(head: i32) -> Self {
        Vault {
            head,
            step: 0,
            tick: 0,
            hold: 0,
        }
    }
}

impl SimHash for Vault {
    fn sim_hash(&self, h: &mut StateHasher) {
        for v in [self.head, self.step, self.tick, self.hold] {
            h.i32(v);
        }
    }
}

/// Getting in from the far side: he runs at the near flank (that is
/// `Entering::door` meanwhile), goes over the roof, then walks to the door.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Over {
    /// The real door.
    pub door: (i64, i64),
    /// Straight across the car.
    pub head: i32,
    /// Set once he is on the roof.
    pub vault: Option<Vault>,
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
    /// Aboard train `n`.
    Riding(usize),
    /// Over a car's roof on foot: SPACE while running at a low car, or
    /// thrown out over the roof when the door is blocked.
    Vaulting(Vault),
    /// Under a tall vehicle, a pixel along every state until clear; the
    /// count is the tick within the state.
    Sliding(i32),
    /// Gone under: there is no swimming. The count is AmiGTA1's
    /// `player_drown`, ticks since he went in plus one.
    Drowning(u32),
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
    pub trains: Trains,
    /// Every car but his: the traffic, and the cars standing in the
    /// street (the map's parked ones, the ones he left, wrecks).
    pub traffic: Traffic,
    /// Everybody on foot but him.
    pub peds: Peds,
    /// AmiGTA1's `player_health`: 100 full, wasted at 0.
    pub health: i32,
    /// Where the level put him, for the stopgap respawn.
    start: Start,
    /// Last tick's buttons, to find presses.
    prev: Buttons,
    log: Vec<LogLine>,
}

impl World {
    pub fn new(map: SimMap, cars: Vec<CarInfo>, view: SimView, start: Start) -> Self {
        World::with_map_cars(map, cars, view, start, true)
    }

    /// As [`World::new`]; `map_cars` false leaves the map's parked cars
    /// out, as AmiGTA1 does (its harnesses compare without them).
    pub fn with_map_cars(
        map: SimMap,
        cars: Vec<CarInfo>,
        view: SimView,
        start: Start,
        map_cars: bool,
    ) -> Self {
        let (mut player, found) = Player::place(&map, start.bx, start.by);
        if let Some(a) = start.angle {
            player.angle = a & 255;
        }
        let trains = Trains::new(&map);
        let mut traffic = Traffic::new(&map, u64::from(gfa_fixed::rng::TRAFFIC_SEED));
        traffic.set_view_blocks(view.half_w);
        // The map's parked cars stand in the fleet for the whole level,
        // placed first so the traffic starts round them.
        if map_cars {
            for car in cars::parked_from_map(&map, &cars) {
                traffic.place_resident(&cars, &car);
            }
        }
        // The traffic starts parked around the level start, as in AmiGTA1.
        let cap = traffic.fleet_cap;
        traffic.park(&map, &cars, start.bx, start.by, 8, cap);
        let mut world = World {
            tick: 0,
            map,
            cars,
            view,
            player,
            doing: Doing::OnFoot,
            driven: None,
            trains,
            traffic,
            peds: Peds::new(peds::SEED, Peds::cap_for(view)),
            health: FULL_HEALTH,
            start,
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
        let trains = world.trains.describe();
        world.say(trains);
        for i in 0..world.trains.trains.len() {
            let c = world.trains.carriage(i, 0);
            let kind = if world.trains.trains[i].tram {
                "tram"
            } else {
                "train"
            };
            world.say(format!(
                "trains: {kind} {i} at ({},{}) layer {}",
                c.body.0 >> 21,
                c.body.1 >> 21,
                c.layer
            ));
        }
        world
    }

    /// Advance one tick.
    pub fn step(&mut self, input: TickInput) {
        let b = input.buttons;
        let pressed = b.pressed_since(self.prev);
        if pressed.contains(Buttons::ENTER) {
            match self.doing {
                Doing::OnFoot => {
                    if !self.try_board() {
                        self.try_enter();
                    }
                }
                Doing::Driving => self.try_exit(),
                Doing::Riding(t) => self.try_get_off(t),
                // A press mid-animation is dropped, not queued.
                Doing::Entering(_)
                | Doing::Exiting(_)
                | Doing::Vaulting(_)
                | Doing::Sliding(_)
                | Doing::Drowning(_) => {}
            }
        }
        // On foot, the handbrake key is jump.
        if pressed.contains(Buttons::HANDBRAKE) && self.doing == Doing::OnFoot {
            self.jump();
        }
        match self.doing {
            Doing::OnFoot => self.walk(b),
            Doing::Entering(e) => self.entering(e),
            Doing::Exiting(e) => self.exiting(e),
            Doing::Riding(t) => self.ride(t, b, pressed.contains(Buttons::FIRE)),
            Doing::Vaulting(v) => self.vaulting(v),
            Doing::Sliding(t) => self.sliding(t),
            // In the water he does nothing but drown.
            Doing::Driving | Doing::Drowning(_) => {}
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
        let centre = self.run_traffic();
        self.run_trains();
        self.peds_view();
        self.burn();
        self.run_peds(centre);
        let mut lines = Vec::new();
        self.traffic.wreck_sweep(&mut |s| lines.push(s));
        for l in lines {
            self.say(l);
        }
        self.harm();
        self.run_over();
        self.prev = b;
        self.tick += 1;
    }

    /// RETURN on foot beside a stopped train: get on (within 24 px of a
    /// carriage, as AmiGTA1 in play).
    fn try_board(&mut self) -> bool {
        let p = &self.player;
        let Some(t) = self.trains.board(p.x.into(), p.y.into(), p.layer, 24) else {
            return false;
        };
        self.trains.rider = Some(t);
        self.doing = Doing::Riding(t);
        self.say(format!("train: boards train {t}"));
        true
    }

    fn try_get_off(&mut self, t: usize) {
        match self.trains.get_off(&self.map, t) {
            Some((x, y, layer)) => {
                if self.trains.trains[t].driven {
                    self.trains.hijack(t);
                }
                self.trains.rider = None;
                self.player.x = x as i32;
                self.player.y = y as i32;
                self.player.layer = layer;
                self.doing = Doing::OnFoot;
                self.say(format!("train: off train {t} at ({},{})", x >> 21, y >> 21));
            }
            None => self.say(format!("train: cannot get off train {t} here")),
        }
    }

    /// Carried by the train; FIRE takes or hands back the controls, and up
    /// and down drive it.
    fn ride(&mut self, t: usize, b: Buttons, fire: bool) {
        if fire {
            let took = self.trains.hijack(t);
            self.say(format!(
                "train: {} train {t}",
                if took { "hijacked" } else { "handed back" }
            ));
        }
        self.trains
            .drive(t, b.contains(Buttons::UP), b.contains(Buttons::DOWN));
        let (x, y, layer) = self.trains.rider_spot(t);
        self.player.x = x as i32;
        self.player.y = y as i32;
        self.player.layer = layer;
        if self.trains.trains[t].wrecked {
            self.trains.rider = None;
            self.doing = Doing::OnFoot;
        }
    }

    /// His car, while he drives it or gets out of it: what the fleet and
    /// the peds see instead of him.
    fn in_car(&self) -> Option<&Driven> {
        self.driven
            .as_ref()
            .filter(|_| self.doing == Doing::Driving || matches!(self.doing, Doing::Exiting(_)))
    }

    /// The fleet's tick, with the view on the player (or his car). The
    /// player and the peds are obstacles the fleet stops for. Returns the
    /// view's centre.
    fn run_traffic(&mut self) -> (i64, i64) {
        let p = &self.player;
        let (x, y, speed, hl) = match self.in_car() {
            Some(d) => {
                let v = &d.veh;
                (v.ox, v.oy, v.vx.abs().max(v.vy.abs()), v.len / 2)
            }
            None => {
                let moving = matches!(p.anim, Anim::Run | Anim::Walk);
                let speed = if moving { i64::from(RUN_SPEED) } else { 0 };
                (i64::from(p.x), i64::from(p.y), speed, 3)
            }
        };
        let layer = p.layer;
        // The police on foot look for him at his car's body (M6).
        let car = self.in_car().map(|d| {
            let v = &d.veh;
            (v.len / 2, v.wid / 2, v.angle())
        });
        self.peds.set_player(x, y, layer, car, (x, y), false);
        self.traffic.set_player(true, x, y, speed, layer, hl);
        self.traffic.set_walkers(self.peds.walkers());
        self.traffic.set_view_blocks(self.view.half_w);
        self.traffic.step(&self.map, &self.cars, x, y);
        (x, y)
    }

    /// The spawner's view: its size, and which way he is heading.
    fn peds_view(&mut self) {
        let (angle, moving) = match self.in_car() {
            Some(d) => (d.veh.angle(), d.veh.vx != 0 || d.veh.vy != 0),
            None => (
                self.player.angle,
                matches!(self.player.anim, Anim::Run | Anim::Walk),
            ),
        };
        self.peds
            .set_view(self.view.half_w, self.view.half_h, angle, moving);
    }

    /// The peds' tick, with the view where the fleet's was.
    fn run_peds(&mut self, centre: (i64, i64)) {
        let traffic = &self.traffic;
        let green = |bx, by, along_x| traffic.light_green(bx, by, along_x);
        let ctx = TickCtx {
            map: &self.map,
            cars: &self.cars,
            cam: centre,
            green: &green,
        };
        let mut lines = Vec::new();
        self.peds.tick(&ctx, &mut |s| lines.push(s));
        for l in lines {
            self.say(l);
        }
    }

    /// His car through the crowd: fast, a ped is killed; slow, shoved.
    fn run_over(&mut self) {
        if self.doing != Doing::Driving {
            return;
        }
        let Some(d) = &self.driven else {
            return;
        };
        let v = &d.veh;
        let (avx, avy) = (v.vx.abs(), v.vy.abs());
        let speed = if avx > avy { avx + avy / 2 } else { avy + avx / 2 };
        let hits = self
            .peds
            .ram(v.ox, v.oy, v.angle(), v.len / 2, v.wid / 2, d.layer, speed);
        if hits > 0 {
            let total = self.peds.stats.runover;
            self.say(format!("car: ran over {hits} - {total} so far"));
        }
    }

    /// The trains' tick, and what it did to the world.
    fn run_trains(&mut self) {
        // Not in a car and not aboard (AmiGTA1's `!in_car && pl_on < 0`):
        // getting out and drowning count as on foot.
        let on_foot = !matches!(self.doing, Doing::Driving | Doing::Riding(_));
        let p = &self.player;
        let player = on_foot.then(|| (i64::from(p.x), i64::from(p.y), p.layer));
        let slots: Vec<usize> = (0..self.traffic.n)
            .filter(|&i| !self.traffic.cars[i].done)
            .collect();
        let cars: Vec<_> = slots
            .iter()
            .map(|&i| {
                let c = &self.traffic.cars[i];
                (c.x, c.y, c.layer)
            })
            .collect();
        let ped_slots: Vec<usize> = (0..self.peds.p.len())
            .filter(|&i| self.peds.p[i].alive && !self.peds.p[i].corpse)
            .collect();
        let peds: Vec<_> = ped_slots
            .iter()
            .map(|&i| {
                let q = &self.peds.p[i];
                (q.x, q.y, q.layer)
            })
            .collect();
        for event in self.trains.tick(player, &cars, &peds) {
            match event {
                TrainEvent::RanOver => {
                    self.health = 0;
                    self.say("train: ran the player over".into());
                }
                TrainEvent::KilledPed(k) => {
                    let i = ped_slots[k];
                    if !self.peds.p[i].corpse {
                        self.peds.kill(i);
                        self.say(format!("train: ran over ped {i}"));
                    }
                }
                TrainEvent::Stopped(t) => {
                    let c = self
                        .trains
                        .carriage(t, if self.trains.trains[t].tram { 0 } else { 1 });
                    self.say(format!(
                        "train: train {t} stops at ({},{}) layer {}",
                        c.body.0 >> 21,
                        c.body.1 >> 21,
                        c.layer
                    ));
                }
                TrainEvent::Crushed(i) => {
                    // 20 points; at 100 the fleet's own sweep burns it.
                    let c = &mut self.traffic.cars[slots[i]];
                    if !c.invuln && c.damage < cars::WRECKED {
                        c.damage = (c.damage + 20).min(cars::WRECKED);
                    }
                    let (x, y) = (c.x >> 21, c.y >> 21);
                    self.say(format!("train: hit a car at ({x},{y})"));
                }
                TrainEvent::Wrecked(t, carriages) => {
                    // Each carriage is left in the fleet as a write-off of
                    // the train's (model 11) or tram's (12) record, which
                    // then burns out.
                    let id = if self.trains.trains[t].tram { 12 } else { 11 };
                    let model = self.cars.iter().position(|c| c.model == id);
                    for (x, y, face, layer) in carriages {
                        if let Some(model) = model {
                            self.traffic.abandon(
                                &self.cars,
                                model,
                                x,
                                y,
                                face,
                                layer,
                                -1,
                                cars::WRECKED,
                            );
                        }
                    }
                    if self.doing == Doing::Riding(t) {
                        self.doing = Doing::OnFoot;
                    }
                    self.say(format!("train: train {t} wrecked"));
                }
            }
        }
    }

    /// Every carriage of every working train, for the renderer.
    pub fn carriages(&self) -> Vec<Carriage> {
        let mut out = Vec::new();
        for (i, t) in self.trains.trains.iter().enumerate() {
            if !t.wrecked {
                out.extend((0..t.carriages()).map(|j| self.trains.carriage(i, j)));
            }
        }
        out
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
        // The UP key itself, as AmiGTA1 asks, not the stick: UP and DOWN
        // together still probe.
        if b.contains(Buttons::UP) {
            self.walk_into_water();
        }
    }

    /// Water is not a wall to a man who walks into it: with water 10 px
    /// ahead, below him with only air between, he goes in (AmiGTA1's main
    /// loop).
    fn walk_into_water(&mut self) {
        let p = &self.player;
        // A Q14 unit times 40 is 10 px.
        let ax = p.x + sin(p.angle) * 40;
        let ay = p.y - cos(p.angle) * 40;
        let (bx, by) = (ax >> (FP + BLOCK_SHIFT), ay >> (FP + BLOCK_SHIFT));
        let Some(z) = self.water_below(bx, by, p.layer) else {
            return;
        };
        self.player.x = ax;
        self.player.y = ay;
        self.player.layer = z;
        self.doing = Doing::Drowning(1);
        self.say(format!("player: in the water at ({bx},{by})"));
    }

    /// The layer of the water at or under layer `z` with only air between.
    fn water_below(&self, bx: i32, by: i32, z: i32) -> Option<i32> {
        let z = (0..=z)
            .rev()
            .find(|&z| self.map.ground_type(bx, by, z) != ground::AIR)?;
        (self.map.ground_type(bx, by, z) == ground::WATER).then_some(z)
    }

    /// What hurts him this tick: the water takes 3 health every other tick
    /// (a second and a half from full); at none he is wasted.
    fn harm(&mut self) {
        if let Doing::Drowning(n) = self.doing {
            if n & 1 != 0 {
                self.health -= 3;
            }
            self.doing = Doing::Drowning(n + 1);
        }
        if self.health <= 0 {
            self.wasted();
        }
    }

    /// Wasted. Until M6 brings the card, lives and hospitals, he starts
    /// again where the level put him, with full health.
    fn wasted(&mut self) {
        let how = match self.doing {
            Doing::Drowning(_) => " - drowned",
            _ => "",
        };
        self.say(format!("player: wasted{how}"));
        self.driven = None;
        let start = self.start;
        let (mut player, _) = Player::place(&self.map, start.bx, start.by);
        if let Some(a) = start.angle {
            player.angle = a & 255;
        }
        self.player = player;
        self.health = FULL_HEALTH;
        self.doing = Doing::OnFoot;
    }

    /// Is a car's body at `(px, py)` on `layer`, grown by `margin`?
    fn car_at(&self, px: i64, py: i64, layer: i32, margin: i32) -> bool {
        self.car_hit(px, py, layer, margin).is_some()
    }

    /// The fleet slot of the first car whose body is at `(px, py)` on
    /// `layer`, grown by `margin`.
    fn car_hit(&self, px: i64, py: i64, layer: i32, margin: i32) -> Option<usize> {
        self.traffic.car_at(&self.cars, px, py, layer, margin)
    }

    /// SPACE on foot: running at a car with its body 3 px ahead, go over a
    /// low one along his own heading, or slide under a tall one. Anything
    /// else does nothing.
    fn jump(&mut self) {
        let p = &self.player;
        if p.anim != Anim::Run {
            return;
        }
        // A Q14 unit times 12 is 3 px.
        let ahead = (
            i64::from(p.x) + i64::from(sin(p.angle)) * 12,
            i64::from(p.y) - i64::from(cos(p.angle)) * 12,
        );
        let Some(i) = self.car_hit(ahead.0, ahead.1, p.layer, 1) else {
            return;
        };
        if cars::vaultable(&self.cars[self.traffic.cars[i].model]) {
            self.doing = Doing::Vaulting(Vault::new(p.angle));
            self.say(format!("player: jump - vaulting car {i}"));
        } else {
            self.doing = Doing::Sliding(0);
            self.say(format!("player: jump - sliding under car {i}"));
        }
    }

    /// One tick over a roof, at running pace. At each state's end `still`
    /// is asked about the point a pixel ahead; returns true when he lands.
    fn vault_step(&mut self, v: &mut Vault, still: impl Fn(&World, (i64, i64)) -> bool) -> bool {
        let (fx, fy) = (i64::from(sin(v.head)), -i64::from(cos(v.head)));
        let run = i64::from(RUN_SPEED);
        let p = &mut self.player;
        p.x = (i64::from(p.x) + ((fx * run) >> 14)) as i32;
        p.y = (i64::from(p.y) + ((fy * run) >> 14)) as i32;
        p.angle = v.head;
        p.anim = Anim::Vault;
        p.frame = v.step as u8;
        v.tick += 1;
        if v.tick < cars::VAULT_TICKS {
            return false;
        }
        v.tick = 0;
        // A Q14 unit times 4 is a pixel.
        let ahead = (i64::from(p.x) + (fx << 2), i64::from(p.y) + (fy << 2));
        if !still(self, ahead) {
            return true;
        }
        v.hold += 1;
        if v.hold > cars::VAULT_HOLD_MAX {
            return true;
        }
        // The frame climbs to the fourth and holds there.
        if v.step < 3 {
            v.step += 1;
        }
        false
    }

    /// The free vault: over whatever car is under him, landing on his feet
    /// with the controls back.
    fn vaulting(&mut self, mut v: Vault) {
        let layer = self.player.layer;
        // Still over a car while the point ahead is on one, or while he is
        // himself within a walker's margin of one (the point alone put him
        // down on the far wing).
        let landed = self.vault_step(&mut v, |w, a| {
            let (px, py) = (i64::from(w.player.x), i64::from(w.player.y));
            w.car_at(a.0, a.1, layer, 0) || w.car_at(px, py, layer, PED_CAR_MARGIN)
        });
        if !landed {
            self.doing = Doing::Vaulting(v);
            return;
        }
        self.stand_up();
        self.say(format!("player: landed after state {}", v.step));
    }

    /// Under a tall vehicle: a pixel along his heading every state while
    /// it is still over him, then up.
    fn sliding(&mut self, mut t: i32) {
        self.player.anim = Anim::SlideUnder;
        self.player.frame = 0;
        t += 1;
        if t < cars::VAULT_TICKS {
            self.doing = Doing::Sliding(t);
            return;
        }
        let p = &self.player;
        if self.car_at(p.x.into(), p.y.into(), p.layer, 1) {
            let a = p.angle;
            self.player.x += sin(a) << 2;
            self.player.y -= cos(a) << 2;
            self.doing = Doing::Sliding(0);
            return;
        }
        self.stand_up();
        self.say("player: out from under".into());
    }

    /// Back on his feet with the controls.
    fn stand_up(&mut self) {
        self.player.anim = Anim::Stand;
        self.player.frame = 0;
        let (bx, by) = self.player.block();
        self.player.ground = self.map.ground_type(bx, by, self.player.layer);
        self.doing = Doing::OnFoot;
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

    /// RETURN on foot: take the nearest car within reach out of the fleet,
    /// standing or moving, and walk to its door.
    fn try_enter(&mut self) {
        let p = &self.player;
        let Some(t) = self.traffic.grab(p.x.into(), p.y.into(), p.layer, REACH) else {
            self.say("car: no car within reach".into());
            return;
        };
        let car = Parked {
            model: t.model,
            x: t.x as i32,
            y: t.y as i32,
            face: t.face,
            layer: t.layer,
            remap: (t.remap + 1) as u8,
            damage: t.damage,
            dmg_bits: t.dmg_bits,
            wrecked: false,
        };
        let ci = &self.cars[car.model];
        let mut door = door_point(ci, car.x.into(), car.y.into(), car.face);
        let (px, py) = (i64::from(self.player.x), i64::from(self.player.y));
        let bike = ci.vtype == CLASS_BIKE;
        let mut over = None;
        if !bike && cars::vaultable(ci) {
            let at = (i64::from(car.x), i64::from(car.y), car.face);
            if let Some((flank, head)) = cars::far_side(ci, at, (px, py), door) {
                over = Some(Over {
                    door,
                    head,
                    vault: None,
                });
                door = flank;
            }
        }
        let walk_len = walk_ticks((px, py), door);
        if let Some(o) = over.as_mut() {
            // Already against the body: straight over.
            if walk_len == 0 {
                o.vault = Some(Vault::new(o.head));
            }
        }
        let (model, width) = (ci.model, cars::half_size(ci).1 * 2 + 4);
        let e = Entering {
            car,
            door,
            from: (px, py, self.player.angle),
            walk_len,
            walk_t: 0,
            step: 0,
            tick: 0,
            bike,
            over,
            serial: t.serial,
            mission: t.mission,
            driver: t.had_driver,
        };
        self.say(format!(
            "car: going for model {model} at ({},{})",
            car.x >> 21,
            car.y >> 21
        ));
        if t.had_driver {
            self.say("car: somebody is at the wheel".into());
        }
        if let Some(o) = over {
            self.say(format!(
                "car: far side - will vault heading {}, over {width} px",
                o.head
            ));
        }
        self.doing = Doing::Entering(e);
    }

    /// The driver of the car he is getting into is dragged out of its
    /// door, or a bike's rider knocked off beside it (AmiGTA1's main loop).
    /// The carjack's score and a police car's driver coming after him on
    /// foot are M6's.
    fn throw_out(&mut self, e: &Entering) {
        let c = e.car;
        let (cx, cy) = (i64::from(c.x), i64::from(c.y));
        let layer = self.player.layer;
        let got = if e.bike {
            // 12 px out to the right, facing away.
            let (x, y) = (
                cx + i64::from(cos(c.face)) * 12 * 4,
                cy + i64::from(sin(c.face)) * 12 * 4,
            );
            self.peds.knock_off(x, y, layer, c.face + 64, -1)
        } else {
            self.peds
                .pull(&self.cars, cx, cy, c.face, c.model, layer, -1)
        };
        match got {
            Some(i) => self.say(format!("car: dragged the driver out (ped {i})")),
            None => self.say("car: driver lost - ped pool full".into()),
        }
    }

    fn entering(&mut self, mut e: Entering) {
        if let Some(mut over) = e.over
            && let Some(mut v) = over.vault
        {
            // Over the roof of the car he is getting into, which is the
            // only car the look ahead asks about.
            let car = e.car;
            let (hl, hw) = cars::half_size(&self.cars[car.model]);
            let landed = self.vault_step(&mut v, |_, a| {
                body_hit(car.x.into(), car.y.into(), car.face, hl, hw, a.0, a.1, 0)
            });
            if landed {
                // Down on the door side: the walk to the door resumes.
                let p = &self.player;
                let at = (i64::from(p.x), i64::from(p.y));
                e.from = (at.0, at.1, p.angle);
                e.door = over.door;
                e.walk_len = walk_ticks(at, over.door);
                e.walk_t = 0;
                e.over = None;
                let px = ((over.door.0 - at.0).abs() + (over.door.1 - at.1).abs()) >> 16;
                self.say(format!(
                    "car: landed after state {} at ({},{}), {px} px from the door",
                    v.step,
                    at.0 >> 16,
                    at.1 >> 16
                ));
            } else {
                over.vault = Some(v);
                e.over = Some(over);
            }
            self.doing = Doing::Entering(e);
            return;
        }
        if e.walk_t < e.walk_len {
            let p = &mut self.player;
            p.anim = Anim::Walk;
            p.frame = ((e.walk_t / 3) % 8) as u8;
            p.x = lerp_fp(e.from.0, e.door.0, e.walk_t, e.walk_len) as i32;
            p.y = lerp_fp(e.from.1, e.door.1, e.walk_t, e.walk_len) as i32;
            // Towards the car's heading, or straight across it when he is
            // making for the roof.
            let face = e.over.map_or(e.car.face, |o| o.head);
            p.angle = lerp_angle(e.from.2, face, e.walk_t, e.walk_len);
            e.walk_t += 1;
            if let Some(o) = e.over.as_mut()
                && e.walk_t >= e.walk_len
            {
                o.vault = Some(Vault::new(o.head));
            }
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
        // The door is fully open (a bike has none): out comes the driver.
        if e.driver && (e.bike || t >= DOOR_OPEN) {
            e.driver = false;
            self.throw_out(&e);
        }
        let p = &mut self.player;
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
            serial: e.serial,
            mission: e.mission,
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
            // Out over the roof: put on the car half way along its front
            // half, facing the flank the door is not on, and into the
            // vault. The car is parked first so the vault finds it under
            // him.
            let (ox, oy, layer) = (v.ox, v.oy, d.layer);
            let (hl, _) = cars::half_size(ci);
            let head = (a - cars::door_side(ci) as i32 * 64) & 255;
            self.park_driven(a);
            // A Q14 unit times 2 is half a pixel.
            let p = &mut self.player;
            p.x = (ox + i64::from(sin(a)) * i64::from(hl) * 2) as i32;
            p.y = (oy - i64::from(cos(a)) * i64::from(hl) * 2) as i32;
            p.angle = head;
            p.layer = layer;
            self.doing = Doing::Vaulting(Vault::new(head));
            self.say(format!(
                "car: door blocked by {what} - over the roof, heading {head}"
            ));
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
        self.park_driven(e.face);
        self.stand_up();
        self.say(format!(
            "car: out at ({},{}) facing {}",
            self.player.x >> 16,
            self.player.y >> 16,
            self.player.angle
        ));
    }

    /// Leave the driven car standing in the fleet where it is, facing
    /// `face`, under the serial it had before (AmiGTA1's `leave_car`). Its
    /// dents go with it, which AmiGTA1 drops.
    fn park_driven(&mut self, face: i32) {
        let Some(d) = self.driven.take() else {
            return;
        };
        let v = &d.veh;
        let remap = i32::from(v.remap.unwrap_or(0)) - 1;
        let (x, y) = (v.ox, v.oy);
        let Some(mut serial) = self
            .traffic
            .abandon(&self.cars, v.model, x, y, face, d.layer, remap, v.damage)
        else {
            self.say("car: fleet full, car lost".into());
            return;
        };
        if d.serial != 0 {
            self.traffic.rename_car(serial, d.serial);
            serial = d.serial;
        }
        if let Some(c) = self.traffic.car_mut(serial) {
            c.dmg_bits = v.dmg_bits;
            c.mission |= d.mission;
        }
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
        let (ox0, oy0, ang0) = (d.veh.ox, d.veh.oy, d.veh.ang16);
        let ci = &self.cars[d.veh.model];
        let water = d.tick(&self.map, ci, pedals, &mut |s| lines.push(s));
        if water.is_none() {
            // The fleet: stopped at the contact rather than inside a car,
            // then the ram (AmiGTA1's main loop).
            let v = &mut d.veh;
            let (hl, hw) = (traffic::half_len(ci), traffic::half_wid(ci));
            let mut pos = (v.ox, v.oy, v.ang16);
            if self
                .traffic
                .sweep_box(&self.cars, ox0, oy0, ang0, &mut pos, hl, hw, d.layer)
            {
                v.x += pos.0 - v.ox;
                v.y += pos.1 - v.oy;
                v.ox = pos.0;
                v.oy = pos.1;
                v.ang16 = pos.2;
            }
            let r = self.traffic.ram(
                &self.cars,
                v.ox,
                v.oy,
                v.angle(),
                v.len / 2,
                v.wid / 2,
                v.vx,
                v.vy,
                v.mass,
                d.layer,
            );
            if r.dvx != 0 || r.dvy != 0 || r.dyaw != 0 {
                v.vx += r.dvx;
                v.vy += r.dvy;
                v.ang16 = (v.ang16 + r.dyaw) & 0xFF_FFFF;
            }
            v.x += r.dpx;
            v.ox += r.dpx;
            v.y += r.dpy;
            v.oy += r.dpy;
            if r.hits > 0 {
                v.damage += r.damage;
                // The push points out of the other car: the dent is the
                // other way.
                let (hx, hy) = (v.ox - r.dpx * 8, v.oy - r.dpy * 8);
                v.dmg_bits |= 1 << panel_delta(ci, v.ox, v.oy, v.angle(), hx, hy);
                lines.push(format!(
                    "car: ram x{} - player dv ({},{}) damage {} (+{} this crash)",
                    r.hits,
                    r.dvx >> 16,
                    r.dvy >> 16,
                    v.damage,
                    r.damage
                ));
            }
        }
        if self.doing == Doing::Driving {
            self.player.x = d.veh.ox as i32;
            self.player.y = d.veh.oy as i32;
            self.player.layer = d.layer;
            self.player.angle = d.veh.angle();
        }
        for l in lines {
            self.say(l);
        }
        if let Some(z) = water {
            self.sink(z);
        }
    }

    /// The car is in the water: it sinks and is lost, and he drowns where
    /// it went down.
    fn sink(&mut self, z: i32) {
        let Some(d) = self.driven.take() else {
            return;
        };
        self.player.x = d.veh.ox as i32;
        self.player.y = d.veh.oy as i32;
        self.player.layer = z;
        self.doing = Doing::Drowning(1);
        self.say("car: sank".into());
    }

    /// A write-off burns for [`cars::FUSE`] ticks, then blows up: the
    /// player is thrown clear to its right and it stays as a wreck
    /// (AmiGTA1's main loop; the blast's harm to others is M6).
    fn burn(&mut self) {
        if self.doing != Doing::Driving {
            return;
        }
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
        let (model, remap) = (v.model, i32::from(v.remap.unwrap_or(0)) - 1);
        self.driven = None;
        self.traffic
            .leave_wreck(&self.cars, model, x, y, face, layer, remap);
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

    /// The car the player is walking to or getting into, with its door
    /// (it is out of the fleet meanwhile).
    pub fn standing_cars(&self) -> Vec<ShownCar> {
        let shown = |c: &Parked, extra: u32| ShownCar {
            model: c.model,
            remap: c.remap,
            pos: [c.x, c.y, self.map.ground_z(c.x, c.y, c.layer)],
            face: c.face,
            deltas: c.dmg_bits | extra,
            fuse: 0,
        };
        let mut out = Vec::new();
        if let Doing::Entering(e) = self.doing {
            let t = e.step * ENTER_TICKS + e.tick;
            // No door until he is at it: not on the way, not on the roof.
            let door = if e.bike || e.walk_t < e.walk_len || e.over.is_some() {
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
            // Aboard a train he is out of sight, and in the water he has
            // gone under.
            Doing::Riding(_) | Doing::Drowning(_) => None,
            Doing::OnFoot => Some(self.player.ped_frame()),
            Doing::Entering(e) if e.walk_t < e.walk_len || e.over.is_some() => {
                Some(self.player.ped_frame())
            }
            Doing::Vaulting(_) | Doing::Sliding(_) => Some(self.player.ped_frame()),
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

    /// A test fixture: the player on foot in the middle of block `(bx, by)`
    /// on `layer`, keeping his heading.
    pub fn stand(&mut self, bx: i32, by: i32, layer: i32) {
        let angle = self.player.angle;
        let (mut player, _) = Player::place(&self.map, bx, by);
        player.layer = layer;
        player.angle = angle;
        player.ground = self.map.ground_type(bx, by, layer);
        self.player = player;
        self.doing = Doing::OnFoot;
        let g = ground_name(self.player.ground);
        self.say(format!(
            "player: stand ({bx},{by}) layer {layer} ground {g}"
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
            serial: 0,
            mission: false,
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
        let x = i64::from(self.player.x) + (i64::from(dx) << 16);
        let y = i64::from(self.player.y) + (i64::from(dy) << 16);
        let face = face & 255;
        let layer = self.player.layer;
        if self
            .traffic
            .abandon(&self.cars, model, x, y, face, layer, -1, 0)
            .is_none()
        {
            self.say("car: park - fleet full".into());
            return;
        }
        self.say(format!(
            "car: parked model {} at ({},{}) facing {face}",
            self.cars[model].model,
            x >> 16,
            y >> 16,
        ));
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
                h.bytes(&e.serial.to_le_bytes());
                h.bool(e.mission);
                h.bool(e.driver);
                match e.over {
                    None => h.u8(0),
                    Some(o) => {
                        h.u8(1);
                        h.bytes(&o.door.0.to_le_bytes());
                        h.bytes(&o.door.1.to_le_bytes());
                        h.i32(o.head);
                        match o.vault {
                            None => h.u8(0),
                            Some(v) => {
                                h.u8(1);
                                v.sim_hash(&mut h);
                            }
                        }
                    }
                }
            }
            Doing::Riding(t) => {
                h.u8(4);
                h.u32(t as u32);
            }
            Doing::Vaulting(v) => {
                h.u8(5);
                v.sim_hash(&mut h);
            }
            Doing::Sliding(t) => {
                h.u8(6);
                h.i32(t);
            }
            Doing::Drowning(n) => {
                h.u8(7);
                h.u32(n);
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
        self.trains.sim_hash(&mut h);
        self.traffic.sim_hash(&mut h);
        self.peds.sim_hash(&mut h);
        h.i32(self.health);
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
/// Ticks to walk from `a` to `b` at the walk-to-the-door pace: three ticks
/// every four pixels of Manhattan distance, at most 60.
fn walk_ticks(a: (i64, i64), b: (i64, i64)) -> i32 {
    let px = ((b.0 - a.0).abs() + (b.1 - a.1).abs()) >> 16;
    ((px * 3) / 4).clamp(0, 60) as i32
}

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
