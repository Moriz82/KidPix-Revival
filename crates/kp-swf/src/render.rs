//! Rasterization (supersampled tiny-skia) and geometric content bounds.

use std::collections::HashMap;

use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, GradientStop, IntSize, LinearGradient, Mask, Paint,
    Path, Pattern, Pixmap, Point, RadialGradient, Rect, Shader, SpreadMode, Stroke, Transform,
};

use crate::shape::{Fill, Gradient, LineStyle, Shape, Spread};
use crate::{Character, ColorTransform, Movie, Placed, Rgba, Text};

/// Supersampling factor per axis.
pub(crate) const SS: u32 = 4;
/// Output pixels per tile edge (bounds the supersampled buffer to (TILE*SS)^2 pixels).
const TILE: u32 = 256;
/// Flash gradient square half-size in twips.
const GRAD: f32 = 16384.0;

fn color(c: [u8; 4]) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], c[3])
}

fn spread(s: Spread) -> SpreadMode {
    match s {
        Spread::Pad => SpreadMode::Pad,
        Spread::Reflect => SpreadMode::Reflect,
        Spread::Repeat => SpreadMode::Repeat,
    }
}

fn det_scale(ts: &Transform) -> f32 {
    (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt()
}

/// Copy of a premultiplied pixmap with a color transform applied.
fn cx_pixmap(src: &Pixmap, cx: &ColorTransform) -> Pixmap {
    let mut out = src.clone();
    for p in out.data_mut().chunks_exact_mut(4) {
        let a = p[3];
        let straight = if a == 0 {
            [0, 0, 0, 0]
        } else {
            let un =
                |c: u8| ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8;
            [un(p[0]), un(p[1]), un(p[2]), a]
        };
        let c = cx.apply(straight);
        let pm = |v: u8| ((u16::from(v) * u16::from(c[3]) + 127) / 255) as u8;
        p.copy_from_slice(&[pm(c[0]), pm(c[1]), pm(c[2]), c[3]]);
    }
    out
}

fn gradient_shader(
    g: &Gradient,
    cx: &ColorTransform,
    make: impl FnOnce(Vec<GradientStop>, SpreadMode) -> Option<Shader<'static>>,
) -> Option<Shader<'static>> {
    let mut last = 0.0f32;
    let mut all_clear = true;
    let stops: Vec<GradientStop> = g
        .stops
        .iter()
        .map(|&(r, c)| {
            let pos = (f32::from(r) / 255.0).max(last);
            last = pos;
            let c = cx.apply(c);
            all_clear &= c[3] == 0;
            GradientStop::new(pos, color(c))
        })
        .collect();
    if all_clear {
        return None;
    }
    let fallback = g.stops.last().map(|&(_, c)| color(cx.apply(c)));
    make(stops, spread(g.spread)).or_else(|| fallback.map(Shader::SolidColor))
}

struct Painter<'m> {
    movie: &'m Movie,
    frame: usize,
    /// Device pixels per output pixel.
    ss: f32,
}

impl<'m> Painter<'m> {
    /// Build a paint for `fill` and hand it to `f`; nothing happens for invisible fills.
    fn with_paint(&self, fill: &Fill, cx: &ColorTransform, f: impl FnOnce(&Paint)) {
        let mut holder: Option<Pixmap> = None;
        let shader = match fill {
            Fill::Solid(c) => {
                let c = cx.apply(*c);
                if c[3] == 0 {
                    return;
                }
                Shader::SolidColor(color(c))
            }
            Fill::Linear(g) => match gradient_shader(g, cx, |s, m| {
                LinearGradient::new(
                    Point::from_xy(-GRAD, 0.0),
                    Point::from_xy(GRAD, 0.0),
                    s,
                    m,
                    g.matrix,
                )
            }) {
                Some(s) => s,
                None => return,
            },
            Fill::Radial(g) => match gradient_shader(g, cx, |s, m| {
                RadialGradient::new(Point::zero(), Point::zero(), GRAD, s, m, g.matrix)
            }) {
                Some(s) => s,
                None => return,
            },
            Fill::Focal(g, focal) => match gradient_shader(g, cx, |s, m| {
                RadialGradient::new(
                    Point::from_xy(focal * GRAD, 0.0),
                    Point::zero(),
                    GRAD,
                    s,
                    m,
                    g.matrix,
                )
            }) {
                Some(s) => s,
                None => return,
            },
            Fill::Bitmap {
                id,
                matrix,
                repeat,
                smooth,
            } => {
                let Some(Character::Bitmap(src)) = self.movie.dict.get(id) else {
                    return;
                };
                let pix: &Pixmap = if cx.is_identity() {
                    src
                } else {
                    holder.insert(cx_pixmap(src, cx))
                };
                Pattern::new(
                    pix.as_ref(),
                    if *repeat {
                        SpreadMode::Repeat
                    } else {
                        SpreadMode::Pad
                    },
                    // Flash's default HIGH quality also smooths bitmaps of static movies.
                    if *smooth || (self.movie.version < 8 && self.movie.frames.len() == 1) {
                        FilterQuality::Bilinear
                    } else {
                        FilterQuality::Nearest
                    },
                    1.0,
                    *matrix,
                )
            }
        };
        let paint = Paint {
            shader,
            blend_mode: BlendMode::SourceOver,
            anti_alias: false,
            force_hq_pipeline: false,
        };
        f(&paint);
    }

