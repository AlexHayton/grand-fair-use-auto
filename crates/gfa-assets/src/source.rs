//! Reading game files by their original names.
//!
//! The game refers to files with DOS names in any mix of case
//! (`STYLE001.G24`, `Style001.g24`, `missuke.ini`). An [`AssetSource`] takes a
//! `/`-separated path relative to the data root and finds the file whatever
//! its case on disk.
//!
//! The web build reads the user's chosen folder into a [`MemSource`] up
//! front, so the rest of the engine sees the same synchronous interface on
//! both targets.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("{0}: not found in the game data")]
    NotFound(String),
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// One directory entry, with the name as it appears on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Read-only access to the game data, case-insensitively.
pub trait AssetSource: Send + Sync {
    /// Read a whole file, e.g. `"GTADATA/NYC.CMP"`.
    fn read(&self, path: &str) -> Result<Vec<u8>, AssetError>;
    /// Whether a file exists.
    fn exists(&self, path: &str) -> bool;
    /// List a directory (`""` is the data root), sorted by lower-case name.
    fn list(&self, dir: &str) -> Result<Vec<Entry>, AssetError>;
}

fn components(path: &str) -> impl Iterator<Item = &str> {
    path.split(['/', '\\'])
        .filter(|c| !c.is_empty() && *c != ".")
}

/// Files on a local filesystem, under a root folder.
pub struct FsSource {
    root: PathBuf,
    /// Directory listings already read: lower-case relative dir -> entries.
    listings: Mutex<HashMap<String, Vec<Entry>>>,
}

impl FsSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            listings: Mutex::new(HashMap::new()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn listing(&self, rel_dir: &str, abs_dir: &Path) -> Result<Vec<Entry>, AssetError> {
        let key = rel_dir.to_ascii_lowercase();
        if let Some(hit) = self.listings.lock().unwrap().get(&key) {
            return Ok(hit.clone());
        }
        let io = |source| AssetError::Io {
            path: rel_dir.to_string(),
            source,
        };
        let mut entries = Vec::new();
        for e in std::fs::read_dir(abs_dir).map_err(io)? {
            let e = e.map_err(io)?;
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            let meta = e.metadata().map_err(io)?;
            entries.push(Entry {
                name,
                is_dir: meta.is_dir(),
                size: meta.len(),
            });
        }
        entries.sort_by_key(|e| e.name.to_ascii_lowercase());
        self.listings.lock().unwrap().insert(key, entries.clone());
        Ok(entries)
    }

    /// The on-disk path for a game path, matching each component
    /// case-insensitively.
    pub fn resolve(&self, path: &str) -> Option<PathBuf> {
        let mut abs = self.root.clone();
        let mut rel = String::new();
        for comp in components(path) {
            let exact = abs.join(comp);
            let found = if exact.exists() {
                // Exact match is the common case on macOS; still record the
                // on-disk spelling for case-sensitive filesystems.
                comp.to_string()
            } else {
                self.listing(&rel, &abs)
                    .ok()?
                    .into_iter()
                    .find(|e| e.name.eq_ignore_ascii_case(comp))?
                    .name
            };
            abs.push(&found);
            if !rel.is_empty() {
                rel.push('/');
            }
            rel.push_str(&found);
        }
        Some(abs)
    }
}

impl AssetSource for FsSource {
    fn read(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        let abs = self
            .resolve(path)
            .ok_or_else(|| AssetError::NotFound(path.to_string()))?;
        std::fs::read(&abs).map_err(|source| AssetError::Io {
            path: path.to_string(),
            source,
        })
    }

    fn exists(&self, path: &str) -> bool {
        self.resolve(path).is_some_and(|p| p.is_file())
    }

    fn list(&self, dir: &str) -> Result<Vec<Entry>, AssetError> {
        let abs = self
            .resolve(dir)
            .ok_or_else(|| AssetError::NotFound(dir.to_string()))?;
        self.listing(dir, &abs)
    }
}

/// Files held in memory, keyed by path. Used by the web build and tests.
#[derive(Default)]
pub struct MemSource {
    /// lower-case path -> (on-disk path, bytes)
    files: HashMap<String, (String, Vec<u8>)>,
}

impl MemSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: &str, bytes: Vec<u8>) {
        let clean: Vec<&str> = components(path).collect();
        let clean = clean.join("/");
        self.files
            .insert(clean.to_ascii_lowercase(), (clean, bytes));
    }
}

impl AssetSource for MemSource {
    fn read(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        let key: Vec<&str> = components(path).collect();
        self.files
            .get(&key.join("/").to_ascii_lowercase())
            .map(|(_, b)| b.clone())
            .ok_or_else(|| AssetError::NotFound(path.to_string()))
    }

    fn exists(&self, path: &str) -> bool {
        let key: Vec<&str> = components(path).collect();
        self.files.contains_key(&key.join("/").to_ascii_lowercase())
    }

    fn list(&self, dir: &str) -> Result<Vec<Entry>, AssetError> {
        let dir: Vec<&str> = components(dir).collect();
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{}/", dir.join("/"))
        };
        let prefix_lc = prefix.to_ascii_lowercase();
        let mut out: HashMap<String, Entry> = HashMap::new();
        for (key, (orig, bytes)) in &self.files {
            let Some(rest) = key.strip_prefix(&prefix_lc) else {
                continue;
            };
            let orig_rest = &orig[prefix.len()..];
            match rest.find('/') {
                Some(i) => {
                    let name = orig_rest[..i].to_string();
                    out.entry(rest[..i].to_string()).or_insert(Entry {
                        name,
                        is_dir: true,
                        size: 0,
                    });
                }
                None => {
                    out.insert(
                        rest.to_string(),
                        Entry {
                            name: orig_rest.to_string(),
                            is_dir: false,
                            size: bytes.len() as u64,
                        },
                    );
                }
            }
        }
        if out.is_empty() && !dir.is_empty() {
            return Err(AssetError::NotFound(dir.join("/")));
        }
        let mut v: Vec<Entry> = out.into_values().collect();
        v.sort_by_key(|e| e.name.to_ascii_lowercase());
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_source_ignores_case() {
        let dir = std::env::temp_dir().join(format!("gfa-fs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("GtaData/UK")).unwrap();
        std::fs::write(dir.join("GtaData/UK/missuke.ini"), b"[1]").unwrap();
        let src = FsSource::new(&dir);
        assert_eq!(src.read("GTADATA/uk/MISSUKE.INI").unwrap(), b"[1]");
        assert!(src.exists("gtadata/UK/MissUKE.ini"));
        assert!(!src.exists("GTADATA/UK/MISSUK.INI"));
        let names: Vec<_> = src
            .list("gtadata")
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["UK"]);
    }

    #[test]
    fn mem_source_lists_and_reads() {
        let mut m = MemSource::new();
        m.insert("GTADATA/NYC.CMP", vec![1, 2, 3]);
        m.insert("GTADATA/UK/UK.CMP", vec![4]);
        assert_eq!(m.read("gtadata/nyc.cmp").unwrap(), [1, 2, 3]);
        let l = m.list("GTADATA").unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(
            l[0],
            Entry {
                name: "NYC.CMP".into(),
                is_dir: false,
                size: 3
            }
        );
        assert!(l[1].is_dir && l[1].name == "UK");
        assert!(m.list("NOPE").is_err());
    }
}
