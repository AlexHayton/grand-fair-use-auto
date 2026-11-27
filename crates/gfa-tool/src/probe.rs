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
    Water,
}

struct GeneratedOverlay {
    cmp_bytes: Vec<u8>,
    ini_bytes: Vec<u8>,
    cmp: Cmp,
}

fn fill_arena(cell: impl Fn(u8, u8) -> Cell) -> CmpBuilder {
    fill_arena_width(104, 151, cell)
}

fn fill_arena_width(west: u8, east: u8, cell: impl Fn(u8, u8) -> Cell) -> CmpBuilder {
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
    let water_floor = Block {
        faces: [0, 0, 0, 0, 41],
        ..Block::default()
    };
    let water = Block {
        type_map: 0x10,
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

    for x in west..=east {
        for y in 104u8..152 {
            let perimeter = x == west || x == east || y == 104 || y == 151;
            let kind = if perimeter {
                Cell::Building
            } else {
                cell(x, y)
            };
            match kind {
                Cell::Building => {
                    b.set(x, y, 0, floor_empty)
                        .set(x, y, 1, pavement_floor)
                        .set(x, y, 2, building);
                }
                Cell::Road => {
                    b.set(x, y, 0, floor_empty)
                        .set(x, y, 1, road_floor)
                        .set(x, y, 2, road);
                }
                Cell::Pavement => {
                    b.set(x, y, 0, floor_empty)
                        .set(x, y, 1, pavement_floor)
                        .set(x, y, 2, pavement);
                }
                Cell::Water => {
                    b.set(x, y, 0, water_floor).set(x, y, 1, water);
                }
            }
        }
    }
    b
}

fn finish_cmp(b: &mut CmpBuilder, district_name: &str) -> Result<(Vec<u8>, Cmp)> {
    finish_cmp_with_hospital(
        b,
        district_name,
        BlockPos {
            x: 136,
            y: 120,
            z: 4,
        },
    )
}

fn finish_cmp_with_hospital(
    b: &mut CmpBuilder,
    district_name: &str,
    hospital: BlockPos,
) -> Result<(Vec<u8>, Cmp)> {
    b.police_station(BlockPos {
        x: 120,
        y: 120,
        z: 4,
    })
    .hospital(hospital)
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
    build_mission_ini_with_cmds(title, decls, &["0 DONOWT 0 0 0 0 0"])
}

fn build_mission_ini_with_cmds(title: &str, decls: &[&str], cmds: &[&str]) -> Result<Vec<u8>> {
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
    ]);
    lines.extend(cmds.iter().map(|s| (*s).to_string()));
    lines.extend(["-1".to_string(), "\x1a".to_string()]);
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

