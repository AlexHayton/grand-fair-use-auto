//! `.CMP` city maps (version 331).
//!
//! Sources: DMA Design's "CityScape Data Structure" document as read by
//! Carnage3D (`src/GameMapManager.cpp`, MIT, © 2019 jericho) and AmiGTA1
//! (`native/gta_map.c`, MIT). Implementation ours.
//!
//! Layout, all little-endian:
//!
//! | section | size |
//! |---|---|
//! | header | 28 bytes: version, style/sample numbers, five section sizes |
//! | base | 256×256 `u32` byte offsets into the column data, `[y][x]` |
//! | columns | `u16` words: per column, the count of empty layers, then block indices top-down |
//! | blocks | 8-byte records: `u16` type, `u8` extended type, faces W E N S lid |
//! | objects | 14-byte startup objects |
//! | routes | `{count, type}` then `count` × `(x, y, z)` |
//! | locations | 6 groups × 6 × `(x, y, z)`: police, hospitals, unused, unused, fire, unused |
//! | navigation | 35-byte district records: `x y w h sample name[30]` |
//!
//! Layer `z = 0` is the ground. The map is kept column-compressed as in the
//! file; [`Cmp::block`] decodes on lookup.

use crate::reader::{FormatError, Reader, latin1_cstr};

pub const CMP_VERSION: u32 = 331;
pub const MAP_DIM: usize = 256;
pub const MAP_LAYERS: usize = 6;
const BLOCK_RECORD: usize = 8;
const OBJECT_RECORD: usize = 14;
const DISTRICT_RECORD: usize = 35;
const LOCATION_GROUPS: usize = 6;
const LOCATIONS_PER_GROUP: usize = 6;
const WHAT: &str = "CMP";

/// Faces in file order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    West = 0,
    East = 1,
    North = 2,
    South = 3,
    Lid = 4,
}

/// What a block is made of, from bits 4..6 of its type word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroundType {
    Air,
    Water,
    Road,
    Pavement,
    Field,
    Building,
    /// Values 6 and 7 are not documented.
    Other(u8),
}

/// One map block record, exactly as stored. Bitfields are decoded on demand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Block {
    pub type_map: u16,
    pub type_map_ext: u8,
    /// Tile number per face, indexed by [`Face`]; 0 means no face.
    pub faces: [u8; 5],
}

impl Block {
    pub fn face(&self, f: Face) -> u8 {
        self.faces[f as usize]
    }
    pub fn up(&self) -> bool {
        self.type_map & 0x01 != 0
    }
    pub fn down(&self) -> bool {
        self.type_map & 0x02 != 0
    }
    pub fn left(&self) -> bool {
        self.type_map & 0x04 != 0
    }
    pub fn right(&self) -> bool {
        self.type_map & 0x08 != 0
    }
    pub fn ground_type(&self) -> GroundType {
        match (self.type_map >> 4) & 7 {
            0 => GroundType::Air,
            1 => GroundType::Water,
            2 => GroundType::Road,
            3 => GroundType::Pavement,
            4 => GroundType::Field,
            5 => GroundType::Building,
            n => GroundType::Other(n as u8),
        }
    }
    /// A flat (billboard-like) block: its faces are drawn as single planes.
    pub fn is_flat(&self) -> bool {
        self.type_map & 0x80 != 0
    }
    /// 0 none; 1–8 26° ramps; 9–40 7° ramps; 41–44 45° ramps (up, down, left, right).
    pub fn slope(&self) -> u8 {
        ((self.type_map >> 8) & 0x3f) as u8
    }
    /// Lid rotation in quarter turns.
    pub fn lid_rotation(&self) -> u8 {
        (self.type_map >> 14) as u8
    }
    /// 0 none, 1 traffic lights, 4–7 train turn/station markers.
    pub fn traffic_hint(&self) -> u8 {
        self.type_map_ext & 7
    }
    /// Lid palette remap, 0–3.
    pub fn remap(&self) -> u8 {
        (self.type_map_ext >> 3) & 3
    }
    pub fn flip_top_bottom(&self) -> bool {
        self.type_map_ext & 0x20 != 0
    }
    pub fn flip_left_right(&self) -> bool {
        self.type_map_ext & 0x40 != 0
    }
    pub fn is_railway(&self) -> bool {
        self.type_map_ext & 0x80 != 0
    }
}