    /// Is the twips-space box `b` (under `ts`) inside the pixmap, with `margin` device px?
    fn visible(&self, pm: &Pixmap, b: [f32; 4], ts: Transform, margin: f32) -> bool {
        let Some(r) = map_box(b, ts) else {
            return false;
        };
        r[2] >= -margin
            && r[3] >= -margin
            && r[0] <= pm.width() as f32 + margin
            && r[1] <= pm.height() as f32 + margin
    }

    fn draw_list(
        &self,
        pm: &mut Pixmap,
        list: &[Placed],
        ts: Transform,
        cx: ColorTransform,
        mask: Option<&Mask>,
    ) {
        let mut clips: Vec<(u16, Mask)> = Vec::new();
        for p in list {
            while clips.last().is_some_and(|c| p.depth > c.0) {
                clips.pop();
            }
            let ts2 = ts.pre_concat(p.matrix);
            if p.clip_depth > 0 {
                let m = {
                    let parent = clips.last().map(|c| &c.1).or(mask);
                    self.make_mask(pm, p.id, ts2, parent)
                };
                clips.push((p.clip_depth, m));
                continue;
            }
            let m = clips.last().map(|c| &c.1).or(mask);
            self.draw_char(pm, p.id, ts2, cx.then(&p.cxform), m);
        }
    }

    fn make_mask(&self, pm: &Pixmap, id: u16, ts: Transform, parent: Option<&Mask>) -> Mask {
        let (w, h) = (pm.width(), pm.height());
        let mut tmp = Pixmap::new(w, h).expect("tile size is non-zero");
        self.draw_char(&mut tmp, id, ts, ColorTransform::IDENTITY, None);
        let mut data: Vec<u8> = tmp
            .data()
            .chunks_exact(4)
            .map(|p| if p[3] > 0 { 255 } else { 0 })
            .collect();
        if let Some(parent) = parent {
            for (d, &m) in data.iter_mut().zip(parent.data()) {
                *d = ((u16::from(*d) * u16::from(m) + 127) / 255) as u8;
            }
        }
        Mask::from_vec(data, IntSize::from_wh(w, h).expect("non-zero")).expect("size matches")
    }

    fn draw_char(
        &self,
        pm: &mut Pixmap,
        id: u16,
        ts: Transform,
        cx: ColorTransform,
        mask: Option<&Mask>,
    ) {
        match self.movie.dict.get(&id) {
            Some(Character::Shape(s)) => self.draw_shape(pm, s, ts, &cx, mask),
            Some(Character::Text(t)) => self.draw_text(pm, t, ts, &cx, mask),
            Some(Character::Sprite(sp)) if !sp.frames.is_empty() => {
                let f = &sp.frames[self.frame % sp.frames.len()];
                self.draw_list(pm, f, ts, cx, mask);
            }
            Some(Character::Bitmap(b)) => {
                let (w, h) = (b.width() as f32 * 20.0, b.height() as f32 * 20.0);
                let Some(rect) = Rect::from_xywh(0.0, 0.0, w, h) else {
                    return;
                };
                let fill = Fill::Bitmap {
                    id,
                    matrix: Transform::from_scale(20.0, 20.0),
                    repeat: false,
                    smooth: self.movie.version >= 8,
                };
                self.with_paint(&fill, &cx, |p| pm.fill_rect(rect, p, ts, mask));
            }
            _ => {}
        }
    }

    fn draw_shape(
        &self,
        pm: &mut Pixmap,
        s: &Shape,
        ts: Transform,
        cx: &ColorTransform,
        mask: Option<&Mask>,
    ) {
        match s.bounds {
            Some(b) if self.visible(pm, b, ts, self.ss * 2.0) => {}
            _ => return,
        }
        for layer in &s.layers {
            for (fill, path) in &layer.fills {
                self.with_paint(fill, cx, |p| pm.fill_path(path, p, s.rule, ts, mask));
            }
            for (ls, path) in &layer.strokes {
                self.stroke(pm, path, ls, ts, cx, mask);
            }
        }
    }

