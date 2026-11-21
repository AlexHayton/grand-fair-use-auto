//! Game data read straight from the archive it came in.
//!
//! People keep the game as they downloaded it: a `.rar` or a `.zip` with
//! the install folder somewhere inside (`GTA1-Directors/C/GTA/GTADATA/…`).
//! Rather than ask them to unpack it, the engine reads it in place:
//!
//! - [`Archive::open`] reads the archive's directory: each member's name,
//!   sizes, where its bytes sit and how they are packed. RAR 5 ([`rar5`])
//!   and ZIP ([`zip`]) are read here from their published layouts: RARLAB's
//!   RAR 5.0 technical note and PKWARE's APPNOTE.
//! - [`GameIndex`] finds the folder holding `GTADATA/` and maps game paths
//!   (`GTADATA/NYC.CMP`) to members, ignoring case as [`crate::source`]
//!   does.
//! - [`Archive::unpack`] reads one member's packed bytes and unpacks them:
//!   ZIP's deflate with `miniz_oxide`, RAR 5's LZ with the decoder of the
//!   `rars` crate (Apache-2.0, THIRD-PARTY.md). Each member is checked
//!   against its stored CRC-32.
//!
//! Reads go through [`ReadAt`]. They are async so that the browser can
//! slice the `File` the user chose and read only the members it needs,
//! never the whole archive into memory. On the desktop they are file reads
//! that finish at once, and [`ArchiveSource`] serves the game from them
//! through the same synchronous [`AssetSource`] as a folder.
//!
//! Every member must unpack on its own, so solid RAR archives (where each
//! file continues the last one's dictionary), sets split into volumes,
//! RAR 4 and passwords are refused, with a message that says so.

pub mod rar5;
pub mod zip;

use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use crate::dataset::Dataset;
use crate::source::{AssetError, AssetSource, Entry, components, list_paths, split_at_gtadata};

/// The largest member unpacked: far above any game file, and below what a
/// browser tab can hold.
const MAX_MEMBER: u64 = 1 << 30;
/// The largest dictionary a RAR member may ask for.
const MAX_DICTIONARY: u64 = 1 << 30;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("reading the archive: {0}")]
    Io(String),
    #[error("not a RAR or ZIP archive")]
    NotAnArchive,
    #[error("{0}")]
    Unsupported(String),
    #[error("the archive is damaged: {0}")]
    Corrupt(String),
    #[error("{0}: the unpacked bytes fail their checksum; the archive is damaged")]
    Checksum(String),
    #[error("there is no GTADATA folder in the archive")]
    NoGame,
}

impl From<std::io::Error> for ArchiveError {
    fn from(e: std::io::Error) -> Self {
        ArchiveError::Io(e.to_string())
    }
}

pub(crate) fn corrupt(what: impl Into<String>) -> ArchiveError {
    ArchiveError::Corrupt(what.into())
}

/// Random access to an archive's bytes.
// The futures are driven on one thread, the page's or the caller's (`now`),
// so they need no `Send` bound.
#[allow(async_fn_in_trait)]
pub trait ReadAt {
    /// The archive's length in bytes.
    fn size(&self) -> u64;
    /// Exactly `len` bytes from `offset`, which the caller keeps inside
    /// [`ReadAt::size`].
    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, ArchiveError>;
}

/// An archive already in memory.
impl ReadAt for Vec<u8> {
    fn size(&self) -> u64 {
        self.len() as u64
    }

    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, ArchiveError> {
        let start = usize::try_from(offset).map_err(|_| corrupt("an offset past the end"))?;
        self.get(start..start.saturating_add(len))
            .map(<[u8]>::to_vec)
            .ok_or_else(|| corrupt("a read past the end"))
    }
}

/// An archive file on disk. Its reads finish at once.
pub struct FileReader {
    file: Mutex<File>,
    size: u64,
}

impl FileReader {
    pub fn open(path: &Path) -> Result<Self, ArchiveError> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        Ok(FileReader {
            file: Mutex::new(file),
            size,
        })
    }
}

impl ReadAt for FileReader {
    fn size(&self) -> u64 {
        self.size
    }

    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, ArchiveError> {
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0; len];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }
}

/// Reads through a window onto the archive, so that walking many small
/// headers costs one read per 64 KiB rather than one per header: in the
/// browser each read is a round trip through a promise.
pub(crate) struct Window<'a, R: ?Sized> {
    reader: &'a R,
    start: u64,
    bytes: Vec<u8>,
}

impl<'a, R: ReadAt + ?Sized> Window<'a, R> {
    const SPAN: usize = 64 * 1024;

    fn new(reader: &'a R) -> Self {
        Window {
            reader,
            start: 0,
            bytes: Vec::new(),
        }
    }

    pub(crate) fn size(&self) -> u64 {
        self.reader.size()
    }

