//! `gfa-tool render` and `gfa-tool holecheck`: the city drawn offscreen.
//!
//! The holecheck follows AmiGTA1's `tools/bin/holecheck.sh`: clear the frame
//! to a value nothing draws, render a spread of inland viewpoints, and count
//! what is left. Here the sentinel is alpha 0, which no face writes, so no
//! palette colour can be mistaken for a hole.

use std::path::Path;

use anyhow::{Context, Result};
use gfa_assets::AssetSource;
use gfa_formats::style::{SpriteCategory, Style};
use gfa_render::Gpu;
use gfa_render::camera::Camera;
use gfa_render::city::{CityRenderer, DebugView, DepthTarget, begin_scene};
use gfa_render::city_mesh::{BLOCK_PX, CityMesh, TileBanks};
use gfa_render::glam::Vec3;
use gfa_render::offscreen::{OFFSCREEN_FORMAT, Offscreen};
use gfa_render::sprites::{SpriteAtlas, SpriteBatch, SpriteInstance, SpriteRenderer};
use gfa_render::style_tables::{StyleTables, TileAnimator};

/// AmiGTA1's eight inland holecheck positions (blocks), chosen for NYC.
pub const HOLECHECK_POSITIONS: [(f32, f32); 8] = [
    (40.0, 40.0),
    (56.0, 58.0),
    (64.0, 64.0),
    (90.0, 70.0),
    (120.0, 120.0),
    (160.0, 90.0),
    (200.0, 200.0),
    (30.0, 180.0),
];

/// Where the camera is.
#[derive(Debug, Clone, Copy)]
pub struct View {
    /// Block coordinates of the point under the centre of the screen.
    pub at: (f32, f32),
    /// Eye height in blocks above the map's floor.
    pub height: f32,
    /// Degrees from straight down, and heading in degrees.
    pub pitch: f32,
    pub yaw: f32,
}

impl View {
    pub fn camera(&self) -> Camera {
        let px = f32::from(BLOCK_PX);
        let mut c = Camera::top_down(Vec3::new(self.at.0 * px, self.at.1 * px, self.height * px));
        c.pitch = self.pitch.to_radians();
        c.yaw = self.yaw.to_radians();
        c
    }
}

/// A map and style on a headless GPU.
pub struct Scene {
    gpu: Gpu,
    city: CityRenderer,
    sprites: SpriteRenderer,
    batch: SpriteBatch,
    /// Draw the sprites (off for the holecheck, which measures the city).
    pub show_sprites: bool,
    pub sprite_summary: String,
    target: Offscreen,
    depth: DepthTarget,
    pub style_path: String,
    style: Style,
    atlas: SpriteAtlas,
}

impl Scene {
    /// Load a map and its style, or `style` instead if given.
    pub fn load(
        src: &dyn AssetSource,
        map_path: &str,
        style: Option<&str>,
        size: (u32, u32),
    ) -> Result<Scene> {
        let (map, style_path, style) = match style {
            None => {
                let dataset = gfa_assets::level::dataset_for_map(src, map_path);
                gfa_assets::level::load_map(src, map_path, dataset)?
            }
            Some(p) => {
                let map = gfa_formats::cmp::Cmp::parse(&src.read(map_path)?)
                    .with_context(|| format!("parsing {map_path}"))?;
                let style = gfa_formats::style::Style::parse(&src.read(p)?)
                    .with_context(|| format!("parsing {p}"))?;
                (map, p.to_string(), style)
            }
        };
        let gpu = pollster::block_on(Gpu::headless()).context("opening the GPU")?;
        let tables = StyleTables::new(&style);
        let mesh = CityMesh::build(
            &map,
            TileBanks {
                side: style.side_tiles,
                lid: style.lid_tiles,
            },
        );
        let city = CityRenderer::new(
            &gpu.device,
            &gpu.queue,
            OFFSCREEN_FORMAT,
            &tables,
            TileAnimator::new(&style),
            &mesh,
        );
        let atlas = SpriteAtlas::new(&style);
        let sprites = SpriteRenderer::new(
            &gpu.device,
            &gpu.queue,
            OFFSCREEN_FORMAT,
            &atlas,
            city.palette_view(),
        );
        let mut batch = SpriteBatch::default();
        let skipped = gfa_render::sprites::push_startup_objects(&map, &style, &atlas, &mut batch);
        let sprite_summary = format!(
            "{} startup objects as sprites ({} without one); sprite atlas 2048x{}",
            batch.len(),
            skipped,
            atlas.height
        );
        let target = Offscreen::new(&gpu.device, size.0, size.1);
        let depth = DepthTarget::new(&gpu.device, size);
        Ok(Scene {
            gpu,
            city,
            sprites,
            batch,
            show_sprites: true,
            sprite_summary,
            target,
            depth,
            style_path,
            style,
            atlas,
        })
    }

