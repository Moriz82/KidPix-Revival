//! `MBTN` button tables.

use crate::{Reader, Result};

/// One 32-byte button record. Script ids of 0 mean "none".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Button {
    pub idle: u16,
    pub hover: u16,
    pub state2: u16,
    pub state3: u16,
    pub command: u16,
    pub click: u16,
    /// Radio group / button kind (field 14).
    pub group: u16,
    pub raw: [u16; 16],
}

pub fn parse_buttons(data: &[u8]) -> Result<Vec<Button>> {
    let mut out = Vec::new();
    let mut r = Reader::new(data);
    while r.remaining() >= 32 {
        let mut raw = [0u16; 16];
        for v in raw.iter_mut() {
            *v = r.u16()?;
        }
        out.push(Button {
            idle: raw[0],
            hover: raw[1],
            state2: raw[2],
            state3: raw[3],
            command: raw[6],
            click: raw[10],
            group: raw[14],
            raw,
        });
    }
    Ok(out)
}
