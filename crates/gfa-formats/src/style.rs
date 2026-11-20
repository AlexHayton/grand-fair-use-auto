//! Style files: `.GRY` (version 325, 8-bit) and `.G24` (version 336, 24-bit).
//!
//! A style holds a city's art and its tables: 64×64 tiles, tile animations,
//! colour data, the object table, the vehicle table, sprites with their damage
//! and door deltas, and the sprite category counts.
//!
//! Sources: Carnage3D `src/StyleData.cpp` (G24, MIT, © 2019 jericho) and
//! AmiGTA1 `native/gta_style.c` (GRY, MIT). Implementation ours. The two
//! formats share every section except the header, the colour data and the
//! sprite record:
//!
//! | section | GRY | G24 |
//! |---|---|---|
//! | header | 13 `u32`, 52 bytes | 16 `u32`, 64 bytes |
//! | tiles | side, lid, aux; 256×256 pages of 4×4 tiles, padded to whole page rows | same |
//! | animations | `u8` count, then `tile which speed n frames[n]` | same |
//! | colour | 768-byte 6-bit palette, 256 remap tables, 256×4 lid remaps | paged BGRA CLUTs (rounded up to 64 KiB), then `u16` palette indices |
//! | objects | 20 bytes + `u16` × `num_into` | same |
//! | cars | 174 bytes + 8 per door | same |
//! | sprites | 10 bytes + 6 per delta | 12 bytes (adds a CLUT) + 6 per delta |
//! | sprite graphics | 256×256 pages | same |
//! | sprite numbers | 21 `u16` category counts | same |
//!
//! Every variable-length section must be consumed to the byte, and the section
//! sizes must account for the whole file. Those two checks are what catch a
//! wrong record layout, rather than a table of plausible nonsense.

use crate::reader::{FormatError, Reader};

pub const GRY_VERSION: u32 = 325;
pub const G24_VERSION: u32 = 336;
pub const TILE_DIM: usize = 64;
pub const TILE_AREA: usize = TILE_DIM * TILE_DIM;
/// Tiles and sprites are stored in square pages of this width.
pub const PAGE_DIM: usize = 256;
pub const PAGE_AREA: usize = PAGE_DIM * PAGE_DIM;
pub const CAR_REMAPS: usize = 12;
pub const CAR_DOORS: usize = 4;
/// G24: the remap CLUTs end with this many pedestrian remaps.
pub const PED_REMAPS: usize = 64;
const TILES_PER_ROW: usize = PAGE_DIM / TILE_DIM;
const CLUT_BYTES: usize = 256 * 4;
const CLUT_PAGE: usize = 64 * 1024;
const PALETTES_PER_CLUT_PAGE: usize = 64;
const OBJECT_RECORD: usize = 20;
const WHAT: &str = "style";

/// Which of the two style formats a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    Gry,
    G24,
}

/// The three tile banks, stored back to back in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileKind {
    Side = 0,
    Lid = 1,
    Aux = 2,
}

/// The 21 sprite categories, in file order. The sprite array is untagged: it
/// is these categories laid end to end, and [`Style::sprite_numbers`] is the
/// only thing that says where each one starts. Order from Carnage3D's
/// `eSpriteType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpriteCategory {
    Arrow,
    Digit,
    Boat,
    Box,
    Bus,
    Car,
    Object,
    Ped,
    Speedo,
    Tank,
    TrafficLight,
    Train,
    TrDoor,
    Bike,
    Tram,
    WreckedCar,
    WBus,
    Ex,
    TumCar,
    TumTruck,
    Ferry,
}

impl SpriteCategory {
    pub const COUNT: usize = 21;
    pub const ALL: [SpriteCategory; Self::COUNT] = [
        Self::Arrow,
        Self::Digit,
        Self::Boat,
        Self::Box,
        Self::Bus,
        Self::Car,
        Self::Object,
        Self::Ped,
        Self::Speedo,
        Self::Tank,
        Self::TrafficLight,
        Self::Train,
        Self::TrDoor,
        Self::Bike,
        Self::Tram,
        Self::WreckedCar,
        Self::WBus,
        Self::Ex,
        Self::TumCar,
        Self::TumTruck,
        Self::Ferry,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Arrow => "arrow",
            Self::Digit => "digit",
            Self::Boat => "boat",
            Self::Box => "box",
            Self::Bus => "bus",
            Self::Car => "car",
            Self::Object => "object",
            Self::Ped => "ped",
            Self::Speedo => "speedo",
            Self::Tank => "tank",
            Self::TrafficLight => "traffic_light",
            Self::Train => "train",
            Self::TrDoor => "trdoor",
            Self::Bike => "bike",
            Self::Tram => "tram",
            Self::WreckedCar => "wrecked_car",
            Self::WBus => "wbus",
            Self::Ex => "ex",
            Self::TumCar => "tumcar",
            Self::TumTruck => "tumtruck",
            Self::Ferry => "ferry",
        }
    }
}

/// A vehicle's class, from its `vtype` byte. The gaps in the numbering are
/// real: the files use 0–4, 8, 9, 13 and 14.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VehicleClass {
    Bus,
    JuggernautFront,
    JuggernautBack,
    Bike,
    Car,
    Train,
    Tram,
    Boat,
    Tank,
}

