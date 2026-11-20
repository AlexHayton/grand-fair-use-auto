//! A bounds-checked little-endian reader and the shared error type.

use std::fmt;

/// Why a file could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    #[error("{what}: file ends at byte {have}, needed {need}")]
    Truncated {
        what: &'static str,
        need: usize,
        have: usize,
    },
    #[error("{what}: version {found}, expected {expected}")]
    Version {
        what: &'static str,
        found: u32,
        expected: u32,
    },
    #[error("{what}: {detail}")]
    Invalid { what: &'static str, detail: String },
}

impl FormatError {
    pub(crate) fn invalid(what: &'static str, detail: impl fmt::Display) -> Self {
        Self::Invalid {
            what,
            detail: detail.to_string(),
        }
    }
}

pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    what: &'static str,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], what: &'static str) -> Self {
        Self { data, pos: 0, what }
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or(FormatError::Truncated {
                what: self.what,
                need: self.pos.saturating_add(n),
                have: self.data.len(),
            })?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], FormatError> {
        Ok(self.bytes(N)?.try_into().expect("length checked"))
    }

    pub fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.array::<1>()?[0])
    }

    pub fn i8(&mut self) -> Result<i8, FormatError> {
        Ok(self.u8()? as i8)
    }

    pub fn u16(&mut self) -> Result<u16, FormatError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn i16(&mut self) -> Result<i16, FormatError> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32, FormatError> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    /// A section length from a header, as `usize`.
    pub fn len32(&mut self) -> Result<usize, FormatError> {
        Ok(self.u32()? as usize)
    }

    /// A sub-reader over the next `n` bytes, advancing past them.
    pub fn section(&mut self, n: usize, what: &'static str) -> Result<Reader<'a>, FormatError> {
        Ok(Reader {
            data: self.bytes(n)?,
            pos: 0,
            what,
        })
    }
}

impl Reader<'_> {
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Fails unless every byte has been consumed. Walking a variable-length
    /// section to the exact byte is the main check that a record layout is
    /// right.
    pub fn expect_end(&self) -> Result<(), FormatError> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(FormatError::invalid(
                self.what,
                format!("{} bytes left over at byte {}", self.remaining(), self.pos),
            ))
        }
    }
}

/// A NUL-terminated (or full-width) Latin-1 string from a fixed field.
pub(crate) fn latin1_cstr(field: &[u8]) -> String {
    field
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect()
}
