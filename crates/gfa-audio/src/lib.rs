//! Audio: SFX banks, front-end audio, positional in-game SFX, continuous
//! vehicle/siren/wreck loops, `VOCALCOM` speech, and clock-based CD-audio radio.
//!
//! Uses Kira on both native (cpal) and WebAssembly (cpal Web Audio). On the
//! web, browsers suspend audio output until the first user gesture, so
//! [`Audio::new`] defers opening the backend until [`Audio::unlock`] is called.

pub mod bank;
pub mod frontend;
pub mod radio;
pub mod sfx;
#[cfg(target_arch = "wasm32")]
mod web_music;

use std::io::Cursor;
use std::time::Duration;

use gfa_assets::{AssetSource, Dataset};
#[cfg(not(target_arch = "wasm32"))]
use kira::sound::FromFileError;
use kira::sound::PlaybackState;
#[cfg(target_arch = "wasm32")]
use kira::sound::static_sound::StaticSoundData;
use kira::sound::static_sound::StaticSoundHandle;
#[cfg(not(target_arch = "wasm32"))]
use kira::sound::streaming::{StreamingSoundData, StreamingSoundHandle};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Tween};

pub use bank::SfxBank;
pub use frontend::MenuSfx;
pub use radio::Radio;

const VOLUME_TWEEN: Tween = Tween {
    start_time: kira::StartTime::Immediate,
    duration: Duration::from_millis(20),
    easing: kira::Easing::Linear,
};

const STOP_TWEEN: Tween = Tween {
    start_time: kira::StartTime::Immediate,
    duration: Duration::from_millis(120),
    easing: kira::Easing::Linear,
};

const LOOP_STOP_TWEEN: Tween = Tween {
    start_time: kira::StartTime::Immediate,
    duration: Duration::from_millis(40),
    easing: kira::Easing::Linear,
};

#[cfg(not(target_arch = "wasm32"))]
type MusicHandle = StreamingSoundHandle<FromFileError>;
#[cfg(target_arch = "wasm32")]
type MusicHandle = StaticSoundHandle;

struct LoopingVoice {
    sample: u8,
    base_rate: f64,
    handle: StaticSoundHandle,
}

struct Backend {
    _manager: AudioManager<DefaultBackend>,
    sfx: TrackHandle,
    music: TrackHandle,
    music_handle: Option<MusicHandle>,
    #[cfg(target_arch = "wasm32")]
    web_music: Option<web_music::WebMusic>,
    voice_handle: Option<StaticSoundHandle>,
    engine: Option<LoopingVoice>,
    siren: Option<LoopingVoice>,
    horn: Option<LoopingVoice>,
    skid: Option<LoopingVoice>,
    wreck: Option<LoopingVoice>,
}

