//! Sorenson Video 1 (`SVQ1`) decoder.
//!
//! The bitstream: a frame header (22-bit picture code, temporal reference, frame type,
//! for key frames the picture size), then the Y, U and V planes (YUV 4:1:0, chroma a
//! quarter of the size each way, every plane padded to 16) in 16×16 macroblocks. Key
//! frames code each macroblock as a binary tree of vectors (16×16 down to 4×2), every
//! leaf a mean plus up to six 4-bit indices into multistage vector-quantisation
//! codebooks. Delta frames add per-macroblock skip / half-pel motion compensation (one
//! or four vectors, median-predicted) / intra, with the residual coded the same way.
//!
//! The codebooks and variable-length codes come from the Sorenson compressor in the
//! user's `QuickTimeAuthoring.qtx` ([`Tables::from_qtx`]): a serialised list of 16 code
//! tables (`count`, `first symbol`, then `length << 27 | code` words) followed by the
//! codebooks (levels 8×8, 8×4, 4×4, 4×2; 6 stages × 16 vectors each, stored `+128`).

use crate::{bad, Error, Result};

/// One variable-length code table as a binary trie.
#[derive(Clone, Debug, Default)]
struct Vlc {
    /// Nodes: `[child0, child1]`; a child `>= 0` is a node index, `< 0` is
    /// `-1 - (symbol - first)`.
    nodes: Vec<[i32; 2]>,
    first: i32,
}

const NONE: i32 = i32::MIN;

impl Vlc {
    fn build(codes: &[(u32, u32, i32)]) -> Result<Vlc> {
        let first = codes.iter().map(|c| c.2).min().unwrap_or(0);
        let mut v = Vlc { nodes: vec![[NONE, NONE]], first };
        for &(len, code, sym) in codes {
            if len == 0 || len > 27 {
                continue;
            }
            let mut n = 0usize;
            for b in (0..len).rev() {
                let bit = ((code >> b) & 1) as usize;
                if b == 0 {
                    if v.nodes[n][bit] != NONE {
                        return bad("ambiguous code table");
                    }
                    v.nodes[n][bit] = -1 - (sym - first);
                } else {
                    let c = v.nodes[n][bit];
                    if c == NONE {
                        v.nodes.push([NONE, NONE]);
                        let id = (v.nodes.len() - 1) as i32;
                        v.nodes[n][bit] = id;
                        n = id as usize;
                    } else if c < 0 {
                        return bad("prefix clash in code table");
                    } else {
                        n = c as usize;
                    }
                }
            }
        }
        Ok(v)
    }

    fn read(&self, br: &mut Bits) -> Result<i32> {
        let mut n = 0usize;
        loop {
            let c = self.nodes[n][br.bit()? as usize];
            if c == NONE {
                return bad("invalid code");
            }
            if c < 0 {
                return Ok(-1 - c + self.first);
            }
            n = c as usize;
        }
    }
}

/// Codebooks and code tables of the Sorenson Video 1 compressor.
#[derive(Clone, Debug)]
pub struct Tables {
    inter_mean: Vlc,
    intra_mean: Vlc,
    motion: Vlc,
    block_type: Vlc,
    intra_stages: [Vlc; 6],
    inter_stages: [Vlc; 6],
    /// Per level 0..4 (4×2, 4×4, 8×4, 8×8): 6 × 16 vectors of `8 << level` signed values.
    intra_cb: [Vec<i8>; 4],
    inter_cb: [Vec<i8>; 4],
}

/// Symbol counts of the 16 serialised tables, in file order.
const COUNTS: [i32; 16] = [512, 256, 64, 4, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8];

impl Tables {
    /// Find and read the tables in the bytes of `QuickTimeAuthoring.qtx`.
    pub fn from_qtx(qtx: &[u8]) -> Result<Tables> {
        let mut at = 0;
        while let Some(p) = find(&qtx[at..], &[0, 2, 0, 0, 0, 0xff, 0xff, 0xff]) {
            let start = at + p;
            if let Ok(t) = Self::parse(qtx, start) {
                return Ok(t);
            }
            at = start + 1;
        }
        bad("the Sorenson Video tables were not found in QuickTimeAuthoring.qtx")
    }

    pub fn from_qtx_file(path: impl AsRef<std::path::Path>) -> Result<Tables> {
        Self::from_qtx(&std::fs::read(path)?)
    }

