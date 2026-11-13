//! Paint Can fills as the original does them (docs/RE_PANELS.md §3.3).

use crate::assets::Assets;
use crate::gfx::Image;
use mohawk::Indexed;

/// One gradient of TP `grad` 21300: type 2 horizontal, 1 vertical, 0 radial.
#[derive(Clone, Debug)]
pub struct Grad {
    pub kind: u16,
    /// (position 0..100, RGB888)
    pub stops: Vec<(u16, u32)>,
}

pub fn parse_grads(d: &[u8]) -> Vec<Grad> {
    let mut out = Vec::new();
    let mut o = 0;
    let u = |o: usize| u16::from_be_bytes([d[o], d[o + 1]]);
    while o + 4 <= d.len() {
        let kind = u(o);
        let n = u(o + 2) as usize;
        o += 4;
        let mut stops = Vec::new();
        for _ in 0..n {
            if o + 6 > d.len() {
                break;
            }
            stops.push((u(o).min(100), 0xff00_0000 | (d[o + 2] as u32) << 16 | (d[o + 3] as u32) << 8 | d[o + 4] as u32));
            o += 6;
        }
        stops.truncate(8);
        out.push(Grad { kind, stops });
    }
    out
}

pub fn gradient(assets: &Assets, i: usize) -> Option<Grad> {
    let a = assets.archive(crate::tools::TP)?;
    parse_grads(a.get(mohawk::Tag(*b"grad"), 21300)?).into_iter().nth(i)
}

fn rgb555(c: u32) -> u32 {
    ((c >> 19) & 31) << 10 | ((c >> 11) & 31) << 5 | (c >> 3) & 31
}

/// Connected pixels with exactly the same 15-bit colour as the click point (4-connected).
pub fn region(img: &Image, x: i32, y: i32) -> Vec<(i32, i32)> {
    let Some(seed) = img.get(x, y).map(rgb555) else { return vec![] };
    let (w, h) = (img.width as i32, img.height as i32);
    let mut seen = vec![false; (w * h) as usize];
    let mut out = Vec::new();
    let mut stack = vec![(x, y)];
    let same = |p: i32| rgb555(img.pixels[p as usize]) == seed;
    while let Some((sx, sy)) = stack.pop() {
        let i = sy * w + sx;
        if seen[i as usize] || !same(i) {
            continue;
        }
        let mut lx = sx;
        while lx > 0 && !seen[(sy * w + lx - 1) as usize] && same(sy * w + lx - 1) {
            lx -= 1;
        }
        let mut rx = sx;
        while rx < w - 1 && !seen[(sy * w + rx + 1) as usize] && same(sy * w + rx + 1) {
            rx += 1;
        }
        for xx in lx..=rx {
            seen[(sy * w + xx) as usize] = true;
            out.push((xx, sy));
            for ny in [sy - 1, sy + 1] {
                if ny >= 0 && ny < h && !seen[(ny * w + xx) as usize] && same(ny * w + xx) {
                    stack.push((xx, ny));
                }
            }
        }
    }
    out
}

fn to5(c: u32) -> (u32, u32, u32) {
    ((c >> 19) & 31, (c >> 11) & 31, (c >> 3) & 31)
}

fn from5(r: u32, g: u32, b: u32) -> u32 {
    let e = |v: u32| (v << 3 | v >> 2) & 255;
    0xff00_0000 | e(r) << 16 | e(g) << 8 | e(b)
}

/// Solid / single-colour texture: blend the colour over the picture by the texel value.
pub fn tint(img: &mut Image, region: &[(i32, i32)], tex: &Indexed, color: u32) {
    let (cr, cg, cb) = to5(color);
    let (tw, th) = (tex.width.max(1) as i32, tex.height.max(1) as i32);
    let w = img.width as i32;
    for &(x, y) in region {
        let g = tex.indices[(y.rem_euclid(th) * tw + x.rem_euclid(tw)) as usize] as u32;
        if g == 0 {
            continue;
        }
        let p = &mut img.pixels[(y * w + x) as usize];
        let (dr, dg, db) = to5(*p);
        let m = |d: u32, c: u32| (d * (255 - g) + c * g) >> 8;
        *p = from5(m(dr, cr), m(dg, cg), m(db, cb));
    }
}