impl Backend {
    fn open(sound_volume: u8, music_volume: u8) -> Option<Self> {
        let mut manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
        {
            Ok(m) => m,
            Err(e) => {
                log::warn!("audio unavailable: {e}");
                return None;
            }
        };
        let sfx = match manager.add_sub_track(TrackBuilder::new().volume(volume_db(sound_volume))) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("could not create SFX track: {e}");
                return None;
            }
        };
        let music = match manager.add_sub_track(TrackBuilder::new().volume(volume_db(music_volume)))
        {
            Ok(t) => t,
            Err(e) => {
                log::warn!("could not create music track: {e}");
                return None;
            }
        };
        Some(Self {
            _manager: manager,
            sfx,
            music,
            music_handle: None,
            #[cfg(target_arch = "wasm32")]
            web_music: web_music::WebMusic::new(music_volume),
            voice_handle: None,
            engine: None,
            siren: None,
            horn: None,
            skid: None,
            wreck: None,
        })
    }

    fn stop_music(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(wm) = self.web_music.as_mut() {
            wm.stop();
        }
        if let Some(mut handle) = self.music_handle.take() {
            handle.stop(STOP_TWEEN);
        }
    }

    fn stop_loops(&mut self) {
        for slot in [
            &mut self.engine,
            &mut self.siren,
            &mut self.horn,
            &mut self.skid,
            &mut self.wreck,
        ] {
            if let Some(mut v) = slot.take() {
                v.handle.stop(LOOP_STOP_TWEEN);
            }
        }
    }

    fn play_music_ogg(&mut self, bytes: Vec<u8>, looping: bool, start_secs: f64) {
        self.stop_music();
        #[cfg(target_arch = "wasm32")]
        if let Some(wm) = self.web_music.as_mut()
            && wm.play_ogg(&bytes, looping, start_secs)
        {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let data = match StreamingSoundData::from_cursor(Cursor::new(bytes)) {
            Ok(d) => {
                let dur = d.duration().as_secs_f64();
                let pos = if dur > 0.5 && start_secs > 0.0 {
                    start_secs % dur
                } else {
                    0.0
                };
                let d = if pos > 0.0 { d.start_position(pos) } else { d };
                if looping { d.loop_region(..) } else { d }
            }
            Err(e) => {
                log::warn!("could not decode music track: {e}");
                return;
            }
        };
        #[cfg(target_arch = "wasm32")]
        let data = match StaticSoundData::from_cursor(Cursor::new(bytes)) {
            Ok(d) => {
                let dur = d.duration().as_secs_f64();
                let pos = if dur > 0.5 && start_secs > 0.0 {
                    start_secs % dur
                } else {
                    0.0
                };
                let d = if pos > 0.0 { d.start_position(pos) } else { d };
                if looping { d.loop_region(..) } else { d }
            }
            Err(e) => {
                log::warn!("could not decode music track: {e}");
                return;
            }
        };
        match self.music.play(data) {
            Ok(handle) => self.music_handle = Some(handle),
            Err(e) => log::warn!("could not play music track: {e}"),
        }
    }
}

