//! Text rendering and simple overlay widgets drawn by the engine itself.

use crate::gfx::{self, argb, blend, split, Image};

/// Face index of the built-in Win9x-style bitmap UI font (`uifont.rs`); size is ignored.
pub const UI_BITMAP: usize = usize::MAX;

fn bitmap_glyph(c: char) -> Option<&'static (char, u8, [u16; 11])> {
    let i = (c as u32).checked_sub(32)? as usize;
    crate::uifont::GLYPHS.get(i).filter(|g| g.0 == c).or_else(|| crate::uifont::GLYPHS.iter().find(|g| g.0 == c))
}

fn bitmap_advance(c: char) -> f32 {
    bitmap_glyph(c).or_else(|| bitmap_glyph('?')).map(|g| g.1 as f32 + 1.0).unwrap_or(4.0)
}

pub struct Fonts {
    pub faces: Vec<(String, fontdue::Font)>,
}

impl Fonts {
    pub fn load(files: &[std::path::PathBuf]) -> Fonts {
        let mut faces = Vec::new();
        for f in files {
            let Ok(bytes) = std::fs::read(f) else { continue };
            let family = crate::textbox::ttf_family(&bytes);
            if let Ok(font) = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                let name = family.unwrap_or_else(|| {
                    f.file_stem().map(|s| s.to_string_lossy().trim_end_matches('_').to_string()).unwrap_or_default()
                });
                faces.push((name, font));
            }
        }
        // Listed by family name, like the original's font enumeration.
        faces.sort_by_key(|f: &(String, fontdue::Font)| f.0.to_lowercase());
        Fonts { faces }
    }


    /// Face whose family name starts with `name` (case-insensitive).
    pub fn find(&self, name: &str) -> Option<usize> {
        let n = name.to_lowercase();
        self.faces.iter().position(|(f, _)| f.to_lowercase().starts_with(&n))
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Index of a readable default UI face.
    pub fn ui_face(&self) -> usize {
        for pref in ["Storybook", "Patrick", "Market", "Teletype"] {
            if let Some(i) = self.find(pref) {
                return i;
            }
        }
        0
    }

    /// Width in pixels of `text` at `size`.
    pub fn measure(&self, face: usize, size: f32, text: &str) -> f32 {
        if face == UI_BITMAP {
            let _ = size;
            return text.chars().map(bitmap_advance).sum::<f32>() - if text.is_empty() { 0.0 } else { 1.0 };
        }
        let Some((_, font)) = self.faces.get(face) else {
            return 0.0;
        };
        text.chars().map(|c| font.metrics(c, size).advance_width).sum()
    }

    /// Draw text with its baseline at `y`. Returns the advance.
    pub fn draw(&self, dst: &mut Image, face: usize, size: f32, x: f32, y: f32, color: u32, text: &str) -> f32 {
        if face == UI_BITMAP {
            let top = y.round() as i32 - crate::uifont::BASELINE;
            let mut pen = x.round() as i32;
            for c in text.chars() {
                let Some(&(_, w, rows)) = bitmap_glyph(c).or_else(|| bitmap_glyph('?')) else { continue };
                for (ry, bits) in rows.iter().enumerate() {
                    for rx in 0..w as i32 {
                        if bits >> (w as i32 - 1 - rx) & 1 == 1 {
                            let (px, py) = (pen + rx, top + ry as i32);
                            if px >= 0 && py >= 0 && px < dst.width as i32 && py < dst.height as i32 {
                                dst.put(px, py, color);
                            }
                        }
                    }
                }
                pen += w as i32 + 1;
            }
            let _ = size;
            return pen as f32 - x;
        }
        let Some((_, font)) = self.faces.get(face) else {
            return 0.0;
        };
        let (ca, cr, cg, cb) = split(color);
        let mut pen = x;
        for c in text.chars() {
            let (m, bmp) = font.rasterize(c, size);
            let gx = pen.round() as i32 + m.xmin;
            let gy = y.round() as i32 - m.height as i32 - m.ymin;
            for yy in 0..m.height {
                for xx in 0..m.width {
                    let cov = bmp[yy * m.width + xx] as u32;
                    if cov == 0 {
                        continue;
                    }
                    let a = (cov * ca / 255) as u8;
                    let px = gx + xx as i32;
                    let py = gy + yy as i32;
                    if let Some(d) = dst.get(px, py) {
                        dst.put(px, py, blend(d, argb(a, cr as u8, cg as u8, cb as u8)));
                    }
                }
            }
            pen += m.advance_width;
        }
        pen - x
    }

    /// Draw text centred in a box, shrinking it to fit.
    pub fn draw_centered(&self, dst: &mut Image, face: usize, size: f32, cx: f32, baseline: f32, max_w: f32, color: u32, text: &str) {
        let mut s = size;
        while s > 7.0 && self.measure(face, s, text) > max_w {
            s -= 1.0;
        }
        let w = self.measure(face, s, text);
        self.draw(dst, face, s, cx - w / 2.0, baseline, color, text);
    }
}


/// A round button with an arrow (dir: -1 left/up, 1 right/down), horizontal or vertical.
pub fn arrow_button(dst: &mut Image, x: i32, y: i32, size: i32, dir: i32, vertical: bool, enabled: bool) {
    let bg = if enabled { gfx::rgb(70, 90, 220) } else { gfx::rgb(170, 175, 200) };
    gfx::fill_circle(dst, x as f32 + size as f32 / 2.0, y as f32 + size as f32 / 2.0, size as f32 / 2.0, bg);
    let c = gfx::rgb(255, 255, 255);
    let cx = x + size / 2;
    let cy = y + size / 2;
    let s = size / 4;
    for i in 0..=s {
        let span = s - i;
        for j in -span..=span {
            let (px, py) = if vertical {
                (cx + j, cy + dir * (i - s / 2))
            } else {
                (cx + dir * (i - s / 2), cy + j)
            };
            dst.put(px, py, c);
        }
    }
}


pub fn hit(x: i32, y: i32, rx: i32, ry: i32, rw: i32, rh: i32) -> bool {
    x >= rx && y >= ry && x < rx + rw && y < ry + rh
}


