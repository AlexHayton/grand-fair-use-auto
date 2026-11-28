//! 2D HUD renderer: original `.FON` bitmap fonts, style HUD sprites, and
//! pure-Rust EPX/Scale2x HD upscaling.
//!
//! All HUD geometry is expressed in a virtual coordinate space that is
//! [`HUD_VIRTUAL_H`] (480) pixels tall and aspect-matched to the active game
//! viewport (`w = 480 * aspect`, at least 640 on 4:3). When a G24 style with
//! the 640×480 hi-res HUD sprites (`SpriteCategory::Arrow` indices 25..49) and
//! `*2.FON` fonts is loaded, everything renders natively at hi-res; GRY styles
//! and `*1.FON` fonts automatically scale 2× into the same 480-tall space.
//!
//! Font upscaling uses EPX/Scale2x on RGBA glyphs (rounding diagonal pixel
//! staircases without inventing colour fringes, preserving alpha 0 exactly),
//! optionally paired with bilinear filtering (`HudFilter::SmoothHd`) for crisp,
//! smooth HD/Retina text.

use gfa_formats::fon::{ACCENT_BASE, ACCENTS, FIRST_CHAR, Font};
use gfa_formats::style::{Colour, SpriteCategory, Style};
use wgpu::util::DeviceExt;

use crate::ASSET_TEXTURE_MAX;

/// Virtual HUD height (matches GTA1's 640×480 hi-res HUD art).
pub const HUD_VIRTUAL_H: f32 = 480.0;

/// How HUD fonts and icons are upscaled for HD/Retina displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HudFilter {
    /// Original bitmap pixels, nearest-neighbour.
    Original,
    /// EPX/Scale2x 2× pixel-art upscaling, nearest-neighbour (crisp edges).
    Epx,
    /// EPX/Scale2x 2× pixel-art upscaling with bilinear filtering (smooth HD).
    #[default]
    SmoothHd,
}

impl HudFilter {
    pub fn name(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Epx => "EPX 2x",
            Self::SmoothHd => "Smooth HD",
        }
    }

    fn use_epx(self) -> bool {
        matches!(self, Self::Epx | Self::SmoothHd)
    }

    fn filter_mode(self) -> wgpu::FilterMode {
        match self {
            Self::Original | Self::Epx => wgpu::FilterMode::Nearest,
            Self::SmoothHd => wgpu::FilterMode::Linear,
        }
    }
}

/// Logical HUD font slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum HudFontKind {
    Pager = 0,
    Score = 1,
    Mult = 2,
    Big = 3,
    Sub = 4,
    CarName = 5,
}

impl HudFontKind {
    pub const COUNT: usize = 6;
}

/// Logical HUD sprites from `SpriteCategory::Arrow`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HudSprite {
    BriefBarLeft,
    BriefBarRight,
    CarPlaque,
    Pager,
    PagerLed,
    /// 0 = pistol, 1 = machine gun, 2 = rocket, 3 = flamethrower.
    Weapon(u8),
    /// 0 = phone, 1 = talk, 2 = mobile, 3 = info, 4 = cop, 5 = skull.
    BriefIcon(u8),
    CopHead(bool),
    Armour(bool),
    JailFree(bool),
    Speed(bool),
}

impl HudSprite {
    const ALL: [HudSprite; 23] = [
        Self::BriefBarLeft,
        Self::BriefBarRight,
        Self::CarPlaque,
        Self::Pager,
        Self::PagerLed,
        Self::Weapon(0),
        Self::Weapon(1),
        Self::Weapon(2),
        Self::Weapon(3),
        Self::BriefIcon(0),
        Self::BriefIcon(1),
        Self::BriefIcon(2),
        Self::BriefIcon(3),
        Self::BriefIcon(4),
        Self::BriefIcon(5),
        Self::CopHead(false),
        Self::CopHead(true),
        Self::Armour(false),
        Self::Armour(true),
        Self::JailFree(false),
        Self::JailFree(true),
        Self::Speed(false),
        Self::Speed(true),
    ];

