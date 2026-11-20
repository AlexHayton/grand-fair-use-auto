//! `gfa`: Grand Fair Use Auto for desktop.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use gfa_assets::DataPath;

/// Grand Fair Use Auto: the 1997 game, played from your own data.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Folder containing GTADATA/. Overrides GFA_DATA and gfa.local.toml.
    #[arg(long)]
    data: Option<PathBuf>,
    /// Mission script section to start in, e.g. 1, 102, 202.
    #[arg(long)]
    level: Option<u32>,
    /// Ignore HD packs and draw the original art.
    #[arg(long)]
    nohd: bool,
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
    if let Some(level) = args.level {
        log::info!("--level {level}: levels load from milestone M2");
    }

    let config = gfa_app::AppConfig {
        data,
        no_hd: args.nohd,
    };
    match gfa_app::run_native(config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            ExitCode::FAILURE
        }
    }
}