impl VehicleClass {
    pub fn from_vtype(vtype: u8) -> Option<Self> {
        Some(match vtype {
            0 => Self::Bus,
            1 => Self::JuggernautFront,
            2 => Self::JuggernautBack,
            3 => Self::Bike,
            4 => Self::Car,
            8 => Self::Train,
            9 => Self::Tram,
            13 => Self::Boat,
            14 => Self::Tank,
            _ => return None,
        })
    }

    /// The category a vehicle's `sprite` number counts within. Both halves of
    /// an articulated lorry are drawn from the car sprites.
    pub fn sprite_category(self) -> SpriteCategory {
        match self {
            Self::Bus => SpriteCategory::Bus,
            Self::JuggernautFront | Self::JuggernautBack | Self::Car => SpriteCategory::Car,
            Self::Bike => SpriteCategory::Bike,
            Self::Train => SpriteCategory::Train,
            Self::Tram => SpriteCategory::Tram,
            Self::Boat => SpriteCategory::Boat,
            Self::Tank => SpriteCategory::Tank,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Bus => "bus",
            Self::JuggernautFront => "jugg-front",
            Self::JuggernautBack => "jugg-back",
            Self::Bike => "bike",
            Self::Car => "car",
            Self::Train => "train",
            Self::Tram => "tram",
            Self::Boat => "boat",
            Self::Tank => "tank",
        }
    }
}

/// An animated tile: `tile` in bank `which` cycles through aux tiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileAnimation {
    pub tile: u8,
    /// Raw bank number; see [`TileAnimation::kind`].
    pub which: u8,
    /// Game frames per animation frame.
    pub speed: u8,
    /// Aux tile numbers.
    pub frames: Vec<u8>,
}

impl TileAnimation {
    pub fn kind(&self) -> Option<TileKind> {
        match self.which {
            0 => Some(TileKind::Side),
            1 => Some(TileKind::Lid),
            _ => None,
        }
    }
}

/// One entry of the object table: an object *type* (a bullet, a crate, a
/// barrier), not a placed object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectInfo {
    /// Size in source pixels, 64 to a block.
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    /// Sprite number within [`SpriteCategory::Object`].
    pub sprite: u16,
    pub weight: i16,
    pub aux: i16,
    /// Carnage3D's reading: 0 normal, 1 ignorable, 2 smashable, 3 invisible,
    /// 5 particle, 6 car object, 8 scenery, 9 power-up.
    pub status: u8,
    /// What the object breaks into. Neither reference uses it; kept raw.
    pub into: Vec<u16>,
}

/// A door on a vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CarDoor {
    /// Hinge position relative to the vehicle's centre.
    pub rpy: i16,
    pub rpx: i16,
    pub object: i16,
    /// The sprite delta that draws this door open.
    pub delta: i16,
}

/// One vehicle model from the car table, exactly as stored.
///
/// The physics values are 16.16 fixed point in the file and are kept as raw
/// `i32`, which is the simulation's own format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarInfo {
    /// Footprint in source pixels, 64 to a block. AmiGTA1 established that
    /// the second field runs along the vehicle (it equals the sprite height)
    /// and the third is how tall it stands; Carnage3D's names suggest
    /// otherwise.
    pub width: i16,
    pub length: i16,
    pub vert: i16,
    /// Sprite number within the class's [`SpriteCategory`].
    pub sprite: i16,
    pub weight: i16,
    pub max_speed: i16,
    pub min_speed: i16,
    pub accel: i16,
    pub braking: i16,
    pub grip: i16,
    pub handling: i16,
    /// 24-bit recolours as (hue, lightness, saturation).
    pub hls_remaps: [[i16; 3]; CAR_REMAPS],
    /// 8-bit recolours: GRY remap table numbers.
    pub remap8: [u8; CAR_REMAPS],
    pub vtype: u8,
    pub model: u8,
    pub turning: i8,
    pub damagable: i8,
    pub value: [i16; 4],
    /// Centre of mass, pixels.
    pub cx: i8,
    pub cy: i8,
    pub moment: i32,
    pub mass: i32,
    pub thrust: i32,
    pub tyre_adhesion_x: i32,
    pub tyre_adhesion_y: i32,
    pub handbrake_friction: i32,
    pub footbrake_friction: i32,
    pub front_brake_bias: i32,
    pub turn_ratio: i16,
    pub drive_wheel_offset: i16,
    pub steering_wheel_offset: i16,
    pub back_end_slide: i32,
    pub handbrake_slide: i32,
    /// Bit 0: convertible. Bit 1: extra driving animation.
    pub convertible: u8,
    pub engine: u8,
    pub radio: u8,
    pub horn: u8,
    pub sound_function: u8,
    pub fast_change: u8,
    pub doors: Vec<CarDoor>,
}

impl CarInfo {
    pub fn class(&self) -> Option<VehicleClass> {
        VehicleClass::from_vtype(self.vtype)
    }
}

/// A delta: a small overlay patched onto a sprite for damage or an open door.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteDelta {
    /// Bytes of run stream.
    pub size: u16,
    /// Byte offset of the run stream in the sprite graphics.
    pub offset: u32,
}

/// One run of a decoded delta, at sprite-local coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaRun<'a> {
    pub x: usize,
    pub y: usize,
    pub pixels: &'a [u8],
}

