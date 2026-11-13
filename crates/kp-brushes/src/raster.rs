//! Drawing primitives standing in for the GDI / OpenGL calls of the original.
//!
//! All colours are 5-bit ([`Rgb5`]); pixels are written expanded (see `color.rs`). GDI
//! conventions: rectangles and ellipses exclude their right/bottom edge, `LineTo` does not
//! paint the last point. OpenGL brushes are alpha-blended in floating point against the
//! 5-bit destination and rounded back to 5 bits [I] (GL rendered into a 16-bit DC).

use crate::color::{argb_to_rgb5, get5, put5, Rgb5};
use mohawk::Image;

/// Solid 1-px GDI line from `a` to `b`, last point excluded (`MoveTo`/`LineTo`).
pub fn line1(img: &mut Image, a: (i32, i32), b: (i32, i32), c: Rgb5) {
    let (mut x, mut y) = a;
    let dx = (b.0 - a.0).abs();
    let dy = -(b.1 - a.1).abs();
    let sx = if a.0 < b.0 { 1 } else { -1 };
    let sy = if a.1 < b.1 { 1 } else { -1 };
    let mut err = dx + dy;
    while (x, y) != b {
        put5(img, x, y, c);
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// GDI pen line of width `w`. Width ≤ 1 is the cosmetic pen ([`line1`]); wider pens are
/// geometric with round caps: every pixel whose centre lies within `w/2` of the segment.
pub fn line_w(img: &mut Image, a: (i32, i32), b: (i32, i32), w: i32, c: Rgb5) {
    if w <= 1 {
        line1(img, a, b, c);
        return;
    }
    let r = w as f32 / 2.0;
    let (ax, ay) = (a.0 as f32 + 0.5, a.1 as f32 + 0.5);
    let (bx, by) = (b.0 as f32 + 0.5, b.1 as f32 + 0.5);
    let x0 = (a.0.min(b.0) as f32 - r).floor() as i32;
    let x1 = (a.0.max(b.0) as f32 + r).ceil() as i32;
    let y0 = (a.1.min(b.1) as f32 - r).floor() as i32;
    let y1 = (a.1.max(b.1) as f32 + r).ceil() as i32;
    let (vx, vy) = (bx - ax, by - ay);
    let len2 = vx * vx + vy * vy;
    let (cw, ch) = (img.width as i32, img.height as i32);
    for y in y0.max(0)..=y1.min(ch - 1) {
        for x in x0.max(0)..=x1.min(cw - 1) {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let t = if len2 > 0.0 { (((px - ax) * vx + (py - ay) * vy) / len2).clamp(0.0, 1.0) } else { 0.0 };
            let (qx, qy) = (ax + vx * t - px, ay + vy * t - py);
            if qx * qx + qy * qy <= r * r * 0.999 + 0.25 {
                put5(img, x, y, c);
            }
        }
    }
}

pub fn fill_rect(img: &mut Image, l: i32, t: i32, r: i32, b: i32, c: Rgb5) {
    let (cw, ch) = (img.width as i32, img.height as i32);
    for y in t.max(0)..b.min(ch) {
        for x in l.max(0)..r.min(cw) {
            put5(img, x, y, c);
        }
    }
}

/// Filled GDI ellipse in the bounding box `[l, r) × [t, b)`.
pub fn fill_ellipse(img: &mut Image, l: i32, t: i32, r: i32, b: i32, c: Rgb5) {
    let (l, r) = (l.min(r), l.max(r));
    let (t, b) = (t.min(b), t.max(b));
    if r - l < 1 || b - t < 1 {
        return;
    }
    let cx = (l + r) as f32 / 2.0;
    let cy = (t + b) as f32 / 2.0;
    let rx = (r - l) as f32 / 2.0;
    let ry = (b - t) as f32 / 2.0;
    let (cw, ch) = (img.width as i32, img.height as i32);
    for y in t.max(0)..b.min(ch) {
        let dy = (y as f32 + 0.5 - cy) / ry;
        for x in l.max(0)..r.min(cw) {
            let dx = (x as f32 + 0.5 - cx) / rx;
            if dx * dx + dy * dy <= 1.0 || (r - l <= 2 && b - t <= 2) {
                put5(img, x, y, c);
            }
        }
    }
}

/// Outline-only GDI ellipse (1-px pen).
pub fn frame_ellipse(img: &mut Image, l: i32, t: i32, r: i32, b: i32, c: Rgb5) {
    let (l, r) = (l.min(r), l.max(r));
    let (t, b) = (t.min(b), t.max(b));
    if r - l < 1 || b - t < 1 {
        return;
    }
    let cx = (l + r) as f32 / 2.0;
    let cy = (t + b) as f32 / 2.0;
    let rx = (r - l) as f32 / 2.0;
    let ry = (b - t) as f32 / 2.0;
    let inside = |x: i32, y: i32| {
        let dx = (x as f32 + 0.5 - cx) / rx;
        let dy = (y as f32 + 0.5 - cy) / ry;
        dx * dx + dy * dy <= 1.0
    };
    let (cw, ch) = (img.width as i32, img.height as i32);
    for y in t.max(0)..b.min(ch) {
        for x in l.max(0)..r.min(cw) {
            if inside(x, y) && (!inside(x - 1, y) || !inside(x + 1, y) || !inside(x, y - 1) || !inside(x, y + 1)) {
                put5(img, x, y, c);
            }
        }
    }
}

/// Pixels inside the ellipse inscribed in `[l, r) × [t, b)` (pixel centres; boxes of at
/// most 2×2 are filled whole).
fn ellipse_pixels(img: &Image, l: i32, t: i32, r: i32, b: i32, mut f: impl FnMut(i32, i32)) {
    let (l, r) = (l.min(r), l.max(r));
    let (t, b) = (t.min(b), t.max(b));
    if r - l < 1 || b - t < 1 {
        return;
    }
    let cx = (l + r) as f32 / 2.0;
    let cy = (t + b) as f32 / 2.0;
    let rx = (r - l) as f32 / 2.0;
    let ry = (b - t) as f32 / 2.0;
    let tiny = r - l <= 2 && b - t <= 2;
    for y in t.max(0)..b.min(img.height as i32) {
        let dy = (y as f32 + 0.5 - cy) / ry;
        for x in l.max(0)..r.min(img.width as i32) {
            let dx = (x as f32 + 0.5 - cx) / rx;
            if tiny || dx * dx + dy * dy <= 1.0 {
                f(x, y);
            }
        }
    }
}

/// GDI `Ellipse` with a solid brush and the NULL pen: the interior only, which GDI makes
/// one pixel smaller to the right and bottom.
pub fn paint_oval(img: &mut Image, l: i32, t: i32, r: i32, b: i32, c: Rgb5) {
    let mut px = Vec::new();
    ellipse_pixels(img, l.min(r), t.min(b), l.max(r) - 1, t.max(b) - 1, |x, y| px.push((x, y)));
    for (x, y) in px {
        put5(img, x, y, c);
    }
}

/// GDI `Ellipse` with the NULL brush and a solid pen of width `w` (outline only; a wide
/// pen straddles the ellipse path).
pub fn frame_oval(img: &mut Image, l: i32, t: i32, r: i32, b: i32, w: i32, c: Rgb5) {
    if w <= 1 {
        frame_ellipse(img, l, t, r, b, c);
        return;
    }
    let (l, r) = (l.min(r), l.max(r));
    let (t, b) = (t.min(b), t.max(b));
    let cx = (l + r) as f32 / 2.0;
    let cy = (t + b) as f32 / 2.0;
    let (rx, ry) = ((r - l) as f32 / 2.0, (b - t) as f32 / 2.0);
    let h = w as f32 / 2.0;
    let (ox, oy) = (rx + h, ry + h);
    let (ix, iy) = (rx - h, ry - h);
    let x0 = (cx - ox).floor() as i32;
    let x1 = (cx + ox).ceil() as i32;
    let y0 = (cy - oy).floor() as i32;
    let y1 = (cy + oy).ceil() as i32;
    for y in y0.max(0)..y1.min(img.height as i32) {
        for x in x0.max(0)..x1.min(img.width as i32) {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let outer = (dx / ox).powi(2) + (dy / oy).powi(2) <= 1.0;
            let inner = ix > 0.0 && iy > 0.0 && (dx / ix).powi(2) + (dy / iy).powi(2) < 1.0;
            if outer && !inner {
                put5(img, x, y, c);
            }
        }
    }
}

/// `PatBlt(PATINVERT)` with a white brush clipped to `CreateEllipticRgn(l, t, r, b)`:
/// inverts the pixels inside the ellipse (5-bit channels `31 - c`).
pub fn invert_oval(img: &mut Image, l: i32, t: i32, r: i32, b: i32) {
    let mut px = Vec::new();
    ellipse_pixels(img, l.min(r), t.min(b), l.max(r) - 1, t.max(b) - 1, |x, y| px.push((x, y)));
    for (x, y) in px {
        if let Some(c) = get5(img, x, y) {
            put5(img, x, y, [31 - c[0], 31 - c[1], 31 - c[2]]);
        }
    }
}

/// Filled polygon (GDI `ALTERNATE` fill, pixel centres) plus its 1-px outline in the same
/// colour (GDI `Polygon` with a matching pen).
pub fn fill_polygon(img: &mut Image, pts: &[(i32, i32)], c: Rgb5) {
    if pts.is_empty() {
        return;
    }
    let ymin = pts.iter().map(|p| p.1).min().unwrap().max(0);
    let ymax = pts.iter().map(|p| p.1).max().unwrap().min(img.height as i32 - 1);
    let mut xs: Vec<f32> = Vec::new();
    for y in ymin..=ymax {
        xs.clear();
        let fy = y as f32 + 0.5;
        for i in 0..pts.len() {
            let (x0, y0) = (pts[i].0 as f32 + 0.5, pts[i].1 as f32 + 0.5);
            let j = (i + 1) % pts.len();
            let (x1, y1) = (pts[j].0 as f32 + 0.5, pts[j].1 as f32 + 0.5);
            if (y0 <= fy && y1 > fy) || (y1 <= fy && y0 > fy) {
                xs.push(x0 + (fy - y0) / (y1 - y0) * (x1 - x0));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in xs.chunks_exact(2) {
            let xa = (pair[0] - 0.5).ceil() as i32;
            let xb = (pair[1] - 0.5).floor() as i32;
            for x in xa.max(0)..=xb.min(img.width as i32 - 1) {
                put5(img, x, y, c);
            }
        }
    }
    for i in 0..pts.len() {
        line1(img, pts[i], pts[(i + 1) % pts.len()], c);
    }
    if pts.len() == 1 {
        put5(img, pts[0].0, pts[0].1, c);
    }
}

/// Alpha-blend one pixel with a float colour (channels 0..1) — GL `SRC_ALPHA,
/// ONE_MINUS_SRC_ALPHA`.
#[inline]
pub fn blend_px(img: &mut Image, x: i32, y: i32, src: [f32; 3], a: f32) {
    let Some(d) = get5(img, x, y) else { return };
    let mut out = [0i32; 3];
    for k in 0..3 {
        let dv = d[k] as f32 / 31.0;
        let v = src[k].clamp(0.0, 1.0) * a + dv * (1.0 - a);
        out[k] = (v.clamp(0.0, 1.0) * 31.0 + 0.5) as i32;
    }
    put5(img, x, y, out);
}

/// Generic GL blend: `out = src*sf + dst*df` per channel with factors computed by `f`.
#[inline]
pub fn blend_px_with(img: &mut Image, x: i32, y: i32, f: impl Fn(f32, f32) -> f32) {
    let Some(d) = get5(img, x, y) else { return };
    let mut out = [0i32; 3];
    for k in 0..3 {
        let dv = d[k] as f32 / 31.0;
        out[k] = (f(dv, k as f32).clamp(0.0, 1.0) * 31.0 + 0.5) as i32;
    }
    put5(img, x, y, out);
}

/// Gouraud-shaded triangle, colours per vertex (0..1), constant alpha, alpha blended.
pub fn gl_triangle(img: &mut Image, v: [(f32, f32); 3], col: [[f32; 3]; 3], alpha: f32) {
    let minx = v.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
    let maxx = v.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil().min(img.width as f32 - 1.0) as i32;
    let miny = v.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
    let maxy = v.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().min(img.height as f32 - 1.0) as i32;
    let area = (v[1].0 - v[0].0) * (v[2].1 - v[0].1) - (v[2].0 - v[0].0) * (v[1].1 - v[0].1);
    if area.abs() < 1e-6 {
        return;
    }
    for y in miny..=maxy {
        for x in minx..=maxx {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((v[1].0 - px) * (v[2].1 - py) - (v[2].0 - px) * (v[1].1 - py)) / area;
            let w1 = ((v[2].0 - px) * (v[0].1 - py) - (v[0].0 - px) * (v[2].1 - py)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let c = [
                col[0][0] * w0 + col[1][0] * w1 + col[2][0] * w2,
                col[0][1] * w0 + col[1][1] * w1 + col[2][1] * w2,
                col[0][2] * w0 + col[1][2] * w1 + col[2][2] * w2,
            ];
            blend_px(img, x, y, c, alpha);
        }
    }
}

/// `gluDisk(inner, outer, 20 slices)` approximated by exact radii, alpha blended.
pub fn gl_ring(img: &mut Image, cx: f32, cy: f32, inner: f32, outer: f32, col: [f32; 3], alpha: f32) {
    if outer <= 0.0 {
        return;
    }
    let x0 = (cx - outer).floor().max(0.0) as i32;
    let x1 = (cx + outer).ceil().min(img.width as f32 - 1.0) as i32;
    let y0 = (cy - outer).floor().max(0.0) as i32;
    let y1 = (cy + outer).ceil().min(img.height as f32 - 1.0) as i32;
    let (i2, o2) = (inner.max(0.0).powi(2), outer * outer);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let d2 = dx * dx + dy * dy;
            if d2 >= i2 && d2 <= o2 {
                blend_px(img, x, y, col, alpha);
            }
        }
    }
}

/// Draw an ARGB sprite with its top-left at `(x, y)`: opaque pixels copied, partial
/// alpha blended, transparent skipped ("transparent mode 8").
pub fn blit(img: &mut Image, spr: &Image, x: i32, y: i32) {
    for sy in 0..spr.height as i32 {
        let dy = y + sy;
        if dy < 0 || dy >= img.height as i32 {
            continue;
        }
        for sx in 0..spr.width as i32 {
            let dx = x + sx;
            if dx < 0 || dx >= img.width as i32 {
                continue;
            }
            let p = spr.pixels[(sy * spr.width as i32 + sx) as usize];
            blit_px(img, dx, dy, p);
        }
    }
}

#[inline]
fn blit_px(img: &mut Image, x: i32, y: i32, p: u32) {
    let a = p >> 24;
    if a == 0 {
        return;
    }
    let s = argb_to_rgb5(p | 0xff00_0000);
    if a >= 255 {
        put5(img, x, y, s);
    } else if let Some(d) = get5(img, x, y) {
        let t = a as i32;
        let m = |s: i32, d: i32| (s * t + d * (255 - t) + 127) / 255;
        put5(img, x, y, [m(s[0], d[0]), m(s[1], d[1]), m(s[2], d[2])]);
    }
}

/// Nearest-neighbour stretch of `spr` into `[l, r) × [t, b)`, transparent copy.
pub fn stretch_blit(img: &mut Image, spr: &Image, l: i32, t: i32, r: i32, b: i32) {
    let (w, h) = (r - l, b - t);
    if w <= 0 || h <= 0 || spr.width == 0 || spr.height == 0 {
        return;
    }
    for y in t.max(0)..b.min(img.height as i32) {
        let sy = (((y - t) as i64 * spr.height as i64) / h as i64) as u32;
        for x in l.max(0)..r.min(img.width as i32) {
            let sx = (((x - l) as i64 * spr.width as i64) / w as i64) as u32;
            let p = spr.pixels[(sy.min(spr.height - 1) * spr.width + sx.min(spr.width - 1)) as usize];
            blit_px(img, x, y, p);
        }
    }
}

/// Paint a coverage map (0..255) in a colour at `(x, y)`.
pub fn coverage(img: &mut Image, cov: &[u8], w: usize, h: usize, x: i32, y: i32, c: Rgb5) {
    for yy in 0..h {
        for xx in 0..w {
            let a = cov[yy * w + xx] as i32;
            if a == 0 {
                continue;
            }
            let (px, py) = (x + xx as i32, y + yy as i32);
            let Some(d) = get5(img, px, py) else { continue };
            let m = |s: i32, d: i32| (s * a + d * (255 - a) + 127) / 255;
            put5(img, px, py, [m(c[0], d[0]), m(c[1], d[1]), m(c[2], d[2])]);
        }
    }
}

/// Text renderer used by ABC and DotToDot.
pub struct TextRun {
    pub cov: Vec<u8>,
    pub w: usize,
    pub h: usize,
    /// Advance width in pixels.
    pub advance: i32,
    /// Offset of the coverage map relative to the pen position (top of the line box).
    pub ox: i32,
    pub oy: i32,
}

/// Rasterise one character at pixel size `px`: with the Jester TTF when available,
/// otherwise the built-in 5×7 bitmap font scaled to the size.
pub fn glyph(font: Option<&fontdue::Font>, ch: char, px: f32) -> TextRun {
    if let Some(f) = font {
        if f.lookup_glyph_index(ch) != 0 || ch == ' ' {
            let (m, cov) = f.rasterize(ch, px);
            let ascent = f.horizontal_line_metrics(px).map(|l| l.ascent).unwrap_or(px * 0.8);
            return TextRun {
                cov,
                w: m.width,
                h: m.height,
                advance: m.advance_width.round() as i32,
                ox: m.xmin,
                oy: (ascent - m.height as f32 - m.ymin as f32).round() as i32,
            };
        }
    }
    let scale = ((px / 8.0).round() as usize).max(1);
    let rows = builtin_glyph(ch);
    let (w, h) = (5 * scale, 7 * scale);
    let mut cov = vec![0u8; w * h];
    for (r, bits) in rows.iter().enumerate() {
        for c in 0..5 {
            if bits & (0x10 >> c) != 0 {
                for yy in 0..scale {
                    for xx in 0..scale {
                        cov[(r * scale + yy) * w + c * scale + xx] = 255;
                    }
                }
            }
        }
    }
    TextRun { cov, w, h, advance: (6 * scale) as i32, ox: 0, oy: 0 }
}

/// Draw a string with its line box top-left at `(x, y)`; returns the advance.
pub fn text(img: &mut Image, font: Option<&fontdue::Font>, s: &str, px: f32, x: i32, y: i32, c: Rgb5) -> i32 {
    let mut pen = x;
    for ch in s.chars() {
        let g = glyph(font, ch, px);
        coverage(img, &g.cov, g.w, g.h, pen + g.ox, y + g.oy, c);
        pen += g.advance;
    }
    pen - x
}

/// 5×7 font rows (bit 4 = left column) for digits, letters and a few signs.
fn builtin_glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1f, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x1e, 0x01, 0x01, 0x11, 0x0e],
        '6' => [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'D' => [0x1c, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1c],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0c],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x11, 0x0a, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '-' => [0, 0, 0, 0x1f, 0, 0, 0],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        ':' => [0, 0x0c, 0x0c, 0, 0x0c, 0x0c, 0],
        ',' => [0, 0, 0, 0, 0x0c, 0x04, 0x08],
        '+' => [0, 0x04, 0x04, 0x1f, 0x04, 0x04, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0x1f],
        '\'' => [0x04, 0x04, 0x08, 0, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0x0c, 0x0c],
        '!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0, 0x04],
        '?' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0, 0x04],
        ' ' => [0; 7],
        _ => [0x1f, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1f],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::WHITE5;

    #[test]
    fn line_excludes_end() {
        let mut img = Image::filled(10, 10, 0xff00_0000);
        line1(&mut img, (1, 1), (5, 1), WHITE5);
        assert_eq!(img.get(1, 1), Some(0xffff_ffff));
        assert_eq!(img.get(4, 1), Some(0xffff_ffff));
        assert_eq!(img.get(5, 1), Some(0xff00_0000));
    }

    #[test]
    fn ellipse_inside_box() {
        let mut img = Image::filled(20, 20, 0xff00_0000);
        fill_ellipse(&mut img, 2, 2, 12, 8, WHITE5);
        assert_eq!(img.get(7, 5), Some(0xffff_ffff));
        assert_eq!(img.get(12, 5), Some(0xff00_0000));
        assert_eq!(img.get(1, 5), Some(0xff00_0000));
    }
}
