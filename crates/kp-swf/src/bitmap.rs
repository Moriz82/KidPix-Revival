//! DefineBitsLossless(2) and DefineBits/JPEG2/3/4 decoding into premultiplied pixmaps.

use tiny_skia::{IntSize, Pixmap};

use crate::reader::{Reader, Result};
use crate::Error;

fn inflate(data: &[u8]) -> Result<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_zlib(data)
        .map_err(|e| Error::Decompress(format!("{:?}", e.status)))
}

fn pixmap(width: u32, height: u32, premul_rgba: Vec<u8>) -> Result<Pixmap> {
    let size = IntSize::from_wh(width, height)
        .ok_or_else(|| Error::Malformed(format!("bitmap size {width}x{height}")))?;
    Pixmap::from_vec(premul_rgba, size).ok_or_else(|| Error::Malformed("bitmap buffer size".into()))
}

#[inline]
fn premul(c: u8, a: u8) -> u8 {
    ((u16::from(c) * u16::from(a) + 127) / 255) as u8
}

/// DefineBitsLossless (`v2 == false`) / DefineBitsLossless2 (`v2 == true`).
/// Lossless2 stores alpha-premultiplied ARGB.
pub(crate) fn read_lossless(r: &mut Reader, v2: bool) -> Result<(u16, Pixmap)> {
    let id = r.u16()?;
    let format = r.u8()?;
    let w = u32::from(r.u16()?);
    let h = u32::from(r.u16()?);
    if w == 0 || h == 0 {
        return Err(Error::Malformed("empty bitmap".into()));
    }
    let (wu, hu) = (w as usize, h as usize);
    let mut out = vec![0u8; wu * hu * 4];
    match format {
        3 => {
            let colors = r.u8()? as usize + 1;
            let data = inflate(r.rest())?;
            let entry = if v2 { 4 } else { 3 };
            let table_len = colors * entry;
            if data.len() < table_len {
                return Err(Error::Truncated);
            }
            let mut table = vec![[0u8; 4]; 256];
            for (i, c) in data[..table_len].chunks_exact(entry).enumerate() {
                table[i] = if v2 {
                    let a = c[3];
                    [c[0].min(a), c[1].min(a), c[2].min(a), a]
                } else {
                    [c[0], c[1], c[2], 255]
                };
            }
            let stride = (wu + 3) & !3;
            let px = &data[table_len..];
            for y in 0..hu {
                let row = px
                    .get(y * stride..y * stride + wu)
                    .ok_or(Error::Truncated)?;
                for (x, &i) in row.iter().enumerate() {
                    let o = (y * wu + x) * 4;
                    out[o..o + 4].copy_from_slice(&table[i as usize]);
                }
            }
        }
        4 if !v2 => {
            let data = inflate(r.rest())?;
            let stride = (wu * 2 + 3) & !3;
            for y in 0..hu {
                let row = data
                    .get(y * stride..y * stride + wu * 2)
                    .ok_or(Error::Truncated)?;
                for (x, p) in row.chunks_exact(2).enumerate() {
                    let v = u16::from_be_bytes([p[0], p[1]]);
                    let c5 = |s: u16| {
                        let c = ((v >> s) & 31) as u8;
                        (c << 3) | (c >> 2)
                    };
                    let o = (y * wu + x) * 4;
                    out[o..o + 4].copy_from_slice(&[c5(10), c5(5), c5(0), 255]);
                }
            }
        }
        5 => {
            let data = inflate(r.rest())?;
            if data.len() < wu * hu * 4 {
                return Err(Error::Truncated);
            }
            for (o, p) in out.chunks_exact_mut(4).zip(data.chunks_exact(4)) {
                let a = if v2 { p[0] } else { 255 };
                o.copy_from_slice(&[p[1].min(a), p[2].min(a), p[3].min(a), a]);
            }
        }
        _ => return Err(Error::Malformed(format!("lossless bitmap format {format}"))),
    }
    Ok((id, pixmap(w, h, out)?))
}

/// Remove the bogus EOI+SOI pairs SWF encoders put between tables and image data.
fn clean_jpeg(parts: &[&[u8]]) -> Vec<u8> {
    let mut joined: Vec<u8> = parts.concat();
    while joined.starts_with(&[0xff, 0xd9, 0xff, 0xd8]) {
        joined.drain(..4);
    }
    let mut out = Vec::with_capacity(joined.len());
    let mut i = 0;
    while i < joined.len() {
        if i > 0 && joined[i..].starts_with(&[0xff, 0xd9, 0xff, 0xd8]) {
            i += 4;
            continue;
        }
        out.push(joined[i]);
        i += 1;
    }
    out
}

fn decode_jpeg(jpeg: &[u8], alpha: Option<&[u8]>) -> Result<Pixmap> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    if jpeg.starts_with(b"\x89PNG") || jpeg.starts_with(b"GIF8") {
        return Err(Error::Unsupported("PNG/GIF data in DefineBitsJPEG"));
    }
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut dec = zune_jpeg::JpegDecoder::new_with_options(jpeg, opts);
    let rgb = dec
        .decode()
        .map_err(|e| Error::Malformed(format!("jpeg: {e:?}")))?;
    let info = dec
        .info()
        .ok_or_else(|| Error::Malformed("jpeg: no info".into()))?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    let n = w as usize * h as usize;
    if rgb.len() < n * 3 {
        return Err(Error::Truncated);
    }
    let mut out = vec![0u8; n * 4];
    for (i, (o, c)) in out.chunks_exact_mut(4).zip(rgb.chunks_exact(3)).enumerate() {
        let a = alpha.and_then(|a| a.get(i).copied()).unwrap_or(255);
        o.copy_from_slice(&[premul(c[0], a), premul(c[1], a), premul(c[2], a), a]);
    }
    pixmap(w, h, out)
}

/// DefineBits (6) / DefineBitsJPEG2 (21) / JPEG3 (35) / JPEG4 (90).
pub(crate) fn read_jpeg(r: &mut Reader, code: u16, tables: Option<&[u8]>) -> Result<(u16, Pixmap)> {
    let id = r.u16()?;
    let pix = match code {
        6 => {
            let data = r.rest();
            let jpeg = clean_jpeg(&[tables.unwrap_or(&[]), data]);
            decode_jpeg(&jpeg, None)?
        }
        21 => decode_jpeg(&clean_jpeg(&[r.rest()]), None)?,
        _ => {
            let alpha_off = r.u32()? as usize;
            if code == 90 {
                r.u16()?; // deblocking filter
            }
            let jpeg = r.bytes(alpha_off)?;
            let alpha = r.rest();
            let alpha = if alpha.is_empty() {
                None
            } else {
                Some(inflate(alpha)?)
            };
            decode_jpeg(&clean_jpeg(&[jpeg]), alpha.as_deref())?
        }
    };
    Ok((id, pix))
}
