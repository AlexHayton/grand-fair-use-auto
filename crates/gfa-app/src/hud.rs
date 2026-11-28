//! Original in-game HUD: state, FXT text lookup, 50 Hz sim-clock timers, and
//! 2D layout for [`gfa_render::hud::HudRenderer`].
//!
//! Layout and timing follow AmiGTA1's `gta_main.c` (`text_displays_tick`,
//! `text_displays`, `hud_score`, MIT) scaled to GTA1's 640×480 hi-res HUD art
//! (`*2.FON` fonts and G24 hi-res `SpriteCategory::Arrow` sprites 25..49).

use gfa_assets::{AssetSource, Dataset};
use gfa_formats::fon::Font;
use gfa_formats::fxt::Fxt;
use gfa_render::hud::{HUD_VIRTUAL_H, HudBatch, HudFontKind, HudFonts, HudRenderer, HudSprite};
use gfa_sim::hud::{BriefKind, HudEvent, HudText};
use gfa_sim::score::crime;
use gfa_sim::world::{Card, World};

/// Pager LCD window inside the hi-res 160×60 pager sprite (2× AmiGTA1's
/// 80×30 measurements: x=9..71, y=6..19, LED at (11, 19)).
const PAGER_X: f32 = 6.0;
const PAGER_Y: f32 = 6.0;
const PAGER_WIN_X0: f32 = 18.0;
const PAGER_WIN_X1: f32 = 142.0;
const PAGER_WIN_Y0: f32 = 12.0;
const PAGER_WIN_Y1: f32 = 38.0;
const PAGER_LED_X: f32 = 22.0;
const PAGER_LED_Y: f32 = 38.0;

/// Duration of the car name plaque in 50 Hz sim ticks (45 original frames).
const CARNAME_TICKS: i32 = 90;

/// Load the in-game HUD fonts (`*2.FON` hi-res preferred, `*1.FON` fallback).
pub fn load_fonts(src: &dyn AssetSource, dataset: Dataset) -> HudFonts {
    let mut out = HudFonts::default();
    let dir = dataset.base_dir();
    let load = |candidates: &[(&str, f32)]| -> Option<(Font, f32)> {
        for &(name, scale) in candidates {
            for base in [dir, "GTADATA"] {
                let path = format!("{base}/{name}");
                if let Ok(bytes) = src.read(&path)
                    && let Ok(font) = Font::parse(&bytes)
                {
                    return Some((font, scale));
                }
            }
        }
        None
    };

    if let Some((f, s)) = load(&[("PAGER2.FON", 1.0), ("PAGER1.FON", 2.0)]) {
        out.set(HudFontKind::Pager, f, s);
    }
    if let Some((f, s)) = load(&[("SCORE2.FON", 1.0), ("SCORE1.FON", 2.0)]) {
        out.set(HudFontKind::Score, f, s);
    }
    if let Some((f, s)) = load(&[("MISSMUL2.FON", 1.0), ("MISSMUL1.FON", 2.0)]) {
        out.set(HudFontKind::Mult, f, s);
    }
    if let Some((f, s)) = load(&[("BIG2.FON", 1.0), ("BIG1.FON", 2.0)]) {
        out.set(HudFontKind::Big, f, s);
    }
    if let Some((f, s)) = load(&[
        ("SUB2.FON", 1.0),
        ("SUB1.FON", 2.0),
        ("PAGER2.FON", 1.0),
        ("PAGER1.FON", 2.0),
    ]) {
        out.set(HudFontKind::Sub, f, s);
    }
    if let Some((f, s)) = load(&[
        ("CARNAME2.FON", 1.0),
        ("CARNAME1.FON", 2.0),
        ("SUB2.FON", 1.0),
        ("SUB1.FON", 2.0),
        ("PAGER2.FON", 1.0),
        ("PAGER1.FON", 2.0),
    ]) {
        out.set(HudFontKind::CarName, f, s);
    }
    out
}

/// Load the default English text table for `dataset`.
pub fn load_fxt(src: &dyn AssetSource, dataset: Dataset) -> Option<Fxt> {
    let candidates: &[&str] = match dataset {
        Dataset::Gta => &["GTADATA/ENGLISH.FXT"],
        Dataset::London69 => &["GTADATA/UK/ENGUK.FXT", "GTADATA/ENGLISH.FXT"],
        Dataset::London61 => &[
            "GTADATA/UK/ENGUKE.FXT",
            "GTADATA/UK/ENGUK.FXT",
            "GTADATA/ENGLISH.FXT",
        ],
    };
    for &path in candidates {
        if let Ok(bytes) = src.read(path)
            && let Ok(fxt) = Fxt::parse(&bytes)
        {
            return Some(fxt);
        }
    }
    None
}

