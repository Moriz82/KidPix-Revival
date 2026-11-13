//! Reader for Broderbund "Mohawk" resource archives as used by Kid Pix Deluxe 3,
//! plus decoders for the resource types the program uses.
//!
//! See `docs/FORMATS.md` in the repository for the format notes.

mod archive;
mod bitmap;
mod button;
mod cursor;
mod script;
mod sound;

pub use archive::{Archive, Entry, Tag};
pub use bitmap::{
    decode_bitmap, decode_bitmap_set, decode_indexed, rgb555_to_argb, unpack_lz, unpack_lz_with_header, BitmapHeader,
    Image, Indexed, Palette,
};
pub use button::{parse_buttons, Button};
pub use cursor::{parse_cursor, Cursor};
pub use script::{parse_script, Cel, Op, Rect, Script};
pub use sound::{decode_sound, Sound};

use std::fmt;

/// Error type for everything in this crate.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// Structural problem in the data (truncated, bad magic, …).
    Format(String),
    /// A valid but unsupported variant (e.g. an unknown bitmap compression).
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Format(s) => write!(f, "bad data: {s}"),
            Error::Unsupported(s) => write!(f, "unsupported: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn bad<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Format(msg.into()))
}

/// Big-endian cursor over a byte slice with bounds-checked reads.
#[derive(Clone)]
pub(crate) struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }
    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Reader { data, pos }
    }
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return bad(format!("truncated: need {n} bytes at {}", self.pos));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }
    pub fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }
}

/// Read a `tCNT` resource: `(base id, count)`.
pub fn parse_count(data: &[u8]) -> Result<(u16, u16)> {
    let mut r = Reader::new(data);
    Ok((r.u16()?, r.u16()?))
}

/// Read a `REGS` resource: one signed coordinate per cel (index 0 unused).
pub fn parse_regs(data: &[u8]) -> Vec<i16> {
    data.chunks_exact(2)
        .map(|c| i16::from_be_bytes([c[0], c[1]]))
        .collect()
}

/// Decode a Latin-1 string (as used by `tATX` titles and names).
pub fn latin1(data: &[u8]) -> String {
    data.iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect()
}
