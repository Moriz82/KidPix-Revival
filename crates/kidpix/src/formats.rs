//! Picture and sound files from outside Kid Pix (Add Graphic, Import Background,
//! Import Sound, Export).

use crate::gfx::Image;
use mohawk::Sound;
use std::path::Path;

/// Load a BMP, GIF, JPEG or PNG picture.
pub fn load_image(path: &Path) -> Result<Image, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.starts_with(b"\x89PNG") {
        return load_png(&data);
    }
    if data.starts_with(&[0xff, 0xd8]) {
        return load_jpeg(&data);
    }
    if data.starts_with(b"BM") {
        return load_bmp(&data);
    }
    if data.starts_with(b"GIF8") {
        return load_gif(&data);
    }
    Err("Kid Pix can't read that kind of picture (use BMP, GIF, JPEG or PNG).".into())
}

fn load_png(data: &[u8]) -> Result<Image, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Indexed => return Err("unsupported PNG".into()),
    };
    Ok(Image::from_rgba8(info.width, info.height, &rgba))
}

fn load_jpeg(data: &[u8]) -> Result<Image, String> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let mut d = zune_jpeg::JpegDecoder::new_with_options(data, DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB));
    let rgb = d.decode().map_err(|e| format!("{e:?}"))?;
    let info = d.info().ok_or("bad JPEG")?;
    let px = rgb.chunks_exact(3).map(|c| 0xff00_0000 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32).collect();
    Ok(Image { width: info.width as u32, height: info.height as u32, pixels: px })
}

fn load_bmp(d: &[u8]) -> Result<Image, String> {
    let u32le = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
    let u16le = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
    if d.len() < 54 {
        return Err("bad BMP".into());
    }
    let off = u32le(10) as usize;
    let hsz = u32le(14) as usize;
    let w = u32le(18) as i32;
    let h_raw = u32le(22) as i32;
    let bpp = u16le(28);
    let comp = u32le(30);
    if comp != 0 && comp != 3 {
        return Err("compressed BMP pictures aren't supported".into());
    }
    let (h, top_down) = if h_raw < 0 { (-h_raw, true) } else { (h_raw, false) };
    let ncol = if bpp <= 8 { (match u32le(46) { 0 => 1u32 << bpp, n => n }) as usize } else { 0 };
    let pal: Vec<u32> = (0..ncol)
        .map(|i| {
            let o = 14 + hsz + i * 4;
            0xff00_0000 | (d[o + 2] as u32) << 16 | (d[o + 1] as u32) << 8 | d[o] as u32
        })
        .collect();
    let stride = ((w as usize * bpp as usize + 31) / 32) * 4;
    let mut img = Image::new(w as u32, h as u32);
    for y in 0..h as usize {
        let row = if top_down { y } else { h as usize - 1 - y };
        let base = off + row * stride;
        for x in 0..w as usize {
            let p = match bpp {
                1 | 4 | 8 => {
                    let bit = x * bpp as usize;
                    let byte = *d.get(base + bit / 8).ok_or("truncated BMP")?;
                    let idx = (byte >> (8 - bpp as usize - bit % 8)) & ((1u16 << bpp) - 1) as u8;
                    *pal.get(idx as usize).unwrap_or(&0xff00_0000)
                }
                16 => {
                    let v = u16le(base + x * 2) as u32;
                    let (r, g, b) = ((v >> 10) & 31, (v >> 5) & 31, v & 31);
                    0xff00_0000 | (r << 3) << 16 | (g << 3) << 8 | b << 3
                }
                24 | 32 => {
                    let o = base + x * (bpp as usize / 8);
                    if o + 2 >= d.len() {
                        return Err("truncated BMP".into());
                    }
                    0xff00_0000 | (d[o + 2] as u32) << 16 | (d[o + 1] as u32) << 8 | d[o] as u32
                }
                _ => return Err(format!("{bpp}-bit BMP not supported")),
            };
            img.pixels[y * w as usize + x] = p;
        }
    }
    Ok(img)
}

fn load_gif(data: &[u8]) -> Result<Image, String> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opts.read_info(std::io::Cursor::new(data)).map_err(|e| e.to_string())?;
    let (w, h) = (dec.width() as u32, dec.height() as u32);
    let frame = dec.read_next_frame().map_err(|e| e.to_string())?.ok_or("empty GIF")?;
    let mut img = Image::new(w, h);
    let fi = Image::from_rgba8(frame.width as u32, frame.height as u32, &frame.buffer);
    crate::gfx::blit(&mut img, &fi, frame.left as i32, frame.top as i32);
    Ok(img)
}

