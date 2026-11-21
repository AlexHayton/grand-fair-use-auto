//! RAR 5 archives: the directory.
//!
//! Read from the block layout in RARLAB's RAR 5.0 archive format note
//! (rarlab.com/technote.htm). An archive is a signature, then blocks, each
//!
//! ```text
//! CRC-32 of the rest of the header   u32
//! header size (from the type on)     vint
//! type, flags                        vint, vint
//! extra area size, data size         vint each, when the flags say so
//! the type's own fields, extra area
//! data area                          (data size bytes)
//! ```
//!
//! where a vint is a little-endian base-128 number. File blocks carry a
//! member's name and sizes, and its packed bytes as the data area. The
//! packed bytes are unpacked in [`super::Archive::unpack`].

use super::{ArchiveError, Member, Packing, ReadAt, Window, corrupt};

pub const SIGNATURE: &[u8] = b"Rar!\x1a\x07\x01\x00";
/// RAR 1.5 to 4's signature, refused with a message.
pub const SIGNATURE_RAR4: &[u8] = b"Rar!\x1a\x07\x00";

const MAIN: u64 = 1;
const FILE: u64 = 2;
const ENCRYPTION: u64 = 4;
const END: u64 = 5;

/// Header flags.
const HAS_EXTRA: u64 = 0x01;
const HAS_DATA: u64 = 0x02;
const FROM_LAST_VOLUME: u64 = 0x08;
const TO_NEXT_VOLUME: u64 = 0x10;

/// File flags.
const DIRECTORY: u64 = 0x01;
const HAS_TIME: u64 = 0x02;
const HAS_CRC: u64 = 0x04;
const SIZE_UNKNOWN: u64 = 0x08;

/// The extra-area record that marks a member encrypted.
const RECORD_ENCRYPTION: u64 = 0x01;

/// The note caps headers at 2 MiB.
const MAX_HEADER: u64 = 2 << 20;

