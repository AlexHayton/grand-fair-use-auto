//! A loaded level on the GPU: the city, the sprites, and what the inspector
//! needs to describe a block.

use gfa_assets::level::Level;
use gfa_formats::cmp::{Face, MAP_DIM, MAP_LAYERS};
use gfa_formats::style::SpriteCategory;
use gfa_render::camera::Camera;
use gfa_render::city::{CityRenderer, DepthTarget};
use gfa_render::city_mesh::{BLOCK_PX, CityMesh, TileBanks};
use gfa_render::sprites::{
    SpriteAtlas, SpriteBatch, SpriteInstance, SpriteRenderer, own_sprite, push_startup_objects,
};
use gfa_render::style_tables::{StyleTables, TileAnimator};
use gfa_render::{Gpu, wgpu};

pub struct Scene {
    pub level: Level,
    pub city: CityRenderer,
    sprites: SpriteRenderer,
    atlas: SpriteAtlas,
    /// The map's startup objects, drawn every frame. The simulation will
    /// own these once it spawns objects (M4 and on).
    statics: SpriteBatch,
    /// This frame's sprites: the statics plus what moves.
    batch: SpriteBatch,
    depth: DepthTarget,
}

impl Scene {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, size: (u32, u32), level: Level) -> Self {
        let tables = StyleTables::new(&level.style);
        let mesh = CityMesh::build(
            &level.map,
            TileBanks {
                side: level.style.side_tiles,
                lid: level.style.lid_tiles,
            },
        );
        log::info!(
            "{}: {} faces from {} blocks; {} tile animations",
            level.files.map,
            mesh.stats.faces,
            mesh.stats.blocks,
            level.style.animations.len()
        );
        let city = CityRenderer::new(
            &gpu.device,
            &gpu.queue,
            format,
            &tables,
            TileAnimator::new(&level.style),
            &mesh,
        );
        let atlas = SpriteAtlas::new(&level.style);
        let sprites =
            SpriteRenderer::new(&gpu.device, &gpu.queue, format, &atlas, city.palette_view());
        let mut statics = SpriteBatch::default();
        let skipped = push_startup_objects(&level.map, &level.style, &atlas, &mut statics, |o| {
            // The simulation owns the parked vehicles and draws them itself.
            gfa_sim::cars::sim_vehicle(o, &level.style.cars).is_none()
        });
        if skipped > 0 {
            log::warn!("{skipped} startup objects have no sprite");
        }
        Scene {
            level,
            city,
            sprites,
            atlas,
            statics,
            batch: SpriteBatch::default(),
            depth: DepthTarget::new(&gpu.device, size),
        }
    }

    /// A ped sprite: `frame` within the style's ped category, at `pos` (map
    /// pixels), turned `angle` radians clockwise.
    pub fn ped_sprite(&self, frame: u16, pos: [f32; 3], angle: f32) -> Option<SpriteInstance> {
        let style = &self.level.style;
        let sprite = style.sprite_index(SpriteCategory::Ped, usize::from(frame))?;
        own_sprite(style, &self.atlas, sprite, pos, angle)
    }
    /// The simulation's sprites this frame: standing cars, the player's
    /// car (dents, door, fire) and the player.
    pub fn game_sprites(&self, game: &crate::game::Game) -> Vec<SpriteInstance> {
        use gfa_render::sprites::{from_sim, push_car, sim_angle};
        let world = &game.world;
        let frame = game.frame();
        let style = &self.level.style;
        let mut out = Vec::with_capacity(world.parked.len() + 8);
        for c in world.standing_cars() {
            let angle = sim_angle(c.face + 128);
            push_car(
                &mut out,
                style,
                &self.atlas,
                c.model,
                c.remap,
                from_sim(c.pos),
                angle,
                c.deltas,
            );
        }
        if let (Some((pose, model, remap)), Some((deltas, fuse))) = (frame.car, world.driven_look())
        {
            let pos = pose.pos.to_array();
            push_car(
                &mut out,
                style,
                &self.atlas,
                model,
                remap,
                pos,
                pose.draw_angle,
                deltas,
            );
            if fuse > 0 {
                out.extend(fire_sprite(style, &self.atlas, fuse, pos));
            }
        }
        if let Some((pose, f)) = frame.player {
            out.extend(self.ped_sprite(f, pose.pos.to_array(), pose.draw_angle));
        }
        out
    }

    /// Draw the city, its objects and `extra` sprites.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: (u32, u32),
        camera: &Camera,
        seconds: f64,
        extra: &[SpriteInstance],
    ) {
        self.depth.fit(&gpu.device, size);
        self.city.animate(&gpu.queue, seconds);
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        self.city.prepare(&gpu.queue, camera, aspect);
        self.batch.clear();
        self.batch.extend(&self.statics);
        for &s in extra {
            self.batch.push(s);
        }
        self.sprites
            .prepare(&gpu.device, &gpu.queue, camera, aspect, &mut self.batch);
        let mut pass = gfa_render::city::begin_scene(encoder, view, &self.depth, SKY);
        self.city.draw(&mut pass);
        self.sprites.draw(&mut pass);
    }

    /// The top block under a window pixel, described for the overlay.
    pub fn inspect(&self, camera: &Camera, size: (u32, u32), px: [f32; 2]) -> Option<String> {
        let aspect = size.0 as f32 / size.1.max(1) as f32;
        let ndc = [
            px[0] / size.0 as f32 * 2.0 - 1.0,
            1.0 - px[1] / size.1 as f32 * 2.0,
        ];
        let map = &self.level.map;
        let block_px = f32::from(BLOCK_PX);
        // Walk down the layers: the first lid the ray meets is what shows.
        for z in (0..MAP_LAYERS).rev() {
            let hit = camera.ray_to_plane(aspect, ndc, (z + 1) as f32 * block_px)?;
            let (bx, by) = ((hit.x / block_px).floor(), (hit.y / block_px).floor());
            if bx < 0.0 || by < 0.0 || bx >= MAP_DIM as f32 || by >= MAP_DIM as f32 {
                continue;
            }
            let (bx, by) = (bx as usize, by as usize);
            let Some(b) = map.block(bx, by, z) else {
                continue;
            };
            if b.face(Face::Lid) == 0 && z > 0 {
                continue;
            }
            let district = map
                .districts
                .iter()
                .rev()
                .find(|d| {
                    !d.is_empty()
                        && (d.x as usize..d.x as usize + d.w as usize).contains(&bx)
                        && (d.y as usize..d.y as usize + d.h as usize).contains(&by)
                })
                .map_or("", |d| d.name.as_str());
            return Some(format!(
                "block {bx},{by},{z}  {district}\n{:?} slope {} lid {} rot {} remap {}\nW {} E {} N {} S {}{}{}",
                b.ground_type(),
                b.slope(),
                b.face(Face::Lid),
                b.lid_rotation(),
                b.remap(),
                b.face(Face::West),
                b.face(Face::East),
                b.face(Face::North),
                b.face(Face::South),
                if b.is_flat() { "  flat" } else { "" },
                if b.is_railway() { "  railway" } else { "" },
            ));
        }
        None
    }
}

/// The flames on a burning car: the fire object's seven frames, a frame
/// every three ticks of the fuse (AmiGTA1).
pub fn fire_sprite(
    style: &gfa_formats::style::Style,
    atlas: &SpriteAtlas,
    fuse: i32,
    pos: [f32; 3],
) -> Option<SpriteInstance> {
    const FIRE: usize = 0x2e;
    let frame = ((fuse / 3) % 7) as usize;
    // A pixel above the car, so the flames win against its roof.
    let pos = [pos[0], pos[1], pos[2] + 1.0];
    gfa_render::sprites::object_sprite(style, atlas, FIRE, frame, pos, 0.0)
}

/// Behind everything: only visible past the map's edge.
pub const SKY: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