/// Multi-colour pattern: tile the 16-bit pattern from the canvas origin.
pub fn pattern(img: &mut Image, region: &[(i32, i32)], pat: &Image) {
    let (tw, th) = (pat.width.max(1) as i32, pat.height.max(1) as i32);
    let w = img.width as i32;
    for &(x, y) in region {
        let p = pat.pixels[(y.rem_euclid(th) * tw + x.rem_euclid(tw)) as usize];
        if p >> 24 != 0 {
            img.pixels[(y * w + x) as usize] = p | 0xff00_0000;
        }
    }
}

fn lerp_c(a: u32, b: u32, t: f32) -> u32 {
    let f = |s: u32| ((a >> s & 255) as f32 * (1.0 - t) + (b >> s & 255) as f32 * t).round() as u32;
    0xff00_0000 | f(16) << 16 | f(8) << 8 | f(0)
}

fn stop_color(stops: &[(u16, u32)], t: f32) -> u32 {
    let p = t * 100.0;
    match stops {
        [] => 0xffff_ffff,
        [s] => s.1,
        _ => {
            if p <= stops[0].0 as f32 {
                return stops[0].1;
            }
            for w in stops.windows(2) {
                let (a, b) = (w[0], w[1]);
                if p <= b.0 as f32 {
                    let span = (b.0 as f32 - a.0 as f32).max(0.001);
                    return lerp_c(a.1, b.1, ((p - a.0 as f32) / span).clamp(0.0, 1.0));
                }
            }
            stops[stops.len() - 1].1
        }
    }
}

/// Gradient fill fitted to the region's bounding box; `focus` = click point (radial).
pub fn gradient_fill(img: &mut Image, region: &[(i32, i32)], g: &Grad, focus: (f32, f32)) {
    if region.is_empty() {
        return;
    }
    let (x0, y0, x1, y1) = crate::paint::region_bbox(region);
    let (bw, bh) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
    let w = img.width as i32;
    let c = (x0 as f32 + bw / 2.0, y0 as f32 + bh / 2.0);
    let r_big = ((bw / 2.0).powi(2) + (bh / 2.0).powi(2)).sqrt().max(1.0);
    // Radial: stop k is a circle of radius pos/100*R, centred between the focus and c.
    let circles: Vec<((f32, f32), f32, u32)> = g
        .stops
        .iter()
        .map(|&(pos, col)| {
            let k = pos as f32 / 100.0;
            ((c.0 + (focus.0 - c.0) * (1.0 - k), c.1 + (focus.1 - c.1) * (1.0 - k)), k * r_big, col)
        })
        .collect();
    let inside = |p: (f32, f32), cc: (f32, f32), r: f32| ((p.0 - cc.0).powi(2) + (p.1 - cc.1).powi(2)).sqrt() <= r;
    for &(x, y) in region {
        let col = match g.kind {
            2 => stop_color(&g.stops, (x - x0) as f32 / bw),
            1 => stop_color(&g.stops, (y - y0) as f32 / bh),
            _ => {
                let p = (x as f32 + 0.5, y as f32 + 0.5);
                match circles.iter().position(|&(cc, r, _)| inside(p, cc, r)) {
                    Some(0) => circles[0].2,
                    None => circles.last().map(|c| c.2).unwrap_or(0xffff_ffff),
                    Some(k) => {
                        // Find s in [0,1] where p lies on the circle between k-1 and k.
                        let (a, b) = (circles[k - 1], circles[k]);
                        let (mut lo, mut hi) = (0.0f32, 1.0f32);
                        for _ in 0..14 {
                            let m = (lo + hi) / 2.0;
                            let cc = (a.0 .0 + (b.0 .0 - a.0 .0) * m, a.0 .1 + (b.0 .1 - a.0 .1) * m);
                            if inside(p, cc, a.1 + (b.1 - a.1) * m) {
                                hi = m;
                            } else {
                                lo = m;
                            }
                        }
                        lerp_c(a.2, b.2, hi)
                    }
                }
            }
        };
        img.pixels[(y * w + x) as usize] = col;
    }
}
