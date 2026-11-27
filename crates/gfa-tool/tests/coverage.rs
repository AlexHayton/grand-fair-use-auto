//! The mission coverage report against the user's mission files: every name
//! GTA's single-player sections use is in GTA8.EXE's tables, every command
//! line carries the five numbers the original reads, and the tables in
//! `docs/MISSION_OPCODES.md` are current. Run with `cargo data-test`.

use gfa_assets::{AssetSource, DataPath, FsSource};
use gfa_tool::coverage::{Report, normalize_markdown};

const BEGIN: &str = "<!-- BEGIN gfa-tool mission coverage --markdown -->";
const END: &str = "<!-- END gfa-tool mission coverage --markdown -->";

fn data() -> FsSource {
    match DataPath::resolve(None) {
        Ok(p) => FsSource::new(p.root),
        Err(e) => panic!("this test needs the game data: {e}"),
    }
}

fn report(src: &FsSource, path: &str) -> Report {
    let sections = gfa_formats::mission::parse(&src.read(path).unwrap()).unwrap();
    Report::new(path, &sections, false)
}

#[test]
#[ignore = "needs game data (cargo data-test)"]
fn mission_coverage_doc_is_current() {
    let r = report(&data(), "GTADATA/MISSION.INI");
    assert_eq!(r.sections, vec![1, 2, 102, 103, 202, 203]);
    assert!(
        r.unknown().is_empty(),
        "names GTA8.EXE lacks: {:?}",
        r.unknown()
    );
    assert!(
        r.odd_lines.is_empty(),
        "lines without five numbers: {:?}",
        r.odd_lines
    );
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/MISSION_OPCODES.md"
    ))
    .unwrap();
    let start = doc.find(BEGIN).expect("the doc's BEGIN marker") + BEGIN.len();
    let end = doc.find(END).expect("the doc's END marker");
    assert_eq!(
        normalize_markdown(&doc[start..end]),
        normalize_markdown(&r.markdown()),
        "docs/MISSION_OPCODES.md is out of date: paste in the output of \
         `cargo run -p gfa-tool -- mission coverage --markdown`"
    );
}

/// London's executables carry commands GTA8.EXE does not, and London 1961
/// has one command line with a sixth number (M11: docs/MISSION_OPCODES.md).
#[test]
#[ignore = "needs game data (cargo data-test)"]
fn london_uses_commands_gta8_lacks() {
    let src = data();
    let names = |r: &Report| {
        r.unknown()
            .into_iter()
            .map(|(what, name, _, _)| format!("{what} {name}"))
            .collect::<Vec<_>>()
    };
    let r69 = report(&src, "GTADATA/UK/MISSUK.INI");
    assert_eq!(names(&r69), ["command RED_ARROWCAR"]);
    assert!(r69.odd_lines.is_empty(), "{:?}", r69.odd_lines);
    let r61 = report(&src, "GTADATA/UK/MISSUKE.INI");
    assert_eq!(
        names(&r61),
        ["command PLAYER_DRIVE_BY_OFF", "command PLAYER_DRIVE_BY"]
    );
    assert_eq!(r61.odd_lines, [(1, 2604, 6)], "a MOBILE_BRIEF");
}