fn build_combat_water() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if (123..=132).contains(&x) && (130..=135).contains(&y) {
            Cell::Water
        } else if x == 130 && (126..=129).contains(&y) {
            Cell::Building
        } else {
            Cell::Pavement
        }
    });
    let road_floor = Block {
        faces: [0, 0, 0, 0, 1],
        ..Block::default()
    };
    let hospital_building = Block {
        type_map: BUILDING,
        faces: [7, 7, 7, 7, 7],
        ..Block::default()
    };
    let road_down = Block {
        type_map: ROAD | 0x02,
        ..Block::default()
    };
    let field_right = Block {
        type_map: 0x48,
        ..Block::default()
    };
    b.set(126, 124, 2, hospital_building)
        .set(127, 124, 1, road_floor)
        .set(127, 124, 2, road_down)
        .set(127, 124, 3, field_right)
        .set(128, 124, 1, road_floor)
        .set(128, 124, 2, road_down);

    let (cmp_bytes, cmp) = finish_cmp_with_hospital(
        &mut b,
        "Probe Combat Water",
        BlockPos {
            x: 127,
            y: 124,
            z: 3,
        },
    )?;
    let ini_bytes = build_mission_ini(
        "Probe Combat Water",
        &[
            "1 1 (125,128,4) PARKED 31 0",
            "2 1 (127,128,4) PLAYER 0 0",
            "3 (128,128,4) POWERUP 3 5",
            "4 (126,128,4) POWERUP 1 20",
        ],
    )?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_combat_score() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if (120..=136).contains(&x) && (120..=136).contains(&y) {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });
    // Two Bulldogs west of player: #2 at (125.5, 128.5) and #3 22 wpx south at (125.5, 129.1875).
    b.object(StartupObject {
        x: 125 * 64 + 32,
        y: 128 * 64 + 32,
        z: 255,
        kind: 31,
        remap: 129,
        rotation: 256,
        pitch: 0,
        roll: 0,
    })
    .object(StartupObject {
        x: 125 * 64 + 32,
        y: 128 * 64 + 32 + 44,
        z: 255,
        kind: 31,
        remap: 129,
        rotation: 256,
        pitch: 0,
        roll: 0,
    });

    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Combat Score")?;
    let ini_bytes = build_mission_ini_with_cmds(
        "Probe Combat Score",
        &[
            "2 1 (127,128,4) PLAYER 0 256",
            "3 1 (129,128,4) PARKED 31 0",
            "6 1 (123,128,4) PARKED 9 0",
            "20 (127,125,4) FUTURECAR 31 256",
            "21 (110,110,4) FUTURECAR 31 256",
            "22 (132,134,4) FUTURECAR 31 0",
            "23 1 (8352,8608,256) FUTUREPED 0 256",
            "30 (127,126,4) TRIGGER 200 0",
            "31 (127,130,4) TRIGGER 210 0",
            "32 (129,132,4) TRIGGER 220 0",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "200 DISABLE 30 0 0 0 0",
            "201 PARKED_ON 20 0 -1 0 0",
            "202 SURVIVE 0 0 0 30 0",
            "203 EXPLODE_CAR 20 0 -1 0 0",
            "204 DONOWT 0 0 0 0 0",
            "210 DISABLE 31 0 0 0 0",
            "211 PARKED_ON 21 0 -1 0 0",
            "212 SURVIVE 0 0 0 30 0",
            "213 EXPLODE_CAR 21 0 -1 0 0",
            "214 DONOWT 0 0 0 0 0",
            "220 DISABLE 32 0 0 0 0",
            "221 PARKED_ON 22 0 -1 0 0",
            "222 PED_ON 23 0 -1 136 0",
            "223 PED_WEAPON 23 0 0 2 0",
            "224 CHANGE_PED_TYPE 23 0 -1 12 22",
            "225 DONOWT 0 0 0 0 0",
        ],
    )?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_combat_score_nonplayer() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if (x == 130 && y == 129) || (x == 111 && y == 110) {
            Cell::Building
        } else if (120..=136).contains(&x) && (120..=136).contains(&y) {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });
    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Combat Score Nonplayer")?;
    let ini_bytes = build_mission_ini_with_cmds(
        "Probe Combat Score Nonplayer",
        &[
            "2 1 (127,129,4) PLAYER 1 512",
            "9 (127,129,4) TRIGGER 100 0",
            "20 (129,129,4) FUTURECAR 31 0",
            "21 (110,110,4) FUTURECAR 31 0",
            "22 (127,127,4) FUTURECAR 31 256",
            "23 1 (8152,8048,256) FUTUREPED 0 0",
            "24 1 (8168,8048,256) FUTUREPED 0 0",
            "30 (130,129,3) DUMMY 0 0",
            "31 (111,110,3) DUMMY 0 0",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 PARKED_ON 20 0 -1 0 0",
            "102 SURVIVE 0 0 0 110 0",
            "103 EXPLODE 30 0 0 0 0",
            "104 SURVIVE 0 0 0 90 0",
            "105 PARKED_ON 21 0 -1 0 0",
            "106 SURVIVE 0 0 0 15 0",
            "107 EXPLODE 31 0 0 0 0",
            "108 SURVIVE 0 0 0 90 0",
            "109 PARKED_ON 22 0 -1 0 0",
            "110 PED_ON 23 0 0 147 0",
            "111 PED_ON 24 0 0 147 0",
            "112 PED_WEAPON 23 0 0 3 0",
            "113 PED_WEAPON 24 0 0 1 0",
            "114 CHANGE_PED_TYPE 23 0 -1 21 2",
            "115 CHANGE_PED_TYPE 24 0 -1 21 2",
            "116 DONOWT 0 0 0 0 0",
        ],
    )?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_combat_score_script() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if (x, y) == (130, 129) {
            Cell::Building
        } else {
            Cell::Road
        }
    });
    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Script Car Explosion")?;
    let ini_bytes = build_mission_ini_with_cmds(
        "Probe Script Car Explosion",
        &[
            "2 1 (127,129,4) PLAYER 1 512",
            "9 1 (127,129,4) TRIGGER 100 1",
            "20 (129,129,4) FUTURECAR 31 0",
            "21 (125,129,4) FUTURECAR 31 0",
            "22 (127,130,4) FUTURECAR 31 0",
            "30 (130,129,1) DUMMY 0 0",
            "31 (130,129,2) DUMMY 0 0",
            "32 (130,129,3) DUMMY 0 0",
            "33 (130,129,4) DUMMY 0 0",
            "34 (130,129,5) DUMMY 0 0",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 PARKED_ON 20 0 -1 0 0",
            "102 PARKED_ON 21 0 -1 0 0",
            "103 EXPLODE 30 0 0 0 0",
            "104 SURVIVE 0 0 0 30 0",
            "105 EXPLODE 31 0 0 0 0",
            "106 SURVIVE 0 0 0 30 0",
            "107 EXPLODE 32 0 0 0 0",
            "108 SURVIVE 0 0 0 30 0",
            "109 EXPLODE 33 0 0 0 0",
            "110 SURVIVE 0 0 0 30 0",
            "111 EXPLODE 34 0 0 0 0",
            "112 PARKED_ON 22 0 -1 0 0",
            "113 DONOWT 0 0 0 0 0",
        ],
    )?;
    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_combat_score_npc() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if (x, y) == (130, 129) {
            Cell::Pavement
        } else {
            Cell::Road
        }
    });
    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe NPC Car Explosion")?;
    let ini_bytes = build_mission_ini_with_cmds(
        "Probe NPC Car Explosion",
        &[
            "2 1 (127,129,4) PLAYER 1 256",
            "9 1 (127,129,4) TRIGGER 100 1",
            "20 (128,129,4) FUTURECAR 31 0",
            "21 1 (8328,8288,256) FUTUREPED 0 768",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 PARKED_ON 20 0 -1 0 0",
            "102 UNLOCK_DOOR 20 0 0 0 0",
            "103 SET_COLLIDE 20 0 0 0 0",
            "104 PED_ON 21 0 0 147 0",
            "105 CHANGE_PED_TYPE 21 0 -1 21 2",
            "106 PED_WEAPON 21 0 0 3 0",
            "107 DONOWT 0 0 0 0 0",
        ],
    )?;
    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_combat_score_npc_parked() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if (x, y) == (130, 129) {
            Cell::Pavement
        } else {
            Cell::Road
        }
    });
    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Parked NPC Car Explosion")?;
    let ini_bytes = build_mission_ini_with_cmds(
        "Probe Parked NPC Car Explosion",
        &[
            "2 1 (127,129,4) PLAYER 1 256",
            "9 1 (127,129,4) TRIGGER 100 1",
            "20 1 (128,129,4) PARKED 31 0",
            "21 1 (8328,8288,256) FUTUREPED 0 768",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 PED_ON 21 0 0 147 0",
            "102 CHANGE_PED_TYPE 21 0 -1 21 2",
            "103 PED_WEAPON 21 0 0 3 0",
            "104 DONOWT 0 0 0 0 0",
        ],
    )?;
    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_combat_punch() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if (118..=122).contains(&x) && (126..=130).contains(&y) && (x, y) != (120, 128) {
            Cell::Building
        } else {
            Cell::Pavement
        }
    });
    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Combat Punch")?;

    let ini_bytes = build_mission_ini_with_cmds(
        "Probe Combat Punch",
        &[
            "2 1 (120,128,4) PLAYER 1 256",
            "9 (120,128,4) TRIGGER 100 1",
            "20 1 (7732,8224,256) FUTUREPED 0 768",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 PED_ON 20 0 -1 136 0",
            "102 CHANGE_PED_TYPE 20 0 -1 4 0",
            "103 SET_PED_SPEED 20 0 0 0 0",
            "104 DONOWT 0 0 0 0 0",
        ],
    )?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_combat_tanker() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if x == 138 && (122..=134).contains(&y) {
            Cell::Building
        } else if (120..=137).contains(&x) && (122..=134).contains(&y) {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });
    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Combat Tanker")?;
    let ini_bytes = build_mission_ini_with_cmds(
        "Probe Combat Tanker",
        &[
            "2 1 (127,128,4) PLAYER 0 256",
            "9 (127,128,4) TRIGGER 100 0",
            "20 (129,128,4) FUTURECAR 44 256",
            "21 (129,125,4) FUTURECAR 44 256",
            "22 (129,131,4) FUTURECAR 31 256",
            "23 (124,128,4) FUTURECAR 44 256",
            "24 (127,126,4) POWERUP 1 50",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 PARKED_ON 20 0 -1 0 0",
            "102 PARKED_ON 21 0 -1 0 0",
            "103 PARKED_ON 22 0 -1 0 0",
            "104 PARKED_ON 23 0 -1 0 0",
            "105 SETBOMB 20 0 0 4 0",
            "106 SETBOMB 22 0 0 4 0",
            "107 KICKSTART 200 0 -1 0 0",
            "108 DONOWT 0 0 0 0 0",
            "200 STEAL 21 0 -1 0 0",
            "201 SETBOMB 21 0 0 4 0",
            "202 DONOWT 0 0 0 0 0",
        ],
    )?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_combat_tanker_drive() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if x == 135 && (122..=134).contains(&y) {
            Cell::Building
        } else if (120..=134).contains(&x) && (122..=134).contains(&y) {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });
    // Plain unbombed StartupObject Tanker A at (129.5, 125.5) facing EAST.
    b.object(StartupObject {
        x: 129 * 64 + 32,
        y: 125 * 64 + 32,
        z: 255,
        kind: 44,
        remap: 129,
        rotation: 256,
        pitch: 0,
        roll: 0,
    });
    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Combat Tanker Drive")?;
    let ini_bytes = build_mission_ini_with_cmds(
        "Probe Combat Tanker Drive",
        &[
            "2 1 (130,130,4) PLAYER 1 512",
            "9 (130,130,4) TRIGGER 100 0",
            "21 (129,129,4) FUTURECAR 44 256",
            "22 (129,132,4) FUTURECAR 31 256",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 PARKED_ON 21 0 0 0 0",
            "102 PARKED_ON 22 0 0 0 0",
            "103 KICKSTART 200 0 -1 0 0",
            "104 KICKSTART 210 0 -1 0 0",
            "105 DONOWT 0 0 0 0 0",
            "200 STEAL 21 0 0 0 5000",
            "201 SETBOMB 21 0 0 4 0",
            "202 DONOWT 0 0 0 0 0",
            "210 STEAL 22 0 0 0 5000",
            "211 SETBOMB 22 0 0 4 0",
            "212 DONOWT 0 0 0 0 0",
        ],
    )?;

    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

