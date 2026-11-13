//! `tCUR` – classic Mac `CURS` cursors.

use crate::{Image, Reader, Result};

#[derive(Clone, Debug)]
pub struct Cursor {
    /// 16x16 ARGB (black/white with transparency; inverted pixels drawn black).
    pub image: Image,
    pub hot_x: u16,
    pub hot_y: u16,
}

pub fn parse_cursor(data: &[u8]) -> Result<Cursor> {
    let mut r = Reader::new(data);
    let img = r.bytes(32)?;
    let mask = r.bytes(32)?;
    let hot_y = r.u16()?.min(15);
    let hot_x = r.u16()?.min(15);
    let mut image = Image::new(16, 16);
    for y in 0..16 {
        let ib = u16::from_be_bytes([img[2 * y], img[2 * y + 1]]);
        let mb = u16::from_be_bytes([mask[2 * y], mask[2 * y + 1]]);
        for x in 0..16 {
            let bit = 0x8000 >> x;
            let (d, m) = (ib & bit != 0, mb & bit != 0);
            let px = match (d, m) {
                (true, _) => 0xff00_0000,      // black
                (false, true) => 0xffff_ffff,  // white
                (false, false) => 0x0000_0000, // transparent
            };
            image.pixels[y * 16 + x] = px;
        }
    }
    Ok(Cursor { image, hot_x, hot_y })
}
