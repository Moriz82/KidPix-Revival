//! `tBMP` / `tTHM` rasters: Mohawk bitmaps in the JPEG flavour (format `0x0F03`).
//!
//! Layout (RE_SHELL.md §8.3), all big-endian:
//!
//! ```text
//! u16 width   u16 height   u16 bytesPerRow (= 2*width)   u16 format (= 0x0F03)
//! 'jpeg'  u32 0  u32 width*height*2  u32 jpegLength
//! <baseline JFIF JPEG, jpegLength bytes>
//! ```

use crate::{Error, Result};
use mohawk::Image;

/// Bitmap format word of a JPEG Mohawk bitmap.
pub const JPEG_FORMAT: u16 = 0x0F03;

/// Quality used when a raster has to be (re-)encoded. The original files use
/// near-lossless tables (DC 2, every AC coefficient 1, both tables); quality
/// 100 matches that and keeps generation loss low across repeated saves.
pub const JPEG_QUALITY: u8 = 100;

/// A page raster plus, when it came from a file, the exact bytes it was read from.
///
/// `image` may be modified freely. On write the encoder compares a fingerprint
/// of the current pixels with the one taken when the bytes were cached; if
/// they still match, the cached resource is emitted verbatim (this is what
/// makes unmodified pages round-trip byte-identically), otherwise the image is
/// re-encoded as JPEG.
#[derive(Clone, Debug)]
pub struct Raster {
    /// Decoded pixels (ARGB8888, alpha 0xFF).
    pub image: Image,
    cached: Option<Cached>,
}

#[derive(Clone, Debug)]
struct Cached {
    bytes: Vec<u8>,
    fingerprint: u64,
}

impl PartialEq for Raster {
    /// Rasters compare by pixels only; cached encodings are an implementation detail.
    fn eq(&self, other: &Self) -> bool {
        self.image == other.image
    }
}

impl Raster {
    /// Wrap a fresh image; it will be JPEG-encoded when the page is written.
    pub fn new(image: Image) -> Raster {
        Raster { image, cached: None }
    }

    /// Decode a `tBMP`/`tTHM` resource, keeping its bytes for verbatim rewrite.
    pub fn decode(resource: &[u8]) -> Result<Raster> {
        if resource.len() < 24 || &resource[8..12] != b"jpeg" {
            return Err(Error::Format("raster is not a JPEG Mohawk bitmap".into()));
        }
        let image = mohawk::decode_bitmap(resource, None).map_err(Error::Mohawk)?;
        let fingerprint = fingerprint(&image);
        Ok(Raster {
            image,
            cached: Some(Cached {
                bytes: resource.to_vec(),
                fingerprint,
            }),
        })
    }

    /// True when the pixels differ from the cached encoding (or none exists).
    pub fn is_modified(&self) -> bool {
        match &self.cached {
            Some(c) => c.fingerprint != fingerprint(&self.image),
            None => true,
        }
    }

    /// The resource bytes this raster was read from (or last committed to),
    /// whether or not the pixels have changed since.
    pub fn cached_bytes(&self) -> Option<&[u8]> {
        self.cached.as_ref().map(|c| c.bytes.as_slice())
    }

    /// Discard the cached encoding, forcing a re-encode on the next write.
    pub fn invalidate(&mut self) {
        self.cached = None;
    }

    /// Encode now and cache the result, so repeated writes reuse it.
    pub fn commit(&mut self) -> Result<()> {
        if self.is_modified() {
            let bytes = encode(&self.image)?;
            self.cached = Some(Cached {
                bytes,
                fingerprint: fingerprint(&self.image),
            });
        }
        Ok(())
    }

    /// The resource bytes to store: the cached ones if still valid, else a fresh encoding.
    pub fn to_resource(&self) -> Result<std::borrow::Cow<'_, [u8]>> {
        match &self.cached {
            Some(c) if c.fingerprint == fingerprint(&self.image) => Ok(std::borrow::Cow::Borrowed(&c.bytes)),
            _ => Ok(std::borrow::Cow::Owned(encode(&self.image)?)),
        }
    }
}

