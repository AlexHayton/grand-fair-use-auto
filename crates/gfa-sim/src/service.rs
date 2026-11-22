use gfa_fixed::fx::{BLOCK_SHIFT, FP};
use gfa_fixed::trig::{cos, dir16, sin};
use gfa_formats::style::CarInfo;

use crate::cars;
use crate::hash::StateHasher;
use crate::map::{SimMap, nav};
use crate::peds::Peds;
use crate::traffic::Traffic;
use crate::weapons::Weapons;

const BS: u32 = FP + BLOCK_SHIFT;

pub const AMB_MAX: usize = 3;
pub const AMB_BODIES: usize = 10;
pub const ENGINE_MAX: usize = 3;
pub const FIREREC_MAX: usize = 4;

pub const REVIVE_FRAMES: i32 = 75;
pub const SPRAY_FRAMES: i32 = 261;
pub const ENGINE_LIFE: i32 = 2500;
pub const HOSE_REACH_PX: i64 = 160;

pub const OBJ_WATER_JET: usize = 0x30;
pub const OBJ_WATER_JET_END: usize = 0x31;

const MODEL_AMBULANCE: u8 = 5;
const MODEL_FIRE_ENGINE: u8 = 42;

fn angle_to(fx: i64, fy: i64, tx: i64, ty: i64) -> i32 {
    (dir16((tx - fx) as i32, (ty - fy) as i32) >> 16) & 255
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AmbState {
    #[default]
    Idle = 0,
    ToScene = 1,
    Crewed = 2,
    ToHospital = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ambulance {
    pub state: AmbState,
    pub car_serial: u64,
    pub body_slot: i32,
    pub body_x: i64,
    pub body_y: i64,
    pub body_layer: i32,
    pub medic: [u64; 2],
    pub medic_state: [u8; 2],
    pub heal_timer: i32,
    pub wait_ticks: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FireState {
    #[default]
    Idle = 0,
    ToFire = 1,
    Spraying = 2,
    Leaving = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FireEngine {
    pub state: FireState,
    pub car_serial: u64,
    pub target_serial: u64,
    pub fire_x: i64,
    pub fire_y: i64,
    pub fire_layer: i32,
    pub spray_ticks: i32,
    pub age: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterJet {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    pub angle: i32,
    pub end: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Service {
    pub amb: [Ambulance; AMB_MAX],
    pub bodies: [i32; AMB_BODIES],
    pub n_bodies: usize,
    pub eng: [FireEngine; ENGINE_MAX],
    pub seen_fires: [u64; FIREREC_MAX],
    pub stat_amb_sent: u64,
    pub stat_revived: u64,
    pub stat_fire_sent: u64,
    pub stat_fires_out: u64,
}

impl Service {
    pub fn new() -> Self {
        Self::default()
    }

    fn nearest_hospital(map: &SimMap, bx: i32, by: i32) -> Option<(i32, i32)> {
        let mut best = None;
        let mut best_d = i32::MAX;
        for h in &map.cmp().locations[1] {
            if h.x == 0 && h.y == 0 {
                continue;
            }
            let d = (i32::from(h.x) - bx).abs() + (i32::from(h.y) - by).abs();
            if d < best_d {
                best_d = d;
                best = Some((i32::from(h.x), i32::from(h.y)));
            }
        }
        best
    }

    fn nearest_fire_base(map: &SimMap, bx: i32, by: i32) -> Option<(i32, i32)> {
        let mut best = None;
        let mut best_d = i32::MAX;
        for h in &map.cmp().locations[4] {
            if h.x == 0 && h.y == 0 {
                continue;
            }
            let d = (i32::from(h.x) - bx).abs() + (i32::from(h.y) - by).abs();
            if d < best_d {
                best_d = d;
                best = Some((i32::from(h.x), i32::from(h.y)));
            }
        }
        best.or_else(|| Self::nearest_hospital(map, bx, by))
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_van(
        map: &SimMap,
        cars: &[CarInfo],
        traffic: &mut Traffic,
        model_id: u8,
        hx: i32,
        hy: i32,
        tx: i32,
        ty: i32,
        svc: u8,
    ) -> Option<u64> {
        let idx = cars.iter().position(|c| c.model == model_id)?;
        for check_view in [true, false] {
            for r in 2..=32i32 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dy.abs() != r {
                            continue;
                        }
                        let bx = hx + dx;
                        let by = hy + dy;
                        if !(1..255).contains(&bx) || !(1..255).contains(&by) {
                            continue;
                        }
                        if check_view && traffic.block_in_view(bx, by) {
                            continue;
                        }
                        for z in 0..6 {
                            let mask = nav::dirs(map.nav(bx, by, z));
                            if mask == 0 {
                                continue;
                            }
                            let ang = if (mask & nav::N) != 0 {
                                0
                            } else if (mask & nav::E) != 0 {
                                64
                            } else if (mask & nav::S) != 0 {
                                128
                            } else {
                                192
                            };
                            let Some(ser) =
                                traffic.spawn_service_car(cars, idx, bx, by, z, ang, svc)
                            else {
                                continue;
                            };
                            traffic.send_to(map, ser, tx, ty);
                            return Some(ser);
                        }
                    }
                }
            }
        }
        None
    }

    fn body_tracked(&self, ped_slot: i32) -> bool {
        for a in &self.amb {
            if a.state != AmbState::Idle && a.body_slot == ped_slot {
                return true;
            }
        }
        self.bodies[..self.n_bodies].contains(&ped_slot)
    }

    fn scan_bodies(&mut self, peds: &Peds) {
        let mut w = 0;
        for i in 0..self.n_bodies {
            let s = self.bodies[i];
            if let Ok(idx) = usize::try_from(s)
                && idx < peds.p.len()
                && peds.p[idx].alive
                && peds.p[idx].corpse
            {
                self.bodies[w] = s;
                w += 1;
            }
        }
        self.n_bodies = w;

        for (i, p) in peds.p.iter().enumerate() {
            if self.n_bodies >= AMB_BODIES {
                break;
            }
            if !p.alive || !p.corpse {
                continue;
            }
            let slot = i as i32;
            if self.body_tracked(slot) {
                continue;
            }
            self.bodies[self.n_bodies] = slot;
            self.n_bodies += 1;
        }
    }

    fn amb_drop(traffic: &mut Traffic, peds: &mut Peds, a: &mut Ambulance) {
        if a.car_serial != 0 {
            traffic.set_service(a.car_serial, 0);
            traffic.set_mission(a.car_serial, false);
        }
        for &ms in &a.medic {
            if ms != 0 {
                peds.take_mission(ms);
            }
        }
        *a = Ambulance::default();
    }

    fn amb_step(
        &mut self,
        map: &SimMap,
        cars: &[CarInfo],
        traffic: &mut Traffic,
        peds: &mut Peds,
        a_idx: usize,
        say: &mut dyn FnMut(String),
    ) {
        let a = self.amb[a_idx];
        match a.state {
            AmbState::Idle => {
                if self.n_bodies == 0 {
                    return;
                }
                let slot = self.bodies[0];
                for i in 1..self.n_bodies {
                    self.bodies[i - 1] = self.bodies[i];
                }
                self.n_bodies -= 1;
                let Ok(u_slot) = usize::try_from(slot) else {
                    return;
                };
                if u_slot >= peds.p.len() || !peds.p[u_slot].alive || !peds.p[u_slot].corpse {
                    return;
                }
                let (px, py, pl) = (peds.p[u_slot].x, peds.p[u_slot].y, peds.p[u_slot].layer);
                let bx = (px >> BS) as i32;
                let by = (py >> BS) as i32;
                let (hx, hy) = Self::nearest_hospital(map, bx, by).unwrap_or((bx, by));
                let Some(ser) =
                    Self::spawn_van(map, cars, traffic, MODEL_AMBULANCE, hx, hy, bx, by, 1)
                else {
                    return;
                };
                peds.p[u_slot].svc_q = 2;
                self.amb[a_idx] = Ambulance {
                    state: AmbState::ToScene,
                    car_serial: ser,
                    body_slot: slot,
                    body_x: px,
                    body_y: py,
                    body_layer: pl,
                    medic: [0; 2],
                    medic_state: [0; 2],
                    heal_timer: 0,
                    wait_ticks: 0,
                };
                self.stat_amb_sent += 1;
                say(format!(
                    "service: ambulance #{ser} dispatched to body at ({bx},{by})"
                ));
            }
            AmbState::ToScene => {
                let Some((cx, cy, cl, _, wrecked)) = traffic.find_car(a.car_serial) else {
                    let mut tmp = a;
                    Self::amb_drop(traffic, peds, &mut tmp);
                    self.amb[a_idx] = tmp;
                    return;
                };
                if wrecked {
                    let mut tmp = a;
                    Self::amb_drop(traffic, peds, &mut tmp);
                    self.amb[a_idx] = tmp;
                    return;
                }
                let Ok(u_slot) = usize::try_from(a.body_slot) else {
                    let mut tmp = a;
                    Self::amb_drop(traffic, peds, &mut tmp);
                    self.amb[a_idx] = tmp;
                    return;
                };
                if u_slot >= peds.p.len() || !peds.p[u_slot].alive || !peds.p[u_slot].corpse {
                    let mut tmp = a;
                    Self::amb_drop(traffic, peds, &mut tmp);
                    self.amb[a_idx] = tmp;
                    return;
                }
                let near = (cx - a.body_x).abs() + (cy - a.body_y).abs() < (5 << BS);
                let mut wait_ticks = a.wait_ticks + 1;
                let mut arrived = traffic.arrived(a.car_serial) || near;
                if !arrived && wait_ticks > 900 {
                    if !near {
                        let mut tmp = a;
                        Self::amb_drop(traffic, peds, &mut tmp);
                        self.amb[a_idx] = tmp;
                        return;
                    }
                    arrived = true;
                }
                if !arrived {
                    self.amb[a_idx].wait_ticks = wait_ticks;
                    return;
                }
                traffic.stop_car(a.car_serial);
                wait_ticks = 0;
                let mut medic = [0u64; 2];
                let mut medic_state = [0u8; 2];
                for m in 0..2 {
                    let side = if m == 0 { -1i64 } else { 1i64 };
                    let mx = cx + side * (18 << FP);
                    let my = cy + side * (10 << FP);
                    if let Some(ms) = peds.spawn_mission(mx, my, cl, 0) {
                        peds.set_remap(ms, 4);
                        peds.set_speed(ms, 2);
                        peds.mission_ai(ms, 2, a.body_x, a.body_y, false);
                        medic[m] = ms;
                        medic_state[m] = 1;
                    }
                }
                self.amb[a_idx].wait_ticks = wait_ticks;
                self.amb[a_idx].medic = medic;
                self.amb[a_idx].medic_state = medic_state;
                self.amb[a_idx].heal_timer = REVIVE_FRAMES;
                self.amb[a_idx].state = AmbState::Crewed;
            }
            AmbState::Crewed => {
                let Some((cx, cy, _, _, wrecked)) = traffic.find_car(a.car_serial) else {
                    let mut tmp = a;
                    Self::amb_drop(traffic, peds, &mut tmp);
                    self.amb[a_idx] = tmp;
                    return;
                };
                if wrecked {
                    let mut tmp = a;
                    Self::amb_drop(traffic, peds, &mut tmp);
                    self.amb[a_idx] = tmp;
                    return;
                }
                let mut tmp = a;
                let mut alive_medics = 0;
                let mut at_body = false;
                let mut back_in = 0;

                for m in 0..2 {
                    let ms = tmp.medic[m];
                    if ms == 0 {
                        continue;
                    }
                    let Some((mx, my, _, dead)) = peds.find(ms) else {
                        tmp.medic[m] = 0;
                        continue;
                    };
                    if dead {
                        tmp.medic[m] = 0;
                        continue;
                    }
                    alive_medics += 1;
                    if tmp.medic_state[m] == 1 {
                        if (mx - tmp.body_x).abs() + (my - tmp.body_y).abs() < (28 << FP) {
                            tmp.medic_state[m] = 2;
                            peds.mission_ai(ms, 1, tmp.body_x, tmp.body_y, false);
                            at_body = true;
                        }
                    } else if tmp.medic_state[m] == 2 {
                        at_body = true;
                    } else if tmp.medic_state[m] == 3 {
                        peds.mission_ai(ms, 2, cx, cy, false);
                        if (mx - cx).abs() + (my - cy).abs() < (28 << FP) {
                            peds.take_mission(ms);
                            tmp.medic[m] = 0;
                            back_in += 1;
                        }
                    }
                }
                if alive_medics == 0 && back_in == 0 {
                    Self::amb_drop(traffic, peds, &mut tmp);
                    self.amb[a_idx] = tmp;
                    return;
                }
                if at_body && tmp.heal_timer > 0 {
                    tmp.heal_timer -= 1;
                    if tmp.heal_timer == 0 {
                        if let Ok(u_slot) = usize::try_from(tmp.body_slot)
                            && u_slot < peds.p.len()
                            && peds.p[u_slot].alive
                            && peds.p[u_slot].corpse
                        {
                            peds.revive(u_slot);
                            self.stat_revived += 1;
                            say(format!("service: medic revived ped {}", tmp.body_slot));
                        }
                        for m in 0..2 {
                            if tmp.medic[m] != 0 {
                                tmp.medic_state[m] = 3;
                                peds.mission_ai(tmp.medic[m], 2, cx, cy, false);
                            }
                        }
                    }
                }
                tmp.wait_ticks += 1;
                if (tmp.medic[0] == 0 && tmp.medic[1] == 0) || tmp.wait_ticks > 600 {
                    for ms in &mut tmp.medic {
                        if *ms != 0 {
                            peds.take_mission(*ms);
                            *ms = 0;
                        }
                    }
                    let bx = (cx >> BS) as i32;
                    let by = (cy >> BS) as i32;
                    if let Some((hx, hy)) = Self::nearest_hospital(map, bx, by) {
                        traffic.send_to(map, tmp.car_serial, hx, hy);
                        tmp.state = AmbState::ToHospital;
                    } else {
                        Self::amb_drop(traffic, peds, &mut tmp);
                    }
                }
                self.amb[a_idx] = tmp;
            }
            AmbState::ToHospital => {
                let Some((_, _, _, _, wrecked)) = traffic.find_car(a.car_serial) else {
                    let mut tmp = a;
                    Self::amb_drop(traffic, peds, &mut tmp);
                    self.amb[a_idx] = tmp;
                    return;
                };
                if wrecked {
                    let mut tmp = a;
                    Self::amb_drop(traffic, peds, &mut tmp);
                    self.amb[a_idx] = tmp;
                    return;
                }
                if !traffic.car_in_view(a.car_serial) {
                    traffic.remove_car(a.car_serial);
                    self.amb[a_idx] = Ambulance::default();
                } else if traffic.arrived(a.car_serial) {
                    traffic.set_service(a.car_serial, 0);
                    traffic.set_mission(a.car_serial, false);
                    traffic.wake(a.car_serial);
                    self.amb[a_idx] = Ambulance::default();
                }
            }
        }
    }

    fn fire_seen(&self, ser: u64) -> bool {
        for &s in &self.seen_fires {
            if s == ser {
                return true;
            }
        }
        for e in &self.eng {
            if e.state != FireState::Idle && e.target_serial == ser {
                return true;
            }
        }
        false
    }

    fn fire_remember(&mut self, ser: u64) {
        for i in (1..FIREREC_MAX).rev() {
            self.seen_fires[i] = self.seen_fires[i - 1];
        }
        self.seen_fires[0] = ser;
    }

    fn find_burning_car(&self, traffic: &Traffic) -> Option<(u64, i64, i64, i32)> {
        for c in &traffic.cars[..traffic.n] {
            if c.done || (!c.wrecked && c.fuse == 0) || (c.wrecked && c.fire_ticks <= 40) {
                continue;
            }
            if self.fire_seen(c.serial) {
                continue;
            }
            return Some((c.serial, c.x, c.y, c.layer));
        }
        None
    }

    fn eng_drop(traffic: &mut Traffic, e: &mut FireEngine) {
        if e.car_serial != 0 {
            traffic.set_service(e.car_serial, 0);
            traffic.set_mission(e.car_serial, false);
        }
        *e = FireEngine::default();
    }

    #[allow(clippy::too_many_arguments)]
    fn eng_step(
        &mut self,
        map: &SimMap,
        cars: &[CarInfo],
        traffic: &mut Traffic,
        peds: &mut Peds,
        weapons: &mut Weapons,
        e_idx: usize,
        say: &mut dyn FnMut(String),
    ) {
        let mut e = self.eng[e_idx];
        if e.state != FireState::Idle {
            e.age += 1;
            if e.age > ENGINE_LIFE && !traffic.car_in_view(e.car_serial) {
                traffic.remove_car(e.car_serial);
                self.eng[e_idx] = FireEngine::default();
                return;
            }
        }
        match e.state {
            FireState::Idle => {
                let Some((tser, fx, fy, fl)) = self.find_burning_car(traffic) else {
                    return;
                };
                let bx = (fx >> BS) as i32;
                let by = (fy >> BS) as i32;
                let (hx, hy) = Self::nearest_fire_base(map, bx, by).unwrap_or((bx, by));
                let Some(ser) =
                    Self::spawn_van(map, cars, traffic, MODEL_FIRE_ENGINE, hx, hy, bx, by, 2)
                else {
                    return;
                };
                self.fire_remember(tser);
                self.eng[e_idx] = FireEngine {
                    state: FireState::ToFire,
                    car_serial: ser,
                    target_serial: tser,
                    fire_x: fx,
                    fire_y: fy,
                    fire_layer: fl,
                    spray_ticks: 0,
                    age: 0,
                };
                self.stat_fire_sent += 1;
                say(format!(
                    "service: fire engine #{ser} dispatched to fire at ({bx},{by})"
                ));
            }
            FireState::ToFire => {
                let Some((cx, cy, _, _, wrecked)) = traffic.find_car(e.car_serial) else {
                    Self::eng_drop(traffic, &mut e);
                    self.eng[e_idx] = e;
                    return;
                };
                if wrecked {
                    Self::eng_drop(traffic, &mut e);
                    self.eng[e_idx] = e;
                    return;
                }
                if let Some((tx, ty, tl, _, _)) = traffic.find_car(e.target_serial) {
                    e.fire_x = tx;
                    e.fire_y = ty;
                    e.fire_layer = tl;
                }
                let d = ((cx - e.fire_x).abs() + (cy - e.fire_y).abs()) >> FP;
                if d <= HOSE_REACH_PX || traffic.arrived(e.car_serial) {
                    e.state = FireState::Spraying;
                    e.spray_ticks = SPRAY_FRAMES;
                    traffic.stop_car(e.car_serial);
                }
                self.eng[e_idx] = e;
            }
            FireState::Spraying => {
                let Some((cx, cy, cl, _, wrecked)) = traffic.find_car(e.car_serial) else {
                    Self::eng_drop(traffic, &mut e);
                    self.eng[e_idx] = e;
                    return;
                };
                if wrecked {
                    Self::eng_drop(traffic, &mut e);
                    self.eng[e_idx] = e;
                    return;
                }
                let n = traffic.n;
                for c in &mut traffic.cars[..n] {
                    if c.done {
                        continue;
                    }
                    if (c.x - cx).abs() + (c.y - cy).abs() <= (HOSE_REACH_PX << FP) {
                        if c.fire_ticks > 0 {
                            c.fire_ticks -= 4;
                            if c.fire_ticks <= 0 {
                                c.fire_ticks = 0;
                                self.stat_fires_out += 1;
                            }
                        }
                        if c.fuse > 0 {
                            c.fuse = 0;
                            c.damage = c.damage.min(cars::WRECKED - 1);
                            self.stat_fires_out += 1;
                        }
                    }
                }
                for f in &mut weapons.fires {
                    if f.ticks == 0 {
                        continue;
                    }
                    if (f.x - cx).abs() + (f.y - cy).abs() <= (HOSE_REACH_PX << FP) {
                        f.ticks = 0;
                        self.stat_fires_out += 1;
                    }
                }
                let ang = angle_to(cx, cy, e.fire_x, e.fire_y);
                let jx = cx + i64::from(sin(ang)) * 48 * 4;
                let jy = cy - i64::from(cos(ang)) * 48 * 4;
                for p in &mut peds.p {
                    if !p.alive || p.corpse || p.shot != 0 || p.layer != cl {
                        continue;
                    }
                    if (p.x - jx).abs() < (28 << FP) && (p.y - jy).abs() < (28 << FP) {
                        p.fall = 12;
                        p.speed = 0;
                        p.burn = 0;
                        p.angle = (ang + 128) & 255;
                    }
                }
                e.spray_ticks -= 1;
                if e.spray_ticks <= 0 {
                    e.state = FireState::Leaving;
                    let bx = (cx >> BS) as i32;
                    let by = (cy >> BS) as i32;
                    if let Some((hx, hy)) = Self::nearest_fire_base(map, bx, by) {
                        traffic.send_to(map, e.car_serial, hx, hy);
                    }
                }
                self.eng[e_idx] = e;
            }
            FireState::Leaving => {
                if !traffic.car_in_view(e.car_serial) {
                    traffic.remove_car(e.car_serial);
                    self.eng[e_idx] = FireEngine::default();
                } else if traffic.arrived(e.car_serial) {
                    traffic.set_service(e.car_serial, 0);
                    traffic.set_mission(e.car_serial, false);
                    traffic.wake(e.car_serial);
                    self.eng[e_idx] = FireEngine::default();
                } else {
                    self.eng[e_idx] = e;
                }
            }
        }
    }

    pub fn step(
        &mut self,
        map: &SimMap,
        cars: &[CarInfo],
        traffic: &mut Traffic,
        peds: &mut Peds,
        weapons: &mut Weapons,
        say: &mut dyn FnMut(String),
    ) {
        self.scan_bodies(peds);
        for i in 0..AMB_MAX {
            self.amb_step(map, cars, traffic, peds, i, say);
        }
        for i in 0..ENGINE_MAX {
            self.eng_step(map, cars, traffic, peds, weapons, i, say);
        }
    }

    pub fn water_jets(&self, traffic: &Traffic) -> Vec<WaterJet> {
        let mut out = Vec::new();
        for e in &self.eng {
            if e.state != FireState::Spraying {
                continue;
            }
            let Some((cx, cy, cl, _, _)) = traffic.find_car(e.car_serial) else {
                continue;
            };
            let dx = e.fire_x - cx;
            let dy = e.fire_y - cy;
            let ang = angle_to(cx, cy, e.fire_x, e.fire_y);
            let dist = ((dx.abs() + dy.abs()) >> FP).clamp(24, HOSE_REACH_PX);
            let mut step = 18i64;
            while step <= dist {
                let x = cx + i64::from(sin(ang)) * step * 4;
                let y = cy - i64::from(cos(ang)) * step * 4;
                let is_end = step + 18 > dist;
                out.push(WaterJet {
                    x,
                    y,
                    layer: cl,
                    angle: ang,
                    end: is_end,
                });
                step += 18;
            }
        }
        out
    }

    pub fn sim_hash(&self, h: &mut StateHasher) {
        for v in [
            self.stat_amb_sent,
            self.stat_revived,
            self.stat_fire_sent,
            self.stat_fires_out,
        ] {
            h.bytes(&v.to_le_bytes());
        }
    }
}
