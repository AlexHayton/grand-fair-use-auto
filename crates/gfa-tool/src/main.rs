//! `gfa-tool`: developer CLI for inspecting the user's game data.
//!
//! Milestone M0 has `info`. M1 adds format dumps and art export; later
//! milestones add headless rendering, script runs and mission coverage.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use gfa_assets::{AssetSource, DataPath, Datasets, FsSource};

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
}

fn parse_xy(s: &str) -> Result<(usize, usize), String> {
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
    }
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
