//! Shape styles, shape records and conversion of SWF edge lists into closed paths.
//!
//! SWF shapes are a list of edges, each carrying the fill style on its left
//! (`fillStyle0`) and right (`fillStyle1`) side plus a line style. To fill
//! with a conventional path rasterizer, every fill style gets its own set of
//! directed edges (fill1 edges as drawn, fill0 edges reversed) which are then
//! chained into closed contours. Because each fill region is bounded by a
//! balanced set of directed edges, a greedy walk always closes.

use std::collections::HashMap;

use tiny_skia::{FillRule, LineCap, LineJoin, Path, PathBuilder, Transform};

use crate::reader::{Reader, Result};
use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Spread {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Clone, Debug)]
pub(crate) struct Gradient {
    /// Gradient square (-16384..16384) to shape twips.
    pub matrix: Transform,
    pub spread: Spread,
    /// (ratio 0..255, straight RGBA)
    pub stops: Vec<(u8, [u8; 4])>,
}

#[derive(Clone, Debug)]
pub(crate) enum Fill {
    Solid([u8; 4]),
    Linear(Gradient),
    Radial(Gradient),
    /// Focal radial gradient; focal point in -1..1 along the gradient x axis.
    Focal(Gradient, f32),
    Bitmap {
        id: u16,
        /// Bitmap pixels to shape twips.
        matrix: Transform,
        repeat: bool,
        smooth: bool,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct LineStyle {
    /// Width in twips (0 = hairline).
    pub width: u16,
    pub fill: Fill,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f32,
    pub no_close: bool,
}

/// Paths of one style group (fills drawn first, then strokes).
#[derive(Clone, Debug, Default)]
pub(crate) struct Layer {
    pub fills: Vec<(Fill, Path)>,
    pub strokes: Vec<(LineStyle, Path)>,
}

#[derive(Clone, Debug)]
pub(crate) struct Shape {
    pub layers: Vec<Layer>,
    pub rule: FillRule,
    /// Control-point bounds in twips including half stroke widths (culling only).
    pub bounds: Option<[f32; 4]>,
}

fn union(a: Option<[f32; 4]>, b: [f32; 4]) -> Option<[f32; 4]> {
    Some(match a {
        None => b,
        Some(a) => [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ],
    })
}

fn shape_bounds(layers: &[Layer]) -> Option<[f32; 4]> {
    let mut acc = None;
    for l in layers {
        for (_, p) in &l.fills {
            let b = p.bounds();
            acc = union(acc, [b.left(), b.top(), b.right(), b.bottom()]);
        }
        for (ls, p) in &l.strokes {
            let b = p.bounds();
            let h = f32::from(ls.width) * 0.5 * ls.miter_limit.max(1.0);
            acc = union(
                acc,
                [b.left() - h, b.top() - h, b.right() + h, b.bottom() + h],
            );
        }
    }
    acc
}

/// Feature usage counters (for census/diagnostics).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShapeStats {
    pub solid_fills: usize,
    pub linear_gradients: usize,
    pub radial_gradients: usize,
    pub focal_gradients: usize,
    pub bitmap_fills: usize,
    pub line_styles: usize,
    pub new_style_groups: usize,
    pub curved_edges: usize,
    pub straight_edges: usize,
}

pub(crate) struct Ctx<'s> {
    pub shape_version: u8,
    pub swf_version: u8,
    pub stats: &'s mut ShapeStats,
}

fn read_color(r: &mut Reader, shape_version: u8) -> Result<[u8; 4]> {
    if shape_version >= 3 {
        r.rgba()
    } else {
        r.rgb()
    }
}

fn read_gradient(r: &mut Reader, ctx: &Ctx) -> Result<Gradient> {
    let matrix = r.matrix()?;
    let flags = r.u8()?;
    let spread = match (flags >> 6) & 3 {
        1 => Spread::Reflect,
        2 => Spread::Repeat,
        _ => Spread::Pad,
    };
    let n = flags & 0x0f;
    let mut stops = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let ratio = r.u8()?;
        let color = read_color(r, ctx.shape_version)?;
        stops.push((ratio, color));
    }
    Ok(Gradient {
        matrix,
        spread,
        stops,
    })
}