fn build_combat_tanker_npc() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if x == 135 && (122..=134).contains(&y) {
            Cell::Building
        } else if (120..=134).contains(&x) && (122..=134).contains(&y) {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });
    // A normal traffic Tanker; unlike a FUTURECAR, it becomes the player's
    // ordinary driven vehicle and can be left parked after exiting.
    b.object(StartupObject {
        x: 129 * 64 + 32,
        y: 128 * 64 + 32,
        z: 255,
        kind: 44,
        remap: 129,
        rotation: 0,
        pitch: 0,
        roll: 0,
    });
    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Combat Tanker NPC")?;
    let ini_bytes = build_mission_ini_with_cmds(
        "Probe Combat Tanker NPC",
        &[
            "2 1 (126,128,4) PLAYER 1 256",
            "9 (126,128,4) TRIGGER 100 0",
            "20 1 (8608,8224,256) FUTUREPED 0 512",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 PED_ON 20 0 0 147 0",
            "102 CHANGE_PED_TYPE 20 0 -1 21 2",
            "103 PED_WEAPON 20 0 0 3 0",
            "104 DONOWT 0 0 0 0 0",
        ],
    )?;
    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

/// One arena for every type-4 tanker-bomb check, for the owner to eyeball.
///
/// West column (x=121), parked and unmanned, 4 blocks apart so their blasts
/// cannot reach each other: SETBOMB-4 Tanker (y=123), SETBOMB-4 Bulldog
/// (y=127), plain Tanker (y=131). Shoot them for the blast counts.
///
/// Driven pair (x=125, facing EAST, 15-block runway to the wall at x=140):
/// SETBOMB-4 Tanker B (y=128) and SETBOMB-4 Bulldog C (y=132). The player
/// starts on foot on the driver's (north) flank of Tanker B.
fn build_combat_tanker_bomb() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if x == 140 && (120..=136).contains(&y) {
            Cell::Building
        } else if (118..=139).contains(&x) && (120..=136).contains(&y) {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });
    // Plain, unbombed StartupObject Tanker (control), parked unmanned.
    b.object(StartupObject {
        x: 121 * 64 + 32,
        y: 131 * 64 + 32,
        z: 255,
        kind: 44,
        remap: 129,
        rotation: 256,
        pitch: 0,
        roll: 0,
    });
    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Combat Tanker Bomb")?;
    let ini_bytes = build_mission_ini_with_cmds(
        "Probe Combat Tanker Bomb",
        &[
            "2 1 (125,127,4) PLAYER 1 512",
            "9 (125,127,4) TRIGGER 100 0",
            "20 (121,123,4) FUTURECAR 44 256",
            "21 (121,127,4) FUTURECAR 31 256",
            "22 (125,128,4) FUTURECAR 44 256",
            "23 (125,132,4) FUTURECAR 31 256",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 PARKED_ON 20 0 -1 0 0",
            "102 PARKED_ON 21 0 -1 0 0",
            "103 PARKED_ON 22 0 0 0 0",
            "104 PARKED_ON 23 0 0 0 0",
            "105 SETBOMB 20 0 0 4 0",
            "106 SETBOMB 21 0 0 4 0",
            "107 KICKSTART 200 0 -1 0 0",
            "108 KICKSTART 210 0 -1 0 0",
            "109 DONOWT 0 0 0 0 0",
            "200 STEAL 22 0 0 0 5000",
            "201 SETBOMB 22 0 0 4 0",
            "202 DONOWT 0 0 0 0 0",
            "210 STEAL 23 0 0 0 5000",
            "211 SETBOMB 23 0 0 4 0",
            "212 DONOWT 0 0 0 0 0",
        ],
    )?;
    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

