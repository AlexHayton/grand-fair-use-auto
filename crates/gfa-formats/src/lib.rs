//! Pure parsers for the original game's file formats.
//!
//! Every parser takes `&[u8]` and returns plain structs. There is no file I/O
//! here, so the same code runs natively, in the browser and in tests.
//! Format notes live in `docs/FORMATS.md`.

pub mod career;
pub mod cmp;
pub mod cue;
pub mod fon;
pub mod fxt;
pub mod mission;
mod reader;
pub mod screen;
pub mod sdt;
pub mod style;

pub use reader::FormatError;