    fn stroke(
        &self,
        pm: &mut Pixmap,
        path: &Path,
        ls: &LineStyle,
        ts: Transform,
        cx: &ColorTransform,
        mask: Option<&Mask>,
    ) {
        let s = det_scale(&ts);
        if !(s > 0.0 && s.is_finite()) {
            return;
        }
        // Flash never draws a line thinner than one output pixel (width 0 = hairline).
        let width = f32::from(ls.width).max(self.ss / s);
        let stroke = Stroke {
            width,
            miter_limit: ls.miter_limit,
            line_cap: ls.cap,
            line_join: ls.join,
            dash: None,
        };
        self.with_paint(&ls.fill, cx, |p| pm.stroke_path(path, p, &stroke, ts, mask));
    }

    fn draw_text(
        &self,
        pm: &mut Pixmap,
        t: &Text,
        ts: Transform,
        cx: &ColorTransform,
        mask: Option<&Mask>,
    ) {
        let base = ts.pre_concat(t.matrix);
        for g in &t.glyphs {
            let Some(Character::Font(font)) = self.movie.dict.get(&g.font) else {
                continue;
            };
            let Some(Some(path)) = font.glyphs.get(usize::from(g.index)) else {
                continue;
            };
            let k = g.size / font.em;
            let gts = base.pre_translate(g.x, g.y).pre_scale(k, k);
            self.with_paint(&Fill::Solid(g.color), cx, |p| {
                pm.fill_path(path, p, FillRule::EvenOdd, gts, mask)
            });
        }
    }
}

/// Box filter `SS x SS` blocks of the premultiplied tile into straight RGBA output.
fn downsample(pm: &Pixmap, out: &mut [u8], out_w: u32, tx: u32, ty: u32, tw: u32, th: u32) {
    let data = pm.data();
    let pw = pm.width() as usize;
    let ss = SS as usize;
    for oy in 0..th as usize {
        let orow = ((ty as usize + oy) * out_w as usize + tx as usize) * 4;
        for ox in 0..tw as usize {
            let mut acc = [0u32; 4];
            for sy in 0..ss {
                let row = ((oy * ss + sy) * pw + ox * ss) * 4;
                for px in data[row..row + ss * 4].chunks_exact(4) {
                    acc[0] += u32::from(px[0]);
                    acc[1] += u32::from(px[1]);
                    acc[2] += u32::from(px[2]);
                    acc[3] += u32::from(px[3]);
                }
            }
            if acc[3] == 0 {
                continue;
            }
            let n = (ss * ss) as u32;
            let a = (acc[3] + n / 2) / n;
            let o = orow + ox * 4;
            for c in 0..3 {
                out[o + c] = ((acc[c] * 255 + acc[3] / 2) / acc[3]).min(255) as u8;
            }
            out[o + 3] = a as u8;
        }
    }
}

fn out_dim(a: f32, b: f32, scale: f32) -> u32 {
    let v = (b - a) * scale;
    if !v.is_finite() || v <= 0.0 {
        0
    } else {
        (v - 1e-3).ceil().clamp(0.0, 32767.0) as u32
    }
}

pub(crate) fn render(movie: &Movie, frame: usize, scale: f32, rect: (f32, f32, f32, f32)) -> Rgba {
    let (x0, y0, x1, y1) = rect;
    let (w, h) = if scale.is_finite() && scale > 0.0 {
        (out_dim(x0, x1, scale), out_dim(y0, y1, scale))
    } else {
        (0, 0)
    };
    let mut pixels = vec![0u8; w as usize * h as usize * 4];
    if w == 0 || h == 0 || movie.frames.is_empty() {
        return Rgba {
            width: w,
            height: h,
            pixels,
        };
    }
    let frame = frame % movie.frames.len();
    let painter = Painter {
        movie,
        frame,
        ss: SS as f32,
    };
    let list = &movie.frames[frame];
    let k = scale * SS as f32 / 20.0;
    let mut pm = Pixmap::new(w.min(TILE) * SS, h.min(TILE) * SS).expect("non-zero tile");
    for ty in (0..h).step_by(TILE as usize) {
        for tx in (0..w).step_by(TILE as usize) {
            let (tw, th) = ((w - tx).min(TILE), (h - ty).min(TILE));
            pm.fill(Color::TRANSPARENT);
            let base = Transform::from_row(
                k,
                0.0,
                0.0,
                k,
                -(x0 * scale + tx as f32) * SS as f32,
                -(y0 * scale + ty as f32) * SS as f32,
            );
            painter.draw_list(&mut pm, list, base, ColorTransform::IDENTITY, None);
            downsample(&pm, &mut pixels, w, tx, ty, tw, th);
        }
    }
    Rgba {
        width: w,
        height: h,
        pixels,
    }
}

