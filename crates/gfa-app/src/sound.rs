//! Bridges `gfa-sim`'s pure state and one-shot events to `gfa-audio`.

use gfa_assets::AssetSource;
use gfa_audio::Audio;
use gfa_audio::sfx::{engine_rate, positional};
use gfa_formats::cmp::Face;
use gfa_sim::audio::{AudioEvent, snd};
use gfa_sim::cars::Siren;
use gfa_sim::gfa_fixed::trig::{cos, sin};
use gfa_sim::map::SimMap;
use gfa_sim::world::{Doing, World};

/// Ten 50-frame AI horn patterns from `GTA8.EXE` (`0x1302b1`, AmiGTA1
/// `gta_main.c:10042`). Indexed by `[honk_pat - 1][49 - honk]`.
const HONK_ROWS: [&[u8; 50]; 10] = [
    b"########################################..........",
    b"##########....####....####....##########..........",
    b"##########.............#################..........",
    b"############...#####....################..........",
    b"######.....###################...#######..........",
    b"########################.....###########..........",
    b"#######....######....####...############..........",
    b"######....#############...........................",
    b"######...###..#######.............................",
    b"#############...###..########..###................",
];

/// Per-level transient state for continuous audio loops and voice cooldowns
/// (not sim state).
#[derive(Default)]
pub struct SoundState {
    was_driving: bool,
    bus_top_speed: i32,
    voice_lcg: u32,
    crash_dice: u32,
    shout_last: Option<u8>,
    shout_cool: [i32; 3],
    yell_cool: i32,
    scream_turn: u8,
    scream_cool: i32,
}

impl SoundState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Original GTA voice LCG: `(x * 0x79 + 1) % 0x800`.
    fn voice_rand(&mut self) -> u32 {
        self.voice_lcg = (self.voice_lcg.wrapping_mul(0x79).wrapping_add(1)) % 0x800;
        self.voice_lcg
    }

    fn crash_roll_over_half(&mut self) -> bool {
        if self.crash_dice == 0 {
            self.crash_dice = 1;
        }
        self.crash_dice = self
            .crash_dice
            .wrapping_mul(1_103_515_245)
            .wrapping_add(12_345);
        ((self.crash_dice >> 16) % 100) > 50
    }
}

