//! Tests against the user's own game data. Run with `cargo data-test`.
//!
//! Every test here is `#[ignore]`d so `cargo test` passes without the data.
//! The data folder comes from `GFA_DATA` or `gfa.local.toml`.

use gfa_assets::{AssetSource, DataPath, Dataset, Datasets, FsSource};

fn data() -> FsSource {
    match DataPath::resolve(None) {
        Ok(p) => FsSource::new(p.root),
        Err(e) => panic!("data tests need the game data: {e}"),
    }
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn finds_the_base_game() {
    let src = data();
    let d = Datasets::detect(&src);
    println!("datasets: {:?}, music: {:?}", d.present, d.music);
    assert!(d.has(Dataset::Gta), "GTADATA/MISSION.INI not found");
    for f in ["GTADATA/NYC.CMP", "GTADATA/SANB.CMP", "GTADATA/MIAMI.CMP"] {
        assert!(src.exists(f), "{f} missing");
    }
}

const MAPS: [&str; 6] = [
    "GTADATA/NYC.CMP",
    "GTADATA/SANB.CMP",
    "GTADATA/MIAMI.CMP",
    "GTADATA/UK/UK.CMP",
    "GTADATA/UK/MC.CMP",
    "GTADATA/UK/UKE.CMP",
];

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_city_map_parses() {
    use gfa_formats::cmp::{Cmp, MAP_DIM, MAP_LAYERS};
    let src = data();
    for path in MAPS {
        let bytes = src.read(path).unwrap_or_else(|e| panic!("{e}"));
        let m = Cmp::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        let mut solid = 0usize;
        for y in 0..MAP_DIM {
            for x in 0..MAP_DIM {
                let h = m.column_height(x, y);
                assert!(h <= MAP_LAYERS);
                solid += (0..h).filter(|&z| m.block(x, y, z).is_some()).count();
            }
        }
        let police_routes = m.routes.iter().filter(|r| r.kind == 0xff).count();
        let named: Vec<&str> = m
            .districts
            .iter()
            .filter(|d| !d.is_empty())
            .map(|d| d.name.as_str())
            .collect();
        println!(
            "{path}: style {} sample {}, {} blocks, {solid} placed, {} objects, {} routes ({police_routes} police), \
             {}/{}/{} police/hospital/fire, {} districts: {}",
            m.style_number,
            m.sample_number,
            m.blocks.len(),
            m.objects.len(),
            m.routes.len(),
            m.police_stations().count(),
            m.hospitals().count(),
            m.fire_stations().count(),
            m.districts.len(),
            named.join(", ")
        );
        assert!(solid > 10_000, "{path}: suspiciously empty");
    }
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn probe_london_map_files() {
    // UK.MAP and MC.MAP have CMP-like headers; find out what they are.
    let src = data();
    for path in ["GTADATA/UK/UK.MAP", "GTADATA/UK/MC.MAP"] {
        let Ok(bytes) = src.read(path) else { continue };
        let words: Vec<u32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .take(7)
            .map(|c| u32::from_le_bytes(*c))
            .collect();
        let parsed = gfa_formats::cmp::Cmp::parse(&bytes).map(|m| m.blocks.len());
        println!(
            "{path}: {} bytes, header words {words:?}, as CMP: {parsed:?}",
            bytes.len()
        );
    }
}

/// Each style, paired with the map that uses it.
const STYLES: [(&str, &str); 9] = [
    ("GTADATA/STYLE001.GRY", "GTADATA/NYC.CMP"),
    ("GTADATA/STYLE001.G24", "GTADATA/NYC.CMP"),
    ("GTADATA/STYLE002.GRY", "GTADATA/SANB.CMP"),
    ("GTADATA/STYLE002.G24", "GTADATA/SANB.CMP"),
    ("GTADATA/STYLE003.GRY", "GTADATA/MIAMI.CMP"),
    ("GTADATA/STYLE003.G24", "GTADATA/MIAMI.CMP"),
    ("GTADATA/UK/STYLE001.GRY", "GTADATA/UK/UK.CMP"),
    ("GTADATA/UK/STYLE001.G24", "GTADATA/UK/UK.CMP"),
    ("GTADATA/UK/STY001.G24", "GTADATA/UK/UKE.CMP"),
];

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_style_parses() {
    use gfa_formats::cmp::{Cmp, Face};
    use gfa_formats::style::{CAR_REMAPS, Colour, PED_REMAPS, SpriteCategory, Style};
    let src = data();
    let mut parsed: Vec<(&str, Style)> = Vec::new();
    for (path, map) in STYLES {
        let bytes = src.read(path).unwrap_or_else(|e| panic!("{e}"));
        let s = Style::parse(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
        let deltas: usize = s.sprites.iter().map(|sp| sp.deltas.len()).sum();
        let colour = match &s.colour {
            Colour::Gry(c) => format!(
                "GRY palette, {} remaps, {} lid remaps",
                c.remaps.len(),
                c.lid_remaps.len()
            ),
            Colour::G24(c) => format!(
                "{} palettes, CLUTs tile {} sprite {} remap {} font {}",
                c.cluts.len(),
                c.tile_cluts,
                c.sprite_cluts,
                c.remap_cluts,
                c.font_cluts
            ),
        };
        println!(
            "{path}: tiles {}/{}/{} side/lid/aux, {} animations, {colour}, {} objects, {} cars, \
             {} sprites with {deltas} deltas",
            s.side_tiles,
            s.lid_tiles,
            s.aux_tiles,
            s.animations.len(),
            s.objects.len(),
            s.cars.len(),
            s.sprites.len(),
        );
        let cats: Vec<String> = SpriteCategory::ALL
            .iter()
            .map(|&c| format!("{} {}", c.name(), s.sprite_count(c)))
            .collect();
        println!("  sprites: {}", cats.join(", "));

        for (i, car) in s.cars.iter().enumerate() {
            let class = car
                .class()
                .unwrap_or_else(|| panic!("{path}: car {i} has vtype {}", car.vtype));
            let sprite = s.car_sprite(car).unwrap_or_else(|| {
                panic!(
                    "{path}: car {i} ({}) sprite {} unresolved",
                    class.name(),
                    car.sprite
                )
            });
            let n = s.sprites[sprite].deltas.len();
            for d in &car.doors {
                assert!(
                    (d.delta as usize) < n.max(1) || n == 0,
                    "{path}: car {i} door delta {} of {n}",
                    d.delta
                );
            }
        }
        for (i, o) in s.objects.iter().enumerate() {
            assert!(
                s.object_sprite(o).is_some(),
                "{path}: object {i} sprite {} unresolved",
                o.sprite
            );
        }
        if let Colour::G24(c) = &s.colour {
            assert_eq!(
                c.tile_cluts,
                s.tile_total() * 4,
                "{path}: four CLUTs per tile"
            );
            assert_eq!(
                c.remap_cluts,
                s.cars.len() * CAR_REMAPS + PED_REMAPS,
                "{path}: remap CLUTs"
            );
        }

        // The map that uses this style only names tiles that exist.
        let m = Cmp::parse(&src.read(map).unwrap()).unwrap();
        let max_of = |faces: &[Face]| {
            m.blocks
                .iter()
                .flat_map(|b| faces.iter().map(|&f| b.face(f)))
                .max()
                .unwrap_or(0) as usize
        };
        let max_lid = max_of(&[Face::Lid]);
        let max_side = max_of(&[Face::West, Face::East, Face::North, Face::South]);
        println!("  {map} uses lid tiles up to {max_lid}, side tiles up to {max_side}");
        assert!(max_lid < s.lid_tiles, "{path}: {map} lid {max_lid}");
        assert!(max_side < s.side_tiles, "{path}: {map} side {max_side}");
        parsed.push((path, s));
    }

    // Each GRY and its G24 describe the same city. The G24s add 25 arrow
    // sprites and more aux tiles (longer animations), so those differ.
    for pair in parsed.chunks(2).take(4) {
        let [(gp, g), (tp, t)] = pair else {
            unreachable!()
        };
        assert_eq!(
            (g.side_tiles, g.lid_tiles),
            (t.side_tiles, t.lid_tiles),
            "{gp} vs {tp} tiles"
        );
        assert_eq!(g.objects, t.objects, "{gp} vs {tp} objects");
        let mut diffs = Vec::new();
        for c in SpriteCategory::ALL {
            let (a, b) = (g.sprite_count(c), t.sprite_count(c));
            if a != b {
                diffs.push(format!("{} count {a}/{b}", c.name()));
                continue;
            }
            let sp = |s: &Style, i: usize| s.sprites[s.sprite_base(c) + i].clone();
            let size = (0..a)
                .filter(|&i| (sp(g, i).width, sp(g, i).height) != (sp(t, i).width, sp(t, i).height))
                .count();
            let deltas = (0..a)
                .filter(|&i| sp(g, i).deltas.len() != sp(t, i).deltas.len())
                .count();
            if size + deltas > 0 {
                diffs.push(format!(
                    "{} {size} sizes/{deltas} delta counts of {a}",
                    c.name()
                ));
            }
        }
        println!(
            "{gp} vs {tp}: {}",
            if diffs.is_empty() {
                "sprites agree".into()
            } else {
                diffs.join("; ")
            }
        );
    }
    // The base game's car tables agree across formats. London's GRY does
    // not: its car table is a byte-for-byte copy of the base STYLE001.GRY,
    // so London's vehicles come from the G24 only.
    for pair in parsed.chunks(2).take(3) {
        let [(gp, g), (tp, t)] = pair else {
            unreachable!()
        };
        assert!(g.cars == t.cars, "{gp} vs {tp} cars");
    }
    let style = |p: &str| &parsed.iter().find(|(q, _)| *q == p).unwrap().1;
    assert!(style("GTADATA/UK/STYLE001.GRY").cars == style("GTADATA/STYLE001.GRY").cars);
    assert!(style("GTADATA/UK/STYLE001.GRY").cars != style("GTADATA/UK/STYLE001.G24").cars);
    assert!(style("GTADATA/UK/STYLE001.G24").cars != style("GTADATA/UK/STY001.G24").cars);
}

/// Every file under `dirs` whose name ends in `ext` (any case), as game paths.
fn files_with(src: &FsSource, dirs: &[&str], ext: &str) -> Vec<String> {
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(list) = src.list(dir) else { continue };
        for e in list {
            if !e.is_dir && e.name.to_ascii_lowercase().ends_with(ext) {
                out.push(format!("{dir}/{}", e.name));
            }
        }
    }
    out.sort();
    out
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_text_table_parses() {
    use gfa_formats::fxt::Fxt;
    use std::collections::HashMap;
    let src = data();
    let paths = files_with(&src, &["GTADATA", "GTADATA/UK", "GTADOS/DATA"], ".fxt");
    assert!(paths.len() >= 18, "found only {paths:?}");
    for path in &paths {
        let f = Fxt::parse(&src.read(path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
        let mut seen: HashMap<&str, &[u8]> = HashMap::new();
        let (mut dups, mut differ) = (0, 0);
        for e in &f.entries {
            if let Some(prev) = seen.insert(&e.key, &e.text) {
                dups += 1;
                differ += usize::from(prev != e.text.as_slice());
            }
        }
        println!(
            "{path}: {} keys, {} duplicates ({differ} with different text), {} bytes after the end",
            f.len(),
            dups,
            f.trailing
        );
    }
    let en = Fxt::parse(&src.read("GTADATA/ENGLISH.FXT").unwrap()).unwrap();
    assert_eq!(en.string("2500").as_deref(), Some("MISSION COMPLETE!"));
    // The one key whose repeats differ: "Criminal Record", later "Crimes".
    let crimes: Vec<String> = en
        .entries
        .iter()
        .filter(|e| e.key == "crimes")
        .map(|e| gfa_formats::fxt::cp1252_to_string(&e.text))
        .collect();
    assert_eq!(crimes, ["Criminal Record", "Crimes"]);
    println!("4003 {:?}, 4004 {:?}", en.string("4003"), en.string("4004"));
    let fr = Fxt::parse(&src.read("GTADATA/FRENCH.FXT").unwrap()).unwrap();
    assert!(
        fr.entries.iter().any(|e| e.text.contains(&0xE9)),
        "French has é"
    );
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_font_parses() {
    use gfa_formats::fon::Font;
    let src = data();
    let paths = files_with(&src, &["GTADATA", "GTADATA/UK", "GTADOS/DATA"], ".fon");
    assert!(paths.len() >= 100, "found only {}", paths.len());
    let mut shapes = std::collections::BTreeMap::<(usize, u8), Vec<String>>::new();
    for path in &paths {
        let f = Font::parse(&src.read(path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
        let name = path.rsplit('/').next().unwrap().to_string();
        shapes
            .entry((f.glyphs.len(), f.height))
            .or_default()
            .push(name);
    }
    for ((n, h), names) in &shapes {
        println!("{n:3} glyphs x {h:3} px: {}", names.join(" "));
    }
    let m = Font::parse(&src.read("GTADATA/F_MTEXT.FON").unwrap()).unwrap();
    assert_eq!(
        m.glyphs.len(),
        140,
        "a full font: ASCII, then 45 accented letters"
    );
    assert!(m.glyph(0xE9).is_some() && m.glyph(b'A').is_some());
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_sound_bank_parses() {
    use gfa_formats::sdt::SoundBank;
    let src = data();
    let paths = files_with(&src, &["GTADATA/AUDIO", "GTADATA/UK/AUDIO"], ".sdt");
    assert_eq!(paths.len(), 12, "{paths:?}");
    for path in &paths {
        let raw_path = format!("{}.RAW", &path[..path.len() - 4]);
        let raw = src.read(&raw_path).unwrap_or_else(|e| panic!("{e}"));
        let b = SoundBank::parse(&src.read(path).unwrap(), raw.len())
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(
            b.is_contiguous(raw.len()),
            "{path}: samples do not tile the RAW"
        );
        let secs: u64 = b
            .entries
            .iter()
            .map(|e| e.length as u64 * 1000 / e.rate.max(1) as u64)
            .sum();
        println!(
            "{path}: {} samples, {} bytes, about {} s at one byte per sample",
            b.entries.len(),
            raw.len(),
            secs / 1000
        );
    }
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_screen_image_parses() {
    use gfa_formats::screen::{parse_palette, parse_rat, parse_raw};
    let src = data();
    let mut n = 0;
    for dir in ["GTADATA", "GTADATA/UK"] {
        for e in src.list(dir).unwrap() {
            let upper = e.name.to_ascii_uppercase();
            let Some((stem, ext)) = upper.rsplit_once('.') else {
                continue;
            };
            if !(stem.starts_with("F_") || stem.starts_with("CUT")) {
                continue;
            }
            let path = format!("{dir}/{}", e.name);
            let bytes = src.read(&path).unwrap();
            match ext {
                "RAW" if stem == "F_PAL" => drop(parse_palette(&bytes).unwrap()),
                "RAW" => drop(parse_raw(&bytes, stem).unwrap_or_else(|e| panic!("{path}: {e}"))),
                "RAT" => drop(parse_rat(&bytes, stem).unwrap_or_else(|e| panic!("{path}: {e}"))),
                "ACT" => drop(parse_palette(&bytes).unwrap()),
                _ => continue,
            }
            n += 1;
        }
    }
    println!("{n} front-end and briefing files parse");
    assert_eq!(n, 140, "63 in GTADATA, 77 in GTADATA/UK");
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn saves_and_cue_sheets_parse() {
    use gfa_formats::{career::Career, cue};
    let src = data();
    let c = Career::parse(&src.read("GTADATA/PLAYER_A.DAT").unwrap()).unwrap();
    let slot = c.current_slot();
    println!(
        "PLAYER_A.DAT: slot {slot} {:?}, best {:?}, sound {} music {} text {}",
        c.name(slot),
        (0..6).map(|l| c.best(slot, l)).collect::<Vec<_>>(),
        c.sound_volume(),
        c.music_volume(),
        c.text_speed()
    );
    for level in 0..6 {
        let rows: Vec<_> = (0..3).map(|r| c.high_score(level, r)).collect();
        println!("  level {level} table {rows:?}");
    }
    for (dir, n) in [("MUSIC/GTA", 11), ("MUSIC/LONDON", 16)] {
        let tracks = cue::parse(&src.read(&format!("{dir}/CDAUDIO.DAT")).unwrap()).unwrap();
        assert_eq!(tracks.len(), n, "{dir}");
        for t in &tracks {
            assert!(
                src.exists(&format!("{dir}/{}", t.file)),
                "{dir}: {} missing",
                t.file
            );
        }
        println!(
            "{dir}: {} tracks, track 1 -> {}",
            tracks.len(),
            tracks[0].file
        );
    }
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn probe_movies_and_replays() {
    // Header probes only; the decoders are M9 (Smacker) and M8 (replays).
    let src = data();
    for path in ["GTADATA/MOVIE.SMK", "GTADATA/UK/MOVIE.SMK"] {
        let b = src.read(path).unwrap();
        let u = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
        let rate = u(16) as i32;
        let fps = match rate {
            r if r > 0 => 1000.0 / r as f64,
            r if r < 0 => 100_000.0 / -r as f64,
            _ => 10.0,
        };
        let audio: Vec<String> = (0..7)
            .map(|i| u(0x48 + 4 * i))
            .filter(|&a| a != 0)
            .map(|a| format!("{} Hz, flags {:#x}", a & 0x00ff_ffff, a >> 24))
            .collect();
        println!(
            "{path}: {:?} {}x{}, {} frames at {fps:.2} fps, flags {:#x}, audio {audio:?}",
            String::from_utf8_lossy(&b[0..4]),
            u(4),
            u(8),
            u(12),
            u(20)
        );
        assert_eq!(&b[0..4], b"SMK2");
    }
    for path in [
        "GTADATA/1.REP",
        "GTADATA/102.REP",
        "GTADATA/202.REP",
        "GTADATA/UK/4.REP",
        "GTADATA/REPLAY.REP",
    ] {
        let b = src.read(path).unwrap();
        let head: Vec<String> = b.iter().take(24).map(|x| format!("{x:02x}")).collect();
        println!("{path}: {} bytes, starts {}", b.len(), head.join(" "));
    }
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn every_mission_file_parses() {
    use gfa_formats::mission;
    use std::collections::BTreeMap;
    let src = data();
    for path in [
        "GTADATA/MISSION.INI",
        "GTADATA/UK/MISSUK.INI",
        "GTADATA/UK/MISSUKE.INI",
        "GTADATA/UK/MISSE8.INI",
    ] {
        let sections =
            mission::parse(&src.read(path).unwrap()).unwrap_or_else(|e| panic!("{path}: {e}"));
        println!("{path}: {} sections", sections.len());
        let mut names: BTreeMap<&str, usize> = BTreeMap::new();
        let mut five = BTreeMap::<&str, usize>::new();
        let mut six = BTreeMap::<&str, usize>::new();
        for s in &sections {
            let ids: std::collections::HashSet<i32> = s.cmds.iter().map(|c| c.id).collect();
            let dup_cmd_ids = s.cmds.len() - ids.len();
            println!(
                "  [{}] {:?} on {:?}: {} declarations, {} commands{}",
                s.id,
                s.name(),
                s.map().unwrap_or_default(),
                s.decls.len(),
                s.cmds.len(),
                if dup_cmd_ids > 0 {
                    format!(", {dup_cmd_ids} repeated command ids")
                } else {
                    String::new()
                }
            );
            for c in &s.cmds {
                *names.entry(&c.name).or_default() += 1;
                if c.nums.len() > 5 {
                    *six.entry(&c.name).or_default() += 1;
                }
            }
            for d in s.decls.iter().filter(|d| d.nums.len() >= 5) {
                *five.entry(&d.kind).or_default() += 1;
            }
        }
        println!(
            "  {} distinct commands; declarations with 5+ numbers: {five:?}; commands with 6+: {six:?}",
            names.len()
        );
        if path == "GTADATA/MISSION.INI" {
            let ids: Vec<u32> = sections.iter().map(|s| s.id).collect();
            assert!(ids.contains(&1003), "the [1003] header glued to a -1 line");
            assert!(
                six.is_empty(),
                "the base game's commands have at most five numbers"
            );
            let one = sections.iter().find(|s| s.id == 1).unwrap();
            // AmiGTA1's recorded counts for Liberty City's first section.
            assert_eq!((one.decls.len(), one.cmds.len()), (575, 2116));
            let doors: Vec<_> = sections
                .iter()
                .flat_map(|s| &s.decls)
                .filter(|d| d.kind == "SPECIFIC_DOOR")
                .collect();
            assert!(
                !doors.is_empty() && doors.iter().all(|d| d.nums.len() == 5),
                "every SPECIFIC_DOOR keeps its fifth number (the remap)"
            );
        }
    }
}
