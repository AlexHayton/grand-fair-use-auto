//! Contact sheets: many small paletted images packed onto one RGBA PNG.

use std::path::Path;

use anyhow::{Context, Result};

/// An RGBA canvas that packs images left to right in rows ("shelves").
pub struct Sheet {
    width: usize,
    gap: usize,
    pixels: Vec<[u8; 4]>,
    height: usize,
    x: usize,
    y: usize,
    shelf: usize,
}

impl Sheet {
    pub fn new(width: usize, gap: usize) -> Self {
        Self {
            width,
            gap,
            pixels: Vec::new(),
            height: 0,
            x: gap,
            y: gap,
            shelf: 0,
        }
    }

    /// Starts a new row, so the next image begins at the left edge.
    pub fn new_row(&mut self) {
        if self.x > self.gap {
            self.y += self.shelf + self.gap;
            self.x = self.gap;
            self.shelf = 0;
        }
    }

    /// Places a `w`×`h` image whose pixel colours come from `colour(x, y)`,
    /// and returns where it went.
    pub fn place(
        &mut self,
        w: usize,
        h: usize,
        colour: impl Fn(usize, usize) -> [u8; 4],
    ) -> (usize, usize) {
        if self.x + w + self.gap > self.width {
            self.new_row();
        }
        let (ox, oy) = (self.x, self.y);
        self.grow(oy + h + self.gap);
        for y in 0..h {
            for x in 0..w {
                self.pixels[(oy + y) * self.width + ox + x] = colour(x, y);
            }
        }
        self.x += w + self.gap;
        self.shelf = self.shelf.max(h);
        (ox, oy)
    }

    fn grow(&mut self, height: usize) {
        if height > self.height {
            self.height = height;
            self.pixels.resize(self.width * height, [0, 0, 0, 0]);
        }
    }

    pub fn write_png(&self, path: &Path) -> Result<()> {
        let file =
            std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
        let mut enc = png::Encoder::new(
            std::io::BufWriter::new(file),
            self.width as u32,
            self.height as u32,
        );
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header()?;
        w.write_image_data(self.pixels.as_flattened())?;
        w.finish()?;
        Ok(())
    }
}
