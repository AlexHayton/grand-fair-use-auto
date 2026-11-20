//! `gfa-tool`: developer CLI for inspecting the user's game data.
//!
//! Format dumps and art export (M1), headless rendering and the holecheck
//! (M2), and script runs against the simulation (M3).

mod export;
mod sheet;

use gfa_tool::{render, run};

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use gfa_assets::{AssetSource, DataPath, Dataset, Datasets, FsSource};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Folder containing GTADATA/. Overrides GFA_DATA and gfa.local.toml.
    #[arg(long, global = true)]
    data: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show where the data is, which games it holds, and its files.
    Info {
        /// Also list every file under GTADATA/.
        #[arg(long)]
        files: bool,
    },
    /// Summarise a .CMP city map, or dump one column of it.
    Map {
        /// Game path, e.g. GTADATA/NYC.CMP (any case).
        path: String,
        /// Dump the column at block X,Y.
        #[arg(long, value_parser = parse_xy)]
        at: Option<(usize, usize)>,
    },
    /// Summarise a .GRY or .G24 style file.
    Style {
        /// Game path, e.g. GTADATA/STYLE001.G24 (any case).
        path: String,
        /// List every vehicle in the car table.
        #[arg(long)]
        cars: bool,
    },
    /// List a mission file's sections, or tally one section's declarations
    /// and commands.
    Mission {
        /// Game path, e.g. GTADATA/MISSION.INI.
        path: String,
        /// Show this section in detail.
        #[arg(long)]
        section: Option<u32>,
        /// With --section: print every declaration and command too.
        #[arg(long)]
        dump: bool,
    },
    /// Look up texts in an .FXT table.
    Text {
        /// Game path, e.g. GTADATA/ENGLISH.FXT.
        path: String,
        /// Print only this key.
        #[arg(long)]
        key: Option<String>,
        /// Print only texts containing this (case-insensitive).
        #[arg(long)]
        grep: Option<String>,
    },
    /// Write contact-sheet PNGs: a style's tiles, sprites, remaps and deltas,
    /// or a font's glyphs.
    Export {
        /// Game path of a style (.GRY, .G24) or font (.FON).
        path: String,
        /// Output folder. Defaults to export/<file name>/, which git ignores.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Draw a map offscreen to a PNG. Holes (nothing drawn) show magenta.
    Render {
        /// Game path of a map, e.g. GTADATA/NYC.CMP.
        path: String,
        /// Block under the centre of the picture.
        #[arg(long, value_parser = parse_xy_f, default_value = "128,128")]
        at: (f32, f32),
        /// Eye height in blocks (the ground is mostly at 2, roofs up to 6).
        #[arg(long, default_value_t = 10.0)]
        height: f32,
        /// Degrees of tilt from straight down.
        #[arg(long, default_value_t = 0.0)]
        pitch: f32,
        /// Heading in degrees (0: north up).
        #[arg(long, default_value_t = 0.0)]
        yaw: f32,
        /// Picture size.
        #[arg(long, value_parser = parse_xy, default_value = "1280,800")]
        size: (usize, usize),
        /// Debug tint: faces or flat.
        #[arg(long)]
        debug: Option<String>,
        /// Seconds of tile animation.
        #[arg(long, default_value_t = 0.0)]
        time: f64,
        /// Style to use instead of the map's own (G24 by default), e.g.
        /// GTADATA/STYLE001.GRY.
        #[arg(long)]
        style: Option<String>,
        /// Output file. Defaults to export/render/<map>_<x>_<y>.png.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Play a test script on a level and check its `# expect:` lines.
    Run {
        /// Script file: AmiGTA1 orders (`wait`, `run`, `goto`, …) or walk
        /// legs (`turn forward walk ticks`).
        script: PathBuf,
        /// Which game: gta, london69 or london61.
        #[arg(long, default_value = "gta", value_parser = Dataset::from_str)]
        game: Dataset,
        /// Mission section to load; overrides the script's `# section:`.
        #[arg(long)]
        section: Option<u32>,
        /// Ticks to run (50 a second); defaults to the script's length.
        #[arg(long)]
        ticks: Option<u32>,
        /// Print the state hash every K ticks.
        #[arg(long)]
        hash_every: Option<u32>,
        /// Print the player's state every tick (the oracle's trace format).
        #[arg(long)]
        trace: bool,
        /// Print every train's state every tick (traintrace's format).
        #[arg(long)]
        trace_trains: bool,
        /// Run as AmiGTA1's harnesses do, to compare with the oracle: its
        /// view size, and no parked cars from the map.
        #[arg(long)]
        oracle: bool,
        /// Write a PNG for each `dump`/`film` tick: PREFIX000.png, …
        #[arg(long)]
        out: Option<PathBuf>,
        /// Save the input as a recording.
        #[arg(long)]
        record: Option<PathBuf>,
    },
    /// Count undrawn pixels at AmiGTA1's eight inland viewpoints (holecheck.sh).
    Holecheck {
        /// Game path of a map. Defaults to GTADATA/NYC.CMP.
        #[arg(default_value = "GTADATA/NYC.CMP")]
        path: String,
        /// Eye height in blocks.
        #[arg(long, default_value_t = 10.0)]
        height: f32,
        /// Also write each view to export/holecheck/.
        #[arg(long)]
        save: bool,
    },
}

fn parse_xy(s: &str) -> Result<(usize, usize), String> {
    let (x, y) = s.split_once(',').ok_or("expected X,Y")?;
    Ok((
        x.trim().parse().map_err(|e| format!("{e}"))?,
        y.trim().parse().map_err(|e| format!("{e}"))?,
    ))
}

fn parse_xy_f(s: &str) -> Result<(f32, f32), String> {
    let (x, y) = s.split_once(',').ok_or("expected X,Y")?;
    Ok((
        x.trim().parse().map_err(|e| format!("{e}"))?,
        y.trim().parse().map_err(|e| format!("{e}"))?,
    ))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let data = DataPath::resolve(cli.data.as_deref()).context("finding the game data")?;
    let src = FsSource::new(&data.root);
    match cli.command {
        Command::Info { files } => info(&data, &src, files),
        Command::Map { path, at } => map(&src, &path, at),
        Command::Style { path, cars } => style(&src, &path, cars),
        Command::Mission {
            path,
            section,
            dump,
        } => mission(&src, &path, section, dump),
        Command::Text { path, key, grep } => text(&src, &path, key.as_deref(), grep.as_deref()),
        Command::Render {
            path,
            at,
            height,
            pitch,
            yaw,
            size,
            debug,
            time,
            style,
            out,
        } => {
            use gfa_render::city::DebugView;
            let debug = match debug.as_deref() {
                None => DebugView::Normal,
                Some("faces") => DebugView::Faces,
                Some("flat") => DebugView::Flat,
                Some(d) => anyhow::bail!("unknown debug view {d:?} (faces, flat)"),
            };
            let size = (size.0 as u32, size.1 as u32);
            let mut scene = render::Scene::load(&src, &path, style.as_deref(), size)?;
            println!("{path} with {}: {}", scene.style_path, scene.mesh_summary());
            println!("{}", scene.sprite_summary);
            let view = render::View {
                at,
                height,
                pitch,
                yaw,
            };
            let rgba = scene.render(&view, debug, time);
            let stem = path
                .rsplit('/')
                .next()
                .unwrap_or(&path)
                .to_ascii_lowercase();
            let stem = stem.split('.').next().unwrap_or("map");
            let out = out.unwrap_or_else(|| {
                PathBuf::from(format!("export/render/{stem}_{}_{}.png", at.0, at.1))
            });
            render::write_png(&out, size, &rgba)?;
            let d = scene.last_draw();
            println!(
                "wrote {} ({} chunks, {} draws, {} triangles; {} holes)",
                out.display(),
                d.chunks,
                d.draws,
                d.triangles,
                render::holes(&rgba)
            );
            Ok(())
        }
        Command::Run {
            script,
            game,
            section,
            ticks,
            hash_every,
            trace,
            trace_trains,
            oracle,
            out,
            record,
        } => run::run(
            &src,
            run::RunArgs {
                script,
                game,
                section,
                ticks,
                hash_every,
                trace,
                trace_trains,
                oracle,
                out,
                record,
            },
        ),
        Command::Holecheck { path, height, save } => {
            let mut scene = render::Scene::load(&src, &path, None, (640, 400))?;
            scene.show_sprites = false;
            let mut total = 0;
            for &at in &render::HOLECHECK_POSITIONS {
                let view = render::View {
                    at,
                    height,
                    pitch: 0.0,
                    yaw: 0.0,
                };
                let rgba = scene.render(&view, gfa_render::city::DebugView::Normal, 0.0);
                let n = render::holes(&rgba);
                println!(
                    "block {},{}  uncovered {n} of {}",
                    at.0,
                    at.1,
                    rgba.len() / 4
                );
                if save {
                    let out = PathBuf::from(format!("export/holecheck/{}_{}.png", at.0, at.1));
                    render::write_png(&out, scene.size(), &rgba)?;
                }
                total += n;
            }
            println!("--- total uncovered pixels: {total} ---");
            if total > 0 {
                anyhow::bail!("{total} uncovered pixels");
            }
            Ok(())
        }
        Command::Export { path, out } => {
            let file = path
                .rsplit('/')
                .next()
                .unwrap_or(&path)
                .to_ascii_lowercase();
            if let Some(stem) = file.strip_suffix(".fon") {
                let f = gfa_formats::fon::Font::parse(&src.read(&path)?)
                    .with_context(|| format!("parsing {path}"))?;
                let out = out.unwrap_or_else(|| PathBuf::from("export/fonts"));
                export::font(&f, &out, stem)?;
                println!(
                    "wrote {}/{stem}.png ({} glyphs, {} px high)",
                    out.display(),
                    f.glyphs.len(),
                    f.height
                );
                return Ok(());
            }
            if let Some((stem, ext)) = file.rsplit_once('.')
                && (ext == "raw" || ext == "rat")
            {
                let img = load_screen(&src, &path, stem, ext)?;
                let out = out.unwrap_or_else(|| PathBuf::from("export/screens"));
                let name = format!("{stem}_{ext}");
                export::screen(&img, &out, &name)?;
                println!(
                    "wrote {}/{name}.png ({}x{})",
                    out.display(),
                    img.width,
                    img.height
                );
                return Ok(());
            }
            let s = load_style(&src, &path)?;
            let out = out.unwrap_or_else(|| PathBuf::from("export").join(file.replace('.', "_")));
            export::style(&s, &out)?;
            println!(
                "wrote {}/{{tiles,sprites,remaps,deltas}}.png",
                out.display()
            );
            Ok(())
        }
    }
}

/// Counts each name, most common first.
fn tally<'a>(names: impl Iterator<Item = &'a str>) -> Vec<(&'a str, usize)> {
    let mut t = std::collections::BTreeMap::<&str, usize>::new();
    for n in names {
        *t.entry(n).or_default() += 1;
    }
    let mut v: Vec<_> = t.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    v
}

