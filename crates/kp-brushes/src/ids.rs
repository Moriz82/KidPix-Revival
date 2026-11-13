//! Tool ids, names and icons (RE_BRUSHES §1).

/// Every brush-like tool of KPD3 covered by this crate.
///
/// The numeric tool id (factory `@0x49c590`) is [`BrushId::tool_id`]; the name (name
/// table `@0x5541b4`, also the KPD3.cfg section) is [`BrushId::name`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BrushId {
    // --- Realistic paint brushes, ids 0–19 (§1.3)
    RealBrush,
    Airbrush,
    BristleBrush,
    ExtrusionBrush,
    /// `CustomBrush1`..`CustomBrush16` (ids 4–19), payload 1..=16.
    CustomBrush(u8),
    // --- Wacky Brushes, ids 20–44, in list order (§1.2)
    Madona,
    Drippy,
    Spiral,
    Poly,
    TriSpin,
    WhispyCircle,
    Particle,
    Garland,
    LeakyPen,
    ScratchyPen,
    LoopingLine,
    LoopingLine2,
    Bricks,
    Log,
    Window,
    Door,
    DoorPlain,
    Roof,
    DotToDot,
    Abc,
    Rainbow,
    ColorTile,
    EightWayMirror,
    Splat,
    Tree,
    // --- Sound Art (§1.5, §6)
    /// `CustomVoiceBrush1..10` (ids 45–54), voice + mouse.
    CustomVoiceBrush(u8),
    /// `Touchless1..10` (ids 55–64), hands-free.
    Touchless(u8),
    // --- Mixer wand, ids 87–98 (§1.4, §3.9)
    SmearBrush,
    BlurBrush,
    GreyscaleBrush,
    ColorizeBrush,
    XRayBrush,
    EmbossBrush,
    EdgeBrush,
    NuclearBrush,
    SolarizeBrush,
    HueShiftBrush,
    PosterizeBrush,
    MosaicBrush,
    // --- Pencil media, ids 99–102 (§4)
    /// Plain pencil (`RCircle`).
    Circle,
    Chalk,
    Crayon,
    Marker,
    // --- Spray can (§7)
    /// `SprayGun` (id 119) with Anims.MHK set `24000 + 100*k`, k = 0..20 ("moving paint").
    SprayGun(u8),
    /// `CyclicBitmap` (id 121) with Cyclic.MHK set `24000 + 100*k`, k = 0..15 ("shapes").
    CyclicBitmap(u8),
}

use BrushId::*;

/// Number of Anims.MHK sets (spray "moving paint") and Cyclic.MHK sets (spray "shapes").
pub const SPRAY_MOVING_SETS: u8 = 20;
pub const SPRAY_SHAPE_SETS: u8 = 15;

const NAMES: [&str; 103] = [
    "RealBrush", "Airbrush", "BristleBrush", "ExtrusionBrush", "CustomBrush1", "CustomBrush2", "CustomBrush3", "CustomBrush4",
    "CustomBrush5", "CustomBrush6", "CustomBrush7", "CustomBrush8", "CustomBrush9", "CustomBrush10", "CustomBrush11",
    "CustomBrush12", "CustomBrush13", "CustomBrush14", "CustomBrush15", "CustomBrush16", "Madona", "Drippy", "Spiral", "Poly",
    "TriSpin", "WhispyCircle", "Particle", "Garland", "LeakyPen", "ScratchyPen", "LoopingLine", "LoopingLine2", "Bricks", "Log",
    "Window", "Door", "DoorPlain", "Roof", "DotToDot", "ABC", "Rainbow", "ColorTile", "8WayMirror", "Splat", "Tree",
    "CustomVoiceBrush1", "CustomVoiceBrush2", "CustomVoiceBrush3", "CustomVoiceBrush4", "CustomVoiceBrush5", "CustomVoiceBrush6",
    "CustomVoiceBrush7", "CustomVoiceBrush8", "CustomVoiceBrush9", "CustomVoiceBrush10", "Touchless1", "Touchless2", "Touchless3",
    "Touchless4", "Touchless5", "Touchless6", "Touchless7", "Touchless8", "Touchless9", "Touchless10", "XRay", "Nuclear", "Emboss",
    "Lighten", "Sepia", "Colorize", "ColorCycle", "StripColor", "Pointilize", "Posterize", "Solarize", "Pinch", "Spherize",
    "Ripple", "Bubbles", "Burlap", "Whirl", "Pulse", "Wavelet", "Shear", "Flip", "Kaleidoscope", "SmearBrush", "BlurBrush",
    "GreyscaleBrush", "ColorizeBrush", "XRayBrush", "EmbossBrush", "EdgeBrush", "NuclearBrush", "SolarizeBrush", "HueShiftBrush",
    "PosterizeBrush", "MosaicBrush", "Circle", "Chalk", "Crayon", "Marker",
];

