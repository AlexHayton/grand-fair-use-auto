//! Archives built in memory: small ZIPs (stored and deflated) and RAR 5
//! archives of stored members, written here from the same layouts the
//! readers follow. RAR's LZ is checked against a real archive in
//! `tests/data.rs`.

use super::*;
use crate::source::is_play_file;

/// A ZIP of `(name, bytes, deflate?)`.
fn zip(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, bytes, deflate) in files {
        let packed = if *deflate {
            miniz_oxide::deflate::compress_to_vec(bytes, 6)
        } else {
            bytes.to_vec()
        };
        let crc = crc32fast::hash(bytes);
        let method: u16 = if *deflate { 8 } else { 0 };
        let offset = out.len() as u32;
        // The local header's extra field differs in length from the
        // directory's, as some tools write it.
        let local_extra = [0xca, 0xfe, 0, 0];
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend([20, 0, 0, 0]);
        out.extend(method.to_le_bytes());
        out.extend([0; 4]);
        out.extend(crc.to_le_bytes());
        out.extend((packed.len() as u32).to_le_bytes());
        out.extend((bytes.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend((local_extra.len() as u16).to_le_bytes());
        out.extend(name.as_bytes());
        out.extend(local_extra);
        out.extend(&packed);

        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend([20, 0, 20, 0, 0, 0]);
        central.extend(method.to_le_bytes());
        central.extend([0; 4]);
        central.extend(crc.to_le_bytes());
        central.extend((packed.len() as u32).to_le_bytes());
        central.extend((bytes.len() as u32).to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0; 4]); // extra, comment
        central.extend([0; 8]); // disk, attributes
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let dir_at = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(dir_at.to_le_bytes());
    let comment = b"a comment pushes the end record back";
    out.extend((comment.len() as u16).to_le_bytes());
    out.extend(comment);
    out
}

fn vint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return out;
        }
        out.push(b | 0x80);
    }
}

/// A RAR 5 block: the CRC, the size, then `fields` (type onwards).
fn block(fields: &[u8]) -> Vec<u8> {
    let mut sized = vint(fields.len() as u64);
    sized.extend(fields);
    let mut out = crc32fast::hash(&sized).to_le_bytes().to_vec();
    out.extend(sized);
    out
}

/// One RAR 5 file header and its (stored) data. `info` is the compression
/// information field; `extra` an extra area.
fn rar_file(name: &str, bytes: &[u8], info: u64, extra: &[u8], dir: bool) -> Vec<u8> {
    let mut f = vint(2); // file
    let mut flags = 0x02; // data
    if !extra.is_empty() {
        flags |= 0x01;
    }
    f.extend(vint(flags));
    if !extra.is_empty() {
        f.extend(vint(extra.len() as u64));
    }
    f.extend(vint(bytes.len() as u64));
    f.extend(vint(0x04 | u64::from(dir))); // CRC present
    f.extend(vint(bytes.len() as u64));
    f.extend(vint(0x20)); // attributes
    f.extend(crc32fast::hash(bytes).to_le_bytes());
    f.extend(vint(info));
    f.extend(vint(0)); // host OS
    f.extend(vint(name.len() as u64));
    f.extend(name.as_bytes());
    f.extend(extra);
    let mut out = block(&f);
    out.extend(bytes);
    out
}

fn rar(members: &[Vec<u8>]) -> Vec<u8> {
    let mut out = rar5::SIGNATURE.to_vec();
    out.extend(block(&[1, 0, 0])); // main: no flags
    for m in members {
        out.extend(m);
    }
    out.extend(block(&[5, 0, 0])); // end
    out
}

const SCRIPT: &[u8] = b"[1]\nmission script\n";
const MAP: &[u8] = &[7; 3000];

fn game_zip() -> Vec<u8> {
    zip(&[
        ("Games/GTA/", b"", false),
        ("Games/GTA/GTADATA/MISSION.INI", SCRIPT, false),
        ("Games/GTA/GTADATA/NYC.CMP", MAP, true),
        ("Games/GTA/MUSIC/GTA/Track02.ogg", b"ogg", false),
        ("__MACOSX/GTADATA/._NYC.CMP", b"resource fork", false),
        ("Games/Other/GTADATA/NOTES.TXT", b"no script here", false),
    ])
}

