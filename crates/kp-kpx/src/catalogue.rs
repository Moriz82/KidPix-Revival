//! The `tRem 1` object catalogue (RE_SHELL.md §8.4).
//!
//! Current format (version word −3):
//!
//! ```text
//! u16 count   i16 version (= -3)   u32 reserved (= 0)
//! count × { 'kRem'  tag[4]  u16 id  i16 top  i16 left  i16 bottom  i16 right  u32 extra }
//! count × 6 bytes of slack (uninitialised in the originals: 0xCD)
//! ```
//!
//! Old format (detected by `'kRem'` right after `count`): entries start at
//! offset 2, are 18 bytes long and carry no `extra` word.

use crate::{Error, Rect, Result, Tag};

/// Version word of the current catalogue format.
pub const REM_VERSION: i16 = -3;

/// Fill byte the original program leaves in the catalogue slack.
pub const REM_SLACK_FILL: u8 = 0xCD;

/// One catalogue entry: a resource reference plus its placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RemEntry {
    pub tag: Tag,
    pub id: u16,
    pub rect: Rect,
    pub extra: u32,
}

/// Parsed catalogue with everything needed for an exact rewrite.
#[derive(Clone, Debug)]
pub(crate) struct Catalogue {
    pub entries: Vec<RemEntry>,
    pub version: i16,
    pub reserved: u32,
    /// Bytes after the last entry.
    pub slack: Vec<u8>,
    /// True for the old 18-byte-entry format.
    pub legacy: bool,
}

pub(crate) fn parse(data: &[u8]) -> Result<Catalogue> {
    if data.len() < 2 {
        return Err(Error::Format("tRem too short".into()));
    }
    let count = u16::from_be_bytes([data[0], data[1]]) as usize;
    let legacy = data.get(2..6) == Some(b"kRem");
    let (start, stride, version, reserved) = if legacy {
        (2, 18, 0, 0)
    } else {
        if data.len() < 8 {
            return Err(Error::Format("tRem header truncated".into()));
        }
        let version = i16::from_be_bytes([data[2], data[3]]);
        let reserved = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        (8, 22, version, reserved)
    };
    let end = start + count * stride;
    if data.len() < end {
        return Err(Error::Format(format!("tRem: {count} entries do not fit in {} bytes", data.len())));
    }
    let mut entries = Vec::with_capacity(count);
    for e in data[start..end].chunks_exact(stride) {
        if &e[0..4] != b"kRem" {
            return Err(Error::Format("tRem entry without 'kRem' marker".into()));
        }
        entries.push(RemEntry {
            tag: Tag(e[4..8].try_into().unwrap()),
            id: u16::from_be_bytes([e[8], e[9]]),
            rect: Rect::from_be(&e[10..18]),
            extra: if legacy { 0 } else { u32::from_be_bytes([e[18], e[19], e[20], e[21]]) },
        });
    }
    Ok(Catalogue {
        entries,
        version,
        reserved,
        slack: data[end..].to_vec(),
        legacy,
    })
}

/// Serialise in the current format. `slack` is reused when it has the
/// expected length (6 bytes per entry), otherwise filled with 0xCD.
pub(crate) fn write(entries: &[RemEntry], version: i16, reserved: u32, slack: &[u8]) -> Vec<u8> {
    let n = entries.len();
    let mut out = Vec::with_capacity(8 + n * 28);
    out.extend_from_slice(&(n as u16).to_be_bytes());
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(&reserved.to_be_bytes());
    for e in entries {
        out.extend_from_slice(b"kRem");
        out.extend_from_slice(&e.tag.0);
        out.extend_from_slice(&e.id.to_be_bytes());
        out.extend_from_slice(&e.rect.to_be());
        out.extend_from_slice(&e.extra.to_be_bytes());
    }
    if slack.len() == n * 6 {
        out.extend_from_slice(slack);
    } else {
        out.resize(out.len() + n * 6, REM_SLACK_FILL);
    }
    out
}
