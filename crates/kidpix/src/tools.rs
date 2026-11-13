//! The eight easel tools, their option lists and stroke logic.

use crate::assets::Assets;
use crate::gfx::{self, Image};
use crate::paint::{self, Effect, FillStyle, MediaParams, MediaStroke, Mix, Pad, Rng, BLACK, WHITE};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Pencil,
    Brush,
    Fill,
    Mixer,
    Stamp,
    Text,
    Mover,
    Eraser,
}

impl ToolKind {
    pub fn from_command(cmd: u16) -> Option<ToolKind> {
        Some(match cmd {
            232 => ToolKind::Pencil,
            233 => ToolKind::Brush,
            234 => ToolKind::Fill,
            237 => ToolKind::Mixer,
            236 => ToolKind::Stamp,
            229 => ToolKind::Text,
            227 => ToolKind::Mover,
            228 => ToolKind::Eraser,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Wacky {
    Rainbow,
    Drippy,
    Bubbles,
    Splatter,
    ZigZag,
    Dots,
    Pine,
    Echo,
    Effect(Effect),
    Smear,
    Extrusion,
    Custom(u8),
    /// Animated sprite trail from `Data/Tools/Cyclic.MHK` (tBMH id).
    Cyclic(u16),
    /// Animated spray from `Data/Tools/Anims.MHK` (tBMH id).
    Spray(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EraserKind {
    Square(i32),
    ClearFade,
    ClearWipe,
    ClearDissolve,
    ClearIris,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillKind {
    /// Solid / single-colour texture i: TP tBMP 20500+i tinted with the current colour
    /// (icon 21500+i; i = 0 is plain solid).
    Tint(u16),
    /// Gradient i: entry i of TP `grad` 21300 (icon 21300+i).
    Gradient(u16),
    /// Multi-colour pattern i: TP tBMP 20400+i tiled (icon 21400+i).
    Pattern(u16),
}

impl FillKind {
    pub fn icon_id(self) -> u16 {
        match self {
            FillKind::Tint(i) => 21500 + i,
            FillKind::Gradient(i) => 21300 + i,
            FillKind::Pattern(i) => 21400 + i,
        }
    }
}

pub const TP: &str = "Data/Tools/TP.MHK";
pub const CYCLIC: &str = "Data/Tools/Cyclic.MHK";
pub const ANIMS: &str = "Data/Tools/Anims.MHK";

/// Archive + set id of a sprite-trail brush.
pub fn trail_source(k: Wacky) -> Option<(&'static str, u16)> {
    match k {
        Wacky::Cyclic(id) => Some((CYCLIC, id)),
        Wacky::Spray(id) => Some((ANIMS, id)),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub enum Opt {
    PencilSize(f32),
    Media(&'static str, &'static str),
    Wacky(Wacky, &'static str),
    Fill(FillKind, &'static str),
    Mix(Mix, &'static str),
    StampSize(u32),
    StampLibrary,
    TextSize(f32),
    TextFont(usize),
    /// Alphabet stamp (TP.MHK tBMP id; the tWAV with the same id says it).
    Letter(u16),
    MoverMove,
    MoverCopy,
    MoverObjects,
    Eraser(EraserKind, &'static str),
}

impl Opt {
    pub fn label(&self) -> String {
        match self {
            Opt::PencilSize(s) => format!("Pencil {s}"),
            Opt::Media(_, l) | Opt::Wacky(_, l) | Opt::Fill(_, l) | Opt::Mix(_, l) | Opt::Eraser(_, l) => l.to_string(),
            Opt::StampSize(s) => format!("Stamp size {s}"),
            Opt::StampLibrary => "Pick a stamp".into(),
            Opt::TextSize(s) => format!("Letters size {s}"),
            Opt::TextFont(_) => "Font".into(),
            Opt::Letter(_) => "Letter stamp".into(),
            Opt::MoverMove => "Move a piece".into(),
            Opt::MoverCopy => "Copy a piece".into(),
            Opt::MoverObjects => "Move stickers".into(),
        }
    }
}

/// Build the option list for a tool. Brush variants that the user's KPD3.cfg defines are
/// included; their parameters are read from it at paint time.
pub fn options(tool: ToolKind, assets: &Assets, font_count: usize) -> Vec<Opt> {
    match tool {
        ToolKind::Pencil => {
            let mut v = vec![
                Opt::PencilSize(1.0),
                Opt::PencilSize(3.0),
                Opt::PencilSize(7.0),
                Opt::PencilSize(14.0),
            ];
            for (sec, label) in [("Crayon", "Crayon"), ("Chalk", "Chalk"), ("Marker", "Marker"), ("Airbrush", "Airbrush")] {
                if assets.cfg.has_section(sec) {
                    v.push(Opt::Media(sec, label));
                }
            }
            v
        }
        ToolKind::Brush => {
            let mut v = vec![
                Opt::Wacky(Wacky::Rainbow, "Rainbow"),
                Opt::Wacky(Wacky::Drippy, "Drippy paint"),
                Opt::Wacky(Wacky::Bubbles, "Bubbles"),
                Opt::Wacky(Wacky::Splatter, "Splatter"),
                Opt::Wacky(Wacky::ZigZag, "Zig zag"),
                Opt::Wacky(Wacky::Dots, "Dots"),
                Opt::Wacky(Wacky::Pine, "Pine tree"),
                Opt::Wacky(Wacky::Echo, "Echo"),
                Opt::Wacky(Wacky::Extrusion, "3-D tube"),
                Opt::Wacky(Wacky::Smear, "Smear"),
                Opt::Wacky(Wacky::Effect(Effect::Blur), "Blur"),
                Opt::Wacky(Wacky::Effect(Effect::Mosaic), "Mosaic"),
                Opt::Wacky(Wacky::Effect(Effect::Invert), "X-ray"),
                Opt::Wacky(Wacky::Effect(Effect::Greyscale), "Grey"),
                Opt::Wacky(Wacky::Effect(Effect::Sepia), "Old photo"),
                Opt::Wacky(Wacky::Effect(Effect::Posterize), "Posterize"),
                Opt::Wacky(Wacky::Effect(Effect::Solarize), "Solarize"),
                Opt::Wacky(Wacky::Effect(Effect::HueShift), "Color shift"),
                Opt::Wacky(Wacky::Effect(Effect::Emboss), "Emboss"),
                Opt::Wacky(Wacky::Effect(Effect::Edge), "Outline"),
            ];
            for n in 1..=16u8 {
                if assets.cfg.has_section(&format!("CustomBrush{n}")) {
                    v.push(Opt::Wacky(Wacky::Custom(n), "Fancy brush"));
                }
            }
            if let Some(a) = assets.archive("Data/Tools/Cyclic.MHK") {
                for id in a.ids(crate::assets::T_BMH) {
                    v.push(Opt::Wacky(Wacky::Cyclic(id), "Picture trail"));
                }
            }
            if let Some(a) = assets.archive(ANIMS) {
                for id in a.ids(crate::assets::T_BMH) {
                    v.push(Opt::Wacky(Wacky::Spray(id), "Magic spray"));
                }
            }
            v
        }
        ToolKind::Fill => vec![Opt::Fill(FillKind::Tint(0), "Fill")],
        ToolKind::Mixer => vec![
            Opt::Mix(Mix::Effect(Effect::Invert), "Invert"),
            Opt::Mix(Mix::Kaleidoscope, "Kaleidoscope"),
            Opt::Mix(Mix::FlipH, "Flip sideways"),
            Opt::Mix(Mix::FlipV, "Flip upside down"),
            Opt::Mix(Mix::Ripple, "Ripple"),
            Opt::Mix(Mix::Swirl, "Swirl"),
            Opt::Mix(Mix::Shatter, "Shatter"),
            Opt::Mix(Mix::Effect(Effect::Mosaic), "Mosaic"),
            Opt::Mix(Mix::Effect(Effect::Blur), "Soften"),
            Opt::Mix(Mix::Effect(Effect::Greyscale), "Grey"),
            Opt::Mix(Mix::Effect(Effect::Sepia), "Old photo"),
            Opt::Mix(Mix::Effect(Effect::Posterize), "Posterize"),
            Opt::Mix(Mix::Effect(Effect::Emboss), "Emboss"),
            Opt::Mix(Mix::Effect(Effect::Edge), "Outline"),
            Opt::Mix(Mix::Effect(Effect::HueShift), "Color shift"),
        ],
        ToolKind::Stamp => vec![
            Opt::StampLibrary,
            Opt::StampSize(1),
            Opt::StampSize(2),
            Opt::StampSize(3),
        ],
        ToolKind::Text => {
            let mut v = vec![Opt::TextSize(18.0), Opt::TextSize(32.0), Opt::TextSize(56.0)];
            for i in 0..font_count {
                v.push(Opt::TextFont(i));
            }
            if let Some(a) = assets.archive(TP) {
                for id in a.ids(crate::assets::T_BMP) {
                    if (22000..22100).contains(&id) {
                        v.push(Opt::Letter(id));
                    }
                }
            }
            v
        }
        ToolKind::Mover => vec![Opt::MoverMove, Opt::MoverCopy, Opt::MoverObjects],
        ToolKind::Eraser => vec![
            Opt::Eraser(EraserKind::Square(6), "Small eraser"),
            Opt::Eraser(EraserKind::Square(16), "Eraser"),
            Opt::Eraser(EraserKind::Square(34), "Big eraser"),
            Opt::Eraser(EraserKind::ClearFade, "Fade away"),
            Opt::Eraser(EraserKind::ClearWipe, "Window shade"),
            Opt::Eraser(EraserKind::ClearDissolve, "Dissolve"),
            Opt::Eraser(EraserKind::ClearIris, "Black hole"),
        ],
    }
}

/// Loaded brush pads (Data/Tools/Brushes.MHK) keyed by tBMP id.
pub struct PadBank {
    pads: std::collections::HashMap<u16, Pad>,
}

impl PadBank {
    pub fn load(assets: &Assets) -> PadBank {
        let mut pads = std::collections::HashMap::new();
        if let Some(a) = assets.archive("Data/Tools/Brushes.MHK") {
            for id in a.ids(crate::assets::T_BMP) {
                let Some(d) = a.get(crate::assets::T_BMP, id) else { continue };
                let Ok(h) = mohawk::BitmapHeader::parse(d) else { continue };
                let Ok(img) = mohawk::decode_bitmap(d, None) else { continue };
                pads.insert(id, Pad::from_image(&img, h.bpp_code() == 3));
            }
        }
        PadBank { pads }
    }
    pub fn get(&self, id: u16) -> Option<&Pad> {
        self.pads.get(&id)
    }
}

/// How a Drawing Tool lays down a path.
#[derive(Clone)]
pub enum Pen {
    Pencil(f32),
    Media(MediaParams, Pad, Option<Rc<Image>>),
}

/// In-progress stroke state.
pub enum Stroke {
    None,
    Pencil { last: (f32, f32), width: f32 },
    Media(MediaStroke),
    Wacky { kind: Wacky, last: (f32, f32), dist: f32, rng: Rng, hue: f32, cel: usize, media: Option<MediaStroke> },
    /// Line / rectangle / oval rubber band (base = picture before the shape).
    ShapeDrag { shape: crate::trays::Shape, start: (f32, f32), cur: (f32, f32), filled: bool, pen: Pen, base: Image },
    /// Curve: after the first drag the line follows the mouse to bend; a click finishes.
    CurveBend { a: (f32, f32), b: (f32, f32), ctrl: (f32, f32), pen: Pen, base: Image },
    /// Polygon: click for each corner, click the first one to close.
    Polygon { pts: Vec<(f32, f32)>, cur: (f32, f32), filled: bool, pen: Pen, base: Image },
    Eraser { last: (f32, f32), size: i32, round: bool },
    /// Mini mixer: mix where the mouse drags.
    MiniMix { effect: crate::paint::Effect, radius: f32, last: (f32, f32) },
    /// Blend fill: holding the button moves the gradient's focus.
    Blend { region: Vec<(i32, i32)>, grad: crate::fills::Grad },
    /// Spray can (3D shapes or moving paint).
    Spray { set: Rc<crate::assets::SpriteSet>, moving: bool, rng: Rng, pos: (f32, f32), acc: f32, cel: usize, radius: f32 },
    /// Sound Art with the mouse.
    SoundArt { last: (f32, f32), hue: f32 },
    /// Scissors: dragging out a free shape, circle or square.
    Cut { shape: crate::trays::CutShape, pts: Vec<(f32, f32)>, start: (f32, f32), cur: (f32, f32) },
    /// Scissors polygon: click for each corner.
    CutPolygon { pts: Vec<(f32, f32)>, cur: (f32, f32) },
    /// Paste jar: the next click places the clipboard.
    PasteAim,
}

pub fn media_params(assets: &Assets, section: &str) -> MediaParams {
    MediaParams::from_cfg(&assets.cfg, section).unwrap_or(MediaParams {
        brush_id: 1001,
        spacing: 2.0,
        ..Default::default()
    })
}

pub fn pad_for(bank: &PadBank, params: &MediaParams, fallback: f32) -> Pad {
    bank.get(params.brush_id)
        .cloned()
        .unwrap_or_else(|| Pad::round(fallback as i32, true))
}

/// One step of a wacky brush from `last` to `p`.
#[allow(clippy::too_many_arguments)]
pub fn wacky_step(
    img: &mut Image,
    kind: Wacky,
    last: (f32, f32),
    p: (f32, f32),
    dist: &mut f32,
    rng: &mut Rng,
    hue: &mut f32,
    cel: &mut usize,
    color: u32,
    trail: Option<&crate::assets::SpriteSet>,
    media: &mut Option<MediaStroke>,
) {
    let (x0, y0) = last;
    let (x1, y1) = p;
    let seg = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
    match kind {
        Wacky::Rainbow => {
            gfx::walk_line(x0, y0, x1, y1, 1.0, |x, y| {
                *hue = (*hue + 2.5) % 360.0;
                gfx::fill_circle(img, x, y, 7.0, gfx::hsv(*hue, 1.0, 1.0));
            });
        }
        Wacky::Drippy => {
            if let Some(m) = media.as_mut() {
                m.to(img, x1, y1, color);
            } else {
                paint::line(img, x0, y0, x1, y1, 8.0, color);
            }
            if rng.f() < seg / 30.0 {
                let len = rng.range(10.0, 45.0);
                let w = rng.range(2.0, 4.5);
                paint::line(img, x1, y1, x1, y1 + len, w, color);
                gfx::fill_circle(img, x1, y1 + len, w * 0.8, color);
            }
        }
        Wacky::Bubbles => {
            *dist += seg;
            while *dist > 9.0 {
                *dist -= 9.0;
                let r = rng.range(3.0, 14.0);
                let ox = rng.range(-16.0, 16.0);
                let oy = rng.range(-16.0, 16.0);
                ring(img, x1 + ox, y1 + oy, r, color);
            }
        }
        Wacky::Splatter => {
            for _ in 0..(seg as i32 / 2 + 2) {
                let a = rng.range(0.0, std::f32::consts::TAU);
                let d = rng.range(0.0, 22.0).powf(1.2);
                gfx::fill_circle(img, x1 + a.cos() * d, y1 + a.sin() * d, rng.range(0.6, 2.6), color);
            }
        }
        Wacky::ZigZag => {
            *dist += seg;
            let amp = 10.0;
            let nx = -(y1 - y0) / seg.max(0.01);
            let ny = (x1 - x0) / seg.max(0.01);
            let ph0 = ((*dist - seg) / 8.0).sin() * amp;
            let ph1 = (*dist / 8.0).sin() * amp;
            paint::line(img, x0 + nx * ph0, y0 + ny * ph0, x1 + nx * ph1, y1 + ny * ph1, 3.0, color);
        }
        Wacky::Dots => {
            *dist += seg;
            while *dist > 14.0 {
                *dist -= 14.0;
                gfx::fill_circle(img, x1, y1, 5.0, color);
            }
        }
        Wacky::Pine => {
            *dist += seg;
            while *dist > 6.0 {
                *dist -= 6.0;
                let nx = -(y1 - y0) / seg.max(0.01);
                let ny = (x1 - x0) / seg.max(0.01);
                let l = 12.0;
                paint::line(img, x1, y1, x1 + nx * l - (x1 - x0) / seg.max(0.01) * 6.0, y1 + ny * l - (y1 - y0) / seg.max(0.01) * 6.0, 1.5, color);
                paint::line(img, x1, y1, x1 - nx * l - (x1 - x0) / seg.max(0.01) * 6.0, y1 - ny * l - (y1 - y0) / seg.max(0.01) * 6.0, 1.5, color);
            }
            paint::line(img, x0, y0, x1, y1, 2.0, gfx::lerp(color, BLACK, 90));
        }
        Wacky::Echo => {
            for (i, off) in [0.0f32, 8.0, 16.0, 24.0].iter().enumerate() {
                let c = gfx::lerp(color, WHITE, (i as u32) * 60);
                paint::line(img, x0 + off, y0 + off, x1 + off, y1 + off, 3.0, c);
            }
        }
        Wacky::Extrusion => {
            gfx::walk_line(x0, y0, x1, y1, 1.0, |x, y| {
                for k in 0..10 {
                    let t = k as f32 / 9.0;
                    let c = gfx::lerp(gfx::lerp(color, BLACK, 120), gfx::lerp(color, WHITE, 140), (t * 255.0) as u32);
                    gfx::fill_circle(img, x - t * 3.0, y - t * 3.0, 9.0 - t * 6.0, c);
                }
            });
        }
        Wacky::Smear => {
            let src = img.clone();
            let dx = (x1 - x0).round() as i32;
            let dy = (y1 - y0).round() as i32;
            let r = 10;
            for yy in -r..=r {
                for xx in -r..=r {
                    if xx * xx + yy * yy > r * r {
                        continue;
                    }
                    let px = x1 as i32 + xx;
                    let py = y1 as i32 + yy;
                    if let (Some(s), Some(d)) = (src.get(px - dx, py - dy), src.get(px, py)) {
                        img.put(px, py, gfx::lerp(d, s, 160));
                    }
                }
            }
        }
        Wacky::Effect(e) => {
            gfx::walk_line(x0, y0, x1, y1, 6.0, |x, y| paint::effect_dab(img, e, x, y, 16.0));
            paint::effect_dab(img, e, x1, y1, 16.0);
        }
        Wacky::Custom(_) => {
            if let Some(m) = media.as_mut() {
                m.to(img, x1, y1, color);
            }
        }
        Wacky::Spray(_) => {
            let Some(set) = trail else { return };
            *dist += seg;
            while *dist >= 18.0 {
                *dist -= 18.0;
                if set.cels.is_empty() {
                    return;
                }
                let c = &set.cels[*cel % set.cels.len()];
                let ox = rng.range(-20.0, 20.0) as i32;
                let oy = rng.range(-20.0, 20.0) as i32;
                gfx::blit(img, c, x1 as i32 + ox - c.width as i32 / 2, y1 as i32 + oy - c.height as i32 / 2);
                *cel += 1;
            }
        }
        Wacky::Cyclic(_) => {
            let Some(set) = trail else { return };
            *dist += seg;
            let step = set
                .cels
                .first()
                .map(|c| (c.width.max(c.height) as f32 * 0.9).max(8.0))
                .unwrap_or(24.0);
            while *dist >= step {
                *dist -= step;
                if set.cels.is_empty() {
                    return;
                }
                let c = &set.cels[*cel % set.cels.len()];
                gfx::blit(img, c, x1 as i32 - c.width as i32 / 2, y1 as i32 - c.height as i32 / 2);
                *cel += 1;
            }
        }
    }
}

fn ring(img: &mut Image, cx: f32, cy: f32, r: f32, color: u32) {
    gfx::fill_circle(img, cx, cy, r, gfx::lerp(color, WHITE, 200));
    let n = (r * 6.0) as i32 + 8;
    for i in 0..n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU;
        gfx::fill_circle(img, cx + a.cos() * r, cy + a.sin() * r, 0.9, color);
    }
    gfx::fill_circle(img, cx - r * 0.35, cy - r * 0.35, r * 0.18, WHITE);
}

/// Render a small preview icon for an option (drawn with the option's own algorithm).
pub fn option_icon(opt: &Opt, assets: &Assets, bank: &PadBank, fonts: &crate::ui::Fonts, size: u32) -> Image {
    let mut img = Image::filled(size, size, WHITE);
    let s = size as f32;
    let color = gfx::rgb(30, 60, 200);
    let mut rng = Rng::new(7);
    let squiggle = |f: &mut dyn FnMut((f32, f32), (f32, f32))| {
        let mut last = (s * 0.15, s * 0.6);
        for i in 1..=24 {
            let t = i as f32 / 24.0;
            let p = (s * (0.15 + 0.7 * t), s * (0.5 + 0.22 * (t * 6.0).sin()));
            f(last, p);
            last = p;
        }
    };
    match opt {
        Opt::PencilSize(w) => {
            let mut im = img.clone();
            squiggle(&mut |a, b| paint::line(&mut im, a.0, a.1, b.0, b.1, *w, BLACK));
            img = im;
        }
        Opt::Media(sec, _) => {
            let params = media_params(assets, sec);
            let pad = pad_for(bank, &params, 6.0);
            let mut st = MediaStroke::new(params, pad, 3);
            let mut im = img.clone();
            squiggle(&mut |_, b| st.to(&mut im, b.0, b.1, color));
            img = im;
        }
        Opt::Wacky(k, _) => {
            // Effects need something to act on.
            if let Wacky::Effect(_) | Wacky::Smear = k {
                for y in 0..size as i32 {
                    for x in 0..size as i32 {
                        img.put(x, y, gfx::hsv((x * 7 + y * 3) as f32, 0.7, if (x / 6 + y / 6) % 2 == 0 { 1.0 } else { 0.6 }));
                    }
                }
            }
            let trail = trail_source(*k).and_then(|(a, id)| assets.sprites(a, id));
            let mut media = match k {
                Wacky::Custom(n) => {
                    let p = media_params(assets, &format!("CustomBrush{n}"));
                    let pad = pad_for(bank, &p, 8.0);
                    Some(MediaStroke::new(p, pad, 5))
                }
                Wacky::Drippy => {
                    let p = media_params(assets, "Drippy");
                    let pad = pad_for(bank, &p, 8.0);
                    Some(MediaStroke::new(p, pad, 5))
                }
                _ => None,
            };
            let (mut dist, mut hue, mut cel) = (0.0, 0.0, 0usize);
            let mut im = img.clone();
            let scale_down = trail.is_some();
            squiggle(&mut |a, b| {
                wacky_step(&mut im, *k, a, b, &mut dist, &mut rng, &mut hue, &mut cel, color, trail.as_deref(), &mut media)
            });
            if scale_down {
                // trails are big; keep as drawn (clipped)
            }
            img = im;
        }
        Opt::Fill(kind, _) => {
            if let Some(i) = assets.image(TP, kind.icon_id()) {
                img = (*i).clone();
            }
        }
        Opt::Letter(id) => {
            if let Some(i) = assets.image(TP, *id) {
                img = gfx::resize(&i, size, size);
            }
        }
        Opt::Mix(m, _) => {
            for y in 0..size as i32 {
                for x in 0..size as i32 {
                    let c = if (x - 12).pow(2) + (y - 14).pow(2) < 80 {
                        gfx::rgb(240, 60, 60)
                    } else if y > size as i32 * 2 / 3 {
                        gfx::rgb(60, 170, 60)
                    } else {
                        gfx::rgb(150, 200, 255)
                    };
                    img.put(x, y, c);
                }
            }
            paint::mix_canvas(&mut img, *m, &mut rng);
        }
        Opt::StampSize(n) => {
            let r = 4.0 + *n as f32 * 4.0;
            gfx::fill_circle(&mut img, s / 2.0, s / 2.0, r, color);
        }
        Opt::StampLibrary => {
            for i in 0..4 {
                let x = 6 + (i % 2) * 18;
                let y = 6 + (i / 2) * 18;
                gfx::fill_round_rect(&mut img, x, y, 16, 16, 4, gfx::hsv(i as f32 * 80.0, 0.7, 0.95));
            }
        }
        Opt::TextSize(sz) => {
            if !fonts.is_empty() {
                let f = fonts.ui_face();
                fonts.draw_centered(&mut img, f, (*sz / 56.0 * s * 0.8).max(10.0), s / 2.0, s * 0.75, s - 4.0, BLACK, "Aa");
            }
        }
        Opt::TextFont(i) => {
            if !fonts.is_empty() {
                fonts.draw_centered(&mut img, *i, s * 0.55, s / 2.0, s * 0.72, s - 4.0, BLACK, "Ab");
            }
        }
        Opt::MoverMove | Opt::MoverCopy | Opt::MoverObjects => {
            let dashed = |im: &mut Image, x: i32, y: i32, w: i32, h: i32| {
                for i in 0..w {
                    if (i / 3) % 2 == 0 {
                        im.put(x + i, y, BLACK);
                        im.put(x + i, y + h - 1, BLACK);
                    }
                }
                for i in 0..h {
                    if (i / 3) % 2 == 0 {
                        im.put(x, y + i, BLACK);
                        im.put(x + w - 1, y + i, BLACK);
                    }
                }
            };
            dashed(&mut img, 6, 6, 22, 22);
            match opt {
                Opt::MoverCopy => {
                    gfx::fill_rect(&mut img, 8, 8, 18, 18, gfx::rgb(250, 180, 40));
                    gfx::fill_rect(&mut img, 22, 22, 18, 18, gfx::rgb(250, 180, 40));
                }
                Opt::MoverObjects => gfx::fill_circle(&mut img, 30.0, 30.0, 10.0, gfx::rgb(240, 80, 160)),
                _ => gfx::fill_rect(&mut img, 22, 22, 18, 18, gfx::rgb(250, 180, 40)),
            }
        }
        Opt::Eraser(k, _) => match k {
            EraserKind::Square(n) => {
                for y in 0..size as i32 {
                    for x in 0..size as i32 {
                        img.put(x, y, gfx::hsv(200.0 + x as f32, 0.5, 0.9));
                    }
                }
                let n = (*n).min(size as i32 - 8);
                gfx::fill_rect(&mut img, (size as i32 - n) / 2, (size as i32 - n) / 2, n, n, WHITE);
                gfx::stroke_rect(&mut img, (size as i32 - n) / 2, (size as i32 - n) / 2, n, n, 1, BLACK);
            }
            _ => {
                for y in 0..size as i32 {
                    for x in 0..size as i32 {
                        let t = match k {
                            EraserKind::ClearFade => (x + y) as f32 / (2.0 * s),
                            EraserKind::ClearWipe => y as f32 / s,
                            EraserKind::ClearDissolve => rng.f(),
                            _ => (((x as f32 - s / 2.0).powi(2) + (y as f32 - s / 2.0).powi(2)).sqrt() / (s / 2.0)).min(1.0),
                        };
                        img.put(x, y, gfx::lerp(gfx::rgb(240, 80, 60), WHITE, (t * 255.0) as u32));
                    }
                }
            }
        },
    }
    img
}

pub fn fill_style(kind: FillKind, assets: &Assets) -> FillStyle {
    match kind {
        FillKind::Tint(i) => assets
            .archive(TP)
            .and_then(|a| a.get(crate::assets::T_BMP, 20500 + i).and_then(|d| mohawk::decode_indexed(d).ok()))
            .map(FillStyle::Tint)
            .unwrap_or(FillStyle::Solid),
        FillKind::Pattern(i) => assets.image(TP, 20400 + i).map(|im| FillStyle::Tile((*im).clone())).unwrap_or(FillStyle::Solid),
        FillKind::Gradient(i) => crate::fills::gradient(assets, i as usize).map(FillStyle::Gradient).unwrap_or(FillStyle::Solid),
    }
}

/// An alphabet-stamp glyph: the dark strokes of the letter icon, in `color`, enlarged.
pub fn letter_glyph(assets: &Assets, id: u16, color: u32, scale: f32) -> Option<Image> {
    let src = assets.image(TP, id)?;
    let (_, r, g, b) = gfx::split(color);
    let mut glyph = Image::new(src.width, src.height);
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, -1, -1);
    for y in 0..src.height as i32 {
        for x in 0..src.width as i32 {
            let p = src.pixels[(y as u32 * src.width + x as u32) as usize];
            if p >> 24 == 0 {
                continue;
            }
            let l = paint::luma(p) as i32;
            let cov = ((190 - l) * 255 / 150).clamp(0, 255);
            if cov > 0 {
                glyph.put(x, y, gfx::argb(cov as u8, r as u8, g as u8, b as u8));
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x1 < 0 {
        return None;
    }
    let c = gfx::crop(&glyph, x0, y0, x1 - x0 + 1, y1 - y0 + 1);
    Some(gfx::resize(&c, (c.width as f32 * scale) as u32, (c.height as f32 * scale) as u32))
}