/// An arena for the tank (model 37, placed from the map's object list, door
/// at its driver's north flank), for
/// the owner to eyeball: handling, the turret, the cannon, armour and what
/// it does to cars and people.
///
/// The player starts on foot beside the tank at (123,128), facing east down
/// a 20-block road to a wall at x=146. Along its path (y=128): two Bulldogs
/// (one lengthways, one broadside), then a Bus. Cannon targets off the line
/// of travel: Bulldogs at (132,122) and (132,134), for aiming the turret
/// away from the hull. A second, unmanned tank at (123,133) is the armour
/// target (shoot it with the cheat's rockets and machine gun). Three
/// motionless peds stand in the tank's way.
fn build_combat_tank() -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|x, y| {
        if x == 146 && (116..=140).contains(&y) {
            Cell::Building
        } else if (118..=145).contains(&x) && (116..=140).contains(&y) {
            Cell::Road
        } else {
            Cell::Pavement
        }
    });
    // Both tanks come from the map's object list, as the original places them.
    for (x, y) in [(123, 128), (123, 133)] {
        b.object(StartupObject {
            x: x * 64 + 32,
            y: y * 64 + 32,
            z: 255,
            kind: 37,
            remap: 129,
            rotation: 256,
            pitch: 0,
            roll: 0,
        });
    }
    let (cmp_bytes, cmp) = finish_cmp(&mut b, "Probe Combat Tank")?;
    let ini_bytes = build_mission_ini_with_cmds(
        "Probe Combat Tank",
        &[
            "5 1 (128,128,4) PARKED 31 256",
            "6 1 (131,128,4) PARKED 31 0",
            "7 1 (136,128,4) PARKED 9 256",
            "8 1 (132,122,4) PARKED 31 256",
            "14 1 (132,134,4) PARKED 31 256",
            // After the cars: `PLAYER 1` names a start car that does not
            // exist here, and declared before the cars the one he gets
            // into will not drive (GTA8.EXE, M6b).
            "2 1 (123,127,4) PLAYER 1 512",
            "9 (123,127,4) TRIGGER 100 0",
            "20 1 (8032,8224,256) FUTUREPED 0 768",
            "21 1 (8288,8480,256) FUTUREPED 0 768",
            "22 1 (8416,8224,256) FUTUREPED 0 768",
        ],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 PED_ON 20 0 -1 136 0",
            "102 CHANGE_PED_TYPE 20 0 -1 4 0",
            "103 SET_PED_SPEED 20 0 0 0 0",
            "104 PED_ON 21 0 -1 136 0",
            "105 CHANGE_PED_TYPE 21 0 -1 4 0",
            "106 SET_PED_SPEED 21 0 0 0 0",
            "107 PED_ON 22 0 -1 136 0",
            "108 CHANGE_PED_TYPE 22 0 -1 4 0",
            "109 SET_PED_SPEED 22 0 0 0 0",
            "110 DONOWT 0 0 0 0 0",
        ],
    )?;
    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

