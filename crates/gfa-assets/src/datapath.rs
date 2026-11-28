//! Resolving the game data: a folder, or the archive the game came in.
//!
//! The data is the folder that contains `GTADATA/`, or a `.rar` or `.zip`
//! with that folder somewhere inside ([`crate::archive`]). Its location is
//! private to the user's machine, so it never appears in committed files.
//! [`DataPath::resolve`] reads, highest priority first:
//!
//! 1. an explicit path (the `--data` flag);
//! 2. the `GFA_DATA` environment variable;
//! 3. `data = "..."` in a `gfa.local.toml`, searched for from the current
//!    directory upwards.
//!
//! The desktop program then looks for an archive beside itself
//! ([`DataPath::find_beside`], [`program_dirs`]), and last asks the player
//! to choose one (`gfa-native`).
//!
//! A path that points at `GTADATA/` itself is accepted and lifted to its
//! parent.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::archive::{ArchiveError, ArchiveSource};
use crate::source::{AssetSource, FsSource};

/// Name of the machine-local config file (gitignored).
pub const LOCAL_CONFIG: &str = "gfa.local.toml";
/// Environment variable naming the data folder.
pub const ENV_VAR: &str = "GFA_DATA";
/// Environment variable naming an archive of the same game, which the data
/// tests check against the folder.
pub const ARCHIVE_ENV_VAR: &str = "GFA_ARCHIVE";
/// The archive extensions looked for beside the program.
pub const ARCHIVE_EXTENSIONS: [&str; 2] = ["rar", "zip"];

/// Where a resolved data path came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataPathOrigin {
    Explicit,
    Env,
    LocalConfig(PathBuf),
    /// Found in the program's folder.
    BesideProgram,
    /// Chosen in the file dialog just now.
    Chosen,
    /// Chosen in the file dialog before, and remembered in this file.
    Remembered(PathBuf),
}

impl fmt::Display for DataPathOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Explicit => write!(f, "--data"),
            Self::Env => write!(f, "${ENV_VAR}"),
            Self::LocalConfig(p) => write!(f, "{}", p.display()),
            Self::BesideProgram => write!(f, "beside the program"),
            Self::Chosen => write!(f, "chosen"),
            Self::Remembered(p) => write!(f, "remembered in {}", p.display()),
        }
    }
}

/// Validated game data: a folder that contains `GTADATA/`, or an archive
/// with `GTADATA/` inside.
#[derive(Debug, Clone)]
pub struct DataPath {
    pub root: PathBuf,
    pub origin: DataPathOrigin,
}

#[derive(Debug, thiserror::Error)]
pub enum DataPathError {
    #[error(
        "no game data configured: pass --data <dir>, set {ENV_VAR}, or create {LOCAL_CONFIG} (see gfa.local.toml.example)"
    )]
    NotConfigured,
    #[error("{path} (from {origin}) is not a folder containing GTADATA/")]
    NoGtaData {
        path: PathBuf,
        origin: DataPathOrigin,
    },
    #[error("{path} (from {origin}): {source}")]
    Archive {
        path: PathBuf,
        origin: DataPathOrigin,
        #[source]
        source: ArchiveError,
    },
    #[error("could not read {path}: {source}")]
    Config {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

#[derive(Deserialize)]
struct LocalConfig {
    data: Option<PathBuf>,
    /// For the data tests only: an archive of the same game.
    archive: Option<PathBuf>,
}

/// The folders to look in for an archive beside the program: its own, and
/// on macOS, when it runs inside an app bundle (`X.app/Contents/MacOS/`),
/// the folder that holds the bundle.
pub fn program_dirs(exe: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = exe.parent() {
        dirs.push(dir.to_path_buf());
        let bundle = dir
            .parent()
            .filter(|c| c.file_name().is_some_and(|n| n == "Contents"))
            .and_then(Path::parent)
            .filter(|b| b.extension().is_some_and(|e| e.eq_ignore_ascii_case("app")));
        if let Some(outside) = bundle.and_then(Path::parent) {
            dirs.push(outside.to_path_buf());
        }
    }
    dirs
}

fn is_archive_name(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ARCHIVE_EXTENSIONS.iter().any(|a| e.eq_ignore_ascii_case(a)))
}

