//! `gfa`: Grand Fair Use Auto for desktop.

use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use clap::Parser;
use gfa_assets::{DataPath, Dataset};

/// Grand Fair Use Auto: the 1997 game, played from your own data.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Folder containing GTADATA/. Overrides GFA_DATA and gfa.local.toml.
    #[arg(long)]
    data: Option<PathBuf>,
    /// Which game: gta, london69 or london61.
    #[arg(long, default_value = "gta", value_parser = Dataset::from_str)]
    game: Dataset,
    /// Mission script section to start in, e.g. 1, 102, 202 (London: 2, 3, 4).
    #[arg(long, default_value_t = 1)]
    level: u32,
    /// Ignore HD packs and draw the original art.
    #[arg(long)]
    nohd: bool,
    /// Save the first frame as a PNG and quit (a smoke test).
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Window size in logical pixels, e.g. 1280x800.
    #[arg(long, default_value = "1280x800", value_parser = parse_size)]
    size: (u32, u32),
}

/// `WIDTHxHEIGHT`.
fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once('x').ok_or("expected WIDTHxHEIGHT")?;
    let n = |v: &str| v.trim().parse::<u32>().ok().filter(|&n| n > 0);
    n(w).zip(n(h)).ok_or_else(|| format!("bad size {s}"))
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();
    let args = Args::parse();

    let data = match DataPath::resolve(args.data.as_deref()) {
        Ok(d) => Some(d),
        // An explicit --data that is wrong is a hard error; a missing
        // config just means an empty screen for now.
        Err(e) if args.data.is_some() => {
            log::error!("{e}");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            log::warn!("{e}");
            None
        }
    };
    let config = gfa_app::AppConfig {
        data,
        game: args.game,
        level: args.level,
        no_hd: args.nohd,
        screenshot: args.screenshot,
        size: args.size,
    };
    match gfa_app::run_native(config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            ExitCode::FAILURE
        }
    }
}
