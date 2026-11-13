//! QuickTime file reader: the `moov` track tables (sample descriptions, chunk offsets,
//! samples per chunk, sample sizes and durations) — enough to pull every video frame and
//! the sound out of the CD's movies.

use crate::{bad, ima4, Error, Result};

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub offset: u64,
    pub size: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Track {
    /// `vide` / `soun`.
    pub handler: [u8; 4],
    /// Sample description format (`SVQ1`, `ima4`, …).
    pub format: [u8; 4],
    pub timescale: u32,
    /// Media duration in `timescale` units.
    pub duration: u64,
    pub width: u32,
    pub height: u32,
    pub channels: u16,
    pub sample_bits: u16,
    pub sample_rate: u32,
    /// Video: one entry per frame. Sound: one entry per chunk.
    pub samples: Vec<Sample>,
    /// Sound: sample frames per chunk (parallel to `samples`).
    pub chunk_frames: Vec<u32>,
    /// Total sample duration (sum of `stts`).
    pub total_time: u64,
}

impl Track {
    pub fn is_video(&self) -> bool {
        &self.handler == b"vide"
    }

    pub fn is_audio(&self) -> bool {
        &self.handler == b"soun"
    }

    /// Frames per second from the time-to-sample table.
    pub fn fps(&self) -> f32 {
        if self.total_time == 0 || self.timescale == 0 {
            return 15.0;
        }
        (self.samples.len() as f64 * self.timescale as f64 / self.total_time as f64) as f32
    }

    /// The whole sound track as interleaved 16-bit samples.
    pub fn decode_audio(&self, data: &[u8]) -> Result<Vec<i16>> {
        let ch = self.channels.max(1) as usize;
        let mut out = Vec::new();
        let mut ima = vec![ima4::State::default(); ch];
        for (s, &frames) in self.samples.iter().zip(&self.chunk_frames) {
            let get = |len: u64| data.get(s.offset as usize..(s.offset + len) as usize).ok_or(Error::Format("sound chunk outside the file".into()));
            match &self.format {
                b"ima4" => {
                    let packets = (frames as u64).div_ceil(64);
                    out.extend(ima4::decode_with(get(packets * 34 * ch as u64)?, ch, &mut ima));
                }
                b"twos" | b"sowt" | b"raw " | b"NONE" => {
                    if self.sample_bits <= 8 {
                        let b = get(frames as u64 * ch as u64)?;
                        let signed = &self.format == b"twos";
                        out.extend(b.iter().map(|&v| if signed { ((v as i8) as i16) << 8 } else { ((v as i16) - 128) << 8 }));
                    } else {
                        let b = get(frames as u64 * ch as u64 * 2)?;
                        let le = &self.format == b"sowt";
                        out.extend(b.chunks_exact(2).map(|p| if le { i16::from_le_bytes([p[0], p[1]]) } else { i16::from_be_bytes([p[0], p[1]]) }));
                    }
                }
                f => return bad(format!("unsupported sound codec {:?}", String::from_utf8_lossy(f))),
            }
        }
        Ok(out)
    }
}

pub struct File {
    pub tracks: Vec<Track>,
}

fn u16be(d: &[u8], o: usize) -> Result<u16> {
    d.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]])).ok_or(Error::Format("truncated atom".into()))
}

fn u32be(d: &[u8], o: usize) -> Result<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])).ok_or(Error::Format("truncated atom".into()))
}

fn u64be(d: &[u8], o: usize) -> Result<u64> {
    Ok(((u32be(d, o)? as u64) << 32) | u32be(d, o + 4)? as u64)
}

/// Child atoms of `[s, e)`: (type, body start, end).
fn atoms(d: &[u8], mut s: usize, e: usize) -> Vec<([u8; 4], usize, usize)> {
    let mut v = Vec::new();
    while s + 8 <= e {
        let Ok(size) = u32be(d, s) else { break };
        let t = [d[s + 4], d[s + 5], d[s + 6], d[s + 7]];
        let (hdr, size) = match size {
            1 => match u64be(d, s + 8) {
                Ok(z) => (16, z as usize),
                Err(_) => break,
            },
            0 => (8, e - s),
            z => (8, z as usize),
        };
        if size < hdr || s + size > e {
            break;
        }
        v.push((t, s + hdr, s + size));
        s += size;
    }
    v
}

fn child(d: &[u8], s: usize, e: usize, t: &[u8; 4]) -> Option<(usize, usize)> {
    atoms(d, s, e).into_iter().find(|a| &a.0 == t).map(|a| (a.1, a.2))
}