/// Archive (relative to the game root) holding a brush icon.
pub const ICONS_MHK: &str = "Data/Tools/Icons.MHK";
pub const CYCLIC_MHK: &str = "Data/Tools/Cyclic.MHK";
pub const ANIMS_MHK: &str = "Data/Tools/Anims.MHK";

impl BrushId {
    /// Factory tool id (§1).
    pub fn tool_id(self) -> u16 {
        match self {
            RealBrush => 0,
            Airbrush => 1,
            BristleBrush => 2,
            ExtrusionBrush => 3,
            CustomBrush(n) => 3 + n.clamp(1, 16) as u16,
            Madona => 20,
            Drippy => 21,
            Spiral => 22,
            Poly => 23,
            TriSpin => 24,
            WhispyCircle => 25,
            Particle => 26,
            Garland => 27,
            LeakyPen => 28,
            ScratchyPen => 29,
            LoopingLine => 30,
            LoopingLine2 => 31,
            Bricks => 32,
            Log => 33,
            Window => 34,
            Door => 35,
            DoorPlain => 36,
            Roof => 37,
            DotToDot => 38,
            Abc => 39,
            Rainbow => 40,
            ColorTile => 41,
            EightWayMirror => 42,
            Splat => 43,
            Tree => 44,
            CustomVoiceBrush(n) => 44 + n.clamp(1, 10) as u16,
            Touchless(n) => 54 + n.clamp(1, 10) as u16,
            SmearBrush => 87,
            BlurBrush => 88,
            GreyscaleBrush => 89,
            ColorizeBrush => 90,
            XRayBrush => 91,
            EmbossBrush => 92,
            EdgeBrush => 93,
            NuclearBrush => 94,
            SolarizeBrush => 95,
            HueShiftBrush => 96,
            PosterizeBrush => 97,
            MosaicBrush => 98,
            Circle => 99,
            Chalk => 100,
            Crayon => 101,
            Marker => 102,
            SprayGun(_) => 119,
            CyclicBitmap(_) => 121,
        }
    }

    /// Inverse of [`tool_id`](Self::tool_id). Spray ids map to set 0; the Big Mixer
    /// effects (65–86), shape modifiers, fills etc. are not brushes of this crate.
    pub fn from_tool_id(id: u16) -> Option<BrushId> {
        Some(match id {
            0 => RealBrush,
            1 => Airbrush,
            2 => BristleBrush,
            3 => ExtrusionBrush,
            4..=19 => CustomBrush((id - 3) as u8),
            20..=44 => WACKY[(id - 20) as usize],
            45..=54 => CustomVoiceBrush((id - 44) as u8),
            55..=64 => Touchless((id - 54) as u8),
            87..=98 => WAND[(id - 87) as usize],
            99 => Circle,
            100 => Chalk,
            101 => Crayon,
            102 => Marker,
            119 => SprayGun(0),
            121 => CyclicBitmap(0),
            _ => return None,
        })
    }