    /// Exactly `len` bytes at `offset`.
    pub(crate) async fn read(&mut self, offset: u64, len: usize) -> Result<Vec<u8>, ArchiveError> {
        let size = self.reader.size();
        let end = offset
            .checked_add(len as u64)
            .filter(|&e| e <= size)
            .ok_or_else(|| corrupt("it ends early"))?;
        let held = self.start + self.bytes.len() as u64;
        if offset < self.start || end > held {
            let span = (len.max(Self::SPAN) as u64).min(size - offset) as usize;
            self.bytes = self.reader.read_at(offset, span).await?;
            self.start = offset;
        }
        let at = (offset - self.start) as usize;
        Ok(self.bytes[at..at + len].to_vec())
    }
}

/// The container a game came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Rar5,
    Zip,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Rar5 => "RAR 5",
            Format::Zip => "ZIP",
        })
    }
}

/// How a member's bytes are packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Packing {
    Stored,
    Deflate,
    /// RAR 5's LZ: the algorithm version (0 for RAR 5, 1 for RAR 7) and the
    /// dictionary it was packed with.
    Rar5 {
        version: u8,
        dictionary: u64,
    },
}

/// One file or folder in the archive.
#[derive(Debug, Clone)]
pub struct Member {
    /// Its path in the archive, `/`-separated, without a trailing `/`.
    pub name: String,
    pub is_dir: bool,
    /// Unpacked size in bytes.
    pub size: u64,
    /// Packed size in bytes.
    pub packed: u64,
    /// RAR: where the packed bytes start. ZIP: where the member's local
    /// header starts; the bytes follow it.
    pub(crate) offset: u64,
    pub(crate) packing: Packing,
    pub(crate) crc32: Option<u32>,
    /// Why it cannot be unpacked on its own, when it cannot.
    pub(crate) refusal: Option<String>,
}

impl Member {
    /// Why this member cannot be unpacked, if it cannot.
    pub fn refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }
}

/// An archive's directory.
#[derive(Debug, Clone)]
pub struct Archive {
    pub format: Format,
    pub members: Vec<Member>,
}

impl Archive {
    /// Read the directory of the archive behind `reader`.
    pub async fn open<R: ReadAt + ?Sized>(reader: &R) -> Result<Self, ArchiveError> {
        let mut w = Window::new(reader);
        let head = w.read(0, w.size().min(8) as usize).await?;
        if head.starts_with(rar5::SIGNATURE) {
            Ok(Archive {
                format: Format::Rar5,
                members: rar5::read(&mut w).await?,
            })
        } else if head.starts_with(rar5::SIGNATURE_RAR4) {
            Err(ArchiveError::Unsupported(
                "this is a RAR 4 archive, and only RAR 5 is read: re-pack it as RAR 5 or ZIP, \
                 or unpack it and choose the folder"
                    .into(),
            ))
        } else if head.starts_with(b"PK") {
            Ok(Archive {
                format: Format::Zip,
                members: zip::read(&mut w).await?,
            })
        } else {
            Err(ArchiveError::NotAnArchive)
        }
    }

    /// Read and unpack one member, checking it against its CRC-32.
    pub async fn unpack<R: ReadAt + ?Sized>(
        &self,
        reader: &R,
        m: &Member,
    ) -> Result<Vec<u8>, ArchiveError> {
        let refuse = |why: &str| ArchiveError::Unsupported(format!("{}: {why}", m.name));
        if let Some(why) = &m.refusal {
            return Err(refuse(why));
        }
        if m.is_dir {
            return Err(refuse("it is a folder"));
        }
        if m.size > MAX_MEMBER {
            return Err(refuse("it is too large"));
        }
        let start = match self.format {
            Format::Rar5 => m.offset,
            Format::Zip => zip::data_start(reader, m.offset).await?,
        };
        let packed_len = usize::try_from(m.packed).map_err(|_| refuse("it is too large"))?;
        if start
            .checked_add(m.packed)
            .is_none_or(|end| end > reader.size())
        {
            return Err(corrupt(format!("{} runs past the end", m.name)));
        }
        let packed = reader.read_at(start, packed_len).await?;
        let size = m.size as usize;
        let bytes = match m.packing {
            Packing::Stored => packed,
            Packing::Deflate => miniz_oxide::inflate::decompress_to_vec_with_limit(&packed, size)
                .map_err(|e| corrupt(format!("{}: {e}", m.name)))?,
            Packing::Rar5 {
                version,
                dictionary,
            } => {
                if dictionary > MAX_DICTIONARY {
                    return Err(refuse("its RAR dictionary is too large"));
                }
                rars::codec::rar50::Unpack50Decoder::new()
                    .decode_member_with_dictionary(
                        &packed,
                        version,
                        size,
                        dictionary as usize,
                        false,
                        rars::codec::rar50::DecodeMode::Lz,
                    )
                    .map_err(|e| corrupt(format!("{}: {e}", m.name)))?
            }
        };
        if bytes.len() != size {
            return Err(corrupt(format!(
                "{} unpacks to {} bytes, not {size}",
                m.name,
                bytes.len()
            )));
        }
        if m.crc32.is_some_and(|crc| crc32fast::hash(&bytes) != crc) {
            return Err(ArchiveError::Checksum(m.name.clone()));
        }
        Ok(bytes)
    }
}

