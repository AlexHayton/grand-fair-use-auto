//! Resolving the game data folder.
//!
//! The data folder is the one that contains `GTADATA/`. Its location is
//! private to the user's machine, so it never appears in committed files.
//! Sources, highest priority first:
//!
//! 1. an explicit path (the `--data` flag);
//! 2. the `GFA_DATA` environment variable;
//! 3. `data = "..."` in a `gfa.local.toml`, searched for from the current
//!    directory upwards.
//!
//! A path that points at `GTADATA/` itself is accepted and lifted to its
//! parent.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Name of the machine-local config file (gitignored).
pub const LOCAL_CONFIG: &str = "gfa.local.toml";
/// Environment variable naming the data folder.
pub const ENV_VAR: &str = "GFA_DATA";

/// Where a resolved data path came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataPathOrigin {
    Explicit,
    Env,
    LocalConfig(PathBuf),
}

impl fmt::Display for DataPathOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Explicit => write!(f, "--data"),
            Self::Env => write!(f, "${ENV_VAR}"),
            Self::LocalConfig(p) => write!(f, "{}", p.display()),
        }
    }
}

/// A validated data folder: it exists and contains `GTADATA/`.
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

    fn validate(path: &Path, origin: DataPathOrigin) -> Result<Self, DataPathError> {
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
    fn folder_without_gtadata_is_rejected() {
        let top = scratch("reject");
        let err = DataPath::resolve_from(Some(&top), None, &top).unwrap_err();
        assert!(matches!(err, DataPathError::NoGtaData { .. }));
    }
}
