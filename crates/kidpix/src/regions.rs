//! Cookie-cutter shapes (`Data/Tools/CutRgns.MHK` MRGN): a Broderbund `rgnR` record —
//! `'rgnR', u32 0, u16 width, u16 height, u16 n, u16 n, n x {u16 left, top, right, bottom}`,
//! the union of the rectangles being the shape.

use crate::assets::Assets;
use crate::gfx::Image;

pub const CUT_ARCHIVE: &str = "Data/Tools/CutRgns.MHK";

pub struct Region {
    pub width: u32,
    pub height: u32,
    pub mask: Vec<bool>,
}

impl Region {
    pub fn parse(d: &[u8]) -> Option<Region> {
        if d.len() < 16 || &d[0..4] != b"rgnR" {
            return None;
        }
        let u = |o: usize| u16::from_be_bytes([d[o], d[o + 1]]) as usize;
        let (w, h, n) = (u(8), u(10), u(12));
        if w == 0 || h == 0 || w > 2048 || h > 2048 {
            return None;
        }
        let mut mask = vec![false; w * h];
        for i in 0..n {
            let o = 16 + i * 8;
            if o + 8 > d.len() {
                break;
            }
            let (l, t, r, b) = (u(o), u(o + 2), u(o + 4).min(w), u(o + 6).min(h));
            for y in t..b {
                for x in l..r {
                    mask[y * w + x] = true;
                }
            }
        }
        Some(Region { width: w as u32, height: h as u32, mask })
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height && self.mask[(y as u32 * self.width + x as u32) as usize]
    }

    pub fn to_image(&self, color: u32) -> Image {
        Image { width: self.width, height: self.height, pixels: self.mask.iter().map(|&m| if m { color } else { 0 }).collect() }
    }
}

pub fn load(assets: &Assets, id: u16) -> Option<Region> {
    let a = assets.archive(CUT_ARCHIVE)?;
    Region::parse(a.get(mohawk::Tag(*b"MRGN"), id)?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn square_region() {
        let mut d = b"rgnR".to_vec();
        d.extend([0, 0, 0, 0, 0, 4, 0, 3, 0, 1, 0, 1, 0, 1, 0, 0, 0, 3, 0, 2]);
        let r = super::Region::parse(&d).unwrap();
        assert_eq!((r.width, r.height), (4, 3));
        assert!(r.contains(1, 0) && r.contains(2, 1) && !r.contains(0, 0) && !r.contains(1, 2));
    }
}