/// Read a WAV (PCM 8/16-bit, mono/stereo).
pub fn load_wav(path: &Path) -> Result<Sound, String> {
    let d = std::fs::read(path).map_err(|e| e.to_string())?;
    if d.len() < 12 || &d[0..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return Err("Kid Pix can only import WAV sounds.".into());
    }
    let mut pos = 12;
    let (mut ch, mut rate, mut bits, mut fmt) = (1u16, 22050u32, 16u16, 1u16);
    while pos + 8 <= d.len() {
        let id = &d[pos..pos + 4];
        let len = u32::from_le_bytes([d[pos + 4], d[pos + 5], d[pos + 6], d[pos + 7]]) as usize;
        let body = &d[pos + 8..(pos + 8 + len).min(d.len())];
        match id {
            b"fmt " if body.len() >= 16 => {
                fmt = u16::from_le_bytes([body[0], body[1]]);
                ch = u16::from_le_bytes([body[2], body[3]]).max(1);
                rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                bits = u16::from_le_bytes([body[14], body[15]]);
            }
            b"data" => {
                if fmt != 1 {
                    return Err("only uncompressed (PCM) WAV sounds can be imported".into());
                }
                let samples = match bits {
                    8 => body.iter().map(|&b| ((b as i16) - 128) << 8).collect(),
                    16 => body.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect(),
                    _ => return Err(format!("{bits}-bit WAV not supported")),
                };
                return Ok(Sound { sample_rate: rate, channels: ch, samples, loop_count: 0 });
            }
            _ => {}
        }
        pos += 8 + len + (len & 1);
    }
    Err("the WAV file has no sound data".into())
}

/// Write the picture as PNG, JPEG or BMP depending on the file extension.
pub fn export(img: &Image, path: &Path) -> Result<(), String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => {
            let rgb: Vec<u8> = img.pixels.iter().flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8]).collect();
            jpeg_encoder::Encoder::new_file(path, 90)
                .map_err(|e| e.to_string())?
                .encode(&rgb, img.width as u16, img.height as u16, jpeg_encoder::ColorType::Rgb)
                .map_err(|e| e.to_string())
        }
        "bmp" => {
            let stride = (img.width as usize * 3 + 3) & !3;
            let size = 54 + stride * img.height as usize;
            let mut b = Vec::with_capacity(size);
            b.extend_from_slice(b"BM");
            b.extend_from_slice(&(size as u32).to_le_bytes());
            b.extend_from_slice(&[0; 4]);
            b.extend_from_slice(&54u32.to_le_bytes());
            b.extend_from_slice(&40u32.to_le_bytes());
            b.extend_from_slice(&(img.width as i32).to_le_bytes());
            b.extend_from_slice(&(img.height as i32).to_le_bytes());
            b.extend_from_slice(&1u16.to_le_bytes());
            b.extend_from_slice(&24u16.to_le_bytes());
            b.extend_from_slice(&[0; 24]);
            for y in (0..img.height as usize).rev() {
                let start = b.len();
                for x in 0..img.width as usize {
                    let p = img.pixels[y * img.width as usize + x];
                    b.extend_from_slice(&[p as u8, (p >> 8) as u8, (p >> 16) as u8]);
                }
                b.resize(start + stride, 0);
            }
            std::fs::write(path, b).map_err(|e| e.to_string())
        }
        _ => crate::save_png(path, img),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bmp_and_jpeg_roundtrip() {
        let mut img = Image::filled(31, 17, 0xff10_2030);
        img.put(3, 4, 0xffff_0000);
        let dir = std::env::temp_dir();
        let b = dir.join("kpr-rt.bmp");
        export(&img, &b).unwrap();
        let back = load_image(&b).unwrap();
        assert_eq!((back.width, back.height), (31, 17));
        assert_eq!(back.get(3, 4), Some(0xffff_0000));
        assert_eq!(back.get(0, 0), Some(0xff10_2030));
        let j = dir.join("kpr-rt.jpg");
        export(&img, &j).unwrap();
        let back = load_image(&j).unwrap();
        assert_eq!((back.width, back.height), (31, 17));
    }

    #[test]
    fn wav_import() {
        let mut d = Vec::new();
        d.extend_from_slice(b"RIFF");
        d.extend_from_slice(&40u32.to_le_bytes());
        d.extend_from_slice(b"WAVEfmt ");
        d.extend_from_slice(&16u32.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&11025u32.to_le_bytes());
        d.extend_from_slice(&22050u32.to_le_bytes());
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&16u16.to_le_bytes());
        d.extend_from_slice(b"data");
        d.extend_from_slice(&4u32.to_le_bytes());
        d.extend_from_slice(&100i16.to_le_bytes());
        d.extend_from_slice(&(-7i16).to_le_bytes());
        let p = std::env::temp_dir().join("kpr-t.wav");
        std::fs::write(&p, d).unwrap();
        let s = load_wav(&p).unwrap();
        assert_eq!((s.sample_rate, s.samples.clone()), (11025, vec![100, -7]));
    }
}

/// Write 16-bit PCM WAV.
pub fn save_wav(path: &Path, s: &mohawk::Sound) -> Result<(), String> {
    let data_len = (s.samples.len() * 2) as u32;
    let ch = s.channels.max(1) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(ch as u16).to_le_bytes());
    out.extend_from_slice(&s.sample_rate.to_le_bytes());
    out.extend_from_slice(&(s.sample_rate * ch * 2).to_le_bytes());
    out.extend_from_slice(&((ch * 2) as u16).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for v in &s.samples {
        out.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}
