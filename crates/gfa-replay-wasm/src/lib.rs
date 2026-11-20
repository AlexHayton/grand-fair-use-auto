//! The replay runner, exported to JavaScript for the determinism check.
//!
//! The `wasm_matches_native` test (`cargo oracle-test`) builds this for
//! `wasm32-unknown-unknown`, runs scripts through it in Node, and compares
//! the run logs and state hashes with the same runs done natively. Everything here is the shared
//! [`gfa_sim::replay::run_bytes`]; this crate only moves bytes across.

use gfa_sim::replay::{RunOptions, run_bytes};
use wasm_bindgen::prelude::*;

/// The map a mission section plays on, relative to `GTADATA/`.
#[wasm_bindgen]
pub fn section_map(mission_ini: &[u8], section: u32) -> Result<String, String> {
    let sections = gfa_formats::mission::parse(mission_ini).map_err(|e| e.to_string())?;
    sections
        .iter()
        .find(|s| s.id == section)
        .and_then(|s| s.map())
        .ok_or_else(|| format!("no map for section [{section}]"))
}

/// The style number a map asks for (`STYLE00n`).
#[wasm_bindgen]
pub fn map_style(cmp: &[u8]) -> Result<u8, String> {
    gfa_formats::cmp::Cmp::parse(cmp)
        .map(|m| m.style_number)
        .map_err(|e| e.to_string())
}

/// Run `script` on a level and return the log and hashes as text, exactly
/// as `gfa-tool run` prints them. `ticks` below 0 runs the whole script;
/// `hash_every` 0 records no hashes.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn replay(
    cmp: &[u8],
    style: &[u8],
    mission_ini: &[u8],
    section: u32,
    gta: bool,
    script: &str,
    ticks: i32,
    hash_every: u32,
) -> Result<String, String> {
    let opts = RunOptions {
        ticks: u32::try_from(ticks).ok(),
        hash_every: (hash_every > 0).then_some(hash_every),
    };
    run_bytes(cmp, style, mission_ini, section, gta, script, opts).map(|r| r.to_text())
}
