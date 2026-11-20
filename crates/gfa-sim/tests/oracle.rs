//! The simulation against the AmiGTA1 oracle, tick by tick.
//!
//! `tools/oracle/build.sh` compiles AmiGTA1's own walking, driving and
//! train code from a sibling `../AmiGTA1` checkout (or `$AMIGTA1`) into
//! small harnesses; each test runs the same script through a harness and
//! through gfa-sim and compares every tick's state. Scripts in
//! `tools/scripts/` take part when their header says `# oracle: walk` or
//! `# oracle: drive` (they may use only the orders the harness knows). Needs the game data,
//! the AmiGTA1 checkout and a C compiler, so these are ignored by plain
//! `cargo test`: run them with `cargo oracle-test`.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use gfa_fixed::rng::NrLcg;
use gfa_sim::replay::{self, RunOptions, trace_line};
use gfa_sim::script::Script;

/// The harnesses, built once per test run.
fn harnesses() -> &'static Path {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT.get_or_init(|| {
        let script = common::root().join("tools/oracle/build.sh");
        let out = Command::new("sh")
            .arg(&script)
            .output()
            .expect("running tools/oracle/build.sh");
        assert!(
            out.status.success(),
            "building the AmiGTA1 harnesses failed (they need ../AmiGTA1 or $AMIGTA1, and clang):\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        common::root().join("tools/oracle/build")
    })
}

/// A game file's real path in the data folder.
fn file(path: &str) -> PathBuf {
    common::data()
        .resolve(path)
        .unwrap_or_else(|| panic!("{path} is not in the data folder"))
}

/// Run a harness and keep the lines starting with `prefix`.
fn oracle_lines(harness: &str, args: &[&Path], extra: &[&str], prefix: &str) -> Vec<String> {
    let out = Command::new(harnesses().join(harness))
        .args(args)
        .args(extra)
        .output()
        .unwrap_or_else(|e| panic!("running {harness}: {e}"));
    assert!(
        out.status.success(),
        "{harness} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.starts_with(prefix))
        .map(str::to_string)
        .collect()
}

/// Run a script in gfa-sim as the oracle runs it and keep the trace lines
/// starting with `prefix`.
fn our_lines(script: &Script, prefix: &str) -> Vec<String> {
    let lv = common::level(&common::data(), script.section.unwrap_or(1));
    let mut w = common::world(&lv, script, true);
    let mut lines = vec![trace_line(&w)];
    replay::run(&mut w, script, RunOptions::default(), |w, _| {
        lines.push(trace_line(w))
    });
    lines
        .into_iter()
        .filter(|l| l.starts_with(prefix))
        .collect()
}

/// Panic at the first tick that differs, showing both.
fn same(name: &str, ours: &[String], theirs: &[String]) {
    assert!(!theirs.is_empty(), "{name}: the oracle printed nothing");
    if let Some(i) = (0..ours.len().max(theirs.len())).find(|&i| ours.get(i) != theirs.get(i)) {
        panic!(
            "{name}: differs at line {i} of {}\n  AmiGTA1: {}\n  gfa-sim: {}",
            theirs.len(),
            theirs.get(i).map_or("(nothing)", String::as_str),
            ours.get(i).map_or("(nothing)", String::as_str),
        );
    }
    println!("{name}: {} ticks match", theirs.len());
}

/// The map a section plays on, as a real path.
fn map_of(section: u32) -> PathBuf {
    let lv = common::level(&common::data(), section);
    file(&lv.files.map)
}

/// The GRY style beside a section's map (AmiGTA1 reads GRY; gfa-sim's G24
/// has the same car table, checked by `g24_and_gry_cars_agree`).
fn gry_of(section: u32) -> PathBuf {
    let lv = common::level(&common::data(), section);
    let gry = lv
        .style_path
        .rsplit_once('.')
        .map_or(lv.style_path.clone(), |(stem, _)| format!("{stem}.GRY"));
    file(&gry)
}

/// Write a script where the harness can read it.
fn script_file(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("gfa-oracle");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, text).unwrap();
    p
}

