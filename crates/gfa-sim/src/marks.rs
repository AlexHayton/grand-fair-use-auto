use gfa_fixed::fx::{BLOCK_SHIFT, FP, block_centre};
use gfa_fixed::trig::{cos, dir16, sin};

use crate::hash::StateHasher;
use crate::map::{SimMap, ground};
use crate::peds::Peds;
use crate::weapons::{MAX_BLASTS, Weapons};

const BS: u32 = FP + BLOCK_SHIFT;

pub const MAX_MARKS: usize = 200;
pub const MAX_SMOKES: usize = 24;
pub const MARK_CELLS: usize = 128;
pub const MAX_STAINS: usize = 48;
pub const MAX_CLOUDS: usize = 4;
pub const MAX_DEBRIS: usize = 100;
pub const MAX_SPARKS: usize = 8;

pub const OBJ_SMOKE: usize = 0x0a;
pub const OBJ_MARK: usize = 0x0b;
pub const OBJ_BLOODY: usize = 0x0c;
pub const OBJ_SPARK: usize = 0x0d;
pub const OBJ_CLOUD: usize = 0x33;
pub const OBJ_SPLASH: usize = 0x36;
pub const OBJ_POOL: usize = 0x3f;
pub const OBJ_SPLAT: usize = 0x40;
pub const OBJ_TRAIL: usize = 0x4d;
pub const OBJ_RUBBLE: [usize; 3] = [0x2c, 0x2d, 0x2b];
pub const OBJ_LUMP: usize = 0x05;

pub const MARK_FRAMES: i32 = 4;
pub const MARK_FRAME_TICKS: i32 = 128;
pub const MARK_LIFE: i32 = MARK_FRAMES * MARK_FRAME_TICKS;
pub const MARK_CELL_MAX: u8 = 16;

pub const SMOKE_FRAMES: i32 = 11;
pub const SMOKE_FRAME_TICKS: i32 = 2;
pub const SMOKE_LIFE: i32 = SMOKE_FRAMES * SMOKE_FRAME_TICKS;
pub const SMOKE_CELL_MAX: u8 = 2;

pub const CLOUD_FRAMES: i32 = 12;
pub const CLOUD_FRAME_TICKS: i32 = 2;
pub const CLOUD_LIFE: i32 = CLOUD_FRAMES * CLOUD_FRAME_TICKS;
pub const CLOUD_AT_FRAME: i32 = 10;

pub const POOL_FRAMES: i32 = 12;
pub const POOL_FRAME_TICKS: i32 = 24;
pub const POOL_LIFE: i32 = POOL_FRAMES * POOL_FRAME_TICKS;
pub const POOL_SPLAT_AT: i32 = POOL_LIFE - 4 * POOL_FRAME_TICKS;