#[test]
fn a_zip_serves_the_game_below_its_gtadata_folder() {
    let src = ArchiveSource::new(game_zip()).unwrap();
    assert_eq!(src.archive().format, Format::Zip);
    assert_eq!(src.game().root, "Games/GTA");
    assert_eq!(src.read("gtadata/mission.ini").unwrap(), SCRIPT);
    assert_eq!(src.read("GTADATA/NYC.CMP").unwrap(), MAP);
    assert!(src.exists("MUSIC/GTA/TRACK02.OGG"));
    assert!(!src.exists("GTADATA/._NYC.CMP"));
    let top: Vec<_> = src.list("").unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(top, ["GTADATA", "MUSIC"]);
    let data = src.list("GTADATA").unwrap();
    assert_eq!(data[1].name, "NYC.CMP");
    assert_eq!(data[1].size, MAP.len() as u64);
    let play: Vec<_> = src
        .game()
        .files()
        .map(|(p, _)| p)
        .filter(|p| is_play_file(p))
        .collect();
    assert_eq!(play, ["GTADATA/MISSION.INI", "GTADATA/NYC.CMP"]);
}

#[test]
fn a_damaged_member_fails_its_checksum() {
    let mut z = game_zip();
    let at = z.windows(SCRIPT.len()).position(|w| w == SCRIPT).unwrap();
    z[at] ^= 1;
    let src = ArchiveSource::new(z).unwrap();
    assert!(matches!(
        src.read("GTADATA/MISSION.INI"),
        Err(AssetError::Archive {
            source: ArchiveError::Checksum(_),
            ..
        })
    ));
    assert_eq!(src.read("GTADATA/NYC.CMP").unwrap(), MAP);
}

#[test]
fn the_game_at_the_top_of_an_archive_has_an_empty_root() {
    let src = ArchiveSource::new(zip(&[("gtadata/mission.ini", SCRIPT, true)])).unwrap();
    assert_eq!(src.game().root, "");
    assert_eq!(src.read("GTADATA/MISSION.INI").unwrap(), SCRIPT);
}

#[test]
fn an_archive_without_the_game_is_refused() {
    let z = zip(&[("readme.txt", b"hello", false)]);
    assert!(matches!(ArchiveSource::new(z), Err(ArchiveError::NoGame)));
    assert!(matches!(
        ArchiveSource::new(b"not an archive at all".to_vec()),
        Err(ArchiveError::NotAnArchive)
    ));
}

#[test]
fn a_rar5_archive_of_stored_members_reads() {
    let r = rar(&[
        rar_file("GTA1/C/GTA", b"", 0, &[], true),
        rar_file("GTA1/C/GTA/GTADATA/MISSION.INI", SCRIPT, 0, &[], false),
        rar_file("GTA1/C/GTA/GTADATA/NYC.CMP", MAP, 0, &[], false),
    ]);
    let src = ArchiveSource::new(r).unwrap();
    assert_eq!(src.archive().format, Format::Rar5);
    assert_eq!(src.archive().members.len(), 3);
    assert!(src.archive().members[0].is_dir);
    assert_eq!(src.game().root, "GTA1/C/GTA");
    assert_eq!(src.read("GTADATA/nyc.cmp").unwrap(), MAP);
    assert_eq!(src.read("GTADATA/MISSION.INI").unwrap(), SCRIPT);
}

#[test]
fn rar5_reads_the_compression_field_and_refuses_what_it_cannot_unpack() {
    // Method 3, 16 MiB dictionary (2^7 x 128 KiB), as WinRAR writes.
    let lz = 3 << 7 | 7 << 10;
    // The same, solid.
    let solid = lz | 0x40;
    // An encryption record in the extra area: size, type 1, fields.
    let locked = [2, 1, 0];
    let r = rar(&[
        rar_file("GTADATA/MISSION.INI", SCRIPT, lz, &[], false),
        rar_file("GTADATA/A.CMP", MAP, solid, &[], false),
        rar_file("GTADATA/B.CMP", MAP, 0, &locked, false),
    ]);
    let a = now(Archive::open(&r)).unwrap();
    assert_eq!(
        a.members[0].packing,
        Packing::Rar5 {
            version: 0,
            dictionary: 16 << 20
        }
    );
    assert!(a.members[0].refusal().is_none());
    assert!(a.members[1].refusal().unwrap().contains("solid"));
    assert!(a.members[2].refusal().unwrap().contains("password"));
    assert!(matches!(
        now(a.unpack(&r, &a.members[2])),
        Err(ArchiveError::Unsupported(_))
    ));
}

#[test]
fn a_rar5_header_with_a_bad_checksum_is_damage() {
    let mut r = rar(&[rar_file("GTADATA/MISSION.INI", SCRIPT, 0, &[], false)]);
    let at = r.windows(11).position(|w| w == b"MISSION.INI").unwrap();
    r[at] = b'm';
    assert!(matches!(
        now(Archive::open(&r)),
        Err(ArchiveError::Corrupt(_))
    ));
}

#[test]
fn rar4_is_refused_by_name() {
    let mut r = rar5::SIGNATURE_RAR4.to_vec();
    r.extend([0; 32]);
    let err = now(Archive::open(&r)).unwrap_err();
    assert!(err.to_string().contains("RAR 4"));
}
