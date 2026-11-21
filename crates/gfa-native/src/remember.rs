//! The game archive the player chose, remembered between runs.
//!
//! One small file in the program's settings folder, on the player's machine
//! and never in the repo:
//!
//! - macOS: `~/Library/Application Support/GrandFairUseAuto/game.toml`
//! - Windows: `%APPDATA%\GrandFairUseAuto\game.toml`
//! - elsewhere: `$XDG_CONFIG_HOME/GrandFairUseAuto/game.toml`, or under
//!   `~/.config`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const APP: &str = "GrandFairUseAuto";
const FILE: &str = "game.toml";

#[derive(Serialize, Deserialize)]
struct Remembered {
    archive: PathBuf,
}

/// The settings folder, if the platform has one.
fn settings_dir() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(target_os = "macos") {
        env("HOME")?.join("Library/Application Support")
    } else if cfg!(windows) {
        env("APPDATA")?
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))?
    };
    Some(base.join(APP))
}

/// The remembered archive, and the file it was remembered in.
pub fn load() -> Option<(PathBuf, PathBuf)> {
    let file = settings_dir()?.join(FILE);
    let text = std::fs::read_to_string(&file).ok()?;
    let r: Remembered = toml::from_str(&text).ok()?;
    Some((r.archive, file))
}

/// Remember `archive` for next time.
pub fn save(archive: &Path) -> Result<PathBuf, String> {
    let dir = settings_dir().ok_or("no settings folder on this platform")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let file = dir.join(FILE);
    let text = toml::to_string(&Remembered {
        archive: archive.to_path_buf(),
    })
    .map_err(|e| e.to_string())?;
    std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}
