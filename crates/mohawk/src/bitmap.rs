//! Mohawk bitmap decoding (`tBMP`, `tBMH`, `tTHM`, `tPAL`).

use crate::{bad, Error, Reader, Result};

/// ARGB8888 image with straight (non-premultiplied) alpha. `pixels[y * width + x]`.
#[derive(Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

impl Image {
    pub fn new(width: u32, height: u32) -> Image {
        Image {
            width,
            height,
            pixels: vec![0; (width as usize) * (height as usize)],
        }
    }

    pub fn filled(width: u32, height: u32, argb: u32) -> Image {
        Image {
            width,
            height,
            pixels: vec![argb; (width as usize) * (height as usize)],
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Option<u32> {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return None;
        }
        Some(self.pixels[y as usize * self.width as usize + x as usize])
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, argb: u32) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return;
        }
        let w = self.width as usize;
        self.pixels[y as usize * w + x as usize] = argb;
    }

    /// Straight RGBA8 bytes (for PNG export etc.).
    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for &p in &self.pixels {
            out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, (p >> 24) as u8]);
        }
        out
    }

    pub fn from_rgba8(width: u32, height: u32, rgba: &[u8]) -> Image {
        let pixels = rgba
            .chunks_exact(4)
            .map(|c| u32::from_be_bytes([c[3], c[0], c[1], c[2]]))
            .collect();
        Image {
            width,
            height,
            pixels,
        }
    }
}

/// 256-entry palette of ARGB colors.
#[derive(Clone, Debug)]
pub struct Palette(pub [u32; 256]);

impl Default for Palette {
    fn default() -> Self {
        // Greyscale ramp; index 0 black.
        let mut p = [0u32; 256];
        for (i, e) in p.iter_mut().enumerate() {
            let v = i as u32;
            *e = 0xff00_0000 | v << 16 | v << 8 | v;
        }
        Palette(p)
    }
}

impl Palette {
    /// Parse a `tPAL`: `u16 first, u16 count, count x {r, g, b, pad}`. Entries not covered
    /// are left as in `base`.
    pub fn parse_onto(base: &Palette, data: &[u8]) -> Result<Palette> {
        let mut r = Reader::new(data);
        let first = r.u16()? as usize;
        let count = r.u16()? as usize;
        let mut p = base.clone();
        for i in 0..count {
            let c = r.bytes(4)?;
            if let Some(e) = p.0.get_mut(first + i) {
                *e = 0xff00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32;
            }
        }
        Ok(p)
    }

    pub fn parse(data: &[u8]) -> Result<Palette> {
        Palette::parse_onto(&Palette::default(), data)
    }
}

/// Decoded 8-bit bitmap (indices), for brush masks and palette effects.
#[derive(Clone, Debug)]
pub struct Indexed {
    pub width: u32,
    pub height: u32,
    pub indices: Vec<u8>,
}

impl Indexed {
    pub fn to_image(&self, pal: &Palette) -> Image {
        Image {
            width: self.width,
            height: self.height,
            pixels: self.indices.iter().map(|&i| pal.0[i as usize]).collect(),
        }
    }
}

/// The 8-byte header shared by all Mohawk bitmaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitmapHeader {
    pub width: u16,
    pub height: u16,
    pub bytes_per_row: u16,
    pub format: u16,
}

impl BitmapHeader {
    pub fn parse(data: &[u8]) -> Result<BitmapHeader> {
        let mut r = Reader::new(data);
        Ok(BitmapHeader {
            width: r.u16()? & 0x3fff,
            height: r.u16()? & 0x3fff,
            bytes_per_row: r.u16()?,
            format: r.u16()?,
        })
    }
    pub fn bpp_code(&self) -> u16 {
        self.format & 7
    }
    pub fn draw(&self) -> u16 {
        self.format & 0xf0
    }
    pub fn pack(&self) -> u16 {
        self.format & 0xf00
    }
    pub fn is_jpeg(&self) -> bool {
        self.pack() == 0xf00
    }
}

#[inline]
pub fn rgb555_to_argb(v: u16) -> u32 {
    let r = ((v >> 10) & 31) as u32;
    let g = ((v >> 5) & 31) as u32;
    let b = (v & 31) as u32;
    // Expand 5-bit to 8-bit by bit replication.
    0xff00_0000 | ((r << 3 | r >> 2) << 16) | ((g << 3 | g >> 2) << 8) | (b << 3 | b >> 2)
}

