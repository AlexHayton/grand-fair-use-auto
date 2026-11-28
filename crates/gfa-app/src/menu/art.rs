//! DOS front-end art drawn in the original 640 × 480 coordinate space.
use egui::{Align, Color32, ColorImage, Rect, TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2};
use gfa_assets::AssetSource;
use gfa_formats::{fon::Font, screen};
use std::collections::HashMap;

pub struct Art {
    palette: Option<screen::Palette>,
    fonts: HashMap<String, Font>,
    reflections: HashMap<String, TextureHandle>,
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
            reflections: HashMap::new(),
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
    pub fn background(&mut self, logo: &str, lower: &str) {
        let area = self.ui.max_rect();
        let menu = self.rect(0., 0., 640., 480.);
        let top = self
            .load_picture(logo, false)
            .or_else(|| self.load_picture("F_LOGO0", false));
        let bottom = self.load_picture(lower, true);

        if menu.is_positive()
            && !menu.contains_rect(area)
            && let Some(reflection) = self.art.reflections.get(lower)
        {
            // Only the lower panel contains scenery throughout. Align the
            // reflection with it and mirror its top edge beside the title.
            let scenery = self.rect(0., 168., 640., 312.);
            let uv = Rect::from_min_max(
                menu_normalized(area.min, scenery),
                menu_normalized(area.max, scenery),
            );
            self.ui
                .painter()
                .image(reflection.id(), area, uv, Color32::WHITE);
        }
        for (picture, y) in [(top, 0.), (bottom, 168.)] {
            if let Some((id, [w, h])) = picture {
                self.paint(id, self.rect(0., y, w as f32, h as f32));
            }
        }
    }
    pub fn picture(&mut self, stem: &str, x: f32, y: f32) {
        if let Some((id, [w, h])) = self.load_picture(stem, false) {
            self.paint(id, self.rect(x, y, w as f32, h as f32));
        }
    }
    fn load_picture(
        &mut self,
        stem: &str,
        backdrop: bool,
    ) -> Option<(egui::TextureId, [usize; 2])> {
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
            if backdrop {
                let reflection = self.ui.ctx().load_texture(
                    "menu scenery reflection",
                    softened_tile(&img),
                    TextureOptions::LINEAR_MIRRORED_REPEAT,
                );
                self.art.reflections.insert(stem.into(), reflection);
            }
            Some(
                self.ui
                    .ctx()
                    .load_texture(stem, img, TextureOptions::NEAREST),
            )
        });
        t.as_ref().map(|t| (t.id(), t.size()))
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

fn menu_normalized(point: egui::Pos2, menu: Rect) -> egui::Pos2 {
    ((point - menu.min) / menu.size()).to_pos2()
}

// Average 8 × 8 source pixels per texel. Linear sampling of this small
// backdrop softens the DOS dithering while retaining the edge colors.
fn softened_tile(image: &ColorImage) -> ColorImage {
    let size = [image.width() / 8, image.height() / 8];
    let mut tile = ColorImage::filled(size, Color32::BLACK);
    for y in 0..size[1] {
        for x in 0..size[0] {
            let mut rgb = [0u32; 3];
            for row in y * 8..y * 8 + 8 {
                let start = row * image.width() + x * 8;
                for pixel in &image.pixels[start..start + 8] {
                    rgb[0] += u32::from(pixel.r());
                    rgb[1] += u32::from(pixel.g());
                    rgb[2] += u32::from(pixel.b());
                }
            }
            let [r, g, b] = rgb.map(|sum| (sum / 64) as u8);
            tile[(x, y)] = Color32::from_rgb(r, g, b);
        }
    }
    tile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflection_covers_wide_and_tall_windows() {
        let mut src = gfa_assets::MemSource::new();
        src.insert("GTADATA/F_LOGO0.RAW", vec![32; 640 * 168 * 3]);
        src.insert("GTADATA/F_LOWER0.RAW", vec![96; 640 * 312 * 3]);
        let mut art = Art::load(&src);
        let ctx = egui::Context::default();
        for (size, uv_min) in [
            (vec2(1280., 480.), pos2(-0.5, -168. / 312.)),
            (vec2(640., 960.), pos2(0., -408. / 312.)),
        ] {
            let area = Rect::from_min_size(egui::Pos2::ZERO, size);
            let input = egui::RawInput {
                screen_rect: Some(area),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                Canvas::new(ui, &mut art, &src).background("F_LOGO0", "F_LOWER0");
            });
            let id = art.reflections["F_LOWER0"].id();
            if let Some(deltas) = output.textures_delta.set.get(&id) {
                let egui::ImageData::Color(image) = &deltas[0].image;
                assert!(
                    image
                        .pixels
                        .iter()
                        .all(|&pixel| pixel == Color32::from_gray(96))
                );
            }
            output.textures_delta.clear();
            let mesh = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == id => Some(mesh),
                    _ => None,
                })
                .unwrap();
            assert_eq!(mesh.calc_bounds(), area);
            assert_eq!(mesh.vertices[0].uv, uv_min);
        }
    }
}
