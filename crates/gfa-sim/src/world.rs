//! The whole simulation state and its tick.

use crate::cars::{
    self, BIKE_ENTER_FIRST, BIKE_EXIT_FIRST, BIKE_FRAMES, BIKE_SIT, DOOR_OPEN, Driven, ENTER_SEQ,
    ENTER_TICKS, EXIT_FIRST, EXIT_FRAMES, EXIT_TICKS, PED_CAR_MARGIN, Parked, REACH, body_hit,
    door_point, door_pos, lerp_angle, lerp_fp, panel_delta,
};
use crate::hash::{SimHash, StateHasher};
use crate::input::{Buttons, TickInput};
use crate::map::{SimMap, ground};
use crate::marks::Marks;
use crate::peds::{self, Peds, Punch, TickCtx};
use crate::pickups::{self, Pickups};
use crate::player::{Anim, BODY_R, Player, RUN_SPEED, Stick};
use crate::score::{self, Score, crime};
use crate::service::Service;
use crate::traffic::{self, Traffic};
use crate::trains::{Carriage, TrainEvent, Trains};
use crate::vehicle::{CLASS_BIKE, Pedals, Veh};
use crate::view::{SimCamera, SimView};
use crate::weapons::{self, Arms, Cannon, Street, Target, Trigger, Weapons};
use gfa_fixed::fx::{BLOCK_SHIFT, FP, block_centre};
use gfa_fixed::trig::{cos, sin, step_dir};
use gfa_formats::mission::Section;
use gfa_formats::style::CarInfo;

/// Health at the start and after a respawn.
pub const FULL_HEALTH: i32 = 100;
/// Ticks he burns once set alight, a point of health each.
pub const BURN_TICKS: i32 = 100;
/// Default starting lives.
pub const DEFAULT_LIVES: i32 = 4;
/// Ticks of double walking/running speed from a speed power-up.
pub const SPEED_TICKS: i32 = 375;
/// Ticks lying on the ground when arrested on foot before the BUSTED card.
pub const FOOT_ARREST_TICKS: i32 = 14;
/// Ticks the BUSTED or WASTED card is shown before respawn.
pub const BUST_TICKS: i32 = 150;

/// Full-screen card state after arrest or death.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Card {
    #[default]
    None,
    Busted,
    Wasted,
    GameOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SprayShop {
    pub line: i32,
    pub bx: i32,
    pub by: i32,
    pub color: i32,
    pub used: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BombShop {
    pub line: i32,
    pub bx: i32,
    pub by: i32,
    pub used: bool,
}

fn emergency_vehicle(model: u8) -> bool {
    matches!(model, 4 | 5 | 15 | 16 | 32 | 42)
}

/// GTA8.EXE 0xe5f30. A plate change or paint job pays for repairs and
/// doubles the police fee at each wanted level. The multiplier adds a
/// percentage surcharge. Paint alone has a flat minimum service price.
fn spray_cost(car: &CarInfo, damage: i32, level: i32, multiplier: i32) -> i64 {
    let repairs = i64::from(damage.max(0)) * i64::from(car.value[0] as u16) * 10;
    if repairs == 0 && level == 0 {
        return 500;
    }
    let police = if level > 0 { 1000 << (level - 1) } else { 0 };
    (repairs + police) * (100 + i64::from(multiplier)) / 100
}

/// About two seconds at `50 Hz` (the counter-punch knockdown, `GTA8.EXE`).
pub const KNOCKED_TICKS: i32 = 100;

/// A type-4 (damage) bomb goes off once its car's damage is above this:
/// `GTA8.EXE` `0x571bc` (`car+0xf7 > 10`), set once at `0x52012`.
pub const BOMB4_DAMAGE: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CarBomb {
    pub serial: u64,
    pub kind: i32,
    pub fuse: i32,
}

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
    /// Waypoints round the nose or tail of a tall non-vaultable vehicle.
    pub via: [(i64, i64); 2],
    pub via_n: u8,
    /// The car's fleet serial, whether a mission had marked it, and
    /// whether somebody was at the wheel.
    pub serial: u64,
    pub mission: bool,
    pub driver: bool,
    pub cop: bool,
}

/// Sliding under a tall vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slide {
    pub steps: i32,
    /// True once his body has overlapped the vehicle beneath which he slides.
    pub entered: bool,
}