fn gradient_or_black(g: Gradient, f: impl FnOnce(Gradient) -> Fill) -> Fill {
    if g.stops.is_empty() {
        Fill::Solid([0, 0, 0, 255])
    } else {
        f(g)
    }
}

fn read_fill(r: &mut Reader, ctx: &mut Ctx) -> Result<Fill> {
    let kind = r.u8()?;
    Ok(match kind {
        0x00 => {
            ctx.stats.solid_fills += 1;
            Fill::Solid(read_color(r, ctx.shape_version)?)
        }
        0x10 => {
            ctx.stats.linear_gradients += 1;
            gradient_or_black(read_gradient(r, ctx)?, Fill::Linear)
        }
        0x12 => {
            ctx.stats.radial_gradients += 1;
            gradient_or_black(read_gradient(r, ctx)?, Fill::Radial)
        }
        0x13 => {
            ctx.stats.focal_gradients += 1;
            let g = read_gradient(r, ctx)?;
            let focal = (r.i16()? as f32 / 256.0).clamp(-0.98, 0.98);
            gradient_or_black(g, |g| Fill::Focal(g, focal))
        }
        0x40..=0x43 => {
            ctx.stats.bitmap_fills += 1;
            let id = r.u16()?;
            let matrix = r.matrix()?;
            Fill::Bitmap {
                id,
                matrix,
                repeat: kind & 1 == 0,
                // Smoothing flag only exists in SWF 8+; older players never smooth fills.
                smooth: ctx.swf_version >= 8 && kind & 2 == 0,
            }
        }
        _ => return Err(Error::Malformed(format!("fill style type {kind:#x}"))),
    })
}

fn cap(v: u16) -> LineCap {
    match v {
        1 => LineCap::Butt,
        2 => LineCap::Square,
        _ => LineCap::Round,
    }
}

fn read_line(r: &mut Reader, ctx: &mut Ctx) -> Result<LineStyle> {
    ctx.stats.line_styles += 1;
    let width = r.u16()?;
    if ctx.shape_version < 4 {
        let color = read_color(r, ctx.shape_version)?;
        return Ok(LineStyle {
            width,
            fill: Fill::Solid(color),
            cap: LineCap::Round,
            join: LineJoin::Round,
            miter_limit: 3.0,
            no_close: false,
        });
    }
    let flags = r.u16()?;
    let start_cap = (flags >> 14) & 3;
    let join_v = (flags >> 12) & 3;
    let has_fill = flags & (1 << 11) != 0;
    let no_close = flags & (1 << 2) != 0;
    let join = match join_v {
        1 => LineJoin::Bevel,
        2 => LineJoin::Miter,
        _ => LineJoin::Round,
    };
    let miter_limit = if join_v == 2 {
        (r.u16()? as f32 / 256.0).max(1.0)
    } else {
        3.0
    };
    let fill = if has_fill {
        read_fill(r, ctx)?
    } else {
        Fill::Solid(r.rgba()?)
    };
    Ok(LineStyle {
        width,
        fill,
        cap: cap(start_cap),
        join,
        miter_limit,
        no_close,
    })
}

pub(crate) struct Styles {
    pub fills: Vec<Fill>,
    pub lines: Vec<LineStyle>,
    pub fill_bits: u32,
    pub line_bits: u32,
}

pub(crate) fn read_styles(r: &mut Reader, ctx: &mut Ctx) -> Result<Styles> {
    let mut n = r.u8()? as usize;
    if n == 0xff && ctx.shape_version >= 2 {
        n = r.u16()? as usize;
    }
    let mut fills = Vec::with_capacity(n);
    for _ in 0..n {
        fills.push(read_fill(r, ctx)?);
    }
    let mut n = r.u8()? as usize;
    if n == 0xff && ctx.shape_version >= 2 {
        n = r.u16()? as usize;
    }
    let mut lines = Vec::with_capacity(n);
    for _ in 0..n {
        lines.push(read_line(r, ctx)?);
    }
    let bits = r.u8()?;
    Ok(Styles {
        fills,
        lines,
        fill_bits: u32::from(bits >> 4),
        line_bits: u32::from(bits & 0x0f),
    })
}

type Pt = (i32, i32);

#[derive(Clone, Copy, Debug)]
struct Edge {
    from: Pt,
    ctrl: Option<Pt>,
    to: Pt,
}

