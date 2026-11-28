//! Finding and reading the user's own game data.
//!
//! - [`datapath`] works out where the data lives: the `--data` flag, then the
//!   `GFA_DATA` environment variable, then a gitignored `gfa.local.toml`.
//! - [`source`] reads files by their original DOS-style names, ignoring case,
//!   so `GTADATA/NYC.CMP` finds `gtadata/nyc.cmp` on any filesystem.
//! - [`dataset`] detects which games are present: GTA, London 1969, London 1961.
//! - [`level`] finds and loads a level's map and style.
//! - [`archive`] reads the game from the `.rar` or `.zip` it came in.

pub mod archive;
pub mod datapath;
pub mod dataset;
pub mod level;
pub mod source;

pub use archive::{Archive, ArchiveError, ArchiveSource, GameIndex, ReadAt};
pub use datapath::{DataPath, DataPathError, DataPathOrigin};
pub use dataset::{Dataset, Datasets};
pub use source::{
    AssetError, AssetSource, FsSource, MemSource, is_play_file, split_at_game_dir, split_at_gtadata,
};
