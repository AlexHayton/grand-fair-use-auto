//! ZIP archives: the directory.
//!
//! Read from the layout in PKWARE's APPNOTE. The central directory at the
//! end lists every member; its end record, found by searching back from
//! the end of the file past any comment, says where the directory starts.
//! Archives over 4 GiB or 65,535 members keep the real numbers in ZIP64
//! records instead. Each member's bytes follow its own local header, whose
//! name and extra fields may differ in length from the directory's, so the
//! local header is read when the member is unpacked ([`data_start`]).
//! Stored and deflated members are read; others are refused.

use super::{ArchiveError, Member, Packing, ReadAt, Window, corrupt};

const END: u32 = 0x0605_4b50;
const END64_LOCATOR: u32 = 0x0706_4b50;
const END64: u32 = 0x0606_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

const END_LEN: usize = 22;
const CENTRAL_LEN: usize = 46;
const LOCAL_LEN: usize = 30;
/// The most a comment can push the end record back.
const MAX_COMMENT: usize = 0xffff;
/// A sanity limit on the directory's size.
const MAX_DIRECTORY: u64 = 256 << 20;

/// General-purpose flags.
const ENCRYPTED: u16 = 0x0001;
/// The extra-field block holding ZIP64 sizes.
const EXTRA_ZIP64: u16 = 0x0001;