fn mission(src: &FsSource, path: &str, section: Option<u32>, dump: bool) -> Result<()> {
    use gfa_formats::mission;
    let sections = mission::parse(&src.read(path)?).with_context(|| format!("parsing {path}"))?;
    let Some(id) = section else {
        for s in &sections {
            println!(
                "[{}] {:<26} {:<12} {:4} declarations {:5} commands",
                s.id,
                s.name(),
                s.map().unwrap_or_default(),
                s.decls.len(),
                s.cmds.len()
            );
        }
        return Ok(());
    };
    let s = sections
        .iter()
        .find(|s| s.id == id)
        .with_context(|| format!("no section [{id}] in {path}"))?;
    println!(
        "[{}] {} on {}, settings {:?}",
        s.id,
        s.name(),
        s.map().unwrap_or_default(),
        s.settings
    );
    println!("declarations ({}):", s.decls.len());
    for (k, n) in tally(s.decls.iter().map(|d| d.kind.as_str())) {
        println!("  {n:5} {k}");
    }
    println!("commands ({}):", s.cmds.len());
    for (k, n) in tally(s.cmds.iter().map(|c| c.name.as_str())) {
        println!("  {n:5} {k}");
    }
    if dump {
        for d in &s.decls {
            let flag = d.flag.map_or(String::new(), |f| format!("{f} "));
            println!(
                "{} {flag}({},{},{}) {} {:?}",
                d.id, d.pos[0], d.pos[1], d.pos[2], d.kind, d.nums
            );
        }
        for c in &s.cmds {
            println!("{} {} {:?}", c.id, c.name, c.nums);
        }
    }
    Ok(())
}