/// Persistent HUD state, ticked on the 50 Hz simulation clock.
pub struct HudState {
    fxt: Option<Fxt>,
    /// Text speed option: 1 slow, 2 normal, 3 fast.
    pub text_speed: i32,
    // Pager scroll (top-left)
    pager_line: Vec<u8>,
    last_pager: Vec<u8>,
    pager_ticks: i32,
    pager_px: f32,
    pager_secs: Option<i32>,
    pager_led: u8,
    // Car name plaque (top-centre)
    carname_text: Vec<u8>,
    carname_ticks: i32,
    // Bottom-left brief box
    brief_text: Vec<u8>,
    last_brief: Vec<u8>,
    brief_kind: BriefKind,
    last_brief_kind: BriefKind,
    brief_ticks: i32,
    // Centred big card (transient messages like GOURANGA!, MISSION COMPLETE!)
    big_text: Vec<u8>,
    big_ticks: i32,
    // Cop head independent flash timers (4 heads)
    head_on: [bool; 4],
    head_t: [u8; 4],
    // Left-column indicator flash timer (armour, jail-free, speed/frenzy)
    ind_t: u8,
    ind_on: bool,
}

impl HudState {
    pub fn new(fxt: Option<Fxt>, text_speed: u8) -> Self {
        Self {
            fxt,
            text_speed: i32::from(text_speed.clamp(1, 3)),
            pager_line: Vec::new(),
            last_pager: Vec::new(),
            pager_ticks: 0,
            pager_px: 0.0,
            pager_secs: None,
            pager_led: 0,
            carname_text: Vec::new(),
            carname_ticks: 0,
            brief_text: Vec::new(),
            last_brief: Vec::new(),
            brief_kind: BriefKind::Info,
            last_brief_kind: BriefKind::Info,
            brief_ticks: 0,
            big_text: Vec::new(),
            big_ticks: 0,
            // Slightly staggered initial phases so the cop heads shimmer naturally.
            head_on: [false, true, false, true],
            head_t: [0, 1, 2, 3],
            ind_t: 0,
            ind_on: false,
        }
    }

    fn resolve_text(&self, text: &HudText) -> Vec<u8> {
        match text {
            HudText::Literal(s) => s.as_bytes().to_vec(),
            HudText::Key(k) => {
                if let Some(bytes) = self.fxt.as_ref().and_then(|f| f.get(k)) {
                    bytes.to_vec()
                } else {
                    fallback_key_text(k).as_bytes().to_vec()
                }
            }
        }
    }

    pub fn show_pager(&mut self, bytes: Vec<u8>, secs: Option<i32>) {
        if bytes.is_empty() {
            return;
        }
        self.last_pager = bytes.clone();
        self.pager_line = bytes;
        self.pager_ticks = 1;
        self.pager_px = 0.0;
        self.pager_secs = secs;
    }

    pub fn show_brief(&mut self, kind: BriefKind, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        if self.brief_ticks > 0 && self.brief_kind.is_high_priority() && !kind.is_high_priority() {
            return;
        }
        let mult = match self.text_speed {
            1 => 4,
            3 => 1,
            _ => 2,
        };
        self.brief_ticks = (bytes.len() as i32 + 24) * mult;
        self.brief_kind = kind;
        self.last_brief_kind = kind;
        self.last_brief = bytes.clone();
        self.brief_text = bytes;
    }

