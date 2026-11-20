//! Pure parsers for the original game's file formats.
//!
//! Every parser takes `&[u8]` and returns plain structs. There is no file I/O
//! here, so the same code runs natively, in the browser and in tests.
//! Format notes live in `docs/FORMATS.md`.

pub mod cmp;
mod reader;

pub use reader::FormatError;