fn text(src: &FsSource, path: &str, key: Option<&str>, grep: Option<&str>) -> Result<()> {
    use gfa_formats::fxt::{Fxt, cp1252_to_string};
    let f = Fxt::parse(&src.read(path)?).with_context(|| format!("parsing {path}"))?;
    let grep = grep.map(str::to_lowercase);
    for e in &f.entries {
        let t = cp1252_to_string(&e.text);
        if key.is_some_and(|k| k != e.key)
            || grep.as_ref().is_some_and(|g| !t.to_lowercase().contains(g))
        {
            continue;
        }
        println!("[{}] {t}", e.key);
    }
    if key.is_none() && grep.is_none() {
        println!("{} records, {} keys", f.entries.len(), f.len());
    }
    Ok(())
}

/// A `.RAW` as it is, or a `.RAT` through its palette: the same-named `.ACT`
/// for a briefing still, `F_PAL.RAW` beside it for the front end.
fn load_screen(
    src: &FsSource,
    path: &str,
    stem: &str,
    ext: &str,
) -> Result<gfa_formats::screen::Image<[u8; 3]>> {
    use gfa_formats::screen::{Image, parse_palette, parse_rat, parse_raw};
    let data = src.read(path)?;
    if ext == "raw" {
        return parse_raw(&data, stem).with_context(|| format!("parsing {path}"));
    }
    let img = parse_rat(&data, stem).with_context(|| format!("parsing {path}"))?;
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    let pal_path = if stem.starts_with("cut") {
        format!("{dir}/{stem}.ACT")
    } else {
        format!("{dir}/F_PAL.RAW")
    };
    let pal =
        parse_palette(&src.read(&pal_path)?).with_context(|| format!("parsing {pal_path}"))?;
    Ok(Image {
        width: img.width,
        height: img.height,
        pixels: img.pixels.iter().map(|&i| pal[i as usize]).collect(),
    })
}

