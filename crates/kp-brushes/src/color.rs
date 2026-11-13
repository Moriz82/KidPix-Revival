//! RGB555 colour handling and the integer HSV helpers (RE_BRUSHES §0, §3.9).
//!
//! The original program paints into a 16-bit (RGB555) GWorld. The engine's canvas is an
//! opaque ARGB8888 [`mohawk::Image`], so every brush reads a canvas pixel as 5-bit per
//! channel and writes it back expanded to 8 bits.
//!
//! Engine-wide convention (documented here once):
//!
//! * **palette colour → 5-bit** (what the pad engine does with the current colour):
//!   `c5 = (31*c8 + 127) / 255` ([`c8_to_c5`], `@0x4597a0`).
//! * **GDI colour → 5-bit** (pens/brushes of the Wacky Brushes): GDI drops the low three
//!   bits (`& 0xF8`) before writing to a 16-bit surface, i.e. `c5 = c8 >> 3` ([`gdi5`]).
//! * **canvas pixel → 5-bit**: `c5 = c8 >> 3` ([`argb_to_555`]). This is exact for every
//!   pixel a brush wrote (see next point) and is what a 16-bit surface does to 24-bit data.
//! * **5-bit → canvas**: bit replication `c8 = (c5 << 3) | (c5 >> 2)` ([`c5_to_c8`]), the
//!   same expansion `mohawk::rgb555_to_argb` uses for RGB555 bitmaps.

/// 5-bit RGB triple, each channel 0..=31 (wider `i32` so formulas can extrapolate).
pub type Rgb5 = [i32; 3];

pub const WHITE5: Rgb5 = [31, 31, 31];
pub const BLACK5: Rgb5 = [0, 0, 0];

/// Palette colour channel to 5 bits: `(31*c8 + 127) / 255` (integer division) [O].
#[inline]
pub fn c8_to_c5(c8: u8) -> u8 {
    ((31 * c8 as u32 + 127) / 255) as u8
}

/// Expand a 5-bit channel to 8 bits by bit replication.
#[inline]
pub fn c5_to_c8(c5: u8) -> u8 {
    let c = c5 & 31;
    (c << 3) | (c >> 2)
}

/// GDI pen/brush quantisation: `& 0xF8`, then the 16-bit surface keeps the top 5 bits.
#[inline]
pub fn gdi5(c8: u8) -> u8 {
    c8 >> 3
}

/// Current palette colour (24-bit) as the pad engine's 5-bit brush colour.
pub fn palette5(c: [u8; 3]) -> Rgb5 {
    [c8_to_c5(c[0]) as i32, c8_to_c5(c[1]) as i32, c8_to_c5(c[2]) as i32]
}

/// A colour as a GDI pen would put it on the 16-bit canvas.
pub fn gdi_rgb5(c: [u8; 3]) -> Rgb5 {
    [gdi5(c[0]) as i32, gdi5(c[1]) as i32, gdi5(c[2]) as i32]
}

/// Canvas ARGB pixel to an RGB555 word (`c8 >> 3` per channel).
#[inline]
pub fn argb_to_555(p: u32) -> u16 {
    let r = (p >> 19) & 31;
    let g = (p >> 11) & 31;
    let b = (p >> 3) & 31;
    (r << 10 | g << 5 | b) as u16
}

/// RGB555 word (bit 15 ignored) to an opaque ARGB pixel.
#[inline]
pub fn rgb555_to_argb(w: u16) -> u32 {
    mohawk::rgb555_to_argb(w & 0x7fff)
}

#[inline]
pub fn unpack555(w: u16) -> Rgb5 {
    [((w >> 10) & 31) as i32, ((w >> 5) & 31) as i32, (w & 31) as i32]
}

/// Pack with masking (normal case).
#[inline]
pub fn pack555(c: Rgb5) -> u16 {
    (((c[0] & 31) << 10) | ((c[1] & 31) << 5) | (c[2] & 31)) as u16
}

/// Pack the way the pad blend does it: each channel stored in a byte (wrapping), then
/// `b | (g | r << 5) << 5` in 16 bits, **without masking** — out-of-range channels bleed
/// into their neighbours exactly as in the original (`@0x44cc00`) [O].
#[inline]
pub fn pack555_unmasked(c: Rgb5) -> u16 {
    let r = (c[0] as u8) as u16;
    let g = (c[1] as u8) as u16;
    let b = (c[2] as u8) as u16;
    b | ((g | (r << 5)) << 5)
}

#[inline]
pub fn rgb5_to_argb(c: Rgb5) -> u32 {
    rgb555_to_argb(pack555(c))
}

#[inline]
pub fn argb_to_rgb5(p: u32) -> Rgb5 {
    unpack555(argb_to_555(p))
}

/// Read a canvas pixel as 5-bit RGB (`None` outside).
#[inline]
pub fn get5(img: &mohawk::Image, x: i32, y: i32) -> Option<Rgb5> {
    img.get(x, y).map(argb_to_rgb5)
}

/// Write a 5-bit colour (masked).
#[inline]
pub fn put5(img: &mut mohawk::Image, x: i32, y: i32, c: Rgb5) {
    img.put(x, y, rgb5_to_argb(c));
}