impl File {
    pub fn parse(d: &[u8]) -> Result<File> {
        let (ms, me) = child(d, 0, d.len(), b"moov").ok_or(Error::Format("not a QuickTime movie (no moov)".into()))?;
        let mut tracks = Vec::new();
        for (t, s, e) in atoms(d, ms, me) {
            if &t == b"trak" {
                if let Ok(tr) = Self::track(d, s, e) {
                    tracks.push(tr);
                }
            }
        }
        Ok(File { tracks })
    }

    fn track(d: &[u8], s: usize, e: usize) -> Result<Track> {
        let missing = |a: &str| Error::Format(format!("track without {a}"));
        let mut tr = Track::default();
        if let Some((a, _)) = child(d, s, e, b"tkhd") {
            // version 0: width/height (16.16) at 76/80
            let w = u32be(d, a + 76)? >> 16;
            let h = u32be(d, a + 80)? >> 16;
            tr.width = w;
            tr.height = h;
        }
        let (ms, me) = child(d, s, e, b"mdia").ok_or_else(|| missing("mdia"))?;
        let (hs, he) = child(d, ms, me, b"mdhd").ok_or_else(|| missing("mdhd"))?;
        let _ = he;
        if d[hs] == 1 {
            tr.timescale = u32be(d, hs + 20)?;
            tr.duration = u64be(d, hs + 24)?;
        } else {
            tr.timescale = u32be(d, hs + 12)?;
            tr.duration = u32be(d, hs + 16)? as u64;
        }
        if let Some((a, _)) = child(d, ms, me, b"hdlr") {
            tr.handler.copy_from_slice(&d[a + 8..a + 12]);
        }
        let (is, ie) = child(d, ms, me, b"minf").ok_or_else(|| missing("minf"))?;
        let (ss, se) = child(d, is, ie, b"stbl").ok_or_else(|| missing("stbl"))?;
        let get = |t: &[u8; 4]| child(d, ss, se, t);
        // sample description
        let (ds, _) = get(b"stsd").ok_or_else(|| missing("stsd"))?;
        let entry = ds + 8;
        tr.format.copy_from_slice(&d[entry + 4..entry + 8]);
        let body = entry + 16;
        if tr.is_video() {
            tr.width = u16be(d, body + 16)? as u32;
            tr.height = u16be(d, body + 18)? as u32;
        } else if tr.is_audio() {
            tr.channels = u16be(d, body + 8)?;
            tr.sample_bits = u16be(d, body + 10)?;
            tr.sample_rate = u32be(d, body + 16)? >> 16;
        }
        // chunk offsets
        let offsets: Vec<u64> = if let Some((a, _)) = get(b"stco") {
            let n = u32be(d, a + 4)? as usize;
            (0..n).map(|i| u32be(d, a + 8 + 4 * i).map(|v| v as u64)).collect::<Result<_>>()?
        } else if let Some((a, _)) = get(b"co64") {
            let n = u32be(d, a + 4)? as usize;
            (0..n).map(|i| u64be(d, a + 8 + 8 * i)).collect::<Result<_>>()?
        } else {
            return Err(missing("stco"));
        };
        // samples per chunk
        let (cs, _) = get(b"stsc").ok_or_else(|| missing("stsc"))?;
        let n = u32be(d, cs + 4)? as usize;
        let mut stsc = Vec::with_capacity(n);
        for i in 0..n {
            stsc.push((u32be(d, cs + 8 + 12 * i)?, u32be(d, cs + 12 + 12 * i)?));
        }
        let per_chunk = |c: usize| -> u32 { stsc.iter().rev().find(|x| x.0 as usize <= c + 1).map(|x| x.1).unwrap_or(0) };
        // durations
        if let Some((a, _)) = get(b"stts") {
            let n = u32be(d, a + 4)? as usize;
            for i in 0..n {
                tr.total_time += u32be(d, a + 8 + 8 * i)? as u64 * u32be(d, a + 12 + 8 * i)? as u64;
            }
        }
        if tr.is_audio() {
            for (c, &o) in offsets.iter().enumerate() {
                tr.samples.push(Sample { offset: o, size: 0 });
                tr.chunk_frames.push(per_chunk(c));
            }
            return Ok(tr);
        }
        // sample sizes
        let (zs, _) = get(b"stsz").ok_or_else(|| missing("stsz"))?;
        let fixed = u32be(d, zs + 4)?;
        let count = u32be(d, zs + 8)? as usize;
        let size = |i: usize| -> Result<u64> { if fixed != 0 { Ok(fixed as u64) } else { u32be(d, zs + 12 + 4 * i).map(|v| v as u64) } };
        let mut i = 0;
        for (c, &o) in offsets.iter().enumerate() {
            let mut off = o;
            for _ in 0..per_chunk(c) {
                if i >= count {
                    break;
                }
                let z = size(i)?;
                tr.samples.push(Sample { offset: off, size: z });
                off += z;
                i += 1;
            }
        }
        Ok(tr)
    }
}
