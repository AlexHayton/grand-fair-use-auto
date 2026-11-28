//! `gfa-tool run`: play a test script on a level, headless.
//!
//! Prints the run log, state hashes and, with `--trace`, the player's state
//! every tick in the format the AmiGTA1 oracle harness prints. Fails when
//! the script's `# expect:` or `# reject:` checks do.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use gfa_assets::level::{self, Level};
use gfa_assets::{AssetSource, Dataset};
use gfa_render::city::DebugView;
use gfa_sim::input::InputLog;
use gfa_sim::replay::{self, RunOptions, trace_line};
use gfa_sim::script::Script;
use gfa_sim::view::SimView;
use gfa_sim::world::World;

use crate::render;

pub struct RunArgs {
    pub script: PathBuf,
    pub game: Dataset,
    pub section: Option<u32>,
    pub ticks: Option<u32>,
    pub hash_every: Option<u32>,
    pub trace: bool,
    pub trace_trains: bool,
    pub trace_traffic: bool,
    pub trace_peds: bool,
    pub oracle: bool,
    pub out: Option<PathBuf>,
    pub record: Option<PathBuf>,
}

pub fn run(src: &dyn AssetSource, args: RunArgs) -> Result<()> {
    let text = std::fs::read_to_string(&args.script)
        .with_context(|| format!("reading {}", args.script.display()))?;
    let script = Script::parse(&text)?;
    let section = args.section.or(script.section).unwrap_or(1);
    let files = level::find(src, args.game, section)?;
    println!(
        "run: {} [{section}] {} ({})",
        args.game, files.name, files.map
    );

    // The style gives the car table; the picture scene is only for --out.
    let level = Level::load(src, files.clone())?;
    let mut scene = match &args.out {
        Some(_) => Some(render::Scene::load_for_sim(
            src,
            &files.map,
            Some(&level.style_path),
            (640, 400),
        )?),
        None => None,
    };
    let view = if args.oracle {
        SimView::AMIGTA1
    } else {
        SimView::GAME
    };
    let gta = files.dataset == Dataset::Gta;
    let cars = level.style.cars.clone();
    let mut world = replay::level_world(
        level.map,
        cars,
        &files.mission,
        gta,
        view,
        script.start,
        !args.oracle,
    )
    .map_err(anyhow::Error::msg)?;
    if args.trace {
        println!("{}", trace_line(&world));
    }

    let mut recording = InputLog::default();
    let mut pictures = 0;
    let mut picture_error = None;
    let opts = RunOptions {
        ticks: args.ticks,
        hash_every: args.hash_every,
    };
    let report = replay::run_recorded(&mut world, &script, opts, &mut recording, |w, dump| {
        if args.trace {
            println!("{}", trace_line(w));
        }
        if args.trace_trains {
            println!("{}", gfa_sim::trains::trace_line(w.tick, &w.trains));
        }
        if args.trace_traffic {
            for line in w.traffic.trace_lines(u64::from(w.tick)) {
                println!("{line}");
            }
        }
        if args.trace_peds {
            for line in w.peds.trace_lines(w.tick) {
                println!("{line}");
            }
        }
        if let (true, Some(scene), Some(out)) = (dump, scene.as_mut(), &args.out) {
            let path = picture_path(out, pictures);
            pictures += 1;
            if let Err(e) = picture(scene, w, &path) {
                picture_error.get_or_insert(e);
            }
        }
    });
    if let Some(e) = picture_error {
        return Err(e);
    }

    print!("{}", report.to_text());
    if let Some(path) = &args.record {
        std::fs::write(path, recording.to_text())
            .with_context(|| format!("writing {}", path.display()))?;
        println!("run: input recorded to {}", path.display());
    }
    if pictures > 0 {
        println!("run: {pictures} pictures written");
    }
    let checks = script.expect.len() + script.reject.len();
    if report.failures.is_empty() {
        println!("run: {} ticks, PASS ({checks} checks)", report.ticks);
        Ok(())
    } else {
        for f in &report.failures {
            println!("FAIL: {f}");
        }
        bail!("{} of {checks} checks failed", report.failures.len())
    }
}

fn picture_path(out: &Path, n: usize) -> PathBuf {
    let name = out
        .file_name()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    out.with_file_name(format!("{name}{n:03}.png"))
}

/// The view from above the player, with him in it.
fn picture(scene: &mut render::Scene, world: &World, path: &Path) -> Result<()> {
    let rgba = render_world(scene, world);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    render::write_png(path, scene.size(), &rgba)
}

/// The camera straight down over the player, six blocks above his layer.
pub fn player_view(world: &World) -> render::View {
    let p = &world.player;
    let block = |v: i32| v as f32 / 65536.0 / 32.0;
    render::View {
        at: (block(p.x), block(p.y)),
        height: 6.0 + p.layer as f32,
        pitch: 0.0,
        yaw: 0.0,
    }
}

