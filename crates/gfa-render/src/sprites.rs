//! Sprites: cars, people, objects and (later) particles, drawn as instanced
//! quads lying flat at their height, palette-indexed like the city.
//!
//! Sources: Carnage3D `src/SpriteBatch.cpp` and `gamedata/shaders/sprites.glsl`
//! (MIT, © 2019 jericho) for flat quads sorted by height then draw order, and
//! `StyleData::GetSpritePaletteIndex` for remaps. Implementation ours.
//!
//! - Sprites go into one 8-bit atlas, shelf-packed, so every sprite and
//!   every remap of it share one texture and one draw.
//! - A sprite's palette comes from the same palette list as the tiles
//!   ([`StyleTables`]): G24 sprite or remap CLUTs, or GRY remap tables.
//! - Instances carry a *draw* angle: clockwise radians applied to the art as
//!   stored. The game's art faces south when unrotated, so a thing heading
//!   `h` (clockwise from north) is drawn at `h + π` ([`draw_angle`]). AmiGTA1
//!   proved this with a test picture (`gta_player.h`, `GTA_SPRITE_ART_SOUTH`:
//!   "draw = heading + 128"); a parked car on the south side of a Liberty
//!   City road then faces east, with the traffic.
//! - Colour 0 is transparent. Sprites test against the city's depth but do
//!   not write it, so they are drawn in order: lowest first, then in the order
//!   they were added.

use gfa_formats::style::{Colour, Style};

use crate::camera::Camera;
use crate::city::DEPTH_FORMAT;
use crate::style_tables::ATLAS_WIDTH;

/// Where each sprite sits in the atlas: `[x, y, width, height]`.
pub type AtlasRect = [u16; 4];

/// All of a style's sprites, packed into one 8-bit image, and each sprite's
/// deltas (dents, open doors) as images of their own: the delta's pixels on
/// colour 0, so one drawn over the sprite patches it.
#[derive(Debug, Clone)]
pub struct SpriteAtlas {
    /// [`ATLAS_WIDTH`] wide.
    pub pixels: Vec<u8>,
    pub height: usize,
    /// Per sprite index.
    pub rects: Vec<AtlasRect>,
    /// Per sprite index, per delta.
    pub deltas: Vec<Vec<AtlasRect>>,
}

impl SpriteAtlas {
    /// Shelf-pack every sprite and delta, tallest first, with one pixel
    /// between them so nothing bleeds.
    pub fn new(style: &Style) -> Self {
        const GAP: usize = 1;
        // (sprite, delta) pairs; `None` is the sprite itself.
        let mut items: Vec<(usize, Option<usize>)> = Vec::new();
        for (i, sp) in style.sprites.iter().enumerate() {
            items.push((i, None));
            items.extend((0..sp.deltas.len()).map(|k| (i, Some(k))));
        }
        let size = |i: usize| {
            let sp = &style.sprites[i];
            (sp.width as usize, sp.height as usize)
        };
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by_key(|&n| {
            let (w, h) = size(items[n].0);
            (std::cmp::Reverse(h), std::cmp::Reverse(w), n)
        });
        let mut placed = vec![[0u16; 4]; items.len()];
        let (mut x, mut y, mut shelf) = (0usize, 0usize, 0usize);
        for &n in &order {
            let (w, h) = size(items[n].0);
            if x + w > ATLAS_WIDTH {
                x = 0;
                y += shelf + GAP;
                shelf = 0;
            }
            placed[n] = [x as u16, y as u16, w as u16, h as u16];
            x += w + GAP;
            shelf = shelf.max(h);
        }
        let height = (y + shelf).max(1);
        let mut pixels = vec![0u8; ATLAS_WIDTH * height];
        let mut rects = vec![[0u16; 4]; style.sprites.len()];
        let mut deltas: Vec<Vec<AtlasRect>> = style
            .sprites
            .iter()
            .map(|sp| vec![[0u16; 4]; sp.deltas.len()])
            .collect();
        for (n, &(i, delta)) in items.iter().enumerate() {
            let r = placed[n];
            let (w, h) = size(i);
            let image = match delta {
                None => {
                    rects[i] = r;
                    style.sprite(i).map(|img| img.to_vec())
                }
                Some(k) => {
                    deltas[i][k] = r;
                    let mut img = vec![0u8; w * h];
                    style.apply_delta(i, k, &mut img).ok().map(|()| img)
                }
            };
            let Some(image) = image else {
                continue;
            };
            for row in 0..h {
                let at = (r[1] as usize + row) * ATLAS_WIDTH + r[0] as usize;
                pixels[at..at + w].copy_from_slice(&image[row * w..(row + 1) * w]);
            }
        }
        SpriteAtlas {
            pixels,
            height,
            rects,
            deltas,
        }
    }
}