/// One sprite record. A sprite is an arbitrary rectangle inside a 256×256
/// page of the sprite graphics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub width: u8,
    pub height: u8,
    /// The byte after the delta count. Unused by both references.
    pub pad: u8,
    /// Bytes per frame; always `width * height`.
    pub size: u16,
    /// G24 only: the sprite's CLUT number.
    pub clut: Option<u16>,
    pub page_x: u8,
    pub page_y: u8,
    pub page: u16,
    pub deltas: Vec<SpriteDelta>,
}

/// GRY colour data: one global palette and 8-bit remap tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GryColour {
    /// The palette as stored: VGA DAC values, 0..=63.
    pub palette: [[u8; 3]; 256],
    /// Remap table `n` maps every palette index to another. Vehicles use
    /// tables 1..=43 and pedestrians 128..=187 (AmiGTA1's reading).
    pub remaps: Vec<[u8; 256]>,
    /// Per lid tile, the remap table for each of the map's four lid remaps.
    /// Inferred: in every style, entry 3 equals entry 0 for almost every lid,
    /// which is the same pattern as the G24 palette indices for lids.
    pub lid_remaps: Vec<[u8; 4]>,
}

impl GryColour {
    /// Palette entry `i` scaled to 8 bits. The top bits are replicated so 63
    /// becomes 255.
    pub fn rgb8(&self, i: u8) -> [u8; 3] {
        self.palette[i as usize].map(|v| (v << 2) | (v >> 4))
    }
}

/// G24 colour data: many 256-colour palettes and an index that maps each
/// tile, sprite, remap and font to one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct G24Colour {
    /// Physical palettes, de-paged, as `[r, g, b, a]`. The file stores them
    /// BGRA, 64 palettes interleaved per 64 KiB page.
    pub cluts: Vec<[[u8; 4]; 256]>,
    /// Logical CLUT number to physical palette. Laid out as tile CLUTs (four
    /// per tile), sprite CLUTs, remap CLUTs, then font CLUTs.
    pub palette_index: Vec<u16>,
    pub tile_cluts: usize,
    pub sprite_cluts: usize,
    pub remap_cluts: usize,
    pub font_cluts: usize,
}

impl G24Colour {
    /// The physical palette for a tile (`linear` across all banks) under one
    /// of the map's lid remaps, 0..=3.
    pub fn tile_palette(&self, linear: usize, remap: u8) -> Option<u16> {
        if remap > 3 {
            return None;
        }
        self.palette_index.get(4 * linear + remap as usize).copied()
    }

    pub fn sprite_palette(&self, clut: u16) -> Option<u16> {
        self.index_in(self.tile_cluts, self.sprite_cluts, clut as usize)
    }

    /// The physical palette for remap CLUT `remap`. Car `n`'s remaps are
    /// `n * 12 ..`; the last [`PED_REMAPS`] are the pedestrians'.
    pub fn remap_palette(&self, remap: usize) -> Option<u16> {
        self.index_in(self.tile_cluts + self.sprite_cluts, self.remap_cluts, remap)
    }

    pub fn font_palette(&self, font: usize) -> Option<u16> {
        let base = self.tile_cluts + self.sprite_cluts + self.remap_cluts;
        self.index_in(base, self.font_cluts, font)
    }

    fn index_in(&self, base: usize, count: usize, i: usize) -> Option<u16> {
        (i < count)
            .then(|| self.palette_index.get(base + i).copied())
            .flatten()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Colour {
    Gry(Box<GryColour>),
    G24(G24Colour),
}

/// A rectangle of palette indices inside a paged buffer.
#[derive(Debug, Clone, Copy)]
pub struct PagedImage<'a> {
    data: &'a [u8],
    pub width: usize,
    pub height: usize,
}

impl<'a> PagedImage<'a> {
    pub fn row(&self, y: usize) -> &'a [u8] {
        let start = y * PAGE_DIM;
        &self.data[start..start + self.width]
    }

    pub fn pixel(&self, x: usize, y: usize) -> u8 {
        self.row(y)[x]
    }

    /// The pixels packed row by row, `width * height` bytes.
    pub fn to_vec(&self) -> Vec<u8> {
        (0..self.height)
            .flat_map(|y| self.row(y))
            .copied()
            .collect()
    }
}

/// A parsed style file.
#[derive(Debug, Clone)]
pub struct Style {
    pub kind: StyleKind,
    pub side_tiles: usize,
    pub lid_tiles: usize,
    pub aux_tiles: usize,
    tiles: Vec<u8>,
    pub animations: Vec<TileAnimation>,
    /// The animation count the file declares; see [`Style::animations`].
    pub animations_declared: u8,
    pub colour: Colour,
    pub objects: Vec<ObjectInfo>,
    pub cars: Vec<CarInfo>,
    pub sprites: Vec<Sprite>,
    sprite_graphics: Vec<u8>,
    /// Sprites per category, in [`SpriteCategory::ALL`] order.
    pub sprite_numbers: [u16; SpriteCategory::COUNT],
}

struct Sizes {
    side: usize,
    lid: usize,
    aux: usize,
    anim: usize,
    objects: usize,
    cars: usize,
    sprite_info: usize,
    sprite_graphics: usize,
    sprite_numbers: usize,
}