/// Drive in-game SFX, voices, continuous vehicle/siren/wreck loops, and radio
/// for the current frame.
pub fn tick(
    audio: &mut Audio,
    state: &mut SoundState,
    world: &mut World,
    src: Option<&dyn AssetSource>,
    dt_secs: f64,
    paused: bool,
    radio_next: bool,
) {
    let [lx, ly, _] = world.player_pos();

    // 1. Drain one-shot SFX and VOCALCOM speech emitted by the simulation.
    for ev in world.drain_audio() {
        match ev {
            AudioEvent::Sfx { id, x, y, layer } => {
                let cov = covered(&world.map, x, y, layer);
                if let Some((gain, pan)) = positional(x, y, lx, ly, cov) {
                    audio.play_level_sfx(id, gain, pan, None);
                }
            }
            AudioEvent::Voice { line, pos, cut } => match pos {
                None => audio.speak(line, 1.0, 0.0, cut),
                Some((x, y, layer)) => {
                    let cov = covered(&world.map, x, y, layer);
                    if let Some((gain, pan)) = positional(x, y, lx, ly, cov) {
                        audio.speak(line, gain, pan, cut);
                    }
                }
            },
            AudioEvent::FootShout { x, y, layer } => {
                let id = snd::FOOT_SHOUT_BASE + (state.voice_rand() & 1) as u8;
                let cov = covered(&world.map, x, y, layer);
                if let Some((gain, pan)) = positional(x, y, lx, ly, cov) {
                    audio.play_level_sfx(id, gain, pan, None);
                }
            }
            AudioEvent::CopShout {
                kind,
                ped,
                x,
                y,
                layer,
            } => {
                if audio.is_speaking() || state.shout_last == Some(kind) {
                    continue;
                }
                let k = kind.wrapping_sub(0x12) as usize;
                if kind != 0x15 {
                    if k > 2 {
                        continue;
                    }
                    let c = state.shout_cool[k];
                    state.shout_cool[k] = c - 1;
                    if c > 0 {
                        continue;
                    }
                }
                let cov = covered(&world.map, x, y, layer);
                if let Some((gain, pan)) = positional(x, y, lx, ly, cov) {
                    let line = kind.saturating_add((ped % 3) * 4);
                    audio.speak(line, gain, pan, false);
                    state.shout_last = Some(kind);
                    if kind != 0x15 {
                        state.shout_cool[k] = 50;
                    }
                }
            }
            AudioEvent::StreetYell {
                x,
                y,
                layer,
                chance_half,
            } => {
                if chance_half && !state.crash_roll_over_half() {
                    continue;
                }
                let r = state.voice_rand();
                if audio.is_speaking() {
                    continue;
                }
                let c = state.yell_cool;
                state.yell_cool = c - 1;
                if c >= 0 {
                    continue;
                }
                let cov = covered(&world.map, x, y, layer);
                if let Some((gain, pan)) = positional(x, y, lx, ly, cov) {
                    audio.speak(30 + (r % 21) as u8, gain, pan, false);
                    state.yell_cool = (r % 6 + 4) as i32;
                }
            }
            AudioEvent::Panic { x, y, layer, first } => {
                let r = state.voice_rand();
                let cov = covered(&world.map, x, y, layer);
                let pos = positional(x, y, lx, ly, cov);
                if first && let Some((gain, pan)) = pos {
                    audio.play_level_sfx(snd::SCREAM_HIGH - state.scream_turn, gain, pan, None);
                }
                state.scream_turn = (state.scream_turn + 1) % 3;
                if !audio.is_speaking() {
                    let c = state.scream_cool;
                    state.scream_cool = c - 1;
                    if c < 0
                        && let Some((gain, pan)) = pos
                    {
                        audio.speak(62 + (r % 8) as u8, gain, pan, false);
                        state.scream_cool = 3;
                    }
                }
            }
            AudioEvent::FrenzyCheer => {
                let r = state.voice_rand();
                let line = 7 + (r & 3) as u8;
                let gain = (60 + (r % 40)) as f32 / 127.0;
                audio.speak(line, gain, 0.0, false);
            }
        }
    }

    if paused {
        audio.stop_loops();
        return;
    }

    // 2. Continuous vehicle loops: engine note, tyre skid, bus air brake, horn.
    let driving = world.doing == Doing::Driving;
    if driving && let Some(d) = &world.driven {
        let ci = &world.cars[d.veh.model];
        let v = signed_speed(d.veh.vx, d.veh.vy, d.veh.angle());
        let abs_v = v.abs();
        let engine_sample = snd::ENGINE_BASE.saturating_add(ci.engine);
        let (hz, gain) = engine_rate(ci.sound_function, engine_sample, v, d.air.is_some());
        audio.set_engine(Some(engine_sample), hz, gain);

        // Tyre skid.
        let bus = ci.sound_function == 5;
        let mut x = if d.veh.skid_level > 0 {
            20 * d.veh.skid / d.veh.skid_level
        } else {
            0
        };
        if d.veh.sliding && abs_v > 2 && x < 14 {
            x = 21;
        }
        if bus {
            x /= 2;
        }
        let skidding = d.air.is_none() && ((bus && abs_v > 14 && x > 20) || (!bus && x > 12));
        if skidding {
            let shz = 8000.0 + 150.0 * (x as f64);
            audio.set_skid(Some(snd::SKID), shz, (gain * 0.85).clamp(0.15, 0.7));
        } else {
            audio.set_skid(None, 8000.0, 0.0);
        }

        // Bus air brake on coming to a stop from above speed 4.
        if bus {
            if abs_v > state.bus_top_speed {
                state.bus_top_speed = abs_v;
            }
            if abs_v == 0 && state.bus_top_speed > 4 {
                audio.play_level_sfx(snd::AIR_BRAKE, 0.75, 0.0, None);
                state.bus_top_speed = 0;
            }
        } else {
            state.bus_top_speed = 0;
        }

        // Player horn (models with horn < 60; >= 60 use the siren).
        if d.horn && ci.horn < 60 {
            let n = snd::HORN_BASE + ci.horn / 10;
            let base = f64::from(audio.level_sample_rate(n).unwrap_or(11025));
            let hz = base + f64::from(ci.horn % 10) * 512.0;
            audio.set_horn(Some(n), hz, 0.85, 0.0);
        } else {
            drive_ai_horn(audio, world, lx, ly);
        }
    } else {
        state.bus_top_speed = 0;
        audio.set_engine(None, 18000.0, 0.0);
        audio.set_skid(None, 8000.0, 0.0);
        drive_ai_horn(audio, world, lx, ly);
    }

    // 3. Siren: player's own emergency vehicle takes priority, else nearest chasing cop.
    if driving
        && let Some(d) = &world.driven
        && d.siren != Siren::Off
    {
        let sample = if d.siren == Siren::Sounding {
            snd::SIREN_YELP
        } else {
            snd::SIREN
        };
        audio.set_siren(Some(sample), 0.85, 0.0);
    } else {
        let mut best: Option<(u8, f32, f32)> = None;
        for c in world.traffic.live() {
            if c.cop < 2 || c.wrecked {
                continue;
            }
            let cov = covered(&world.map, c.x as i32, c.y as i32, c.layer);
            if let Some((gain, pan)) = positional(c.x as i32, c.y as i32, lx, ly, cov)
                && best.is_none_or(|b| gain > b.1)
            {
                let sample = if c.lead_kind != 0 {
                    snd::SIREN_YELP
                } else {
                    snd::SIREN
                };
                best = Some((sample, gain * 0.85, pan));
            }
        }
        match best {
            Some((sample, gain, pan)) => audio.set_siren(Some(sample), gain, pan),
            None => audio.set_siren(None, 0.0, 0.0),
        }
    }

    // 4. Nearest burning wreck crackle.
    let mut best_wreck: Option<(f32, f32)> = None;
    for c in world.traffic.live() {
        if !(c.wrecked || c.fuse > 0 || c.fire_ticks > 0) {
            continue;
        }
        let cov = covered(&world.map, c.x as i32, c.y as i32, c.layer);
        if let Some((gain, pan)) = positional(c.x as i32, c.y as i32, lx, ly, cov)
            && best_wreck.is_none_or(|b| gain > b.0)
        {
            best_wreck = Some((gain * 0.7, pan));
        }
    }
    match best_wreck {
        Some((gain, pan)) => audio.set_wreck_fire(Some(snd::WRECK_FIRE), gain, pan),
        None => audio.set_wreck_fire(None, 0.0, 0.0),
    }

    // 5. Clock-based CD-audio radio.
    audio.radio.advance(dt_secs);
    if let Some(src) = src {
        if driving && !state.was_driving {
            if let Some(d) = &world.driven {
                let ci = &world.cars[d.veh.model];
                let is_police = gfa_sim::cars::has_siren(ci.model);
                if let Some((path, start)) = audio
                    .radio
                    .station_for_car(ci.radio, is_police)
                    .map(|(s, t)| (s.path.clone(), t))
                {
                    audio.play_radio_file(src, &path, start);
                }
            }
        } else if !driving && state.was_driving && audio.radio.music_mode == 0 {
            audio.radio.stop();
            audio.stop_music();
        } else if audio.radio.music_mode == 1
            && let Some((path, start)) = audio
                .radio
                .ensure_constant()
                .map(|(s, t)| (s.path.clone(), t))
        {
            audio.play_radio_file(src, &path, start);
        }

        if radio_next
            && (driving || audio.radio.music_mode == 1)
            && let Some((path, start)) =
                audio.radio.next_station().map(|(s, t)| (s.path.clone(), t))
        {
            audio.play_radio_file(src, &path, start);
        }
    }
    state.was_driving = driving;
}