/// Which colours a sprite is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpriteColours {
    /// Its own palette.
    Own,
    /// Remap `remap` (1..=12; 0 means none) of car record `car`.
    Car { car: usize, remap: u8 },
}

/// The palette, in [`crate::style_tables::StyleTables`] numbering, for a sprite.
pub fn sprite_palette(style: &Style, sprite: usize, colours: SpriteColours) -> u16 {
    match (&style.colour, colours) {
        (Colour::G24(c), SpriteColours::Car { car, remap }) if remap > 0 => c
            .remap_palette(car * gfa_formats::style::CAR_REMAPS + remap as usize - 1)
            .unwrap_or(0),
        (Colour::G24(c), _) => style
            .sprites
            .get(sprite)
            .and_then(|s| s.clut)
            .and_then(|k| c.sprite_palette(k))
            .unwrap_or(0),
        // GRY palette `k` is remap table `k`; a car's remaps name tables.
        (Colour::Gry(_), SpriteColours::Car { car, remap }) if remap > 0 => style
            .cars
            .get(car)
            .and_then(|c| c.remap8.get(remap as usize - 1))
            .map_or(0, |&t| u16::from(t)),
        (Colour::Gry(_), _) => 0,
    }
}

/// One sprite to draw. 28 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteInstance {
    /// Centre `x, y, z` in map pixels, then the draw angle in radians,
    /// clockwise ([`draw_angle`]).
    pub pos: [f32; 4],
    pub rect: AtlasRect,
    /// Palette, then flags (reserved).
    pub palette_flags: [u16; 2],
}

/// Sprites for one frame, sorted before upload.
#[derive(Debug, Default, Clone)]
pub struct SpriteBatch {
    items: Vec<(f32, u32, SpriteInstance)>,
}

impl SpriteBatch {
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn push(&mut self, instance: SpriteInstance) {
        let order = self.items.len() as u32;
        self.items.push((instance.pos[2], order, instance));
    }