impl Style {
    pub fn parse(data: &[u8]) -> Result<Self, FormatError> {
        let mut r = Reader::new(data, WHAT);
        match r.u32()? {
            GRY_VERSION => Self::parse_gry(r, data.len()),
            G24_VERSION => Self::parse_g24(r, data.len()),
            found => Err(FormatError::invalid(
                WHAT,
                format!("version {found}, expected {GRY_VERSION} (GRY) or {G24_VERSION} (G24)"),
            )),
        }
    }

    fn parse_gry(mut r: Reader<'_>, file_len: usize) -> Result<Self, FormatError> {
        let side = r.len32()?;
        let lid = r.len32()?;
        let aux = r.len32()?;
        let anim = r.len32()?;
        let palette = r.len32()?;
        let remaps = r.len32()?;
        let lid_remaps = r.len32()?;
        let sizes = Sizes {
            side,
            lid,
            aux,
            anim,
            objects: r.len32()?,
            cars: r.len32()?,
            sprite_info: r.len32()?,
            sprite_graphics: r.len32()?,
            sprite_numbers: r.len32()?,
        };
        let tiles_len = padded_tiles_len(&sizes)?;
        check_total(
            file_len,
            &[52, tiles_len, palette, remaps, lid_remaps],
            &sizes,
        )?;
        if palette != 768 {
            return Err(FormatError::invalid(
                WHAT,
                format!("GRY palette is {palette} bytes, expected 768"),
            ));
        }
        if remaps % 256 != 0 || lid_remaps % 4 != 0 {
            return Err(FormatError::invalid(
                WHAT,
                format!(
                    "GRY remap sections of {remaps} and {lid_remaps} bytes are not whole tables"
                ),
            ));
        }

        let tiles = r.bytes(tiles_len)?.to_vec();
        let animations = parse_animations(r.section(sizes.anim, "style animations")?)?;
        let mut pal = [[0u8; 3]; 256];
        for entry in &mut pal {
            *entry = r.array()?;
        }
        if let Some(v) = pal.iter().flatten().find(|&&v| v > 63) {
            return Err(FormatError::invalid(
                WHAT,
                format!("GRY palette value {v} is not 6-bit"),
            ));
        }
        let remaps = r.bytes(remaps)?.as_chunks::<256>().0.to_vec();
        let lid_remaps = r.bytes(lid_remaps)?.as_chunks::<4>().0.to_vec();
        let colour = Colour::Gry(Box::new(GryColour {
            palette: pal,
            remaps,
            lid_remaps,
        }));
        Self::parse_tail(r, sizes, StyleKind::Gry, tiles, animations, colour)
    }

    fn parse_g24(mut r: Reader<'_>, file_len: usize) -> Result<Self, FormatError> {
        let side = r.len32()?;
        let lid = r.len32()?;
        let aux = r.len32()?;
        let anim = r.len32()?;
        let clut = r.len32()?;
        let tile_clut = r.len32()?;
        let sprite_clut = r.len32()?;
        let remap_clut = r.len32()?;
        let font_clut = r.len32()?;
        let palette_index = r.len32()?;
        let sizes = Sizes {
            side,
            lid,
            aux,
            anim,
            objects: r.len32()?,
            cars: r.len32()?,
            sprite_info: r.len32()?,
            sprite_graphics: r.len32()?,
            sprite_numbers: r.len32()?,
        };
        let tiles_len = padded_tiles_len(&sizes)?;
        // The CLUT pages are stored whole even when the last is part-used.
        let clut_stored = clut.div_ceil(CLUT_PAGE) * CLUT_PAGE;
        check_total(
            file_len,
            &[64, tiles_len, clut_stored, palette_index],
            &sizes,
        )?;
        for (what, n) in [
            ("CLUT data", clut),
            ("tile CLUTs", tile_clut),
            ("sprite CLUTs", sprite_clut),
            ("remap CLUTs", remap_clut),
            ("font CLUTs", font_clut),
        ] {
            if n % CLUT_BYTES != 0 {
                return Err(FormatError::invalid(
                    WHAT,
                    format!("G24 {what} of {n} bytes is not whole palettes"),
                ));
            }
        }
        if palette_index % 2 != 0 {
            return Err(FormatError::invalid(
                WHAT,
                "G24 palette index has an odd length",
            ));
        }

        let tiles = r.bytes(tiles_len)?.to_vec();
        let animations = parse_animations(r.section(sizes.anim, "style animations")?)?;
        let cluts = depage_cluts(r.bytes(clut_stored)?, clut / CLUT_BYTES);
        let palette_index: Vec<u16> = r
            .bytes(palette_index)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect();
        if let Some(&bad) = palette_index.iter().find(|&&p| p as usize >= cluts.len()) {
            return Err(FormatError::invalid(
                WHAT,
                format!(
                    "G24 palette index {bad} is past the {} palettes",
                    cluts.len()
                ),
            ));
        }
        let colour = G24Colour {
            cluts,
            palette_index,
            tile_cluts: tile_clut / CLUT_BYTES,
            sprite_cluts: sprite_clut / CLUT_BYTES,
            remap_cluts: remap_clut / CLUT_BYTES,
            font_cluts: font_clut / CLUT_BYTES,
        };
        let logical =
            colour.tile_cluts + colour.sprite_cluts + colour.remap_cluts + colour.font_cluts;
        if logical != colour.palette_index.len() {
            return Err(FormatError::invalid(
                WHAT,
                format!(
                    "G24 CLUT counts sum to {logical} but the palette index has {}",
                    colour.palette_index.len()
                ),
            ));
        }
        Self::parse_tail(
            r,
            sizes,
            StyleKind::G24,
            tiles,
            animations,
            Colour::G24(colour),
        )
    }

