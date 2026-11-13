//! QuickTime IMA 4:1 ADPCM (`ima4`): per channel, 34-byte packets of a 2-byte header
//! (high 9 bits = predictor, low 7 bits = step index) and 32 bytes holding 64 samples
//! (low nibble first). Channels alternate packet by packet. The step and index tables
//! are those of the IMA Digital Audio Focus and Technical Working Group's ADPCM
//! recommendation (1992).

const STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253,
    279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026,
    4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

const INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

/// Per-channel decoder state, kept across packets and chunks.
#[derive(Clone, Copy, Default)]
pub struct State {
    pred: i32,
    idx: i32,
}

/// Decode whole packets for `channels` interleaved channels with fresh state.
pub fn decode(data: &[u8], channels: usize) -> Vec<i16> {
    decode_with(data, channels, &mut vec![State::default(); channels.max(1)])
}

/// Decode whole packets, continuing from `state` (one per channel). A packet header
/// carries only the top 9 bits of the predictor, so — as Apple's decoder does — the
/// running predictor is kept when the header's step index matches and its predictor is
/// within 0x7f of it.
pub fn decode_with(data: &[u8], channels: usize, state: &mut [State]) -> Vec<i16> {
    let ch = channels.max(1);
    let blocks = data.len() / (34 * ch);
    let mut out = vec![0i16; blocks * 64 * ch];
    for b in 0..blocks {
        for c in 0..ch {
            let p = &data[(b * ch + c) * 34..(b * ch + c + 1) * 34];
            let header = u16::from_be_bytes([p[0], p[1]]) as i16 as i32;
            let (hp, hi) = (header & !0x7f, header & 0x7f);
            let st = &mut state[c];
            if st.idx != hi || (hp - st.pred).abs() > 0x7f {
                st.idx = hi;
                st.pred = hp;
            }
            let mut pred = st.pred;
            let mut idx = st.idx.min(88);
            for (k, &byte) in p[2..].iter().enumerate() {
                for (h, nib) in [byte & 15, byte >> 4].into_iter().enumerate() {
                    let step = STEPS[idx as usize];
                    let mut diff = step >> 3;
                    if nib & 4 != 0 {
                        diff += step;
                    }
                    if nib & 2 != 0 {
                        diff += step >> 1;
                    }
                    if nib & 1 != 0 {
                        diff += step >> 2;
                    }
                    pred = if nib & 8 != 0 { pred - diff } else { pred + diff }.clamp(-32768, 32767);
                    idx = (idx + INDEX[nib as usize]).clamp(0, 88);
                    out[(b * 64 + 2 * k + h) * ch + c] = pred as i16;
                }
            }
            st.pred = pred;
            st.idx = idx;
        }
    }
    out
}
