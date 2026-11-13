//! Little-endian byte reader with MSB-first bit fields, as used by SWF.

use crate::Error;
use tiny_skia::Transform;

pub(crate) type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    /// Bits already consumed from `data[pos]` (0..8). Byte reads align first.
    bit: u8,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader {
            data,
            pos: 0,
            bit: 0,
        }
    }

    /// Byte position (rounded up to the next byte if inside a bit field).
    pub fn pos(&self) -> usize {
        self.pos + usize::from(self.bit != 0)
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos())
    }

    pub fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.pos += 1;
        }
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.align();
        let end = self.pos.checked_add(n).ok_or(Error::Truncated)?;
        let s = self.data.get(self.pos..end).ok_or(Error::Truncated)?;
        self.pos = end;
        Ok(s)
    }

    pub fn rest(&mut self) -> &'a [u8] {
        self.align();
        let s = self.data.get(self.pos..).unwrap_or(&[]);
        self.pos = self.data.len();
        s
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Null-terminated string (bytes only; content is not needed for rendering).
    pub fn cstr(&mut self) -> Result<&'a [u8]> {
        self.align();
        let rest = self.data.get(self.pos..).ok_or(Error::Truncated)?;
        let n = rest.iter().position(|&b| b == 0).ok_or(Error::Truncated)?;
        self.pos += n + 1;
        Ok(&rest[..n])
    }

    pub fn bit(&mut self) -> Result<bool> {
        Ok(self.ub(1)? != 0)
    }

    pub fn ub(&mut self, n: u32) -> Result<u32> {
        if n > 32 {
            return Err(Error::Malformed(format!("bit field of {n} bits")));
        }
        let mut v: u64 = 0;
        for _ in 0..n {
            let byte = *self.data.get(self.pos).ok_or(Error::Truncated)?;
            v = (v << 1) | u64::from((byte >> (7 - self.bit)) & 1);
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.pos += 1;
            }
        }
        Ok(v as u32)
    }

    pub fn sb(&mut self, n: u32) -> Result<i32> {
        if n == 0 {
            return Ok(0);
        }
        let v = self.ub(n)?;
        let shift = 32 - n;
        Ok(((v << shift) as i32) >> shift)
    }

    /// 16.16 fixed point bit field.
    pub fn fb(&mut self, n: u32) -> Result<f32> {
        Ok(self.sb(n)? as f32 / 65536.0)
    }

    pub fn rgb(&mut self) -> Result<[u8; 4]> {
        let b = self.bytes(3)?;
        Ok([b[0], b[1], b[2], 255])
    }

    pub fn rgba(&mut self) -> Result<[u8; 4]> {
        let b = self.bytes(4)?;
        Ok([b[0], b[1], b[2], b[3]])
    }

    /// RECT in twips: (x_min, x_max, y_min, y_max) re-ordered to (x0, y0, x1, y1).
    pub fn rect(&mut self) -> Result<[i32; 4]> {
        self.align();
        let n = self.ub(5)?;
        let x0 = self.sb(n)?;
        let x1 = self.sb(n)?;
        let y0 = self.sb(n)?;
        let y1 = self.sb(n)?;
        self.align();
        Ok([x0, y0, x1, y1])
    }

    /// MATRIX record (maps child twips to parent twips).
    pub fn matrix(&mut self) -> Result<Transform> {
        self.align();
        let (mut sx, mut sy) = (1.0, 1.0);
        if self.bit()? {
            let n = self.ub(5)?;
            sx = self.fb(n)?;
            sy = self.fb(n)?;
        }
        let (mut r0, mut r1) = (0.0, 0.0);
        if self.bit()? {
            let n = self.ub(5)?;
            r0 = self.fb(n)?;
            r1 = self.fb(n)?;
        }
        let n = self.ub(5)?;
        let tx = self.sb(n)? as f32;
        let ty = self.sb(n)? as f32;
        self.align();
        // x' = sx*x + r1*y + tx ; y' = r0*x + sy*y + ty
        Ok(Transform::from_row(sx, r0, r1, sy, tx, ty))
    }

    /// CXFORM (PlaceObject) or CXFORMWITHALPHA (PlaceObject2/3).
    pub fn cxform(&mut self, with_alpha: bool) -> Result<crate::ColorTransform> {
        self.align();
        let has_add = self.bit()?;
        let has_mul = self.bit()?;
        let n = self.ub(4)?;
        let channels = if with_alpha { 4 } else { 3 };
        let mut cx = crate::ColorTransform::IDENTITY;
        if has_mul {
            for c in 0..channels {
                cx.mul[c] = self.sb(n)? as f32 / 256.0;
            }
        }
        if has_add {
            for c in 0..channels {
                cx.add[c] = self.sb(n)? as f32;
            }
        }
        self.align();
        Ok(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_and_sign_extension() {
        // 0b1011_0000: ub(1)=1, sb(3)=0b011=3, then next 4 bits 0
        let mut r = Reader::new(&[0b1011_0000, 0xff]);
        assert_eq!(r.ub(1).unwrap(), 1);
        assert_eq!(r.sb(3).unwrap(), 3);
        r.align();
        assert_eq!(r.sb(4).unwrap(), -1);
        assert_eq!(r.ub(4).unwrap(), 15);
        assert!(r.ub(1).is_err());
    }

    #[test]
    fn rect_parse() {
        // nbits=5? Use the classic 550x400 stage: 0x78 0x00 0x05 0x5F 0x00 0x00 0x0F 0xA0 0x00
        let mut r = Reader::new(&[0x78, 0x00, 0x05, 0x5F, 0x00, 0x00, 0x0F, 0xA0, 0x00]);
        assert_eq!(r.rect().unwrap(), [0, 0, 11000, 8000]);
        assert_eq!(r.pos(), 9);
    }
}