fn u16_at(b: &[u8], at: usize) -> Result<u16, ArchiveError> {
    b.get(at..at + 2)
        .map(|v| u16::from_le_bytes([v[0], v[1]]))
        .ok_or_else(|| corrupt("the directory ends early"))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, ArchiveError> {
    b.get(at..at + 4)
        .map(|v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
        .ok_or_else(|| corrupt("the directory ends early"))
}

fn u64_at(b: &[u8], at: usize) -> Result<u64, ArchiveError> {
    b.get(at..at + 8)
        .map(|v| u64::from_le_bytes([v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]]))
        .ok_or_else(|| corrupt("the directory ends early"))
}

/// Every member, in directory order.
pub(crate) async fn read<R: ReadAt + ?Sized>(
    w: &mut Window<'_, R>,
) -> Result<Vec<Member>, ArchiveError> {
    let size = w.size();
    if size < END_LEN as u64 {
        return Err(ArchiveError::NotAnArchive);
    }
    let tail_len = size.min((END_LEN + MAX_COMMENT) as u64);
    let tail_at = size - tail_len;
    let tail = w.read(tail_at, tail_len as usize).await?;
    let e = (0..=tail.len() - END_LEN)
        .rev()
        .find(|&i| u32_at(&tail, i).is_ok_and(|s| s == END))
        .ok_or_else(|| corrupt("it has no end-of-directory record"))?;
    let mut count = u64::from(u16_at(&tail, e + 10)?);
    let mut dir_len = u64::from(u32_at(&tail, e + 12)?);
    let mut dir_at = u64::from(u32_at(&tail, e + 16)?);
    if count == 0xffff || dir_len == 0xffff_ffff || dir_at == 0xffff_ffff {
        // ZIP64: a locator just before the end record points at the
        // 64-bit end record.
        let end_at = tail_at + e as u64;
        let locator = w
            .read(
                end_at
                    .checked_sub(20)
                    .ok_or_else(|| corrupt("its ZIP64 locator is missing"))?,
                20,
            )
            .await?;
        if u32_at(&locator, 0)? != END64_LOCATOR {
            return Err(corrupt("its ZIP64 locator is missing"));
        }
        let record = w.read(u64_at(&locator, 8)?, 56).await?;
        if u32_at(&record, 0)? != END64 {
            return Err(corrupt("its ZIP64 end record is missing"));
        }
        count = u64_at(&record, 32)?;
        dir_len = u64_at(&record, 40)?;
        dir_at = u64_at(&record, 48)?;
    }
    if dir_len > MAX_DIRECTORY {
        return Err(ArchiveError::Unsupported(
            "its directory is too large".into(),
        ));
    }
    let dir = w.read(dir_at, dir_len as usize).await?;
    let mut members = Vec::with_capacity(count.min(1 << 16) as usize);
    let mut p = 0usize;
    for _ in 0..count {
        if u32_at(&dir, p)? != CENTRAL {
            return Err(corrupt("a directory entry is missing"));
        }
        let flags = u16_at(&dir, p + 8)?;
        let method = u16_at(&dir, p + 10)?;
        let crc32 = u32_at(&dir, p + 16)?;
        let mut packed = u64::from(u32_at(&dir, p + 20)?);
        let mut size = u64::from(u32_at(&dir, p + 24)?);
        let name_len = usize::from(u16_at(&dir, p + 28)?);
        let extra_len = usize::from(u16_at(&dir, p + 30)?);
        let comment_len = usize::from(u16_at(&dir, p + 32)?);
        let mut offset = u64::from(u32_at(&dir, p + 42)?);
        let name_at = p + CENTRAL_LEN;
        let name = dir
            .get(name_at..name_at + name_len)
            .ok_or_else(|| corrupt("a name runs past the directory"))?;
        let extra = dir
            .get(name_at + name_len..name_at + name_len + extra_len)
            .ok_or_else(|| corrupt("an extra field runs past the directory"))?;
        // ZIP64 keeps each field that overflowed, in this order.
        if let Some(z) = extra_block(extra, EXTRA_ZIP64) {
            let mut q = 0;
            for field in [&mut size, &mut packed, &mut offset] {
                if *field == 0xffff_ffff {
                    *field = u64_at(z, q)?;
                    q += 8;
                }
            }
        }
        // Names are UTF-8 when flag 0x0800 says so, else code page 437; the
        // game's names are ASCII either way.
        let name = String::from_utf8_lossy(name).replace('\\', "/");
        let is_dir = name.ends_with('/');
        let (packing, refusal) = match method {
            0 => (Packing::Stored, None),
            8 => (Packing::Deflate, None),
            m => (
                Packing::Stored,
                Some(format!(
                    "it is packed with method {m}; only stored and deflated members are read"
                )),
            ),
        };
        let refusal = if flags & ENCRYPTED != 0 {
            Some("it is password-protected".to_string())
        } else {
            refusal
        };
        members.push(Member {
            name: name.trim_end_matches('/').to_string(),
            is_dir,
            size,
            packed,
            offset,
            packing,
            crc32: Some(crc32),
            refusal,
        });
        p = name_at + name_len + extra_len + comment_len;
    }
    Ok(members)
}

/// The data of the extra-field block with this id: blocks are an id, a
/// length and that many bytes.
fn extra_block(extra: &[u8], id: u16) -> Option<&[u8]> {
    let mut at = 0;
    while at + 4 <= extra.len() {
        let this = u16::from_le_bytes([extra[at], extra[at + 1]]);
        let len = usize::from(u16::from_le_bytes([extra[at + 2], extra[at + 3]]));
        let data = extra.get(at + 4..at + 4 + len)?;
        if this == id {
            return Some(data);
        }
        at += 4 + len;
    }
    None
}

/// Where a member's packed bytes begin: after its local header.
pub(crate) async fn data_start<R: ReadAt + ?Sized>(
    reader: &R,
    header_at: u64,
) -> Result<u64, ArchiveError> {
    if header_at
        .checked_add(LOCAL_LEN as u64)
        .is_none_or(|end| end > reader.size())
    {
        return Err(corrupt("a member's header is past the end"));
    }
    let h = reader.read_at(header_at, LOCAL_LEN).await?;
    if u32_at(&h, 0)? != LOCAL {
        return Err(corrupt("a member's header is missing"));
    }
    Ok(header_at + LOCAL_LEN as u64 + u64::from(u16_at(&h, 26)?) + u64::from(u16_at(&h, 28)?))
}
