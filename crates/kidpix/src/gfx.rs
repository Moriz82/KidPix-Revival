//! Software rendering helpers on `mohawk::Image` (ARGB8888, straight alpha).

pub use mohawk::Image;

#[inline]
pub fn argb(a: u8, r: u8, g: u8, b: u8) -> u32 {
    (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

#[inline]
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    argb(255, r, g, b)
}

#[inline]
pub fn split(c: u32) -> (u32, u32, u32, u32) {
    (c >> 24, (c >> 16) & 255, (c >> 8) & 255, c & 255)
}

/// Blend `src` over `dst` (dst assumed opaque or straight alpha).
#[inline]
pub fn blend(dst: u32, src: u32) -> u32 {
    let sa = src >> 24;
    if sa == 255 {
        return src;
    }
    if sa == 0 {
        return dst;
    }
    let (da, dr, dg, db) = split(dst);
    let (_, sr, sg, sb) = split(src);
    let ia = 255 - sa;
    if da == 255 {
        let r = (sr * sa + dr * ia) / 255;
        let g = (sg * sa + dg * ia) / 255;
        let b = (sb * sa + db * ia) / 255;
        return 0xff00_0000 | r << 16 | g << 8 | b;
    }
    let oa = sa + da * ia / 255;
    let oa_ = oa.max(1);
    let r = (sr * sa + dr * da * ia / 255) / oa_;
    let g = (sg * sa + dg * da * ia / 255) / oa_;
    let b = (sb * sa + db * da * ia / 255) / oa_;
    oa << 24 | r.min(255) << 16 | g.min(255) << 8 | b.min(255)
}

/// Mix two opaque colors: t=0 → a, t=255 → b.
#[inline]
pub fn lerp(a: u32, b: u32, t: u32) -> u32 {
    let (_, ar, ag, ab) = split(a);
    let (_, br, bg, bb) = split(b);
    let it = 255 - t;
    let r = (ar * it + br * t) / 255;
    let g = (ag * it + bg * t) / 255;
    let bl = (ab * it + bb * t) / 255;
    0xff00_0000 | r << 16 | g << 8 | bl
}

/// Alpha-blit `src` onto `dst` at (x, y).
pub fn blit(dst: &mut Image, src: &Image, x: i32, y: i32) {
    blit_clip(dst, src, x, y, None)
}

/// Alpha-blit, optionally restricted to a clip rectangle (x0, y0, x1, y1) in dst coords.
pub fn blit_clip(dst: &mut Image, src: &Image, x: i32, y: i32, clip: Option<(i32, i32, i32, i32)>) {
    let (cx0, cy0, cx1, cy1) = clip.unwrap_or((0, 0, dst.width as i32, dst.height as i32));
    let x0 = x.max(cx0).max(0);
    let y0 = y.max(cy0).max(0);
    let x1 = (x + src.width as i32).min(cx1).min(dst.width as i32);
    let y1 = (y + src.height as i32).min(cy1).min(dst.height as i32);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let dw = dst.width as usize;
    let sw = src.width as usize;
    for yy in y0..y1 {
        let srow = (yy - y) as usize * sw;
        let drow = yy as usize * dw;
        for xx in x0..x1 {
            let s = src.pixels[srow + (xx - x) as usize];
            if s >> 24 == 0 {
                continue;
            }
            let d = &mut dst.pixels[drow + xx as usize];
            *d = blend(*d, s);
        }
    }
}

/// Copy a sub-rectangle of an image (clamped).
pub fn crop(src: &Image, x: i32, y: i32, w: i32, h: i32) -> Image {
    let mut out = Image::new(w.max(0) as u32, h.max(0) as u32);
    for yy in 0..h {
        for xx in 0..w {
            if let Some(p) = src.get(x + xx, y + yy) {
                out.pixels[(yy * w + xx) as usize] = p;
            }
        }
    }
    out
}

pub fn fill_rect(dst: &mut Image, x: i32, y: i32, w: i32, h: i32, c: u32) {
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(dst.width as i32);
    let y1 = (y + h).min(dst.height as i32);
    let dw = dst.width as usize;
    for yy in y0..y1 {
        for xx in x0..x1 {
            let d = &mut dst.pixels[yy as usize * dw + xx as usize];
            *d = blend(*d, c);
        }
    }
}

pub fn stroke_rect(dst: &mut Image, x: i32, y: i32, w: i32, h: i32, t: i32, c: u32) {
    fill_rect(dst, x, y, w, t, c);
    fill_rect(dst, x, y + h - t, w, t, c);
    fill_rect(dst, x, y + t, t, h - 2 * t, c);
    fill_rect(dst, x + w - t, y + t, t, h - 2 * t, c);
}

/// Filled rounded rectangle with radius `r` (anti-aliased edges).
pub fn fill_round_rect(dst: &mut Image, x: i32, y: i32, w: i32, h: i32, r: i32, c: u32) {
    let (ca, cr, cg, cb) = split(c);
    let r = r.min(w / 2).min(h / 2).max(0) as f32;
    for yy in 0..h {
        for xx in 0..w {
            let px = xx as f32 + 0.5;
            let py = yy as f32 + 0.5;
            let dx = (r - px).max(px - (w as f32 - r)).max(0.0);
            let dy = (r - py).max(py - (h as f32 - r)).max(0.0);
            let d = (dx * dx + dy * dy).sqrt();
            let cov = (r - d + 0.5).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let a = (ca as f32 * cov) as u32;
            let px_ = argb(a as u8, cr as u8, cg as u8, cb as u8);
            if let Some(d) = dst.get(x + xx, y + yy) {
                dst.put(x + xx, y + yy, blend(d, px_));
            }
        }
    }
}

/// Filled disc (anti-aliased).
pub fn fill_circle(dst: &mut Image, cx: f32, cy: f32, rad: f32, c: u32) {
    let (ca, cr, cg, cb) = split(c);
    let x0 = (cx - rad - 1.0).floor() as i32;
    let x1 = (cx + rad + 1.0).ceil() as i32;
    let y0 = (cy - rad - 1.0).floor() as i32;
    let y1 = (cy + rad + 1.0).ceil() as i32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let cov = (rad - d + 0.5).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            if let Some(dp) = dst.get(x, y) {
                let a = (ca as f32 * cov) as u8;
                dst.put(x, y, blend(dp, argb(a, cr as u8, cg as u8, cb as u8)));
            }
        }
    }
}