/// A little-endian base-128 number at `b[*at..]`.
pub(crate) fn vint(b: &[u8], at: &mut usize) -> Result<u64, ArchiveError> {
    let mut v = 0u64;
    for i in 0..10 {
        let byte = *b.get(*at).ok_or_else(|| corrupt("a header ends early"))?;
        *at += 1;
        v |= u64::from(byte & 0x7f) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err(corrupt("a header number runs on"))
}

fn u32_at(b: &[u8], at: &mut usize) -> Result<u32, ArchiveError> {
    let v = b
        .get(*at..*at + 4)
        .ok_or_else(|| corrupt("a header ends early"))?;
    *at += 4;
    Ok(u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
}

/// Every member, in archive order.
pub(crate) async fn read<R: ReadAt + ?Sized>(
    w: &mut Window<'_, R>,
) -> Result<Vec<Member>, ArchiveError> {
    let size = w.size();
    let mut pos = SIGNATURE.len() as u64;
    let mut members = Vec::new();
    while pos < size {
        // The CRC, then the header size: three bytes at most by the note.
        let head = w.read(pos, (size - pos).min(4 + 10) as usize).await?;
        let mut at = 0;
        let crc = u32_at(&head, &mut at)?;
        let header_size = vint(&head, &mut at)?;
        if header_size == 0 || header_size > MAX_HEADER {
            return Err(corrupt("a header has an impossible size"));
        }
        let body = w.read(pos + at as u64, header_size as usize).await?;
        let mut sum = crc32fast::Hasher::new();
        sum.update(&head[4..at]);
        sum.update(&body);
        if sum.finalize() != crc {
            return Err(corrupt("a header fails its checksum"));
        }
        let mut c = 0;
        let kind = vint(&body, &mut c)?;
        let flags = vint(&body, &mut c)?;
        let extra = if flags & HAS_EXTRA != 0 {
            vint(&body, &mut c)?
        } else {
            0
        };
        let data = if flags & HAS_DATA != 0 {
            vint(&body, &mut c)?
        } else {
            0
        };
        let data_at = pos + at as u64 + header_size;
        match kind {
            MAIN => {}
            FILE => members.push(file(&body, c, extra, flags, data_at, data)?),
            ENCRYPTION => {
                return Err(ArchiveError::Unsupported(
                    "the archive is password-protected".into(),
                ));
            }
            END => break,
            // Service blocks (comments, quick-open copies, recovery) and
            // types newer than the note.
            _ => {}
        }
        pos = data_at
            .checked_add(data)
            .ok_or_else(|| corrupt("a block has an impossible size"))?;
    }
    Ok(members)
}

/// A file block's fields, from `c` (after the common fields) on.
fn file(
    body: &[u8],
    mut c: usize,
    extra_len: u64,
    flags: u64,
    data_at: u64,
    packed: u64,
) -> Result<Member, ArchiveError> {
    let file_flags = vint(body, &mut c)?;
    let size = vint(body, &mut c)?;
    let _attributes = vint(body, &mut c)?;
    if file_flags & HAS_TIME != 0 {
        u32_at(body, &mut c)?;
    }
    let crc32 = if file_flags & HAS_CRC != 0 {
        Some(u32_at(body, &mut c)?)
    } else {
        None
    };
    let info = vint(body, &mut c)?;
    let _host_os = vint(body, &mut c)?;
    let name_len = vint(body, &mut c)? as usize;
    let name = body
        .get(c..c.saturating_add(name_len))
        .ok_or_else(|| corrupt("a file name runs past its header"))?;
    let name = String::from_utf8_lossy(name)
        .trim_end_matches('/')
        .to_string();
    let extra = usize::try_from(extra_len)
        .ok()
        .and_then(|n| body.len().checked_sub(n))
        .map(|from| &body[from..])
        .ok_or_else(|| corrupt("an extra area runs past its header"))?;

    // Compression information: bits 0-5 the algorithm version, 6 solid,
    // 7-9 the method (0 is stored), 10-14 the dictionary's power of two
    // over 128 KiB, and for version 1 (RAR 7) bits 15-19 a fraction of it
    // in 32nds.
    let version = (info & 0x3f) as u8;
    let solid = info & 0x40 != 0;
    let method = (info >> 7) & 0x07;
    let power = (info >> 10) & 0x1f;
    let fraction = (info >> 15) & 0x1f;
    let dictionary = if version == 1 {
        (fraction + 32) << (power + 12)
    } else {
        (128 << 10) << power
    };

    let refusal = if has_record(extra, RECORD_ENCRYPTION)? {
        Some("it is password-protected".to_string())
    } else if flags & (FROM_LAST_VOLUME | TO_NEXT_VOLUME) != 0 {
        Some("it is split across volumes; join them into one archive".into())
    } else if file_flags & SIZE_UNKNOWN != 0 {
        Some("its size is not recorded".into())
    } else if method != 0 && solid {
        Some(
            "the archive is solid (each file continues from the one before); \
             re-pack it without the solid option, or unpack it and choose the folder"
                .into(),
        )
    } else if method != 0 && version > 1 {
        Some(format!(
            "it uses RAR compression version {version}, newer than this reads"
        ))
    } else {
        None
    };
    Ok(Member {
        name,
        is_dir: file_flags & DIRECTORY != 0,
        size,
        packed,
        offset: data_at,
        packing: if method == 0 {
            Packing::Stored
        } else {
            Packing::Rar5 {
                version,
                dictionary,
            }
        },
        crc32,
        refusal,
    })
}

/// Whether the extra area holds a record of type `kind`. Each record is
/// its size (from the type on), its type, then its fields.
fn has_record(extra: &[u8], kind: u64) -> Result<bool, ArchiveError> {
    let mut at = 0;
    while at < extra.len() {
        let len = vint(extra, &mut at)? as usize;
        let start = at;
        if vint(extra, &mut at)? == kind {
            return Ok(true);
        }
        at = start
            .checked_add(len)
            .filter(|&e| e <= extra.len())
            .ok_or_else(|| corrupt("an extra record runs past its header"))?;
    }
    Ok(false)
}