    pub fn show_big(&mut self, bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }
        self.big_ticks = (bytes.len() as i32 * 10).max(80);
        self.big_text = bytes;
    }

    /// F7: repeat the last pager message.
    pub fn repeat_pager(&mut self) {
        if !self.last_pager.is_empty() {
            let line = self.last_pager.clone();
            self.show_pager(line, None);
        }
    }

    /// F10: repeat the last brief message.
    pub fn repeat_brief(&mut self) {
        if !self.last_brief.is_empty() {
            let line = self.last_brief.clone();
            let kind = self.last_brief_kind;
            self.brief_ticks = 0;
            self.show_brief(kind, line);
        }
    }

    /// F9: show the player's current district name on the pager.
    pub fn show_area(&mut self, world: &World) {
        let [px, py, _] = world.player_pos();
        let bx = (px >> 21) as usize;
        let by = (py >> 21) as usize;
        if let Some(d) = world.map.cmp().districts.iter().rev().find(|d| {
            !d.is_empty()
                && (d.x as usize..d.x as usize + d.w as usize).contains(&bx)
                && (d.y as usize..d.y as usize + d.h as usize).contains(&by)
        }) {
            self.show_pager(d.name.as_bytes().to_vec(), None);
        }
    }

    /// Drain simulation HUD events and advance HUD timers by `sim_ticks` (50 Hz).
    pub fn tick(&mut self, world: &mut World, renderer: &HudRenderer, sim_ticks: u32) {
        for ev in world.drain_hud() {
            match ev {
                HudEvent::Pager { text, secs } => {
                    let bytes = self.resolve_text(&text);
                    self.show_pager(bytes, secs);
                }
                HudEvent::Brief { kind, text } => {
                    let bytes = self.resolve_text(&text);
                    self.show_brief(kind, bytes);
                }
                HudEvent::Big { text } => {
                    let bytes = self.resolve_text(&text);
                    self.show_big(bytes);
                }
                HudEvent::CarName { model } => {
                    let key = format!("car{model}");
                    if let Some(bytes) = self.fxt.as_ref().and_then(|f| f.get(&key)) {
                        self.carname_text = bytes.to_vec();
                        self.carname_ticks = CARNAME_TICKS;
                    }
                }
                HudEvent::BustCrimes { crimes } => {
                    if let Some(line) = self.format_bust_crimes(&crimes) {
                        self.show_pager(line, None);
                    }
                }
            }
        }

        let pager_wide = if self.pager_ticks > 0 && !self.pager_line.is_empty() {
            renderer.measure(HudFontKind::Pager, &self.pager_line)
        } else {
            0.0
        };
        let win_w = PAGER_WIN_X1 - PAGER_WIN_X0;

        for _ in 0..sim_ticks {
            if self.pager_ticks > 0 && !self.pager_line.is_empty() {
                self.pager_led = (self.pager_led + 1) % 16;
                self.pager_ticks += 1;
                if (self.pager_ticks & 1) != 0 {
                    // 1, 2, or 3 virtual pixels per 25 Hz original tick.
                    self.pager_px += self.text_speed as f32;
                    if self.pager_px > win_w + pager_wide {
                        self.pager_ticks = 0;
                    }
                    if let Some(s) = self.pager_secs.as_mut()
                        && *s > 0
                        && self.pager_ticks % 50 == 0
                    {
                        *s -= 1;
                    }
                }
            }
            if self.brief_ticks > 0 {
                self.brief_ticks -= 1;
            }
            if self.big_ticks > 0 {
                self.big_ticks -= 1;
            }
            if self.carname_ticks > 0 {
                self.carname_ticks -= 1;
            }
            if world.score.level > 0 {
                let n = (world.score.level as usize).min(4);
                for k in 0..n {
                    self.head_t[k] += 1;
                    if self.head_t[k] >= 4 {
                        self.head_t[k] = 0;
                        self.head_on[k] = !self.head_on[k];
                    }
                }
            }
            self.ind_t += 1;
            if self.ind_t >= 8 {
                self.ind_t = 0;
                self.ind_on = !self.ind_on;
            }
        }
    }

    fn format_bust_crimes(&self, crimes: &[u32; crime::COUNT]) -> Option<Vec<u8>> {
        let mut parts: Vec<String> = Vec::new();
        for (i, &count) in crimes.iter().enumerate() {
            if count == 0 {
                continue;
            }
            let key = format!("crime{i}");
            let label = self
                .fxt
                .as_ref()
                .and_then(|f| f.string(&key))
                .unwrap_or_else(|| default_crime_name(i).to_string());
            if label.is_empty() {
                continue;
            }
            parts.push(format!("{label}: {count}"));
        }
        if parts.is_empty() {
            return None;
        }
        Some(parts.join("  -  ").into_bytes())
    }

    /// Populate `batch` with every active HUD element for the current frame.
    pub fn build(
        &self,
        world: &World,
        renderer: &HudRenderer,
        batch: &mut HudBatch,
        vw: f32,
        paused: bool,
    ) {
        batch.clear();
        let vh = HUD_VIRTUAL_H;
        let right = vw - 8.0;
        let top = 6.0;

        // 1. Score (top-right, SCORE font) + Lives (immediately left, MISSMUL font)
        //    + Multiplier (under score, MISSMUL font).
        let score_digits = digits_of(world.score.score.max(0) as u64);
        let score_w = renderer.measure_indices(HudFontKind::Score, &score_digits);
        let score_h = renderer.font_height(HudFontKind::Score).unwrap_or(20.0);
        renderer.push_indices(
            batch,
            HudFontKind::Score,
            right - score_w,
            top,
            &score_digits,
            true,
        );

        let mut lives_glyphs = Vec::with_capacity(4);
        lives_glyphs.push(10); // boxed 'x'
        if world.lives == 255 {
            lives_glyphs.push(10);
            lives_glyphs.push(10);
        } else {
            let l = world.lives.max(0) as usize;
            if l >= 10 {
                lives_glyphs.push((l / 10) % 10);
            }
            lives_glyphs.push(l % 10);
        }
        let lives_w = renderer.measure_indices(HudFontKind::Mult, &lives_glyphs);
        renderer.push_indices(
            batch,
            HudFontKind::Mult,
            right - score_w - lives_w - 4.0,
            top + 2.0,
            &lives_glyphs,
            true,
        );

        let mut right_y = top + score_h + 3.0;
        if world.score.multiplier > 1 {
            let mut mult_glyphs = vec![10];
            mult_glyphs.extend(digits_of(world.score.multiplier as u64));
            let mw = renderer.measure_indices(HudFontKind::Mult, &mult_glyphs);
            let mh = renderer.font_height(HudFontKind::Mult).unwrap_or(13.0);
            renderer.push_indices(
                batch,
                HudFontKind::Mult,
                right - mw,
                right_y,
                &mult_glyphs,
                true,
            );
            right_y += mh + 4.0;
        }

        // Frenzy / endless weapon countdown (seconds remaining, right-aligned).
        if world.arms.inf_ticks > 0 {
            let secs = (world.arms.inf_ticks / 50).max(0) as u64;
            let digits = digits_of(secs);
            let dw = renderer.measure_indices(HudFontKind::Score, &digits);
            renderer.push_indices(
                batch,
                HudFontKind::Score,
                right - dw,
                right_y,
                &digits,
                true,
            );
        }

        // 2. Cop heads (top-centre, 1..=4 shimmering icons).
        let mut top_centre_y = 4.0f32;
        if world.score.level > 0 {
            let n = (world.score.level as usize).min(4);
            let [cw, ch] = renderer
                .sprite_size(HudSprite::CopHead(false))
                .unwrap_or([18.0, 18.0]);
            let step = cw + 3.0;
            let total_w = n as f32 * step - 3.0;
            let x0 = (vw - total_w) * 0.5;
            for (k, &on) in self.head_on.iter().enumerate().take(n) {
                renderer.push_sprite(
                    batch,
                    HudSprite::CopHead(on),
                    x0 + k as f32 * step,
                    top_centre_y,
                );
            }
            top_centre_y += ch + 4.0;
        }

        // 3. Car name plaque (top-centre, below cop heads if wanted).
        if self.carname_ticks > 0 && !self.carname_text.is_empty() {
            let [pw, ph] = renderer
                .sprite_size(HudSprite::CarPlaque)
                .unwrap_or([227.0, 34.0]);
            let px = (vw - pw) * 0.5;
            renderer.push_sprite(batch, HudSprite::CarPlaque, px, top_centre_y);
            let tw = renderer.measure(HudFontKind::CarName, &self.carname_text);
            let th = renderer.font_height(HudFontKind::CarName).unwrap_or(20.0);
            renderer.push_text(
                batch,
                HudFontKind::CarName,
                (vw - tw) * 0.5,
                top_centre_y + (ph - th) * 0.5 + 1.0,
                &self.carname_text,
                false,
                None,
            );
        }

        // 4. Pager scroll (top-left).
        let pager_active = self.pager_ticks > 0 && !self.pager_line.is_empty();
        if pager_active {
            renderer.push_sprite(batch, HudSprite::Pager, PAGER_X, PAGER_Y);
            let win_w = PAGER_WIN_X1 - PAGER_WIN_X0;
            let tx = PAGER_X + PAGER_WIN_X0 + win_w - self.pager_px;
            let ph = renderer.font_height(HudFontKind::Pager).unwrap_or(20.0);
            let ty = PAGER_Y + PAGER_WIN_Y0 + ((PAGER_WIN_Y1 - PAGER_WIN_Y0) - ph) * 0.5;
            renderer.push_text(
                batch,
                HudFontKind::Pager,
                tx,
                ty,
                &self.pager_line,
                true,
                Some([
                    PAGER_X + PAGER_WIN_X0,
                    PAGER_Y + PAGER_WIN_Y0,
                    PAGER_X + PAGER_WIN_X1,
                    PAGER_Y + PAGER_WIN_Y1,
                ]),
            );
            if (self.pager_led & 8) != 0 {
                renderer.push_sprite(
                    batch,
                    HudSprite::PagerLed,
                    PAGER_X + PAGER_LED_X,
                    PAGER_Y + PAGER_LED_Y,
                );
            }
            if let Some(secs) = self.pager_secs
                && secs >= 0
            {
                let digits = digits_of(secs as u64);
                let pager_w = renderer
                    .sprite_size(HudSprite::Pager)
                    .map_or(160.0, |s| s[0]);
                renderer.push_indices(
                    batch,
                    HudFontKind::Score,
                    PAGER_X + pager_w + 6.0,
                    PAGER_Y + 14.0,
                    &digits,
                    true,
                );
            }
        }

        // 5. Left column: weapon + ammo, armour, jail-free card, speed/frenzy.
        let left_x = 6.0f32;
        let mut left_y = if pager_active {
            let pager_h = renderer
                .sprite_size(HudSprite::Pager)
                .map_or(60.0, |s| s[1]);
            PAGER_Y + pager_h + 6.0
        } else {
            6.0
        };
        let w = world.arms.weapon;
        if (1..=4).contains(&w) {
            let sp = HudSprite::Weapon((w - 1) as u8);
            let [iw, ih] = renderer.sprite_size(sp).unwrap_or([64.0, 36.0]);
            renderer.push_sprite(batch, sp, left_x, left_y);
            let ammo = world.arms.ammo[w].max(0) as u64;
            let digits = digits_of(ammo);
            let ah = renderer.font_height(HudFontKind::Score).unwrap_or(20.0);
            renderer.push_indices(
                batch,
                HudFontKind::Score,
                left_x + iw + 6.0,
                left_y + (ih - ah) * 0.5,
                &digits,
                true,
            );
            left_y += ih + 6.0;
        }
        if world.armour > 0 {
            let sp = HudSprite::Armour(self.ind_on);
            let ih = renderer.sprite_size(sp).map_or(38.0, |s| s[1]);
            renderer.push_sprite(batch, sp, left_x, left_y);
            left_y += ih + 6.0;
        }
        if world.jail_free {
            let sp = HudSprite::JailFree(self.ind_on);
            let ih = renderer.sprite_size(sp).map_or(43.0, |s| s[1]);
            renderer.push_sprite(batch, sp, left_x, left_y);
            left_y += ih + 6.0;
        }
        if world.player_speed > 0 || world.in_frenzy() {
            let sp = HudSprite::Speed(self.ind_on);
            renderer.push_sprite(batch, sp, left_x, left_y);
        }

        // 6. Brief box (bottom-left: icon on the bottom edge + wrapped lines stacked upwards).
        if self.brief_ticks > 0 && !self.brief_text.is_empty() {
            let icon = HudSprite::BriefIcon(self.brief_kind as u8);
            let [iw, ih] = renderer.sprite_size(icon).unwrap_or([48.0, 60.0]);
            let icon_x = 6.0;
            let icon_y = vh - ih - 6.0;
            renderer.push_sprite(batch, icon, icon_x, icon_y);

            let text_x = icon_x + iw + 8.0;
            let wrap_w = (vw - text_x - 10.0).max(120.0);
            let lines = wrap_bytes(&self.brief_text, wrap_w, |s| {
                renderer.measure(HudFontKind::Sub, s)
            });
            let lh = renderer.font_height(HudFontKind::Sub).unwrap_or(22.0) + 2.0;
            let n = lines.len();
            for (i, line) in lines.iter().enumerate() {
                let y = vh - 8.0 - (n - i) as f32 * lh;
                renderer.push_text(batch, HudFontKind::Sub, text_x, y, line, true, None);
            }
        }

        // 7. Big two-line / multi-line card (centred, BIG font).
        //    State cards (BUSTED/WASTED/GAME OVER/PAUSED) take priority over
        //    transient big messages (GOURANGA!, MISSION COMPLETE!, etc.).
        let card_bytes: Option<Vec<u8>> = if paused {
            Some(self.resolve_text(&HudText::key("paused")))
        } else if world.bust_timer > 0 || world.card == Card::GameOver {
            match world.card {
                Card::Busted => Some(self.resolve_text(&HudText::key("4003"))),
                Card::Wasted => Some(self.resolve_text(&HudText::key("4004"))),
                Card::GameOver => Some(self.resolve_text(&HudText::key("4005"))),
                Card::None => None,
            }
        } else if self.big_ticks > 0 && !self.big_text.is_empty() {
            Some(self.big_text.clone())
        } else {
            None
        };

        if let Some(text) = card_bytes {
            draw_big_card(renderer, batch, vw, vh, &text);
        }
    }
}