/// Decode any Kid Pix bitmap to ARGB. 8-bit bitmaps use `palette` (greyscale if `None`).
pub fn decode_bitmap(data: &[u8], palette: Option<&Palette>) -> Result<Image> {
    let h = BitmapHeader::parse(data)?;
    if h.is_jpeg() {
        return decode_jpeg_bitmap(data);
    }
    match h.bpp_code() {
        2 => {
            let pal_default;
            let pal = match palette {
                Some(p) => p,
                None => {
                    pal_default = Palette::default();
                    &pal_default
                }
            };
            Ok(decode_indexed(data)?.to_image(pal))
        }
        3 => decode_16(data, &h),
        4 => decode_32(data, &h),
        _ => Err(Error::Unsupported(format!("bitmap format {:#06x}", h.format))),
    }
}

/// Decode an 8-bit bitmap to palette indices.
pub fn decode_indexed(data: &[u8]) -> Result<Indexed> {
    let h = BitmapHeader::parse(data)?;
    if h.bpp_code() != 2 {
        return Err(Error::Unsupported(format!(
            "not an 8-bit bitmap ({:#06x})",
            h.format
        )));
    }
    if h.format & 8 != 0 {
        return Err(Error::Unsupported("bitmap with embedded CLUT".into()));
    }
    let unpacked;
    let body: &[u8] = match h.pack() {
        0 => &data[8..],
        0x100 => {
            unpacked = unpack_lz(&data[8..])?;
            &unpacked
        }
        0x200 => {
            unpacked = unpack_lz_with_header(&data[8..], 14)?;
            &unpacked
        }
        // The original program refuses any other packing too (error 0x2a63).
        p => return Err(Error::Unsupported(format!("pack {p:#x}"))),
    };
    let (w, ht) = (h.width as usize, h.height as usize);
    let mut out = vec![0u8; w * ht];
    match h.draw() {
        0 => {
            let bpr = h.bytes_per_row as usize;
            for y in 0..ht {
                let row = body.get(y * bpr..y * bpr + w).unwrap_or(&[]);
                out[y * w..y * w + row.len()].copy_from_slice(row);
            }
        }
        0x10 => {
            let mut r = Reader::new(body);
            for y in 0..ht {
                let Ok(row_bytes) = r.u16() else { break };
                let start = r.pos;
                let mut x = 0usize;
                while x < w {
                    let code = r.u8()?;
                    let n = ((code & 0x7f) as usize + 1).min(w - x);
                    if code & 0x80 != 0 {
                        let v = r.u8()?;
                        out[y * w + x..y * w + x + n].fill(v);
                    } else {
                        let src = r.bytes((code & 0x7f) as usize + 1)?;
                        out[y * w + x..y * w + x + n].copy_from_slice(&src[..n]);
                    }
                    x += n;
                }
                r.pos = start + row_bytes as usize;
            }
        }
        d => return Err(Error::Unsupported(format!("8-bit draw mode {d:#x}"))),
    }
    Ok(Indexed {
        width: h.width as u32,
        height: h.height as u32,
        indices: out,
    })
}

fn decode_16(data: &[u8], h: &BitmapHeader) -> Result<Image> {
    let unpacked;
    let body: &[u8] = match h.pack() {
        0 => &data[8..],
        0x100 => {
            unpacked = unpack_lz(&data[8..])?;
            &unpacked
        }
        0x200 => {
            unpacked = unpack_lz_with_header(&data[8..], 14)?;
            &unpacked
        }
        // The original program refuses any other packing too (error 0x2a63).
        p => return Err(Error::Unsupported(format!("pack {p:#x}"))),
    };
    let (w, ht) = (h.width as usize, h.height as usize);
    let mut img = Image::new(w as u32, ht as u32);
    match h.draw() {
        0 => {
            let bpr = h.bytes_per_row as usize;
            for y in 0..ht {
                for x in 0..w {
                    let o = y * bpr + x * 2;
                    let Some(b) = body.get(o..o + 2) else { break };
                    img.pixels[y * w + x] = rgb555_to_argb(u16::from_be_bytes([b[0], b[1]]));
                }
            }
        }
        0x30 => decode_sprite_rle16(body, &mut img)?,
        d => return Err(Error::Unsupported(format!("16-bit draw mode {d:#x}"))),
    }
    Ok(img)
}

