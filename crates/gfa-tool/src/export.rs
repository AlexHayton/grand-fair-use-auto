//! `gfa-tool export`: contact sheets for reviewing a file's art by eye.

use std::path::Path;

use anyhow::{Context, Result};
use gfa_formats::style::{
    CAR_REMAPS, Colour, PED_REMAPS, SpriteCategory, Style, TILE_DIM, TileKind,
};

use crate::sheet::Sheet;

type Lut = [[u8; 4]; 256];

/// Writes one front-end or briefing picture as `<stem>.png`.
pub fn screen(img: &gfa_formats::screen::Image<[u8; 3]>, out: &Path, stem: &str) -> Result<()> {
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let mut sheet = Sheet::new(img.width, 0);
    sheet.place(img.width, img.height, |x, y| {
        opaque(img.pixels[y * img.width + x])
    });
    sheet.write_png(&out.join(format!("{stem}.png")))
}

/// Writes `<name>.png`: every glyph of a font, in order, 16 to a row, with
/// the ASCII range and the accented letters on separate rows.
pub fn font(f: &gfa_formats::fon::Font, out: &Path, name: &str) -> Result<()> {
    use gfa_formats::fon::ACCENT_BASE;
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let widest = f.glyphs.iter().map(|g| g.width as usize).max().unwrap_or(1);
    let mut sheet = Sheet::new((16 * (widest + 2) + 2).max(64), 2);
    let mut lut: Lut = std::array::from_fn(|i| opaque(f.palette[i]));
    lut[0] = [0, 0, 0, 0];
    for (i, g) in f.glyphs.iter().enumerate() {
        if i % 16 == 0 || i == ACCENT_BASE {
            sheet.new_row();
        }
        let w = g.width as usize;
        // A zero-width glyph still gets a mark, so the count reads right.
        if w == 0 {
            sheet.place(1, f.height as usize, |_, _| [255, 0, 255, 255]);
            continue;
        }
        sheet.place(w, f.height as usize, |x, y| {
            lut[g.pixels[y * w + x] as usize]
        });
    }
    sheet.write_png(&out.join(format!("{name}.png")))
}

/// Writes `tiles.png`, `sprites.png`, `remaps.png` and `deltas.png` for one
/// style.
pub fn style(s: &Style, out: &Path) -> Result<()> {
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;

    // Tiles: one block of rows per bank, 16 to a row.
    let mut sheet = Sheet::new(16 * (TILE_DIM + 2) + 2, 2);
    for kind in [TileKind::Side, TileKind::Lid, TileKind::Aux] {
        sheet.new_row();
        for i in 0..s.tile_count(kind) {
            let linear = s.tile_linear(kind, i).expect("in range");
            let img = s.tile(linear).expect("validated");
            let lut = tile_lut(s, linear);
            sheet.place(TILE_DIM, TILE_DIM, |x, y| lut[img.pixel(x, y) as usize]);
        }
    }
    sheet.write_png(&out.join("tiles.png"))?;

    // Sprites: one block of rows per category.
    let mut sheet = Sheet::new(1024, 2);
    for c in SpriteCategory::ALL {
        sheet.new_row();
        for n in 0..s.sprite_count(c) {
            let i = s.sprite_index(c, n).expect("in range");
            draw_sprite(&mut sheet, s, i, &sprite_lut(s, i, None));
        }
    }
    sheet.write_png(&out.join("sprites.png"))?;

    // Remaps: each vehicle in its twelve recolours, then the first
    // pedestrian in every pedestrian recolour.
    let mut sheet = Sheet::new(1024, 2);
    for (n, car) in s.cars.iter().enumerate() {
        let Some(i) = s.car_sprite(car) else { continue };
        sheet.new_row();
        draw_sprite(&mut sheet, s, i, &sprite_lut(s, i, None));
        for (r, &table) in car.remap8.iter().enumerate() {
            let remap = match &s.colour {
                Colour::Gry(_) => Remap::Table(table),
                Colour::G24(_) => Remap::Clut(n * CAR_REMAPS + r),
            };
            draw_sprite(&mut sheet, s, i, &sprite_lut(s, i, Some(remap)));
        }
    }
    if let Some(ped) = s.sprite_index(SpriteCategory::Ped, 0) {
        sheet.new_row();
        let remaps: Vec<Remap> = match &s.colour {
            Colour::Gry(_) => (128..=187).map(Remap::Table).collect(),
            Colour::G24(c) => (c.remap_cluts - PED_REMAPS..c.remap_cluts)
                .map(Remap::Clut)
                .collect(),
        };
        for remap in remaps {
            draw_sprite(&mut sheet, s, ped, &sprite_lut(s, ped, Some(remap)));
        }
    }
    sheet.write_png(&out.join("remaps.png"))?;

    // Deltas: every sprite that has any, then each delta applied on its own.
    let mut sheet = Sheet::new(1024, 2);
    for (i, sp) in s
        .sprites
        .iter()
        .enumerate()
        .filter(|(_, sp)| !sp.deltas.is_empty())
    {
        sheet.new_row();
        let lut = sprite_lut(s, i, None);
        let base = s.sprite(i).expect("validated").to_vec();
        let w = sp.width as usize;
        sheet.place(w, sp.height as usize, |x, y| lut[base[y * w + x] as usize]);
        for d in 0..sp.deltas.len() {
            let mut px = base.clone();
            s.apply_delta(i, d, &mut px).expect("validated");
            sheet.place(w, sp.height as usize, |x, y| lut[px[y * w + x] as usize]);
        }
    }
    sheet.write_png(&out.join("deltas.png"))?;
    Ok(())
}