    fn parse(d: &[u8], mut o: usize) -> Result<Tables> {
        let rd = |o: usize| -> Result<i32> { d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or(Error::Format("truncated".into())) };
        let mut vlcs = Vec::with_capacity(16);
        for &want in &COUNTS {
            let n = rd(o)?;
            let first = rd(o + 4)?;
            if n != want {
                return bad("unexpected table layout");
            }
            o += 8;
            let mut codes = Vec::with_capacity(n as usize);
            let mut kraft = 0.0f64;
            for i in 0..n {
                let w = rd(o)? as u32;
                o += 4;
                let len = w >> 27;
                if len > 0 {
                    kraft += 0.5f64.powi(len as i32);
                }
                codes.push((len, w & 0x07ff_ffff, first + i));
            }
            if !(0.99..=1.0 + 1e-9).contains(&kraft) {
                return bad("not a prefix code");
            }
            vlcs.push(Vlc::build(&codes)?);
        }
        // codebooks: levels 3, 2, 1, 0; each an intra block then an inter block
        let mut intra_cb: [Vec<i8>; 4] = Default::default();
        let mut inter_cb: [Vec<i8>; 4] = Default::default();
        for level in (0..4).rev() {
            let n = 6 * 16 * (8usize << level);
            for book in [&mut intra_cb[level], &mut inter_cb[level]] {
                let b = d.get(o..o + n).ok_or(Error::Format("truncated codebooks".into()))?;
                *book = b.iter().map(|&v| v as i8).collect();
                o += n;
            }
        }
        let mut it = vlcs.into_iter();
        let mut next = || it.next().unwrap();
        let inter_mean = next();
        let intra_mean = next();
        let motion = next();
        let block_type = next();
        // serialised level 5 first
        let mut intra_stages: [Vlc; 6] = Default::default();
        for l in (0..6).rev() {
            intra_stages[l] = next();
        }
        let mut inter_stages: [Vlc; 6] = Default::default();
        for l in (0..6).rev() {
            inter_stages[l] = next();
        }
        Ok(Tables { inter_mean, intra_mean, motion, block_type, intra_stages, inter_stages, intra_cb, inter_cb })
    }
}

/// The compressor keeps a vector as 4×4 tiles in reading order (left, right; top,
/// bottom), each tile row by row — the way the vector tree splits a block.
fn cb_index(x: usize, y: usize) -> usize {
    (y >> 2) * 32 + (x >> 2) * 16 + (y & 3) * 4 + (x & 3)
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

/// MSB-first bit reader.
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    fn bit(&mut self) -> Result<u32> {
        let b = *self.d.get(self.pos >> 3).ok_or(Error::Format("frame data ended early".into()))?;
        let v = (b >> (7 - (self.pos & 7))) & 1;
        self.pos += 1;
        Ok(v as u32)
    }

    fn bits(&mut self, n: u32) -> Result<u32> {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }
}

/// One decoded picture (planes padded to multiples of 16).
#[derive(Clone)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// Y, U, V: (data, pitch, rows).
    pub planes: [(Vec<u8>, usize, usize); 3],
}

impl Picture {
    fn new(width: u32, height: u32) -> Picture {
        let al = |v: u32| ((v + 15) & !15) as usize;
        let y = (vec![0u8; al(width) * al(height)], al(width), al(height));
        let c = |_: usize| (vec![128u8; al(width / 4) * al(height / 4)], al(width / 4), al(height / 4));
        Picture { width, height, planes: [y, c(1), c(2)] }
    }