impl DataPath {
    /// Resolve from the standard sources, starting the config search at the
    /// current directory.
    pub fn resolve(explicit: Option<&Path>) -> Result<Self, DataPathError> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let env = std::env::var_os(ENV_VAR).map(PathBuf::from);
        Self::resolve_from(explicit, env.as_deref(), &cwd)
    }

    /// Resolution with every input explicit, for tests.
    pub fn resolve_from(
        explicit: Option<&Path>,
        env: Option<&Path>,
        search_from: &Path,
    ) -> Result<Self, DataPathError> {
        if let Some(p) = explicit {
            return Self::validate(p, DataPathOrigin::Explicit);
        }
        if let Some(p) = env.filter(|p| !p.as_os_str().is_empty()) {
            return Self::validate(p, DataPathOrigin::Env);
        }
        if let Some(cfg) = find_upwards(search_from, LOCAL_CONFIG) {
            let text = std::fs::read_to_string(&cfg).map_err(|e| DataPathError::Config {
                path: cfg.clone(),
                source: Box::new(e),
            })?;
            let parsed: LocalConfig = toml::from_str(&text).map_err(|e| DataPathError::Config {
                path: cfg.clone(),
                source: Box::new(e),
            })?;
            if let Some(data) = parsed.data {
                // Relative paths are relative to the config file.
                let data = match cfg.parent() {
                    Some(dir) if data.is_relative() => dir.join(data),
                    _ => data,
                };
                return Self::validate(&data, DataPathOrigin::LocalConfig(cfg));
            }
        }
        Err(DataPathError::NotConfigured)
    }

    /// The game in the first archive in `dir` (by name) that holds one.
    /// The archives passed over come back as errors, to be reported.
    pub fn find_beside(dir: &Path) -> (Option<Self>, Vec<DataPathError>) {
        let mut archives: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_file() && is_archive_name(p))
                    .collect()
            })
            .unwrap_or_default();
        archives.sort();
        let mut passed = Vec::new();
        for a in archives {
            match Self::check(&a, DataPathOrigin::BesideProgram) {
                Ok(d) => return (Some(d), passed),
                Err(e) => passed.push(e),
            }
        }
        (None, passed)
    }

    /// Check that `path` is game data: a folder with `GTADATA/`, or an
    /// archive with the game inside.
    pub fn check(path: &Path, origin: DataPathOrigin) -> Result<Self, DataPathError> {
        Self::validate(path, origin)
    }

    /// Whether the data is an archive rather than a folder.
    pub fn is_archive(&self) -> bool {
        self.root.is_file()
    }

    /// Open the data for reading.
    pub fn open(&self) -> Result<Box<dyn AssetSource>, DataPathError> {
        if self.is_archive() {
            ArchiveSource::open(&self.root)
                .map(|a| Box::new(a) as Box<dyn AssetSource>)
                .map_err(|source| DataPathError::Archive {
                    path: self.root.clone(),
                    origin: self.origin.clone(),
                    source,
                })
        } else {
            Ok(Box::new(FsSource::new(&self.root)))
        }
    }

    /// For the data tests: an archive of the same game, from
    /// `GFA_ARCHIVE` or `archive = "..."` in `gfa.local.toml`.
    pub fn test_archive() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os(ARCHIVE_ENV_VAR).filter(|p| !p.is_empty()) {
            return Some(PathBuf::from(p));
        }
        let cwd = std::env::current_dir().ok()?;
        let cfg = find_upwards(&cwd, LOCAL_CONFIG)?;
        let parsed: LocalConfig = toml::from_str(&std::fs::read_to_string(&cfg).ok()?).ok()?;
        let archive = parsed.archive?;
        Some(match cfg.parent() {
            Some(dir) if archive.is_relative() => dir.join(archive),
            _ => archive,
        })
    }

    fn validate(path: &Path, origin: DataPathOrigin) -> Result<Self, DataPathError> {
        if path.is_file() {
            return match ArchiveSource::open(path) {
                Ok(_) => Ok(Self {
                    root: path.to_path_buf(),
                    origin,
                }),
                Err(source) => Err(DataPathError::Archive {
                    path: path.to_path_buf(),
                    origin,
                    source,
                }),
            };
        }
        let is_gtadata = |p: &Path| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case("GTADATA"))
        };
        let root = if is_gtadata(path) {
            path.parent().unwrap_or(path).to_path_buf()
        } else {
            path.to_path_buf()
        };
        let has_gtadata = std::fs::read_dir(&root)
            .map(|rd| {
                rd.flatten()
                    .any(|e| e.file_type().is_ok_and(|t| t.is_dir()) && is_gtadata(&e.path()))
            })
            .unwrap_or(false);
        if has_gtadata {
            Ok(Self { root, origin })
        } else {
            Err(DataPathError::NoGtaData {
                path: path.to_path_buf(),
                origin,
            })
        }
    }
}