// ---------------------------------------------------------------------------
// Geometric bounds

fn map_box(b: [f32; 4], ts: Transform) -> Option<[f32; 4]> {
    let mut pts = [
        Point::from_xy(b[0], b[1]),
        Point::from_xy(b[2], b[1]),
        Point::from_xy(b[0], b[3]),
        Point::from_xy(b[2], b[3]),
    ];
    ts.map_points(&mut pts);
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in pts {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        r = [r[0].min(p.x), r[1].min(p.y), r[2].max(p.x), r[3].max(p.y)];
    }
    Some(r)
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

#[derive(Clone, Copy)]
enum Clip {
    All,
    Rect([f32; 4]),
    Nothing,
}

impl Clip {
    fn and(self, b: Option<[f32; 4]>) -> Clip {
        let Some(b) = b else { return Clip::Nothing };
        match self {
            Clip::All => Clip::Rect(b),
            Clip::Nothing => Clip::Nothing,
            Clip::Rect(a) => {
                let r = [
                    a[0].max(b[0]),
                    a[1].max(b[1]),
                    a[2].min(b[2]),
                    a[3].min(b[3]),
                ];
                if r[0] < r[2] && r[1] < r[3] {
                    Clip::Rect(r)
                } else {
                    Clip::Nothing
                }
            }
        }
    }
}

fn path_box(path: &Path, ts: Transform, outset: f32) -> Option<[f32; 4]> {
    let b = path.clone().transform(ts)?.compute_tight_bounds()?;
    if outset <= 0.0 && (b.width() <= 0.0 || b.height() <= 0.0) {
        return None;
    }
    Some([
        b.left() - outset,
        b.top() - outset,
        b.right() + outset,
        b.bottom() + outset,
    ])
}

fn fill_visible(fill: &Fill, cx: &ColorTransform) -> bool {
    match fill {
        Fill::Solid(c) => cx.apply(*c)[3] != 0,
        Fill::Linear(g) | Fill::Radial(g) | Fill::Focal(g, _) => {
            g.stops.iter().any(|&(_, c)| cx.apply(c)[3] != 0)
        }
        Fill::Bitmap { .. } => bitmap_visible(cx),
    }
}

fn bitmap_visible(cx: &ColorTransform) -> bool {
    cx.mul[3] > 0.0 || cx.add[3] > 0.0
}

type CacheKey = (u16, [u32; 14]);

struct Bounder<'m> {
    movie: &'m Movie,
    frame: usize,
    cache: HashMap<CacheKey, Option<[f32; 4]>>,
}

impl Bounder<'_> {
    fn list(
        &mut self,
        list: &[Placed],
        ts: Transform,
        cx: ColorTransform,
        clip: Clip,
        acc: &mut Option<[f32; 4]>,
    ) {
        let mut clips: Vec<(u16, Clip)> = Vec::new();
        for p in list {
            while clips.last().is_some_and(|c| p.depth > c.0) {
                clips.pop();
            }
            let eff = clips.last().map_or(clip, |c| c.1);
            let ts2 = ts.pre_concat(p.matrix);
            if p.clip_depth > 0 {
                let mut tmp = None;
                self.chr(p.id, ts2, ColorTransform::IDENTITY, Clip::All, &mut tmp);
                clips.push((p.clip_depth, eff.and(tmp)));
                continue;
            }
            self.chr(p.id, ts2, cx.then(&p.cxform), eff, acc);
        }
    }

    fn chr(
        &mut self,
        id: u16,
        ts: Transform,
        cx: ColorTransform,
        clip: Clip,
        acc: &mut Option<[f32; 4]>,
    ) {
        if matches!(clip, Clip::Nothing) {
            return;
        }
        let b = match self.movie.dict.get(&id) {
            Some(Character::Sprite(sp)) => {
                if !sp.frames.is_empty() {
                    let f = &sp.frames[self.frame % sp.frames.len()];
                    self.list(f, ts, cx, clip, acc);
                }
                return;
            }
            Some(c) => {
                let mut key = [0u32; 14];
                let m = [ts.sx, ts.ky, ts.kx, ts.sy, ts.tx, ts.ty];
                for (k, v) in key.iter_mut().zip(m.iter().chain(&cx.mul).chain(&cx.add)) {
                    *k = v.to_bits();
                }
                let movie = self.movie;
                *self
                    .cache
                    .entry((id, key))
                    .or_insert_with(|| leaf_bounds(movie, c, ts, &cx))
            }
            None => None,
        };
        let Some(b) = b else { return };
        let b = match clip {
            Clip::All => Some(b),
            Clip::Rect(c) => {
                let r = [
                    b[0].max(c[0]),
                    b[1].max(c[1]),
                    b[2].min(c[2]),
                    b[3].min(c[3]),
                ];
                (r[0] < r[2] && r[1] < r[3]).then_some(r)
            }
            Clip::Nothing => None,
        };
        if let Some(b) = b {
            *acc = union(*acc, b);
        }
    }
}

