//! The Big Mixer's 22 effects (docs/RE_PANELS.md §3.4), on 5-bit channels with the
//! original's integer HSV. `alt` = Space held at click time.

use crate::gfx::Image;
use crate::paint::Rng;

pub const NAMES: [&str; 22] = [
    "XRay", "Nuclear", "Emboss", "Lighten", "Sepia", "Colorize", "ColorCycle", "StripColor", "Pointilize", "Posterize", "Solarize",
    "Pinch", "Spherize", "Ripple", "Bubbles", "Burlap", "Whirl", "Pulse", "Wavelet", "Shear", "Flip", "Kaleidoscope",
];

fn muldiv(a: i32, b: i32, c: i32) -> i32 {
    if c == 0 {
        0
    } else {
        (a * b + c / 2) / c
    }
}

fn rgb_to_hsv(r: i32, g: i32, b: i32) -> (i32, i32, i32) {
    let v = r.max(g).max(b);
    let d = v - r.min(g).min(b);
    let s = if v != 0 { muldiv(31, d, v) } else { 0 };
    let h = if d == 0 {
        0
    } else if r == v {
        muldiv(60, g - b, d)
    } else if g == v {
        muldiv(60, b - r, d) + 120
    } else {
        muldiv(60, r - g, d) + 240
    };
    (h.rem_euclid(360), s, v)
}

fn hsv_to_rgb(h: i32, s: i32, v: i32) -> (i32, i32, i32) {
    let (h, s, v) = (h.rem_euclid(360), s.clamp(0, 31), v.clamp(0, 31));
    let p = muldiv(31 - s, v, 31);
    let f = h % 60;
    let up = p + muldiv(f, v - p, 60);
    let down = p + muldiv(60 - f, v - p, 60);
    let (r, g, b) = match h / 60 {
        0 => (v, up, p),
        1 => (down, v, p),
        2 => (p, v, up),
        3 => (p, down, v),
        4 => (up, p, v),
        _ => (v, p, down),
    };
    (r.clamp(0, 31), g.clamp(0, 31), b.clamp(0, 31))
}

fn split5(c: u32) -> (i32, i32, i32) {
    (((c >> 19) & 31) as i32, ((c >> 11) & 31) as i32, ((c >> 3) & 31) as i32)
}

fn join5(r: i32, g: i32, b: i32) -> u32 {
    let e = |v: i32| {
        let v = v.clamp(0, 31) as u32;
        v << 3 | v >> 2
    };
    0xff00_0000 | e(r) << 16 | e(g) << 8 | e(b)
}

fn rand_in(rng: &mut Rng, a: i32, b: i32) -> i32 {
    a + rng.below((b - a + 1).max(1) as u32) as i32
}

/// Apply effect `i` (0..22) to the whole picture, centred on `c` for the geometric ones.
pub fn apply(img: &mut Image, i: usize, c: (i32, i32), size: i32, alt: bool, color: u32, rng: &mut Rng) {
    let (w, h) = (img.width as i32, img.height as i32);
    let s = size.clamp(1, 3);
    match i {
        0..=10 => {
            let (cr, cg, cb) = split5(color);
            let (ch, cs, _) = rgb_to_hsv(cr, cg, cb);
            let src = img.clone();
            for y in 0..h {
                for x in 0..w {
                    let (r, g, b) = split5(src.pixels[(y * w + x) as usize]);
                    let (hh, ss, vv) = rgb_to_hsv(r, g, b);
                    let out = match i {
                        0 => {
                            let (r, g, b) = if alt { hsv_to_rgb(hh + 180, ss, vv) } else { (r, g, b) };
                            join5(31 - r, 31 - g, 31 - b)
                        }
                        1 => {
                            let (r, g, b) = hsv_to_rgb(hh + 180, ss, vv);
                            if alt {
                                join5(31 - r, 31 - g, 31 - b)
                            } else {
                                join5(r, g, b)
                            }
                        }
                        2 => {
                            let (nx, ny) = ((x + 2).min(w - 1), (y + 2).min(h - 1));
                            let (r2, g2, b2) = split5(src.pixels[(ny * w + nx) as usize]);
                            let v2 = r2.max(g2).max(b2);
                            let (r, g, b) = hsv_to_rgb(hh, 0, (vv - v2 + 15).clamp(0, 31));
                            join5(r, g, b)
                        }
                        3 => {
                            let d = if alt { -1 } else { 1 };
                            join5(r + d, g + d, b + d)
                        }
                        4 => {
                            let (r, g, b) = hsv_to_rgb(27, 15, if vv < 30 { vv + 2 } else { vv });
                            join5(r, g, b)
                        }
                        5 => {
                            let (r, g, b) = hsv_to_rgb(ch, cs, vv);
                            join5(r, g, b)
                        }
                        6 => {
                            let (r, g, b) = hsv_to_rgb(hh + if alt { 330 } else { 30 }, ss, vv);
                            join5(r, g, b)
                        }
                        7 => {
                            let (r, g, b) = hsv_to_rgb(hh, if alt { ss - 1 } else { 0 }, vv);
                            join5(r, g, b)
                        }
                        8 => {
                            let (dh, dv) = if alt { (rand_in(rng, -30, 30), rand_in(rng, 0, 31)) } else { (rand_in(rng, -60, 60), rand_in(rng, 0, 15)) };
                            let (r, g, b) = hsv_to_rgb(hh + dh, ss, vv + dv);
                            join5(r, g, b)
                        }
                        9 => {
                            let q = if alt { 2 } else { 8 };
                            let (r, g, b) = hsv_to_rgb(hh, ss, (((vv + q) / q) * q).min(31));
                            join5(r, g, b)
                        }
                        _ => {
                            let mut v = vv;
                            if v > 15 {
                                v = 31 - v;
                            }
                            let (r, g, b) = hsv_to_rgb(hh, ss, (2 * v).min(31));
                            join5(r, g, b)
                        }
                    };
                    img.pixels[(y * w + x) as usize] = out;
                }
            }
        }
        19 => {
            // Shear: shift each row.
            let src = img.clone();
            let m = if alt { 12 * s } else { 3 * s };
            for y in 0..h {
                let off = 2 * rand_in(rng, -m, m);
                for x in 0..w {
                    let sx = x - off;
                    img.pixels[(y * w + x) as usize] = if sx >= 0 && sx < w { src.pixels[(y * w + sx) as usize] } else { 0xffff_ffff };
                }
            }
        }
        20 => {
            let src = img.clone();
            for y in 0..h {
                for x in 0..w {
                    let (sx, sy) = if alt { (x, h - 1 - y) } else { (w - 1 - x, y) };
                    img.pixels[(y * w + x) as usize] = src.pixels[(sy * w + sx) as usize];
                }
            }
        }
        _ => polar(img, i, c, s, alt),
    }
}