/// 64-bit FNV-1a over the dimensions and pixels.
fn fingerprint(img: &Image) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: u32| {
        for b in v.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    mix(img.width);
    mix(img.height);
    for &p in &img.pixels {
        mix(p);
    }
    h
}

/// Encode an image as a JPEG Mohawk bitmap resource.
pub fn encode(img: &Image) -> Result<Vec<u8>> {
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 || w > 0x3FFF || h > 0x3FFF {
        return Err(Error::TooLarge(format!("raster size {w}x{h} not storable")));
    }
    if img.pixels.len() != (w as usize) * (h as usize) {
        return Err(Error::Format(format!("image buffer does not match {w}x{h}")));
    }
    let rgb: Vec<u8> = img
        .pixels
        .iter()
        .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8])
        .collect();
    let mut jpeg = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut jpeg, JPEG_QUALITY);
    enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    enc.set_density(jpeg_encoder::Density::Inch { x: 72, y: 72 });
    enc.encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| Error::Image(format!("jpeg encode: {e}")))?;
    // The originals declare JFIF 1.01; the encoder writes 1.02. Match the originals.
    if jpeg.len() > 13 && &jpeg[6..11] == b"JFIF\0" {
        jpeg[11] = 1;
        jpeg[12] = 1;
    }

    let mut out = Vec::with_capacity(24 + jpeg.len());
    out.extend_from_slice(&(w as u16).to_be_bytes());
    out.extend_from_slice(&(h as u16).to_be_bytes());
    out.extend_from_slice(&((w * 2) as u16).to_be_bytes());
    out.extend_from_slice(&JPEG_FORMAT.to_be_bytes());
    out.extend_from_slice(b"jpeg");
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&(w * h * 2).to_be_bytes());
    out.extend_from_slice(&(jpeg.len() as u32).to_be_bytes());
    out.extend_from_slice(&jpeg);
    Ok(out)
}

/// Area-averaging downscale (used for the 96×72 thumbnail).
pub fn downscale(src: &Image, width: u32, height: u32) -> Image {
    let (sw, sh) = (src.width as u64, src.height as u64);
    let (dw, dh) = (width.max(1) as u64, height.max(1) as u64);
    let mut out = Image::new(dw as u32, dh as u32);
    if sw == 0 || sh == 0 {
        return out;
    }
    // Work in a fixed-point grid where one destination pixel spans `sw`
    // (resp. `sh`) units and one source pixel spans `dw` (resp. `dh`).
    for dy in 0..dh {
        let (y0, y1) = (dy * sh, (dy + 1) * sh);
        for dx in 0..dw {
            let (x0, x1) = (dx * sw, (dx + 1) * sw);
            let mut acc = [0u64; 4];
            let mut total = 0u64;
            let mut sy = y0 / dh;
            while sy * dh < y1 && sy < sh {
                let wy = (y1.min((sy + 1) * dh)) - (y0.max(sy * dh));
                let mut sx = x0 / dw;
                while sx * dw < x1 && sx < sw {
                    let wx = (x1.min((sx + 1) * dw)) - (x0.max(sx * dw));
                    let wgt = wx * wy;
                    let p = src.pixels[(sy * sw + sx) as usize];
                    for (k, a) in acc.iter_mut().enumerate() {
                        *a += ((p >> (24 - 8 * k)) & 0xFF) as u64 * wgt;
                    }
                    total += wgt;
                    sx += 1;
                }
                sy += 1;
            }
            let ch = |k: usize| ((acc[k] + total / 2) / total.max(1)) as u32;
            out.pixels[(dy * dw + dx) as usize] = ch(0) << 24 | ch(1) << 16 | ch(2) << 8 | ch(3);
        }
    }
    out
}