impl Edge {
    fn reversed(self) -> Edge {
        Edge {
            from: self.to,
            ctrl: self.ctrl,
            to: self.from,
        }
    }
    fn is_degenerate(&self) -> bool {
        self.from == self.to && self.ctrl.map_or(true, |c| c == self.from)
    }
}

fn emit(pb: &mut PathBuilder, e: &Edge) {
    match e.ctrl {
        Some(c) => pb.quad_to(c.0 as f32, c.1 as f32, e.to.0 as f32, e.to.1 as f32),
        None => pb.line_to(e.to.0 as f32, e.to.1 as f32),
    }
}

/// Max distance (twips) between a flattened curve and the true curve.
const FLATTEN_TOLERANCE: f32 = 0.5;

/// Emit an edge for filling, with curves flattened to lines.
///
/// Two fills sharing a curved edge traverse it in opposite directions. tiny-skia
/// subdivides quads direction-dependently, which can leave single-sample pinholes
/// between the fills; flattening from a canonical orientation makes both sides use
/// bit-identical vertices.
fn emit_fill(pb: &mut PathBuilder, e: &Edge) {
    let Some(c) = e.ctrl else {
        pb.line_to(e.to.0 as f32, e.to.1 as f32);
        return;
    };
    let flip = e.to < e.from;
    let (a, b) = if flip { (e.to, e.from) } else { (e.from, e.to) };
    let dx = (a.0 - 2 * c.0 + b.0) as f32;
    let dy = (a.1 - 2 * c.1 + b.1) as f32;
    let dev = (dx * dx + dy * dy).sqrt() * 0.25;
    let n = ((dev / FLATTEN_TOLERANCE).sqrt().ceil() as usize).clamp(1, 64);
    let point = |i: usize| -> (f32, f32) {
        let t = i as f32 / n as f32;
        let u = 1.0 - t;
        let (w0, w1, w2) = (u * u, 2.0 * u * t, t * t);
        (
            w0 * a.0 as f32 + w1 * c.0 as f32 + w2 * b.0 as f32,
            w0 * a.1 as f32 + w1 * c.1 as f32 + w2 * b.1 as f32,
        )
    };
    for k in 1..n {
        let (x, y) = point(if flip { n - k } else { k });
        pb.line_to(x, y);
    }
    pb.line_to(e.to.0 as f32, e.to.1 as f32);
}

/// Chain directed fill edges into contours.
fn fill_path(edges: &[Edge]) -> Option<Path> {
    let mut starts: HashMap<Pt, (usize, Vec<usize>)> = HashMap::new();
    for (i, e) in edges.iter().enumerate() {
        starts.entry(e.from).or_default().1.push(i);
    }
    let mut used = vec![false; edges.len()];
    let mut pb = PathBuilder::new();
    for i in 0..edges.len() {
        if used[i] {
            continue;
        }
        let start = edges[i].from;
        pb.move_to(start.0 as f32, start.1 as f32);
        let mut cur = i;
        loop {
            used[cur] = true;
            emit_fill(&mut pb, &edges[cur]);
            let p = edges[cur].to;
            if p == start {
                pb.close();
                break;
            }
            let next = starts.get_mut(&p).and_then(|(cursor, list)| {
                while *cursor < list.len() && used[list[*cursor]] {
                    *cursor += 1;
                }
                list.get(*cursor).copied()
            });
            match next {
                Some(j) => cur = j,
                None => break,
            }
        }
    }
    pb.finish()
}

/// Chain stroke edges in drawing order.
fn stroke_path(edges: &[Edge], no_close: bool) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let mut last: Option<Pt> = None;
    let mut sub_start: Pt = (0, 0);
    let mut sub_len = 0usize;
    let finish_sub = |pb: &mut PathBuilder, last: Option<Pt>, start: Pt, len: usize| {
        if last == Some(start) && len > 1 && !no_close {
            pb.close();
        }
    };
    for e in edges {
        if last != Some(e.from) {
            finish_sub(&mut pb, last, sub_start, sub_len);
            pb.move_to(e.from.0 as f32, e.from.1 as f32);
            sub_start = e.from;
            sub_len = 0;
        }
        emit(&mut pb, e);
        sub_len += 1;
        last = Some(e.to);
    }
    finish_sub(&mut pb, last, sub_start, sub_len);
    pb.finish()
}