    /// Add every sprite of `other`, after this batch's own.
    pub fn extend(&mut self, other: &SpriteBatch) {
        for &(_, _, instance) in &other.items {
            self.push(instance);
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Lowest first, then in the order added.
    fn sorted(&mut self) -> Vec<SpriteInstance> {
        self.items
            .sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        self.items.iter().map(|&(_, _, i)| i).collect()
    }
}

/// Height in map pixels of a startup object's `z`, which counts down from
/// the top of the six-layer map (255 is ground level, 128 px up). Carnage3D
/// `CreateStartupObjects` inverts the layer the same way.
pub fn startup_height(z: i16) -> f32 {
    f32::from(6 * 64 - 1 - z)
}

/// A startup object's heading: its rotation field counts 1024 steps a turn,
/// clockwise from north (256 is east, as Carnage3D reads it). Radians.
pub fn startup_heading(rotation: i16) -> f32 {
    f32::from(rotation) * std::f32::consts::TAU / 1024.0
}

/// The angle to draw a sprite at for a heading: the art faces south.
pub fn draw_angle(heading: f32) -> f32 {
    heading + std::f32::consts::PI
}

/// Render pixels per simulation pixel: the simulation's blocks are 32
/// pixels across (AmiGTA1's units), the renderer's 64.
pub const SIM_SCALE: f32 = 2.0;

/// A simulation position (16.16 world pixels, 32 to a block and to a
/// layer) in render pixels.
pub fn from_sim(pos16: [i32; 3]) -> [f32; 3] {
    pos16.map(|v| v as f32 / 65536.0 * SIM_SCALE)
}

/// A simulation angle (256 steps a turn, clockwise from north) in radians.
pub fn sim_angle(angle: i32) -> f32 {
    (angle & 255) as f32 * std::f32::consts::TAU / 256.0
}

/// A sprite drawn in its own colours at `pos`, turned `angle` radians
/// clockwise.
pub fn own_sprite(
    style: &Style,
    atlas: &SpriteAtlas,
    sprite: usize,
    pos: [f32; 3],
    angle: f32,
) -> Option<SpriteInstance> {
    Some(SpriteInstance {
        pos: [pos[0], pos[1], pos[2], angle],
        rect: *atlas.rects.get(sprite)?,
        palette_flags: [sprite_palette(style, sprite, SpriteColours::Own), 0],
    })
}

/// A vehicle: car table record `car` in colour remap `remap` (0 for its
/// own colours) at `pos`, turned `angle` radians clockwise.
pub fn car_sprite(
    style: &Style,
    atlas: &SpriteAtlas,
    car: usize,
    remap: u8,
    pos: [f32; 3],
    angle: f32,
) -> Option<SpriteInstance> {
    let mut out = Vec::new();
    push_car(&mut out, style, atlas, car, remap, pos, angle, 0);
    out.first().copied()
}

/// A vehicle with its deltas shown: bit `n` of `deltas` draws delta `n`
/// (dents, an open door) over it, in the car's own colours. Pushes nothing
/// if the model has no sprite.
#[allow(clippy::too_many_arguments)]
pub fn push_car(
    out: &mut Vec<SpriteInstance>,
    style: &Style,
    atlas: &SpriteAtlas,
    car: usize,
    remap: u8,
    pos: [f32; 3],
    angle: f32,
    deltas: u32,
) {
    let Some(sprite) = style.cars.get(car).and_then(|c| style.car_sprite(c)) else {
        return;
    };
    let Some(&rect) = atlas.rects.get(sprite) else {
        return;
    };
    let palette = sprite_palette(style, sprite, SpriteColours::Car { car, remap });
    let at = [pos[0], pos[1], pos[2], angle];
    out.push(SpriteInstance {
        pos: at,
        rect,
        palette_flags: [palette, 0],
    });
    // Same place and height, added after: drawn on top of the car.
    for (k, &rect) in atlas.deltas[sprite].iter().enumerate().take(32) {
        if deltas & (1 << k) != 0 {
            out.push(SpriteInstance {
                pos: at,
                rect,
                palette_flags: [palette, 0],
            });
        }
    }
}

/// Map object `object`'s sprite, frame `frame` of it (fires and other
/// animated objects keep their frames in a run of sprites).
pub fn object_sprite(
    style: &Style,
    atlas: &SpriteAtlas,
    object: usize,
    frame: usize,
    pos: [f32; 3],
    angle: f32,
) -> Option<SpriteInstance> {
    let sprite = style.object_sprite(style.objects.get(object)?)? + frame;
    own_sprite(style, atlas, sprite, pos, angle)
}

/// The map's startup objects that `keep` accepts, as sprites: street
/// furniture, and the vehicles the simulation does not own (trains and
/// trams, until it does). Returns how many were skipped for want of a
/// sprite.
pub fn push_startup_objects(
    map: &gfa_formats::cmp::Cmp,
    style: &Style,
    atlas: &SpriteAtlas,
    batch: &mut SpriteBatch,
    keep: impl Fn(&gfa_formats::cmp::StartupObject) -> bool,
) -> usize {
    let mut skipped = 0;
    for o in map.objects.iter().filter(|o| keep(o)) {
        // A remap byte of 128 and up marks a car whose model is `kind`.
        let found = if o.remap >= 128 {
            style
                .cars
                .iter()
                .position(|c| c.model == o.kind)
                .and_then(|car| {
                    let sprite = style.car_sprite(&style.cars[car])?;
                    let colours = SpriteColours::Car {
                        car,
                        remap: o.remap - 128,
                    };
                    Some((sprite, colours))
                })
        } else {
            style
                .objects
                .get(o.kind as usize)
                .and_then(|info| style.object_sprite(info))
                .map(|s| (s, SpriteColours::Own))
        };
        let Some((sprite, colours)) = found else {
            skipped += 1;
            continue;
        };
        batch.push(SpriteInstance {
            pos: [
                f32::from(o.x),
                f32::from(o.y),
                startup_height(o.z),
                draw_angle(startup_heading(o.rotation)),
            ],
            rect: atlas.rects[sprite],
            palette_flags: [sprite_palette(style, sprite, colours), 0],
        });
    }
    skipped
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    camera: [f32; 4],
}

/// The sprite pipeline and atlas.
pub struct SpriteRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    capacity: usize,
    count: u32,
}

impl SpriteRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        atlas: &SpriteAtlas,
        palettes: &wgpu::TextureView,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: ATLAS_WIDTH as u32,
            height: atlas.height as u32,
            depth_or_array_layers: 1,
        };
        let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sprite atlas"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ATLAS_WIDTH as u32),
                rows_per_image: None,
            },
            size,
        );
        let atlas_view = atlas_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tex_entry = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sprites"),
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
                tex_entry(1, wgpu::TextureSampleType::Uint),
                tex_entry(2, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sprites"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(palettes),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprites.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sprites.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprites"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let attributes = [
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint16x4,
                offset: 16,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint16x2,
                offset: 24,
                shader_location: 2,
            },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprites"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SpriteInstance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(target_format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        let capacity = 1024;
        SpriteRenderer {
            pipeline,
            bind_group,
            globals,
            instances: Self::instance_buffer(device, capacity),
            capacity,
            count: 0,
        }
    }

    fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite instances"),
            size: (capacity * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Upload this frame's camera and sprites.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: &Camera,
        aspect: f32,
        batch: &mut SpriteBatch,
    ) {
        let g = Globals {
            view_proj: camera.view_proj(aspect).to_cols_array_2d(),
            camera: camera.eye.extend(1.0).to_array(),
        };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));
        let sorted = batch.sorted();
        if sorted.len() > self.capacity {
            self.capacity = sorted.len().next_power_of_two();
            self.instances = Self::instance_buffer(device, self.capacity);
        }
        queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&sorted));
        self.count = sorted.len() as u32;
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        pass.draw(0..6, 0..self.count);
    }

    pub fn count(&self) -> u32 {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(z: f32, tag: u16) -> SpriteInstance {
        SpriteInstance {
            pos: [0.0, 0.0, z, 0.0],
            rect: [tag, 0, 1, 1],
            palette_flags: [0, 0],
        }
    }

    #[test]
    fn startup_objects_stand_on_the_ground_and_face_their_heading() {
        // z counts down from the map's top: 255 is the ground at 128 px.
        assert_eq!(startup_height(255), 128.0);
        assert_eq!(startup_height(191), 192.0);
        // Rotation 256 is east; the art faces south, so it turns 270°.
        let east = startup_heading(256);
        assert!((east - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((draw_angle(east) - 3.0 * std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn batches_sort_by_height_then_order() {
        let mut b = SpriteBatch::default();
        b.push(inst(128.0, 1));
        b.push(inst(64.0, 2));
        b.push(inst(128.0, 3));
        let order: Vec<u16> = b.sorted().iter().map(|i| i.rect[0]).collect();
        assert_eq!(order, [2, 1, 3]);
    }
}