fn check_walk(name: &str, text: &str) {
    let script = Script::parse(text).unwrap();
    let path = script_file(name, text);
    let map = map_of(script.section.unwrap_or(1));
    let theirs = oracle_lines("walktrace", &[&map, &path], &[], "t ");
    same(name, &our_lines(&script, "t "), &theirs);
}

fn check_drive(name: &str, text: &str) {
    let script = Script::parse(text).unwrap();
    let section = script.section.unwrap_or(1);
    let path = script_file(name, text);
    let theirs = oracle_lines(
        "drivetrace",
        &[&map_of(section), &gry_of(section), &path],
        &[],
        "c ",
    );
    same(name, &our_lines(&script, "c "), &theirs);
}

const CITIES: [u32; 3] = [1, 102, 202];

/// One of `options`, from the generator.
fn pick<T: Copy>(rng: &mut NrLcg, options: &[T]) -> T {
    options[rng.next15() as usize % options.len()]
}

#[test]
#[ignore = "needs game data and ../AmiGTA1 (cargo oracle-test)"]
fn oracle_walk_scripts() {
    for (name, text) in common::scripts() {
        if text.lines().any(|l| l.trim() == "# oracle: walk") {
            check_walk(&name, &text);
        }
    }
}

#[test]
#[ignore = "needs game data and ../AmiGTA1 (cargo oracle-test)"]
fn oracle_drive_scripts() {
    for (name, text) in common::scripts() {
        if text.lines().any(|l| l.trim() == "# oracle: drive") {
            check_drive(&name, &text);
        }
    }
}

#[test]
#[ignore = "needs game data and ../AmiGTA1 (cargo oracle-test)"]
fn oracle_random_walks() {
    let mut rng = NrLcg::new(1);
    for section in CITIES {
        for i in 0..8 {
            let mut text = format!(
                "# section: {section}\nstart {} {}\n",
                rng.next15() % 256,
                rng.next15() % 256
            );
            for _ in 0..30 {
                text += &format!(
                    "{} {} {} {}\n",
                    pick(&mut rng, &[-1, 0, 0, 1]),
                    pick(&mut rng, &[-1, 0, 1, 1, 1]),
                    pick(&mut rng, &[0, 0, 0, 1]),
                    1 + rng.next15() % 119
                );
            }
            check_walk(&format!("walk_{section}_{i}.txt"), &text);
        }
    }
}

#[test]
#[ignore = "needs game data and ../AmiGTA1 (cargo oracle-test)"]
fn oracle_random_drives() {
    let mut rng = NrLcg::new(2);
    for section in CITIES {
        for i in 0..8 {
            // Records 0..20 exist in every GTA style: cars, bikes, buses.
            let mut text = format!(
                "# section: {section}\ndrive {} {} {} {}\n",
                rng.next15() % 21,
                rng.next15() % 256,
                rng.next15() % 256,
                rng.next15() % 256
            );
            for _ in 0..30 {
                let up = pick(&mut rng, &[0, 1, 1, 1]);
                let down = if up == 1 { 0 } else { pick(&mut rng, &[0, 1]) };
                text += &format!(
                    "run {} {up} {down} {} {}\n",
                    1 + rng.next15() % 89,
                    pick(&mut rng, &[-1, 0, 0, 1]),
                    pick(&mut rng, &[0, 0, 0, 1])
                );
            }
            check_drive(&format!("drive_{section}_{i}.txt"), &text);
        }
    }
}

#[test]
#[ignore = "needs game data and ../AmiGTA1 (cargo oracle-test)"]
fn oracle_trains() {
    const TICKS: u32 = 12_000;
    for section in CITIES {
        let script = Script::parse(&format!("# section: {section}\nwait {TICKS}\n")).unwrap();
        let lv = common::level(&common::data(), section);
        let mut w = common::world(&lv, &script, true);
        let mut ours = Vec::new();
        replay::run(&mut w, &script, RunOptions::default(), |w, _| {
            ours.push(gfa_sim::trains::trace_line(w.tick, &w.trains));
        });
        let ticks = TICKS.to_string();
        let theirs = oracle_lines("traintrace", &[&map_of(section)], &[&ticks], "r ");
        same(&format!("trains [{section}]"), &ours, &theirs);
    }
}