    fn slot(self) -> usize {
        match self {
            Self::BriefBarLeft => 0,
            Self::BriefBarRight => 1,
            Self::CarPlaque => 2,
            Self::Pager => 3,
            Self::PagerLed => 4,
            Self::Weapon(w) => 5 + (w.min(3) as usize),
            Self::BriefIcon(k) => 9 + (k.min(5) as usize),
            Self::CopHead(on) => 15 + usize::from(on),
            Self::Armour(on) => 17 + usize::from(on),
            Self::JailFree(on) => 19 + usize::from(on),
            Self::Speed(on) => 21 + usize::from(on),
        }
    }

    /// Index within `SpriteCategory::Arrow` for hi-res (G24, 49+ arrows) or
    /// low-res (GRY, 25 arrows).
    fn arrow_index(self, hires: bool) -> Option<usize> {
        if hires {
            Some(match self {
                Self::BriefBarLeft => 25,
                Self::BriefBarRight => 26,
                Self::CarPlaque => 27,
                Self::Pager => 28,
                Self::PagerLed => 29,
                Self::Weapon(w) => 30 + (w.min(3) as usize),
                // Order: Phone=36, Talk=35, Mobile=37, Info=39, Cop=38, Skull=40.
                Self::BriefIcon(0) => 36,
                Self::BriefIcon(1) => 35,
                Self::BriefIcon(2) => 37,
                Self::BriefIcon(3) => 39,
                Self::BriefIcon(4) => 38,
                Self::BriefIcon(_) => 40,
                Self::CopHead(on) => 41 + usize::from(on),
                Self::Armour(on) => 43 + usize::from(on),
                Self::JailFree(on) => 45 + usize::from(on),
                Self::Speed(on) => 47 + usize::from(on),
            })
        } else {
            match self {
                Self::BriefBarLeft => Some(1),
                Self::BriefBarRight => None,
                Self::CarPlaque => Some(2),
                Self::Pager => Some(3),
                Self::PagerLed => Some(4),
                Self::Weapon(w) => Some(5 + (w.min(3) as usize)),
                // Order: Phone=11, Talk=10, Mobile=12, Info=14, Cop=13, Skull=15.
                Self::BriefIcon(0) => Some(11),
                Self::BriefIcon(1) => Some(10),
                Self::BriefIcon(2) => Some(12),
                Self::BriefIcon(3) => Some(14),
                Self::BriefIcon(4) => Some(13),
                Self::BriefIcon(_) => Some(15),
                Self::CopHead(on) => Some(16 + usize::from(on)),
                Self::Armour(on) => Some(18 + usize::from(on)),
                Self::JailFree(on) => Some(20 + usize::from(on)),
                Self::Speed(on) => Some(22 + usize::from(on)),
            }
        }
    }
}

/// Loaded `.FON` files handed to [`HudRenderer::new`]. Each entry pairs the
/// parsed font with its virtual scale (`1.0` for `*2.FON` hi-res fonts,
/// `2.0` for `*1.FON` low-res fonts).
#[derive(Debug, Clone, Default)]
pub struct HudFonts {
    pub fonts: [Option<(Font, f32)>; HudFontKind::COUNT],
}