/// The game's audio engine. Safe to call even when no audio device is present.
pub struct Audio {
    backend: Option<Backend>,
    /// True when the engine is intentionally silent (tests, `--screenshot`).
    silent: bool,
    /// True once we have tried to open the backend (so we don't retry every frame).
    tried_open: bool,
    sound_volume: u8,
    music_volume: u8,
    frontend_bank: Option<SfxBank>,
    level_bank: Option<SfxBank>,
    voice_bank: Option<SfxBank>,
    pub radio: Radio,
    /// Music queued before the web AudioContext was unlocked by a user gesture.
    pending_music: Option<(Vec<u8>, bool, f64)>,
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Audio {
    /// Create the audio engine. Opens the device immediately on desktop; on
    /// the web waits for [`Self::unlock`] on the first user gesture.
    pub fn new() -> Self {
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut audio = Self {
            backend: None,
            silent: false,
            tried_open: false,
            sound_volume: 7,
            music_volume: 6,
            frontend_bank: None,
            level_bank: None,
            voice_bank: None,
            radio: Radio::new(),
            pending_music: None,
        };
        #[cfg(not(target_arch = "wasm32"))]
        audio.ensure_open();
        audio
    }

    /// A silent audio engine that never opens an output device.
    pub fn silent() -> Self {
        Self {
            backend: None,
            silent: true,
            tried_open: true,
            sound_volume: 0,
            music_volume: 0,
            frontend_bank: None,
            level_bank: None,
            voice_bank: None,
            radio: Radio::new(),
            pending_music: None,
        }
    }

    /// Call on any user gesture (key press, pointer click) to open audio on
    /// the web and start any music that was queued before the gesture.
    pub fn unlock(&mut self) {
        if self.silent {
            return;
        }
        self.ensure_open();
        if let (Some(backend), Some((bytes, looping, start_secs))) =
            (self.backend.as_mut(), self.pending_music.take())
        {
            backend.play_music_ogg(bytes, looping, start_secs);
        }
    }

    fn ensure_open(&mut self) {
        if self.silent || self.tried_open {
            return;
        }
        self.tried_open = true;
        self.backend = Backend::open(self.sound_volume, self.music_volume);
    }

    /// Set the SFX volume (`0..=7`, matching `PLAYER_A.DAT`).
    pub fn set_sound_volume(&mut self, vol: u8) {
        let vol = vol.min(7);
        if self.sound_volume == vol {
            return;
        }
        self.sound_volume = vol;
        if let Some(b) = self.backend.as_mut() {
            b.sfx.set_volume(volume_db(vol), VOLUME_TWEEN);
        }
    }

    /// Set the music volume (`0..=7`, matching `PLAYER_A.DAT`).
    pub fn set_music_volume(&mut self, vol: u8) {
        let vol = vol.min(7);
        if self.music_volume == vol {
            return;
        }
        self.music_volume = vol;
        if let Some(b) = self.backend.as_mut() {
            b.music.set_volume(volume_db(vol), VOLUME_TWEEN);
            #[cfg(target_arch = "wasm32")]
            if let Some(wm) = b.web_music.as_mut() {
                wm.set_volume(vol);
            }
        }
    }

    /// Load the front-end sound bank (`LEVEL000`) and start the looping title
    /// theme (`Track03.ogg`).
    pub fn start_menu(&mut self, src: &dyn AssetSource, dataset: Dataset, sound: u8, music: u8) {
        self.stop_loops();
        self.set_sound_volume(sound);
        self.set_music_volume(music);
        let bank_path = format!("{}/AUDIO/LEVEL000", dataset.base_dir());
        match SfxBank::load(src, &bank_path) {
            Ok(bank) => {
                log::info!("loaded {bank_path}: {} samples", bank.len());
                self.frontend_bank = Some(bank);
            }
            Err(e) => {
                log::warn!("front-end sound bank unavailable ({bank_path}): {e}");
                self.frontend_bank = None;
            }
        }
        if let Some(bytes) = frontend::load_menu_music(src, dataset) {
            self.play_music_ogg(bytes, true, 0.0);
        } else {
            log::info!("no title music found in {}", dataset.music_dir());
        }
    }

    /// Load the in-game level SFX bank (`LEVEL00x`), `VOCALCOM`, and radio tracks.
    pub fn load_level(
        &mut self,
        src: &dyn AssetSource,
        dataset: Dataset,
        sample_number: u8,
        music_mode: u8,
    ) {
        self.stop_loops();
        self.stop_music();
        let base = dataset.base_dir();
        let num = sample_number.clamp(1, 3);
        let bank_path = format!("{base}/AUDIO/LEVEL{num:03}");
        self.level_bank = SfxBank::load(src, &bank_path)
            .or_else(|_| SfxBank::load(src, &format!("{base}/AUDIO/LEVEL001")))
            .inspect(|b| log::info!("loaded {bank_path}: {} samples", b.len()))
            .inspect_err(|e| log::warn!("level sound bank unavailable ({bank_path}): {e}"))
            .ok();
        let voice_path = format!("{base}/AUDIO/VOCALCOM");
        self.voice_bank = SfxBank::load(src, &voice_path)
            .inspect(|b| log::info!("loaded {voice_path}: {} lines", b.len()))
            .inspect_err(|e| log::warn!("voice bank unavailable ({voice_path}): {e}"))
            .ok();
        self.radio.music_mode = music_mode.min(1);
        self.radio.load(src, dataset);
    }

    /// Play a front-end sound effect from `LEVEL000`.
    pub fn play_menu(&mut self, sfx: MenuSfx) {
        if self.silent {
            return;
        }
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let Some(data) = self.frontend_bank.as_ref().and_then(|b| b.get(sfx.index())) else {
            return;
        };
        if let Err(e) = backend.sfx.play(data.clone()) {
            log::debug!("could not play menu sfx {:?}: {e}", sfx);
        }
    }

    /// Play a one-shot sound effect from the level bank (`LEVEL001`–`003`).
    pub fn play_level_sfx(&mut self, id: u8, gain: f32, pan: f32, rate_hz: Option<u32>) {
        if self.silent || gain <= 0.01 {
            return;
        }
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let Some(base) = self
            .level_bank
            .as_ref()
            .and_then(|b| b.get(usize::from(id)))
        else {
            return;
        };
        let mut data = base.volume(gain_db(gain)).panning(pan.clamp(-1.0, 1.0));
        if let Some(hz) = rate_hz
            && base.sample_rate > 0
        {
            data = data.playback_rate(f64::from(hz) / f64::from(base.sample_rate));
        }
        if let Err(e) = backend.sfx.play(data) {
            log::debug!("could not play level sfx {id}: {e}");
        }
    }

    /// Whether a `VOCALCOM` speech line is currently playing.
    pub fn is_speaking(&self) -> bool {
        self.backend.as_ref().is_some_and(|b| {
            b.voice_handle
                .as_ref()
                .is_some_and(|h| h.state() == PlaybackState::Playing)
        })
    }

    /// Say a line from `VOCALCOM`. One voice plays at a time: `cut == true`
    /// interrupts the current line, `cut == false` drops if one is playing.
    pub fn speak(&mut self, line: u8, gain: f32, pan: f32, cut: bool) {
        if self.silent || gain <= 0.01 {
            return;
        }
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        if let Some(h) = backend.voice_handle.as_mut()
            && h.state() == PlaybackState::Playing
        {
            if !cut {
                return;
            }
            h.stop(LOOP_STOP_TWEEN);
        }
        let Some(base) = self
            .voice_bank
            .as_ref()
            .and_then(|b| b.get(usize::from(line)))
        else {
            return;
        };
        let data = base.volume(gain_db(gain)).panning(pan.clamp(-1.0, 1.0));
        match backend.sfx.play(data) {
            Ok(h) => backend.voice_handle = Some(h),
            Err(e) => log::debug!("could not play voice {line}: {e}"),
        }
    }

    /// Update the continuous looping engine note. Pass `None` to stop it.
    pub fn set_engine(&mut self, sample: Option<u8>, rate_hz: f64, gain: f32) {
        self.update_loop(LoopKind::Engine, sample, Some(rate_hz), gain, 0.0);
    }

    /// Update the continuous looping siren (player or nearest chasing cop).
    pub fn set_siren(&mut self, sample: Option<u8>, gain: f32, pan: f32) {
        self.update_loop(LoopKind::Siren, sample, None, gain, pan);
    }

    /// Update the continuous looping car horn (while held).
    pub fn set_horn(&mut self, sample: Option<u8>, rate_hz: f64, gain: f32, pan: f32) {
        self.update_loop(LoopKind::Horn, sample, Some(rate_hz), gain, pan);
    }

    /// Update the continuous looping tyre skid.
    pub fn set_skid(&mut self, sample: Option<u8>, rate_hz: f64, gain: f32) {
        self.update_loop(LoopKind::Skid, sample, Some(rate_hz), gain, 0.0);
    }

    /// Update the continuous burning-wreck crackle.
    pub fn set_wreck_fire(&mut self, sample: Option<u8>, gain: f32, pan: f32) {
        self.update_loop(LoopKind::Wreck, sample, None, gain, pan);
    }

    fn update_loop(
        &mut self,
        kind: LoopKind,
        sample: Option<u8>,
        rate_hz: Option<f64>,
        gain: f32,
        pan: f32,
    ) {
        if self.silent {
            return;
        }
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let slot = match kind {
            LoopKind::Engine => &mut backend.engine,
            LoopKind::Siren => &mut backend.siren,
            LoopKind::Horn => &mut backend.horn,
            LoopKind::Skid => &mut backend.skid,
            LoopKind::Wreck => &mut backend.wreck,
        };
        let Some(id) = sample.filter(|_| gain > 0.01) else {
            if let Some(mut v) = slot.take() {
                v.handle.stop(LOOP_STOP_TWEEN);
            }
            return;
        };
        if let Some(v) = slot.as_mut()
            && v.sample == id
            && v.handle.state() == PlaybackState::Playing
        {
            v.handle.set_volume(gain_db(gain), VOLUME_TWEEN);
            v.handle.set_panning(pan.clamp(-1.0, 1.0), VOLUME_TWEEN);
            if let Some(hz) = rate_hz
                && v.base_rate > 0.0
            {
                v.handle
                    .set_playback_rate((hz / v.base_rate).clamp(0.25, 8.0), VOLUME_TWEEN);
            }
            return;
        }
        if let Some(mut old) = slot.take() {
            old.handle.stop(LOOP_STOP_TWEEN);
        }
        let Some(base) = self
            .level_bank
            .as_ref()
            .and_then(|b| b.get(usize::from(id)))
        else {
            return;
        };
        let base_rate = f64::from(base.sample_rate.max(1));
        let mut data = base
            .loop_region(..)
            .volume(gain_db(gain))
            .panning(pan.clamp(-1.0, 1.0));
        if let Some(hz) = rate_hz {
            data = data.playback_rate((hz / base_rate).clamp(0.25, 8.0));
        }
        if let Ok(handle) = backend.sfx.play(data) {
            *slot = Some(LoopingVoice {
                sample: id,
                base_rate,
                handle,
            });
        }
    }

    /// Stop all continuous in-game loops (on pause or leaving a level).
    pub fn stop_loops(&mut self) {
        if let Some(b) = self.backend.as_mut() {
            b.stop_loops();
        }
    }

    /// Sample rate (Hz) of an entry in the loaded level bank, if available.
    pub fn level_sample_rate(&self, id: u8) -> Option<u32> {
        self.level_bank
            .as_ref()
            .and_then(|b| b.get(usize::from(id)))
            .map(|s| s.sample_rate)
    }

    /// Start playing an Ogg Vorbis music track (looping if `looping` is true).
    pub fn play_music_ogg(&mut self, bytes: Vec<u8>, looping: bool, start_secs: f64) {
        if self.silent {
            return;
        }
        if let Some(backend) = self.backend.as_mut() {
            self.pending_music = None;
            backend.play_music_ogg(bytes, looping, start_secs);
        } else if !self.tried_open {
            // Waiting for the browser's first user gesture.
            self.pending_music = Some((bytes, looping, start_secs));
        }
    }

    /// Read and play a radio station Ogg track at its live clock offset.
    pub fn play_radio_file(&mut self, src: &dyn AssetSource, path: &str, start_secs: f64) {
        match src.read(path) {
            Ok(bytes) => self.play_music_ogg(bytes, true, start_secs),
            Err(e) => log::debug!("radio track unavailable ({path}): {e}"),
        }
    }

    /// Stop the current music track.
    pub fn stop_music(&mut self) {
        self.pending_music = None;
        if let Some(backend) = self.backend.as_mut() {
            backend.stop_music();
        }
    }
}

#[derive(Clone, Copy)]
enum LoopKind {
    Engine,
    Siren,
    Horn,
    Skid,
    Wreck,
}

/// Map GTA's `0..=7` volume setting to decibels.
pub fn volume_db(level: u8) -> Decibels {
    match level {
        0 => Decibels::SILENCE,
        v => gain_db(f32::from(v.min(7)) / 7.0),
    }
}

/// Map a linear gain (`0.0..=1.0`) to decibels.
pub fn gain_db(gain: f32) -> Decibels {
    if gain <= 0.0001 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * gain.clamp(0.0001, 1.0).log10())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_curve_maps_zero_to_silence_and_seven_to_unity() {
        assert_eq!(volume_db(0), Decibels::SILENCE);
        assert!((volume_db(7).0 - 0.0).abs() < 1e-5);
        assert!(volume_db(1).0 < volume_db(4).0);
        assert!(volume_db(4).0 < volume_db(7).0);
        assert_eq!(volume_db(99), volume_db(7));
    }
}