/// Write a raw RGB555 word (bit 15 dropped).
#[inline]
pub fn put555(img: &mut mohawk::Image, x: i32, y: i32, w: u16) {
    img.put(x, y, rgb555_to_argb(w));
}

#[inline]
pub fn clamp31(v: i32) -> i32 {
    v.clamp(0, 31)
}

/// `trunc(x)` like MSVC `__ftol` (toward zero), saturating instead of UB.
#[inline]
pub fn trunc(x: f64) -> i32 {
    x as i32
}

/// `round(x) = trunc(x + 0.5)` as in the spec.
#[inline]
pub fn round(x: f64) -> i32 {
    (x + 0.5) as i32
}

/// The pad engine's "store in a byte, clamp if > 31 as unsigned" (negative → 31).
#[inline]
pub fn byte_clamp31(v: i64) -> i32 {
    let b = v as u8;
    if b > 31 {
        31
    } else {
        b as i32
    }
}

/// Integer HSV, `@0x447ce0` [O]. Channels in any scale `max` (31 for pixels, 255 for the
/// 8-bit brush colour); `s` comes out 0..31, `h` 0..359, `v` in the input scale.
pub fn rgb2hsv(r: i32, g: i32, b: i32) -> (i32, i32, i32) {
    let v = r.max(g).max(b);
    let m = r.min(g).min(b);
    let d = v - m;
    let s = if v != 0 { (31 * d + v / 2) / v } else { 0 };
    let mut h = 0;
    if s >= 1 && d != 0 {
        if r == v {
            h = (60 * (g - b) + d / 2) / d;
        } else if g == v {
            h = (60 * (b - r) + d / 2) / d + 120;
        } else {
            h = (60 * (r - g) + d / 2) / d + 240;
        }
        if h < 0 {
            h += 360;
        }
        if h > 359 {
            h -= 360;
        }
    }
    (h, s, v)
}

/// Integer HSV back to RGB, `@0x447e10` [O].
pub fn hsv2rgb(h: i32, s: i32, v: i32) -> (i32, i32, i32) {
    let h = h.rem_euclid(360);
    let f = h % 60;
    let p = ((31 - s) * v + 15) / 31;
    let q = v - p;
    let rise = (f * q + 30) / 60 + p;
    let fall = ((60 - f) * q + 30) / 60 + p;
    match h / 60 {
        0 => (v, rise, p),
        1 => (fall, v, p),
        2 => (p, v, rise),
        3 => (p, fall, v),
        4 => (rise, p, v),
        _ => (v, p, fall),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantise_palette() {
        assert_eq!(c8_to_c5(0), 0);
        assert_eq!(c8_to_c5(255), 31);
        assert_eq!(c8_to_c5(4), 0);
        assert_eq!(c8_to_c5(5), 1); // (155+127)/255 = 1
        assert_eq!(c8_to_c5(128), 16); // (3968+127)/255 = 16
        assert_eq!(c8_to_c5(127), 15); // (3937+127)/255 = 15
        for c in 0..=255u8 {
            let q = c8_to_c5(c);
            assert!(q <= 31);
            // rounding to nearest: |q*255/31 - c| <= 255/62
            let back = q as f64 * 255.0 / 31.0;
            assert!((back - c as f64).abs() <= 255.0 / 62.0 + 1e-9, "{c} -> {q}");
        }
    }

    #[test]
    fn expand_roundtrip() {
        for c5 in 0..32u8 {
            let c8 = c5_to_c8(c5);
            assert_eq!(c8 >> 3, c5);
            assert_eq!(gdi5(c8), c5);
            assert_eq!(c8_to_c5(c8), c5, "palette quantiser must also round-trip {c5}");
        }
        assert_eq!(c5_to_c8(31), 255);
        assert_eq!(c5_to_c8(0), 0);
        let w = 0x7fff & ((3 << 10) | (17 << 5) | 30);
        assert_eq!(argb_to_555(rgb555_to_argb(w)), w);
    }

    #[test]
    fn unmasked_pack_bleeds() {
        assert_eq!(pack555_unmasked([1, 2, 3]), (1 << 10) | (2 << 5) | 3);
        // blue 32 carries into green
        assert_eq!(pack555_unmasked([0, 0, 32]), 32);
        assert_eq!(unpack555(pack555_unmasked([0, 0, 32])), [0, 1, 0]);
    }

    #[test]
    fn hsv_roundtrip_primaries() {
        assert_eq!(rgb2hsv(31, 0, 0), (0, 31, 31));
        assert_eq!(rgb2hsv(0, 31, 0), (120, 31, 31));
        assert_eq!(rgb2hsv(0, 0, 31), (240, 31, 31));
        assert_eq!(hsv2rgb(0, 31, 31), (31, 0, 0));
        assert_eq!(hsv2rgb(120, 31, 31), (0, 31, 0));
        assert_eq!(hsv2rgb(240, 31, 31), (0, 0, 31));
        let (h, s, v) = rgb2hsv(20, 20, 20);
        assert_eq!((h, s), (0, 0));
        assert_eq!(hsv2rgb(h, 0, v), (20, 20, 20));
    }
}