fn load_style(src: &FsSource, path: &str) -> Result<gfa_formats::style::Style> {
    gfa_formats::style::Style::parse(&src.read(path)?).with_context(|| format!("parsing {path}"))
}

fn style(src: &FsSource, path: &str, cars: bool) -> Result<()> {
    use gfa_formats::style::{Colour, SpriteCategory};
    let s = load_style(src, path)?;
    println!("{path}: {:?}", s.kind);
    println!(
        "  tiles: {} side, {} lid, {} aux",
        s.side_tiles, s.lid_tiles, s.aux_tiles
    );
    for a in &s.animations {
        println!(
            "  animation: {:?} tile {} every {} frames: aux {:?}",
            a.kind().expect("validated"),
            a.tile,
            a.speed,
            a.frames
        );
    }
    if s.animations_declared as usize != s.animations.len() {
        println!(
            "  (the file declares {} animations but holds {})",
            s.animations_declared,
            s.animations.len()
        );
    }
    match &s.colour {
        Colour::Gry(c) => println!(
            "  colour: one 6-bit palette, {} remap tables, {} lid remap rows",
            c.remaps.len(),
            c.lid_remaps.len()
        ),
        Colour::G24(c) => println!(
            "  colour: {} palettes; CLUTs: {} tile, {} sprite, {} remap, {} font",
            c.cluts.len(),
            c.tile_cluts,
            c.sprite_cluts,
            c.remap_cluts,
            c.font_cluts
        ),
    }
    let deltas: usize = s.sprites.iter().map(|sp| sp.deltas.len()).sum();
    println!(
        "  {} objects, {} vehicles, {} sprites with {deltas} deltas",
        s.objects.len(),
        s.cars.len(),
        s.sprites.len()
    );
    for c in SpriteCategory::ALL {
        let n = s.sprite_count(c);
        if n > 0 {
            println!("    {:<14} {n:4} from {}", c.name(), s.sprite_base(c));
        }
    }
    if cars {
        for (i, c) in s.cars.iter().enumerate() {
            println!(
                "  car {i:2}: model {:3} {:<10} sprite {:4} {}x{}x{} max {:3} mass {:#x} thrust {:#x} doors {}",
                c.model,
                c.class().map_or("?", |k| k.name()),
                s.car_sprite(c).map_or(-1, |i| i as i64),
                c.width,
                c.length,
                c.vert,
                c.max_speed,
                c.mass,
                c.thrust,
                c.doors.len()
            );
        }
    }
    Ok(())
}