    pub fn mesh_summary(&self) -> String {
        let s = &self.city.mesh_stats;
        format!(
            "{} blocks ({} flat, {} sloped) -> {} faces ({} lids); skipped {} door faces, {} bad tiles, {} empty slope faces",
            s.blocks,
            s.flat_blocks,
            s.sloped_blocks,
            s.faces,
            s.lids,
            s.door_faces,
            s.bad_tiles,
            s.degenerate
        )
    }

    /// Render one view; returns RGBA rows. Holes have alpha 0.
    pub fn render(&mut self, view: &View, debug: DebugView, seconds: f64) -> Vec<u8> {
        self.render_with(view, debug, seconds, &[])
    }

    /// Render with `extra` sprites besides the map's own.
    pub fn render_with(
        &mut self,
        view: &View,
        debug: DebugView,
        seconds: f64,
        extra: &[SpriteInstance],
    ) -> Vec<u8> {
        let aspect = self.target.width as f32 / self.target.height as f32;
        self.city.debug_view = debug;
        self.city.animate(&self.gpu.queue, seconds);
        let camera = view.camera();
        self.city.prepare(&self.gpu.queue, &camera, aspect);
        if self.show_sprites {
            let mut batch = self.batch.clone();
            for &s in extra {
                batch.push(s);
            }
            self.sprites.prepare(
                &self.gpu.device,
                &self.gpu.queue,
                &camera,
                aspect,
                &mut batch,
            );
        }
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu_label("render"));
        {
            let mut pass = begin_scene(
                &mut encoder,
                self.target.view(),
                &self.depth,
                gfa_render::wgpu::Color::TRANSPARENT,
            );
            self.city.draw(&mut pass);
            if self.show_sprites {
                self.sprites.draw(&mut pass);
            }
        }
        self.gpu.queue.submit([encoder.finish()]);
        self.target.read(&self.gpu)
    }

    /// A ped sprite: `frame` within the ped category, at a simulation
    /// position, drawn at a simulation angle.
    pub fn ped_sprite(
        &self,
        frame: u16,
        pos16: [i32; 3],
        draw_angle: i32,
    ) -> Option<SpriteInstance> {
        let sprite = self
            .style
            .sprite_index(SpriteCategory::Ped, usize::from(frame))?;
        gfa_render::sprites::own_sprite(
            &self.style,
            &self.atlas,
            sprite,
            gfa_render::sprites::from_sim(pos16),
            gfa_render::sprites::sim_angle(draw_angle),
        )
    }

    pub fn last_draw(&self) -> gfa_render::city::DrawStats {
        self.city.last_draw
    }

    pub fn size(&self) -> (u32, u32) {
        (self.target.width, self.target.height)
    }
}

fn wgpu_label(label: &str) -> gfa_render::wgpu::CommandEncoderDescriptor<'_> {
    gfa_render::wgpu::CommandEncoderDescriptor { label: Some(label) }
}

/// Pixels nothing was drawn on.
pub fn holes(rgba: &[u8]) -> usize {
    rgba.as_chunks::<4>().0.iter().filter(|p| p[3] == 0).count()
}

/// Write RGBA as a PNG, showing holes as magenta.
pub fn write_png(path: &Path, (w, h): (u32, u32), rgba: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let rgb: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            if p[3] == 0 {
                [255, 0, 255]
            } else {
                [p[0], p[1], p[2]]
            }
        })
        .collect();
    let file =
        std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgb)?;
    Ok(())
}
