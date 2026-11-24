//! Generate synthetic DOSBox-X probe overlays (`gfa-tool probe-overlay`).

use std::path::Path;

use anyhow::{Context, Result, bail};
use gfa_formats::cmp::{Block, BlockPos, Cmp, CmpBuilder, District, StartupObject};
use gfa_formats::mission;

const ROAD: u16 = 0x20;
const PAVEMENT: u16 = 0x30;
const BUILDING: u16 = 0x50;

#[derive(Clone, Copy)]
enum Cell {
    Pavement,
    Road,
    Building,
}

struct GeneratedOverlay {
    cmp_bytes: Vec<u8>,
    ini_bytes: Vec<u8>,
    cmp: Cmp,
}

fn fill_arena(cell: impl Fn(u8, u8) -> Cell) -> CmpBuilder {
    let mut b = CmpBuilder::new(1);
    b.sample_number(0);

    let floor_empty = Block::default();
    let road_floor = Block {
        faces: [0, 0, 0, 0, 1],
        ..Block::default()
    };
    let pavement_floor = Block {
        faces: [0, 0, 0, 0, 7],
        ..Block::default()
    };
    let road = Block {
        type_map: ROAD,
        ..Block::default()
    };
    let pavement = Block {
        type_map: PAVEMENT,
        ..Block::default()
    };
    let building = Block {
        type_map: BUILDING,
        faces: [7, 7, 7, 7, 7],
        ..Block::default()
    };

    for x in 104u8..152 {
        for y in 104u8..152 {
            let perimeter = x == 104 || x == 151 || y == 104 || y == 151;
            let kind = if perimeter {
                Cell::Building
            } else {
                cell(x, y)
            };
            b.set(x, y, 0, floor_empty);
            match kind {
                Cell::Building => b.set(x, y, 1, pavement_floor).set(x, y, 2, building),
                Cell::Road => b.set(x, y, 1, road_floor).set(x, y, 2, road),
                Cell::Pavement => b.set(x, y, 1, pavement_floor).set(x, y, 2, pavement),
            };
        }
    }
    b
}

fn finish_cmp(b: &mut CmpBuilder, district_name: &str) -> Result<(Vec<u8>, Cmp)> {
    b.police_station(BlockPos {
        x: 120,
        y: 120,
        z: 4,
    })
    .hospital(BlockPos {
        x: 136,
        y: 120,
        z: 4,
    })
    .fire_station(BlockPos {
        x: 120,
        y: 136,
        z: 4,
    })
    .district(District {
        x: 1,
        y: 1,
        w: 253,
        h: 255,
        sample: 0,
        name: district_name.into(),
    });
    let cmp_bytes = b.build();
    let cmp = Cmp::parse(&cmp_bytes).context("validating generated CMP")?;
    Ok((cmp_bytes, cmp))
}

fn build_mission_ini(title: &str, decls: &[&str]) -> Result<Vec<u8>> {
    let mut lines = vec![
        "[1]".to_string(),
        String::new(),
        format!("{title}, 22, test.cmp, 0,"),
        "100 1 1 1 1 0".to_string(),
        String::new(),
    ];
    lines.extend(decls.iter().map(|s| (*s).to_string()));
    lines.extend([
        "10 1 (0,0,0) MISSION_COUNTER 4 0".to_string(),
        "11 1 (0,0,0) SECRET_MISSION_COUNTER 7 0".to_string(),
        "12 1 (0,0,0) MISSION_TOTAL 11 0".to_string(),
        "13 1 (0,0,0) TARGET_SCORE 1000000 0".to_string(),
        "-1".to_string(),
        "0 DONOWT 0 0 0 0 0".to_string(),
        "-1".to_string(),
        "\x1a".to_string(),
    ]);
    let ini_bytes = lines.join("\r\n").into_bytes();
    mission::parse(&ini_bytes).context("validating generated MISSION.INI")?;
    Ok(ini_bytes)
}