impl SimHash for Slide {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.i32(self.steps);
        h.bool(self.entered);
    }
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
    /// Sliding under a tall vehicle; carries steps elapsed and whether his
    /// body has overlapped the vehicle yet.
    Sliding(Slide),
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
    /// The view at rest.
    pub view: SimView,
    /// Where the view is now: its lead and zoom.
    pub camera: SimCamera,
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
    /// Each point stops one bullet.
    pub armour: i32,
    /// Ticks he has left alight: a point of health each.
    pub burning: i32,
    /// His guns and the fire key.
    pub arms: Arms,
    /// Everything in flight, every explosion and fire.
    pub weapons: Weapons,
    /// The tank's cannon, loaded by a kill frenzy.
    pub cannon: Cannon,
    /// A kill frenzy the script started (M7's `KF_PROCESS` to `RESET_KF`);
    /// the pickup's runs on its endless weapon ([`World::in_frenzy`]).
    pub frenzy: bool,
    /// Weapon cooldown, projectile flight, and splat clock: 23.333 Hz against 50.
    weapons_clock: crate::clock::Ratio,
    pub pickups: Pickups,
    pub marks: Marks,
    pub service: Service,
    pub score: Score,
    pub lives: i32,
    pub jail_free: bool,
    pub player_speed: i32,
    /// Ticks he is down for after a ped's counter-punch (`GTA8.EXE`
    /// knocks him down about two seconds, with no harm to health or armour).
    pub knocked: i32,
    pub bust_timer: i32,
    pub card: Card,
    pub foot_arrest: i32,
    pub game_over: bool,
    pub live_city: bool,
    pub sprays: Vec<SprayShop>,
    pub bombshops: Vec<BombShop>,
    pub bomb_cost: i32,
    pub bombs: [CarBomb; 4],
    pub shop_half: bool,
    pub cops_killed_seen: u64,
    pub section: Option<Section>,
    pub explode_proc: Option<(usize, i32)>,
    /// The mission script, running ([`World::start_mission`]); `None` until a
    /// level starts it.
    pub mission: Option<Box<crate::mission::Vm>>,
    /// Where the level put him, for the stopgap respawn.
    start: Start,
    /// Last tick's buttons, to find presses.
    prev: Buttons,
    /// Raw unmasked fire button from the previous tick, for the event-style latch.
    prev_fire: bool,
    log: Vec<LogLine>,
    /// Transient one-shot audio events for the audio layer (not in `World::hash`).
    audio: Vec<crate::audio::AudioEvent>,
    /// Transient one-shot HUD events for the HUD layer (not in `World::hash`).
    hud: Vec<crate::hud::HudEvent>,
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
        if map_cars {
            traffic.police_start(&map, &cars);
        }
        let score = Score::new(map.cmp().style_number);
        let mut world = World {
            tick: 0,
            map,
            cars,
            view,
            camera: SimCamera::REST,
            player,
            doing: Doing::OnFoot,
            driven: None,
            trains,
            traffic,
            peds: Peds::new(peds::SEED, Peds::cap_for(view)),
            health: FULL_HEALTH,
            armour: 0,
            burning: 0,
            arms: Arms::default(),
            weapons: Weapons::default(),
            cannon: Cannon::default(),
            frenzy: false,
            weapons_clock: crate::clock::weapons(),
            pickups: Pickups::new(),
            marks: Marks::new(),
            service: Service::new(),
            score,
            lives: DEFAULT_LIVES,
            jail_free: false,
            player_speed: 0,
            knocked: 0,
            bust_timer: 0,
            card: Card::None,
            foot_arrest: 0,
            game_over: false,
            live_city: map_cars,
            sprays: Vec::new(),
            bombshops: Vec::new(),
            bomb_cost: 5000,
            bombs: [CarBomb::default(); 4],
            shop_half: false,
            cops_killed_seen: 0,
            section: None,
            explode_proc: None,
            mission: None,
            start,
            prev: Buttons::NONE,
            prev_fire: false,
            log: Vec::new(),
            audio: Vec::new(),
            hud: Vec::new(),
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

    /// Load crates, spray shops and bomb shops from a mission section.
    pub fn load_section(&mut self, section: &Section) {
        self.pickups = Pickups::from_section(section, &self.map);
        self.sprays.clear();
        self.bombshops.clear();
        self.bomb_cost = 5000;
        for d in &section.decls {
            match d.kind.as_str() {
                "SPRAY" => {
                    self.sprays.push(SprayShop {
                        line: d.id,
                        bx: d.pos[0],
                        by: d.pos[1],
                        color: d.nums.first().copied().unwrap_or(0),
                        used: false,
                    });
                }
                "BOMBSHOP" => {
                    self.bombshops.push(BombShop {
                        line: d.id,
                        bx: d.pos[0],
                        by: d.pos[1],
                        used: false,
                    });
                }
                "BOMBSHOP_COST" => {
                    self.bomb_cost = d.nums.first().copied().unwrap_or(5000);
                }
                _ => {}
            }
        }
        self.section = Some(section.clone());
    }

    /// Drain one-shot audio events emitted since the last call.
    pub fn drain_audio(&mut self) -> Vec<crate::audio::AudioEvent> {
        std::mem::take(&mut self.audio)
    }

    /// Drain one-shot HUD events emitted since the last call.
    pub fn drain_hud(&mut self) -> Vec<crate::hud::HudEvent> {
        std::mem::take(&mut self.hud)
    }

    pub fn emit_hud(&mut self, event: crate::hud::HudEvent) {
        self.hud.push(event);
    }

    /// Queue a pager message (FXT key or literal text).
    pub fn hud_pager(&mut self, text: crate::hud::HudText, secs: Option<i32>) {
        self.emit_hud(crate::hud::HudEvent::Pager { text, secs });
    }

    /// Queue a bottom-left brief box message.
    pub fn hud_brief(&mut self, kind: crate::hud::BriefKind, text: crate::hud::HudText) {
        self.emit_hud(crate::hud::HudEvent::Brief { kind, text });
    }

    /// Queue a centred big card message.
    pub fn hud_big(&mut self, text: crate::hud::HudText) {
        self.emit_hud(crate::hud::HudEvent::Big { text });
    }

    fn emit_sfx(&mut self, id: u8, x: i32, y: i32, layer: i32) {
        self.audio
            .push(crate::audio::AudioEvent::Sfx { id, x, y, layer });
    }

    fn emit_voice(&mut self, line: u8, pos: Option<(i32, i32, i32)>, cut: bool) {
        self.audio
            .push(crate::audio::AudioEvent::Voice { line, pos, cut });
    }

    /// Advance one tick.
    pub fn step(&mut self, input: TickInput) {
        let prev_blasts: Vec<bool> = self.weapons.blasts.iter().map(|b| b.frame > 0).collect();
        let prev_splats: Vec<i32> = self.weapons.splats.iter().map(|s| s.ticks).collect();
        let prev_peds: Vec<(bool, bool, bool)> = self
            .peds
            .p
            .iter()
            .map(|p| (p.alive, p.burn > 0, p.corpse))
            .collect();
        let prev_kills = self.score.kills;
        let raw_fire = input.buttons.contains(Buttons::FIRE);
        let b = if self.bust_timer > 0 || self.foot_arrest > 0 || self.game_over {
            Buttons::NONE
        } else {
            input.buttons
        };
        let pressed = b.pressed_since(self.prev);
        let gun_step = self.weapons_clock.step();
        // In a tank the special key held turns the turret with left and
        // right, on the game frames, and the hull does not steer.
        let aiming = self.aiming(b);
        if aiming
            && gun_step
            && let Some(d) = self.driven.as_mut()
        {
            let turn = i32::from(b.contains(Buttons::RIGHT)) - i32::from(b.contains(Buttons::LEFT));
            d.turret = (d.turret + turn * cars::TURRET_STEP) & 255;
        }
        if self.foot_arrest > 0 {
            self.player.anim = Anim::SlideUnder;
            self.player.frame = 0;
        }
        // The cycle keys work anywhere, skipping guns with no rounds.
        for (key, forward) in [(Buttons::WEAPON_NEXT, true), (Buttons::WEAPON_PREV, false)] {
            if pressed.contains(key) {
                self.arms.cycle(forward);
                let w = self.arms.weapon;
                self.say(format!("player: weapon {w} (ammo {})", self.arms.ammo[w]));
            }
        }
        if !b.contains(Buttons::FIRE) {
            self.arms.fire_held = false;
        } else if !self.prev_fire {
            self.arms.fire_held = true;
        }
        if self.doing == Doing::Driving
            && b.contains(Buttons::FIRE)
            && let Some(d) = &self.driven
            && d.serial != 0
        {
            let ser = d.serial;
            let mut armed = false;
            for bm in &mut self.bombs {
                if bm.serial == ser && bm.kind == 1 {
                    bm.kind = 2;
                    bm.fuse = 250;
                    armed = true;
                }
            }
            if armed {
                self.say("car: bomb armed - five seconds".into());
                self.hud_pager(crate::hud::HudText::literal("BOMB ARMED!"), Some(5));
            }
        }
        if pressed.contains(Buttons::ALL_WEAPONS) {
            self.arms.ammo[weapons::weapon::PISTOL..].fill(99);
            self.say("cheat: keypad-* - every weapon, 99 rounds".into());
        }
        if pressed.contains(Buttons::ENTER) {
            match self.doing {
                Doing::OnFoot if self.knocked > 0 => {}
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
        if pressed.contains(Buttons::HANDBRAKE) && self.doing == Doing::OnFoot && self.knocked == 0
        {
            self.jump();
        }
        let released = self.prev.pressed_since(b);
        self.special_key(
            pressed.contains(Buttons::SPECIAL),
            released.contains(Buttons::SPECIAL),
        );
        if b.contains(Buttons::SPECIAL) && !aiming {
            self.pull_over();
        }
        match self.doing {
            Doing::OnFoot => self.walk(b),
            Doing::Entering(e) => self.entering(e),
            Doing::Exiting(e) => self.exiting(e),
            Doing::Riding(t) => self.ride(t, b, pressed.contains(Buttons::FIRE)),
            Doing::Vaulting(v) => self.vaulting(v),
            Doing::Sliding(s) => self.sliding(s),
            // In the water he does nothing but drown.
            Doing::Driving | Doing::Drowning(_) => {}
        }
        if self.doing == Doing::Driving {
            let pull = self.peds.cop_pulling();
            let pedals = if pull > 0 {
                if pull == 1
                    && let Some(d) = self.driven.as_mut()
                    && self.cars[d.veh.model].vtype != CLASS_BIKE
                {
                    d.door_tick = 0;
                }
                Pedals {
                    throttle: false,
                    brake: false,
                    steer: 0,
                    handbrake: true,
                }
            } else {
                Pedals {
                    throttle: b.contains(Buttons::UP),
                    brake: b.contains(Buttons::DOWN),
                    steer: if aiming {
                        0
                    } else {
                        i32::from(b.contains(Buttons::RIGHT)) - i32::from(b.contains(Buttons::LEFT))
                    },
                    handbrake: b.contains(Buttons::HANDBRAKE),
                }
            };
            self.drive(pedals);
        }
        self.trigger(gun_step);
        self.cannon_tick(gun_step);
        // The DOS camera and weapons share the 23.333 Hz game-frame cadence.
        if gun_step {
            let car = self.in_car().map(|d| (d.veh.speed(), d.veh.ang16 as i32));
            self.camera.follow(self.view, car);
        }
        let centre = self.run_traffic();
        self.run_trains();
        if self.live_city {
            let mut lines = Vec::new();
            self.service.step(
                &self.map,
                &self.cars,
                &mut self.traffic,
                &mut self.peds,
                &mut self.weapons,
                &mut |s| lines.push(s),
            );
            for l in lines {
                self.say(l);
            }
        }
        self.shops_tick();
        self.bombs_tick();
        self.explode_proc_tick();
        self.mission_tick();
        self.peds_view();
        self.burn();
        self.score.tick(gun_step);
        self.run_peds(centre);
        // The shots fly after the people have moved.
        self.street(|w, st| w.tick(st, gun_step));
        self.marks.tick(&self.map, &mut self.peds, &self.weapons);
        self.pickups_tick();
        self.aim_at_player();
        self.harm();
        self.run_over();
        self.gouranga();
        let mut new_sfx = Vec::new();
        for (b, was) in self.weapons.blasts.iter().zip(prev_blasts) {
            if b.frame > 0 && !was {
                new_sfx.push((
                    crate::audio::snd::EXPLOSION,
                    b.x as i32,
                    b.y as i32,
                    b.layer,
                ));
            }
        }
        for (s, prev_t) in self.weapons.splats.iter().zip(prev_splats) {
            if s.ticks > prev_t {
                new_sfx.push((
                    crate::audio::snd::BULLET_HIT,
                    s.x as i32,
                    s.y as i32,
                    s.layer,
                ));
            }
        }
        for (p, (was_alive, was_burning, was_corpse)) in self.peds.p.iter().zip(prev_peds) {
            if !p.alive || !was_alive {
                continue;
            }
            if !was_burning && p.burn > 0 {
                new_sfx.push((
                    crate::audio::snd::IGNITE,
                    p.x as i32,
                    p.y as i32,
                    p.layer,
                ));
            }
            if !was_corpse && p.corpse {
                let id = if was_burning && p.burn == 0 {
                    crate::audio::snd::BURN_DEATH
                } else {
                    crate::audio::snd::DEATH
                };
                new_sfx.push((id, p.x as i32, p.y as i32, p.layer));
            }
        }
        for (id, x, y, layer) in new_sfx {
            self.emit_sfx(id, x, y, layer);
        }
        if self.score.kills != prev_kills && self.in_frenzy() {
            self.audio.push(crate::audio::AudioEvent::FrenzyCheer);
        }
        self.prev = b;
        self.prev_fire = raw_fire;
        self.tick += 1;
    }

    /// The special key, TAB (the original's control 7; AmiGTA1
    /// `special_key`), on the ticks it goes `down` or comes `up`: in a car
    /// with a siren, the siren ([`cars::Siren`]); in any other car, the
    /// horn while it is held; on foot, a shout (its sample, one of two by
    /// the voice's dice, is the sound's to pick: M8). Out of the car, the
    /// siren and the horn stop.
    fn special_key(&mut self, down: bool, up: bool) {
        let mut line = None;
        match self.doing {
            Doing::Driving => {
                let Some(d) = self.driven.as_mut() else {
                    return;
                };
                let id = d.veh.model_id;
                if id == crate::vehicle::TANK_MODEL {
                    // The turret's key in a tank (see `aiming`): no horn
                    // (owner, under DOSBox-X).
                } else if cars::has_siren(id) {
                    let was = d.siren;
                    d.siren = match d.siren {
                        cars::Siren::Off if down => cars::Siren::On,
                        cars::Siren::On if down => cars::Siren::Sounding,
                        cars::Siren::Sounding if up => cars::Siren::On,
                        s => s,
                    };
                    if d.siren != was {
                        let what = if d.siren == cars::Siren::Sounding {
                            "sounding"
                        } else {
                            "on"
                        };
                        line = Some(format!("special key - siren {what} (model {id})"));
                    }
                } else if down || up {
                    d.horn = down;
                    let what = if down { "down" } else { "up" };
                    line = Some(format!("special key - horn {what} (model {id})"));
                }
            }
            Doing::OnFoot | Doing::Entering(_) if down => {
                self.audio.push(crate::audio::AudioEvent::FootShout {
                    x: self.player.x,
                    y: self.player.y,
                    layer: self.player.layer,
                });
                line = Some("special key - a shout".into());
            }
            _ => {}
        }
        if self.doing != Doing::Driving
            && let Some(d) = self.driven.as_mut()
        {
            d.siren = cars::Siren::Off;
            d.horn = false;
        }
        if let Some(l) = line {
            self.say(l);
        }
    }

    /// The special key held in a car: the car ahead pulls over
    /// ([`Traffic::pull_over`]).
    fn pull_over(&mut self) {
        let Some(d) = self
            .driven
            .as_ref()
            .filter(|_| self.doing == Doing::Driving)
        else {
            return;
        };
        let v = &d.veh;
        let told = self
            .traffic
            .pull_over(&self.map, v.ox, v.oy, d.layer, v.angle(), v.len / 2);
        if let Some(serial) = told {
            self.say(format!("traffic: car {serial} pulls over"));
        }
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
            if took {
                self.crime(crime::HIJACK);
            }
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

    /// The fire key's tick: a punch in the air runs every tick (`50 Hz`),
    /// while gun cooldowns and triggers step on the `23.333 Hz` weapon clock
    /// (`gun_step`). On his feet, a punch is thrown or a shot fired, and the
    /// shot frightens the people around (AmiGTA1's main loop).
    fn trigger(&mut self, gun_step: bool) {
        let can = self.knocked == 0 && matches!(self.doing, Doing::OnFoot | Doing::Drowning(_));
        let p = &self.player;
        let (x, y, layer, angle) = (i64::from(p.x), i64::from(p.y), p.layer, p.angle);
        match self.arms.tick(can, gun_step) {
            None | Some(Trigger::Swing) => {}
            Some(Trigger::Blow) => match self.peds.punch(x, y, angle, layer) {
                Punch::Down(i) => {
                    self.emit_sfx(crate::audio::snd::PUNCH, x as i32, y as i32, layer);
                    self.say(format!("player: punch - ped {i} down"));
                }
                Punch::Counter(i) => {
                    self.emit_sfx(crate::audio::snd::PUNCH, x as i32, y as i32, layer);
                    self.knocked = KNOCKED_TICKS;
                    self.say(format!("player: ped {i} punched back - he is down"));
                }
                Punch::Nobody => {}
            },
            Some(Trigger::Shot(w)) => {
                let running = p.anim == Anim::Run;
                if self.weapons.fire(w, x, y, layer, angle, running, None) {
                    if let Some(id) = crate::audio::snd::for_weapon(w) {
                        self.emit_sfx(id, x as i32, y as i32, layer);
                    }
                    let scared = self.peds.panic(x, y, layer);
                    for (i, (px, py, pl)) in scared.into_iter().enumerate() {
                        self.audio.push(crate::audio::AudioEvent::Panic {
                            x: px,
                            y: py,
                            layer: pl,
                            first: i == 0,
                        });
                    }
                    if self.arms.spent() {
                        let w = self.arms.weapon;
                        self.say(format!("player: out of ammo - weapon {w}"));
                    }
                }
            }
        }
    }

    /// Is he driving a tank?
    pub fn driving_tank(&self) -> bool {
        self.doing == Doing::Driving
            && self
                .driven
                .as_ref()
                .is_some_and(|d| d.veh.model_id == crate::vehicle::TANK_MODEL)
    }

    /// Is he in a tank with the special key held: aiming its turret?
    fn aiming(&self, b: Buttons) -> bool {
        b.contains(Buttons::SPECIAL) && self.driving_tank()
    }

    /// Is a kill frenzy on: the script's, or the pickup's endless weapon?
    pub fn in_frenzy(&self) -> bool {
        self.frenzy || self.arms.inf_ticks > 0
    }

    /// A kill frenzy starts (the script's; a test's): the cannon is loaded.
    pub fn start_frenzy(&mut self) {
        self.frenzy = true;
        self.cannon.load();
        self.say("frenzy: started".into());
    }

    /// The cannon's tick: in a tank during a frenzy, fire sends a shell
    /// along the turret.
    fn cannon_tick(&mut self, gun_step: bool) {
        let can = self.driving_tank() && self.in_frenzy();
        if !self.cannon.tick(can, self.arms.fire_held, gun_step) {
            return;
        }
        let Some(d) = self.driven.as_ref() else {
            return;
        };
        let angle = (d.veh.angle() + d.turret) & 255;
        let (x, y, layer) = (d.veh.ox, d.veh.oy, d.layer);
        if self.weapons.fire_shell(x, y, layer, angle) {
            self.emit_sfx(crate::audio::snd::ROCKET, x as i32, y as i32, layer);
            let scared = self.peds.panic(x, y, layer);
            for (i, (px, py, pl)) in scared.into_iter().enumerate() {
                self.audio.push(crate::audio::AudioEvent::Panic {
                    x: px,
                    y: py,
                    layer: pl,
                    first: i == 0,
                });
            }
            let left = self.cannon.shells;
            self.say(format!("tank: shell fired at angle {angle} ({left} left)"));
        }
    }

    /// Is car model `m` (an index into the style's cars) the tank, which
    /// carries a turret? The player's points where he aimed it
    /// ([`Driven::turret`], from the hull); any other along its hull.
    pub fn is_tank(&self, m: usize) -> bool {
        self.cars
            .get(m)
            .is_some_and(|c| c.model == crate::vehicle::TANK_MODEL)
    }

    /// The weapons' call, with the street they act on.
    fn street<R>(&mut self, f: impl FnOnce(&mut Weapons, &mut Street) -> R) -> R {
        let mut lines = Vec::new();
        let r = {
            let mut say = |s: String| lines.push(s);
            let mut st = Street {
                map: &self.map,
                cars: &self.cars,
                peds: &mut self.peds,
                traffic: &mut self.traffic,
                pickups: &mut self.pickups,
                score: &mut self.score,
                say: &mut say,
            };
            f(&mut self.weapons, &mut st)
        };
        for l in lines {
            self.say(l);
        }
        r
    }

    /// Where the weapons will find him next tick: his car's box while he
    /// is in it, else his body; and any shots the police or gunmen fired.
    fn aim_at_player(&mut self) {
        let layer = self.player.layer;
        let target = match self.in_car() {
            Some(d) => Target {
                x: d.veh.ox,
                y: d.veh.oy,
                layer,
                car: Some((d.veh.len / 2, d.veh.wid / 2)),
            },
            None => Target {
                x: i64::from(self.player.x),
                y: i64::from(self.player.y),
                layer,
                car: None,
            },
        };
        self.weapons.set_target(target);
        let shoot_mode = if self.score.level >= 4 {
            2
        } else if self.arms.armed() || self.score.level >= 3 {
            1
        } else {
            0
        };
        self.peds.set_cop_shoot(shoot_mode);
        while let Some((si, sx, sy, sl, sa)) = self.peds.cop_shot() {
            let w = match self.peds.p[si].weapon {
                1 => weapons::weapon::MACHINE_GUN,
                2 => weapons::weapon::ROCKET,
                3 => weapons::weapon::FLAME,
                _ => weapons::weapon::PISTOL,
            };
            if self.weapons.fire(w, sx, sy, sl, sa, false, Some(si))
                && let Some(id) = crate::audio::snd::for_weapon(w)
            {
                self.emit_sfx(id, sx as i32, sy as i32, sl);
            }
        }
    }

    /// Report a crime, logging a change of wanted level.
    fn crime(&mut self, c: usize) {
        if let Some(l) = self.score.crime(c) {
            self.say(l);
        }
    }

    /// His car, while he drives it or gets out of it: what the fleet and
    /// the peds see instead of him.
    fn in_car(&self) -> Option<&Driven> {
        self.driven
            .as_ref()
            .filter(|_| self.doing == Doing::Driving || matches!(self.doing, Doing::Exiting(_)))
    }

    /// The view now: the view at rest at the camera's zoom.
    pub fn view_now(&self) -> SimView {
        self.view.at(self.camera.zoom)
    }

    /// The view's centre now, 16.16: on the player (or his car), plus the
    /// lead.
    pub fn view_centre(&self) -> (i64, i64) {
        let (x, y) = match self.in_car() {
            Some(d) => (d.veh.ox, d.veh.oy),
            None => (i64::from(self.player.x), i64::from(self.player.y)),
        };
        self.camera.centre(x, y)
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
        if self.live_city {
            self.traffic
                .set_wanted(self.score.level, self.in_car().is_some());
        }
        let car = self.in_car().map(|d| {
            let v = &d.veh;
            (v.len / 2, v.wid / 2, v.angle())
        });
        let (door, slow) = match self.in_car() {
            Some(d) => {
                let ci = &self.cars[d.veh.model];
                let dp = door_point(ci, d.veh.ox, d.veh.oy, d.veh.angle());
                let slow = self.bust_timer == 0
                    && (speed < (2 << FP) || self.traffic.pl_still >= traffic::PL_STILL_TICKS);
                (dp, slow)
            }
            None => ((x, y), false),
        };
        self.peds.set_player(x, y, layer, car, door, slow);
        self.traffic.set_player(true, x, y, speed, layer, hl);
        self.traffic.set_walkers(self.peds.walkers());
        self.traffic.set_view_blocks(self.view_now().half_w);
        let centre = self.view_centre();
        self.traffic.step(&self.map, &self.cars, centre.0, centre.1);
        for i in 0..self.traffic.n {
            let c = &self.traffic.cars[i];
            if !c.done && !c.wrecked && c.knock > 0 {
                let ci = &self.cars[c.model];
                self.marks.slide(
                    &self.map,
                    c.x,
                    c.y,
                    c.layer,
                    i32::from(ci.width) >> 1,
                    c.face,
                    c.kvx,
                    c.kvy,
                    ci.vtype == CLASS_BIKE,
                );
            }
        }
        if self.peds.stats.cops_killed != self.cops_killed_seen {
            self.cops_killed_seen = self.peds.stats.cops_killed;
            self.traffic.cops_give_up();
        }
        if let Some((cx, cy, cl, ca)) = self.traffic.take_cop_out() {
            let max_out = (self.score.level as usize).max(1);
            if self.peds.cops_out() < max_out && self.peds.spawn_cop(cx, cy, cl, ca) {
                let ped = (self.peds.last_slot().unwrap_or(0) % 3) as u8;
                self.audio.push(crate::audio::AudioEvent::CopShout {
                    kind: 0x12,
                    ped,
                    x: cx as i32,
                    y: cy as i32,
                    layer: cl,
                });
                self.say(format!(
                    "police: cop out on foot at ({},{}) layer {cl}",
                    cx >> 21,
                    cy >> 21
                ));
            }
        }
        if let Some((yx, yy, yl)) = self.traffic.yell.take() {
            self.audio.push(crate::audio::AudioEvent::StreetYell {
                x: yx as i32,
                y: yy as i32,
                layer: yl,
                chance_half: false,
            });
        }
        while let Some((cx, cy, cl, ca)) = self.traffic.take_roadblock_cop() {
            if self.peds.spawn_cop(cx, cy, cl, ca) {
                self.peds.post_last_cop();
            }
        }
        if self.peds.cop_event()
            && self.bust_timer == 0
            && self.foot_arrest == 0
            && self.score.level > 0
            && self.peds.cop_shoot == 0
        {
            if self.in_car().is_some() {
                self.busted();
            } else {
                self.foot_arrest = FOOT_ARREST_TICKS;
            }
        }
        if self.foot_arrest > 0 {
            self.foot_arrest -= 1;
            if self.foot_arrest == 0 {
                self.busted();
            }
        }
        if self.bust_timer > 0 {
            self.bust_timer -= 1;
            if self.bust_timer == 0 {
                self.finish_card();
            }
        }
        centre
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
        let view = self.view_now();
        self.peds.set_view(view.half_w, view.half_h, angle, moving);
    }

    /// The peds' tick, with the view where the fleet's was.
    fn run_peds(&mut self, centre: (i64, i64)) {
        let traffic = &self.traffic;
        let green = |along_x| traffic.light_green(along_x);
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
        // Those his blast set alight, burnt to death: paid now, as burnt.
        for kind in self.peds.take_owed() {
            let award = self.score.event(kind, score::reason::BURNED);
            self.crime(crime::MURDER);
            self.say(format!(
                "score: burnt to death, {award} points (score {})",
                self.score.score
            ));
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
        let speed = if avx > avy {
            avx + avy / 2
        } else {
            avy + avx / 2
        };
        let kinds = self
            .peds
            .ram(v.ox, v.oy, v.angle(), v.len / 2, v.wid / 2, d.layer, speed);
        if !kinds.is_empty() {
            // A run-over is a "hit and run"; from a motorbike the original
            // files nothing.
            let bike = self.cars[v.model].vtype == CLASS_BIKE;
            let mut award = 0;
            for &kind in &kinds {
                award = self.score.event(kind, score::reason::RUNOVER);
                if !bike {
                    self.crime(crime::RUNOVER);
                }
            }
            let (hits, total) = (kinds.len(), self.peds.stats.runover);
            self.say(format!(
                "car: ran over {hits} - {total} so far, {award} points (score {}, heat {})",
                self.score.score, self.score.heat
            ));
        }
    }

    /// GOURANGA!: the bonus the score paid for the 7th Krishna in a streak,
    /// whatever killed him. The big text and its sound are M8's.
    fn gouranga(&mut self) {
        let bonus = self.score.take_gouranga();
        if bonus > 0 {
            self.say(format!(
                "score: GOURANGA! +{bonus} points (score {})",
                self.score.score
            ));
            self.hud_big(crate::hud::HudText::key("8787"));
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
        if self.knocked > 0 {
            // Flat on his back: no walking, no gun, until he gets up.
            self.knocked -= 1;
            self.player.anim = Anim::SlideUnder;
            self.player.frame = 0;
            return;
        }
        if let Some(w) = self.arms.step_endless() {
            self.say(format!("player: endless weapon ended -> weapon {w}"));
        }
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
        if self.arms.punch_left > 0 && !b.contains(Buttons::UP) && !b.contains(Buttons::DOWN) {
            // The standing punch is a state of its own; he may still turn.
            let p = &mut self.player;
            p.anim = Anim::Punch;
            let thrown = weapons::PUNCH_TICKS * weapons::PUNCH_STATES - self.arms.punch_left;
            p.frame = (thrown / weapons::PUNCH_TICKS) as u8;
            p.angle = (p.angle + stick.turn * 5) & 255;
            return;
        }
        self.player.update(&self.map, stick);
        self.car_block(ox, oy);
        if self.player_speed > 0 {
            self.player_speed -= 1;
            let (ox2, oy2) = (self.player.x, self.player.y);
            self.player.update(
                &self.map,
                Stick {
                    turn: 0,
                    forward: stick.forward,
                    walk: stick.walk,
                },
            );
            self.car_block(ox2, oy2);
        }
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
        self.marks.splash(i64::from(ax), i64::from(ay), z);
        self.say(format!("player: in the water at ({bx},{by})"));
    }

    /// The layer of the water at or under layer `z` with only air between.
    fn water_below(&self, bx: i32, by: i32, z: i32) -> Option<i32> {
        let z = (0..=z)
            .rev()
            .find(|&z| self.map.ground_type(bx, by, z) != ground::AIR)?;
        (self.map.ground_type(bx, by, z) == ground::WATER).then_some(z)
    }

    /// What hurts him this tick: bullets take 10 each, or a point of armour;
    /// a bullet into his car 5 of its bodywork; a blast's inner ring kills
    /// him; fire sets him alight for 100 ticks of a point each; the water
    /// takes 3 every other tick (a second and a half from full). At none he
    /// is wasted.
    fn harm(&mut self) {
        let hurt = self.weapons.take_hurt();
        let exec = self.peds.cop_execute();
        if self.bust_timer > 0 {
            return;
        }
        if exec {
            self.health = 0;
            self.say("police: executed at close range".into());
        }
        for _ in 0..hurt.bullets {
            if self.armour > 0 {
                self.armour -= 1;
            } else {
                self.health -= 10;
            }
        }
        if hurt.car_hits > 0
            && self.in_car().is_some()
            && let Some(d) = self.driven.as_mut()
        {
            d.veh.damage += 5 * hurt.car_hits;
        }
        // In the tank a blast does him no harm: the cars it crushes blow
        // up against it and he drives on (`GTA8.EXE`, the owner's play).
        // Ours: any blast, a rocket's too, which nobody has fired at a
        // tank with him in it.
        if hurt.blast > 0 && !self.driving_tank() {
            self.health = 0;
        }
        if hurt.burn > 0 && self.burning < BURN_TICKS {
            self.burning = BURN_TICKS;
        }
        if self.burning > 0 {
            self.burning -= 1;
            self.health -= 1;
        }
        if hurt.bullets > 0 || hurt.car_hits > 0 || hurt.blast > 0 {
            self.say(format!(
                "player: hit - bullets {}, on the car {}, blast {}; health {} armour {}",
                hurt.bullets, hurt.car_hits, hurt.blast, self.health, self.armour
            ));
        }
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

    /// Wasted: his guns and the wanted level go; the score and the
    /// multiplier stay. In live city play the WASTED card runs for
    /// [`BUST_TICKS`] before respawning at the nearest hospital (or
    /// ending in GAME OVER when out of lives); in oracle harness mode
    /// (`!live_city`) he respawns immediately at `start`.
    fn wasted(&mut self) {
        if self.bust_timer > 0 {
            return;
        }
        let how = match self.doing {
            Doing::Drowning(_) => " - drowned",
            _ if self.burning > 0 => " - burnt",
            _ => "",
        };
        self.say(format!("player: wasted{how}"));
        self.emit_voice(crate::audio::voice::BUSTED_WASTED, None, true);
        self.burning = 0;
        self.knocked = 0;
        self.player_speed = 0;
        self.foot_arrest = 0;
        self.arms.disarm();
        if let Some(l) = self.score.clear_heat() {
            self.say(l);
        }
        let has_hospitals = self.map.cmp().locations[1]
            .iter()
            .any(|l| l.x != 0 || l.y != 0);
        if !self.live_city || !has_hospitals {
            self.driven = None;
            let start = self.start;
            let (mut player, _) = Player::place(&self.map, start.bx, start.by);
            if let Some(a) = start.angle {
                player.angle = a & 255;
            }
            self.player = player;
            self.health = FULL_HEALTH;
            self.doing = Doing::OnFoot;
            return;
        }
        self.traffic.police_reset();
        self.peds.clear_cops();
        self.bust_timer = BUST_TICKS;
        self.card = Card::Wasted;
    }

    /// Busted: his wanted level clears, the BUSTED card runs for
    /// [`BUST_TICKS`], and unless he holds a Get Out of Jail Free key
    /// he loses his weapons and armour and his multiplier is halved.
    pub fn busted(&mut self) {
        if self.bust_timer > 0 {
            return;
        }
        self.foot_arrest = 0;
        self.bust_timer = BUST_TICKS;
        self.card = Card::Busted;
        if self.doing == Doing::Driving {
            let face = self.driven.as_ref().map_or(0, |d| d.veh.angle());
            self.park_driven(face);
            self.doing = Doing::OnFoot;
        }
        self.burning = 0;
        self.player_speed = 0;
        if !self.jail_free {
            self.arms.disarm();
            self.armour = 0;
            self.score.multiplier = (self.score.multiplier / 2).max(1);
        }
        if let Some(l) = self.score.clear_heat() {
            self.say(l);
        }
        self.traffic.police_reset();
        self.peds.clear_cops();
        self.say(format!(
            "police: BUSTED (multiplier {}, jail_free {})",
            self.score.multiplier, self.jail_free
        ));
        self.emit_hud(crate::hud::HudEvent::BustCrimes {
            crimes: self.score.crimes,
        });
        self.emit_voice(crate::audio::voice::BUSTED_WASTED, None, true);
    }

    fn finish_card(&mut self) {
        let was = self.card;
        self.card = Card::None;
        if self.doing == Doing::Driving {
            let face = self.driven.as_ref().map_or(0, |d| d.veh.angle());
            self.park_driven(face);
        }
        self.driven = None;
        self.doing = Doing::OnFoot;
        self.burning = 0;
        self.health = FULL_HEALTH;
        if was == Card::Wasted {
            self.armour = 0;
            if self.lives > 0 {
                self.lives -= 1;
            }
            if self.lives == 0 {
                self.game_over = true;
                self.card = Card::GameOver;
                self.say("player: GAME OVER".into());
                return;
            }
        }
        if was == Card::Busted {
            self.jail_free = false;
        }
        self.score.new_life();

        let (pbx, pby) = self.player.block();
        let locs = if was == Card::Busted {
            &self.map.cmp().locations[0]
        } else {
            &self.map.cmp().locations[1]
        };
        let mut best_base = None;
        let mut best_d = i32::MAX;
        for loc in locs {
            if loc.x == 0 && loc.y == 0 {
                continue;
            }
            let d = (i32::from(loc.x) - pbx).abs() + (i32::from(loc.y) - pby).abs();
            if d < best_d {
                best_d = d;
                best_base = Some((i32::from(loc.x), i32::from(loc.y)));
            }
        }
        let (rx, ry) = if let Some((hx, hy)) = best_base {
            let mut pick = (hx, hy);
            let mut best_score = -1;
            for r in 0..=6i32 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dy.abs() != r {
                            continue;
                        }
                        let (bx, by) = (hx + dx, hy + dy);
                        if !(1..255).contains(&bx) || !(1..255).contains(&by) {
                            continue;
                        }
                        for z in 0..6 {
                            let g = self.map.nav_ground(bx, by, z);
                            let sc = if g == ground::PAVEMENT {
                                2
                            } else if g == ground::ROAD || g == ground::FIELD {
                                1
                            } else {
                                -1
                            };
                            if sc > best_score {
                                best_score = sc;
                                pick = (bx, by);
                                if sc == 2 {
                                    break;
                                }
                            }
                        }
                        if best_score == 2 {
                            break;
                        }
                    }
                    if best_score == 2 {
                        break;
                    }
                }
                if best_score == 2 {
                    break;
                }
            }
            pick
        } else {
            (self.start.bx, self.start.by)
        };
        let (mut player, _) = Player::place(&self.map, rx, ry);
        player.angle = self.start.angle.unwrap_or(128) & 255;
        self.player = player;
        self.camera = SimCamera::REST;
        let where_str = if was == Card::Busted {
            "police station"
        } else {
            "hospital"
        };
        self.say(format!(
            "player: respawned at {where_str} ({rx},{ry}) layer {} (lives {})",
            self.player.layer, self.lives
        ));
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
            self.doing = Doing::Sliding(Slide {
                steps: 0,
                entered: false,
            });
            self.say(format!("player: jump - sliding under car {i}"));
        }
    }

    /// Would stepping to `(nx, ny)` cross a fence or place the player's body
    /// inside a building wall?
    fn vault_blocked(&self, x: i32, y: i32, nx: i32, ny: i32, layer: i32) -> bool {
        if self.map.wall_cross(layer, x, y, nx, ny) {
            return true;
        }
        let r = BODY_R;
        [
            (nx - r, ny - r),
            (nx + r, ny - r),
            (nx - r, ny + r),
            (nx + r, ny + r),
        ]
        .into_iter()
        .any(|(cx, cy)| {
            self.map
                .nav_ground(cx >> (FP + BLOCK_SHIFT), cy >> (FP + BLOCK_SHIFT), layer)
                == ground::BUILDING
        })
    }

    /// Is the car's door blocked by another vehicle, fence, or building wall?
    #[allow(clippy::too_many_arguments)]
    fn door_blocked(
        &self,
        ci: &CarInfo,
        cx: i64,
        cy: i64,
        face: i32,
        layer: i32,
        skip: Option<usize>,
        check_door_point: bool,
    ) -> Option<&'static str> {
        let outside = door_pos(ci, cx, cy, face, 0, 2);
        if self
            .traffic
            .car_at_except(&self.cars, outside.0, outside.1, layer, 2, skip)
            .is_some()
        {
            return Some("a car");
        }
        let wall_at = |pt: (i64, i64)| {
            self.map
                .nav_ground((pt.0 >> 21) as i32, (pt.1 >> 21) as i32, layer)
                == ground::BUILDING
                || self
                    .map
                    .wall_cross(layer, cx as i32, cy as i32, pt.0 as i32, pt.1 as i32)
        };
        if wall_at(outside) {
            return Some("a building");
        }
        if check_door_point && wall_at(door_point(ci, cx, cy, face)) {
            return Some("a building");
        }
        None
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

    /// The free vault: over whatever car is under him, turning along the
    /// roof if a building wall blocks the far side, landing on his feet
    /// with the controls back.
    fn vaulting(&mut self, mut v: Vault) {
        let layer = self.player.layer;
        let (fx, fy) = (i64::from(sin(v.head)), -i64::from(cos(v.head)));
        let run = i64::from(RUN_SPEED);
        let nx = (i64::from(self.player.x) + ((fx * run) >> 14)) as i32;
        let ny = (i64::from(self.player.y) + ((fy * run) >> 14)) as i32;
        if self.vault_blocked(self.player.x, self.player.y, nx, ny, layer) {
            let turn = [
                (v.head - 64) & 255,
                (v.head + 64) & 255,
                (v.head + 128) & 255,
            ]
            .into_iter()
            .find(|&h| {
                let tx = (i64::from(self.player.x) + i64::from(sin(h)) * 16) as i32;
                let ty = (i64::from(self.player.y) - i64::from(cos(h)) * 16) as i32;
                !self.vault_blocked(self.player.x, self.player.y, tx, ty, layer)
            });
            if let Some(h) = turn
                && v.hold < cars::VAULT_HOLD_MAX
            {
                v.head = h;
                v.step = 3;
                v.tick = 0;
                v.hold += 1;
                self.player.angle = h;
                self.player.anim = Anim::Vault;
                self.player.frame = 3;
                self.doing = Doing::Vaulting(v);
                return;
            }
            self.stand_up();
            self.say(format!("player: landed after state {}", v.step));
            return;
        }
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

    /// Under a tall vehicle: slides forward at running pace under the chassis
    /// until clear on the other side, then stands up.
    fn sliding(&mut self, mut s: Slide) {
        self.player.anim = Anim::SlideUnder;
        self.player.frame = 0;
        let run = i64::from(RUN_SPEED);
        let (fx, fy) = (
            i64::from(sin(self.player.angle)),
            -i64::from(cos(self.player.angle)),
        );
        let nx = (i64::from(self.player.x) + ((fx * run) >> 14)) as i32;
        let ny = (i64::from(self.player.y) + ((fy * run) >> 14)) as i32;
        if self.vault_blocked(self.player.x, self.player.y, nx, ny, self.player.layer) {
            if s.entered && s.steps < cars::SLIDE_MAX_TICKS {
                self.player.angle = (self.player.angle + 128) & 255;
                self.doing = Doing::Sliding(s);
                return;
            }
            self.stand_up();
            self.say("player: out from under".into());
            return;
        }
        self.player.x = nx;
        self.player.y = ny;
        s.steps += 1;
        let p = &self.player;
        if self.car_at(p.x.into(), p.y.into(), p.layer, 0) {
            s.entered = true;
        }
        if s.steps < cars::SLIDE_MAX_TICKS
            && ((!s.entered && s.steps < 12)
                || self.car_at(p.x.into(), p.y.into(), p.layer, PED_CAR_MARGIN))
        {
            self.doing = Doing::Sliding(s);
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
    /// standing or moving, and walk to its door (unless the door is blocked).
    fn try_enter(&mut self) {
        let p = &self.player;
        let Some(slot) = self
            .traffic
            .nearest_enterable(p.x.into(), p.y.into(), p.layer, REACH)
        else {
            self.say("car: no car within reach".into());
            return;
        };
        let (model, cx, cy, face, layer) = {
            let c = &self.traffic.cars[slot];
            (c.model, c.x, c.y, c.face, c.layer)
        };
        if let Some(what) =
            self.door_blocked(&self.cars[model], cx, cy, face, layer, Some(slot), true)
        {
            self.say(format!("car: door blocked by {what} - cannot get in"));
            return;
        }
        let t = self.traffic.take_car(slot);
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
        let mut via = [(0, 0); 2];
        let mut via_n = 0u8;
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
        } else if !bike {
            let at = (i64::from(car.x), i64::from(car.y), car.face);
            if let Some((nose, nc, tail, tc)) = cars::far_side_around(ci, at, (px, py), door) {
                let clear = |pts: [(i64, i64); 2]| {
                    pts.into_iter().all(|pt| {
                        self.map
                            .nav_ground((pt.0 >> 21) as i32, (pt.1 >> 21) as i32, layer)
                            != ground::BUILDING
                    })
                };
                let chosen = match (clear(nose), clear(tail)) {
                    (true, false) => nose,
                    (false, true) => tail,
                    _ if nc <= tc => nose,
                    _ => tail,
                };
                let final_door = door;
                door = chosen[0];
                via = [chosen[1], final_door];
                via_n = 2;
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
            via,
            via_n,
            serial: t.serial,
            mission: t.mission,
            driver: t.had_driver,
            cop: t.cop,
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
            Some(i) => {
                if e.cop {
                    self.peds.make_cop(i);
                }
                // Taking a car off somebody scores; a parked one is worth
                // nothing.
                self.score.event(score::kind::CAR, 0);
                self.crime(crime::CARJACK);
                self.say(format!("car: dragged the driver out (ped {i})"));
            }
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
            let face = if e.via_n > 0 {
                let (mut sx, mut sy) = (e.door.0 - e.from.0, e.door.1 - e.from.1);
                while sx.abs() > i64::from(i32::MAX) || sy.abs() > i64::from(i32::MAX) {
                    sx >>= 1;
                    sy >>= 1;
                }
                if sx != 0 || sy != 0 {
                    step_dir(sx as i32, sy as i32)
                } else {
                    e.from.2
                }
            } else {
                e.over.map_or(e.car.face, |o| o.head)
            };
            let p = &mut self.player;
            p.anim = Anim::Walk;
            p.frame = ((e.walk_t / 3) % 8) as u8;
            p.x = lerp_fp(e.from.0, e.door.0, e.walk_t, e.walk_len) as i32;
            p.y = lerp_fp(e.from.1, e.door.1, e.walk_t, e.walk_len) as i32;
            p.angle = lerp_angle(e.from.2, face, e.walk_t, e.walk_len);
            e.walk_t += 1;
            if e.walk_t >= e.walk_len {
                if e.via_n > 0 {
                    let at = (i64::from(p.x), i64::from(p.y));
                    e.from = (at.0, at.1, p.angle);
                    e.door = e.via[0];
                    e.via[0] = e.via[1];
                    e.via_n -= 1;
                    e.walk_len = walk_ticks(at, e.door);
                    e.walk_t = 0;
                } else if let Some(o) = e.over.as_mut() {
                    o.vault = Some(Vault::new(o.head));
                }
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
        // Door opens on an occupied car: play its radio snippet (`0x7e..0x82`).
        if !e.bike && e.driver && t == 0 {
            let radio = self.cars[e.car.model].radio;
            if let Some(id) = crate::audio::snd::for_car_radio(radio) {
                self.emit_sfx(id, e.car.x, e.car.y, e.car.layer);
            }
        }
        // The door is fully open (a bike has none): out comes the driver.
        if e.driver && (e.bike || t >= DOOR_OPEN) {
            e.driver = false;
            self.throw_out(&e);
        }
        let ci = &self.cars[e.car.model];
        let (cx, cy) = (i64::from(e.car.x), i64::from(e.car.y));
        let side = cars::door_side(ci) as i32;
        if e.bike {
            let seat = door_pos(ci, cx, cy, e.car.face, 0, -i64::from(cars::half_size(ci).1));
            self.player.x = seat.0 as i32;
            self.player.y = seat.1 as i32;
            self.player.angle = e.car.face;
        } else {
            // He stands at the door until pulling himself inside laterally
            // along the door's own hinge station (`d.rpy`).
            let slide = (steps - 3) * ENTER_TICKS;
            let seat = door_pos(ci, cx, cy, e.car.face, 0, -4);
            let (px, py) = if t <= slide {
                e.door
            } else {
                (
                    lerp_fp(e.door.0, seat.0, t - slide, total - slide),
                    lerp_fp(e.door.1, seat.1, t - slide, total - slide),
                )
            };
            self.player.x = px as i32;
            self.player.y = py as i32;
            self.player.angle = if e.step < 2 {
                e.car.face
            } else {
                (e.car.face + side * 32) & 255
            };
        }
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
            siren: cars::Siren::Off,
            horn: false,
            turret: 0,
        });
        self.player.anim = Anim::Stand;
        self.player.frame = 0;
        self.doing = Doing::Driving;
        if !e.bike {
            self.emit_sfx(crate::audio::snd::DOOR, car.x, car.y, car.layer);
        }
        self.emit_sfx(crate::audio::snd::CAR_START, car.x, car.y, car.layer);
        self.emit_hud(crate::hud::HudEvent::CarName {
            model: self.cars[car.model].model,
        });
        self.say(format!(
            "car: in - model {} at ({},{})",
            self.cars[car.model].model,
            car.x >> 21,
            car.y >> 21
        ));
    }

    /// RETURN in a car: open the door, if it is slow enough and the door is
    /// clear; when blocked, climb onto the roof and vault forward along the
    /// car's heading.
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
        if let Some(what) = self.door_blocked(ci, v.ox, v.oy, a, d.layer, None, false) {
            let (ox, oy, layer) = (v.ox, v.oy, d.layer);
            let (hl, _) = cars::half_size(ci);
            let start_along = ci
                .doors
                .first()
                .map_or(0, |d| i64::from(d.rpy) / 2)
                .clamp(-i64::from(hl) / 2, i64::from(hl) / 2);
            let head = a & 255;
            self.park_driven(a);
            let p = &mut self.player;
            p.x = (ox + i64::from(sin(a)) * start_along * 4) as i32;
            p.y = (oy - i64::from(cos(a)) * start_along * 4) as i32;
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
        let side = cars::door_side(ci) as i32;
        let steps = if e.bike { BIKE_FRAMES } else { EXIT_FRAMES };
        let total = steps * EXIT_TICKS;
        let t = e.step * EXIT_TICKS + e.tick;
        let (x, y) = if e.bike {
            let start = door_pos(ci, v.ox, v.oy, e.face, 0, 1);
            let end = door_pos(ci, v.ox, v.oy, e.face, 0, 3);
            (
                lerp_fp(start.0, end.0, t, total),
                lerp_fp(start.1, end.1, t, total),
            )
        } else {
            let (along, lat) = cars::exit_offsets(e.step);
            door_pos(ci, v.ox, v.oy, e.face, along, lat)
        };
        self.player.x = x as i32;
        self.player.y = y as i32;
        self.player.angle = if e.bike {
            (e.face + side * 64) & 255
        } else {
            e.face
        };
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
        let (x, y) = if e.bike {
            door_pos(ci, v.ox, v.oy, e.face, 0, 3)
        } else {
            door_pos(ci, v.ox, v.oy, e.face, -3, 4)
        };
        self.player.x = x as i32;
        self.player.y = y as i32;
        self.player.angle = (e.face + side * if e.bike { 64 } else { 32 }) & 255;
        self.park_driven(e.face);
        if !e.bike {
            self.emit_sfx(
                crate::audio::snd::DOOR,
                self.player.x,
                self.player.y,
                self.player.layer,
            );
        }
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
        let mut sfx = Vec::new();
        let mut ram_yells = Vec::new();
        let (ox0, oy0, ang0) = (d.veh.ox, d.veh.oy, d.veh.ang16);
        let prev_damage = d.veh.damage;
        let was_air = d.air.is_some();
        let ci = &self.cars[d.veh.model];
        let water = d.tick(&self.map, ci, pedals, &mut |s| lines.push(s));
        if water.is_none() {
            if d.veh.damage > prev_damage {
                sfx.push((
                    crate::audio::snd::WALL,
                    d.veh.ox as i32,
                    d.veh.oy as i32,
                    d.layer,
                ));
            }
            if was_air && d.air.is_none() {
                sfx.push((
                    crate::audio::snd::LANDING,
                    d.veh.ox as i32,
                    d.veh.oy as i32,
                    d.layer,
                ));
                self.marks.sparks(
                    d.veh.ox,
                    d.veh.oy,
                    d.layer,
                    d.veh.len,
                    d.veh.wid,
                    d.veh.angle(),
                );
            }
            if d.veh.sliding && d.air.is_none() && ci.model != 0x25 {
                self.marks.slide(
                    &self.map,
                    d.veh.ox,
                    d.veh.oy,
                    d.layer,
                    d.veh.wid,
                    d.veh.angle(),
                    d.veh.vx,
                    d.veh.vy,
                    ci.vtype == CLASS_BIKE,
                );
            }
            // The fleet. The tank: nothing stops it, and every car it
            // touches is written off (`Traffic::crush`). Any other car:
            // stopped at the contact rather than inside a car, then the
            // ram (AmiGTA1's main loop).
            let v = &mut d.veh;
            if ci.model == crate::vehicle::TANK_MODEL {
                let cops_before = self.traffic.rammed_cops;
                let n = self.traffic.crush(
                    &self.cars,
                    v.ox,
                    v.oy,
                    v.angle(),
                    v.len / 2,
                    v.wid / 2,
                    d.layer,
                );
                if self.traffic.rammed_cops != cops_before
                    && let Some(l) = self.score.force_level(1)
                {
                    lines.push(l);
                }
                if n > 0 {
                    sfx.push((
                        crate::audio::snd::CRASH,
                        v.ox as i32,
                        v.oy as i32,
                        d.layer,
                    ));
                    lines.push(format!("car: the tank crushed {n}"));
                }
            } else {
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
                let (hvx, hvy) = (v.vx, v.vy);
                let cops_before = self.traffic.rammed_cops;
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
                // Touching a police car is a wanted level at once; a shunt at
                // speed (five units on either axis, as a run-over needs) is
                // two heat a car.
                if self.traffic.rammed_cops != cops_before
                    && let Some(l) = self.score.force_level(1)
                {
                    lines.push(l);
                }
                if r.hits > 0
                    && (hvx.abs() >= crate::peds::RUN_OVER_SPEED
                        || hvy.abs() >= crate::peds::RUN_OVER_SPEED)
                {
                    for _ in 0..r.hits {
                        if let Some(l) = self.score.crime(crime::SHUNT) {
                            lines.push(l);
                        }
                    }
                }
                if r.hits > 0 {
                    let sp = (hvx.abs().max(hvy.abs()) >> 15) as i32;
                    let impact = if sp < 7 {
                        crate::audio::snd::IMPACT_LIGHT
                    } else if sp < 17 {
                        crate::audio::snd::IMPACT_MED
                    } else {
                        crate::audio::snd::IMPACT_HARD
                    };
                    sfx.push((impact, v.ox as i32, v.oy as i32, d.layer));
                    if sp > 10 {
                        ram_yells.push((v.ox as i32, v.oy as i32, d.layer));
                    }
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
        for (id, x, y, layer) in sfx {
            self.emit_sfx(id, x, y, layer);
        }
        for (x, y, layer) in ram_yells {
            self.audio.push(crate::audio::AudioEvent::StreetYell {
                x,
                y,
                layer,
                chance_half: true,
            });
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
        self.marks.splash(d.veh.ox, d.veh.oy, z);
        self.emit_sfx(crate::audio::snd::SPLASH, d.veh.ox as i32, d.veh.oy as i32, z);
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
        let bombed = v.bombed;
        let (x, y, face, layer) = (v.ox, v.oy, v.angle(), d.layer);
        let (model, remap) = (v.model, i32::from(v.remap.unwrap_or(0)) - 1);
        self.driven = None;
        // A Q14 unit times 48 is 12 px.
        self.player.x = (x + i64::from(cos(face)) * 48) as i32;
        self.player.y = (y + i64::from(sin(face)) * 48) as i32;
        self.player.angle = (face + 64) & 255;
        self.player.layer = layer;
        self.player.anim = Anim::Stand;
        self.player.frame = 0;
        self.doing = Doing::OnFoot;
        // The blast finds him where he last was: still in it, so a driver
        // who stays in a burning car dies with it.
        let cars = std::mem::take(&mut self.cars);
        self.street(|w, st| {
            if bombed {
                w.bomb_blasts(st, &cars[model], x, y, face, layer, true);
            } else {
                w.wreck_car(st, &cars[model], x, y, face, layer, true);
            }
        });
        self.cars = cars;
        self.traffic
            .leave_wreck(&self.cars, model, x, y, face, layer, remap);
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
            let door = if e.bike || e.walk_t < e.walk_len || e.via_n > 0 || e.over.is_some() {
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
                cars::exit_door_delta(e.step, e.tick)
            };
        }
        // The siren's lights, turn about.
        let lights = if d.siren == cars::Siren::Off {
            0
        } else {
            1 << (cars::DELTA_LIGHT0 + (self.tick / cars::LIGHT_FLASH_TICKS) % 2)
        };
        Some((
            d.veh.dmg_bits | lights | door.map_or(0, |d| 1 << d),
            d.veh.fuse,
        ))
    }

    /// True when the player should be drawn *before* vehicles (so a car and
    /// its open door overlap him during car enter/exit, or hide him while
    /// sliding underneath). Vaulting and bike mount/dismount stay on top.
    pub fn player_under_car(&self) -> bool {
        match self.doing {
            Doing::Entering(e) => {
                !e.bike && e.over.is_none() && e.via_n == 0 && e.walk_t >= e.walk_len
            }
            Doing::Exiting(e) => !e.bike,
            Doing::Sliding(_) => true,
            _ => false,
        }
    }

    /// The ped sprite frame to draw the player with, or `None` while he is
    /// hidden inside a car.
    /// On his feet: punching on the run is the run frame moved on; with a
    /// gun out and the fire key held, a walker or runner is the cycle's
    /// frame moved on and a man standing has a pose of his own. Only the
    /// sprite changes.
    fn armed_frame(&self) -> u16 {
        use crate::player::frames;
        let p = &self.player;
        let f = p.ped_frame();
        let moving = matches!(p.anim, Anim::Walk | Anim::Run);
        if self.arms.punch_left > 0 && p.anim == Anim::Run {
            return f + frames::RUN_PUNCH_OFFSET;
        }
        if !self.arms.armed() {
            return f;
        }
        match p.anim {
            _ if moving => f + frames::ARMED_OFFSET,
            Anim::Stand => frames::ARMED_STAND,
            _ => f,
        }
    }

    pub fn player_frame(&self) -> Option<u16> {
        if self.foot_arrest > 0 {
            return Some(peds::frame::LIES);
        }
        match self.doing {
            // Aboard a train he is out of sight, and in the water he has
            // gone under.
            Doing::Riding(_) | Doing::Drowning(_) => None,
            Doing::OnFoot => Some(self.armed_frame()),
            Doing::Entering(e) if e.walk_t < e.walk_len || e.via_n > 0 || e.over.is_some() => {
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
            siren: cars::Siren::Off,
            horn: false,
            turret: 0,
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

    /// A test fixture: a ped `(dx, dy)` world px from the player, facing
    /// `angle` (AmiGTA1's `ped` order). The city keeps the pool full, so a
    /// full pool gives up the ped farthest from the player first.
    pub fn drop_ped(&mut self, dx: i32, dy: i32, angle: i32) {
        let (px, py) = (i64::from(self.player.x), i64::from(self.player.y));
        if self.peds.p.iter().all(|p| p.alive) {
            let far = self
                .peds
                .p
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let (dx, dy) = ((p.x - px) >> 16, (p.y - py) >> 16);
                    (dx * dx + dy * dy, i)
                })
                // The first of equals, as AmiGTA1's strict `>` keeps.
                .fold(None, |best: Option<(i64, usize)>, (d, i)| match best {
                    Some((bd, _)) if bd >= d => best,
                    _ => Some((d, i)),
                });
            if let Some((_, i)) = far {
                self.peds.p[i].alive = false;
            }
        }
        let (x, y) = (px + (i64::from(dx) << 16), py + (i64::from(dy) << 16));
        let layer = self.player.layer;
        if self.peds.drop_ped(x, y, layer, angle & 255, -1, 0) {
            self.say(format!(
                "ped: dropped a ped at ({},{})",
                (px >> 16) + i64::from(dx),
                (py >> 16) + i64::from(dy)
            ));
        } else {
            self.say("ped: drop - pool full".into());
        }
    }

    /// Add a line to the run log.
    /// Hand him weapon `w` with `rounds` units (the scripts' `weapon`).
    pub fn give_weapon(&mut self, w: i32, rounds: i32) {
        if let Ok(w) = usize::try_from(w) {
            self.arms.give(w, rounds);
        }
    }

    /// Take `points` of health (the scripts' `hurt`).
    pub fn hurt(&mut self, points: i32) {
        self.health -= points;
    }

    /// Shoot the living ped nearest him, within `range` px measured along
    /// both axes, along his facing (the scripts' `shootped`).
    pub fn shoot_ped(&mut self, range: i32) {
        let (px, py) = (i64::from(self.player.x), i64::from(self.player.y));
        let mut best = (None, i64::from(range));
        for (i, p) in self.peds.p.iter().enumerate() {
            let d = ((p.x - px) >> FP).abs() + ((p.y - py) >> FP).abs();
            if p.alive && !p.corpse && p.pull < 0 && d < best.1 {
                best = (Some(i), d);
            }
        }
        if let Some(i) = best.0 {
            self.peds.shoot(i, self.player.angle);
        }
        self.say(format!(
            "script: shootped - ped {}, {} px away",
            best.0.map_or(-1, |i| i as i64),
            best.1
        ));
    }

    fn pickups_tick(&mut self) {
        if self.bust_timer > 0 || matches!(self.doing, Doing::Drowning(_)) {
            return;
        }
        let (px, py, pl, r) = match self.in_car() {
            Some(d) => (d.veh.ox, d.veh.oy, d.layer, 12),
            None => (
                i64::from(self.player.x),
                i64::from(self.player.y),
                self.player.layer,
                8,
            ),
        };
        if self.in_car().is_some() {
            let mut lines = Vec::new();
            self.pickups.open_at(px, py, pl, r, &mut |s| lines.push(s));
            for l in lines {
                self.say(l);
            }
        }
        let mut scan = 0usize;
        while let Some((idx, kind, amount)) = self.pickups.peek_from(px, py, pl, r, scan) {
            scan = idx + 1;
            let want = match kind {
                1..=4 => {
                    let w = kind as usize;
                    self.arms.ammo[w] < 99 && (self.arms.inf_ticks <= 0 || self.arms.inf_w != w)
                }
                6..=8 => self.player_speed <= 0,
                pickups::kind::BRIBE => self.score.level > 0,
                pickups::kind::ARMOUR => self.armour < 10,
                pickups::kind::MULTIPLIER => self.score.multiplier < 99,
                pickups::kind::JAILFREE => !self.jail_free,
                pickups::kind::LIFE | 15 => self.lives < 99,
                pickups::kind::FRENZY => true,
                _ => true,
            };
            if !want {
                continue;
            }
            self.pickups.take_index(idx);
            let pickup_snd = if (1..=4).contains(&kind) {
                crate::audio::snd::PICKUP_GUN
            } else if (kind & 1) != 0 {
                crate::audio::snd::PICKUP_ODD
            } else {
                crate::audio::snd::PICKUP_EVEN
            };
            self.emit_sfx(pickup_snd, px as i32, py as i32, pl);
            match kind {
                1..=4 => {
                    let w = kind as usize;
                    let add = if amount > 0 {
                        amount
                    } else {
                        weapons::CRATE_AMMO[w]
                    };
                    self.arms.ammo[w] = (self.arms.ammo[w] + add).min(99);
                    if self.arms.weapon == weapons::weapon::FIST
                        || self.arms.ammo[self.arms.weapon] <= 0
                    {
                        self.arms.weapon = w;
                    }
                    self.say(format!(
                        "pickups: took {} (+{add}, ammo {})",
                        pickups::kind_name(kind),
                        self.arms.ammo[w]
                    ));
                }
                6..=8 => {
                    self.player_speed = SPEED_TICKS;
                    self.say(format!("pickups: took speed ({SPEED_TICKS} ticks)"));
                }
                pickups::kind::BRIBE => {
                    if let Some(l) = self.score.clear_heat() {
                        self.say(l);
                    }
                    self.traffic.police_reset();
                    self.peds.clear_cops();
                    self.say("pickups: took police bribe - wanted level cleared".into());
                }
                pickups::kind::ARMOUR => {
                    let add = if amount > 0 { amount } else { 3 };
                    self.armour = (self.armour + add).min(10);
                    self.say(format!("pickups: took armour (+{add} -> {})", self.armour));
                }
                pickups::kind::MULTIPLIER => {
                    self.score.multiplier = (self.score.multiplier + 1).min(99);
                    self.say(format!(
                        "pickups: took multiplier (+1 -> x{})",
                        self.score.multiplier
                    ));
                }
                pickups::kind::JAILFREE => {
                    self.jail_free = true;
                    self.say("pickups: took get out of jail free key".into());
                }
                pickups::kind::LIFE | 15 => {
                    let add = if amount > 0 { amount } else { 1 };
                    self.lives = (self.lives + add).min(99);
                    self.emit_voice(crate::audio::voice::EXTRA_LIFE, None, true);
                    self.say(format!(
                        "pickups: took extra life (+{add} -> {})",
                        self.lives
                    ));
                }
                pickups::kind::FRENZY => {
                    self.arms.give_endless(weapons::weapon::MACHINE_GUN, 1500);
                    self.cannon.load();
                    self.emit_voice(crate::audio::voice::FRENZY, None, true);
                    self.say("pickups: took kill frenzy".into());
                }
                _ => {}
            }
        }
    }

    fn shops_tick(&mut self) {
        self.shop_half = !self.shop_half;
        if self.shop_half {
            return;
        }
        if self.doing != Doing::Driving {
            for sp in &mut self.sprays {
                sp.used = false;
            }
            for bs in &mut self.bombshops {
                bs.used = false;
            }
            return;
        }
        let Some(d) = self.driven.as_mut() else {
            return;
        };
        let cbx = (d.veh.ox >> 21) as i32;
        let cby = (d.veh.oy >> 21) as i32;
        let mut sprayed = None;
        let mut spray_refusal = None;
        for sp in &mut self.sprays {
            let inside = sp.bx == cbx && sp.by == cby;
            if !inside {
                sp.used = false;
                continue;
            }
            if sp.used {
                continue;
            }
            sp.used = true;
            let ci = &self.cars[d.veh.model];
            if emergency_vehicle(ci.model) {
                continue;
            }
            // The colour branch requires a mission-linked car (car+0x12b).
            // The plate-only path still works on unlinked vehicles.
            if d.mission && ci.model == 47 {
                spray_refusal = Some("no_modelcar_respray");
                break;
            }
            let paint = match u8::try_from(sp.color) {
                Ok(0) => Some(None),
                Ok(color)
                    if ci
                        .remap8
                        .get(usize::from(color - 1))
                        .is_some_and(|&r| r != 0) =>
                {
                    Some(Some(color))
                }
                _ => None,
            };
            let repaint = paint.filter(|&remap| d.mission && remap != d.veh.remap);
            if repaint.is_none() && self.score.level == 0 {
                continue;
            }
            let cost = spray_cost(ci, d.veh.damage, self.score.level, self.score.multiplier);
            if self.score.score < cost {
                spray_refusal = Some("no_respray");
                break;
            }
            self.score.score -= cost;
            d.veh.damage = 0;
            d.veh.dmg_bits = 0;
            d.veh.fuse = 0;
            if let Some(remap) = repaint {
                d.veh.remap = remap;
            }
            let key = if repaint.is_some() {
                "respray_done"
            } else {
                "clean_done"
            };
            sprayed = Some((sp.bx, sp.by, d.veh.remap, key, cost));
            break;
        }
        if let Some(key) = spray_refusal {
            self.say(format!("shop: {key}"));
            self.hud_pager(crate::hud::HudText::key(key), None);
        }
        if let Some((bx, by, remap, key, cost)) = sprayed {
            self.emit_sfx(
                crate::audio::snd::RESPRAY,
                (bx << 21) + (1 << 20),
                (by << 21) + (1 << 20),
                self.player.layer,
            );
            if let Some(l) = self.score.clear_heat() {
                self.say(l);
            }
            self.traffic.police_reset();
            self.peds.clear_cops();
            self.say(format!(
                "shop: {key} at ({bx},{by}) remap {remap:?} for {cost} points"
            ));
            self.hud_pager(crate::hud::HudText::key(key), None);
        }

        let Some(d) = self.driven.as_mut() else {
            return;
        };
        let mut rigged = None;
        let mut refused = None;
        for bs in &mut self.bombshops {
            let inside = bs.bx == cbx && bs.by == cby;
            if !inside {
                bs.used = false;
                continue;
            }
            if bs.used {
                continue;
            }
            bs.used = true;
            refused = match self.cars[d.veh.model].model {
                37 => Some("no_tank_respray"),
                47 => Some("no_modelcar_respray"),
                model if emergency_vehicle(model) && !matches!(model, 4 | 32) => {
                    Some("no_emerg_respray")
                }
                _ => None,
            };
            if refused.is_some() {
                break;
            }
            if d.serial != 0
                && self
                    .bombs
                    .iter()
                    .any(|bomb| bomb.serial == d.serial && bomb.kind != 0)
            {
                break;
            }
            let cost = i64::from(self.bomb_cost.max(0));
            if self.score.score < cost {
                refused = Some("no_respray");
                break;
            }
            if d.serial == 0 {
                d.serial = 0x7000_0000 + u64::from(self.tick) + 1;
            }
            self.score.score -= cost;
            d.veh.damage = 0;
            rigged = Some((bs.bx, bs.by, d.serial, cost));
            break;
        }
        if let Some(key) = refused {
            self.say(format!("shop: {key}"));
            self.hud_pager(crate::hud::HudText::key(key), None);
        }
        if let Some((bx, by, ser, cost)) = rigged {
            let mut slot = 0;
            for (i, bm) in self.bombs.iter().enumerate() {
                if bm.kind == 0 || bm.serial == ser {
                    slot = i;
                    break;
                }
            }
            self.bombs[slot] = CarBomb {
                serial: ser,
                kind: 1,
                fuse: 0,
            };
            self.say(format!(
                "shop: bomb fitted to car #{ser} at ({bx},{by}) for {cost} points"
            ));
            self.hud_pager(crate::hud::HudText::key("bomb_on"), None);
        }
    }

    fn bombs_tick(&mut self) {
        let mut blow = Vec::new();
        for i in 0..self.bombs.len() {
            let mut b = self.bombs[i];
            if b.kind == 0 || b.serial == 0 {
                continue;
            }
            let in_this = self.doing == Doing::Driving
                && self.driven.as_ref().is_some_and(|d| d.serial == b.serial);
            if b.kind == 2 {
                if b.fuse > 0 {
                    b.fuse -= 1;
                }
                if b.fuse <= 0 {
                    b.kind = 0;
                    blow.push((b.serial, in_this, true, true));
                }
            } else if b.kind == 3 && in_this {
                b.kind = 0;
                blow.push((b.serial, true, false, true));
            } else if b.kind == 4 {
                let damaged = if in_this {
                    self.driven
                        .as_ref()
                        .is_some_and(|d| d.veh.damage > BOMB4_DAMAGE)
                } else {
                    self.traffic
                        .cars
                        .iter()
                        .take(self.traffic.n)
                        .any(|c| !c.done && c.serial == b.serial && c.damage > BOMB4_DAMAGE)
                };
                if damaged {
                    b.kind = 0;
                    blow.push((b.serial, in_this, true, false));
                }
            } else if (b.kind == 5 || b.kind == 6) && in_this {
                let spd = self
                    .driven
                    .as_ref()
                    .map_or(0, |d| d.veh.vx.abs() + d.veh.vy.abs());
                if b.kind == 5 {
                    if spd > (7 << FP) {
                        b.kind = 6;
                    }
                } else if spd < (3 << FP) {
                    b.kind = 0;
                    blow.push((b.serial, true, true, true));
                }
            }
            self.bombs[i] = b;
        }
        for (ser, in_this, five, own) in blow {
            if own && !in_this {
                self.traffic.mark_player_kill(ser);
            }
            if in_this {
                if let Some(d) = self.driven.as_mut() {
                    d.veh.damage = cars::WRECKED;
                    d.veh.fuse = 1;
                    d.veh.bombed = five;
                }
                self.say(format!("car: bomb detonated in driven car #{ser}"));
            } else if if five {
                self.traffic.bomb_off(ser)
            } else {
                self.traffic.write_off(ser)
            } {
                self.say(format!("car: bomb detonated in fleet car #{ser}"));
            }
        }
    }

    fn explode_proc_tick(&mut self) {
        let Some((idx, wait)) = self.explode_proc else {
            return;
        };
        if wait > 0 {
            self.explode_proc = Some((idx, wait - 1));
            return;
        }
        let Some(sec) = &self.section else {
            self.explode_proc = None;
            return;
        };
        let Some(cmd) = sec.cmds.get(idx).cloned() else {
            self.explode_proc = None;
            return;
        };
        let (target_id, face) = match cmd.name.as_str() {
            "PLAIN_EXPL_BUILDING" => (cmd.p(1), cmd.p(4)),
            "EXPLODE" => (cmd.p(1), 0),
            _ => {
                self.explode_proc = None;
                return;
            }
        };
        let decl = sec.decls.iter().find(|d| d.id == target_id).map(|d| d.pos);
        if let Some([bx, by, bz]) = decl {
            let z = if bz > 0 {
                bz
            } else {
                self.map.stand_layer(bx, by).unwrap_or(self.player.layer)
            };
            self.street(|w, st| {
                w.explode_face(st, bx, by, z, face, false);
            });
            self.marks.debris(bx, by, z, face);
            self.say(format!(
                "script: exploded building decl {target_id} at ({bx},{by},{z}) face {face}"
            ));
        }
        self.explode_proc = Some((idx + 1, 1));
    }

    /// A test fixture: place a crate `(dx, dy)` blocks from the player.
    pub fn drop_crate(&mut self, dx: i32, dy: i32, kind: i32, amount: i32) {
        let (pbx, pby) = self.player.block();
        let (bx, by) = (pbx + dx, pby + dy);
        if self.pickups.add_block(&self.map, bx, by, kind, amount) {
            self.say(format!(
                "pickups: placed crate at ({bx},{by}) kind {kind} ({}) amount {amount}",
                pickups::kind_name(kind)
            ));
        } else {
            self.say(format!("pickups: failed to place crate at ({bx},{by})"));
        }
    }

    /// A test fixture: place a patrol police car `(dx, dy)` blocks from the player.
    pub fn drop_copcar(&mut self, dx: i32, dy: i32, face: i32) {
        self.live_city = true;
        let (pbx, pby) = self.player.block();
        let (bx, by) = (pbx + dx, pby + dy);
        let z = self.map.stand_layer(bx, by).unwrap_or(self.player.layer);
        if let Some(slot) = self
            .traffic
            .cop_place(&self.map, &self.cars, bx, by, z, face & 255, 1)
        {
            self.say(format!(
                "police: placed patrol car at ({bx},{by}) layer {z} slot {slot}"
            ));
        } else {
            self.say(format!("police: could not place patrol car at ({bx},{by})"));
        }
    }

    /// A test fixture: place a mature blood pool `(dx, dy)` px from the player.
    pub fn drop_pool(&mut self, dx: i32, dy: i32) {
        let x = i64::from(self.player.x) + (i64::from(dx) << FP);
        let y = i64::from(self.player.y) + (i64::from(dy) << FP);
        let z = self.player.layer;
        self.marks.pool(x, y, z);
        for s in &mut self.marks.stains {
            if s.left > crate::marks::POOL_SPLAT_AT {
                s.left = crate::marks::POOL_SPLAT_AT;
            }
        }
        self.say(format!(
            "marks: placed blood pool at ({},{}) layer {z}",
            x >> FP,
            y >> FP
        ));
    }

    /// A test fixture: spawn and kill a ped at mission declaration `line` (or near the player).
    pub fn kill_ped_decl(&mut self, line: i32) {
        self.live_city = true;
        let (bx, by) = self
            .section
            .as_ref()
            .and_then(|s| s.decls.iter().find(|d| d.id == line))
            .map(|d| (d.pos[0], d.pos[1]))
            .unwrap_or_else(|| self.player.block());
        let z = self.map.stand_layer(bx, by).unwrap_or(self.player.layer);
        let x = i64::from(block_centre(bx));
        let y = i64::from(block_centre(by));
        if let Some(ser) = self.peds.spawn_mission(x, y, z, 0)
            && let Some(i) = self.peds.p.iter().position(|p| p.alive && p.serial == ser)
        {
            self.peds.p[i].mission = false;
            self.peds.kill(i);
            self.say(format!(
                "script: killped decl {line} -> ped {i} dead at ({bx},{by})"
            ));
        }
    }

    /// A test fixture: wreck the car at mission declaration `line` (or spawn a burning wreck).
    pub fn wreck_car_decl(&mut self, line: i32) {
        self.live_city = true;
        let (bx, by, model_id) = self
            .section
            .as_ref()
            .and_then(|s| s.decls.iter().find(|d| d.id == line))
            .map(|d| {
                (
                    d.pos[0],
                    d.pos[1],
                    d.nums.first().copied().unwrap_or(0) as u8,
                )
            })
            .unwrap_or_else(|| {
                let (px, py) = self.player.block();
                (px + 1, py, 0)
            });
        let z = self.map.stand_layer(bx, by).unwrap_or(self.player.layer);
        let x = i64::from(block_centre(bx));
        let y = i64::from(block_centre(by));
        let model = self
            .cars
            .iter()
            .position(|c| c.model == model_id)
            .unwrap_or(0);
        if let Some(ci) = self.traffic.car_at(&self.cars, x, y, z, 24) {
            self.traffic.cars[ci].damage = cars::WRECKED;
            self.traffic.cars[ci].fuse = 1;
            self.say(format!(
                "script: wreck decl {line} -> fleet car {ci} at ({bx},{by})"
            ));
        } else if let Some(ser) =
            self.traffic
                .abandon(&self.cars, model, x, y, 0, z, -1, cars::WRECKED)
        {
            self.traffic.write_off(ser);
            self.say(format!(
                "script: wreck decl {line} -> car #{ser} at ({bx},{by})"
            ));
        }
    }

    /// A test fixture: add `delta` directly to the player's score.
    pub fn add_score(&mut self, delta: i64) {
        self.score.score = (self.score.score + delta).clamp(0, score::SCORE_CAP);
        self.say(format!("score: {:+} -> {}", delta, self.score.score));
    }

    /// A test fixture: run a sequence of building-explosion commands starting at `line`.
    pub fn start_script_line(&mut self, line: i32) {
        let Some(sec) = &self.section else {
            self.say(format!("script: no section loaded for line {line}"));
            return;
        };
        if let Some(pos) = sec.cmds.iter().position(|c| c.id == line) {
            self.explode_proc = Some((pos, 0));
            self.say(format!("script: started proc at line {line}"));
        } else {
            self.say(format!("script: line {line} not found"));
        }
    }

    /// Start the section's mission script (`docs/MISSION_VM.md`). False
    /// when the world has no section.
    ///
    /// The crates become the script's. GTA8.EXE's loader only keeps a
    /// `POWERUP`'s two numbers (0x79db3); a crate exists once a
    /// `POWERUP_ON` names it. So the ones [`World::load_section`] put out
    /// go, and the script's own make them again.
    pub fn start_mission(&mut self) -> bool {
        let Some(section) = &self.section else {
            return false;
        };
        self.mission = Some(Box::new(crate::mission::Vm::new(section)));
        let placed = self
            .pickups
            .p
            .iter()
            .filter(|p| p.state != pickups::PickupState::Gone)
            .count();
        self.pickups = Pickups::new();
        self.say(format!(
            "pickups: {placed} cleared; the script's POWERUP_ON makes them now"
        ));
        true
    }

    /// The mission script's tick: a game frame of it when its clock says.
    fn mission_tick(&mut self) {
        let Some(mut vm) = self.mission.take() else {
            return;
        };
        vm.tick(self);
        for line in vm.drain_log() {
            self.say(line);
        }
        self.mission = Some(vm);
    }

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
        h.bool(self.prev_fire);
        if self.knocked > 0 {
            h.i32(self.knocked);
        }
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
                h.bool(e.cop);
                h.u8(e.via_n);
                for pt in &e.via[..usize::from(e.via_n)] {
                    h.bytes(&pt.0.to_le_bytes());
                    h.bytes(&pt.1.to_le_bytes());
                }
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
            Doing::Sliding(s) => {
                h.u8(6);
                s.sim_hash(&mut h);
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
        h.i32(self.armour);
        h.i32(self.burning);
        self.arms.sim_hash(&mut h);
        self.cannon.sim_hash(&mut h);
        h.bool(self.frenzy);
        h.u32(self.weapons_clock.phase());
        self.weapons.sim_hash(&mut h);
        self.pickups.sim_hash(&mut h);
        self.marks.sim_hash(&mut h);
        self.service.sim_hash(&mut h);
        self.score.sim_hash(&mut h);
        h.i32(self.lives);
        h.bool(self.jail_free);
        h.i32(self.player_speed);
        h.i32(self.bust_timer);
        h.u8(self.card as u8);
        if let Some(vm) = &self.mission {
            vm.sim_hash(&mut h);
        }
        h.i32(self.foot_arrest);
        h.bool(self.game_over);
        self.camera.sim_hash(&mut h);
        h.finish()
    }

    /// The player's position for the renderer: 16.16 world pixels, with
    /// the height of the ground under him (32 px to a layer).
    pub fn player_pos(&self) -> [i32; 3] {
        let p = &self.player;
        [p.x, p.y, self.map.ground_z(p.x, p.y, p.layer)]
    }

    /// The gtadump-style one-line summary of the player, or of his car.
    /// Health, his gun, the score and the wanted level, for the overlay.
    pub fn status_summary(&self) -> String {
        let a = &self.arms;
        let s = &self.score;
        let card = match self.card {
            Card::None => "",
            Card::Busted => "  [BUSTED]",
            Card::Wasted => "  [WASTED]",
            Card::GameOver => "  [GAME OVER]",
        };
        let key = if self.jail_free { " key" } else { "" };
        format!(
            "lives {}{key}  health {} armour {}  weapon {} ({})  score {} x{}  heat {} level {}{card}",
            self.lives,
            self.health,
            self.armour,
            a.weapon,
            a.ammo[a.weapon],
            s.score,
            s.multiplier,
            s.heat,
            s.level
        )
    }

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