fn map(src: &FsSource, path: &str, at: Option<(usize, usize)>) -> Result<()> {
    use gfa_formats::cmp::{Cmp, Face};
    let m = Cmp::parse(&src.read(path)?).with_context(|| format!("parsing {path}"))?;
    if let Some((x, y)) = at {
        println!("column ({x},{y}): height {}", m.column_height(x, y));
        for z in (0..m.column_height(x, y)).rev() {
            let Some(b) = m.block(x, y, z) else { continue };
            println!(
                "  z={z} {:?} slope {:2} lid {:3} rot {} W{:3} E{:3} N{:3} S{:3} {}{}{}{}{}{}",
                b.ground_type(),
                b.slope(),
                b.face(Face::Lid),
                b.lid_rotation(),
                b.face(Face::West),
                b.face(Face::East),
                b.face(Face::North),
                b.face(Face::South),
                if b.up() { "↑" } else { "" },
                if b.down() { "↓" } else { "" },
                if b.left() { "←" } else { "" },
                if b.right() { "→" } else { "" },
                if b.is_flat() { " flat" } else { "" },
                if b.is_railway() { " rail" } else { "" },
            );
        }
        return Ok(());
    }
    println!(
        "{path}: style {}, sample {}",
        m.style_number, m.sample_number
    );
    println!(
        "  {} block records, {} startup objects, {} routes",
        m.blocks.len(),
        m.objects.len(),
        m.routes.len()
    );
    println!(
        "  bases: {} police, {} hospitals, {} fire",
        m.police_stations().count(),
        m.hospitals().count(),
        m.fire_stations().count()
    );
    for (i, d) in m
        .districts
        .iter()
        .enumerate()
        .filter(|(_, d)| !d.is_empty())
    {
        println!(
            "  district {i:2}: {:<20} at ({},{}) {}x{}",
            d.name, d.x, d.y, d.w, d.h
        );
    }
    Ok(())
}

fn info(data: &DataPath, src: &FsSource, files: bool) -> Result<()> {
    println!("data:     {} (from {})", data.root.display(), data.origin);
    let ds = Datasets::detect(src);
    for d in &ds.present {
        let music = if ds.music.contains(d) {
            "music"
        } else {
            "no music"
        };
        println!("game:     {d} ({}, {music})", d.mission_ini());
    }
    if files {
        list(src, "GTADATA", 0)?;
    }
    Ok(())
}

fn list(src: &FsSource, dir: &str, depth: usize) -> Result<()> {
    for e in src.list(dir)? {
        let indent = "  ".repeat(depth + 1);
        if e.is_dir {
            println!("{indent}{}/", e.name);
            list(src, &format!("{dir}/{}", e.name), depth + 1)?;
        } else {
            println!("{indent}{:<20} {:>10}", e.name, e.size);
        }
    }
    Ok(())
}