    /// RGB (0xRRGGBB) at `w × h` (the movie's size; normally the picture's own), ITU-R
    /// BT.601 with studio-range luma, chroma interpolated bilinearly.
    pub fn to_rgb(&self, w: u32, h: u32) -> Vec<u32> {
        let (yd, yp, _) = &self.planes[0];
        let (ud, cp, ch) = &self.planes[1];
        let (vd, _, _) = &self.planes[2];
        let (cw, chh) = ((self.width / 4).max(1) as f32, (self.height / 4).max(1) as f32);
        let chroma = |p: &Vec<u8>, x: f32, y: f32| -> f32 {
            let x = (x - 0.5).clamp(0.0, cw - 1.0);
            let y = (y - 0.5).clamp(0.0, chh - 1.0);
            let (x0, y0) = (x.floor() as usize, y.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(*cp - 1), (y0 + 1).min(*ch - 1));
            let (fx, fy) = (x - x0 as f32, y - y0 as f32);
            let g = |xx: usize, yy: usize| p[yy * cp + xx] as f32;
            (g(x0, y0) * (1.0 - fx) + g(x1, y0) * fx) * (1.0 - fy) + (g(x0, y1) * (1.0 - fx) + g(x1, y1) * fx) * fy
        };
        let mut out = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = ((x * self.width / w) as usize, (y * self.height / h) as usize);
                let yy = yd[sy * yp + sx] as f32;
                let (fx, fy) = ((sx as f32 + 0.5) / 4.0, (sy as f32 + 0.5) / 4.0);
                let u = chroma(ud, fx, fy) - 128.0;
                let v = chroma(vd, fx, fy) - 128.0;
                let l = 1.164 * (yy - 16.0);
                let r = (l + 1.596 * v).round().clamp(0.0, 255.0) as u32;
                let g = (l - 0.392 * u - 0.813 * v).round().clamp(0.0, 255.0) as u32;
                let b = (l + 2.017 * u).round().clamp(0.0, 255.0) as u32;
                out.push(r << 16 | g << 8 | b);
            }
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct Mv {
    x: i32,
    y: i32,
}

fn median(a: i32, b: i32, c: i32) -> i32 {
    a.max(b).min(a.min(b).max(c))
}

/// Wrap to a 6-bit signed value.
fn sext6(v: i32) -> i32 {
    (v << 26) >> 26
}

const SIZES: [(u32, u32); 7] = [(160, 120), (128, 96), (176, 144), (352, 288), (704, 576), (240, 180), (320, 240)];

pub struct Decoder<'t> {
    t: &'t Tables,
    width: u32,
    height: u32,
    reference: Option<Picture>,
}