/// An empty arena for measuring the tank's handling from captures: one
/// object-list tank at `(bx, 128)` facing EAST on open road filling the
/// whole arena (inside its perimeter wall, x and y 105..150), nothing else
/// on it. The player starts on foot at its driver's (north) flank.
fn build_tank_handling(name: &str, bx: u8) -> Result<GeneratedOverlay> {
    let mut b = fill_arena(|_, _| Cell::Road);
    b.object(StartupObject {
        x: i16::from(bx) * 64 + 32,
        y: 128 * 64 + 32,
        z: 255,
        kind: 37,
        remap: 129,
        rotation: 256,
        pitch: 0,
        roll: 0,
    });
    let (cmp_bytes, cmp) = finish_cmp(&mut b, name)?;
    let player = format!("2 1 ({bx},127,4) PLAYER 1 512");
    let trigger = format!("9 ({bx},127,4) TRIGGER 100 0");
    let ini_bytes = build_mission_ini_with_cmds(
        name,
        &[&player, &trigger],
        &[
            "0 DONOWT 0 0 0 0 0",
            "100 DISABLE 9 0 0 0 0",
            "101 DONOWT 0 0 0 0 0",
        ],
    )?;
    Ok(GeneratedOverlay {
        cmp_bytes,
        ini_bytes,
        cmp,
    })
}

