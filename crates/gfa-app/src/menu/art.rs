//! DOS front-end art drawn in the original 640 × 480 coordinate space.
use egui::{Align, Color32, ColorImage, Rect, TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2};
use gfa_assets::AssetSource;
use gfa_formats::{fon::Font, screen};
use std::collections::HashMap;

pub struct Art {
    palette: Option<screen::Palette>,
    fonts: HashMap<String, Font>,
    textures: HashMap<String, Option<TextureHandle>>,
}

impl Art {
    #[cfg(test)]
    pub fn all_loaded(&self) -> bool {
        self.textures.values().all(Option::is_some) && self.fonts.len() == 7
    }
    pub fn load(src: &dyn AssetSource) -> Self {
        let palette = src
            .read("GTADATA/F_PAL.RAW")
            .ok()
            .and_then(|b| screen::parse_palette(&b).ok());
        let mut fonts = HashMap::new();
        for name in ["MHEAD", "MTEXT", "MMISS", "KEY", "CITY1", "CITY3", "CITY4"] {
            if let Some(f) = src
                .read(&format!("GTADATA/F_{name}.FON"))
                .ok()
                .and_then(|b| Font::parse(&b).ok())
            {
                fonts.insert(name.into(), f);
            }
        }
        Self {
            palette,
            fonts,
            textures: HashMap::new(),
        }
    }
}

pub struct Canvas<'a> {
    pub ui: &'a Ui,
    art: &'a mut Art,
    src: &'a dyn AssetSource,
    scale: f32,
    origin: egui::Pos2,
}

impl<'a> Canvas<'a> {
    pub fn new(ui: &'a Ui, art: &'a mut Art, src: &'a dyn AssetSource) -> Self {
        let area = ui.max_rect();
        ui.painter().rect_filled(area, 0., Color32::BLACK);
        let scale = (area.width() / 640.).min(area.height() / 480.);
        Self {
            ui,
            art,
            src,
            scale,
            origin: area.center() - vec2(320., 240.) * scale,
        }
    }
    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(
            self.origin + vec2(x, y) * self.scale,
            vec2(w, h) * self.scale,
        )
    }
    fn paint(&self, id: egui::TextureId, rect: Rect) {
        self.ui.painter().image(
            id,
            rect,
            Rect::from_min_max(pos2(0., 0.), pos2(1., 1.)),
            Color32::WHITE,
        );
    }
    pub fn picture(&mut self, stem: &str, x: f32, y: f32) {
        let t = self.art.textures.entry(stem.into()).or_insert_with(|| {
            // GTA8 blits indexed RATs through the VGA DAC. Old browser caches
            // containing only the RGB masters can still draw those pictures.
            let indexed = self.art.palette.as_ref().and_then(|pal| {
                let bytes = self.src.read(&format!("GTADATA/{stem}.RAT")).ok()?;
                let img = screen::parse_rat(&bytes, stem).ok()?;
                let pixels = img
                    .pixels
                    .iter()
                    .map(|&v| {
                        let [r, g, b] = pal[usize::from(v)].map(|c| (c & 0xfc) | (c >> 6));
                        Color32::from_rgb(r, g, b)
                    })
                    .collect();
                Some(ColorImage::new([img.width, img.height], pixels))
            });
            let img = indexed.or_else(|| {
                let b = self.src.read(&format!("GTADATA/{stem}.RAW")).ok()?;
                let (w, h) = screen::screen_size(stem, b.len() / 3)?;
                (b.len() == w * h * 3).then(|| ColorImage::from_rgb([w, h], &b))
            })?;
            Some(
                self.ui
                    .ctx()
                    .load_texture(stem, img, TextureOptions::NEAREST),
            )
        });
        if let Some((id, [w, h])) = t.as_ref().map(|t| (t.id(), t.size())) {
            self.paint(id, self.rect(x, y, w as f32, h as f32));
        } else if stem.starts_with("F_LOGO") && stem != "F_LOGO0" {
            self.picture("F_LOGO0", x, y);
        }
    }
    pub fn text(&mut self, font: &str, text: &str, x: f32, y: f32, align: Align) -> Rect {
        let key = format!("{font}/{text}");
        let texture = self.art.textures.entry(key.clone()).or_insert_with(|| {
            let f = self.art.fonts.get(font)?;
            let glyphs: Vec<_> = text
                .chars()
                .map(|c| u8::try_from(c).ok().and_then(|c| f.glyph(c)))
                .collect();
            // The DOS menu advances a space by one ordinary letter (10
            // pixels in F_MTEXT, 6 in F_MMISS).
            let space = f.glyph(b'a').map_or(4, |g| usize::from(g.width));
            let width: usize = glyphs
                .iter()
                .map(|g| g.map_or(space, |g| usize::from(g.width)))
                .sum();
            let height = usize::from(f.height);
            if width == 0 || width > 4096 || height == 0 {
                return None;
            }
            let mut img = ColorImage::filled([width, height], Color32::TRANSPARENT);
            let mut dx = 0;
            for g in glyphs {
                let Some(g) = g else {
                    dx += space;
                    continue;
                };
                let w = usize::from(g.width);
                for (i, &v) in g.pixels.iter().enumerate().filter(|(_, v)| **v != 0) {
                    let [r, g, b] = f.palette[usize::from(v)];
                    img[(dx + i % w, i / w)] = Color32::from_rgb(r, g, b);
                }
                dx += w;
            }
            Some(
                self.ui
                    .ctx()
                    .load_texture(key, img, TextureOptions::NEAREST),
            )
        });
        if let Some((id, Vec2 { x: w, y: h })) = texture.as_ref().map(|t| (t.id(), t.size_vec2())) {
            let rect = self.rect(x - w * align.to_factor(), y, w, h);
            self.paint(id, rect);
            rect
        } else {
            let size = match font {
                "MHEAD" => 32.,
                "MMISS" => 12.,
                _ => 21.,
            };
            self.ui.painter().text(
                self.rect(x, y, 0., 0.).min,
                egui::Align2([align, Align::Min]),
                text,
                egui::FontId::proportional(size * self.scale),
                Color32::GOLD,
            )
        }
    }
    pub fn action(
        &mut self,
        font: &str,
        text: &str,
        x: f32,
        y: f32,
        align: Align,
    ) -> egui::Response {
        let rect = self.text(font, text, x, y, align);
        self.ui
            .interact(
                rect,
                self.ui.scope_id().with((font, text, x as i32, y as i32)),
                egui::Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
    }
    pub fn icon(&mut self, font: &str, frame: usize, x: f32, y: f32) {
        let count = self.art.fonts.get(font).map_or(0, |f| f.glyphs.len());
        if count > 0 {
            let ch = (b'!' + (frame % count) as u8) as char;
            self.text(font, &ch.to_string(), x, y, Align::Min);
        } else if font == "KEY" {
            self.text("MTEXT", ">", x + 52., y + 20., Align::Min);
        }
    }
}