    /// Name from the tool name table (= KPD3.cfg section name).
    pub fn name(self) -> &'static str {
        match self {
            SprayGun(_) => "SprayGun",
            CyclicBitmap(_) => "CyclicBitmap",
            _ => NAMES[self.tool_id() as usize],
        }
    }

    /// Section actually read from KPD3.cfg. BlurBrush's constructor passes tool id 0, so
    /// it reads `[RealBrush]` / `[RealBrushSnd]` (§3.1 quirk) [O].
    pub fn cfg_section(self) -> &'static str {
        match self {
            BlurBrush => "RealBrush",
            _ => self.name(),
        }
    }

    /// Icon resource: `(archive, tBMP id)`. Option-tray icons live in Icons.MHK; spray
    /// sets use their own tBMP of the same id as the set. Pencil media have no tray icon.
    pub fn icon(self) -> Option<(&'static str, u16)> {
        let t = self.tool_id();
        Some(match self {
            RealBrush | Airbrush | BristleBrush | ExtrusionBrush | CustomBrush(_) => (ICONS_MHK, 3500 + t),
            CustomVoiceBrush(n) => (ICONS_MHK, 3546 + n.clamp(1, 10) as u16),
            Touchless(n) => (ICONS_MHK, 3556 + n.clamp(1, 10) as u16),
            SmearBrush | BlurBrush | GreyscaleBrush | ColorizeBrush | XRayBrush | EmbossBrush | EdgeBrush | NuclearBrush
            | SolarizeBrush | HueShiftBrush | PosterizeBrush | MosaicBrush => (ICONS_MHK, 3800 + (t - 87)),
            SprayGun(k) => (ANIMS_MHK, 24000 + 100 * k.min(SPRAY_MOVING_SETS - 1) as u16),
            CyclicBitmap(k) => (CYCLIC_MHK, 24000 + 100 * k.min(SPRAY_SHAPE_SETS - 1) as u16),
            Circle | Chalk | Crayon | Marker => return None,
            // Wacky: 3520 + index (Drippy's v13 returns 0xdc1 = 3521 too)
            _ => (ICONS_MHK, 3500 + t),
        })
    }

    /// Icon tBMP id (0 when the tool has none). See [`icon`](Self::icon) for the archive.
    pub fn icon_id(self) -> u16 {
        self.icon().map(|i| i.1).unwrap_or(0)
    }

    /// The three values selected by the size buttons (`v0` size table, §2.2 / §1.2).
    pub fn size_table(self) -> [i32; 3] {
        match self {
            Madona | Garland => [10, 20, 40],
            Particle => [5, 10, 50],
            LeakyPen => [2, 5, 10],
            Abc => [16, 24, 32],
            Splat => [1, 2, 3],
            Circle => [2, 10, 20],
            CyclicBitmap(_) => [0, 1, 2],
            _ => [5, 10, 20],
        }
    }

    pub fn is_wacky(self) -> bool {
        (20..=44).contains(&self.tool_id())
    }

    /// Every brush, in tool-id order, with every spray set.
    pub fn all() -> Vec<BrushId> {
        let mut v = Vec::new();
        v.extend(REALISTIC);
        v.extend(WACKY);
        v.extend((1..=10).map(CustomVoiceBrush));
        v.extend((1..=10).map(Touchless));
        v.extend(WAND);
        v.extend(PENCIL_MEDIA);
        v.extend((0..SPRAY_MOVING_SETS).map(SprayGun));
        v.extend((0..SPRAY_SHAPE_SETS).map(CyclicBitmap));
        v
    }
}

/// Realistic Paint Brush option list (ids 0–19, §1.3).
pub const REALISTIC: [BrushId; 20] = [
    RealBrush, Airbrush, BristleBrush, ExtrusionBrush,
    CustomBrush(1), CustomBrush(2), CustomBrush(3), CustomBrush(4), CustomBrush(5), CustomBrush(6), CustomBrush(7), CustomBrush(8),
    CustomBrush(9), CustomBrush(10), CustomBrush(11), CustomBrush(12), CustomBrush(13), CustomBrush(14), CustomBrush(15), CustomBrush(16),
];

/// Wacky Brush option list in exact order (ids 20–44, §1.2).
pub const WACKY: [BrushId; 25] = [
    Madona, Drippy, Spiral, Poly, TriSpin, WhispyCircle, Particle, Garland, LeakyPen, ScratchyPen, LoopingLine, LoopingLine2,
    Bricks, Log, Window, Door, DoorPlain, Roof, DotToDot, Abc, Rainbow, ColorTile, EightWayMirror, Splat, Tree,
];

/// Mixer Wand list (ids 87–98, §1.4).
pub const WAND: [BrushId; 12] = [
    SmearBrush, BlurBrush, GreyscaleBrush, ColorizeBrush, XRayBrush, EmbossBrush, EdgeBrush, NuclearBrush, SolarizeBrush,
    HueShiftBrush, PosterizeBrush, MosaicBrush,
];

/// Pencil panel media (MBTN 18400 param 0..3 → ids 99..102, §4).
pub const PENCIL_MEDIA: [BrushId; 4] = [Circle, Chalk, Crayon, Marker];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_roundtrip() {
        for b in BrushId::all() {
            let t = b.tool_id();
            let back = BrushId::from_tool_id(t).unwrap();
            match b {
                SprayGun(_) | CyclicBitmap(_) => assert_eq!(back.tool_id(), t),
                _ => assert_eq!(back, b),
            }
        }
        assert_eq!(Madona.icon_id(), 3520);
        assert_eq!(Tree.icon_id(), 3544);
        assert_eq!(Drippy.icon_id(), 3521);
        assert_eq!(CustomVoiceBrush(1).icon_id(), 3547);
        assert_eq!(Touchless(10).icon_id(), 3566);
        assert_eq!(MosaicBrush.icon_id(), 3811);
        assert_eq!(EightWayMirror.name(), "8WayMirror");
        assert_eq!(Abc.name(), "ABC");
        assert_eq!(BlurBrush.cfg_section(), "RealBrush");
        assert_eq!(CustomBrush(16).tool_id(), 19);
    }
}
