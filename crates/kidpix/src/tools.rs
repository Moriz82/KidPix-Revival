//! The eight easel tools, their option lists and stroke logic.

use crate::assets::Assets;
use crate::gfx::Image;
use crate::paint::{FillStyle, MediaParams, Pad};
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
    /// Line / rectangle / oval rubber band (base = picture before the shape).
    ShapeDrag { shape: crate::trays::Shape, start: (f32, f32), cur: (f32, f32), filled: bool, pen: Pen, base: Image },
    /// Curve: after the first drag the line follows the mouse to bend; a click finishes.
    CurveBend { a: (f32, f32), b: (f32, f32), pen: Pen, base: Image },
    /// Polygon: click for each corner, click the first one to close.
    Polygon { pts: Vec<(f32, f32)>, filled: bool, pen: Pen, base: Image },
    /// Blend fill: holding the button moves the gradient's focus.
    Blend { region: Vec<(i32, i32)>, grad: crate::fills::Grad },
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

