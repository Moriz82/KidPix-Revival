//! `KpInstall/KPD3.cfg` reader (RE_BRUSHES §2.4, §3.1).
//!
//! The original uses its own INI reader (`@0x4f0543`, key match `@0x4f13c1`): section and
//! key names are compared **case-insensitively** (`_memicmp`), the **first** section with a
//! name wins, lines starting with `;` are comments, a section ends at the next `[` line,
//! the value is the rest of the line after `=` with trailing blanks trimmed and one pair
//! of matching quotes removed. Numbers are parsed with `strtol(value, &end, 0)` (so `0x`
//! = hex, leading `0` = octal, parsing stops at the first non-digit — that is why
//! `spatter = 1;` works) and must fit in `i16`, otherwise the default is kept.
//! `//spacing = 1` is simply a key named `//spacing` and is never found.

use crate::ids::BrushId;

/// Parsed INI text, preserving section and key order.
#[derive(Clone, Debug, Default)]
pub struct Ini {
    sections: Vec<(String, Vec<(String, String)>)>,
}

impl Ini {
    pub fn parse(text: &str) -> Ini {
        let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for raw in text.lines() {
            let line = raw.trim_start_matches([' ', '\t']).trim_end_matches(['\r', '\n']);
            if line.starts_with(';') || line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                let name = match rest.find(']') {
                    Some(e) => &rest[..e],
                    None => rest,
                };
                sections.push((name.to_string(), Vec::new()));
                continue;
            }
            let Some(eq) = line.find('=') else { continue };
            let key = line[..eq].trim_end_matches([' ', '\t']).to_string();
            let mut val = line[eq + 1..].trim_start_matches([' ', '\t']).trim_end_matches([' ', '\t']);
            let vb = val.as_bytes();
            if vb.len() >= 2 && (vb[0] == b'"' || vb[0] == b'\'') && vb[vb.len() - 1] == vb[0] {
                val = &val[1..val.len() - 1];
            }
            if let Some(s) = sections.last_mut() {
                s.1.push((key, val.to_string()));
            }
        }
        Ini { sections }
    }

    /// Raw value of `key` in the first section named `section` (both case-insensitive).
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let (_, keys) = self.sections.iter().find(|(n, _)| n.eq_ignore_ascii_case(section))?;
        keys.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    pub fn has_section(&self, section: &str) -> bool {
        self.sections.iter().any(|(n, _)| n.eq_ignore_ascii_case(section))
    }

    /// `strtol(v, 0)` limited to i16, like `@0x4f07ee`.
    pub fn get_i16(&self, section: &str, key: &str) -> Option<i16> {
        let v = strtol0(self.get(section, key)?)?;
        if (-32768..=32767).contains(&v) {
            Some(v as i16)
        } else {
            None
        }
    }

    /// Boolean (`on/off`, `yes/no`, `true/false`, numbers) [I]: the boolean reader was not
    /// traced; the cfg only uses `on`, `off`, `0`, `1`.
    pub fn get_bool(&self, section: &str, key: &str) -> Option<bool> {
        let v = self.get(section, key)?.trim();
        let l = v.to_ascii_lowercase();
        match l.as_str() {
            "on" | "yes" | "true" => Some(true),
            "off" | "no" | "false" => Some(false),
            _ => strtol0(v).map(|n| n != 0),
        }
    }

    pub fn section_names(&self) -> impl Iterator<Item = &str> {
        self.sections.iter().map(|(n, _)| n.as_str())
    }
}

/// C `strtol(s, &end, 0)`; `None` when no digits were consumed.
pub fn strtol0(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut radix = 10;
    if i + 1 < b.len() && b[i] == b'0' && (b[i + 1] == b'x' || b[i + 1] == b'X') && i + 2 < b.len() && (b[i + 2] as char).is_ascii_hexdigit() {
        radix = 16;
        i += 2;
    } else if i < b.len() && b[i] == b'0' {
        radix = 8;
    }
    let start = i;
    let mut v: i64 = 0;
    while i < b.len() {
        let Some(d) = (b[i] as char).to_digit(radix) else { break };
        v = v.saturating_mul(radix as i64).saturating_add(d as i64);
        i += 1;
    }
    if i == start {
        return None;
    }
    Some(if neg { -v } else { v })
}