struct Converter {
    fills: Vec<Fill>,
    lines: Vec<LineStyle>,
    fill_edges: Vec<Vec<Edge>>,
    line_edges: Vec<Vec<Edge>>,
    layers: Vec<Layer>,
}

impl Converter {
    fn new(styles: &Styles) -> Self {
        let mut c = Converter {
            fills: Vec::new(),
            lines: Vec::new(),
            fill_edges: Vec::new(),
            line_edges: Vec::new(),
            layers: Vec::new(),
        };
        c.set_styles(styles.fills.clone(), styles.lines.clone());
        c
    }

    fn set_styles(&mut self, fills: Vec<Fill>, lines: Vec<LineStyle>) {
        self.fill_edges = vec![Vec::new(); fills.len()];
        self.line_edges = vec![Vec::new(); lines.len()];
        self.fills = fills;
        self.lines = lines;
    }

    fn flush(&mut self) {
        let mut layer = Layer::default();
        for (i, edges) in self.fill_edges.iter_mut().enumerate() {
            if edges.is_empty() {
                continue;
            }
            if let Some(p) = fill_path(edges) {
                layer.fills.push((self.fills[i].clone(), p));
            }
            edges.clear();
        }
        for (i, edges) in self.line_edges.iter_mut().enumerate() {
            if edges.is_empty() {
                continue;
            }
            let ls = &self.lines[i];
            if let Some(p) = stroke_path(edges, ls.no_close) {
                layer.strokes.push((ls.clone(), p));
            }
            edges.clear();
        }
        if !layer.fills.is_empty() || !layer.strokes.is_empty() {
            self.layers.push(layer);
        }
    }

    fn edge(&mut self, e: Edge, fill0: usize, fill1: usize, line: usize) {
        if fill0 != fill1 && !e.is_degenerate() {
            if fill0 > 0 && fill0 <= self.fill_edges.len() {
                self.fill_edges[fill0 - 1].push(e.reversed());
            }
            if fill1 > 0 && fill1 <= self.fill_edges.len() {
                self.fill_edges[fill1 - 1].push(e);
            }
        }
        if line > 0 && line <= self.line_edges.len() {
            self.line_edges[line - 1].push(e);
        }
    }
}

/// Read SHAPERECORDs up to the end record and convert them to layered paths.
pub(crate) fn read_records(r: &mut Reader, ctx: &mut Ctx, styles: Styles) -> Result<Vec<Layer>> {
    let mut conv = Converter::new(&styles);
    let (mut fill_bits, mut line_bits) = (styles.fill_bits, styles.line_bits);
    let (mut x, mut y) = (0i32, 0i32);
    let (mut fill0, mut fill1, mut line) = (0usize, 0usize, 0usize);
    loop {
        if r.bit()? {
            let straight = r.bit()?;
            let nbits = r.ub(4)? + 2;
            let from = (x, y);
            let e = if straight {
                ctx.stats.straight_edges += 1;
                let general = r.bit()?;
                let (dx, dy) = if general {
                    (r.sb(nbits)?, r.sb(nbits)?)
                } else if r.bit()? {
                    (0, r.sb(nbits)?)
                } else {
                    (r.sb(nbits)?, 0)
                };
                x = x.wrapping_add(dx);
                y = y.wrapping_add(dy);
                Edge {
                    from,
                    ctrl: None,
                    to: (x, y),
                }
            } else {
                ctx.stats.curved_edges += 1;
                let cx = x.wrapping_add(r.sb(nbits)?);
                let cy = y.wrapping_add(r.sb(nbits)?);
                x = cx.wrapping_add(r.sb(nbits)?);
                y = cy.wrapping_add(r.sb(nbits)?);
                Edge {
                    from,
                    ctrl: Some((cx, cy)),
                    to: (x, y),
                }
            };
            conv.edge(e, fill0, fill1, line);
        } else {
            let flags = r.ub(5)?;
            if flags == 0 {
                break;
            }
            let mv = if flags & 1 != 0 {
                let n = r.ub(5)?;
                Some((r.sb(n)?, r.sb(n)?))
            } else {
                None
            };
            let f0 = if flags & 2 != 0 {
                Some(r.ub(fill_bits)? as usize)
            } else {
                None
            };
            let f1 = if flags & 4 != 0 {
                Some(r.ub(fill_bits)? as usize)
            } else {
                None
            };
            let ls = if flags & 8 != 0 {
                Some(r.ub(line_bits)? as usize)
            } else {
                None
            };
            if flags & 16 != 0 {
                // New style arrays: everything drawn so far becomes a finished layer.
                // Seen in the wild even in DefineShape1, so accept it for any version.
                ctx.stats.new_style_groups += 1;
                let st = read_styles(r, ctx)?;
                conv.flush();
                conv.set_styles(st.fills, st.lines);
                fill_bits = st.fill_bits;
                line_bits = st.line_bits;
                fill0 = 0;
                fill1 = 0;
                line = 0;
            }
            if let Some((mx, my)) = mv {
                x = mx;
                y = my;
            }
            if let Some(v) = f0 {
                fill0 = v;
            }
            if let Some(v) = f1 {
                fill1 = v;
            }
            if let Some(v) = ls {
                line = v;
            }
        }
    }
    conv.flush();
    Ok(conv.layers)
}

