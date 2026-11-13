//! `tSCR` animation scripts.

use crate::{bad, Reader, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub left: i16,
    pub top: i16,
    pub right: i16,
    pub bottom: i16,
}

impl Rect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left as i32 && x < self.right as i32 && y >= self.top as i32 && y < self.bottom as i32
    }
    pub fn width(&self) -> i32 {
        self.right as i32 - self.left as i32
    }
    pub fn height(&self) -> i32 {
        self.bottom as i32 - self.top as i32
    }
}

/// One cel placement inside a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cel {
    /// 1-based cel number in the script's `tBMH`.
    pub cel: u16,
    /// `false` when bit 15 of the cel word is clear (cel listed but not drawn).
    pub visible: bool,
    pub x: i16,
    pub y: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Tempo(u16),
    /// Play sound `index` (resource id = base + index, see engine) with a flag word.
    Sound { index: u16, flag: u16 },
    Frame(Vec<Cel>),
    /// Display the current frame for one tick.
    Tick,
    Unknown { op: u8, data: Vec<u8> },
}

#[derive(Clone, Debug)]
pub struct Script {
    pub bitmap_set: u16,
    pub cel_count: u16,
    pub bounds: Rect,
    pub ops: Vec<Op>,
}

impl Script {
    /// All frames in order (each frame = cels shown at one `Tick`).
    pub fn frames(&self) -> Vec<&[Cel]> {
        let mut out = Vec::new();
        let mut cur: &[Cel] = &[];
        for op in &self.ops {
            match op {
                Op::Frame(c) => cur = c,
                Op::Tick => out.push(cur),
                _ => {}
            }
        }
        out
    }

    /// First frame's cels (the "rest" appearance).
    pub fn first_frame(&self) -> &[Cel] {
        for op in &self.ops {
            if let Op::Frame(c) = op {
                return c;
            }
        }
        &[]
    }
}

pub fn parse_script(data: &[u8]) -> Result<Script> {
    let mut r = Reader::new(data);
    let _magic = r.u16()?;
    let bitmap_set = r.u16()?;
    let cel_count = r.u16()?;
    r.skip(10)?;
    let bounds = Rect {
        left: r.i16()?,
        top: r.i16()?,
        right: r.i16()?,
        bottom: r.i16()?,
    };
    r.skip(2)?;
    let mut ops = Vec::new();
    while r.remaining() >= 2 {
        let op = r.u8()?;
        let len = r.u8()?;
        match op {
            0 => break,
            0x10 => {
                let mut cels = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    let c = r.u16()?;
                    cels.push(Cel {
                        cel: c & 0x7fff,
                        visible: c & 0x8000 != 0,
                        x: r.i16()?,
                        y: r.i16()?,
                    });
                }
                ops.push(Op::Frame(cels));
            }
            _ => {
                if len < 2 {
                    return bad(format!("script op {op:#x} with length {len}"));
                }
                let payload = r.bytes(len as usize - 2)?;
                let mut p = Reader::new(payload);
                ops.push(match (op, payload.len()) {
                    (1, 0) => Op::Tick,
                    (4, 2) => Op::Tempo(p.u16()?),
                    (3, 4) => Op::Sound {
                        index: p.u16()?,
                        flag: p.u16()?,
                    },
                    _ => Op::Unknown {
                        op,
                        data: payload.to_vec(),
                    },
                });
            }
        }
    }
    Ok(Script {
        bitmap_set,
        cel_count,
        bounds,
        ops,
    })
}