/// A position in blocks, as the file stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockPos {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

/// An object or vehicle placed when the level starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StartupObject {
    pub x: i16,
    pub y: i16,
    pub z: i16,
    pub kind: u8,
    pub remap: u8,
    pub rotation: i16,
    pub pitch: i16,
    pub roll: i16,
}

/// A route: a police patrol (`kind` 0xff), another class (0xfe), or the
/// roadblock positions for the district with that index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub kind: u8,
    pub nodes: Vec<BlockPos>,
}

/// A named rectangle of the map. Records with zero size are kept so that
/// indices match the roadblock zone numbers (AmiGTA1 `gta_map.h`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct District {
    pub x: u8,
    pub y: u8,
    pub w: u8,
    pub h: u8,
    pub sample: u8,
    pub name: String,
}

impl District {
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }
}

/// A parsed city map.
#[derive(Debug, Clone)]
pub struct Cmp {
    pub style_number: u8,
    pub sample_number: u8,
    /// Byte offsets into the column data, `[y * 256 + x]`.
    base: Vec<u32>,
    columns: Vec<u16>,
    pub blocks: Vec<Block>,
    pub objects: Vec<StartupObject>,
    pub routes: Vec<Route>,
    /// Service-base table: police, hospitals, unused, unused, fire, unused.
    pub locations: [[BlockPos; LOCATIONS_PER_GROUP]; LOCATION_GROUPS],
    pub districts: Vec<District>,
}