/// Render the world from [`player_view`], with everything it holds drawn.
pub fn render_world(scene: &mut render::Scene, world: &World) -> Vec<u8> {
    let extra = world_sprites(scene, world);
    scene.render_with(
        &player_view(world),
        DebugView::Normal,
        f64::from(world.tick) / 50.0,
        &extra,
    )
}

/// The simulation's sprites: the traffic lights, the crowd, standing cars,
/// the fleet, the player's car, the trains and the player.
pub fn world_sprites(
    scene: &render::Scene,
    world: &World,
) -> Vec<gfa_render::sprites::SpriteInstance> {
    let mut out = Vec::new();
    // The lights first: on the pavement, under whoever passes.
    for l in world.traffic.lamps(&world.map) {
        let (x, y) = l.pos();
        let pos = [x, y, world.map.ground_z(x, y, l.z)];
        out.extend(scene.lamp_sprite(world.traffic.lamp_light(&l).sprite(), pos, l.toward));
    }
    let at = |x: i64, y: i64, layer: i32, lift: i32| {
        let (x, y) = (x as i32, y as i32);
        [x, y, world.map.ground_z(x, y, layer) + (lift << 16)]
    };
    for m in world.marks.ground_sprites() {
        out.extend(scene.object_sprite(m.object, m.frame, at(m.x, m.y, m.layer, 0), m.angle + 128));
    }
    for p in &world.pickups.p {
        use gfa_sim::pickups::{OBJ_CRATE, PickupState, item_object};
        match p.state {
            PickupState::Gone => {}
            PickupState::Crate => {
                out.extend(scene.object_sprite(OBJ_CRATE, 0, at(p.x, p.y, p.layer, 0), 0));
            }
            PickupState::Open | PickupState::Loose => {
                if let Some(obj) = item_object(p.kind) {
                    out.extend(scene.object_sprite(obj, 0, at(p.x, p.y, p.layer, 0), 0));
                }
            }
        }
    }
    for s in &world.pickups.smashes {
        if let Some(frame) = s.frame() {
            out.extend(scene.object_sprite(
                gfa_sim::pickups::OBJ_CRATE_OPEN,
                frame,
                at(s.x, s.y, s.layer, 0),
                0,
            ));
        }
    }
    // The crowd first, so a car at the same height is drawn over a body.
    for p in world.peds.p.iter().filter(|p| p.shown()) {
        let (x, y) = (p.x as i32, p.y as i32);
        let pos = [x, y, world.map.ground_z(x, y, p.layer)];
        let table = p.remap.clamp(0, 255) as u8;
        out.extend(scene.ped_sprite(p.sprite_frame(), table, pos, p.angle + 128));
        if p.burn > 0 {
            out.extend(scene.object_sprite(
                0x2e,
                p.burn_frame.max(0) as usize,
                [x, y, pos[2] + (1 << 16)],
                0,
            ));
        }
    }
    let player_under = world.player_under_car();
    if player_under && let Some(frame) = world.player_frame() {
        let p = &world.player;
        out.extend(scene.ped_sprite(frame, 0, world.player_pos(), p.draw_angle()));
    }
    for c in world.standing_cars() {
        scene.push_car(&mut out, c.model, c.remap, c.pos, c.face + 128, c.deltas);
    }
    for c in world.traffic.live() {
        let (x, y) = (c.x as i32, c.y as i32);
        let pos = [x, y, world.map.ground_z(x, y, c.layer)];
        let remap = (c.remap + 1).clamp(0, 255) as u8;
        if c.wrecked
            && let Some(ws) = scene.wreck_sprite(c.model, pos, c.face + 128)
        {
            out.push(ws);
        } else {
            let lights = if c.cop >= 2 || c.svc == 2 {
                1 << (gfa_sim::cars::DELTA_LIGHT0
                    + (world.tick / gfa_sim::cars::LIGHT_FLASH_TICKS) % 2)
            } else {
                0
            };
            scene.push_car(
                &mut out,
                c.model,
                remap,
                pos,
                c.face + 128,
                c.dmg_bits | lights,
            );
            if world.is_tank(c.model) {
                push_turret(scene, &mut out, c.x, c.y, pos[2], c.face);
            }
        }
        if c.cop == 4 {
            use gfa_sim::gfa_fixed::trig::{cos, sin};
            let ba = (c.face + 64) & 255;
            let bx = c.x + i64::from(sin(ba)) * 32 * 4;
            let by = c.y - i64::from(cos(ba)) * 32 * 4;
            out.extend(scene.object_sprite(0x18, 0, at(bx, by, c.layer, 0), c.face + 128));
        }
        let fire = if c.fuse > 0 { c.fuse } else { c.fire_ticks };
        if fire > 0 {
            out.extend(scene.object_sprite(
                0x2e,
                ((fire / 3) % 7) as usize,
                [x, y, pos[2] + (1 << 16)],
                0,
            ));
        }
    }
    if let (Some(d), Some((deltas, _))) = (&world.driven, world.driven_look()) {
        let v = &d.veh;
        let z = d.z(&world.map) as i32;
        let pos = [v.ox as i32, v.oy as i32, z];
        scene.push_car(
            &mut out,
            v.model,
            v.remap.unwrap_or(0),
            pos,
            v.angle() + 128,
            deltas,
        );
        if world.is_tank(v.model) {
            push_turret(scene, &mut out, v.ox, v.oy, z, v.angle() + d.turret);
        }
    }
    for c in world.carriages() {
        let z = world
            .map
            .ground_z(c.body.0 as i32, c.body.1 as i32, c.layer);
        scene.push_carriage(&mut out, &c, z);
    }
    if !player_under && let Some(frame) = world.player_frame() {
        let p = &world.player;
        out.extend(scene.ped_sprite(frame, 0, world.player_pos(), p.draw_angle()));
    }
    push_weapons(scene, world, &mut out);
    out
}