/// Drive the horn loop from the nearest honking AI car within 16 blocks whose
/// 50-frame pattern (`HONK_ROWS`) is currently active.
fn drive_ai_horn(audio: &mut Audio, world: &World, lx: i32, ly: i32) {
    struct AiHorn {
        sample: u8,
        hz: f64,
        gain: f32,
        pan: f32,
    }
    // Track nearest honking car by raw proximity gain so a gap in the closest
    // car's pattern isn't overridden by a farther car.
    let mut best_dist: f32 = 0.0;
    let mut best: Option<AiHorn> = None;
    for c in world.traffic.live() {
        if c.honk == 0 || c.honk > 40 || c.honk_pat == 0 || c.honk_pat > 10 {
            continue;
        }
        let Some(ci) = world.cars.get(c.model) else {
            continue;
        };
        if ci.horn >= 60 {
            continue;
        }
        let cov = covered(&world.map, c.x as i32, c.y as i32, c.layer);
        if let Some((gain, pan)) = positional(c.x as i32, c.y as i32, lx, ly, cov)
            && gain > best_dist
        {
            best_dist = gain;
            let on = HONK_ROWS[usize::from(c.honk_pat - 1)][usize::from(49 - c.honk)] == b'#';
            best = if on {
                let sample = snd::HORN_BASE + ci.horn / 10;
                let base = f64::from(audio.level_sample_rate(sample).unwrap_or(11025));
                let hz = base + f64::from(ci.horn % 10) * 512.0;
                Some(AiHorn {
                    sample,
                    hz,
                    gain: gain * 0.85,
                    pan,
                })
            } else {
                None
            };
        }
    }
    match best {
        Some(h) => audio.set_horn(Some(h.sample), h.hz, h.gain, h.pan),
        None => audio.set_horn(None, 11025.0, 0.0, 0.0),
    }
}

/// Signed speed in original units (half a sim pixel per physics step), positive
/// when moving along the nose and negative in reverse.
fn signed_speed(vx: i64, vy: i64, angle: i32) -> i32 {
    let ax = vx.abs();
    let ay = vy.abs();
    let mag = if ax > ay { ax + ay / 2 } else { ay + ax / 2 };
    let v = (mag / 32768) as i32;
    let fx = i64::from(sin(angle));
    let fy = -i64::from(cos(angle));
    if (vx >> 8) * fx + (vy >> 8) * fy < 0 {
        -v
    } else {
        v
    }
}

/// Roof/bridge/tunnel cover test (AmiGTA1 `covered()`): true when the block at
/// `layer` has a non-flat, non-slope lid or any block above has a non-flat lid.
fn covered(map: &SimMap, x: i32, y: i32, layer: i32) -> bool {
    let bx = (x >> 21).clamp(0, 255) as usize;
    let by = (y >> 21).clamp(0, 255) as usize;
    let cmp = map.cmp();
    let z0 = layer.clamp(0, 5) as usize;
    if let Some(b) = cmp.block(bx, by, z0)
        && b.face(Face::Lid) != 0
        && (b.type_map & 0x3f80) == 0
    {
        return true;
    }
    for z in (z0 + 1)..6 {
        if let Some(b) = cmp.block(bx, by, z)
            && b.face(Face::Lid) != 0
            && (b.type_map & 0x0080) == 0
        {
            return true;
        }
    }
    false
}