/// Draw a centred big card split into up to 3 words (one word per line,
/// matching the original's 2-line `MISSION COMPLETE!` / `GAME OVER` cards).
fn draw_big_card(renderer: &HudRenderer, batch: &mut HudBatch, vw: f32, vh: f32, text: &[u8]) {
    let words: Vec<&[u8]> = text
        .split(|&b| b == b' ')
        .filter(|w| !w.is_empty())
        .take(3)
        .collect();
    if words.is_empty() {
        return;
    }
    let bh = renderer.font_height(HudFontKind::Big).unwrap_or(65.0);
    let lh = bh + 6.0;
    let total_h = words.len() as f32 * lh - 6.0;
    let y0 = (vh - total_h) * 0.5;
    for (i, w) in words.iter().enumerate() {
        let bw = renderer.measure(HudFontKind::Big, w);
        renderer.push_text(
            batch,
            HudFontKind::Big,
            (vw - bw) * 0.5,
            y0 + i as f32 * lh,
            w,
            true,
            None,
        );
    }
}

fn digits_of(mut n: u64) -> Vec<usize> {
    if n == 0 {
        return vec![0];
    }
    let mut out = Vec::with_capacity(10);
    while n > 0 {
        out.push((n % 10) as usize);
        n /= 10;
    }
    out.reverse();
    out
}

