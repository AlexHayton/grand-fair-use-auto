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
    /// The simulation's sprites this frame: the traffic lights, the crowd
    /// (first, so a car at the same height is drawn over a body), standing
    /// cars, the player's car (dents, door, fire) and the player.
    pub fn game_sprites(&self, game: &crate::game::Game) -> Vec<SpriteInstance> {
        use gfa_render::sprites::{from_sim, object_sprite, ped_sprite, push_car, sim_angle};
        use gfa_sim::weapons::{self, Kind};
        let world = &game.world;
        let frame = game.frame();
        let style = &self.level.style;
        let mut out = Vec::with_capacity(world.traffic.n + world.peds.p.len() + 64);
        // The traffic lights first: on the pavement, under whoever passes.
        for (light, pos, angle) in game.lamps() {
            if let Some(sprite) = style.sprite_index(SpriteCategory::TrafficLight, light.sprite()) {
                out.extend(own_sprite(style, &self.atlas, sprite, pos, angle));
            }
        }
        // Where bullets stopped, on the ground under everybody.
        let ground = |x: i64, y: i64, layer: i32, lift: i32| {
            let z = world.map.ground_z(x as i32, y as i32, layer);
            from_sim([x as i32, y as i32, z + (lift << 16)])
        };
        for sp in world.weapons.splats.iter().filter(|s| s.ticks > 0) {
            let pos = ground(sp.x, sp.y, sp.layer, 0);
            out.extend(object_sprite(style, &self.atlas, OBJ_SPLAT, 0, pos, 0.0));
        }
        for m in world.marks.ground_sprites() {
            let pos = ground(m.x, m.y, m.layer, 0);
            out.extend(object_sprite(
                style,
                &self.atlas,
                m.object,
                m.frame,
                pos,
                sim_angle(m.angle + 128),
            ));
        }
        for p in &world.pickups.p {
            use gfa_sim::pickups::{OBJ_CRATE, OBJ_CRATE_OPEN, PickupState, item_object};
            match p.state {
                PickupState::Gone => {}
                PickupState::Crate => {
                    let pos = ground(p.x, p.y, p.layer, 0);
                    out.extend(object_sprite(style, &self.atlas, OBJ_CRATE, 0, pos, 0.0));
                }
                PickupState::Open => {
                    let pos = ground(p.x, p.y, p.layer, 0);
                    out.extend(object_sprite(
                        style,
                        &self.atlas,
                        OBJ_CRATE_OPEN,
                        0,
                        pos,
                        0.0,
                    ));
                    if let Some(obj) = item_object(p.kind) {
                        out.extend(object_sprite(style, &self.atlas, obj, 0, pos, 0.0));
                    }
                }
            }
        }
        for p in game.peds() {
            let pos = p.pose.pos.to_array();
            out.extend(ped_sprite(
                style,
                &self.atlas,
                p.frame,
                p.table,
                pos,
                p.pose.draw_angle,
            ));
            if let Some(f) = p.burning {
                out.extend(flames(style, &self.atlas, f, pos));
            }
        }
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
        for t in game.traffic() {
            if t.wrecked
                && let Some(ci) = style.cars.get(t.model)
                && let Some(ws) = gfa_render::sprites::wreck_sprite(style, ci.vtype)
            {
                out.extend(own_sprite(
                    style,
                    &self.atlas,
                    ws,
                    t.pose.pos.to_array(),
                    t.pose.draw_angle,
                ));
            } else {
                push_car(
                    &mut out,
                    style,
                    &self.atlas,
                    t.model,
                    t.remap,
                    t.pose.pos.to_array(),
                    t.pose.draw_angle,
                    t.deltas,
                );
            }
            if t.cop == 4 {
                use gfa_sim::gfa_fixed::trig::{cos, sin};
                let ba = (t.face + 64) & 255;
                let bx = t.x + i64::from(sin(ba)) * 32 * 4;
                let by = t.y - i64::from(cos(ba)) * 32 * 4;
                let bpos = ground(bx, by, t.layer, 0);
                out.extend(object_sprite(
                    style,
                    &self.atlas,
                    OBJ_BARRIER,
                    0,
                    bpos,
                    sim_angle(t.face + 128),
                ));
            }
            if t.fuse > 0 {
                out.extend(fire_sprite(
                    style,
                    &self.atlas,
                    t.fuse,
                    t.pose.pos.to_array(),
                ));
            }
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
        for c in world.carriages() {
            let (x, y) = (c.body.0 as i32, c.body.1 as i32);
            let z = world.map.ground_z(x, y, c.layer);
            let xy = |p: (i64, i64)| {
                let r = from_sim([p.0 as i32, p.1 as i32, 0]);
                [r[0], r[1]]
            };
            gfa_render::sprites::push_carriage(
                &mut out,
                style,
                &self.atlas,
                xy(c.body),
                xy(c.front),
                xy(c.back),
                from_sim([0, 0, z])[2],
                sim_angle(c.angle + 128),
                c.first,
                c.last,
            );
        }
        if let Some((pose, f)) = frame.player {
            out.extend(self.ped_sprite(f, pose.pos.to_array(), pose.draw_angle));
        }
        // In the air, over everything: the shots, the fires and the
        // explosions. The art of a shot faces south; the rocket's and the
        // flame's point back along the flight.
        let w = &world.weapons;
        for sh in w.shots.iter().filter(|s| s.alive) {
            let pos = ground(sh.x, sh.y, sh.layer, 4);
            let (object, frame, angle) = match sh.kind {
                Kind::Bullet => (OBJ_BULLET, 0, sh.heading + 128),
                Kind::Rocket => (OBJ_ROCKET, sh.frame - 1, sh.heading),
                Kind::Flame => (OBJ_FLAME, sh.frame - 1, sh.heading),
            };
            let frame = usize::try_from(frame).unwrap_or(0);
            out.extend(object_sprite(
                style,
                &self.atlas,
                object,
                frame,
                pos,
                sim_angle(angle),
            ));
        }
        for f in w.fires.iter().filter(|f| f.ticks > 0) {
            let pos = ground(f.x, f.y, f.layer, 0);
            out.extend(flames(style, &self.atlas, f.frame as usize, pos));
        }
        // An explosion is four quarter sprites a block across, each with
        // its own run of frames, a pixel short of a block apart down the
        // screen so the halves meet.
        for b in w.blasts.iter().filter(|b| b.frame > 0) {
            for q in 0..4usize {
                let dx = if q & 1 != 0 { 16 } else { -16 };
                let dy = if q & 2 != 0 { 15 } else { -15 };
                let n = q * weapons::BLAST_FRAMES as usize + (b.frame - 1) as usize;
                let Some(sprite) = style.sprite_index(SpriteCategory::Ex, n) else {
                    continue;
                };
                let pos = ground(b.x + (dx << 16), b.y + (dy << 16), b.layer, 8);
                out.extend(own_sprite(style, &self.atlas, sprite, pos, 0.0));
            }
        }
        for j in world.service.water_jets(&world.traffic) {
            let obj = if j.end {
                gfa_sim::service::OBJ_WATER_JET_END
            } else {
                gfa_sim::service::OBJ_WATER_JET
            };
            let pos = ground(j.x, j.y, j.layer, 2);
            out.extend(object_sprite(
                style,
                &self.atlas,
                obj,
                0,
                pos,
                sim_angle(j.angle + 128),
            ));
        }
        for c in world.marks.air_sprites() {
            let pos = ground(c.x, c.y, c.layer, 10);
            out.extend(object_sprite(
                style,
                &self.atlas,
                c.object,
                c.frame,
                pos,
                0.0,
            ));
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
        // Past 21:9 the scene is drawn in a band, black either side.
        let [x, y, w, h] = gfa_render::camera::pillarbox(size.0, size.1);
        let aspect = w as f32 / h.max(1) as f32;
        self.city.prepare(&gpu.queue, camera, aspect);
        self.batch.clear();
        self.batch.extend(&self.statics);
        for &s in extra {
            self.batch.push(s);
        }
        self.sprites
            .prepare(&gpu.device, &gpu.queue, camera, aspect, &mut self.batch);
        let mut pass = gfa_render::city::begin_scene(encoder, view, &self.depth, SKY);
        pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
        pass.set_scissor_rect(x, y, w, h);
        self.city.draw(&mut pass);
        self.sprites.draw(&mut pass);
    }

    /// The top block under a window pixel, described for the overlay.
    pub fn inspect(&self, camera: &Camera, size: (u32, u32), px: [f32; 2]) -> Option<String> {
        // In the band the scene is drawn in.
        let [x, y, w, h] = gfa_render::camera::pillarbox(size.0, size.1);
        let (w, h) = (w as f32, h.max(1) as f32);
        let (px, py) = (px[0] - x as f32, px[1] - y as f32);
        if !(0.0..w).contains(&px) || !(0.0..h).contains(&py) {
            return None;
        }
        let aspect = w / h;
        let ndc = [px / w * 2.0 - 1.0, 1.0 - py / h * 2.0];
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
    flames(style, atlas, ((fuse / 3) % 7) as usize, pos)
}

/// Frame `frame` of the fire object at `pos`.
fn flames(
    style: &gfa_formats::style::Style,
    atlas: &SpriteAtlas,
    frame: usize,
    pos: [f32; 3],
) -> Option<SpriteInstance> {
    const FIRE: usize = 0x2e;
    // A pixel above what burns, so the flames win against its top.
    let pos = [pos[0], pos[1], pos[2] + 1.0];
    gfa_render::sprites::object_sprite(style, atlas, FIRE, frame, pos, 0.0)
}

/// The map objects the weapons draw with.
const OBJ_SPLAT: usize = 0x0d;
const OBJ_BARRIER: usize = 0x18;
const OBJ_ROCKET: usize = 0x1f;
const OBJ_BULLET: usize = 0x4a;
const OBJ_FLAME: usize = 0x4b;

/// Behind everything: only visible past the map's edge.
pub const SKY: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 1.0,
};