fn polar(img: &mut Image, i: usize, c: (i32, i32), s: i32, alt: bool) {
    use std::f64::consts::PI;
    let (w, h) = (img.width as i32, img.height as i32);
    let rr = (w.max(h) / 2) as f64;
    let sf = s as f64;
    let src = img.clone();
    for y in 0..h {
        for x in 0..w {
            let mut dx = (x - c.0) as f64;
            let mut dy = (y - c.1) as f64;
            if !(dx == 0.0 && dy == 0.0) {
                let r = (dx * dx + dy * dy).sqrt();
                let th = dy.atan2(dx);
                match i {
                    11 => {
                        let r2 = (r * rr).sqrt();
                        if r2 < rr {
                            dx = r2 * th.cos();
                            dy = r2 * th.sin();
                        }
                    }
                    12 => {
                        if r < rr {
                            let r2 = r * r / rr;
                            dx = r2 * th.cos();
                            dy = r2 * th.sin();
                        }
                    }
                    13 => {
                        if alt {
                            let f = 5.0 * sf;
                            let t = (dx / f).sin() + (dx / f).cos() + (dy / f).cos() + (dy / f).sin();
                            let odx = dx;
                            dx += f * t;
                            let _ = odx;
                            dy += f * ((dx / f).sin() + (dx / f).cos() + (dy / f).cos() + (dy / f).sin());
                        } else {
                            let k = 8.0 / sf;
                            dx += PI * (dy / (k * PI)).cos();
                            dy += PI * (dx / (k * PI)).sin();
                        }
                    }
                    14 => {
                        if alt {
                            let f = 4.0 * sf;
                            dy += 4.0 * f * (dx / f).cos() * (dy / f).sin();
                        } else {
                            dy += 4.0 * sf * (dx / sf).cos() * (dy / sf).sin();
                        }
                    }
                    15 => {
                        let s2 = sf.max(1.0);
                        let m = if alt { 24.0 * s2 } else { 6.0 * s2 } as i64;
                        let a = (dx.trunc() as i64 % m) as f64;
                        dx += a.cos() * a;
                        let b = (dy.trunc() as i64 % m) as f64;
                        dy += b.sin() * b;
                    }
                    16 => {
                        let d = ((r * rr).sqrt() / r) * PI / (6.0 / sf);
                        let t2 = if alt { th - d } else { th + d };
                        dx = r * t2.cos();
                        dy = r * t2.sin();
                    }
                    17 => {
                        if dx != 0.0 {
                            let f = if alt { 100.0 * sf } else { 10.0 * sf };
                            dx += PI * (dy / (PI * dx / f)).cos();
                            if dy != 0.0 {
                                dy += PI * (dx / (PI * dy / f)).sin();
                            }
                        }
                    }
                    18 => {
                        if r < rr {
                            let d = sf * (2.0 * PI * r / rr).sin() / (2.0 * PI);
                            let t2 = if alt { th - d } else { th + d };
                            dx = r * t2.cos();
                            dy = r * t2.sin();
                        }
                    }
                    _ => {
                        // Kaleidoscope
                        let t2 = 2.0 * sf * th;
                        if alt {
                            let r2 = r / (sf * PI / 3.0);
                            dx = (t2.cos() - t2.sin()) * r2;
                            dy = (t2.cos() + t2.sin()) * r2;
                        } else {
                            let r2 = r / (sf * PI * 0.5);
                            dx = (t2.sin() + t2.cos()) * r2;
                            dy = (t2.sin() - t2.cos()) * r2;
                        }
                    }
                }
            }
            let sx = dx.trunc() as i32 + c.0;
            let sy = dy.trunc() as i32 + c.1;
            img.pixels[(y * w + x) as usize] = if sx > 0 && sx < w && sy > 0 && sy < h { src.pixels[(sy * w + sx) as usize] } else { 0xffff_ffff };
        }
    }
}