/// Real-media parameters of one tool (`[<Name>]`), defaults from the ctor `@0x44c4c0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrushParams {
    pub brush_id: i16,
    pub spacing: i16,
    pub spatter: i16,
    pub density_bias: i16,
    pub texture_bias: i16,
    pub smear_bias: i16,
    pub dry_out_rate: i16,
    pub dry_out_threshold: i16,
    pub oscillation: i16,
    pub shadow_offset: i16,
    pub shadow_bias: i16,
    pub glint_offset: i16,
    pub glint_bias: i16,
    /// Voice brushes (`@0x44ffd0`: default 1).
    pub frequency_bias: i16,
    pub amplitude_bias: i16,
    /// Hands-free brushes. The code asks for `accelerationBias`; the cfg spells it
    /// `acceleratonBias`, so it keeps its default (1 for the Touchless brushes).
    pub acceleration_bias: i16,
    pub gravity_bias: i16,
    pub gravity_time: i16,
    pub gravity_range: i16,
    pub damp_bias: i16,
    pub wrap: i16,
}

impl Default for BrushParams {
    fn default() -> Self {
        BrushParams {
            brush_id: 1001,
            spacing: 1,
            spatter: 0,
            density_bias: 1,
            texture_bias: 1,
            smear_bias: 0,
            dry_out_rate: 0,
            dry_out_threshold: 0,
            oscillation: 0,
            shadow_offset: 0,
            shadow_bias: 0,
            glint_offset: 0,
            glint_bias: 0,
            frequency_bias: 1,
            amplitude_bias: 1,
            acceleration_bias: 0,
            gravity_bias: 0,
            gravity_time: 0,
            gravity_range: 0,
            damp_bias: 0,
            wrap: 0,
        }
    }
}

impl BrushParams {
    /// Hands-free defaults: accelerationBias 1, gravityBias 1, gravityTime 100,
    /// gravityRange 1, dampBias 2, wrap 0.
    pub fn hands_free_defaults() -> BrushParams {
        BrushParams { acceleration_bias: 1, gravity_bias: 1, gravity_time: 100, gravity_range: 1, damp_bias: 2, wrap: 0, ..BrushParams::default() }
    }

    pub fn from_ini(ini: &Ini, section: &str) -> BrushParams {
        let mut p = BrushParams::default();
        let rd = |k: &str, f: &mut i16| {
            if let Some(v) = ini.get_i16(section, k) {
                *f = v;
            }
        };
        rd("brushID", &mut p.brush_id);
        rd("spacing", &mut p.spacing);
        rd("spatter", &mut p.spatter);
        rd("densityBias", &mut p.density_bias);
        rd("textureBias", &mut p.texture_bias);
        rd("smearBias", &mut p.smear_bias);
        rd("dryOutRate", &mut p.dry_out_rate);
        rd("dryOutThreshold", &mut p.dry_out_threshold);
        rd("oscillation", &mut p.oscillation);
        rd("shadowOffset", &mut p.shadow_offset);
        rd("shadowBias", &mut p.shadow_bias);
        rd("glintOffset", &mut p.glint_offset);
        rd("glintBias", &mut p.glint_bias);
        rd("frequencyBias", &mut p.frequency_bias);
        rd("amplitudeBias", &mut p.amplitude_bias);
        rd("accelerationBias", &mut p.acceleration_bias);
        rd("gravityBias", &mut p.gravity_bias);
        rd("gravityTime", &mut p.gravity_time);
        rd("gravityRange", &mut p.gravity_range);
        rd("dampBias", &mut p.damp_bias);
        rd("wrap", &mut p.wrap);
        p
    }
}

/// `[<Name>Snd]` (§2.4). Ids are `tWAV` in `WackyBrushSounds.MHK`; 0 = none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SoundParams {
    pub continuous: bool,
    pub lingers: bool,
    pub start: u16,
    pub stop: u16,
    pub loop_: u16,
    pub fx: u16,
    pub north: u16,
    pub east: u16,
    pub south: u16,
    pub west: u16,
}

