//! Text boxes on the page (the original `RTextPageObject`): typed with the ABC tool,
//! styled from the text panel (font, size 7–72, bold / italic / underline, alignment),
//! movable with the Moving Van's glove until flattened into the picture.

use crate::gfx::{self, Image};
use crate::ui::Fonts;

pub const SIZES: [f32; 14] = [7.0, 9.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 28.0, 32.0, 48.0, 72.0];

#[derive(Clone, Debug, PartialEq)]
pub struct TextBox {
    /// Top-left of the box on the canvas.
    pub x: i32,
    pub y: i32,
    pub text: String,
    pub font: usize,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// 0 left, 1 centre, 2 right.
    pub align: u8,
    pub color: u32,
}

const PAD: i32 = 3;

impl TextBox {
    fn line_h(&self) -> f32 {
        (self.size * 1.2).ceil()
    }

    fn lines(&self) -> Vec<&str> {
        self.text.split('\n').collect()
    }

    fn line_w(&self, fonts: &Fonts, l: &str) -> f32 {
        fonts.measure(self.font, self.size, l) + if self.bold { 1.0 } else { 0.0 } + if self.italic { self.size * 0.2 } else { 0.0 }
    }

    /// Box size (at least one em wide so an empty box can be clicked).
    pub fn size_px(&self, fonts: &Fonts) -> (i32, i32) {
        let w = self.lines().iter().map(|l| self.line_w(fonts, l)).fold(self.size * 0.6, f32::max);
        let h = self.lines().len() as f32 * self.line_h();
        (w.ceil() as i32 + 2 * PAD, h as i32 + 2 * PAD)
    }

    pub fn rect(&self, fonts: &Fonts) -> (i32, i32, i32, i32) {
        let (w, h) = self.size_px(fonts);
        (self.x, self.y, w, h)
    }

    pub fn contains(&self, fonts: &Fonts, x: i32, y: i32) -> bool {
        let (rx, ry, w, h) = self.rect(fonts);
        x >= rx && y >= ry && x < rx + w && y < ry + h
    }

    fn line_x(&self, fonts: &Fonts, box_w: i32, l: &str) -> f32 {
        let lw = self.line_w(fonts, l);
        let inner = (box_w - 2 * PAD) as f32;
        PAD as f32
            + match self.align {
                1 => (inner - lw) / 2.0,
                2 => inner - lw,
                _ => 0.0,
            }
    }

    /// Render onto `dst` with the box's top-left at (ox + x, oy + y).
    pub fn draw(&self, dst: &mut Image, fonts: &Fonts, ox: i32, oy: i32) {
        if fonts.is_empty() {
            return;
        }
        let (w, h) = self.size_px(fonts);
        let mut img = Image::new(w as u32 + 2, h as u32 + 2);
        let lh = self.line_h();
        for (i, l) in self.lines().iter().enumerate() {
            let x = self.line_x(fonts, w, l);
            let base = PAD as f32 + i as f32 * lh + self.size * 0.95;
            fonts.draw(&mut img, self.font, self.size, x, base, self.color, l);
            if self.bold {
                fonts.draw(&mut img, self.font, self.size, x + 1.0, base, self.color, l);
            }
            if self.underline && !l.is_empty() {
                let uw = fonts.measure(self.font, self.size, l).round() as i32;
                let t = (self.size / 14.0).ceil().max(1.0) as i32;
                gfx::fill_rect(&mut img, x as i32, (base + self.size * 0.12) as i32, uw, t, self.color);
            }
        }
        if self.italic {
            // Synthetic oblique: shift rows right in proportion to their height above the baseline.
            let mut out = Image::new(img.width, img.height);
            for yy in 0..img.height as i32 {
                let row_in_line = (yy - PAD) as f32 % lh;
                let shift = ((lh - row_in_line) * 0.2).round() as i32;
                for xx in 0..img.width as i32 {
                    let nx = xx + shift;
                    if nx < out.width as i32 {
                        out.pixels[(yy * out.width as i32 + nx) as usize] = img.pixels[(yy * img.width as i32 + xx) as usize];
                    }
                }
            }
            img = out;
        }
        gfx::blit(dst, &img, ox + self.x, oy + self.y);
    }

    /// Screen position (relative to the box origin) and height of the caret at char index `c`.
    pub fn caret(&self, fonts: &Fonts, c: usize) -> (i32, i32, i32) {
        let (w, _) = self.size_px(fonts);
        let mut left = c;
        let lines = self.lines();
        for (i, l) in lines.iter().enumerate() {
            let n = l.chars().count();
            if left <= n || i + 1 == lines.len() {
                let pre: String = l.chars().take(left).collect();
                let x = self.line_x(fonts, w, l) + fonts.measure(self.font, self.size, &pre);
                let y = PAD as f32 + i as f32 * self.line_h();
                return (x as i32, y as i32, self.line_h() as i32);
            }
            left -= n + 1;
        }
        (PAD, PAD, self.line_h() as i32)
    }
}

/// The family name from a TrueType `name` table (name id 1), if present.
pub fn ttf_family(data: &[u8]) -> Option<String> {
    let u16be = |o: usize| -> Option<u16> { Some(u16::from_be_bytes([*data.get(o)?, *data.get(o + 1)?])) };
    let u32be = |o: usize| -> Option<u32> { Some(u32::from_be_bytes([*data.get(o)?, *data.get(o + 1)?, *data.get(o + 2)?, *data.get(o + 3)?])) };
    let n = u16be(4)? as usize;
    let mut name_off = None;
    for i in 0..n {
        let rec = 12 + i * 16;
        if data.get(rec..rec + 4)? == b"name" {
            name_off = Some(u32be(rec + 8)? as usize);
        }
    }
    let t = name_off?;
    let count = u16be(t + 2)? as usize;
    let strings = t + u16be(t + 4)? as usize;
    let mut best: Option<String> = None;
    for i in 0..count {
        let r = t + 6 + i * 12;
        let (pid, _eid, _lid, nid, len, off) = (u16be(r)?, u16be(r + 2)?, u16be(r + 4)?, u16be(r + 6)?, u16be(r + 8)? as usize, u16be(r + 10)? as usize);
        if nid != 1 {
            continue;
        }
        let bytes = data.get(strings + off..strings + off + len)?;
        let s = if pid == 3 || pid == 0 {
            let u: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&u)
        } else {
            bytes.iter().map(|&b| b as char).collect()
        };
        let s = s.trim().to_string();
        if !s.is_empty() && (best.is_none() || pid == 3) {
            best = Some(s);
        }
    }
    best
}