fn decode_32(data: &[u8], h: &BitmapHeader) -> Result<Image> {
    let unpacked;
    let body: &[u8] = match h.pack() {
        0 => &data[8..],
        0x100 => {
            unpacked = unpack_lz(&data[8..])?;
            &unpacked
        }
        0x200 => {
            unpacked = unpack_lz_with_header(&data[8..], 14)?;
            &unpacked
        }
        // The original program refuses any other packing too (error 0x2a63).
        p => return Err(Error::Unsupported(format!("pack {p:#x}"))),
    };
    let (w, ht) = (h.width as usize, h.height as usize);
    let mut img = Image::new(w as u32, ht as u32);
    let px = |b: &[u8]| 0xff00_0000 | (u32::from_be_bytes([b[0], b[1], b[2], b[3]]) & 0x00ff_ffff);
    match h.draw() {
        0 => {
            let bpr = h.bytes_per_row as usize;
            for y in 0..ht {
                for x in 0..w {
                    let o = y * bpr + x * 4;
                    let Some(b) = body.get(o..o + 4) else { break };
                    img.pixels[y * w + x] = px(b);
                }
            }
        }
        0x30 => {
            // Same row/command layout as the 16-bit sprite RLE, with 32-bit XRGB pixels;
            // a run carries a single pixel.
            let mut r = Reader::new(body);
            for y in 0..ht {
                if r.remaining() < 4 {
                    break;
                }
                let dwords = r.u16()? as usize;
                let mut x = r.u16()? as usize;
                let end = r.pos + dwords * 4;
                let row = &mut img.pixels[y * w..y * w + w];
                while r.pos + 4 <= end {
                    let cmd = r.u16()?;
                    let skip = r.u16()? as usize;
                    let count = (cmd & 0x7fff) as usize;
                    if cmd & 0x8000 != 0 {
                        let v = px(r.bytes(4)?);
                        for i in 0..count {
                            if let Some(p) = row.get_mut(x + i) {
                                *p = v;
                            }
                        }
                    } else {
                        let b = r.bytes(count * 4)?;
                        for i in 0..count {
                            if let Some(p) = row.get_mut(x + i) {
                                *p = px(&b[4 * i..4 * i + 4]);
                            }
                        }
                    }
                    x += count + skip;
                }
                r.pos = end;
            }
        }
        d => return Err(Error::Unsupported(format!("32-bit draw mode {d:#x}"))),
    }
    Ok(img)
}

/// 16bpp sprite RLE; see FORMATS.md.
fn decode_sprite_rle16(body: &[u8], img: &mut Image) -> Result<()> {
    let (w, ht) = (img.width as usize, img.height as usize);
    let mut r = Reader::new(body);
    for y in 0..ht {
        if r.remaining() < 4 {
            break; // trailing rows omitted = transparent
        }
        let dwords = r.u16()? as usize;
        let mut x = r.u16()? as usize;
        let end = r.pos + dwords * 4;
        let row = &mut img.pixels[y * w..y * w + w];
        while r.pos + 4 <= end {
            let cmd = r.u16()?;
            let skip = r.u16()? as usize;
            let count = (cmd & 0x7fff) as usize;
            if cmd & 0x8000 != 0 {
                let a = rgb555_to_argb(r.u16()?);
                let b = rgb555_to_argb(r.u16()?);
                for i in 0..count {
                    if let Some(p) = row.get_mut(x + i) {
                        *p = if i % 2 == 0 { a } else { b };
                    }
                }
            } else {
                let px = r.bytes(count * 2)?;
                for i in 0..count {
                    if let Some(p) = row.get_mut(x + i) {
                        *p = rgb555_to_argb(u16::from_be_bytes([px[2 * i], px[2 * i + 1]]));
                    }
                }
                if count % 2 == 1 {
                    r.skip(2)?; // pad to 32 bits
                }
            }
            x += count + skip;
        }
        r.pos = end;
    }
    Ok(())
}

fn decode_jpeg_bitmap(data: &[u8]) -> Result<Image> {
    // header(8) 'jpeg' u32 0, u32 decodedSize, u32 jpegLen, JFIF…
    let mut r = Reader::at(data, 8);
    let tag = r.bytes(4)?;
    if tag != b"jpeg" {
        return bad("JPEG bitmap without 'jpeg' tag");
    }
    r.u32()?;
    r.u32()?;
    let len = r.u32()? as usize;
    let start = r.pos;
    let end = (start + len).min(data.len());
    decode_jpeg(&data[start..end])
}

fn decode_jpeg(jpeg: &[u8]) -> Result<Image> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(jpeg, opts);
    let rgb = dec
        .decode()
        .map_err(|e| Error::Format(format!("jpeg: {e:?}")))?;
    let info = dec
        .info()
        .ok_or_else(|| Error::Format("jpeg: no info".into()))?;
    let (w, h) = (info.width as u32, info.height as u32);
    let pixels = rgb
        .chunks_exact(3)
        .map(|c| 0xff00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32)
        .collect();
    Ok(Image {
        width: w,
        height: h,
        pixels,
    })
}