/// The straight-line runway: the tank at the west end, 43 blocks to the wall.
fn build_tank_drive() -> Result<GeneratedOverlay> {
    build_tank_handling("Probe Tank Drive", 107)
}

/// The turning pad: the tank in the middle, 20 blocks clear all round.
fn build_tank_turn() -> Result<GeneratedOverlay> {
    build_tank_handling("Probe Tank Turn", 128)
}

fn build_camera_runway() -> Result<GeneratedOverlay> {
    let name = "Probe Camera Runway";
    let mut b = fill_arena_width(20, 235, |_, _| Cell::Road);
    let (cmp_bytes, cmp) = finish_cmp(&mut b, name)?;
    let ini_bytes = build_mission_ini(
        name,
        &["1 1 (34,128,4) PARKED 4 256", "2 1 (34,127,4) PLAYER 1 512"],
    )?;
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
        "combat_water" => build_combat_water()?,
        "combat_score" => build_combat_score()?,
        "combat_score_nonplayer" => build_combat_score_nonplayer()?,
        "combat_score_script" => build_combat_score_script()?,
        "combat_score_npc" => build_combat_score_npc()?,
        "combat_score_npc_parked" => build_combat_score_npc_parked()?,
        "combat_punch" => build_combat_punch()?,
        "combat_tanker" => build_combat_tanker()?,
        "combat_tanker_drive" => build_combat_tanker_drive()?,
        "combat_tanker_npc" => build_combat_tanker_npc()?,
        "combat_tanker_bomb" => build_combat_tanker_bomb()?,
        "combat_tank" => build_combat_tank()?,
        "tank_drive" => build_tank_drive()?,
        "tank_turn" => build_tank_turn()?,
        "camera_runway" => build_camera_runway()?,
        _ => bail!(
            "unknown probe overlay preset {preset:?} (available: arena, shooting, getin, vault, blocked_exit, combat_water, combat_score, combat_score_nonplayer, combat_score_script, combat_score_npc, combat_score_npc_parked, combat_punch, combat_tanker, combat_tanker_drive, combat_tanker_npc, combat_tanker_bomb, combat_tank, tank_drive, tank_turn, camera_runway)"
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

    #[test]
    fn combat_water_preset_is_valid_and_compact() {
        let g = build_combat_water().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert_eq!(g.cmp.column_height(127, 130), 2);
        assert_eq!(
            g.cmp.block(127, 130, 1).unwrap().ground_type(),
            gfa_formats::cmp::GroundType::Water
        );
        assert_eq!(
            g.cmp.locations[1][0],
            BlockPos {
                x: 127,
                y: 124,
                z: 3
            }
        );
        assert!(g.cmp.block(127, 124, 3).unwrap().right());
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!((start.bx, start.by, start.angle), (127, 128, Some(0)));
    }

    #[test]
    fn combat_score_preset_is_valid_and_compact() {
        let g = build_combat_score().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert_eq!(g.cmp.objects.len(), 2);
        assert!(sections[0].cmds.len() >= 15);
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!((start.bx, start.by, start.angle), (127, 128, Some(64)));
    }

    #[test]
    fn combat_score_nonplayer_preset_is_valid_and_compact() {
        let g = build_combat_score_nonplayer().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert!(sections[0].cmds.len() >= 17);
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!((start.bx, start.by, start.angle), (127, 129, Some(128)));
    }

    #[test]
    fn combat_punch_preset_is_valid_and_compact() {
        let g = build_combat_punch().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert_eq!(
            g.cmp.block(120, 127, 2).unwrap().ground_type(),
            gfa_formats::cmp::GroundType::Building
        );
        assert_eq!(
            g.cmp.block(120, 128, 2).unwrap().ground_type(),
            gfa_formats::cmp::GroundType::Pavement
        );
        assert_eq!(
            sections[0]
                .decls
                .iter()
                .filter(|d| d.kind == "FUTUREPED")
                .count(),
            1
        );
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!((start.bx, start.by, start.angle), (120, 128, Some(64)));
    }

    #[test]
    fn combat_tanker_preset_is_valid_and_compact() {
        let g = build_combat_tanker().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert_eq!(
            sections[0]
                .decls
                .iter()
                .filter(|d| d.kind == "FUTURECAR")
                .count(),
            4
        );
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!((start.bx, start.by, start.angle), (127, 128, Some(64)));
    }

    #[test]
    fn combat_tanker_drive_preset_is_valid_and_compact() {
        let g = build_combat_tanker_drive().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(sections.len(), 1);
        assert_eq!(g.cmp.objects.len(), 1);
        assert_eq!(
            sections[0]
                .decls
                .iter()
                .filter(|d| d.kind == "FUTURECAR")
                .count(),
            2
        );
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!((start.bx, start.by, start.angle), (130, 130, Some(128)));
    }
    #[test]
    fn combat_tanker_bomb_preset_is_valid_and_compact() {
        let g = build_combat_tanker_bomb().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(g.cmp.objects.len(), 1);
        assert_eq!(
            sections[0]
                .decls
                .iter()
                .filter(|d| d.kind == "FUTURECAR")
                .count(),
            4
        );
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!((start.bx, start.by, start.angle), (125, 127, Some(128)));
    }
    #[test]
    fn combat_tank_preset_is_valid_and_compact() {
        let g = build_combat_tank().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(g.cmp.objects.len(), 2);
        assert_eq!(
            sections[0]
                .decls
                .iter()
                .filter(|d| d.kind == "PARKED")
                .count(),
            5
        );
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!((start.bx, start.by, start.angle), (123, 127, Some(128)));
    }
    #[test]
    fn tank_drive_preset_is_valid_and_compact() {
        let g = build_tank_drive().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        assert!(g.cmp.blocks.len() < 20, "blocks: {}", g.cmp.blocks.len());
        assert_eq!(g.cmp.objects.len(), 1);
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!((start.bx, start.by, start.angle), (107, 127, Some(128)));
        let g = build_tank_turn().unwrap();
        let sections = mission::parse(&g.ini_bytes).unwrap();
        let start = Start::from_section(&sections[0]).unwrap();
        assert_eq!((start.bx, start.by), (128, 127));
    }
}
