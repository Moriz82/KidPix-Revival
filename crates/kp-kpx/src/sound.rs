//! The page sound (`tWAV 1`, RE_SHELL.md §8.7).
//!
//! The resource is kept as raw bytes. [`PageSound::encode`] builds a new one in
//! the same shape the original recorder produces:
//!
//! ```text
//! 'MHWK' u32 (4 + DataChunkSize) 'WAVE'
//! 'ADPC' u32 12  00 01 00 01 00 00 00 00 00 40 00 00
//! 'Data' u32 DataChunkSize (counts its own 8-byte header)
//!        u16 rate, u32 sampleCount, u8 bits=16, u8 channels=1, u16 encoding=1 (IMA ADPCM),
//!        u16 loopCount=0, u32 loopStart=0, u32 loopEnd=0, ADPCM nibbles (high first)
//! ```
//! Both size quirks (the outer size ignoring the `ADPC` chunk, the `Data`
//! size including its header) hold in every sample file.

use crate::{Error, Rect, Result};

/// Sample rate the original program records at.
pub const RECORD_RATE: u16 = 22050;

/// Payload of the `ADPC` checkpoint chunk found in every sample file.
const ADPC_PAYLOAD: [u8; 12] = [0, 1, 0, 1, 0, 0, 0, 0, 0, 0x40, 0, 0];

/// The page's sound plus its catalogue placement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageSound {
    /// Complete Mohawk `WAVE` resource bytes.
    pub data: Vec<u8>,
    /// Catalogue rect (sound icon bounds; all zero when no icon is shown).
    pub rect: Rect,
    /// Catalogue `extra` word (0 in all samples).
    pub extra: u32,
}

impl PageSound {
    /// Wrap existing `tWAV` bytes with an empty placement.
    pub fn from_bytes(data: Vec<u8>) -> PageSound {
        PageSound {
            data,
            rect: Rect::default(),
            extra: 0,
        }
    }

    /// Encode mono 16-bit PCM as an IMA ADPCM Mohawk sound.
    pub fn encode(sample_rate: u16, samples: &[i16]) -> PageSound {
        PageSound::from_bytes(encode_wave(sample_rate, samples))
    }

    /// Decode to PCM.
    pub fn decode(&self) -> Result<mohawk::Sound> {
        mohawk::decode_sound(&self.data).map_err(Error::Mohawk)
    }
}

/// Build a Mohawk `WAVE` resource holding IMA ADPCM (mono).
pub fn encode_wave(sample_rate: u16, samples: &[i16]) -> Vec<u8> {
    let adpcm = ima_encode(samples);
    let data_chunk = 8 + 20 + adpcm.len();
    let mut out = Vec::with_capacity(32 + data_chunk);
    out.extend_from_slice(b"MHWK");
    out.extend_from_slice(&((4 + data_chunk) as u32).to_be_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"ADPC");
    out.extend_from_slice(&(ADPC_PAYLOAD.len() as u32).to_be_bytes());
    out.extend_from_slice(&ADPC_PAYLOAD);
    out.extend_from_slice(b"Data");
    out.extend_from_slice(&(data_chunk as u32).to_be_bytes());
    out.extend_from_slice(&sample_rate.to_be_bytes());
    out.extend_from_slice(&(samples.len() as u32).to_be_bytes());
    out.push(16);
    out.push(1);
    out.extend_from_slice(&1u16.to_be_bytes()); // IMA ADPCM
    out.extend_from_slice(&0u16.to_be_bytes()); // loop count
    out.extend_from_slice(&0u32.to_be_bytes()); // loop start
    out.extend_from_slice(&0u32.to_be_bytes()); // loop end
    out.extend_from_slice(&adpcm);
    out
}

const STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97,
    107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796,
    876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428,
    4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350,
    22385, 24623, 27086, 29794, 32767,
];
const INDEX_STEP: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

/// Standard IMA/DVI ADPCM encoder (predictor and step index start at 0),
/// two samples per byte, high nibble first. The decoder state is tracked
/// exactly so the stream decodes with `mohawk::decode_sound`.
fn ima_encode(samples: &[i16]) -> Vec<u8> {
    let mut pred: i32 = 0;
    let mut index: i32 = 0;
    let mut out = Vec::with_capacity(samples.len().div_ceil(2));
    let mut pending: Option<u8> = None;
    for &s in samples {
        let step = STEPS[index as usize];
        let mut diff = s as i32 - pred;
        let mut nib: u8 = 0;
        if diff < 0 {
            nib = 8;
            diff = -diff;
        }
        // Quantise against step, step/2, step/4 and rebuild the delta the
        // decoder will compute from the chosen bits.
        let mut delta = step >> 3;
        if diff >= step {
            nib |= 4;
            diff -= step;
            delta += step;
        }
        if diff >= step >> 1 {
            nib |= 2;
            diff -= step >> 1;
            delta += step >> 1;
        }
        if diff >= step >> 2 {
            nib |= 1;
            delta += step >> 2;
        }
        pred = if nib & 8 != 0 { pred - delta } else { pred + delta }.clamp(-32768, 32767);
        index = (index + INDEX_STEP[(nib & 7) as usize]).clamp(0, 88);
        match pending.take() {
            None => pending = Some(nib << 4),
            Some(hi) => out.push(hi | nib),
        }
    }
    if let Some(hi) = pending {
        out.push(hi);
    }
    out
}