    /// The sections after the colour data, which both formats share.
    fn parse_tail(
        mut r: Reader<'_>,
        sizes: Sizes,
        kind: StyleKind,
        tiles: Vec<u8>,
        (animations, animations_declared): (Vec<TileAnimation>, u8),
        colour: Colour,
    ) -> Result<Self, FormatError> {
        let objects = parse_objects(r.section(sizes.objects, "style objects")?)?;
        let cars = parse_cars(r.section(sizes.cars, "style cars")?)?;
        let sprites = parse_sprites(r.section(sizes.sprite_info, "style sprites")?, kind)?;
        let sprite_graphics = r.bytes(sizes.sprite_graphics)?.to_vec();
        let mut numbers = r.section(sizes.sprite_numbers, "style sprite numbers")?;
        let mut sprite_numbers = [0u16; SpriteCategory::COUNT];
        for n in &mut sprite_numbers {
            *n = numbers.u16()?;
        }
        numbers.expect_end()?;
        r.expect_end()?;

        let style = Self {
            kind,
            side_tiles: sizes.side / TILE_AREA,
            lid_tiles: sizes.lid / TILE_AREA,
            aux_tiles: sizes.aux / TILE_AREA,
            tiles,
            animations,
            animations_declared,
            colour,
            objects,
            cars,
            sprites,
            sprite_graphics,
            sprite_numbers,
        };
        style.validate()?;
        Ok(style)
    }

    fn validate(&self) -> Result<(), FormatError> {
        let total: usize = self.sprite_numbers.iter().map(|&n| n as usize).sum();
        if total != self.sprites.len() {
            return Err(FormatError::invalid(
                WHAT,
                format!(
                    "sprite numbers sum to {total} but there are {} sprites",
                    self.sprites.len()
                ),
            ));
        }
        for (i, s) in self.sprites.iter().enumerate() {
            if self.sprite(i).is_none() {
                return Err(FormatError::invalid(
                    WHAT,
                    format!(
                        "sprite {i} ({}x{} at page {} {},{}) is outside the sprite graphics",
                        s.width, s.height, s.page, s.page_x, s.page_y
                    ),
                ));
            }
            for (d, delta) in s.deltas.iter().enumerate() {
                if let Err(e) = self.delta_runs(i, *delta) {
                    return Err(FormatError::invalid(
                        WHAT,
                        format!("sprite {i} delta {d}: {e}"),
                    ));
                }
            }
        }
        for a in &self.animations {
            if a.kind().is_none() {
                return Err(FormatError::invalid(
                    WHAT,
                    format!("animation of tile {} has bank {}", a.tile, a.which),
                ));
            }
            if let Some(&f) = a.frames.iter().find(|&&f| f as usize >= self.aux_tiles) {
                return Err(FormatError::invalid(
                    WHAT,
                    format!(
                        "animation of tile {} uses aux tile {f} of {}",
                        a.tile, self.aux_tiles
                    ),
                ));
            }
        }
        Ok(())
    }

    pub fn tile_count(&self, kind: TileKind) -> usize {
        match kind {
            TileKind::Side => self.side_tiles,
            TileKind::Lid => self.lid_tiles,
            TileKind::Aux => self.aux_tiles,
        }
    }

    /// Tiles across all three banks, without the page padding.
    pub fn tile_total(&self) -> usize {
        self.side_tiles + self.lid_tiles + self.aux_tiles
    }

    /// The position of a tile across all three banks.
    pub fn tile_linear(&self, kind: TileKind, index: usize) -> Option<usize> {
        (index < self.tile_count(kind)).then(|| match kind {
            TileKind::Side => index,
            TileKind::Lid => self.side_tiles + index,
            TileKind::Aux => self.side_tiles + self.lid_tiles + index,
        })
    }

