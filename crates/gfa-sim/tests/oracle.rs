//! The simulation against the AmiGTA1 oracle, tick by tick.
//!
//! `tools/oracle/build.sh` compiles AmiGTA1's own walking, driving and
//! train code from a sibling `../AmiGTA1` checkout (or `$AMIGTA1`) into
//! small harnesses; each test runs the same script through a harness and
//! through gfa-sim and compares every tick's state. Scripts in
//! `tools/scripts/` take part when their header says `# oracle: walk` or
//! `# oracle: drive` (they may use only the orders the harness knows). The
//! walk and drive harnesses also run the fleet and the trains around the
//! player, and the whole fleet is compared after every tick. Needs the game data,
//! the AmiGTA1 checkout and a C compiler, so these are ignored by plain
//! `cargo test`: run them with `cargo oracle-test`.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use gfa_fixed::rng::NrLcg;
use gfa_sim::replay::{self, RunOptions, trace_line};
use gfa_sim::script::Script;
use gfa_sim::traffic::Traffic;
use gfa_sim::world::Start;

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
/// starting with `prefix`, each followed by the fleet after that tick (the
/// harness prints the fleet only on ticks it traces).
fn our_lines(script: &Script, prefix: &str) -> Vec<String> {
    let lv = common::level(&common::data(), script.section.unwrap_or(1));
    let mut w = common::world(&lv, script, true);
    let mut lines = Vec::new();
    let first = trace_line(&w);
    if first.starts_with(prefix) {
        lines.push(first);
    }
    replay::run(&mut w, script, RunOptions::default(), |w, _| {
        let l = trace_line(w);
        if l.starts_with(prefix) {
            lines.push(l);
            lines.extend(w.traffic.trace_lines(u64::from(w.tick)));
            lines.extend(w.peds.trace_lines(w.tick));
        }
    });
    lines
}

/// The harness's lines for the player (`prefix`), the fleet and the crowd,
/// in order.
fn their_lines(harness: &str, args: &[&Path], prefix: &str) -> Vec<String> {
    let mut lines = oracle_lines(harness, args, &[], "");
    lines.retain(|l| l.starts_with(prefix) || l.starts_with("f ") || l.starts_with("p "));
    lines
}

/// Where the level starts the player, which the fleet is parked around.
fn start_of(script: &Script) -> (i32, i32) {
    if let Some(s) = script.start {
        return s;
    }
    let lv = common::level(&common::data(), script.section.unwrap_or(1));
    let s = Start::from_section(&lv.files.mission).expect("a PLAYER");
    (s.bx, s.by)
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
    let section = script.section.unwrap_or(1);
    let path = script_file(name, text);
    let theirs = their_lines(
        "walktrace",
        &[&map_of(section), &gry_of(section), &path],
        "t ",
    );
    same(name, &our_lines(&script, "t "), &theirs);
}

fn check_drive(name: &str, text: &str) {
    let script = Script::parse(text).unwrap();
    let section = script.section.unwrap_or(1);
    let path = script_file(name, text);
    let (bx, by) = start_of(&script);
    let (bx, by) = (PathBuf::from(bx.to_string()), PathBuf::from(by.to_string()));
    let theirs = their_lines(
        "drivetrace",
        &[&map_of(section), &gry_of(section), &path, &bx, &by],
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

/// The fleet run as `traffictrace` runs it: parked out to 8 blocks around
/// `(bx, by)`, then ticked with the camera held there.
fn our_traffic(section: u32, bx: i32, by: i32, ticks: u64, seed: u64, view: i32) -> Vec<String> {
    let script = Script::parse(&format!("# section: {section}\n")).unwrap();
    let lv = common::level(&common::data(), section);
    let w = common::world(&lv, &script, true);
    let mut t = Traffic::new(&w.map, seed);
    t.set_view_blocks(view);
    let cap = t.fleet_cap;
    t.park(&w.map, &w.cars, bx, by, 8, cap);
    let mut lines: Vec<String> = t.trace_lines(0).collect();
    let cam_x = i64::from(bx * 32 + 16) << 16;
    let cam_y = i64::from(by * 32 + 16) << 16;
    for tick in 1..=ticks {
        t.step(&w.map, &w.cars, cam_x, cam_y);
        lines.extend(t.trace_lines(tick));
    }
    lines
}

fn check_traffic(section: u32, bx: i32, by: i32, ticks: u64, seed: u64, view: i32) {
    let ours = our_traffic(section, bx, by, ticks, seed, view);
    let args = [
        bx.to_string(),
        by.to_string(),
        ticks.to_string(),
        seed.to_string(),
        view.to_string(),
    ];
    let extra: Vec<&str> = args.iter().map(String::as_str).collect();
    let theirs = oracle_lines(
        "traffictrace",
        &[&map_of(section), &gry_of(section)],
        &extra,
        "f ",
    );
    same(
        &format!("traffic [{section}] ({bx},{by}) seed {seed}"),
        &ours,
        &theirs,
    );
}

/// AmiGTA1's traffic gate (`fullgate.sh`): its four sites on Liberty City,
/// seeds 1 to 3, 12,000 ticks each, with the fleet's view `gtadump drive`
/// gives it (6 blocks). What the runs do is `traffic_gate`'s (data.rs).
#[test]
#[ignore = "needs game data and ../AmiGTA1 (cargo oracle-test)"]
fn oracle_traffic() {
    let ticks: u64 = std::env::var("GFA_TRAFFIC_TICKS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(12_000);
    for seed in [1, 2, 3] {
        for (bx, by) in [(64, 64), (61, 52), (108, 228), (106, 114)] {
            check_traffic(1, bx, by, ticks, seed, 6);
        }
    }
}
