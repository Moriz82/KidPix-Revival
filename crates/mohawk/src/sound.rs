//! `tWAV` Mohawk sounds.

use crate::{bad, Error, Reader, Result};

#[derive(Clone)]
pub struct Sound {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved signed 16-bit samples.
    pub samples: Vec<i16>,
    /// Loop count from the header (0 = play once, 0xffff = forever).
    pub loop_count: u16,
}

impl std::fmt::Debug for Sound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Sound({} Hz, {} ch, {} samples)",
            self.sample_rate,
            self.channels,
            self.samples.len()
        )
    }
}

impl Sound {
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / (self.sample_rate.max(1) as f32 * self.channels.max(1) as f32)
    }
}

pub fn decode_sound(data: &[u8]) -> Result<Sound> {
    let mut r = Reader::new(data);
    if r.bytes(4)? != b"MHWK" {
        return bad("sound without MHWK header");
    }
    r.u32()?;
    if r.bytes(4)? != b"WAVE" {
        return bad("sound without WAVE tag");
    }
    loop {
        let tag = r.bytes(4)?;
        let size = r.u32()? as usize;
        if tag != b"Data" {
            r.skip(size)?;
            continue;
        }
        let mut d = Reader::new(r.bytes(size.min(r.remaining()))?);
        let rate = d.u16()? as u32;
        let count = d.u32()? as usize;
        let bits = d.u8()?;
        let channels = d.u8()?.max(1) as u16;
        let encoding = d.u16()?;
        let loop_count = d.u16()?;
        let _loop_start = d.u32()?;
        let _loop_end = d.u32()?;
        let body = &d.data[d.pos..];
        let samples = match encoding {
            0 => match bits {
                8 => body.iter().take(count * channels as usize).map(|&b| ((b as i16) - 128) << 8).collect(),
                16 => body
                    .chunks_exact(2)
                    .take(count * channels as usize)
                    .map(|c| i16::from_be_bytes([c[0], c[1]]))
                    .collect(),
                b => return Err(Error::Unsupported(format!("{b}-bit PCM"))),
            },
            1 => decode_ima_adpcm(body, channels as usize, count * channels as usize),
            e => return Err(Error::Unsupported(format!("sound encoding {e}"))),
        };
        return Ok(Sound {
            sample_rate: rate,
            channels,
            samples,
            loop_count,
        });
    }
}

/// DVI/IMA ADPCM, high nibble first, interleaved per nibble for stereo.
fn decode_ima_adpcm(body: &[u8], channels: usize, max: usize) -> Vec<i16> {
    const STEPS: [i32; 89] = [
        7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60,
        66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371,
        408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707,
        1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132,
        7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623,
        27086, 29794, 32767,
    ];
    const IDX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];
    let mut pred = vec![0i32; channels];
    let mut idx = vec![0i32; channels];
    let mut out = Vec::with_capacity(max);
    let mut ch = 0;
    for &b in body {
        for nib in [b >> 4, b & 0x0f] {
            if out.len() >= max {
                return out;
            }
            let step = STEPS[idx[ch] as usize];
            let mut diff = step >> 3;
            if nib & 1 != 0 {
                diff += step >> 2;
            }
            if nib & 2 != 0 {
                diff += step >> 1;
            }
            if nib & 4 != 0 {
                diff += step;
            }
            if nib & 8 != 0 {
                diff = -diff;
            }
            pred[ch] = (pred[ch] + diff).clamp(-32768, 32767);
            idx[ch] = (idx[ch] + IDX[nib as usize]).clamp(0, 88);
            out.push(pred[ch] as i16);
            ch = (ch + 1) % channels;
        }
    }
    out
}