impl SoundParams {
    pub fn from_ini(ini: &Ini, section: &str) -> SoundParams {
        let id = |k: &str| -> u16 { ini.get_i16(section, k).map(|v| v.max(0) as u16).unwrap_or(0) };
        SoundParams {
            continuous: ini.get_bool(section, "continuous").unwrap_or(false),
            lingers: ini.get_bool(section, "lingers").unwrap_or(false),
            start: id("startSoundID"),
            stop: id("stopSoundID"),
            loop_: id("loopSoundID"),
            fx: id("specialFxID"),
            north: id("directionNorth"),
            east: id("directionEast"),
            south: id("directionSouth"),
            west: id("directionWest"),
        }
    }

    pub fn is_silent(&self) -> bool {
        self.start == 0 && self.stop == 0 && self.loop_ == 0 && self.fx == 0 && self.north == 0 && self.east == 0 && self.south == 0 && self.west == 0
    }
}

/// The whole brush configuration.
#[derive(Clone, Debug, Default)]
pub struct BrushConfig {
    pub ini: Ini,
}

impl BrushConfig {
    pub fn parse(text: &str) -> BrushConfig {
        BrushConfig { ini: Ini::parse(text) }
    }

    /// Read `<game root>/KpInstall/KPD3.cfg` (Latin-1).
    pub fn load(game_root: &std::path::Path) -> std::io::Result<BrushConfig> {
        let p = crate::assets::find_ci(game_root, &["KpInstall", "KPD3.cfg"]).unwrap_or_else(|| game_root.join("KpInstall/KPD3.cfg"));
        let bytes = std::fs::read(p)?;
        Ok(BrushConfig::parse(&mohawk::latin1(&bytes.iter().map(|&b| if b == 0 { b' ' } else { b }).collect::<Vec<u8>>())))
    }

    /// Parameters of a tool. Honours the BlurBrush quirk (it reads `[RealBrush]`).
    pub fn params(&self, id: BrushId) -> BrushParams {
        let mut p = BrushParams::from_ini(&self.ini, id.cfg_section());
        if matches!(id, BrushId::Touchless(_)) {
            // RHandsFreeBrush ctor (`@0x44a9d0`) defaults for keys the section lacks
            let d = BrushParams::hands_free_defaults();
            let has = |k: &str| self.ini.get_i16(id.cfg_section(), k).is_some();
            for (k, f, v) in [
                ("accelerationBias", &mut p.acceleration_bias, d.acceleration_bias),
                ("gravityBias", &mut p.gravity_bias, d.gravity_bias),
                ("gravityTime", &mut p.gravity_time, d.gravity_time),
                ("gravityRange", &mut p.gravity_range, d.gravity_range),
                ("dampBias", &mut p.damp_bias, d.damp_bias),
            ] {
                if !has(k) {
                    *f = v;
                }
            }
        }
        p
    }

    /// Sound section of a tool (`[<cfg section>Snd]`).
    pub fn sounds(&self, id: BrushId) -> SoundParams {
        SoundParams::from_ini(&self.ini, &format!("{}Snd", id.cfg_section()))
    }

    /// `maxMovement` is read from KPD3.ini (absent → 0x7fff) — not in this file.
    pub fn section(&self, name: &str) -> BrushParams {
        BrushParams::from_ini(&self.ini, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strtol_semantics() {
        assert_eq!(strtol0("1;"), Some(1));
        assert_eq!(strtol0(" 42 // x"), Some(42));
        assert_eq!(strtol0("0x10"), Some(16));
        assert_eq!(strtol0("010"), Some(8));
        assert_eq!(strtol0("0"), Some(0));
        assert_eq!(strtol0("-7"), Some(-7));
        assert_eq!(strtol0("off"), None);
        assert_eq!(strtol0("= Resouce"), None);
    }

    #[test]
    fn ini_rules() {
        let t = "[A]\nkey = 5\nKey = 6\n;x = 1\n//spacing = 9\n[a]\nkey = 7\n[B]\nname = \"q\" \n";
        let ini = Ini::parse(t);
        assert_eq!(ini.get_i16("a", "KEY"), Some(5)); // first section, first key, case-insensitive
        assert_eq!(ini.get("A", "spacing"), None);
        assert_eq!(ini.get("A", "x"), None);
        assert_eq!(ini.get("b", "name"), Some("q"));
    }
}