/// Split a `tBMH` bitmap set and decode every member (16-bit sprites).
pub fn decode_bitmap_set(data: &[u8], palette: Option<&Palette>) -> Result<Vec<Image>> {
    decode_bitmap_set_masks(data, palette).map(|(imgs, _)| imgs)
}

/// Like [`decode_bitmap_set`], also telling which members are 8-bit masks (no palette):
/// their pixels are `alpha = value, rgb = 0`. The engine draws them as Tint masks in the
/// current colour.
pub fn decode_bitmap_set_masks(data: &[u8], palette: Option<&Palette>) -> Result<(Vec<Image>, Vec<bool>)> {
    let mut r = Reader::new(data);
    let count = r.u16()? as usize;
    r.skip(4)?;
    let set_format = r.u16()?;
    // Packed sets compress everything after the 8-byte set header; member offsets are
    // relative to the start of (header + unpacked body).
    let unpacked;
    let data: &[u8] = match set_format & 0xf00 {
        0 => data,
        0x100 | 0x200 => {
            let mut v = data[..8].to_vec();
            let hl = if set_format & 0xf00 == 0x100 { 10 } else { 14 };
            v.extend_from_slice(&unpack_lz_with_header(&data[8..], hl)?);
            unpacked = v;
            &unpacked
        }
        p => return Err(Error::Unsupported(format!("bitmap set pack {p:#x}"))),
    };
    let mut r = Reader::at(data, 8);
    let mut offsets = Vec::with_capacity(count + 1);
    for _ in 0..count {
        offsets.push(r.u32()? as usize);
    }
    offsets.push(data.len());
    let mut out = Vec::with_capacity(count);
    let mut masks = Vec::with_capacity(count);
    for i in 0..count {
        let (a, b) = (offsets[i], offsets[i + 1].max(offsets[i]));
        let Some(slice) = data.get(a..b.min(data.len())) else {
            return bad(format!("tBMH member {i} out of range"));
        };
        let h = BitmapHeader::parse(slice)?;
        let mut img = decode_bitmap(slice, palette)?;
        let mask = h.bpp_code() == 2 && palette.is_none();
        if mask {
            // 8-bit members without a palette are intensity masks (0 = transparent).
            for p in img.pixels.iter_mut() {
                *p = (*p & 0xff) << 24;
            }
        }
        out.push(img);
        masks.push(mask);
    }
    Ok((out, masks))
}

/// Mohawk LZSS decompression (header: u32 uncompressed, u32 compressed, u16 dict size).
/// The dictionary size selects the window (0x100..0x1000 → 8..12 position bits), as in the
/// original program.
pub fn unpack_lz(data: &[u8]) -> Result<Vec<u8>> {
    unpack_lz_with_header(data, 10)
}

/// `header_len` is 10 for pack type 0x100 and 14 for 0x200.
pub fn unpack_lz_with_header(data: &[u8], header_len: usize) -> Result<Vec<u8>> {
    const MIN_STRING: usize = 3;
    let mut r = Reader::new(data);
    let usize_ = r.u32()? as usize;
    let _csize = r.u32()?;
    let dict = r.u16()?;
    let pos_bits: u32 = match dict {
        0x100 => 8,
        0x200 => 9,
        0x400 => 10,
        0x800 => 11,
        0x1000 => 12,
        _ => return Err(Error::Unsupported(format!("LZ dictionary size {dict:#x}"))),
    };
    r.pos = header_len;
    let mask: usize = (1 << pos_bits) - 1;
    // Longest match = (1 << (16 - pos_bits)) + 2; the window starts that far before its end.
    let max_string: usize = (1 << (16 - pos_bits)) + MIN_STRING - 1;
    if usize_ > 64 << 20 {
        return bad("LZ output too large");
    }
    let mut out: Vec<u8> = Vec::with_capacity(usize_);
    let mut flags: u16 = 0;
    let mut insert: usize = 0;
    while r.remaining() > 0 && out.len() < usize_ {
        flags >>= 1;
        if flags & 0x100 == 0 {
            flags = r.u8()? as u16 | 0xff00;
        }
        if flags & 1 != 0 {
            out.push(r.u8()?);
            insert = (insert + 1) & mask;
        } else {
            let off_len = r.u16()? as usize;
            let len = (off_len >> pos_bits) + MIN_STRING;
            let pos = (off_len + max_string) & mask;
            let back = (insert.wrapping_sub(pos)) & mask;
            let start = out.len() as isize - back as isize;
            for i in 0..len {
                let idx = start + i as isize;
                let v = if idx >= 0 { out[idx as usize] } else { 0 };
                out.push(v);
            }
            insert = (insert + len) & mask;
        }
    }
    out.truncate(usize_);
    Ok(out)
}
