//! Tool trays, organised like the original (User Guide, "Using the Art Tools"):
//! a tool's tray has a left-hand group of tool variants, then sizes, shapes, modes and a
//! scrolling list of options. Each group remembers its own selection.

use crate::assets::{self, Assets};
use crate::tools::{FillKind, ToolKind};
#[allow(unused_imports)]

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum G {
    /// Left side of the tray: which tool of the family.
    Variant,
    Size,
    Shape,
    FillMode,
    Mode,
    /// Scrolling list on the right side of the tray.
    Options,
    /// One-shot buttons (trash, copy, paste jar, stamp editor…).
    Actions,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DrawTool {
    Pencil,
    Chalk,
    Crayon,
    Marker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Freehand,
    Curve,
    Line,
    Rect,
    Oval,
    Polygon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintTool {
    Brush,
    SoundArt,
    Spray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bucket {
    Solid,
    Blend,
    Pattern,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrabTool {
    Hand,
    Scissors,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutShape {
    Free,
    Polygon,
    Circle,
    Square,
    /// Cookie cutter: `Data/Tools/CutRgns.MHK` MRGN id.
    Cookie(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrabAction {
    Trash,
    Copy,
    Paste,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Draw(DrawTool),
    Size(u8),
    Shape(Shape),
    Filled(bool),
    Paint(PaintTool),
    /// Paintbrush mode: false = Realistic, true = Wacky.
    Wacky(bool),
    /// Sound Art: false = Mouse-move, true = Hands-free.
    HandsFree(bool),
    /// Spray Can: false = 3D Shapes, true = Moving Paint.
    Moving(bool),
    Bucket(Bucket),
    Fill(FillKind),
    /// Mixer: false = Mega (whole picture), true = Mini (where you drag).
    Mini(bool),
    /// Big Mixer effect i (tool id 65 + i; icon Icons.MHK 3900 + i).
    MixFx(u8),
    /// Mixer Wand brush i (tool id 87 + i; icon Icons.MHK 3800 + i).
    Wand(u8),
    /// Big eraser i (tool id 123 + i; icon Icons.MHK 3700 + i).
    BigErase(u8),
    StampSize(u32),
    Stamp(u16),
    StampEditor,
    Grab(GrabTool),
    GrabAct(GrabAction),
    Cut(CutShape),
    /// Eraser: false = Little, true = Big.
    BigEraser(bool),
    EraserRound(bool),
    TextSize(f32),
    TextFont(usize),
    Letter(u16),
    /// A brush of the original brush engine (kp-brushes).
    KpBrush(kp_brushes::BrushId),
}

#[derive(Clone)]
pub struct Group {
    pub kind: G,
    pub items: Vec<Item>,
    pub sel: usize,
    pub page: usize,
}

#[derive(Clone)]
pub struct Tray {
    pub groups: Vec<Group>,
}

impl Tray {
    pub fn group(&self, g: G) -> Option<&Group> {
        self.groups.iter().find(|x| x.kind == g)
    }
    pub fn get(&self, g: G) -> Option<&Item> {
        let grp = self.group(g)?;
        grp.items.get(grp.sel)
    }
    fn sel_of(&self, g: G) -> usize {
        self.group(g).map(|x| x.sel).unwrap_or(0)
    }
    pub fn size_idx(&self) -> u8 {
        match self.get(G::Size) {
            Some(Item::Size(s)) => *s,
            _ => 1,
        }
    }
}

fn grp(kind: G, items: Vec<Item>, sel: usize) -> Group {
    let sel = sel.min(items.len().saturating_sub(1));
    Group { kind, items, sel, page: 0 }
}

fn sizes(n: u8) -> Vec<Item> {
    (0..n).map(Item::Size).collect()
}

/// Build (or rebuild, keeping selections from `prev`) the tray for a tool.
pub fn build(tool: ToolKind, assets: &Assets, font_count: usize, prev: Option<&Tray>) -> Tray {
    let keep = |g: G| prev.map(|p| p.sel_of(g)).unwrap_or(0);
    let keep_page = |g: G| prev.and_then(|p| p.group(g)).map(|x| x.page).unwrap_or(0);
    let mut groups = Vec::new();
    match tool {
        ToolKind::Pencil => {
            groups.push(grp(G::Variant, [DrawTool::Pencil, DrawTool::Chalk, DrawTool::Crayon, DrawTool::Marker].map(Item::Draw).to_vec(), keep(G::Variant)));
            groups.push(grp(G::Size, sizes(3), prev.map(|p| p.sel_of(G::Size)).unwrap_or(1)));
            groups.push(grp(
                G::Shape,
                [Shape::Freehand, Shape::Curve, Shape::Line, Shape::Rect, Shape::Oval, Shape::Polygon].map(Item::Shape).to_vec(),
                keep(G::Shape),
            ));
            groups.push(grp(G::FillMode, vec![Item::Filled(true), Item::Filled(false)], prev.map(|p| p.sel_of(G::FillMode)).unwrap_or(1)));
        }
        ToolKind::Brush => {
            // RE_BRUSHES §1.1: Paint Brush (realistic 0–19 / wacky 20–44), Sound Art
            // (voice 45–54 / hands-free 55–64), Spray (Cyclic shapes / Anims moving paint).
            // Defaults: realistic, mouse, shapes; size index 1.
            use kp_brushes::BrushId as B;
            let variant = keep(G::Variant);
            groups.push(grp(G::Variant, [PaintTool::Brush, PaintTool::SoundArt, PaintTool::Spray].map(Item::Paint).to_vec(), variant));
            let mode = keep(G::Mode);
            let (modes, list): (Vec<Item>, Vec<B>) = match variant {
                0 => (vec![Item::Wacky(false), Item::Wacky(true)], if mode == 1 { kp_brushes::WACKY.to_vec() } else { kp_brushes::REALISTIC.to_vec() }),
                1 => (vec![Item::HandsFree(false), Item::HandsFree(true)], (1..=10).map(if mode == 1 { B::Touchless } else { B::CustomVoiceBrush }).collect()),
                _ => (
                    vec![Item::Moving(false), Item::Moving(true)],
                    if mode == 1 { (0..kp_brushes::ids::SPRAY_MOVING_SETS).map(B::SprayGun).collect() } else { (0..kp_brushes::ids::SPRAY_SHAPE_SETS).map(B::CyclicBitmap).collect() },
                ),
            };
            groups.push(grp(G::Size, sizes(3), prev.map(|p| p.sel_of(G::Size)).unwrap_or(1)));
            groups.push(grp(G::Mode, modes, mode));
            let mut g = grp(G::Options, list.into_iter().map(Item::KpBrush).collect(), keep(G::Options));
            g.page = keep_page(G::Options);
            groups.push(g);
        }
        ToolKind::Fill => {
            let bucket = keep(G::Variant);
            groups.push(grp(G::Variant, [Bucket::Solid, Bucket::Blend, Bucket::Pattern].map(Item::Bucket).to_vec(), bucket));
            // The icon list counts up from the base id until a bitmap is missing.
            let tp = assets.archive(crate::tools::TP);
            let count = |base: u16| tp.as_ref().map(|a| (0..).take_while(|i| a.has(assets::T_BMP, base + i)).count() as u16).unwrap_or(0);
            let opts: Vec<Item> = match bucket {
                0 => (0..count(21500)).map(|i| Item::Fill(FillKind::Tint(i))).collect(),
                1 => (0..count(21300)).map(|i| Item::Fill(FillKind::Gradient(i))).collect(),
                _ => (0..count(21400)).map(|i| Item::Fill(FillKind::Pattern(i))).collect(),
            };
            let mut g = grp(G::Options, opts, keep(G::Options));
            g.page = keep_page(G::Options);
            groups.push(g);
        }
        ToolKind::Mixer => {
            let wand = keep(G::Variant);
            groups.push(grp(G::Variant, vec![Item::Mini(false), Item::Mini(true)], wand));
            groups.push(grp(G::Size, sizes(3), prev.map(|p| p.sel_of(G::Size)).unwrap_or(1)));
            let opts: Vec<Item> = if wand == 1 { (0..12).map(Item::Wand).collect() } else { (0..22).map(Item::MixFx).collect() };
            groups.push(grp(G::Options, opts, keep(G::Options)));
        }
        ToolKind::Stamp => {
            groups.push(grp(G::Size, (1..=3).map(Item::StampSize).collect(), keep(G::Size)));
            groups.push(grp(G::Actions, vec![Item::StampEditor], 0));
            // The stamps themselves come from the current stamp folder (filled by the easel).
            groups.push(grp(G::Options, Vec::new(), keep(G::Options)));
        }
        ToolKind::Text => {
            let mut v = vec![Item::TextSize(18.0), Item::TextSize(32.0), Item::TextSize(56.0)];
            v.extend((0..font_count).map(Item::TextFont));
            groups.push(grp(G::Size, v, keep(G::Size)));
            let letters = assets
                .archive(crate::tools::TP)
                .map(|a| a.ids(assets::T_BMP).into_iter().filter(|i| (22000..22100).contains(i)).map(Item::Letter).collect())
                .unwrap_or_default();
            groups.push(grp(G::Options, letters, usize::MAX));
        }
        ToolKind::Mover => {
            let scissors = keep(G::Variant);
            groups.push(grp(G::Variant, vec![Item::Grab(GrabTool::Hand), Item::Grab(GrabTool::Scissors)], scissors));
            if scissors == 1 {
                let mut cuts: Vec<Item> = [CutShape::Free, CutShape::Polygon, CutShape::Circle, CutShape::Square].map(Item::Cut).to_vec();
                if let Some(a) = assets.archive("Data/Tools/CutRgns.MHK") {
                    cuts.extend(a.ids(mohawk::Tag(*b"MRGN")).into_iter().map(|id| Item::Cut(CutShape::Cookie(id))));
                }
                groups.push(grp(G::Options, cuts, keep(G::Options)));
            } else {
                groups.push(grp(
                    G::Actions,
                    vec![Item::GrabAct(GrabAction::Trash), Item::GrabAct(GrabAction::Copy), Item::GrabAct(GrabAction::Paste)],
                    0,
                ));
            }
        }
        ToolKind::Eraser => {
            let big = keep(G::Variant);
            groups.push(grp(G::Variant, vec![Item::BigEraser(false), Item::BigEraser(true)], big));
            // Defaults: size value 3, round. Size and shape stay with the big erasers too.
            groups.push(grp(G::Size, sizes(3), prev.map(|p| p.sel_of(G::Size)).unwrap_or(2)));
            groups.push(grp(G::Shape, vec![Item::EraserRound(true), Item::EraserRound(false)], keep(G::Shape)));
            if big == 1 {
                groups.push(grp(G::Options, (0..15).map(Item::BigErase).collect(), keep(G::Options)));
            }
        }
    }
    Tray { groups }
}



/// Does choosing this item change which groups follow (so the tray must be rebuilt)?
pub fn restructures(g: G, tool: ToolKind) -> bool {
    match (tool, g) {
        (ToolKind::Brush, G::Variant | G::Mode) => true,
        (ToolKind::Fill | ToolKind::Mixer | ToolKind::Mover | ToolKind::Eraser, G::Variant) => true,
        _ => false,
    }
}

/// One-shot items act when clicked and don't stay selected.
pub fn one_shot(item: &Item) -> bool {
    matches!(item, Item::GrabAct(_) | Item::StampEditor)
}

/// The CD's own icon for scroller items, where it has one.
/// Icons drawn tinted with the current colour: the solid/texture fills' 8-bit icons
/// (TP 21500+i) and the realistic brushes' 8-bit icons (Icons.MHK 3500–3519) are
/// intensity maps blended over white with the colour (Tint blend mode).
pub fn tinted_icon(item: &Item, assets: &Assets, color: u32) -> Option<Image> {
    let (arch, id) = match item {
        Item::Fill(k @ FillKind::Tint(_)) => (crate::tools::TP, k.icon_id()),
        Item::KpBrush(b) if b.tool_id() < 20 => b.icon()?,
        _ => return None,
    };
    let ix = assets.archive(arch)?.get(assets::T_BMP, id).and_then(|d| mohawk::decode_indexed(d).ok())?;
    let c5 = [(color >> 19) & 31, (color >> 11) & 31, (color >> 3) & 31];
    let ex = |v: u32| (v << 3) | (v >> 2);
    let pixels = ix
        .indices
        .iter()
        .map(|&g| {
            let g = g as u32;
            let ch = |c: u32| ex((31 * (255 - g) + c * g) >> 8);
            0xff00_0000 | ch(c5[0]) << 16 | ch(c5[1]) << 8 | ch(c5[2])
        })
        .collect();
    Some(Image { width: ix.width, height: ix.height, pixels })
}

pub fn original_icon(item: &Item, assets: &Assets) -> Option<Image> {
    let (arch, id) = match item {
        Item::Fill(k) => (crate::tools::TP, k.icon_id()),
        Item::KpBrush(b) => {
            let (arch, id) = b.icon()?;
            if let Some(img) = assets.image(arch, id) {
                return Some((*img).clone());
            }
            // Spray sets: the first cel of the set.
            return assets.sprites(arch, id).and_then(|s| s.cels.first().cloned());
        }
        Item::MixFx(i) => ("Data/Tools/Icons.MHK", 3900 + *i as u16),
        Item::Wand(i) => ("Data/Tools/Icons.MHK", 3800 + *i as u16),
        Item::BigErase(i) => ("Data/Tools/Icons.MHK", 3700 + *i as u16),
        Item::Cut(c) => (
            crate::regions::CUT_ARCHIVE,
            match c {
                CutShape::Free => 13000,
                CutShape::Polygon => 13001,
                CutShape::Circle => 13002,
                CutShape::Square => 13003,
                CutShape::Cookie(id) => *id,
            },
        ),
        _ => return None,
    };
    assets.image(arch, id).map(|i| (*i).clone())
}

// ------------------------------------------------------------------ icons

use crate::gfx::Image;