fn find_upwards(start: &Path, name: &str) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gfa-datapath-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn explicit_beats_env_and_config() {
        let a = scratch("explicit-a");
        let b = scratch("explicit-b");
        std::fs::create_dir(a.join("GTADATA")).unwrap();
        std::fs::create_dir(b.join("gtadata")).unwrap();
        let got = DataPath::resolve_from(Some(&a), Some(&b), &a).unwrap();
        assert_eq!(got.root, a);
        assert_eq!(got.origin, DataPathOrigin::Explicit);
        let got = DataPath::resolve_from(None, Some(&b), &a).unwrap();
        assert_eq!(got.root, b);
        assert_eq!(got.origin, DataPathOrigin::Env);
    }

    #[test]
    fn config_is_found_upwards_and_relative_to_itself() {
        let top = scratch("config");
        std::fs::create_dir_all(top.join("game/GTADATA")).unwrap();
        std::fs::create_dir_all(top.join("deep/er")).unwrap();
        std::fs::write(top.join(LOCAL_CONFIG), "data = \"game\"\n").unwrap();
        let got = DataPath::resolve_from(None, None, &top.join("deep/er")).unwrap();
        assert_eq!(got.root, top.join("game"));
        assert!(matches!(got.origin, DataPathOrigin::LocalConfig(_)));
    }

    #[test]
    fn gtadata_itself_is_lifted_to_its_parent() {
        let top = scratch("lift");
        std::fs::create_dir(top.join("GTADATA")).unwrap();
        let got = DataPath::resolve_from(Some(&top.join("GTADATA")), None, &top).unwrap();
        assert_eq!(got.root, top);
    }

    #[test]
    fn an_archive_beside_the_program_is_found_and_others_reported() {
        let top = scratch("beside");
        std::fs::write(top.join("a-broken.zip"), b"PK not really").unwrap();
        std::fs::write(top.join("readme.txt"), b"hello").unwrap();
        let (none, passed) = DataPath::find_beside(&top);
        assert!(none.is_none());
        assert_eq!(passed.len(), 1);
        assert!(matches!(passed[0], DataPathError::Archive { .. }));
        // A game at the top of a stored ZIP.
        std::fs::write(top.join("b-game.ZIP"), one_file_zip("GTADATA/MISSION.INI")).unwrap();
        let (found, _) = DataPath::find_beside(&top);
        let found = found.unwrap();
        assert_eq!(found.root, top.join("b-game.ZIP"));
        assert!(found.is_archive());
        assert!(found.open().unwrap().exists("gtadata/mission.ini"));
    }

    #[test]
    fn program_dirs_reach_out_of_an_app_bundle() {
        let exe = Path::new("/Games/GFA.app/Contents/MacOS/gfa");
        assert_eq!(
            program_dirs(exe),
            [
                PathBuf::from("/Games/GFA.app/Contents/MacOS"),
                PathBuf::from("/Games")
            ]
        );
        assert_eq!(
            program_dirs(Path::new("/Games/gfa")),
            [PathBuf::from("/Games")]
        );
    }

    /// A ZIP holding one empty stored file.
    fn one_file_zip(name: &str) -> Vec<u8> {
        let n = name.len() as u16;
        let crc = 0u32; // CRC-32 of nothing
        let mut z = Vec::new();
        z.extend(0x0403_4b50u32.to_le_bytes());
        z.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend(crc.to_le_bytes());
        z.extend([0; 8]);
        z.extend(n.to_le_bytes());
        z.extend([0, 0]);
        z.extend(name.as_bytes());
        let dir_at = z.len() as u32;
        let mut c = Vec::new();
        c.extend(0x0201_4b50u32.to_le_bytes());
        c.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        c.extend(crc.to_le_bytes());
        c.extend([0; 8]);
        c.extend(n.to_le_bytes());
        c.extend([0; 12]);
        c.extend(0u32.to_le_bytes());
        c.extend(name.as_bytes());
        z.extend(&c);
        z.extend(0x0605_4b50u32.to_le_bytes());
        z.extend([0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend((c.len() as u32).to_le_bytes());
        z.extend(dir_at.to_le_bytes());
        z.extend([0, 0]);
        z
    }

    #[test]
    fn folder_without_gtadata_is_rejected() {
        let top = scratch("reject");
        let err = DataPath::resolve_from(Some(&top), None, &top).unwrap_err();
        assert!(matches!(err, DataPathError::NoGtaData { .. }));
    }
}

/// The settings folder, if the platform has one.
pub fn settings_dir() -> Option<PathBuf> {
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
    Some(base.join("GrandFairUseAuto"))
}