    /// The 64×64 palette indices of tile `linear`.
    pub fn tile(&self, linear: usize) -> Option<PagedImage<'_>> {
        if linear >= self.tile_total() {
            return None;
        }
        let page = linear / (TILES_PER_ROW * TILES_PER_ROW);
        let in_page = linear % (TILES_PER_ROW * TILES_PER_ROW);
        let start = page * PAGE_AREA
            + (in_page / TILES_PER_ROW) * TILE_DIM * PAGE_DIM
            + (in_page % TILES_PER_ROW) * TILE_DIM;
        paged(&self.tiles, start, TILE_DIM, TILE_DIM)
    }

    /// The palette indices of sprite `index`. Index 0 is transparent.
    pub fn sprite(&self, index: usize) -> Option<PagedImage<'_>> {
        let s = self.sprites.get(index)?;
        let start = s.page as usize * PAGE_AREA + s.page_y as usize * PAGE_DIM + s.page_x as usize;
        paged(
            &self.sprite_graphics,
            start,
            s.width as usize,
            s.height as usize,
        )
    }

    /// Decodes one of a sprite's deltas into runs at sprite-local positions.
    ///
    /// The stream is `u16 skip, u8 len, len pixels`, repeated. The cursor
    /// advances by `skip` and then by `len`, in units of the 256-pixel page
    /// width, not the sprite's own width. Getting that wrong decodes without
    /// error and smears the overlay diagonally (AmiGTA1's finding).
    pub fn delta_runs(
        &self,
        sprite: usize,
        delta: SpriteDelta,
    ) -> Result<Vec<DeltaRun<'_>>, String> {
        let s = self.sprites.get(sprite).ok_or("no such sprite")?;
        let start = delta.offset as usize;
        let stream = start
            .checked_add(delta.size as usize)
            .and_then(|end| self.sprite_graphics.get(start..end))
            .ok_or_else(|| {
                format!(
                    "{} bytes at {start} are outside the sprite graphics",
                    delta.size
                )
            })?;
        let mut runs = Vec::new();
        let mut cursor = 0usize;
        let mut pos = 0usize;
        while pos < stream.len() {
            let head = stream.get(pos..pos + 3).ok_or("run header is cut off")?;
            let skip = u16::from_le_bytes([head[0], head[1]]) as usize;
            let len = head[2] as usize;
            pos += 3;
            let pixels = stream.get(pos..pos + len).ok_or("run pixels are cut off")?;
            pos += len;
            cursor += skip;
            let (x, y) = (cursor % PAGE_DIM, cursor / PAGE_DIM);
            if x + len > s.width as usize || y >= s.height as usize {
                return Err(format!(
                    "run of {len} at {x},{y} is outside the {}x{} sprite",
                    s.width, s.height
                ));
            }
            runs.push(DeltaRun { x, y, pixels });
            cursor += len;
        }
        Ok(runs)
    }

    /// Patches delta `delta` of sprite `sprite` onto `dst`, a packed
    /// `width * height` copy of that sprite.
    pub fn apply_delta(&self, sprite: usize, delta: usize, dst: &mut [u8]) -> Result<(), String> {
        let s = self.sprites.get(sprite).ok_or("no such sprite")?;
        let d = *s.deltas.get(delta).ok_or("no such delta")?;
        let w = s.width as usize;
        if dst.len() != w * s.height as usize {
            return Err("destination is not the sprite's size".into());
        }
        for run in self.delta_runs(sprite, d)? {
            let at = run.y * w + run.x;
            dst[at..at + run.pixels.len()].copy_from_slice(run.pixels);
        }
        Ok(())
    }

    /// The first sprite of a category.
    pub fn sprite_base(&self, category: SpriteCategory) -> usize {
        self.sprite_numbers[..category as usize]
            .iter()
            .map(|&n| n as usize)
            .sum()
    }

    pub fn sprite_count(&self, category: SpriteCategory) -> usize {
        self.sprite_numbers[category as usize] as usize
    }

    /// The absolute sprite index of sprite `n` of a category.
    pub fn sprite_index(&self, category: SpriteCategory, n: usize) -> Option<usize> {
        (n < self.sprite_count(category)).then(|| self.sprite_base(category) + n)
    }

    pub fn car_sprite(&self, car: &CarInfo) -> Option<usize> {
        let n = usize::try_from(car.sprite).ok()?;
        self.sprite_index(car.class()?.sprite_category(), n)
    }

    pub fn object_sprite(&self, object: &ObjectInfo) -> Option<usize> {
        self.sprite_index(SpriteCategory::Object, object.sprite as usize)
    }
}

fn paged(data: &[u8], start: usize, width: usize, height: usize) -> Option<PagedImage<'_>> {
    if width == 0 || height == 0 || width > PAGE_DIM || start % PAGE_DIM + width > PAGE_DIM {
        return None;
    }
    let end = start + (height - 1) * PAGE_DIM + width;
    Some(PagedImage {
        data: data.get(start..end)?,
        width,
        height,
    })
}

/// Tile bytes including the padding that completes the last page row. The
/// header's sizes leave the padding out.
fn padded_tiles_len(s: &Sizes) -> Result<usize, FormatError> {
    for (what, n) in [("side", s.side), ("lid", s.lid), ("aux", s.aux)] {
        if n % TILE_AREA != 0 {
            return Err(FormatError::invalid(
                WHAT,
                format!("{what} tiles of {n} bytes are not whole tiles"),
            ));
        }
    }
    let tiles = (s.side + s.lid + s.aux) / TILE_AREA;
    Ok(tiles.next_multiple_of(TILES_PER_ROW) * TILE_AREA)
}

fn check_total(file_len: usize, head: &[usize], s: &Sizes) -> Result<(), FormatError> {
    let tail = [
        s.anim,
        s.objects,
        s.cars,
        s.sprite_info,
        s.sprite_graphics,
        s.sprite_numbers,
    ];
    let total = head
        .iter()
        .chain(&tail)
        .try_fold(0usize, |acc, &n| acc.checked_add(n));
    if total != Some(file_len) {
        return Err(FormatError::invalid(
            WHAT,
            format!("sections sum to {total:?} bytes but the file is {file_len}"),
        ));
    }
    Ok(())
}

