//! `tTXT` text box objects (RE_SHELL.md §8.6).
//!
//! ```text
//! 0x00 u32 textLength
//! 0x04 i16 top, left, bottom, right        text box rect (Mac order)
//! 0x0C char[32] font face name (NUL padded)
//! 0x2C u16 font size
//! 0x2E u16 style flags
//! 0x30 u16 version (= 2)
//! 0x32 u8 r, g, b, flag                    flag 0xFF = default colour
//! 0x36 u16 justification
//! 0x38 text bytes (Windows ANSI, textLength bytes, no terminator)
//! ```

use crate::{Error, Rect, Result};

/// Size of the fixed part of a `tTXT` blob.
pub const TEXT_HEADER_LEN: usize = 0x38;

/// Version word written by the original program.
pub const TEXT_VERSION: u16 = 2;

/// Colour flag byte meaning "use the default colour".
pub const COLOR_DEFAULT_FLAG: u8 = 0xFF;

/// A text box. Every header field is kept as stored so rewrites are exact;
/// helpers give typed access where the meaning is known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBox {
    /// Box rectangle on the page, as stored inside the blob.
    pub rect: Rect,
    /// Windows font face name, NUL padded. Bytes after the first NUL are kept.
    pub face: [u8; 32],
    /// Font size (point size as the editor stores it).
    pub size: u16,
    /// Style bit field (believed to hold bold/italic/underline; kept raw).
    pub style: u16,
    /// Blob version word, normally [`TEXT_VERSION`].
    pub version: u16,
    /// Colour bytes `[r, g, b, flag]`; `flag == 0xFF` selects the default colour.
    pub color: [u8; 4],
    /// Justification word (believed 0 = left, 1 = centre, 2 = right; kept raw).
    pub justification: u16,
    /// The text, decoded from Windows-1252 (lossless for every byte value).
    pub text: String,
    /// Bytes following the text, if the blob is longer than the header says.
    pub trailing: Vec<u8>,
}

impl TextBox {
    /// A new box with default colour, left justification and plain style.
    pub fn new(rect: Rect, text: impl Into<String>, face: &str, size: u16) -> TextBox {
        let mut t = TextBox {
            rect,
            face: [0; 32],
            size,
            style: 0,
            version: TEXT_VERSION,
            color: [0, 0, 0, COLOR_DEFAULT_FLAG],
            justification: 0,
            text: text.into(),
            trailing: Vec::new(),
        };
        t.set_face_name(face);
        t
    }

    /// Parse a `tTXT` resource.
    pub fn parse(data: &[u8]) -> Result<TextBox> {
        if data.len() < TEXT_HEADER_LEN {
            return Err(Error::Format(format!("tTXT too short ({} bytes)", data.len())));
        }
        let u16_at = |o: usize| u16::from_be_bytes([data[o], data[o + 1]]);
        let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let body = &data[TEXT_HEADER_LEN..];
        if len > body.len() {
            return Err(Error::Format(format!("tTXT text length {len} exceeds blob ({})", body.len())));
        }
        Ok(TextBox {
            rect: Rect::from_be(&data[4..12]),
            face: data[0x0C..0x2C].try_into().unwrap(),
            size: u16_at(0x2C),
            style: u16_at(0x2E),
            version: u16_at(0x30),
            color: data[0x32..0x36].try_into().unwrap(),
            justification: u16_at(0x36),
            text: decode_ansi(&body[..len]),
            trailing: body[len..].to_vec(),
        })
    }

    /// Serialise to a `tTXT` resource.
    pub fn to_bytes(&self) -> Vec<u8> {
        let text = encode_ansi(&self.text);
        let mut out = Vec::with_capacity(TEXT_HEADER_LEN + text.len() + self.trailing.len());
        out.extend_from_slice(&(text.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.rect.to_be());
        out.extend_from_slice(&self.face);
        out.extend_from_slice(&self.size.to_be_bytes());
        out.extend_from_slice(&self.style.to_be_bytes());
        out.extend_from_slice(&self.version.to_be_bytes());
        out.extend_from_slice(&self.color);
        out.extend_from_slice(&self.justification.to_be_bytes());
        out.extend_from_slice(&text);
        out.extend_from_slice(&self.trailing);
        out
    }

    /// Face name up to the first NUL.
    pub fn face_name(&self) -> String {
        let end = self.face.iter().position(|&b| b == 0).unwrap_or(self.face.len());
        decode_ansi(&self.face[..end])
    }

    /// Replace the face name (truncated to 31 bytes, zero padded).
    pub fn set_face_name(&mut self, name: &str) {
        let bytes = encode_ansi(name);
        let n = bytes.len().min(31);
        self.face = [0; 32];
        self.face[..n].copy_from_slice(&bytes[..n]);
    }

    /// Explicit colour, or `None` when the default-colour flag is set.
    pub fn rgb(&self) -> Option<(u8, u8, u8)> {
        (self.color[3] != COLOR_DEFAULT_FLAG).then_some((self.color[0], self.color[1], self.color[2]))
    }

    /// Set an explicit colour (flag byte 0).
    pub fn set_rgb(&mut self, r: u8, g: u8, b: u8) {
        self.color = [r, g, b, 0];
    }
}

/// Windows-1252 code points for bytes 0x80..=0x9F. The five bytes the code page
/// leaves undefined map to the matching C1 control so decoding stays reversible.
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

/// Decode Windows-1252 bytes.
pub fn decode_ansi(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect()
}

/// Encode to Windows-1252; characters outside the code page become `?`.
pub fn encode_ansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| {
            if let Some(i) = CP1252_HIGH.iter().position(|&h| h == c) {
                0x80 + i as u8
            } else if (c as u32) < 0x100 && !(0x80..=0x9F).contains(&(c as u32)) {
                c as u8
            } else {
                b'?'
            }
        })
        .collect()
}