/// Call `f` for points along a line, every `step` pixels (>= 1), excluding the start.
pub fn walk_line(x0: f32, y0: f32, x1: f32, y1: f32, step: f32, mut f: impl FnMut(f32, f32)) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx * dx + dy * dy).sqrt();
    let step = step.max(0.5);
    let n = (len / step).floor() as i32;
    for i in 1..=n {
        let t = i as f32 * step / len.max(1e-6);
        f(x0 + dx * t, y0 + dy * t);
    }
}

/// Nearest-neighbour scale of `src` into a u32 frame buffer (0RGB) with letterboxing.
/// Returns (offset_x, offset_y, scale).
pub fn present(src: &Image, out: &mut [u32], ow: u32, oh: u32, border: u32) -> (f32, f32, f32) {
    let scale = (ow as f32 / src.width as f32).min(oh as f32 / src.height as f32);
    let dw = (src.width as f32 * scale).round() as u32;
    let dh = (src.height as f32 * scale).round() as u32;
    let ox = (ow - dw.min(ow)) / 2;
    let oy = (oh - dh.min(oh)) / 2;
    let sw = src.width as usize;
    // Precompute x mapping.
    let xmap: Vec<usize> = (0..dw)
        .map(|x| ((x as f32 + 0.5) / scale) as usize)
        .map(|x| x.min(sw - 1))
        .collect();
    for y in 0..oh {
        let row = &mut out[(y * ow) as usize..((y + 1) * ow) as usize];
        if y < oy || y >= oy + dh {
            row.fill(border);
            continue;
        }
        let sy = (((y - oy) as f32 + 0.5) / scale) as usize;
        let sy = sy.min(src.height as usize - 1);
        let srow = &src.pixels[sy * sw..(sy + 1) * sw];
        row[..ox as usize].fill(border);
        for (i, &sx) in xmap.iter().enumerate() {
            row[ox as usize + i] = srow[sx] & 0x00ff_ffff;
        }
        row[(ox + dw) as usize..].fill(border);
    }
    (ox as f32, oy as f32, scale)
}

/// Bilinear resize (used for thumbnails / stamp sizes).
pub fn resize(src: &Image, w: u32, h: u32) -> Image {
    let mut out = Image::new(w.max(1), h.max(1));
    if src.width == 0 || src.height == 0 {
        return out;
    }
    for y in 0..out.height {
        for x in 0..out.width {
            let sx = ((x as f32 + 0.5) * src.width as f32 / out.width as f32 - 0.5).max(0.0);
            let sy = ((y as f32 + 0.5) * src.height as f32 / out.height as f32 - 0.5).max(0.0);
            let x0 = sx.floor() as i32;
            let y0 = sy.floor() as i32;
            let fx = sx - x0 as f32;
            let fy = sy - y0 as f32;
            let p = |xx: i32, yy: i32| {
                src.get(
                    xx.min(src.width as i32 - 1),
                    yy.min(src.height as i32 - 1),
                )
                .unwrap_or(0)
            };
            let c = [p(x0, y0), p(x0 + 1, y0), p(x0, y0 + 1), p(x0 + 1, y0 + 1)];
            let w = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
            let mut acc = [0f32; 4];
            for i in 0..4 {
                let (a, r, g, b) = split(c[i]);
                let af = a as f32 * w[i];
                acc[0] += af;
                acc[1] += r as f32 * af;
                acc[2] += g as f32 * af;
                acc[3] += b as f32 * af;
            }
            let a = acc[0];
            out.pixels[(y * out.width + x) as usize] = if a < 0.5 {
                0
            } else {
                argb(
                    a.round() as u8,
                    (acc[1] / a).round() as u8,
                    (acc[2] / a).round() as u8,
                    (acc[3] / a).round() as u8,
                )
            };
        }
    }
    out
}

pub fn hsv(h: f32, s: f32, v: f32) -> u32 {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    rgb(
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    )
}

/// Integer nearest-neighbour upscale.
pub fn resize_nearest(src: &Image, s: u32) -> Image {
    let s = s.max(1);
    let mut out = Image::new(src.width * s, src.height * s);
    for y in 0..out.height {
        for x in 0..out.width {
            out.pixels[(y * out.width + x) as usize] = src.pixels[((y / s) * src.width + x / s) as usize];
        }
    }
    out
}