#[derive(Clone, Copy)]
enum Remap {
    /// GRY: a remap table number.
    Table(u8),
    /// G24: a remap CLUT number.
    Clut(usize),
}

fn draw_sprite(sheet: &mut Sheet, s: &Style, i: usize, lut: &Lut) {
    let img = s.sprite(i).expect("validated");
    sheet.place(img.width, img.height, |x, y| lut[img.pixel(x, y) as usize]);
}

fn tile_lut(s: &Style, linear: usize) -> Lut {
    match &s.colour {
        Colour::Gry(c) => std::array::from_fn(|i| opaque(c.rgb8(i as u8))),
        Colour::G24(c) => {
            let p = &c.cluts[c.tile_palette(linear, 0).expect("validated") as usize];
            std::array::from_fn(|i| opaque(rgb(p[i])))
        }
    }
}

/// Palette index 0 is transparent in sprites.
fn sprite_lut(s: &Style, sprite: usize, remap: Option<Remap>) -> Lut {
    let mut lut: Lut = match (&s.colour, remap) {
        (Colour::Gry(c), remap) => {
            let table = match remap {
                Some(Remap::Table(t)) => c.remaps.get(t as usize),
                _ => None,
            };
            std::array::from_fn(|i| opaque(c.rgb8(table.map_or(i as u8, |t| t[i]))))
        }
        (Colour::G24(c), remap) => {
            let pal = match remap {
                Some(Remap::Clut(r)) => c.remap_palette(r),
                _ => s.sprites[sprite].clut.and_then(|k| c.sprite_palette(k)),
            };
            match pal {
                Some(p) => {
                    let p = &c.cluts[p as usize];
                    std::array::from_fn(|i| opaque(rgb(p[i])))
                }
                // Make a missing palette obvious.
                None => [[255, 0, 255, 255]; 256],
            }
        }
    };
    lut[0] = [0, 0, 0, 0];
    lut
}

fn rgb([r, g, b, _]: [u8; 4]) -> [u8; 3] {
    [r, g, b]
}

fn opaque([r, g, b]: [u8; 3]) -> [u8; 4] {
    [r, g, b, 255]
}