/// DefineShape / 2 / 3 / 4 body (after the tag header).
pub(crate) fn read_define_shape(
    r: &mut Reader,
    shape_version: u8,
    swf_version: u8,
    stats: &mut ShapeStats,
) -> Result<(u16, Shape)> {
    let id = r.u16()?;
    let _bounds = r.rect()?;
    let mut rule = FillRule::EvenOdd;
    if shape_version >= 4 {
        let _edge_bounds = r.rect()?;
        let flags = r.u8()?;
        if flags & 0b100 != 0 {
            rule = FillRule::Winding;
        }
    }
    let mut ctx = Ctx {
        shape_version,
        swf_version,
        stats,
    };
    let styles = read_styles(r, &mut ctx)?;
    let layers = read_records(r, &mut ctx, styles)?;
    let bounds = shape_bounds(&layers);
    Ok((
        id,
        Shape {
            layers,
            rule,
            bounds,
        },
    ))
}

/// A font glyph SHAPE (no style arrays; fill style 1 is the text color).
pub(crate) fn read_glyph(data: &[u8], swf_version: u8) -> Result<Option<Path>> {
    let mut r = Reader::new(data);
    let bits = r.u8()?;
    let styles = Styles {
        fills: vec![Fill::Solid([0, 0, 0, 255])],
        lines: Vec::new(),
        fill_bits: u32::from(bits >> 4),
        line_bits: u32::from(bits & 0x0f),
    };
    let mut stats = ShapeStats::default();
    let mut ctx = Ctx {
        shape_version: 1,
        swf_version,
        stats: &mut stats,
    };
    let layers = read_records(&mut r, &mut ctx, styles)?;
    let mut pb = PathBuilder::new();
    for l in &layers {
        for (_, p) in &l.fills {
            pb.push_path(p);
        }
    }
    Ok(pb.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<Edge> {
        let p = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        (0..4)
            .map(|i| Edge {
                from: p[i],
                ctrl: None,
                to: p[(i + 1) % 4],
            })
            .collect()
    }

    #[test]
    fn shuffled_edges_close() {
        let mut e = sq(0, 0, 10, 10);
        e.extend(sq(20, 20, 30, 30));
        e.swap(0, 5);
        e.swap(2, 7);
        let p = fill_path(&e).unwrap();
        let closes = p
            .segments()
            .filter(|s| matches!(s, tiny_skia::PathSegment::Close))
            .count();
        assert_eq!(closes, 2);
    }

    #[test]
    fn shared_edge_cancels() {
        let mut c = Converter::new(&Styles {
            fills: vec![Fill::Solid([255, 0, 0, 255]), Fill::Solid([0, 0, 255, 255])],
            lines: vec![],
            fill_bits: 2,
            line_bits: 0,
        });
        // Same style on both sides -> ignored for filling.
        c.edge(
            Edge {
                from: (0, 0),
                ctrl: None,
                to: (5, 5),
            },
            1,
            1,
            0,
        );
        assert!(c.fill_edges[0].is_empty());
        c.edge(
            Edge {
                from: (0, 0),
                ctrl: None,
                to: (5, 5),
            },
            1,
            2,
            0,
        );
        assert_eq!(c.fill_edges[0][0].from, (5, 5));
        assert_eq!(c.fill_edges[1][0].from, (0, 0));
    }
}
