//! `gfa`: Grand Fair Use Auto for desktop.
//!
//! It finds the player's game in the first place that has it:
//!
//! 1. `--data`, `GFA_DATA` or `gfa.local.toml`: a folder holding `GTADATA/`
//!    or the game's archive (`gfa_assets::datapath`);
//! 2. a `.rar` or `.zip` of the game in the program's folder (or, for an
//!    app bundle, the folder holding it);
//! 3. the archive chosen last time ([`remember`]);
//! 4. a file dialog asking for the archive, whose answer is remembered.
//!
//! An archive is read in place, never unpacked to disk
//! (`gfa_assets::archive`).

mod remember;

use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use clap::Parser;
use gfa_assets::datapath::program_dirs;
use gfa_assets::{DataPath, DataPathError, DataPathOrigin, Dataset};

/// Grand Fair Use Auto: the 1997 game, played from your own data.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// The game: a folder containing GTADATA/, or the .rar or .zip it came
    /// in. Overrides GFA_DATA and gfa.local.toml.
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
    /// Save the first frame as a PNG and quit (a smoke test). Never asks
    /// for the game.
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

/// An archive of the game in the program's folder.
fn beside_program() -> Option<DataPath> {
    let exe = std::env::current_exe().ok()?;
    for dir in program_dirs(&exe) {
        let (found, passed) = DataPath::find_beside(&dir);
        for e in passed {
            log::warn!("passed over: {e}");
        }
        if found.is_some() {
            return found;
        }
    }
    None
}

/// The archive chosen last time, if it is still there and still good.
fn remembered() -> Option<DataPath> {
    let (archive, file) = remember::load()?;
    DataPath::check(&archive, DataPathOrigin::Remembered(file))
        .inspect_err(|e| log::warn!("the game chosen last time cannot be used: {e}"))
        .ok()
}

/// Ask for the archive until the player gives a good one or gives up.
fn choose() -> Option<DataPath> {
    loop {
        let picked = rfd::FileDialog::new()
            .set_title("Select your GTA game archive (.rar or .zip)")
            .add_filter("Game archive", &["rar", "zip", "RAR", "ZIP"])
            .pick_file()?;
        match DataPath::check(&picked, DataPathOrigin::Chosen) {
            Ok(d) => {
                match remember::save(&d.root) {
                    Ok(file) => log::info!("remembered in {}", file.display()),
                    Err(e) => log::warn!("could not remember the game: {e}"),
                }
                return Some(d);
            }
            Err(e) => {
                log::warn!("{e}");
                let again = rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Warning)
                    .set_title("That archive cannot be used")
                    .set_description(format!("{e}\n\nChoose another?"))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show();
                if again != rfd::MessageDialogResult::Yes {
                    return None;
                }
            }
        }
    }
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"),
    )
    .init();
    let args = Args::parse();

    // Before any dialog: on macOS the event loop must own the application.
    let event_loop = match gfa_app::NativeLoop::new() {
        Ok(l) => l,
        Err(e) => {
            log::error!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let configured = match DataPath::resolve(args.data.as_deref()) {
        Ok(d) => Some(d),
        // An explicit --data that is wrong is a hard error.
        Err(e) if args.data.is_some() => {
            log::error!("{e}");
            return ExitCode::FAILURE;
        }
        Err(DataPathError::NotConfigured) => None,
        Err(e) => {
            log::warn!("{e}");
            None
        }
    };
    let unattended = args.screenshot.is_some();
    let data = configured
        .or_else(beside_program)
        .or_else(remembered)
        .or_else(|| if unattended { None } else { choose() });
    if data.is_none() {
        log::error!(
            "no game chosen: select the game's .rar or .zip, put it beside the program, \
             or pass --data"
        );
        return ExitCode::FAILURE;
    }

    let config = gfa_app::AppConfig {
        data,
        game: args.game,
        level: args.level,
        no_hd: args.nohd,
        screenshot: args.screenshot,
        size: args.size,
    };
    match event_loop.run(config) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("{e}");
            ExitCode::FAILURE
        }
    }
}
