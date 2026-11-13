//! Text rendering and simple overlay widgets drawn by the engine itself.

use crate::gfx::{self, argb, blend, split, Image};

pub struct Fonts {
    pub faces: Vec<(String, fontdue::Font)>,
}

impl Fonts {
    pub fn load(files: &[std::path::PathBuf]) -> Fonts {
        let mut faces = Vec::new();
        for f in files {
            let Ok(bytes) = std::fs::read(f) else { continue };
            if let Ok(font) = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                let name = f
                    .file_stem()
                    .map(|s| s.to_string_lossy().trim_end_matches('_').to_string())
                    .unwrap_or_default();
                faces.push((name, font));
            }
        }
        Fonts { faces }
    }

    /// Add the operating system's UI font (for menus/dialogs, like the original's Win32 menus).
    pub fn add_system_font(&mut self) -> Option<usize> {
        let mut cands: Vec<std::path::PathBuf> = Vec::new();
        if cfg!(target_os = "windows") {
            let w = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
            for f in ["tahoma.ttf", "arial.ttf", "segoeui.ttf"] {
                cands.push(std::path::Path::new(&w).join("Fonts").join(f));
            }
        } else if cfg!(target_os = "macos") {
            for f in ["/System/Library/Fonts/Supplemental/Tahoma.ttf", "/System/Library/Fonts/Supplemental/Arial.ttf", "/Library/Fonts/Arial.ttf", "/System/Library/Fonts/Geneva.ttf"] {
                cands.push(f.into());
            }
        } else {
            for d in ["/usr/share/fonts", "/usr/local/share/fonts", "/usr/share/fonts/truetype"] {
                for name in ["DejaVuSans.ttf", "LiberationSans-Regular.ttf", "NotoSans-Regular.ttf", "Arial.ttf"] {
                    if let Some(p) = find_file(std::path::Path::new(d), name, 3) {
                        cands.push(p);
                    }
                }
            }
        }
        for p in cands {
            if let Ok(bytes) = std::fs::read(&p) {
                if let Ok(font) = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                    self.faces.push(("System".into(), font));
                    return Some(self.faces.len() - 1);
                }
            }
        }
        None
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    /// Index of a readable default UI face.
    pub fn ui_face(&self) -> usize {
        for pref in ["STORYBOO", "PATRICK", "MARKET", "TELETYPE"] {
            if let Some(i) = self.faces.iter().position(|(n, _)| n.eq_ignore_ascii_case(pref)) {
                return i;
            }
        }
        0
    }

    /// Width in pixels of `text` at `size`.
    pub fn measure(&self, face: usize, size: f32, text: &str) -> f32 {
        let Some((_, font)) = self.faces.get(face) else {
            return 0.0;
        };
        text.chars().map(|c| font.metrics(c, size).advance_width).sum()
    }

    /// Draw text with its baseline at `y`. Returns the advance.
    pub fn draw(&self, dst: &mut Image, face: usize, size: f32, x: f32, y: f32, color: u32, text: &str) -> f32 {
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

/// Panel look used for engine-made overlays (pickers, prompts).
pub fn panel(dst: &mut Image, x: i32, y: i32, w: i32, h: i32) {
    gfx::fill_round_rect(dst, x + 4, y + 5, w, h, 16, argb(90, 0, 0, 40));
    gfx::fill_round_rect(dst, x, y, w, h, 16, gfx::rgb(40, 50, 170));
    gfx::fill_round_rect(dst, x + 4, y + 4, w - 8, h - 8, 13, gfx::rgb(246, 246, 255));
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

/// Close (x) button.
pub fn close_button(dst: &mut Image, x: i32, y: i32, size: i32) {
    gfx::fill_circle(dst, x as f32 + size as f32 / 2.0, y as f32 + size as f32 / 2.0, size as f32 / 2.0, gfx::rgb(220, 60, 60));
    let c = gfx::rgb(255, 255, 255);
    let m = size / 4;
    for i in m..size - m {
        for t in -1..=1 {
            dst.put(x + i, y + i + t, c);
            dst.put(x + size - 1 - i, y + i + t, c);
        }
    }
}

pub fn hit(x: i32, y: i32, rx: i32, ry: i32, rw: i32, rh: i32) -> bool {
    x >= rx && y >= ry && x < rx + rw && y < ry + rh
}

/// The engine's own color palette (an original layout: hue columns x value rows + greys).
pub fn palette_colors() -> Vec<u32> {
    let mut v = Vec::new();
    // 6 rows x 12 hues
    for row in 0..6 {
        for col in 0..12 {
            let h = col as f32 * 30.0;
            let (s, val) = match row {
                0 => (0.35, 1.0),
                1 => (0.6, 1.0),
                2 => (1.0, 1.0),
                3 => (1.0, 0.8),
                4 => (1.0, 0.6),
                _ => (1.0, 0.4),
            };
            v.push(gfx::hsv(h, s, val));
        }
    }
    // greys + browns + skin tones
    for i in 0..8 {
        let g = (i * 255 / 7) as u8;
        v.push(gfx::rgb(g, g, g));
    }
    for c in [
        (120, 72, 40),
        (160, 100, 60),
        (205, 150, 100),
        (240, 200, 160),
    ] {
        v.push(gfx::rgb(c.0, c.1, c.2));
    }
    v
}

fn find_file(dir: &std::path::Path, name: &str, depth: u32) -> Option<std::path::PathBuf> {
    let p = dir.join(name);
    if p.is_file() {
        return Some(p);
    }
    if depth == 0 {
        return None;
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        if e.path().is_dir() {
            if let Some(f) = find_file(&e.path(), name, depth - 1) {
                return Some(f);
            }
        }
    }
    None
}
