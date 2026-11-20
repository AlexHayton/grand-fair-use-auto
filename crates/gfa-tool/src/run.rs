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
    pub amigta1_view: bool,
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

    // The style is only needed for pictures.
    let (map, mut scene) = match &args.out {
        Some(_) => {
            let level = Level::load(src, files.clone())?;
            let scene = render::Scene::load(src, &files.map, Some(&level.style_path), (640, 400))?;
            (level.map, Some(scene))
        }
        None => {
            let map = gfa_formats::cmp::Cmp::parse(&src.read(&files.map)?)
                .with_context(|| format!("parsing {}", files.map))?;
            (map, None)
        }
    };
    let view = if args.amigta1_view {
        SimView::AMIGTA1
    } else {
        SimView::GAME
    };
    let gta = files.dataset == Dataset::Gta;
    let mut world = replay::level_world(map, &files.mission, gta, view, script.start)
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
    let p = &world.player;
    let pos = world.player_pos();
    let block = |v: i32| v as f32 / 65536.0 / 32.0;
    let view = render::View {
        at: (block(p.x), block(p.y)),
        height: 6.0 + p.layer as f32,
        pitch: 0.0,
        yaw: 0.0,
    };
    let sprite = scene.ped_sprite(p.ped_frame(), pos, p.draw_angle());
    let extra: Vec<_> = sprite.into_iter().collect();
    let rgba = scene.render_with(
        &view,
        DebugView::Normal,
        f64::from(world.tick) / 50.0,
        &extra,
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    render::write_png(path, scene.size(), &rgba)
}
