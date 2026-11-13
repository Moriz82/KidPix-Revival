//! Tool trays, organised like the original (User Guide, "Using the Art Tools"):
//! a tool's tray has a left-hand group of tool variants, then sizes, shapes, modes and a
//! scrolling list of options. Each group remembers its own selection.

use crate::assets::{self, Assets};
use crate::paint::{Effect, Mix};
use crate::tools::{EraserKind, FillKind, ToolKind, Wacky};
#[allow(unused_imports)]
use crate::paint::Mix as _MixAlias;

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
    /// A realistic paintbrush: KPD3.cfg section name.
    Real(String),
    Brush(Wacky),
    /// 3D spray (Cyclic.MHK set) / moving paint (Anims.MHK set).
    Spray3D(u16),
    SprayMoving(u16),
    Bucket(Bucket),
    Fill(FillKind),
    /// Mixer: false = Mega (whole picture), true = Mini (where you drag).
    Mini(bool),
    Mix(Mix),
    MiniMix(Effect),
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
    Erase(EraserKind),
    TextSize(f32),
    TextFont(usize),
    Letter(u16),
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
    pub tool: ToolKind,
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
            let variant = keep(G::Variant);
            groups.push(grp(G::Variant, [PaintTool::Brush, PaintTool::SoundArt, PaintTool::Spray].map(Item::Paint).to_vec(), variant));
            match variant {
                0 => {
                    groups.push(grp(G::Size, sizes(3), prev.map(|p| p.sel_of(G::Size)).unwrap_or(1)));
                    let wacky = prev.map(|p| p.sel_of(G::Mode)).unwrap_or(1);
                    groups.push(grp(G::Mode, vec![Item::Wacky(false), Item::Wacky(true)], wacky));
                    let opts = if wacky == 1 { wacky_brushes() } else { realistic_brushes(assets) };
                    let mut g = grp(G::Options, opts, keep(G::Options));
                    g.page = keep_page(G::Options);
                    groups.push(g);
                }
                1 => {
                    groups.push(grp(G::Size, sizes(3), prev.map(|p| p.sel_of(G::Size)).unwrap_or(1)));
                    groups.push(grp(G::Mode, vec![Item::HandsFree(false), Item::HandsFree(true)], keep(G::Mode)));
                }
                _ => {
                    groups.push(grp(G::Size, sizes(3), prev.map(|p| p.sel_of(G::Size)).unwrap_or(1)));
                    let moving = keep(G::Mode);
                    groups.push(grp(G::Mode, vec![Item::Moving(false), Item::Moving(true)], moving));
                    let (arch, mk): (&str, fn(u16) -> Item) =
                        if moving == 1 { (crate::tools::ANIMS, Item::SprayMoving) } else { (crate::tools::CYCLIC, Item::Spray3D) };
                    let opts = assets.archive(arch).map(|a| a.ids(assets::T_BMH).into_iter().map(mk).collect()).unwrap_or_default();
                    groups.push(grp(G::Options, opts, keep(G::Options)));
                }
            }
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
            if big == 1 {
                groups.push(grp(G::Options, (0..15).map(Item::BigErase).collect(), keep(G::Options)));
            } else {
                // Defaults: size value 3, round.
                groups.push(grp(G::Size, sizes(3), prev.map(|p| p.sel_of(G::Size)).unwrap_or(2)));
                groups.push(grp(G::Shape, vec![Item::EraserRound(true), Item::EraserRound(false)], keep(G::Shape)));
            }
        }
    }
    Tray { tool, groups }
}

fn wacky_brushes() -> Vec<Item> {
    use crate::paint::Effect as E;
    let mut v: Vec<Item> = [Wacky::Rainbow, Wacky::Drippy, Wacky::Bubbles, Wacky::Splatter, Wacky::ZigZag, Wacky::Dots, Wacky::Pine, Wacky::Echo, Wacky::Extrusion]
        .map(Item::Brush)
        .to_vec();
    v.extend([E::Blur, E::Mosaic, E::Invert, E::Greyscale, E::Sepia, E::Posterize, E::Solarize, E::HueShift, E::Emboss, E::Edge].map(|e| Item::Brush(Wacky::Effect(e))));
    v.push(Item::Brush(Wacky::Smear));
    v
}