/// Bounds (in the space `ts` maps to) of a non-sprite character.
fn leaf_bounds(
    movie: &Movie,
    c: &Character,
    ts: Transform,
    cx: &ColorTransform,
) -> Option<[f32; 4]> {
    let mut acc = None;
    match c {
        Character::Shape(s) => {
            let scale = det_scale(&ts);
            for l in &s.layers {
                for (fill, path) in &l.fills {
                    if fill_visible(fill, cx) {
                        if let Some(b) = path_box(path, ts, 0.0) {
                            acc = union(acc, b);
                        }
                    }
                }
                for (ls, path) in &l.strokes {
                    if fill_visible(&ls.fill, cx) {
                        // At scale 1 lines are at least one pixel wide.
                        let mut half = (f32::from(ls.width) * scale).max(1.0) * 0.5;
                        if ls.join == tiny_skia::LineJoin::Miter {
                            half *= ls.miter_limit.max(1.0);
                        }
                        if let Some(b) = path_box(path, ts, half) {
                            acc = union(acc, b);
                        }
                    }
                }
            }
        }
        Character::Text(t) => {
            let base = ts.pre_concat(t.matrix);
            for g in &t.glyphs {
                if cx.apply(g.color)[3] == 0 {
                    continue;
                }
                let Some(Character::Font(font)) = movie.dict.get(&g.font) else {
                    continue;
                };
                let Some(Some(path)) = font.glyphs.get(usize::from(g.index)) else {
                    continue;
                };
                let k = g.size / font.em;
                if let Some(b) = path_box(path, base.pre_translate(g.x, g.y).pre_scale(k, k), 0.0) {
                    acc = union(acc, b);
                }
            }
        }
        Character::Bitmap(b) if bitmap_visible(cx) => {
            acc = map_box(
                [0.0, 0.0, b.width() as f32 * 20.0, b.height() as f32 * 20.0],
                ts,
            );
        }
        _ => {}
    }
    acc
}

/// Geometric bounds of all frames (stage pixels), ignoring invisible styles.
fn geometric_bounds(movie: &Movie) -> Option<[f32; 4]> {
    let base = Transform::from_scale(1.0 / 20.0, 1.0 / 20.0);
    let mut acc = None;
    let mut b = Bounder {
        movie,
        frame: 0,
        cache: HashMap::new(),
    };
    for (i, list) in movie.frames.iter().enumerate() {
        b.frame = i;
        b.list(list, base, ColorTransform::IDENTITY, Clip::All, &mut acc);
    }
    acc
}

/// Geometric bounds refined by rendering every frame at scale 1, so that geometry
/// too small to produce a single covered sample (stray specks in traced art) does
/// not inflate the box.
pub(crate) fn content_bounds(movie: &Movie) -> (f32, f32, f32, f32) {
    let Some(g) = geometric_bounds(movie) else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let rect = (g[0], g[1], g[2], g[3]);
    // Without sprites a frame's image depends only on its display list.
    let has_sprites = movie
        .dict
        .values()
        .any(|c| matches!(c, Character::Sprite(_)));
    let mut px: Option<(u32, u32, u32, u32)> = None;
    for (i, list) in movie.frames.iter().enumerate() {
        if !has_sprites && movie.frames[..i].iter().any(|prev| prev == list) {
            continue;
        }
        if let Some(b) = render(movie, i, 1.0, rect).opaque_bounds() {
            px = Some(match px {
                None => b,
                Some(p) => (p.0.min(b.0), p.1.min(b.1), p.2.max(b.2), p.3.max(b.3)),
            });
        }
    }
    let Some((x0, y0, x1, y1)) = px else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    (
        g[0].max(g[0] + x0 as f32),
        g[1].max(g[1] + y0 as f32),
        g[2].min(g[0] + x1 as f32),
        g[3].min(g[1] + y1 as f32),
    )
}