/// Returns the records and the count the section declares. They can differ:
/// STYLE003.GRY declares 3 but holds 2, which end exactly at the section's
/// end, so a walk stops cleanly at the boundary.
fn parse_animations(mut r: Reader<'_>) -> Result<(Vec<TileAnimation>, u8), FormatError> {
    if r.is_empty() {
        return Ok((Vec::new(), 0));
    }
    let count = r.u8()?;
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if r.is_empty() {
            break;
        }
        let tile = r.u8()?;
        let which = r.u8()?;
        let speed = r.u8()?;
        let n = r.u8()?;
        let frames = r.bytes(n as usize)?.to_vec();
        out.push(TileAnimation {
            tile,
            which,
            speed,
            frames,
        });
    }
    r.expect_end()?;
    Ok((out, count))
}

/// The CLUT pages hold 64 palettes each: 256 rows, one per colour, and each
/// row is that colour in all 64 palettes, as BGRA.
fn depage_cluts(data: &[u8], count: usize) -> Vec<[[u8; 4]; 256]> {
    let mut out = vec![[[0u8; 4]; 256]; count];
    for (p, pal) in out.iter_mut().enumerate() {
        let page = p / PALETTES_PER_CLUT_PAGE;
        let column = p % PALETTES_PER_CLUT_PAGE;
        for (entry, colour) in pal.iter_mut().enumerate() {
            let at = page * CLUT_PAGE + (entry * PALETTES_PER_CLUT_PAGE + column) * 4;
            let [b, g, r, a] = data[at..at + 4].try_into().expect("4 bytes");
            *colour = [r, g, b, a];
        }
    }
    out
}

fn parse_objects(mut r: Reader<'_>) -> Result<Vec<ObjectInfo>, FormatError> {
    let mut out = Vec::new();
    while r.remaining() >= OBJECT_RECORD {
        let width = r.i32()?;
        let height = r.i32()?;
        let depth = r.i32()?;
        let sprite = r.u16()?;
        let weight = r.i16()?;
        let aux = r.i16()?;
        let status = r.u8()?;
        let n = r.u8()?;
        let into = (0..n).map(|_| r.u16()).collect::<Result<_, _>>()?;
        out.push(ObjectInfo {
            width,
            height,
            depth,
            sprite,
            weight,
            aux,
            status,
            into,
        });
    }
    r.expect_end()?;
    Ok(out)
}

fn parse_cars(mut r: Reader<'_>) -> Result<Vec<CarInfo>, FormatError> {
    let mut out = Vec::new();
    while !r.is_empty() {
        let width = r.i16()?;
        let length = r.i16()?;
        let vert = r.i16()?;
        let sprite = r.i16()?;
        let weight = r.i16()?;
        let max_speed = r.i16()?;
        let min_speed = r.i16()?;
        let accel = r.i16()?;
        let braking = r.i16()?;
        let grip = r.i16()?;
        let handling = r.i16()?;
        let mut hls_remaps = [[0i16; 3]; CAR_REMAPS];
        for hls in &mut hls_remaps {
            for v in hls.iter_mut() {
                *v = r.i16()?;
            }
        }
        let remap8 = r.array()?;
        let vtype = r.u8()?;
        let model = r.u8()?;
        let turning = r.i8()?;
        let damagable = r.i8()?;
        let mut value = [0i16; 4];
        for v in &mut value {
            *v = r.i16()?;
        }
        let cx = r.i8()?;
        let cy = r.i8()?;
        let moment = r.i32()?;
        let mass = r.i32()?;
        let thrust = r.i32()?;
        let tyre_adhesion_x = r.i32()?;
        let tyre_adhesion_y = r.i32()?;
        let handbrake_friction = r.i32()?;
        let footbrake_friction = r.i32()?;
        let front_brake_bias = r.i32()?;
        let turn_ratio = r.i16()?;
        let drive_wheel_offset = r.i16()?;
        let steering_wheel_offset = r.i16()?;
        let back_end_slide = r.i32()?;
        let handbrake_slide = r.i32()?;
        let convertible = r.u8()?;
        let engine = r.u8()?;
        let radio = r.u8()?;
        let horn = r.u8()?;
        let sound_function = r.u8()?;
        let fast_change = r.u8()?;
        let n_doors = r.i16()?;
        if !(0..=CAR_DOORS as i16).contains(&n_doors) {
            return Err(FormatError::invalid(
                "style cars",
                format!("car {} claims {n_doors} doors", out.len()),
            ));
        }
        let doors = (0..n_doors)
            .map(|_| {
                Ok(CarDoor {
                    rpy: r.i16()?,
                    rpx: r.i16()?,
                    object: r.i16()?,
                    delta: r.i16()?,
                })
            })
            .collect::<Result<_, FormatError>>()?;
        out.push(CarInfo {
            width,
            length,
            vert,
            sprite,
            weight,
            max_speed,
            min_speed,
            accel,
            braking,
            grip,
            handling,
            hls_remaps,
            remap8,
            vtype,
            model,
            turning,
            damagable,
            value,
            cx,
            cy,
            moment,
            mass,
            thrust,
            tyre_adhesion_x,
            tyre_adhesion_y,
            handbrake_friction,
            footbrake_friction,
            front_brake_bias,
            turn_ratio,
            drive_wheel_offset,
            steering_wheel_offset,
            back_end_slide,
            handbrake_slide,
            convertible,
            engine,
            radio,
            horn,
            sound_function,
            fast_change,
            doors,
        });
    }
    Ok(out)
}