/// A tank's turret on the hull centred at `(x, y)`, height `z`, its barrel
/// pointing `heading` (as the app draws it): the barrel, its centre
/// `BARREL_PX` ahead, then the dome over its base.
fn push_turret(
    scene: &render::Scene,
    out: &mut Vec<gfa_render::sprites::SpriteInstance>,
    x: i64,
    y: i64,
    z: i32,
    heading: i32,
) {
    use gfa_sim::gfa_fixed::trig::{cos, sin};
    use gfa_sim::weapons::{BARREL_PX, OBJ_BARREL, OBJ_TURRET};
    let bx = x + i64::from(sin(heading)) * BARREL_PX * 4;
    let by = y - i64::from(cos(heading)) * BARREL_PX * 4;
    out.extend(scene.object_sprite(
        OBJ_BARREL,
        0,
        [bx as i32, by as i32, z + (1 << 15)],
        heading,
    ));
    out.extend(scene.object_sprite(
        OBJ_TURRET,
        0,
        [x as i32, y as i32, z + (1 << 16)],
        heading + 128,
    ));
}

/// What the weapons leave in the picture: the splats on the ground, and in
/// the air the shots, the fires and the explosions (as the app draws them).
fn push_weapons(
    scene: &render::Scene,
    world: &World,
    out: &mut Vec<gfa_render::sprites::SpriteInstance>,
) {
    use gfa_sim::weapons::{BLAST_FRAMES, Kind};
    let at = |x: i64, y: i64, layer: i32, lift: i32| {
        let (x, y) = (x as i32, y as i32);
        [x, y, world.map.ground_z(x, y, layer) + (lift << 16)]
    };
    let w = &world.weapons;
    for s in w.splats.iter().filter(|s| s.ticks > 0) {
        out.extend(scene.object_sprite(0x0d, 0, at(s.x, s.y, s.layer, 0), 0));
    }
    for s in w.shots.iter().filter(|s| s.alive) {
        let (object, frame, angle) = match s.kind {
            Kind::Bullet => (0x4a, 0, s.heading + 128),
            Kind::Rocket | Kind::Shell => (0x1f, s.frame - 1, s.heading),
            Kind::Flame => (0x4b, s.frame - 1, s.heading),
        };
        let frame = usize::try_from(frame).unwrap_or(0);
        out.extend(scene.object_sprite(object, frame, at(s.x, s.y, s.layer, 4), angle));
    }
    for f in w.fires.iter().filter(|f| f.ticks > 0) {
        out.extend(scene.object_sprite(0x2e, f.frame as usize, at(f.x, f.y, f.layer, 1), 0));
    }
    for b in w.blasts.iter().filter(|b| b.frame > 0) {
        for q in 0..4usize {
            let dx: i64 = if q & 1 != 0 { 16 } else { -16 };
            let dy: i64 = if q & 2 != 0 { 15 } else { -15 };
            let n = q * BLAST_FRAMES as usize + (b.frame - 1) as usize;
            let pos = at(b.x + (dx << 16), b.y + (dy << 16), b.layer, 8);
            out.extend(scene.blast_sprite(n, pos));
        }
    }
    for j in world.service.water_jets(&world.traffic) {
        let obj = if j.end {
            gfa_sim::service::OBJ_WATER_JET_END
        } else {
            gfa_sim::service::OBJ_WATER_JET
        };
        out.extend(scene.object_sprite(obj, 0, at(j.x, j.y, j.layer, 2), j.angle + 128));
    }
    for c in world.marks.air_sprites() {
        out.extend(scene.object_sprite(c.object, c.frame, at(c.x, c.y, c.layer, 10), 0));
    }
}