/// The game inside an archive: the folder that holds `GTADATA/`, and the
/// member behind each game path below it.
#[derive(Debug, Clone)]
pub struct GameIndex {
    /// The game's folder in the archive (`""` when `GTADATA/` is at the
    /// top).
    pub root: String,
    /// Lower-case game path -> (game path as stored, member index).
    files: BTreeMap<String, (String, usize)>,
}

impl GameIndex {
    /// Find the game. Where there is more than one `GTADATA/`, the one with
    /// a mission script wins, then the shallowest. macOS's `__MACOSX/`
    /// shadow folders are passed over.
    pub fn new(archive: &Archive) -> Result<Self, ArchiveError> {
        let files = || {
            archive
                .members
                .iter()
                .enumerate()
                .filter(|(_, m)| !m.is_dir && !is_macos_shadow(&m.name))
        };
        let mut roots: Vec<String> = Vec::new();
        for (_, m) in files() {
            if let Some((root, _)) = split_at_gtadata(&m.name)
                && !roots.iter().any(|r| r.eq_ignore_ascii_case(&root))
            {
                roots.push(root);
            }
        }
        let names: std::collections::HashSet<String> = files()
            .map(|(_, m)| clean(&m.name).to_ascii_lowercase())
            .collect();
        let has_script = |root: &str| {
            Dataset::ALL
                .iter()
                .any(|ds| names.contains(&join(root, ds.mission_ini()).to_ascii_lowercase()))
        };
        let root = roots
            .iter()
            .enumerate()
            .min_by_key(|(i, r)| (!has_script(r), components(r).count(), *i))
            .map(|(_, r)| r.clone())
            .ok_or(ArchiveError::NoGame)?;
        let prefix = if root.is_empty() {
            String::new()
        } else {
            format!("{}/", root.to_ascii_lowercase())
        };
        let mut index = BTreeMap::new();
        for (i, m) in files() {
            let name = clean(&m.name);
            if name.to_ascii_lowercase().starts_with(&prefix) {
                let path = name[prefix.len()..].to_string();
                index.entry(path.to_ascii_lowercase()).or_insert((path, i));
            }
        }
        Ok(GameIndex { root, files: index })
    }

    /// The member index behind a game path, ignoring case.
    pub fn get(&self, path: &str) -> Option<usize> {
        let key = components(path).collect::<Vec<_>>().join("/");
        self.files.get(&key.to_ascii_lowercase()).map(|(_, i)| *i)
    }

    /// Every file below the root: game path and member index, in path
    /// order.
    pub fn files(&self) -> impl Iterator<Item = (&str, usize)> {
        self.files.values().map(|(p, i)| (p.as_str(), *i))
    }
}

fn is_macos_shadow(name: &str) -> bool {
    components(name)
        .next()
        .is_some_and(|c| c.eq_ignore_ascii_case("__MACOSX"))
}

fn clean(name: &str) -> String {
    components(name).collect::<Vec<_>>().join("/")
}

fn join(root: &str, path: &str) -> String {
    if root.is_empty() {
        path.to_string()
    } else {
        format!("{root}/{path}")
    }
}

/// Run a future whose reads finish at once (a file, memory) to its end.
fn now<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => {
            panic!("an archive read waited: ArchiveSource needs reads that finish at once")
        }
    }
}

/// A game served from an archive, unpacking each file as it is read.
pub struct ArchiveSource<R> {
    reader: R,
    archive: Archive,
    game: GameIndex,
}

impl ArchiveSource<FileReader> {
    /// Open the archive at `path` and find the game in it.
    pub fn open(path: &Path) -> Result<Self, ArchiveError> {
        Self::new(FileReader::open(path)?)
    }
}

impl<R: ReadAt> ArchiveSource<R> {
    /// Read the directory through `reader`, whose reads must finish at once.
    pub fn new(reader: R) -> Result<Self, ArchiveError> {
        let archive = now(Archive::open(&reader))?;
        let game = GameIndex::new(&archive)?;
        Ok(ArchiveSource {
            reader,
            archive,
            game,
        })
    }

    pub fn archive(&self) -> &Archive {
        &self.archive
    }

    pub fn game(&self) -> &GameIndex {
        &self.game
    }

    /// Unpack member `i` of the archive (any member, not only the game's).
    pub fn unpack(&self, i: usize) -> Result<Vec<u8>, ArchiveError> {
        now(self.archive.unpack(&self.reader, &self.archive.members[i]))
    }
}

impl<R: ReadAt + Send + Sync> AssetSource for ArchiveSource<R> {
    fn read(&self, path: &str) -> Result<Vec<u8>, AssetError> {
        let i = self
            .game
            .get(path)
            .ok_or_else(|| AssetError::NotFound(path.to_string()))?;
        self.unpack(i).map_err(|source| AssetError::Archive {
            path: path.to_string(),
            source,
        })
    }

    fn exists(&self, path: &str) -> bool {
        self.game.get(path).is_some()
    }

    fn list(&self, dir: &str) -> Result<Vec<Entry>, AssetError> {
        list_paths(
            self.game
                .files()
                .map(|(p, i)| (p, self.archive.members[i].size)),
            dir,
        )
    }
}

#[cfg(test)]
mod tests;