/// Greedily wrap `text` on ASCII spaces to fit `max_w` (at most 8 lines).
fn wrap_bytes(text: &[u8], max_w: f32, mut measure: impl FnMut(&[u8]) -> f32) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    while start < text.len() && lines.len() < 8 {
        while start < text.len() && text[start] == b' ' {
            start += 1;
        }
        if start >= text.len() {
            break;
        }
        let mut pos = start;
        let mut last_fit = None;
        while pos < text.len() {
            let mut end = pos;
            while end < text.len() && text[end] != b' ' {
                end += 1;
            }
            let candidate = &text[start..end];
            if measure(candidate) > max_w && last_fit.is_some() {
                break;
            }
            last_fit = Some(end);
            pos = end + 1;
        }
        let Some(end) = last_fit else {
            break;
        };
        lines.push(&text[start..end]);
        start = end;
    }
    lines
}

fn fallback_key_text(key: &str) -> &'static str {
    match key {
        "4003" => "BUSTED!",
        "4004" => "WASTED!",
        "4005" => "GAME OVER",
        "8787" => "GOURANGA!",
        "2500" => "MISSION COMPLETE!",
        "2501" => "MISSION FAILED!",
        "2504" => "FRENZY FAILED!",
        "paused" => "PAUSED",
        "respray_done" => "NEW PAINT JOB!",
        "clean_done" => "CLEAN PLATES!",
        "no_respray" => "NOT ENOUGH CASH!",
        "bomb_on" => "BOMB FITTED!",
        _ => "",
    }
}

fn default_crime_name(i: usize) -> &'static str {
    match i {
        crime::SHUNT => "DAMAGE",
        crime::RUNOVER => "HIT AND RUN",
        crime::HIJACK => "HIJACKING",
        crime::CARJACK => "CARJACKING",
        crime::GTA => "GRAND THEFT AUTO",
        crime::FIREARM => "FIREARMS",
        crime::MURDER => "MURDER",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_long_brief_on_spaces() {
        let lines = wrap_bytes(b"GO TO THE DOCKS AND STEAL THE RED CAR", 100.0, |s| {
            s.len() as f32 * 8.0
        });
        assert_eq!(
            lines,
            vec![
                &b"GO TO THE"[..],
                &b"DOCKS AND"[..],
                &b"STEAL THE"[..],
                &b"RED CAR"[..],
            ]
        );
    }

    #[test]
    fn digits_of_formats_zero_and_large_scores() {
        assert_eq!(digits_of(0), vec![0]);
        assert_eq!(digits_of(12500), vec![1, 2, 5, 0, 0]);
    }
}
