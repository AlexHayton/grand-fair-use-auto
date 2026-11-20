//! Determinism across platforms: the same scripts, run natively and as
//! wasm in Node, give the same run logs and state hashes.
//!
//! Builds this crate for `wasm32-unknown-unknown` (into
//! `target/wasmcheck`, so it does not wait on the test's own build), binds
//! it with the wasm-bindgen CLI that matches `Cargo.lock` (Trunk's cached
//! copy if there is one), and runs `tools/wasmcheck/replay.mjs`. Needs the
//! game data, Node and wasm-bindgen: run with `cargo oracle-test`.

use std::path::{Path, PathBuf};
use std::process::Command;

use gfa_assets::level::{self, load_map};
use gfa_assets::{AssetSource, DataPath, Dataset, FsSource};
use gfa_sim::gfa_fixed::rng::NrLcg;
use gfa_sim::replay::{RunOptions, run_bytes};
use gfa_sim::script::Script;

const HASH_EVERY: u32 = 25;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(cmd: &mut Command, what: &str) -> String {
    let out = cmd.output().unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(
        out.status.success(),
        "{what} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The wasm-bindgen CLI whose version matches Cargo.lock.
fn wasm_bindgen() -> PathBuf {
    let lock = std::fs::read_to_string(root().join("Cargo.lock")).unwrap();
    let want = lock
        .split("[[package]]")
        .find(|p| p.contains("name = \"wasm-bindgen\"\n"))
        .and_then(|p| p.lines().find_map(|l| l.strip_prefix("version = \"")))
        .map(|v| v.trim_end_matches('"').to_string())
        .expect("wasm-bindgen in Cargo.lock");
    let home = std::env::var("HOME").unwrap_or_default();
    let trunk = PathBuf::from(home).join(format!(
        "Library/Caches/dev.trunkrs.trunk/wasm-bindgen-{want}/wasm-bindgen"
    ));
    let cli = if trunk.exists() {
        trunk
    } else {
        PathBuf::from("wasm-bindgen")
    };
    let have = run(
        Command::new(&cli).arg("--version"),
        "wasm-bindgen --version",
    );
    assert!(
        have.split_whitespace().nth(1) == Some(want.as_str()),
        "wasm-bindgen {} does not match Cargo.lock's {want}",
        have.trim()
    );
    cli
}

/// Build the replay runner for wasm and bind it for Node.
fn build_wasm() -> PathBuf {
    let target = root().join("target/wasmcheck");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    run(
        Command::new(cargo)
            .current_dir(root())
            .env("CARGO_TARGET_DIR", &target)
            .args([
                "build",
                "--release",
                "--target",
                "wasm32-unknown-unknown",
                "-p",
                "gfa-replay-wasm",
            ]),
        "building gfa-replay-wasm for wasm32",
    );
    let pkg = target.join("pkg");
    let wasm = target.join("wasm32-unknown-unknown/release/gfa_replay_wasm.wasm");
    run(
        Command::new(wasm_bindgen())
            .arg("--target")
            .arg("nodejs")
            .arg("--out-dir")
            .arg(&pkg)
            .arg(&wasm),
        "wasm-bindgen",
    );
    pkg
}

/// The scripts to compare: tools/scripts/ and some long random ones.
fn scripts(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(root().join("tools/scripts"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    out.sort();
    let mut rng = NrLcg::new(7);
    for section in [1, 102, 202] {
        for i in 0..3 {
            let mut text = format!(
                "# section: {section}\nstart {} {}\n",
                40 + rng.next15() % 176,
                40 + rng.next15() % 176
            );
            for _ in 0..60 {
                text += &format!(
                    "{} {} {} {}\n",
                    (rng.next15() % 3) as i32 - 1,
                    [0, 1, 1, -1][rng.next15() as usize % 4],
                    u32::from(rng.next15().is_multiple_of(4)),
                    1 + rng.next15() % 149
                );
            }
            let p = dir.join(format!("random_{section}_{i}.txt"));
            std::fs::write(&p, text).unwrap();
            out.push(p);
        }
    }
    out
}

#[test]
#[ignore = "needs game data, Node and wasm-bindgen (cargo oracle-test)"]
fn wasm_matches_native() {
    let data = DataPath::resolve(None).expect("the game data").root;
    let src = FsSource::new(&data);
    let tmp = std::env::temp_dir().join("gfa-wasmcheck");
    std::fs::create_dir_all(&tmp).unwrap();
    let scripts = scripts(&tmp);
    let pkg = build_wasm();

    let mut node = Command::new("node");
    node.arg(root().join("tools/wasmcheck/replay.mjs"))
        .arg(&pkg)
        .arg(&data)
        .arg("gta")
        .arg(HASH_EVERY.to_string())
        .args(&scripts);
    let wasm = run(&mut node, "node replay.mjs");

    let ini = src.read("GTADATA/MISSION.INI").unwrap();
    let opts = RunOptions {
        ticks: None,
        hash_every: Some(HASH_EVERY),
    };
    let mut native = String::new();
    for path in &scripts {
        let text = std::fs::read_to_string(path).unwrap();
        let section = Script::parse(&text).unwrap().section.unwrap_or(1);
        let files = level::find(&src, Dataset::Gta, section).unwrap();
        let (_, style_path, _) = load_map(&src, &files.map, Dataset::Gta).unwrap();
        let cmp = src.read(&files.map).unwrap();
        let style = src.read(&style_path).unwrap();
        let report = run_bytes(&cmp, &style, &ini, section, true, &text, opts).unwrap();
        native += &format!("== {}\n", path.file_name().unwrap().to_string_lossy());
        native += &report.to_text();
    }

    let (n, w): (Vec<&str>, Vec<&str>) = (native.lines().collect(), wasm.lines().collect());
    if let Some(i) = (0..n.len().max(w.len())).find(|&i| n.get(i) != w.get(i)) {
        panic!(
            "native and wasm differ at line {i}:\n  native: {}\n  wasm:   {}",
            n.get(i).unwrap_or(&"(nothing)"),
            w.get(i).unwrap_or(&"(nothing)")
        );
    }
    let hashes = n.iter().filter(|l| l.starts_with("hash ")).count();
    println!(
        "native and wasm agree: {} scripts, {hashes} hashes, {} lines",
        scripts.len(),
        n.len()
    );
    assert!(hashes > 100);
}