impl<'t> Decoder<'t> {
    pub fn new(t: &'t Tables) -> Decoder<'t> {
        Decoder { t, width: 0, height: 0, reference: None }
    }

    /// Decode one frame. Droppable frames are decoded but not kept as reference.
    pub fn decode(&mut self, data: &[u8]) -> Result<Picture> {
        let mut buf;
        let mut d = data;
        if d.len() < 4 {
            return bad("empty frame");
        }
        let code = u32::from_be_bytes([d[0], d[1], d[2], d[3]]) >> 10;
        if code & !0x70 != 0 || code & 0x60 == 0 {
            return bad("not a Sorenson Video 1 frame");
        }
        if code != 0x20 {
            // words 1..4 of scrambled frames are swapped and xored with words 7..4
            if d.len() < 36 {
                return bad("frame too short");
            }
            buf = d.to_vec();
            for i in 0..4 {
                let w = |k: usize| u32::from_le_bytes([buf[4 + 4 * k], buf[5 + 4 * k], buf[6 + 4 * k], buf[7 + 4 * k]]);
                let v = w(i).rotate_left(16) ^ w(7 - i);
                buf[4 + 4 * i..8 + 4 * i].copy_from_slice(&v.to_le_bytes());
            }
            d = &buf;
        }
        let mut br = Bits { d, pos: 22 };
        let _temporal = br.bits(8)?;
        let ftype = br.bits(2)?;
        if ftype == 3 {
            return bad("invalid frame type");
        }
        if ftype == 0 {
            if code == 0x50 || code == 0x60 {
                br.bits(16)?; // checksum
            }
            if (code ^ 0x10) >= 0x50 {
                // an (encrypted) text message: length, then that many bytes
                let n = br.bits(8)?;
                for _ in 0..n {
                    br.bits(8)?;
                }
            }
            br.bits(5)?;
            let sc = br.bits(3)?;
            let (w, h) = if sc == 7 { (br.bits(12)?, br.bits(12)?) } else { SIZES[sc as usize] };
            if w == 0 || h == 0 {
                return bad("bad picture size");
            }
            self.width = w;
            self.height = h;
        } else if self.reference.is_none() {
            return bad("delta frame without a key frame");
        }
        if br.bit()? == 1 {
            br.bits(2)?;
            if br.bits(2)? != 0 {
                return bad("unsupported frame flags");
            }
        }
        if br.bit()? == 1 {
            br.bits(8)?;
            while br.bit()? == 1 {
                br.bits(8)?;
            }
        }
        let mut cur = Picture::new(self.width, self.height);
        for p in 0..3 {
            let (pw, ph) = (cur.planes[p].1, cur.planes[p].2);
            if ftype == 0 {
                for by in (0..ph).step_by(16) {
                    for bx in (0..pw).step_by(16) {
                        let plane = &mut cur.planes[p].0;
                        self.block_intra(&mut br, plane, by * pw + bx, pw)?;
                    }
                }
            } else {
                let reference = self.reference.as_ref().unwrap();
                let prev = &reference.planes[p].0;
                if prev.len() != cur.planes[p].0.len() {
                    return bad("picture size changed in a delta frame");
                }
                let mut motion = vec![Mv::default(); pw / 8 + 3];
                for by in (0..ph).step_by(16) {
                    for bx in (0..pw).step_by(16) {
                        let plane = &mut cur.planes[p].0;
                        self.block_delta(&mut br, plane, prev, pw, ph, bx, by, &mut motion)?;
                    }
                    motion[0] = Mv::default();
                }
            }
        }
        if ftype != 2 {
            self.reference = Some(cur.clone());
        }
        Ok(cur)
    }

    /// Walk the vector tree of a 16×16 block; `leaf(level, offset)` handles each leaf.
    fn tree(br: &mut Bits, origin: usize, pitch: usize, mut leaf: impl FnMut(&mut Bits, usize, usize) -> Result<()>) -> Result<()> {
        let mut list = [0usize; 64];
        list[0] = origin;
        let (mut i, mut m, mut n, mut level) = (0usize, 1usize, 1usize, 5usize);
        while i < n {
            while level > 0 {
                if i == m {
                    m = n;
                    level -= 1;
                    if level == 0 {
                        break;
                    }
                }
                if br.bit()? == 0 {
                    break;
                }
                let step = if level & 1 != 0 { pitch } else { 1 } << ((level >> 1) + 1);
                list[n] = list[i];
                list[n + 1] = list[i] + step;
                n += 2;
                i += 1;
            }
            leaf(br, level, list[i])?;
            i += 1;
        }
        Ok(())
    }

    fn dims(level: usize) -> (usize, usize) {
        (1 << ((4 + level) / 2), 1 << ((3 + level) / 2))
    }

    /// Codebook indices of a leaf: vectors for stages 0..n.
    fn vectors<'c>(br: &mut Bits, cb: &'c [i8], level: usize, stages: usize) -> Result<Vec<&'c [i8]>> {
        let len = 8 << level;
        let idx = br.bits(4 * stages as u32)?;
        Ok((0..stages)
            .map(|j| {
                let k = ((idx >> (4 * (stages - j - 1))) & 15) as usize + 16 * j;
                &cb[k * len..(k + 1) * len]
            })
            .collect())
    }

    fn block_intra(&self, br: &mut Bits, plane: &mut [u8], origin: usize, pitch: usize) -> Result<()> {
        let t = self.t;
        Self::tree(br, origin, pitch, |br, level, off| {
            let (w, h) = Self::dims(level);
            let stages = t.intra_stages[level].read(br)? - 1;
            if stages == -1 {
                for y in 0..h {
                    plane[off + y * pitch..off + y * pitch + w].fill(0);
                }
                return Ok(());
            }
            if stages > 0 && level >= 4 {
                return bad("invalid vector");
            }
            let mean = t.intra_mean.read(br)?;
            if stages == 0 {
                for y in 0..h {
                    plane[off + y * pitch..off + y * pitch + w].fill(mean as u8);
                }
                return Ok(());
            }
            let vs = Self::vectors(br, &t.intra_cb[level], level, stages as usize)?;
            for y in 0..h {
                for x in 0..w {
                    let s: i32 = vs.iter().map(|v| v[cb_index(x, y)] as i32).sum();
                    plane[off + y * pitch + x] = (mean + s).clamp(0, 255) as u8;
                }
            }
            Ok(())
        })
    }

    fn block_residual(&self, br: &mut Bits, plane: &mut [u8], origin: usize, pitch: usize) -> Result<()> {
        let t = self.t;
        Self::tree(br, origin, pitch, |br, level, off| {
            let (w, h) = Self::dims(level);
            let stages = t.inter_stages[level].read(br)? - 1;
            if stages == -1 {
                return Ok(());
            }
            if stages > 0 && level >= 4 {
                return bad("invalid vector");
            }
            let mean = t.inter_mean.read(br)?;
            let vs = if stages == 0 { Vec::new() } else { Self::vectors(br, &t.inter_cb[level], level, stages as usize)? };
            for y in 0..h {
                for x in 0..w {
                    let s: i32 = vs.iter().map(|v| v[cb_index(x, y)] as i32).sum();
                    let p = &mut plane[off + y * pitch + x];
                    *p = (*p as i32 + mean + s).clamp(0, 255) as u8;
                }
            }
            Ok(())
        })
    }

    /// A motion vector: horizontal then vertical difference (each wrapped to −32..31)
    /// added to the median of the three predictors.
    fn read_mv(&self, br: &mut Bits, pred: [Mv; 3]) -> Result<Mv> {
        let dx = self.t.motion.read(br)? - 32;
        let dy = self.t.motion.read(br)? - 32;
        let x = sext6(dx + median(pred[0].x, pred[1].x, pred[2].x));
        let y = sext6(dy + median(pred[0].y, pred[1].y, pred[2].y));
        Ok(Mv { x, y })
    }

    /// Half-pel copy of a `size`×`size` block from `prev` at (sx + mx/2, sy + my/2).
    #[allow(clippy::too_many_arguments)]
    fn predict(cur: &mut [u8], prev: &[u8], pitch: usize, dx: usize, dy: usize, sx: i32, sy: i32, mv: Mv, size: usize) {
        let x0 = (sx + (mv.x >> 1)) as usize;
        let y0 = (sy + (mv.y >> 1)) as usize;
        let (hx, hy) = (mv.x & 1 != 0, mv.y & 1 != 0);
        let rows = prev.len() / pitch;
        let px = |x: usize, y: usize| prev[y.min(rows - 1) * pitch + x.min(pitch - 1)] as u32;
        for y in 0..size {
            for x in 0..size {
                let (a, b) = (x0 + x, y0 + y);
                let v = match (hx, hy) {
                    (false, false) => px(a, b),
                    (true, false) => (px(a, b) + px(a + 1, b) + 1) >> 1,
                    (false, true) => (px(a, b) + px(a, b + 1) + 1) >> 1,
                    (true, true) => (px(a, b) + px(a + 1, b) + px(a, b + 1) + px(a + 1, b + 1) + 2) >> 2,
                };
                cur[(dy + y) * pitch + dx + x] = v as u8;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn block_delta(&self, br: &mut Bits, cur: &mut [u8], prev: &[u8], pw: usize, ph: usize, x: usize, y: usize, motion: &mut [Mv]) -> Result<()> {
        // encoder symbols: 0 inter, 1 inter (4 vectors), 2 intra, 3 skip
        let kind = self.t.block_type.read(br)?;

        let c = x / 8;
        if kind == 3 || kind == 2 {
            motion[0] = Mv::default();
            motion[c + 2] = Mv::default();
            motion[c + 3] = Mv::default();
        }
        let (w, h) = (pw as i32, ph as i32);
        let (xi, yi) = (x as i32, y as i32);
        match kind {
            3 => {
                for r in 0..16 {
                    let o = (y + r) * pw + x;
                    cur[o..o + 16].copy_from_slice(&prev[o..o + 16]);
                }
            }
            0 => {
                let pred = if y == 0 { [motion[0]; 3] } else { [motion[0], motion[c + 2], motion[c + 4]] };
                let mv = self.read_mv(br, pred)?;
                motion[0] = mv;
                motion[c + 2] = mv;
                motion[c + 3] = mv;
                let mv = Mv { x: mv.x.clamp(-2 * xi, 2 * (w - xi - 16)), y: mv.y.clamp(-2 * yi, 2 * (h - yi - 16)) };
                Self::predict(cur, prev, pw, x, y, xi, yi, mv, 16);
                self.block_residual(br, cur, y * pw + x, pw)?;
            }
            1 => {
                // four 8×8 vectors (TL, TR, BL, BR), stored into the predictor row as
                // they are decoded; predictors: TL (left, above, above-right), TR (TL,
                // above, above-right), BL (TL, TR, the left block's last), BR (BL, TR, TL)
                let p0 = if y == 0 { [motion[0]; 3] } else { [motion[0], motion[c + 2], motion[c + 4]] };
                let m0 = self.read_mv(br, p0)?;
                let p1 = if y == 0 { [m0; 3] } else { [m0, motion[c + 3], motion[c + 4]] };
                motion[0] = self.read_mv(br, p1)?;
                motion[c + 2] = self.read_mv(br, [m0, motion[0], motion[c + 1]])?;
                motion[c + 3] = self.read_mv(br, [motion[c + 2], motion[0], m0])?;
                let mv = [m0, motion[0], motion[c + 2], motion[c + 3]];
                for (i, m) in mv.iter().enumerate() {
                    let ox = (i & 1) as i32 * 8;
                    let oy = (i >> 1) as i32 * 8;
                    let mx = (m.x + ox * 2).clamp(-2 * xi, 2 * (w - xi - 8));
                    let my = (m.y + oy * 2).clamp(-2 * yi, 2 * (h - yi - 8));
                    Self::predict(cur, prev, pw, x + ox as usize, y + oy as usize, xi, yi, Mv { x: mx, y: my }, 8);
                }
                self.block_residual(br, cur, y * pw + x, pw)?;
            }
            _ => self.block_intra(br, cur, y * pw + x, pw)?,
        }
        Ok(())
    }
}