pub const SPLAT_FREE_LIFE: i32 = 1000;
pub const BLOOD_REACH: i64 = 4 << FP;
/// Bloody tyre-mark drops left after running over a ped.
pub const BLOOD_TYRE_STEPS: i32 = 16;
/// Distance (16.16 world px) between consecutive bloody tyre marks.
pub const BLOOD_TYRE_STEP_PX: i64 = 4 << FP;
pub const SPARK_LIFE: i32 = 5;
const ORIG_UNIT: i64 = 0x4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mark {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    pub angle: i32,
    pub left: i32,
    pub cell: u8,
    pub bloody: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StainKind {
    #[default]
    Pool = 0,
    Splat = 1,
    Trail = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stain {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    pub angle: i32,
    pub left: i32,
    pub kind: StainKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cloud {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    pub left: i32,
    pub splash: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Debris {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    pub angle: i32,
    pub obj: usize,
    pub frame: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Spark {
    pub x: i64,
    pub y: i64,
    pub vx: i64,
    pub vy: i64,
    pub layer: i32,
    pub angle: i32,
    pub left: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkSprite {
    pub x: i64,
    pub y: i64,
    pub layer: i32,
    pub angle: i32,
    pub object: usize,
    pub frame: usize,
}

#[derive(Debug, Clone)]
pub struct Marks {
    pub marks: [Mark; MAX_MARKS],
    pub smokes: [Mark; MAX_SMOKES],
    pub mark_cell: [u8; MARK_CELLS],
    pub smoke_cell: [u8; MARK_CELLS],
    pub stains: [Stain; MAX_STAINS],
    pub clouds: [Cloud; MAX_CLOUDS],
    pub cloud_seen: [bool; MAX_BLASTS],
    pub debris: [Debris; MAX_DEBRIS],
    pub sparks: [Spark; MAX_SPARKS],
    pub n_debris: usize,
    pub head_mark: usize,
    pub head_smoke: usize,
    pub head_stain: usize,
    pub head_debris: usize,
    pub half: bool,
    pub spark_rng: u32,
    pub stat_marks: u64,
    pub stat_bloody: u64,
    pub stat_smokes: u64,
    pub stat_pools: u64,
    pub stat_splats: u64,
    pub stat_trails: u64,
    pub stat_clouds: u64,
    pub stat_splashes: u64,
    pub stat_debris: u64,
    pub stat_sparks: u64,
}

impl Default for Marks {
    fn default() -> Self {
        Self::new()
    }
}

impl Marks {
    pub fn new() -> Self {
        Self {
            marks: [Mark::default(); MAX_MARKS],
            smokes: [Mark::default(); MAX_SMOKES],
            mark_cell: [0; MARK_CELLS],
            smoke_cell: [0; MARK_CELLS],
            stains: [Stain::default(); MAX_STAINS],
            clouds: [Cloud::default(); MAX_CLOUDS],
            cloud_seen: [false; MAX_BLASTS],
            debris: [Debris::default(); MAX_DEBRIS],
            sparks: [Spark::default(); MAX_SPARKS],
            n_debris: 0,
            head_mark: 0,
            head_smoke: 0,
            head_stain: 0,
            head_debris: 0,
            half: false,
            spark_rng: 12345,
            stat_marks: 0,
            stat_bloody: 0,
            stat_smokes: 0,
            stat_pools: 0,
            stat_splats: 0,
            stat_trails: 0,
            stat_clouds: 0,
            stat_splashes: 0,
            stat_debris: 0,
            stat_sparks: 0,
        }
    }

    fn cell_index(x: i64, y: i64) -> u8 {
        let bx = (x >> BS) as i32;
        let by = (y >> BS) as i32;
        (((bx + (by << 4)) as usize) & (MARK_CELLS - 1)) as u8
    }

    fn stain_put(&mut self, x: i64, y: i64, layer: i32, angle: i32, kind: StainKind, life: i32) {
        let mut slot = None;
        for (i, s) in self.stains.iter().enumerate() {
            if s.left <= 0 {
                slot = Some(i);
                break;
            }
        }
        let slot = slot.unwrap_or_else(|| {
            let s = self.head_stain;
            self.head_stain = (self.head_stain + 1) % MAX_STAINS;
            s
        });
        self.stains[slot] = Stain {
            x,
            y,
            layer,
            angle: angle & 255,
            left: life,
            kind,
        };
    }

    pub fn pool(&mut self, x: i64, y: i64, layer: i32) {
        for s in &self.stains {
            if s.left > 0
                && s.kind == StainKind::Pool
                && s.layer == layer
                && (s.x - x).abs() < (6 << FP)
                && (s.y - y).abs() < (6 << FP)
            {
                return;
            }
        }
        self.stain_put(x, y, layer, 0, StainKind::Pool, POOL_LIFE);
        self.stat_pools += 1;
    }

    pub fn splat(&mut self, x: i64, y: i64, layer: i32, angle: i32) {
        self.stain_put(x, y, layer, angle, StainKind::Splat, SPLAT_FREE_LIFE);
        self.stat_splats += 1;
    }

    fn trail(&mut self, x: i64, y: i64, layer: i32, angle: i32) {
        self.stain_put(x, y, layer, angle, StainKind::Trail, SPLAT_FREE_LIFE);
        self.stat_trails += 1;
    }

    pub fn in_blood(&self, x: i64, y: i64, layer: i32) -> bool {
        for s in &self.stains {
            if s.left <= 0 || s.layer != layer {
                continue;
            }
            if s.kind == StainKind::Pool && s.left > POOL_SPLAT_AT {
                continue;
            }
            if (s.x - x).abs() < BLOOD_REACH && (s.y - y).abs() < BLOOD_REACH {
                return true;
            }
        }
        false
    }

    fn cloud_put(&mut self, x: i64, y: i64, layer: i32, splash: bool) {
        let mut slot = 0;
        let mut best = i32::MAX;
        for (i, c) in self.clouds.iter().enumerate() {
            if c.left <= 0 {
                slot = i;
                break;
            }
            if c.left < best {
                best = c.left;
                slot = i;
            }
        }
        self.clouds[slot] = Cloud {
            x,
            y,
            layer,
            left: CLOUD_LIFE,
            splash,
        };
        if splash {
            self.stat_splashes += 1;
        } else {
            self.stat_clouds += 1;
        }
    }

    pub fn splash(&mut self, x: i64, y: i64, layer: i32) {
        self.cloud_put(x, y, layer, true);
    }

    fn debris_put(&mut self, x: i64, y: i64, layer: i32, angle: i32, obj: usize, frame: usize) {
        let slot = self.head_debris;
        self.head_debris = (self.head_debris + 1) % MAX_DEBRIS;
        if self.n_debris < MAX_DEBRIS {
            self.n_debris += 1;
        }
        self.debris[slot] = Debris {
            x,
            y,
            layer,
            angle: angle & 255,
            obj,
            frame,
        };
        self.stat_debris += 1;
    }

    pub fn debris(&mut self, bx: i32, by: i32, layer: i32, face: i32) {
        static LUMP_XY: [[i32; 2]; 5] = [[-56, 20], [-20, 44], [0, 32], [28, 12], [56, 48]];
        static LUMP_ANG: [i32; 5] = [24, 96, 160, 200, 48];
        static LUMP_FR: [usize; 5] = [0, 1, 2, 0, 1];
        // Face encoding matches Weapons::explode_face (0 W, 1 E, 2 N, 3 S) and AmiGTA1
        let (fx, fy, rx, ry, ang) = match face & 3 {
            0 => (-1i64, 0i64, 0i64, -1i64, 192),
            1 => (1, 0, 0, 1, 64),
            2 => (0, -1, 1, 0, 0),
            _ => (0, 1, -1, 0, 128),
        };
        let cx = i64::from(block_centre(bx)) + fx * (16 << FP);
        let cy = i64::from(block_centre(by)) + fy * (16 << FP);
        let obj = OBJ_RUBBLE[((bx + by + face).rem_euclid(3)) as usize];

        for side in [-1i64, 1] {
            let x = cx + rx * side * (16 << FP) + fx * (14 << FP);
            let y = cy + ry * side * (16 << FP) + fy * (14 << FP);
            self.debris_put(x, y, layer, ang, obj, 0);
        }
        for k in 0..5 {
            let r = i64::from(LUMP_XY[k][0]) * ORIG_UNIT;
            let f = i64::from(LUMP_XY[k][1]) * ORIG_UNIT;
            let x = cx + rx * r + fx * f;
            let y = cy + ry * r + fy * f;
            self.debris_put(x, y, layer, ang + LUMP_ANG[k], OBJ_LUMP, LUMP_FR[k]);
        }
    }

    pub fn sparks(&mut self, x: i64, y: i64, layer: i32, len: i32, wid: i32, heading: i32) {
        let h = heading & 255;
        let fx = i64::from(sin(h));
        let fy = -i64::from(cos(h));
        let rx = i64::from(cos(h));
        let ry = i64::from(sin(h));
        let hl = i64::from(len / 2);
        let hw = i64::from(wid / 2);

        for s in &mut self.sparks {
            if s.left > 0 {
                continue;
            }
            self.spark_rng = self
                .spark_rng
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let r = self.spark_rng;
            let along = ((i64::from((r & 255) as i32 - 128)) * hl) >> 7;
            let side = if (r & 256) != 0 { hw } else { -hw };
            let jitter = ((r >> 9) & 31) as i32 - 16;
            let ang = (h + 128 + jitter) & 255;
            let spd = 2 + (((r >> 14) & 3) as i64);

            s.x = x + (along * fx + side * rx) * 4;
            s.y = y + (along * fy + side * ry) * 4;
            s.vx = i64::from(sin(ang)) * spd * 4;
            s.vy = -i64::from(cos(ang)) * spd * 4;
            s.layer = layer;
            s.angle = ang;
            s.left = SPARK_LIFE;
            self.stat_sparks += 1;
        }
    }

    fn put_mark(&mut self, x: i64, y: i64, layer: i32, angle: i32, bloody: bool) {
        let cell = Self::cell_index(x, y);
        if self.mark_cell[cell as usize] >= MARK_CELL_MAX {
            return;
        }
        let slot = self.head_mark;
        self.head_mark = (self.head_mark + 1) % MAX_MARKS;
        if self.marks[slot].left > 0 {
            let old = self.marks[slot].cell as usize;
            self.mark_cell[old] = self.mark_cell[old].saturating_sub(1);
        }
        self.marks[slot] = Mark {
            x,
            y,
            layer,
            angle: angle & 255,
            left: MARK_LIFE,
            cell,
            bloody,
        };
        self.mark_cell[cell as usize] += 1;
        self.stat_marks += 1;
        if bloody {
            self.stat_bloody += 1;
        }
    }

    fn put_smoke(&mut self, x: i64, y: i64, layer: i32) {
        let cell = Self::cell_index(x, y);
        if self.smoke_cell[cell as usize] >= SMOKE_CELL_MAX {
            return;
        }
        let slot = self.head_smoke;
        self.head_smoke = (self.head_smoke + 1) % MAX_SMOKES;
        if self.smokes[slot].left > 0 {
            let old = self.smokes[slot].cell as usize;
            self.smoke_cell[old] = self.smoke_cell[old].saturating_sub(1);
        }
        self.smokes[slot] = Mark {
            x,
            y,
            layer,
            angle: 0,
            left: SMOKE_LIFE,
            cell,
            bloody: false,
        };
        self.smoke_cell[cell as usize] += 1;
        self.stat_smokes += 1;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn slide(
        &mut self,
        map: &SimMap,
        x: i64,
        y: i64,
        layer: i32,
        wid: i32,
        heading: i32,
        vx: i64,
        vy: i64,
        single: bool,
    ) {
        if self.half {
            return;
        }
        let bx = (x >> BS) as i32;
        let by = (y >> BS) as i32;
        let g = map.nav_ground(bx, by, layer);
        if matches!(g, ground::WATER | ground::BUILDING | ground::AIR) {
            return;
        }
        let ang = if vx.abs() + vy.abs() > (1 << 14) {
            (dir16((vx >> 8) as i32, (vy >> 8) as i32) >> 16) & 255
        } else {
            heading & 255
        };
        if single {
            let bl = self.in_blood(x, y, layer);
            self.put_mark(x, y, layer, ang, bl);
        } else {
            let half_w = i64::from((wid / 2).max(4));
            let rx = i64::from(cos(heading)) * half_w * 4;
            let ry = i64::from(sin(heading)) * half_w * 4;
            let bl0 = self.in_blood(x - rx, y - ry, layer);
            let bl1 = self.in_blood(x + rx, y + ry, layer);
            self.put_mark(x - rx, y - ry, layer, ang, bl0);
            self.put_mark(x + rx, y + ry, layer, ang, bl1);
        }
        self.put_smoke(x, y, layer);
    }

    /// Leave red bloody tyre marks at the wheels (left/right pair, or single
    /// centre mark for a bike), without skid smoke.
    #[allow(clippy::too_many_arguments)]
    pub fn blood_track(
        &mut self,
        map: &SimMap,
        x: i64,
        y: i64,
        layer: i32,
        wid: i32,
        heading: i32,
        vx: i64,
        vy: i64,
        single: bool,
    ) {
        let bx = (x >> BS) as i32;
        let by = (y >> BS) as i32;
        let g = map.nav_ground(bx, by, layer);
        if matches!(g, ground::WATER | ground::BUILDING | ground::AIR) {
            return;
        }
        let ang = if vx.abs() + vy.abs() > (1 << 14) {
            (dir16((vx >> 8) as i32, (vy >> 8) as i32) >> 16) & 255
        } else {
            heading & 255
        };
        if single {
            self.put_mark(x, y, layer, ang, true);
        } else {
            let half_w = i64::from((wid / 2).max(4));
            let rx = i64::from(cos(heading)) * half_w * 4;
            let ry = i64::from(sin(heading)) * half_w * 4;
            self.put_mark(x - rx, y - ry, layer, ang, true);
            self.put_mark(x + rx, y + ry, layer, ang, true);
        }
    }

    /// Emit tyre smoke on a valid ground surface (on half-rate ticks).
    pub fn smoke(&mut self, map: &SimMap, x: i64, y: i64, layer: i32) {
        if self.half {
            return;
        }
        let bx = (x >> BS) as i32;
        let by = (y >> BS) as i32;
        let g = map.nav_ground(bx, by, layer);
        if matches!(g, ground::WATER | ground::BUILDING | ground::AIR) {
            return;
        }
        self.put_smoke(x, y, layer);
    }

    fn cloud_poll(&mut self, w: &Weapons) {
        for (i, b) in w.blasts.iter().enumerate() {
            if b.frame == 0 {
                self.cloud_seen[i] = false;
                continue;
            }
            if !self.cloud_seen[i] && b.frame >= CLOUD_AT_FRAME {
                self.cloud_seen[i] = true;
                self.cloud_put(b.x, b.y, b.layer, false);
            }
        }
    }

    fn blood_peds(&mut self, peds: &mut Peds) {
        for i in 0..peds.p.len() {
            if !peds.p[i].alive {
                continue;
            }
            let x = peds.p[i].x;
            let y = peds.p[i].y;
            let layer = peds.p[i].layer;
            if peds.p[i].corpse || peds.p[i].shot != 0 {
                if !peds.p[i].bled && peds.p[i].corpse {
                    peds.p[i].bled = true;
                    self.pool(x, y, layer);
                }
                continue;
            }
            if peds.p[i].blood_steps > 0 {
                let d = (x - peds.p[i].blood_x).abs() + (y - peds.p[i].blood_y).abs();
                if d >= (6 << FP) {
                    peds.p[i].blood_x = x;
                    peds.p[i].blood_y = y;
                    peds.p[i].blood_steps -= 1;
                    let angle = peds.p[i].angle;
                    self.trail(x, y, layer, angle);
                }
            }
            for s in 0..MAX_STAINS {
                let st = self.stains[s];
                if st.left <= 0 || st.kind != StainKind::Pool || st.layer != layer {
                    continue;
                }
                if st.left > POOL_SPLAT_AT {
                    continue;
                }
                if (st.x - x).abs() < BLOOD_REACH && (st.y - y).abs() < BLOOD_REACH {
                    if peds.p[i].blood_steps == 0 {
                        let angle = peds.p[i].angle;
                        self.splat(st.x, st.y, layer, angle);
                    }
                    peds.p[i].blood_steps = 8;
                    peds.p[i].blood_x = x;
                    peds.p[i].blood_y = y;
                    break;
                }
            }
        }
    }

    pub fn tick(&mut self, map: &SimMap, peds: &mut Peds, weapons: &Weapons) {
        self.cloud_poll(weapons);
        self.half = !self.half;
        if self.half {
            return;
        }
        self.blood_peds(peds);
        for m in &mut self.marks {
            if m.left > 0 {
                m.left -= 1;
                if m.left == 0 {
                    let c = m.cell as usize;
                    self.mark_cell[c] = self.mark_cell[c].saturating_sub(1);
                }
            }
        }
        for m in &mut self.smokes {
            if m.left > 0 {
                m.left -= 1;
                if m.left == 0 {
                    let c = m.cell as usize;
                    self.smoke_cell[c] = self.smoke_cell[c].saturating_sub(1);
                }
            }
        }
        for s in &mut self.stains {
            if s.left > 0 {
                s.left -= 1;
            }
        }
        for c in &mut self.clouds {
            if c.left > 0 {
                c.left -= 1;
            }
        }
        for sp in &mut self.sparks {
            if sp.left <= 0 {
                continue;
            }
            let nx = sp.x + sp.vx;
            let ny = sp.y + sp.vy;
            let g = map.nav_ground((nx >> BS) as i32, (ny >> BS) as i32, sp.layer);
            if g == ground::BUILDING {
                sp.left = 0;
            } else {
                sp.x = nx;
                sp.y = ny;
                sp.left -= 1;
            }
        }
    }

    pub fn ground_sprites(&self) -> Vec<MarkSprite> {
        let mut out = Vec::new();
        for sp in &self.sparks {
            if sp.left > 0 {
                out.push(MarkSprite {
                    x: sp.x,
                    y: sp.y,
                    layer: sp.layer,
                    angle: sp.angle,
                    object: OBJ_SPARK,
                    frame: 0,
                });
            }
        }
        for k in 0..self.n_debris {
            let idx = (self.head_debris + MAX_DEBRIS - self.n_debris + k) % MAX_DEBRIS;
            let d = &self.debris[idx];
            out.push(MarkSprite {
                x: d.x,
                y: d.y,
                layer: d.layer,
                angle: d.angle,
                object: d.obj,
                frame: d.frame,
            });
        }
        for pass in 0..2 {
            for k in 0..MAX_STAINS {
                let idx = (self.head_stain + k) % MAX_STAINS;
                let s = &self.stains[idx];
                if s.left <= 0 {
                    continue;
                }
                if (pass == 0) == (s.kind == StainKind::Pool) {
                    continue;
                }
                let (obj, fr) = match s.kind {
                    StainKind::Pool => (
                        OBJ_POOL,
                        ((POOL_LIFE - s.left) / POOL_FRAME_TICKS).clamp(0, POOL_FRAMES - 1)
                            as usize,
                    ),
                    StainKind::Splat => (OBJ_SPLAT, 0),
                    StainKind::Trail => (OBJ_TRAIL, 0),
                };
                out.push(MarkSprite {
                    x: s.x,
                    y: s.y,
                    layer: s.layer,
                    angle: s.angle,
                    object: obj,
                    frame: fr,
                });
            }
        }
        let mut drawn_marks = 0;
        for k in 1..=MAX_MARKS {
            if drawn_marks >= 48 {
                break;
            }
            let idx = (self.head_mark + MAX_MARKS - k) % MAX_MARKS;
            let m = &self.marks[idx];
            if m.left <= 0 {
                continue;
            }
            let fr = ((MARK_LIFE - m.left) / MARK_FRAME_TICKS).clamp(0, MARK_FRAMES - 1) as usize;
            let obj = if m.bloody { OBJ_BLOODY } else { OBJ_MARK };
            out.push(MarkSprite {
                x: m.x,
                y: m.y,
                layer: m.layer,
                angle: m.angle,
                object: obj,
                frame: fr,
            });
            drawn_marks += 1;
        }
        for k in 0..MAX_SMOKES {
            let idx = (self.head_smoke + k) % MAX_SMOKES;
            let m = &self.smokes[idx];
            if m.left <= 0 {
                continue;
            }
            let fr =
                ((SMOKE_LIFE - m.left) / SMOKE_FRAME_TICKS).clamp(0, SMOKE_FRAMES - 1) as usize;
            out.push(MarkSprite {
                x: m.x,
                y: m.y,
                layer: m.layer,
                angle: 0,
                object: OBJ_SMOKE,
                frame: fr,
            });
        }
        out
    }

    pub fn air_sprites(&self) -> Vec<MarkSprite> {
        let mut out = Vec::new();
        for c in &self.clouds {
            if c.left <= 0 {
                continue;
            }
            let fr =
                ((CLOUD_LIFE - c.left) / CLOUD_FRAME_TICKS).clamp(0, CLOUD_FRAMES - 1) as usize;
            let obj = if c.splash { OBJ_SPLASH } else { OBJ_CLOUD };
            out.push(MarkSprite {
                x: c.x,
                y: c.y,
                layer: c.layer,
                angle: 0,
                object: obj,
                frame: fr,
            });
        }
        out
    }

    pub fn sim_hash(&self, h: &mut StateHasher) {
        h.bool(self.half);
        h.u32(self.spark_rng);
        for v in [
            self.stat_marks,
            self.stat_bloody,
            self.stat_smokes,
            self.stat_pools,
            self.stat_splats,
            self.stat_trails,
            self.stat_clouds,
            self.stat_splashes,
            self.stat_debris,
            self.stat_sparks,
        ] {
            h.bytes(&v.to_le_bytes());
        }
    }
}