fn parse_sprites(mut r: Reader<'_>, kind: StyleKind) -> Result<Vec<Sprite>, FormatError> {
    let mut out = Vec::new();
    while !r.is_empty() {
        let width = r.u8()?;
        let height = r.u8()?;
        let n_deltas = r.u8()?;
        let pad = r.u8()?;
        let size = r.u16()?;
        let clut = match kind {
            StyleKind::G24 => Some(r.u16()?),
            StyleKind::Gry => None,
        };
        let page_x = r.u8()?;
        let page_y = r.u8()?;
        let page = r.u16()?;
        // size == w * h is the invariant that identifies a good record, and
        // turns a desynchronised walk into an error.
        if size as usize != width as usize * height as usize {
            return Err(FormatError::invalid(
                "style sprites",
                format!(
                    "sprite {} is {width}x{height} but has size {size}",
                    out.len()
                ),
            ));
        }
        let deltas = (0..n_deltas)
            .map(|_| {
                Ok(SpriteDelta {
                    size: r.u16()?,
                    offset: r.u32()?,
                })
            })
            .collect::<Result<_, FormatError>>()?;
        out.push(Sprite {
            width,
            height,
            pad,
            size,
            clut,
            page_x,
            page_y,
            page,
            deltas,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal GRY: one side, one lid and one aux tile (padded to four),
    /// one animation, no objects or cars, and two sprites, one with a delta.
    fn tiny_gry() -> Vec<u8> {
        let tiles = 4 * TILE_AREA;
        let mut tile_data = vec![0u8; tiles];
        // Tile 1 (the lid) starts 64 pixels into the first page row.
        tile_data[TILE_DIM] = 7;
        let anim = [1u8, 0, 1, 5, 1, 0];
        let mut sprite_info = Vec::new();
        // Sprite 0: 2x2 at (0,0) page 0, no deltas.
        sprite_info.extend_from_slice(&[2, 2, 0, 0, 4, 0, 0, 0, 0, 0]);
        // Sprite 1: 3x2 at (4,0) page 0, one delta of 5 bytes at offset 600.
        sprite_info.extend_from_slice(&[3, 2, 1, 0, 6, 0, 4, 0, 0, 0]);
        sprite_info.extend_from_slice(&[5, 0, 600u32 as u8, (600u32 >> 8) as u8, 0, 0]);
        let mut graphics = vec![0u8; 1024];
        graphics[0] = 9;
        graphics[4] = 3;
        // Delta: skip 257 (to x=1, y=1), 2 pixels.
        graphics[600..605].copy_from_slice(&[1, 1, 2, 11, 12]);
        let mut numbers = [0u16; SpriteCategory::COUNT];
        numbers[SpriteCategory::Car as usize] = 2;

        let mut out = Vec::new();
        let header = [
            GRY_VERSION as usize,
            TILE_AREA,
            TILE_AREA,
            TILE_AREA,
            anim.len(),
            768,
            256,
            4,
            0,
            0,
            sprite_info.len(),
            graphics.len(),
            42,
        ];
        for h in header {
            out.extend_from_slice(&(h as u32).to_le_bytes());
        }
        out.extend_from_slice(&tile_data);
        out.extend_from_slice(&anim);
        out.extend_from_slice(&[63u8; 768]);
        out.extend((0..=255u8).collect::<Vec<_>>());
        out.extend_from_slice(&[0, 1, 2, 0]);
        out.extend_from_slice(&sprite_info);
        out.extend_from_slice(&graphics);
        for n in numbers {
            out.extend_from_slice(&n.to_le_bytes());
        }
        out
    }

    #[test]
    fn parses_a_tiny_gry() {
        let s = Style::parse(&tiny_gry()).unwrap();
        assert_eq!(s.kind, StyleKind::Gry);
        assert_eq!((s.side_tiles, s.lid_tiles, s.aux_tiles), (1, 1, 1));
        assert_eq!(s.tile_linear(TileKind::Lid, 0), Some(1));
        assert_eq!(s.tile(1).unwrap().pixel(0, 0), 7);
        assert!(s.tile(3).is_none(), "padding is not a tile");
        assert_eq!(s.animations[0].frames, vec![0]);
        assert_eq!(s.sprites.len(), 2);
        assert_eq!(s.sprite(0).unwrap().to_vec(), vec![9, 0, 0, 0]);
        assert_eq!(s.sprite_index(SpriteCategory::Car, 1), Some(1));
        assert_eq!(s.sprite_index(SpriteCategory::Car, 2), None);

        let mut px = s.sprite(1).unwrap().to_vec();
        assert_eq!(px, vec![3, 0, 0, 0, 0, 0]);
        s.apply_delta(1, 0, &mut px).unwrap();
        assert_eq!(px, vec![3, 0, 0, 0, 11, 12]);

        let Colour::Gry(c) = &s.colour else {
            panic!("not GRY")
        };
        assert_eq!(c.rgb8(0), [255, 255, 255]);
        assert_eq!(c.lid_remaps, vec![[0, 1, 2, 0]]);
    }

    #[test]
    fn rejects_a_wrong_length() {
        let mut data = tiny_gry();
        data.push(0);
        assert!(Style::parse(&data).is_err());
    }

    #[test]
    fn rejects_a_delta_outside_its_sprite() {
        let mut data = tiny_gry();
        // Point the delta's skip past the sprite's two rows.
        let at = data.len() - 42 - 1024 + 600;
        data[at + 1] = 2;
        assert!(Style::parse(&data).is_err());
    }
}