impl HudFonts {
    pub fn set(&mut self, kind: HudFontKind, font: Font, scale: f32) {
        self.fonts[kind as usize] = Some((font, scale));
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct AtlasEntry {
    /// Normalized UVs in the atlas: `[u0, v0, u1, v1]`.
    uv: [f32; 4],
    /// Logical width and height in virtual 480-tall HUD pixels.
    size: [f32; 2],
    present: bool,
}

#[derive(Debug, Clone)]
struct PackedFont {
    height: f32,
    space: f32,
    /// Inter-glyph spacing in virtual HUD pixels.
    gap: f32,
    glyphs: Vec<PackedGlyph>,
}

#[derive(Debug, Clone, Copy)]
struct PackedGlyph {
    uv: [f32; 4],
    width: f32,
    blank: bool,
}

impl PackedFont {
    fn glyph_for_byte(&self, b: u8) -> Option<&PackedGlyph> {
        let lookup = |ch: u8| -> Option<usize> {
            let idx = match ch {
                0xC0.. => ACCENT_BASE + *ACCENTS.get((ch - 0xC0) as usize)?.as_ref()? as usize,
                _ => ch.checked_sub(FIRST_CHAR)? as usize,
            };
            (idx < self.glyphs.len()).then_some(idx)
        };
        if let Some(i) = lookup(b) {
            let g = &self.glyphs[i];
            if !g.blank {
                return Some(g);
            }
        }
        // Pager and big fonts leave lowercase blank; fall back to uppercase.
        if b.is_ascii_lowercase()
            && let Some(i) = lookup(b.to_ascii_uppercase())
        {
            let g = &self.glyphs[i];
            if !g.blank {
                return Some(g);
            }
        }
        if (0xE0..=0xFC).contains(&b)
            && let Some(i) = lookup(b - 0x20)
        {
            let g = &self.glyphs[i];
            if !g.blank {
                return Some(g);
            }
        }
        None
    }

    fn glyph_by_index(&self, idx: usize) -> Option<&PackedGlyph> {
        self.glyphs.get(idx).filter(|g| !g.blank)
    }

    fn measure_bytes(&self, text: &[u8]) -> f32 {
        let mut x = 0.0f32;
        for &b in text {
            if b == b' ' {
                x += self.space;
            } else if let Some(g) = self.glyph_for_byte(b) {
                x += g.width + self.gap;
            } else {
                x += self.space;
            }
        }
        x
    }

    fn measure_indices(&self, indices: &[usize]) -> f32 {
        let mut x = 0.0f32;
        for &i in indices {
            if let Some(g) = self.glyph_by_index(i) {
                x += g.width + self.gap;
            }
        }
        x
    }
}

/// Classic EPX / Scale2x doubling on an RGBA8 image.
///
/// Every output pixel is copied verbatim from one of its 5 input neighbours,
/// rounding diagonal staircases while preserving transparency (alpha == 0)
/// and never inventing colour fringes.
pub fn epx2_rgba(src: &[u8], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    if w == 0 || h == 0 {
        return (Vec::new(), 0, 0);
    }
    let ow = w * 2;
    let oh = h * 2;
    let mut out = vec![0u8; ow * oh * 4];
    let px = |x: isize, y: isize| -> [u8; 4] {
        // Treat outside-of-glyph as transparent so outer edges bevel cleanly
        // without smearing border pixels into the padding.
        if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
            return [0, 0, 0, 0];
        }
        let i = (y as usize * w + x as usize) * 4;
        [src[i], src[i + 1], src[i + 2], src[i + 3]]
    };
    let mut put = |x: usize, y: usize, c: [u8; 4]| {
        let i = (y * ow + x) * 4;
        out[i..i + 4].copy_from_slice(&c);
    };
    for y in 0..h {
        let iy = y as isize;
        for x in 0..w {
            let ix = x as isize;
            let c = px(ix, iy);
            let a = px(ix, iy - 1);
            let b = px(ix + 1, iy);
            let d = px(ix - 1, iy);
            let e = px(ix, iy + 1);
            let p0 = if d == a && d != e && a != b { a } else { c };
            let p1 = if a == b && a != d && b != e { b } else { c };
            let p2 = if e == d && e != b && d != a { d } else { c };
            let p3 = if b == e && b != a && e != d { e } else { c };
            put(x * 2, y * 2, p0);
            put(x * 2 + 1, y * 2, p1);
            put(x * 2, y * 2 + 1, p2);
            put(x * 2 + 1, y * 2 + 1, p3);
        }
    }
    (out, ow, oh)
}

fn sprite_to_rgba(style: &Style, sprite_idx: usize) -> Option<(Vec<u8>, usize, usize)> {
    let sp = style.sprites.get(sprite_idx)?;
    let w = sp.width as usize;
    let h = sp.height as usize;
    let pixels = style.sprite(sprite_idx)?.to_vec();
    let mut rgba = vec![0u8; w * h * 4];
    let pal: [[u8; 3]; 256] = match &style.colour {
        Colour::G24(c) => {
            let pid = sp
                .clut
                .and_then(|k| c.sprite_palette(k))
                .unwrap_or(0) as usize;
            let clut = c.cluts.get(pid)?;
            std::array::from_fn(|i| [clut[i][0], clut[i][1], clut[i][2]])
        }
        Colour::Gry(c) => c.palette,
    };
    for (i, &v) in pixels.iter().enumerate() {
        if v != 0 {
            let [r, g, b] = pal[usize::from(v)];
            rgba[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    Some((rgba, w, h))
}

struct ShelfPacker {
    width: usize,
    x: usize,
    y: usize,
    shelf: usize,
    pixels: Vec<u8>,
    height: usize,
}

impl ShelfPacker {
    const PAD: usize = 2;

    fn new(width: usize) -> Self {
        Self {
            width,
            x: Self::PAD,
            y: Self::PAD,
            shelf: 0,
            pixels: Vec::new(),
            height: Self::PAD,
        }
    }

    /// Place an RGBA image with transparent padding around it. Returns pixel
    /// rect `(x, y, w, h)`, or `None` if it would exceed `ASSET_TEXTURE_MAX`.
    fn push(&mut self, rgba: &[u8], w: usize, h: usize) -> Option<[usize; 4]> {
        if w == 0 || h == 0 {
            return None;
        }
        if self.x + w + Self::PAD > self.width {
            self.x = Self::PAD;
            self.y += self.shelf + Self::PAD;
            self.shelf = 0;
        }
        let need_h = self.y + h + Self::PAD;
        if need_h > ASSET_TEXTURE_MAX as usize {
            return None;
        }
        if need_h > self.height {
            self.height = need_h;
            self.pixels.resize(self.width * self.height * 4, 0);
        }
        let px = self.x;
        let py = self.y;
        for row in 0..h {
            let dst_off = ((py + row) * self.width + px) * 4;
            let src_off = row * w * 4;
            self.pixels[dst_off..dst_off + w * 4]
                .copy_from_slice(&rgba[src_off..src_off + w * 4]);
        }
        self.x += w + Self::PAD;
        self.shelf = self.shelf.max(h);
        Some([px, py, w, h])
    }
}

struct BuiltAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    sprites: [AtlasEntry; HudSprite::ALL.len()],
    fonts: [Option<PackedFont>; HudFontKind::COUNT],
}

fn build_atlas(style: &Style, fonts: &HudFonts, epx: bool) -> BuiltAtlas {
    let atlas_w = ASSET_TEXTURE_MAX as usize;
    let mut packer = ShelfPacker::new(atlas_w);
    let hires = style.sprite_count(SpriteCategory::Arrow) >= 49;
    let sprite_scale = if hires { 1.0f32 } else { 2.0f32 };

    let mut sprite_rects = [None::<([usize; 4], [f32; 2])>; HudSprite::ALL.len()];
    for sp in HudSprite::ALL {
        let Some(arrow_i) = sp.arrow_index(hires) else {
            continue;
        };
        let Some(global_i) = style.sprite_index(SpriteCategory::Arrow, arrow_i) else {
            continue;
        };
        let Some((rgba, w, h)) = sprite_to_rgba(style, global_i) else {
            continue;
        };
        let (img, pw, ph) = if epx {
            epx2_rgba(&rgba, w, h)
        } else {
            (rgba, w, h)
        };
        if let Some(rect) = packer.push(&img, pw, ph) {
            sprite_rects[sp.slot()] =
                Some((rect, [w as f32 * sprite_scale, h as f32 * sprite_scale]));
        }
    }

    struct RawFont {
        vh: f32,
        space: f32,
        gap: f32,
        glyphs: Vec<(Option<[usize; 4]>, f32, bool)>,
    }
    let mut font_raw: [Option<RawFont>; HudFontKind::COUNT] = std::array::from_fn(|_| None);
    for (fi, entry) in fonts.fonts.iter().enumerate() {
        let Some((font, scale)) = entry else {
            continue;
        };
        let h = usize::from(font.height);
        let mut glyphs = Vec::with_capacity(font.glyphs.len());
        for g in &font.glyphs {
            let w = usize::from(g.width);
            let blank = w == 0 || h == 0 || g.pixels.iter().all(|&p| p == 0);
            if blank {
                glyphs.push((None, w as f32 * scale, true));
                continue;
            }
            let mut rgba = vec![0u8; w * h * 4];
            for (i, &v) in g.pixels.iter().enumerate() {
                if v != 0 {
                    let [r, gr, b] = font.palette[usize::from(v)];
                    rgba[i * 4..i * 4 + 4].copy_from_slice(&[r, gr, b, 255]);
                }
            }
            // Apply EPX 2× when requested; if the atlas is running low on space
            // (e.g. very tall font), fall back cleanly to 1× for that glyph.
            let rect = if epx {
                let (up, uw, uh) = epx2_rgba(&rgba, w, h);
                packer.push(&up, uw, uh).or_else(|| packer.push(&rgba, w, h))
            } else {
                packer.push(&rgba, w, h)
            };
            glyphs.push((rect, w as f32 * scale, false));
        }
        let vh = font.height as f32 * scale;
        let space = (vh / 3.0).max(4.0);
        let gap = *scale;
        font_raw[fi] = Some(RawFont {
            vh,
            space,
            gap,
            glyphs,
        });
    }

    let final_h = packer.height.max(4);
    packer.pixels.resize(atlas_w * final_h * 4, 0);
    let inv_w = 1.0 / atlas_w as f32;
    let inv_h = 1.0 / final_h as f32;
    let to_uv = |r: [usize; 4]| {
        [
            r[0] as f32 * inv_w,
            r[1] as f32 * inv_h,
            (r[0] + r[2]) as f32 * inv_w,
            (r[1] + r[3]) as f32 * inv_h,
        ]
    };

    let mut sprites = [AtlasEntry::default(); HudSprite::ALL.len()];
    for (i, item) in sprite_rects.into_iter().enumerate() {
        if let Some((r, size)) = item {
            sprites[i] = AtlasEntry {
                uv: to_uv(r),
                size,
                present: true,
            };
        }
    }

    let fonts = std::array::from_fn(|i| {
        font_raw[i].as_ref().map(|rf| PackedFont {
            height: rf.vh,
            space: rf.space,
            gap: rf.gap,
            glyphs: rf
                .glyphs
                .iter()
                .map(|&(rect, width, blank)| PackedGlyph {
                    uv: rect.map_or([0.0; 4], to_uv),
                    width,
                    blank: blank || rect.is_none(),
                })
                .collect(),
        })
    });

    BuiltAtlas {
        width: atlas_w as u32,
        height: final_h as u32,
        pixels: packer.pixels,
        sprites,
        fonts,
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    screen: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct HudQuad {
    dst: [f32; 4],
    src: [f32; 4],
    tint: [f32; 4],
    clip: [f32; 4],
}

const NO_CLIP: [f32; 4] = [-1e6, -1e6, 1e6, 1e6];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// Flat black drop shadow (negative alpha triggers flat-shadow mode in WGSL).
const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, -0.85];

/// Builds a list of 2D HUD quads for one frame.
#[derive(Debug, Default)]
pub struct HudBatch {
    quads: Vec<HudQuad>,
}

impl HudBatch {
    pub fn clear(&mut self) {
        self.quads.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
    }
}

/// GPU resources and packed font/icon atlases for the 2D HUD.
pub struct HudRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    bind_group: wgpu::BindGroup,
    sprites: [AtlasEntry; HudSprite::ALL.len()],
    fonts: [Option<PackedFont>; HudFontKind::COUNT],
    filter: HudFilter,
}

impl HudRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        style: &Style,
        fonts: &HudFonts,
        filter: HudFilter,
    ) -> Self {
        let bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("hud_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hud_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/hud.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hud_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hud_pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<HudQuad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4,
                        1 => Float32x4,
                        2 => Float32x4,
                        3 => Float32x4,
                    ],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hud_globals"),
            contents: bytemuck::bytes_of(&Globals {
                screen: [640.0, HUD_VIRTUAL_H, 0.0, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let capacity = 256;
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hud_instances"),
            size: (capacity * std::mem::size_of::<HudQuad>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (bind_group, sprites, packed_fonts) =
            upload_atlas(device, queue, &bind_group_layout, &globals, style, fonts, filter);

        Self {
            pipeline,
            bind_group_layout,
            globals,
            instances,
            capacity,
            bind_group,
            sprites,
            fonts: packed_fonts,
            filter,
        }
    }

    pub fn filter(&self) -> HudFilter {
        self.filter
    }

    /// Rebuild the HUD atlas and sampler when the user changes [`HudFilter`].
    pub fn set_filter(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        style: &Style,
        fonts: &HudFonts,
        filter: HudFilter,
    ) {
        if self.filter == filter {
            return;
        }
        let (bind_group, sprites, packed_fonts) = upload_atlas(
            device,
            queue,
            &self.bind_group_layout,
            &self.globals,
            style,
            fonts,
            filter,
        );
        self.bind_group = bind_group;
        self.sprites = sprites;
        self.fonts = packed_fonts;
        self.filter = filter;
    }

    /// Virtual size `(w, h)` of a HUD sprite, or `None` if absent.
    pub fn sprite_size(&self, sp: HudSprite) -> Option<[f32; 2]> {
        let e = self.sprites[sp.slot()];
        e.present.then_some(e.size)
    }

    /// Virtual line height of a HUD font.
    pub fn font_height(&self, kind: HudFontKind) -> Option<f32> {
        self.fonts[kind as usize].as_ref().map(|f| f.height)
    }

    /// Measure a Windows-1252 byte string in `kind` (virtual HUD pixels).
    pub fn measure(&self, kind: HudFontKind, text: &[u8]) -> f32 {
        self.fonts[kind as usize]
            .as_ref()
            .map_or(0.0, |f| f.measure_bytes(text))
    }

    /// Measure a sequence of raw glyph indices in `kind` (for `SCORE` / `MISSMUL`).
    pub fn measure_indices(&self, kind: HudFontKind, indices: &[usize]) -> f32 {
        self.fonts[kind as usize]
            .as_ref()
            .map_or(0.0, |f| f.measure_indices(indices))
    }

    /// Push a HUD sprite at `(x, y)` in virtual HUD pixels.
    pub fn push_sprite(&self, batch: &mut HudBatch, sp: HudSprite, x: f32, y: f32) {
        let e = self.sprites[sp.slot()];
        if !e.present {
            return;
        }
        batch.quads.push(HudQuad {
            dst: [x, y, e.size[0], e.size[1]],
            src: e.uv,
            tint: WHITE,
            clip: NO_CLIP,
        });
    }

    /// Push Windows-1252 text with optional drop shadow and optional clip rect.
    #[allow(clippy::too_many_arguments)]
    pub fn push_text(
        &self,
        batch: &mut HudBatch,
        kind: HudFontKind,
        x: f32,
        y: f32,
        text: &[u8],
        shadow: bool,
        clip: Option<[f32; 4]>,
    ) {
        let Some(font) = &self.fonts[kind as usize] else {
            return;
        };
        let clip_rect = clip.unwrap_or(NO_CLIP);
        if shadow {
            let off = 2.0f32;
            let sclip = clip.map_or(NO_CLIP, |c| [c[0] + off, c[1] + off, c[2] + off, c[3] + off]);
            self.push_text_pass(batch, font, x + off, y + off, text, SHADOW, sclip);
        }
        self.push_text_pass(batch, font, x, y, text, WHITE, clip_rect);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_text_pass(
        &self,
        batch: &mut HudBatch,
        font: &PackedFont,
        mut x: f32,
        y: f32,
        text: &[u8],
        tint: [f32; 4],
        clip: [f32; 4],
    ) {
        for &b in text {
            if b == b' ' {
                x += font.space;
                continue;
            }
            let Some(g) = font.glyph_for_byte(b) else {
                x += font.space;
                continue;
            };
            if x + g.width > clip[0] && x < clip[2] {
                batch.quads.push(HudQuad {
                    dst: [x, y, g.width, font.height],
                    src: g.uv,
                    tint,
                    clip,
                });
            }
            x += g.width + font.gap;
        }
    }

    /// Push raw glyph indices (used by `SCORE` 0..9 and `MISSMUL` 0..10).
    pub fn push_indices(
        &self,
        batch: &mut HudBatch,
        kind: HudFontKind,
        mut x: f32,
        y: f32,
        indices: &[usize],
        shadow: bool,
    ) {
        let Some(font) = &self.fonts[kind as usize] else {
            return;
        };
        if shadow {
            let mut sx = x + 2.0;
            for &idx in indices {
                if let Some(g) = font.glyph_by_index(idx) {
                    batch.quads.push(HudQuad {
                        dst: [sx, y + 2.0, g.width, font.height],
                        src: g.uv,
                        tint: SHADOW,
                        clip: NO_CLIP,
                    });
                    sx += g.width + font.gap;
                }
            }
        }
        for &idx in indices {
            if let Some(g) = font.glyph_by_index(idx) {
                batch.quads.push(HudQuad {
                    dst: [x, y, g.width, font.height],
                    src: g.uv,
                    tint: WHITE,
                    clip: NO_CLIP,
                });
                x += g.width + font.gap;
            }
        }
    }

    /// Upload `batch` and draw it over `view` inside `viewport` (`[x, y, w, h]`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        viewport: [u32; 4],
        virtual_size: (f32, f32),
        batch: &HudBatch,
    ) {
        if batch.quads.is_empty() || viewport[2] == 0 || viewport[3] == 0 {
            return;
        }
        queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                screen: [virtual_size.0, virtual_size.1, 0.0, 0.0],
            }),
        );
        if batch.quads.len() > self.capacity {
            self.capacity = batch.quads.len().next_power_of_two();
            self.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hud_instances"),
                size: (self.capacity * std::mem::size_of::<HudQuad>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&batch.quads));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hud"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_viewport(
            viewport[0] as f32,
            viewport[1] as f32,
            viewport[2] as f32,
            viewport[3] as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(viewport[0], viewport[1], viewport[2], viewport[3]);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, 0..batch.quads.len() as u32);
    }
}

fn upload_atlas(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bgl: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    style: &Style,
    fonts: &HudFonts,
    filter: HudFilter,
) -> (
    wgpu::BindGroup,
    [AtlasEntry; HudSprite::ALL.len()],
    [Option<PackedFont>; HudFontKind::COUNT],
) {
    let built = build_atlas(style, fonts, filter.use_epx());
    let size = wgpu::Extent3d {
        width: built.width,
        height: built.height,
        depth_or_array_layers: 1,
    };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hud_atlas"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &built.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(built.width * 4),
            rows_per_image: None,
        },
        size,
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let mode = filter.filter_mode();
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("hud_sampler"),
        mag_filter: mode,
        min_filter: mode,
        ..Default::default()
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("hud_bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    (bind_group, built.sprites, built.fonts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epx2_rounds_diagonal_and_preserves_transparency() {
        // 2x2 diagonal:
        // [A, 0]
        // [0, A] -> no adjacent pair matches, stays sharp;
        // 3x3 corner:
        // [0, A, 0]
        // [A, 0, 0]
        // [0, 0, 0] -> centre (1,1) has D==A (both A) and D!=E (0) and A!=B (0),
        // so its top-left subpixel rounds to A!
        let a = [255u8, 200, 0, 255];
        let z = [0u8, 0, 0, 0];
        let mut src = Vec::new();
        for p in [z, a, z, a, z, z, z, z, z] {
            src.extend_from_slice(&p);
        }
        let (out, ow, oh) = epx2_rgba(&src, 3, 3);
        assert_eq!((ow, oh), (6, 6));
        let at = |x: usize, y: usize| -> [u8; 4] {
            let i = (y * ow + x) * 4;
            [out[i], out[i + 1], out[i + 2], out[i + 3]]
        };
        assert_eq!(at(2, 2), a, "top-left subpixel of (1,1) rounds to A");
        assert_eq!(at(3, 2), z, "other subpixels stay transparent");
        assert_eq!(at(2, 3), z);
        assert_eq!(at(3, 3), z);
    }
}