/// Built synthetic overlay files in memory.
fn build_arena() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        let landmark = (132..134).contains(&x) && (124..126).contains(&y);
        let on_road = (126..131).contains(&x) || (126..131).contains(&y);
        if landmark {
            Cell::Building
        } else if on_road {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });

    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Arena")?;
    let ini_bytes = build_mission_ini(
        "Probe Arena",
        &[
            "1 1 (129,128,4) PARKED 31 0",
            "2 1 (127,128,4) PLAYER 1 256",
        ],
    )?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_shooting() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        let target_wall = x == 130 && (127..130).contains(&y);
        let ruler_even = y == 126 && (120..132).contains(&x) && x.is_multiple_of(2);
        if target_wall {
            Cell::Building
        } else if ruler_even {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });

    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Shooting")?;
    let ini_bytes = build_mission_ini(
        "Probe Shooting",
        &[
            "2 1 (127,128,4) PLAYER 0 256",
            "3 (127,130,4) POWERUP 1 99",
            "4 (126,130,4) POWERUP 2 99",
            "5 (125,130,4) POWERUP 3 99",
            "6 (124,130,4) POWERUP 4 99",
        ],
    )?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_getin() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if (124..=130).contains(&x) && (124..=132).contains(&y) {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });

    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Get-In")?;
    let ini_bytes = build_mission_ini(
        "Probe Get-In",
        &[
            "1 1 (128,128,4) PARKED 31 0",
            "2 1 (125,128,4) PARKED 31 0",
            "3 1 (127,125,4) PARKED 4 256",
            "4 1 (127,131,4) PARKED 3 256",
            "5 1 (127,128,4) PLAYER 1 256",
        ],
    )?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_vault() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if x == 130 && (123..=133).contains(&y) {
            Cell::Building
        } else if (123..=129).contains(&x) && (125..=132).contains(&y) {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });
    b.object(StartupObject {
        x: 130 * 64 - 18,
        y: 128 * 64 + 32,
        z: 255,
        kind: 31,
        remap: 129,
        rotation: 0,
        pitch: 0,
        roll: 0,
    })
    .object(StartupObject {
        x: 130 * 64 - 18,
        y: 131 * 64 + 32,
        z: 255,
        kind: 31,
        remap: 129,
        rotation: 512,
        pitch: 0,
        roll: 0,
    });

    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Vault")?;
    let ini_bytes = build_mission_ini(
        "Probe Vault",
        &["1 1 (124,128,4) PARKED 9 0", "2 1 (127,128,4) PLAYER 1 256"],
    )?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_blocked_exit() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if ((x == 130 || x == 126) && (128..=135).contains(&y))
            || (x == 129 && y == 132)
            || (x == 122 && (123..=127).contains(&y))
        {
            Cell::Building
        } else if ((123..=125).contains(&x) || (127..=129).contains(&x)) && (123..=135).contains(&y)
        {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });
    b.object(StartupObject {
        x: 130 * 64 - 19,
        y: 126 * 64 + 32,
        z: 255,
        kind: 31,
        remap: 129,
        rotation: 0,
        pitch: 0,
        roll: 0,
    })
    .object(StartupObject {
        x: 126 * 64 - 22,
        y: 126 * 64 + 16,
        z: 255,
        kind: 9,
        remap: 129,
        rotation: 0,
        pitch: 0,
        roll: 0,
    })
    .object(StartupObject {
        x: 123 * 64 + 19,
        y: 128 * 64 + 32,
        z: 255,
        kind: 31,
        remap: 129,
        rotation: 512,
        pitch: 0,
        roll: 0,
    });

    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Blocked Exit")?;
    let ini_bytes = build_mission_ini("Probe Blocked Exit", &["1 1 (126,127,4) PLAYER 1 768"])?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

pub fn probe_overlay(preset: &str, out: &Path) -> Result<()> {
    let generated = match preset {
        "arena" => build_arena()?,
        "shooting" => build_shooting()?,
        "getin" => build_getin()?,
        "vault" => build_vault()?,
        "blocked_exit" => build_blocked_exit()?,
        _ => bail!(
            "unknown probe overlay preset {preset:?} (available: arena, shooting, getin, vault, blocked_exit)"
        ),
    };

    let gtadata = out.join("GTADATA");
    std::fs::create_dir_all(&gtadata).with_context(|| format!("creating {}", gtadata.display()))?;
    let cmp_path = gtadata.join("TEST.CMP");
    let ini_path = gtadata.join("MISSION.INI");
    std::fs::write(&cmp_path, &generated.cmp_bytes)
        .with_context(|| format!("writing {}", cmp_path.display()))?;
    std::fs::write(&ini_path, &generated.ini_bytes)
        .with_context(|| format!("writing {}", ini_path.display()))?;

    println!(
        "probe-overlay {preset}: wrote {} ({} bytes, {} block records) and {} ({} bytes)",
        cmp_path.display(),
        generated.cmp_bytes.len(),
        generated.cmp.blocks.len(),
        ini_path.display(),
        generated.ini_bytes.len(),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gfa_sim::world::Start;

    #[test]
    fn arena_preset_is_valid_and_compact() {
        let g = build_arena().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].map().as_deref(), Some("test.cmp"));
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!(start.bx, 127);
        assert_eq!(start.by, 128);
        assert_eq!(start.angle, Some(64));
    }

    #[test]
    fn shooting_preset_is_valid_and_compact() {
        let g = build_shooting().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].map().as_deref(), Some("test.cmp"));
        assert_eq!(
            sections[0]
                .decls
                .iter()
                .filter(|d| d.kind == "POWERUP")
                .count(),
            4
        );
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!(start.bx, 127);
        assert_eq!(start.by, 128);
        assert_eq!(start.angle, Some(64));
    }

    #[test]
    fn getin_preset_is_valid_and_compact() {
        let g = build_getin().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].map().as_deref(), Some("test.cmp"));
        assert_eq!(
            sections[0]
                .decls
                .iter()
                .filter(|d| d.kind == "PARKED")
                .count(),
            4
        );
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!(start.bx, 127);
        assert_eq!(start.by, 128);
        assert_eq!(start.angle, Some(64));
    }

    #[test]
    fn vault_preset_is_valid_and_compact() {
        let g = build_vault().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].map().as_deref(), Some("test.cmp"));
        assert_eq!(g.cmp.objects.len(), 2);
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!(start.bx, 127);
        assert_eq!(start.by, 128);
        assert_eq!(start.angle, Some(64));
    }

    #[test]
    fn blocked_exit_preset_is_valid_and_compact() {
        let g = build_blocked_exit().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].map().as_deref(), Some("test.cmp"));
        assert_eq!(g.cmp.objects.len(), 3);
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!(start.bx, 126);
        assert_eq!(start.by, 127);
        assert_eq!(start.angle, Some(192));
    }
}