fn realistic_brushes(assets: &Assets) -> Vec<Item> {
    let mut v = Vec::new();
    for sec in ["Airbrush", "BristleBrush", "Drippy"] {
        if assets.cfg.has_section(sec) {
            v.push(Item::Real(sec.to_string()));
        }
    }
    for n in 1..=16 {
        let sec = format!("CustomBrush{n}");
        if assets.cfg.has_section(&sec) {
            v.push(Item::Real(sec));
        }
    }
    v
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
pub fn original_icon(item: &Item, assets: &Assets) -> Option<Image> {
    let (arch, id) = match item {
        Item::Fill(k) => (crate::tools::TP, k.icon_id()),
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

use crate::gfx::{self, Image};
use crate::paint::{self as pnt, Pad, BLACK, WHITE};
use crate::tools::{self, Opt, PadBank};

fn cursor_icon(assets: &Assets, n: u16, size: u32) -> Option<Image> {
    let c = assets.cursor(10000 + n)?;
    let big = gfx::resize_nearest(&c.image, 2);
    let mut out = Image::filled(size, size, WHITE);
    gfx::blit(&mut out, &big, (size as i32 - 32) / 2, (size as i32 - 32) / 2);
    Some(out)
}

/// Icon for a tray item.
pub fn icon(item: &Item, assets: &Assets, pads: &PadBank, fonts: &crate::ui::Fonts, size: u32) -> Image {
    let s = size as f32;
    let blue = gfx::rgb(30, 60, 200);
    let mut img = Image::filled(size, size, WHITE);
    let squig = |im: &mut Image, f: &mut dyn FnMut(&mut Image, (f32, f32), (f32, f32))| {
        let mut last = (s * 0.15, s * 0.6);
        for i in 1..=20 {
            let t = i as f32 / 20.0;
            let p = (s * (0.15 + 0.7 * t), s * (0.5 + 0.2 * (t * 6.0).sin()));
            f(im, last, p);
            last = p;
        }
    };
    let to_opt = |it: &Item| -> Option<Opt> {
        Some(match it {
            Item::Brush(w) => Opt::Wacky(*w, ""),
            Item::Fill(k) => Opt::Fill(*k, ""),
            Item::Mix(m) => Opt::Mix(*m, ""),
            Item::Erase(k) => Opt::Eraser(*k, ""),
            Item::TextSize(z) => Opt::TextSize(*z),
            Item::TextFont(i) => Opt::TextFont(*i),
            Item::Letter(i) => Opt::Letter(*i),
            Item::Real(sec) => Opt::Media(Box::leak(sec.clone().into_boxed_str()), ""),
            _ => return None,
        })
    };
    if let Some(o) = to_opt(item) {
        return tools::option_icon(&o, assets, pads, fonts, size);
    }
    match item {
        Item::Draw(t) => {
            let sec = match t {
                DrawTool::Pencil => None,
                DrawTool::Chalk => Some("Chalk"),
                DrawTool::Crayon => Some("Crayon"),
                DrawTool::Marker => Some("Marker"),
            };
            match sec {
                None => squig(&mut img, &mut |im, a, b| pnt::line(im, a.0, a.1, b.0, b.1, 3.0, BLACK)),
                Some(sec) => {
                    let p = tools::media_params(assets, sec);
                    let pad = tools::pad_for(pads, &p, 6.0);
                    let mut st = pnt::MediaStroke::new(p, pad, 3);
                    squig(&mut img, &mut |im, _, b| st.to(im, b.0, b.1, blue));
                }
            }
        }
        Item::Size(n) => gfx::fill_circle(&mut img, s / 2.0, s / 2.0, 2.0 + *n as f32 * 3.5, BLACK),
        Item::StampSize(n) => gfx::fill_rect(&mut img, (s / 2.0) as i32 - 4 * *n as i32, (s / 2.0) as i32 - 4 * *n as i32, 8 * *n as i32, 8 * *n as i32, blue),
        Item::Shape(k) => {
            let (a, b) = (s * 0.2, s * 0.8);
            match k {
                Shape::Freehand => squig(&mut img, &mut |im, p, q| pnt::line(im, p.0, p.1, q.0, q.1, 2.0, BLACK)),
                Shape::Curve => {
                    let mut last = (a, b);
                    for i in 1..=20 {
                        let t = i as f32 / 20.0;
                        let p = (a + (b - a) * t, b - (b - a) * 4.0 * t * (1.0 - t));
                        pnt::line(&mut img, last.0, last.1, p.0, p.1, 2.0, BLACK);
                        last = p;
                    }
                }
                Shape::Line => pnt::line(&mut img, a, b, b, a, 2.0, BLACK),
                Shape::Rect => gfx::stroke_rect(&mut img, a as i32, (a + 3.0) as i32, (b - a) as i32, (b - a - 6.0) as i32, 2, BLACK),
                Shape::Oval => {
                    let pts = ellipse_points(s / 2.0, s / 2.0, (b - a) / 2.0, (b - a) / 2.6, 40);
                    stroke_poly(&mut img, &pts, 2.0, BLACK, true);
                }
                Shape::Polygon => stroke_poly(&mut img, &[(a, b), (s / 2.0, a), (b, b * 0.85), (s * 0.55, b)], 2.0, BLACK, true),
            }
        }
        Item::Filled(f) => {
            let (a, w) = ((s * 0.22) as i32, (s * 0.56) as i32);
            if *f {
                gfx::fill_rect(&mut img, a, a, w, w, blue);
            }
            gfx::stroke_rect(&mut img, a, a, w, w, 2, BLACK);
        }
        Item::Paint(t) => match t {
            PaintTool::Brush => {
                let p = tools::media_params(assets, "CustomBrush1");
                let pad = tools::pad_for(pads, &p, 8.0);
                let mut st = pnt::MediaStroke::new(p, pad, 3);
                squig(&mut img, &mut |im, _, b| st.to(im, b.0, b.1, blue));
            }
            PaintTool::SoundArt => {
                if let Some(m) = assets.image("Data/Misc.MHK", 31500) {
                    img = gfx::resize(&m, size, size);
                }
            }
            PaintTool::Spray => {
                let mut r = pnt::Rng::new(5);
                for _ in 0..90 {
                    let a = r.range(0.0, 6.28);
                    let d = r.range(0.0, s * 0.35);
                    img.put((s / 2.0 + a.cos() * d) as i32, (s / 2.0 + a.sin() * d) as i32, blue);
                }
            }
        },
        Item::Wacky(w) => {
            if *w {
                return tools::option_icon(&Opt::Wacky(Wacky::Rainbow, ""), assets, pads, fonts, size);
            }
            let p = tools::media_params(assets, "Airbrush");
            let pad = Pad::round(8, true);
            let mut st = pnt::MediaStroke::new(p, pad, 3);
            squig(&mut img, &mut |im, _, b| st.to(im, b.0, b.1, blue));
        }
        Item::HandsFree(h) => {
            if *h {
                if let Some(m) = assets.image("Data/Misc.MHK", 31500) {
                    img = gfx::resize(&m, size, size);
                }
            } else if let Some(c) = cursor_icon(assets, 28, size) {
                img = c;
            }
        }
        Item::Moving(m) => {
            if *m {
                squig(&mut img, &mut |im, _, b| gfx::fill_circle(im, b.0, b.1, 2.5, gfx::rgb(240, 80, 160)));
            } else {
                for k in 0..10 {
                    let t = k as f32 / 9.0;
                    gfx::fill_circle(&mut img, s / 2.0 - t * 4.0, s / 2.0 - t * 4.0, s * 0.32 - t * s * 0.25, gfx::lerp(gfx::rgb(120, 20, 20), gfx::rgb(255, 200, 200), (t * 255.0) as u32));
                }
            }
        }
        Item::Spray3D(id) | Item::SprayMoving(id) => {
            let arch = if matches!(item, Item::Spray3D(_)) { tools::CYCLIC } else { tools::ANIMS };
            if let Some(set) = assets.sprites(arch, *id) {
                if let Some(c) = set.cels.first() {
                    let k = (s / c.width.max(c.height) as f32).min(1.0);
                    let t = gfx::resize(c, ((c.width as f32 * k) as u32).max(1), ((c.height as f32 * k) as u32).max(1));
                    gfx::blit(&mut img, &t, (size as i32 - t.width as i32) / 2, (size as i32 - t.height as i32) / 2);
                }
            }
        }
        Item::Bucket(b) => {
            let (a, w) = ((s * 0.18) as i32, (s * 0.64) as i32);
            match b {
                Bucket::Solid => gfx::fill_rect(&mut img, a, a, w, w, blue),
                Bucket::Blend => {
                    for y in 0..w {
                        gfx::fill_rect(&mut img, a, a + y, w, 1, gfx::lerp(gfx::rgb(255, 230, 0), gfx::rgb(200, 0, 160), (y * 255 / w) as u32));
                    }
                }
                Bucket::Pattern => {
                    for y in 0..w {
                        for x in 0..w {
                            let c = if (x / 6 + y / 6) % 2 == 0 { gfx::rgb(240, 70, 70) } else { gfx::rgb(70, 160, 240) };
                            img.put(a + x, a + y, c);
                        }
                    }
                }
            }
            gfx::stroke_rect(&mut img, a, a, w, w, 1, BLACK);
        }
        Item::Mini(m) => {
            let r = if *m { s * 0.16 } else { s * 0.36 };
            gfx::fill_circle(&mut img, s / 2.0, s / 2.0, r, gfx::rgb(250, 160, 40));
            gfx::fill_circle(&mut img, s / 2.0, s / 2.0, r * 0.5, gfx::rgb(80, 160, 240));
        }
        Item::MiniMix(e) => {
            for y in 0..size as i32 {
                for x in 0..size as i32 {
                    img.put(x, y, gfx::hsv((x * 7 + y * 3) as f32, 0.7, if (x / 6 + y / 6) % 2 == 0 { 1.0 } else { 0.6 }));
                }
            }
            pnt::effect_dab(&mut img, *e, s / 2.0, s / 2.0, s * 0.35);
        }
        Item::Stamp(id) => {
            let _ = id;
        }
        Item::StampEditor => {
            if let Some(c) = cursor_icon(assets, 1, size) {
                img = c;
            }
        }
        Item::Grab(g) => {
            if let Some(c) = cursor_icon(assets, if *g == GrabTool::Hand { 9 } else { 13 }, size) {
                img = c;
            }
        }
        Item::GrabAct(a) => {
            let (x0, w) = ((s * 0.25) as i32, (s * 0.5) as i32);
            match a {
                GrabAction::Trash => {
                    gfx::fill_rect(&mut img, x0, x0 + 4, w, w, gfx::rgb(120, 120, 130));
                    gfx::fill_rect(&mut img, x0 - 3, x0, w + 6, 4, gfx::rgb(80, 80, 90));
                }
                GrabAction::Copy => {
                    gfx::stroke_rect(&mut img, x0 - 4, x0 - 4, w, w, 2, BLACK);
                    gfx::fill_rect(&mut img, x0 + 4, x0 + 4, w, w, gfx::rgb(250, 200, 60));
                    gfx::stroke_rect(&mut img, x0 + 4, x0 + 4, w, w, 2, BLACK);
                }
                GrabAction::Paste => {
                    gfx::fill_round_rect(&mut img, x0, x0 + 2, w, w + 4, 6, gfx::rgb(180, 220, 250));
                    gfx::fill_rect(&mut img, x0 + 2, x0 - 3, w - 4, 5, gfx::rgb(200, 60, 60));
                }
            }
        }
        Item::Cut(c) => match c {
            // The CD's own Scissors icons: tBMP 13000-13003 (free, polygon, circle, square)
            // and 13004+ for the cookie cutters (same ids as their MRGN shapes).
            _ if cut_icon(assets, c).is_some() => {
                let i = cut_icon(assets, c).unwrap();
                img = gfx::resize(&i, size, size);
            }
            CutShape::Cookie(id) => {
                if let Some(m) = crate::regions::load(assets, *id) {
                    let k = (s * 0.8 / m.width.max(m.height) as f32).min(1.0);
                    let t = gfx::resize(&m.to_image(gfx::rgb(250, 170, 40)), ((m.width as f32 * k) as u32).max(1), ((m.height as f32 * k) as u32).max(1));
                    gfx::blit(&mut img, &t, (size as i32 - t.width as i32) / 2, (size as i32 - t.height as i32) / 2);
                }
            }
            CutShape::Free => squig(&mut img, &mut |im, p, q| dashed_line(im, p, q)),
            CutShape::Polygon => {
                for w in [(0.2, 0.8, 0.5, 0.2), (0.5, 0.2, 0.8, 0.8), (0.8, 0.8, 0.2, 0.8)] {
                    dashed_line(&mut img, (w.0 * s, w.1 * s), (w.2 * s, w.3 * s));
                }
            }
            CutShape::Circle => {
                let pts = ellipse_points(s / 2.0, s / 2.0, s * 0.3, s * 0.3, 24);
                for i in 0..pts.len() {
                    if i % 2 == 0 {
                        pnt::line(&mut img, pts[i].0, pts[i].1, pts[(i + 1) % pts.len()].0, pts[(i + 1) % pts.len()].1, 1.0, BLACK);
                    }
                }
            }
            CutShape::Square => {
                let (a, b) = (s * 0.22, s * 0.78);
                for (p, q) in [((a, a), (b, a)), ((b, a), (b, b)), ((b, b), (a, b)), ((a, b), (a, a))] {
                    dashed_line(&mut img, p, q);
                }
            }
        },
        Item::BigEraser(b) => {
            if let Some(c) = cursor_icon(assets, if *b { 4 } else { 3 }, size) {
                img = c;
            }
        }
        Item::EraserRound(r) => {
            if *r {
                gfx::fill_circle(&mut img, s / 2.0, s / 2.0, s * 0.3, BLACK);
                gfx::fill_circle(&mut img, s / 2.0, s / 2.0, s * 0.3 - 2.0, WHITE);
            } else {
                let a = (s * 0.2) as i32;
                gfx::stroke_rect(&mut img, a, a, (s * 0.6) as i32, (s * 0.6) as i32, 2, BLACK);
            }
        }
        _ => {}
    }
    img
}

fn cut_icon(assets: &Assets, c: &CutShape) -> Option<std::rc::Rc<Image>> {
    let id = match c {
        CutShape::Free => 13000,
        CutShape::Polygon => 13001,
        CutShape::Circle => 13002,
        CutShape::Square => 13003,
        CutShape::Cookie(id) => *id,
    };
    assets.image(crate::regions::CUT_ARCHIVE, id)
}

fn dashed_line(img: &mut Image, a: (f32, f32), b: (f32, f32)) {
    let mut k = 0;
    gfx::walk_line(a.0, a.1, b.0, b.1, 1.0, |x, y| {
        if (k / 3) % 2 == 0 {
            img.put(x as i32, y as i32, BLACK);
        }
        k += 1;
    });
}

pub fn ellipse_points(cx: f32, cy: f32, rx: f32, ry: f32, n: usize) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            (cx + a.cos() * rx, cy + a.sin() * ry)
        })
        .collect()
}

pub fn stroke_poly(img: &mut Image, pts: &[(f32, f32)], w: f32, c: u32, closed: bool) {
    for i in 0..pts.len() {
        if i + 1 == pts.len() && !closed {
            break;
        }
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        pnt::line(img, a.0, a.1, b.0, b.1, w, c);
    }
}