impl Cmp {
    pub fn parse(data: &[u8]) -> Result<Self, FormatError> {
        let mut r = Reader::new(data, WHAT);
        let version = r.u32()?;
        if version != CMP_VERSION {
            return Err(FormatError::Version {
                what: WHAT,
                found: version,
                expected: CMP_VERSION,
            });
        }
        let [style_number, sample_number, _, _] = r.array::<4>()?;
        let route_size = r.len32()?;
        let object_size = r.len32()?;
        let column_size = r.len32()?;
        let block_size = r.len32()?;
        let nav_size = r.len32()?;

        let mut base_r = r.section(MAP_DIM * MAP_DIM * 4, "CMP base")?;
        let base = (0..MAP_DIM * MAP_DIM)
            .map(|_| base_r.u32())
            .collect::<Result<Vec<_>, _>>()?;

        if column_size % 2 != 0 {
            return Err(FormatError::invalid(
                WHAT,
                format!("column section of {column_size} bytes is not whole words"),
            ));
        }
        let mut col_r = r.section(column_size, "CMP columns")?;
        let columns = (0..column_size / 2)
            .map(|_| col_r.u16())
            .collect::<Result<Vec<_>, _>>()?;

        if block_size % BLOCK_RECORD != 0 {
            return Err(FormatError::invalid(
                WHAT,
                format!("block section of {block_size} bytes is not whole records"),
            ));
        }
        let mut blk_r = r.section(block_size, "CMP blocks")?;
        let blocks = (0..block_size / BLOCK_RECORD)
            .map(|_| -> Result<Block, FormatError> {
                Ok(Block {
                    type_map: blk_r.u16()?,
                    type_map_ext: blk_r.u8()?,
                    faces: blk_r.array()?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if object_size % OBJECT_RECORD != 0 {
            return Err(FormatError::invalid(
                WHAT,
                format!("object section of {object_size} bytes is not whole records"),
            ));
        }
        let mut obj_r = r.section(object_size, "CMP objects")?;
        let objects = (0..object_size / OBJECT_RECORD)
            .map(|_| -> Result<StartupObject, FormatError> {
                Ok(StartupObject {
                    x: obj_r.i16()?,
                    y: obj_r.i16()?,
                    z: obj_r.i16()?,
                    kind: obj_r.u8()?,
                    remap: obj_r.u8()?,
                    rotation: obj_r.i16()?,
                    pitch: obj_r.i16()?,
                    roll: obj_r.i16()?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut route_r = r.section(route_size, "CMP routes")?;
        let mut routes = Vec::new();
        while route_r.remaining() >= 2 {
            let count = route_r.u8()?;
            let kind = route_r.u8()?;
            let nodes = (0..count)
                .map(|_| read_pos(&mut route_r))
                .collect::<Result<Vec<_>, _>>()?;
            routes.push(Route { kind, nodes });
        }
        if route_r.remaining() != 0 {
            return Err(FormatError::invalid(WHAT, "route section ends mid-record"));
        }

        let mut locations = [[BlockPos { x: 0, y: 0, z: 0 }; LOCATIONS_PER_GROUP]; LOCATION_GROUPS];
        for group in &mut locations {
            for loc in group.iter_mut() {
                *loc = read_pos(&mut r)?;
            }
        }

        if nav_size % DISTRICT_RECORD != 0 {
            return Err(FormatError::invalid(
                WHAT,
                format!("navigation section of {nav_size} bytes is not whole records"),
            ));
        }
        let mut nav_r = r.section(nav_size, "CMP navigation")?;
        let districts = (0..nav_size / DISTRICT_RECORD)
            .map(|_| -> Result<District, FormatError> {
                let [x, y, w, h, sample] = nav_r.array()?;
                Ok(District {
                    x,
                    y,
                    w,
                    h,
                    sample,
                    name: latin1_cstr(nav_r.bytes(30)?),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if r.remaining() != 0 {
            return Err(FormatError::invalid(
                WHAT,
                format!(
                    "{} unexpected bytes after the navigation section",
                    r.remaining()
                ),
            ));
        }

        let cmp = Cmp {
            style_number,
            sample_number,
            base,
            columns,
            blocks,
            objects,
            routes,
            locations,
            districts,
        };
        cmp.validate()?;
        Ok(cmp)
    }

    /// Check every column is inside the column data and names real blocks,
    /// so lookups never fail later.
    fn validate(&self) -> Result<(), FormatError> {
        for (cell, &off) in self.base.iter().enumerate() {
            let (x, y) = (cell % MAP_DIM, cell / MAP_DIM);
            let bad = |d: String| Err(FormatError::invalid(WHAT, format!("column ({x},{y}): {d}")));
            if off % 2 != 0 {
                return bad(format!("odd offset {off}"));
            }
            let start = off as usize / 2;
            let Some(&empty) = self.columns.get(start) else {
                return bad(format!("offset {off} outside the column data"));
            };
            if empty as usize > MAP_LAYERS {
                return bad(format!("{empty} empty layers"));
            }
            let height = MAP_LAYERS - empty as usize;
            let Some(ids) = self.columns.get(start + 1..start + 1 + height) else {
                return bad("runs past the column data".into());
            };
            if let Some(id) = ids.iter().find(|&&id| id as usize >= self.blocks.len()) {
                return bad(format!("block {id} of {}", self.blocks.len()));
            }
        }
        Ok(())
    }

    fn column(&self, x: usize, y: usize) -> &[u16] {
        let start = self.base[y * MAP_DIM + x] as usize / 2;
        let height = MAP_LAYERS - self.columns[start] as usize;
        &self.columns[start..=start + height]
    }

    /// How many layers of the column at `(x, y)` hold a block, from the ground up.
    pub fn column_height(&self, x: usize, y: usize) -> usize {
        self.column(x, y).len() - 1
    }

    /// The block index at `(x, y, z)`, or `None` for air and out-of-range.
    pub fn block_index(&self, x: usize, y: usize, z: usize) -> Option<u16> {
        if x >= MAP_DIM || y >= MAP_DIM || z >= MAP_LAYERS {
            return None;
        }
        let col = self.column(x, y);
        let height = col.len() - 1;
        (z < height).then(|| col[height - z])
    }

    /// The block at `(x, y, z)`, or `None` for air and out-of-range.
    pub fn block(&self, x: usize, y: usize, z: usize) -> Option<&Block> {
        self.block_index(x, y, z).map(|i| &self.blocks[i as usize])
    }

    fn located(&self, group: usize) -> impl Iterator<Item = BlockPos> + '_ {
        self.locations[group]
            .iter()
            .copied()
            .filter(|p| (p.x, p.y, p.z) != (0, 0, 0))
    }
    pub fn police_stations(&self) -> impl Iterator<Item = BlockPos> + '_ {
        self.located(0)
    }
    pub fn hospitals(&self) -> impl Iterator<Item = BlockPos> + '_ {
        self.located(1)
    }
    pub fn fire_stations(&self) -> impl Iterator<Item = BlockPos> + '_ {
        self.located(4)
    }
}

/// Writes a small map in the CMP layout, for tests: give it the blocks of
/// a few columns and it writes the file [`Cmp::parse`] reads. Every other
/// column is empty.
#[derive(Debug, Clone, Default)]
pub struct CmpBuilder {
    /// `(x, y)` to the column's blocks, ground first (layer 0 upwards).
    columns: std::collections::BTreeMap<(u8, u8), Vec<Block>>,
    objects: Vec<StartupObject>,
    pub style_number: u8,
}

impl CmpBuilder {
    pub fn new(style_number: u8) -> Self {
        CmpBuilder {
            style_number,
            ..CmpBuilder::default()
        }
    }

    /// Put `block` at `(x, y, z)`; layers below it that are not set become
    /// empty blocks.
    pub fn set(&mut self, x: u8, y: u8, z: usize, block: Block) -> &mut Self {
        assert!(z < MAP_LAYERS);
        let col = self.columns.entry((x, y)).or_default();
        if col.len() <= z {
            col.resize(z + 1, Block::default());
        }
        col[z] = block;
        self
    }

    pub fn object(&mut self, o: StartupObject) -> &mut Self {
        self.objects.push(o);
        self
    }

    /// The file's bytes.
    pub fn build(&self) -> Vec<u8> {
        let mut blocks: Vec<Block> = Vec::new();
        let mut columns: Vec<u16> = vec![MAP_LAYERS as u16];
        let mut base = vec![0u32; MAP_DIM * MAP_DIM];
        for (&(x, y), col) in &self.columns {
            base[usize::from(y) * MAP_DIM + usize::from(x)] = (columns.len() * 2) as u32;
            columns.push((MAP_LAYERS - col.len()) as u16);
            // Block indices are stored top down.
            for b in col.iter().rev() {
                columns.push(blocks.len() as u16);
                blocks.push(*b);
            }
        }
        let mut f = Vec::new();
        f.extend(CMP_VERSION.to_le_bytes());
        f.extend([self.style_number, 1, 0, 0]);
        let sizes = [
            0u32,
            (self.objects.len() * OBJECT_RECORD) as u32,
            (columns.len() * 2) as u32,
            (blocks.len() * BLOCK_RECORD) as u32,
            0,
        ];
        for n in sizes {
            f.extend(n.to_le_bytes());
        }
        for b in base {
            f.extend(b.to_le_bytes());
        }
        for c in columns {
            f.extend(c.to_le_bytes());
        }
        for b in blocks {
            f.extend(b.type_map.to_le_bytes());
            f.push(b.type_map_ext);
            f.extend(b.faces);
        }
        for o in &self.objects {
            for v in [o.x, o.y, o.z] {
                f.extend(v.to_le_bytes());
            }
            f.extend([o.kind, o.remap]);
            for v in [o.rotation, o.pitch, o.roll] {
                f.extend(v.to_le_bytes());
            }
        }
        f.extend([0u8; LOCATION_GROUPS * LOCATIONS_PER_GROUP * 3]);
        f
    }
}

fn read_pos(r: &mut Reader<'_>) -> Result<BlockPos, FormatError> {
    let [x, y, z] = r.array()?;
    Ok(BlockPos { x, y, z })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal valid map: every column points at one shared column holding
    /// two blocks (ground and one above), plus one route, one district.
    fn tiny() -> Vec<u8> {
        let column: [u16; 3] = [4, 1, 0]; // 4 empty layers; top block 1, ground block 0
        let blocks: [[u8; 8]; 2] = [[0x20, 0x41, 0x88, 1, 2, 3, 4, 5], [0, 0, 0, 0, 0, 0, 0, 9]];
        let route: [u8; 8] = [2, 0xff, 10, 11, 4, 12, 13, 4];
        let mut district = vec![5, 6, 7, 8, 1];
        district.extend_from_slice(b"Downtown");
        district.resize(35, 0);

        let mut f = Vec::new();
        f.extend(CMP_VERSION.to_le_bytes());
        f.extend([1, 0, 0, 0]);
        for size in [
            route.len(),
            0,
            column.len() * 2,
            blocks.len() * 8,
            district.len(),
        ] {
            f.extend((size as u32).to_le_bytes());
        }
        for _ in 0..MAP_DIM * MAP_DIM {
            f.extend(0u32.to_le_bytes());
        }
        column.iter().for_each(|w| f.extend(w.to_le_bytes()));
        blocks.iter().for_each(|b| f.extend(b));
        f.extend(route);
        let mut locations = [0u8; 108];
        locations[..3].copy_from_slice(&[1, 2, 3]); // one police station
        f.extend(locations);
        f.extend(district);
        f
    }

    #[test]
    fn parses_a_tiny_map() {
        let m = Cmp::parse(&tiny()).unwrap();
        assert_eq!(m.style_number, 1);
        assert_eq!(m.column_height(17, 200), 2);
        let ground = m.block(0, 0, 0).unwrap();
        assert_eq!(ground.ground_type(), GroundType::Road);
        assert_eq!(ground.slope(), 0x01);
        assert_eq!(ground.lid_rotation(), 1);
        assert_eq!(ground.traffic_hint(), 0);
        assert_eq!(ground.remap(), 1);
        assert!(ground.is_railway());
        assert_eq!(ground.face(Face::Lid), 5);
        assert_eq!(m.block(0, 0, 1).unwrap().face(Face::Lid), 9);
        assert!(m.block(0, 0, 2).is_none());
        assert_eq!(
            m.routes,
            [Route {
                kind: 0xff,
                nodes: vec![
                    BlockPos { x: 10, y: 11, z: 4 },
                    BlockPos { x: 12, y: 13, z: 4 }
                ]
            }]
        );
        assert_eq!(
            m.police_stations().collect::<Vec<_>>(),
            [BlockPos { x: 1, y: 2, z: 3 }]
        );
        assert_eq!(m.hospitals().count(), 0);
        assert_eq!(m.districts[0].name, "Downtown");
    }

    #[test]
    fn rejects_bad_versions_truncation_and_bad_columns() {
        let mut f = tiny();
        f[0] = 0;
        assert!(matches!(Cmp::parse(&f), Err(FormatError::Version { .. })));

        let f = tiny();
        assert!(matches!(
            Cmp::parse(&f[..f.len() - 1]),
            Err(FormatError::Truncated { .. })
        ));

        let mut f = tiny();
        let col0 = 28 + MAP_DIM * MAP_DIM * 4;
        f[col0..col0 + 2].copy_from_slice(&3u16.to_le_bytes()); // 3 empty -> 3 blocks, runs off the end
        assert!(matches!(Cmp::parse(&f), Err(FormatError::Invalid { .. })));
    }

    #[test]
    fn builder_round_trip() {
        let road = Block {
            type_map: 0x20,
            ..Block::default()
        };
        let floor = Block {
            faces: [0, 0, 0, 0, 7],
            ..Block::default()
        };
        let mut b = CmpBuilder::new(1);
        b.set(10, 20, 1, floor).set(10, 20, 2, road);
        let m = Cmp::parse(&b.build()).unwrap();
        assert_eq!(m.style_number, 1);
        assert_eq!(m.column_height(10, 20), 3);
        assert_eq!(m.block(10, 20, 2), Some(&road));
        assert_eq!(m.block(10, 20, 1), Some(&floor));
        assert_eq!(m.block(10, 20, 0), Some(&Block::default()));
        assert_eq!(m.block(11, 20, 0), None);
    }
}
