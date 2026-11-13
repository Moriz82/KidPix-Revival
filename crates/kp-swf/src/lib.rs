//! Renderer for the small Flash movies Kid Pix Deluxe 3 uses for stickers and toons.
//!
//! Parses uncompressed (`FWS`) or zlib (`CWS`) SWF files and rasterizes any frame to
//! straight-alpha RGBA8. Supported: DefineShape1–4 (solid, linear, radial and focal
//! gradient fills, bitmap fills, line styles), DefineBitsLossless/2, DefineBits/JPEG2/3/4
//! (JPEG only), DefineFont/2/3 + DefineText/2 (static text), DefineSprite (nested
//! timelines), PlaceObject/2/3 (matrix, color transform, clip depth), RemoveObject/2 and
//! ShowFrame. Sound, actions, buttons, morph shapes and edit text are skipped.
//!
//! Rendering supersamples 4×4 without per-path anti-aliasing and box-filters down, so
//! adjacent fills of one shape meet without the light seams a per-path AA rasterizer
//! leaves on shared edges.

mod bitmap;
mod reader;
mod render;
mod shape;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::OnceLock;

use tiny_skia::{Path, Pixmap, Transform};

use reader::Reader;
use shape::Shape;
pub use shape::ShapeStats;

/// A rendered frame: straight (non-premultiplied) RGBA8, row-major, `width * height * 4` bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl fmt::Debug for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rgba({}x{})", self.width, self.height)
    }
}

impl Rgba {
    /// Bounding box `(x0, y0, x1, y1)` (exclusive end) of pixels with alpha > 0.
    pub fn opaque_bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..self.height {
            for x in 0..self.width {
                if self.pixels[((y * self.width + x) * 4 + 3) as usize] != 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        (x1 > 0).then_some((x0, y0, x1, y1))
    }
}

#[derive(Debug)]
pub enum Error {
    /// Not an SWF file (bad signature or too short).
    NotSwf,
    /// Data ended inside a record.
    Truncated,
    /// Structurally invalid data.
    Malformed(String),
    /// zlib stream could not be inflated.
    Decompress(String),
    /// A feature this crate does not implement (e.g. LZMA `ZWS` movies).
    Unsupported(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotSwf => write!(f, "not an SWF file"),
            Error::Truncated => write!(f, "unexpected end of SWF data"),
            Error::Malformed(m) => write!(f, "malformed SWF: {m}"),
            Error::Decompress(m) => write!(f, "zlib error: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported SWF feature: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// Flash color transform: `out = clamp(in * mul + add)` per RGBA channel (0..255 scale).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ColorTransform {
    pub mul: [f32; 4],
    pub add: [f32; 4],
}

impl ColorTransform {
    pub const IDENTITY: ColorTransform = ColorTransform {
        mul: [1.0; 4],
        add: [0.0; 4],
    };

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// `self` applied after `child`.
    pub fn then(&self, child: &ColorTransform) -> ColorTransform {
        let mut r = ColorTransform::IDENTITY;
        for c in 0..4 {
            r.mul[c] = self.mul[c] * child.mul[c];
            r.add[c] = self.mul[c] * child.add[c] + self.add[c];
        }
        r
    }

    pub fn apply(&self, c: [u8; 4]) -> [u8; 4] {
        if self.is_identity() {
            return c;
        }
        let mut o = [0u8; 4];
        for i in 0..4 {
            o[i] = (f32::from(c[i]) * self.mul[i] + self.add[i])
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        o
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Placed {
    pub depth: u16,
    pub id: u16,
    pub matrix: Transform,
    pub cxform: ColorTransform,
    pub ratio: u16,
    pub clip_depth: u16,
}

pub(crate) struct Font {
    pub glyphs: Vec<Option<Path>>,
    /// Glyph EM square size in font units (1024, or 20480 for DefineFont3).
    pub em: f32,
}

pub(crate) struct TextGlyph {
    pub font: u16,
    pub index: u16,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: [u8; 4],
}

pub(crate) struct Text {
    pub matrix: Transform,
    pub glyphs: Vec<TextGlyph>,
}

pub(crate) struct Sprite {
    pub frames: Vec<Vec<Placed>>,
}

pub(crate) enum Character {
    Shape(Shape),
    Bitmap(Pixmap),
    Font(Font),
    Text(Text),
    Sprite(Sprite),
    /// Buttons, morph shapes, edit text: may be placed but are not drawn.
    Unsupported,
}

/// A parsed SWF movie, ready to render.
pub struct Movie {
    version: u8,
    /// Stage rect in twips (x0, y0, x1, y1).
    stage: [i32; 4],
    frame_rate: f32,
    header_frames: u16,
    frames: Vec<Vec<Placed>>,
    dict: HashMap<u16, Character>,
    tag_counts: BTreeMap<u16, usize>,
    stats: ShapeStats,
    warnings: Vec<String>,
    bounds: OnceLock<(f32, f32, f32, f32)>,
}

impl fmt::Debug for Movie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Movie")
            .field("version", &self.version)
            .field("stage", &self.stage())
            .field("frame_rate", &self.frame_rate)
            .field("frames", &self.frames.len())
            .field("characters", &self.dict.len())
            .finish()
    }
}

/// Builds display-list snapshots from control tags.
struct Timeline {
    list: BTreeMap<u16, Placed>,
    frames: Vec<Vec<Placed>>,
}

impl Timeline {
    fn new() -> Self {
        Timeline {
            list: BTreeMap::new(),
            frames: Vec::new(),
        }
    }

    fn snapshot(&mut self) {
        self.frames.push(self.list.values().cloned().collect());
    }

    fn finish(mut self) -> Vec<Vec<Placed>> {
        if self.frames.is_empty() {
            self.snapshot();
        }
        self.frames
    }

    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self,
        depth: u16,
        is_move: bool,
        id: Option<u16>,
        matrix: Option<Transform>,
        cxform: Option<ColorTransform>,
        ratio: Option<u16>,
        clip_depth: Option<u16>,
    ) {
        match (is_move, self.list.get_mut(&depth)) {
            (true, Some(p)) => {
                if let Some(id) = id {
                    p.id = id;
                }
                if let Some(m) = matrix {
                    p.matrix = m;
                }
                if let Some(c) = cxform {
                    p.cxform = c;
                }
                if let Some(r) = ratio {
                    p.ratio = r;
                }
                if let Some(c) = clip_depth {
                    p.clip_depth = c;
                }
            }
            _ => {
                if let Some(id) = id {
                    self.list.insert(
                        depth,
                        Placed {
                            depth,
                            id,
                            matrix: matrix.unwrap_or_default(),
                            cxform: cxform.unwrap_or(ColorTransform::IDENTITY),
                            ratio: ratio.unwrap_or(0),
                            clip_depth: clip_depth.unwrap_or(0),
                        },
                    );
                }
            }
        }
    }

    /// Handles a control tag; returns false if the tag is not a control tag.
    fn control(&mut self, code: u16, body: &[u8]) -> reader::Result<bool> {
        let mut r = Reader::new(body);
        match code {
            1 => self.snapshot(),
            4 => {
                let id = r.u16()?;
                let depth = r.u16()?;
                let matrix = r.matrix()?;
                let cx = if r.remaining() > 0 {
                    Some(r.cxform(false)?)
                } else {
                    None
                };
                self.place(depth, false, Some(id), Some(matrix), cx, None, None);
            }
            5 => {
                let _id = r.u16()?;
                let depth = r.u16()?;
                self.list.remove(&depth);
            }
            28 => {
                let depth = r.u16()?;
                self.list.remove(&depth);
            }
            26 | 70 => {
                let flags = r.u8()?;
                let flags2 = if code == 70 { r.u8()? } else { 0 };
                let depth = r.u16()?;
                let has_char = flags & 0x02 != 0;
                if code == 70 && (flags2 & 0x08 != 0 || (flags2 & 0x10 != 0 && has_char)) {
                    r.cstr()?; // class name
                }
                let id = if has_char { Some(r.u16()?) } else { None };
                let matrix = if flags & 0x04 != 0 {
                    Some(r.matrix()?)
                } else {
                    None
                };
                let cx = if flags & 0x08 != 0 {
                    Some(r.cxform(true)?)
                } else {
                    None
                };
                let ratio = if flags & 0x10 != 0 {
                    Some(r.u16()?)
                } else {
                    None
                };
                if flags & 0x20 != 0 {
                    r.cstr()?; // instance name
                }
                let clip = if flags & 0x40 != 0 {
                    Some(r.u16()?)
                } else {
                    None
                };
                self.place(depth, flags & 0x01 != 0, id, matrix, cx, ratio, clip);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

/// Iterate `(code, body)` over a tag stream; stops at End or end of data.
fn for_each_tag(
    data: &[u8],
    warnings: &mut Vec<String>,
    mut f: impl FnMut(u16, &[u8], &mut Vec<String>),
) {
    let mut pos = 0usize;
    while pos + 2 <= data.len() {
        let h = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        let code = h >> 6;
        let mut len = usize::from(h & 0x3f);
        if len == 0x3f {
            if pos + 4 > data.len() {
                warnings.push("truncated long tag header".into());
                break;
            }
            len = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                as usize;
            pos += 4;
        }
        if code == 0 {
            break;
        }
        let end = match pos.checked_add(len) {
            Some(e) if e <= data.len() => e,
            _ => {
                warnings.push(format!("tag {code} truncated"));
                data.len()
            }
        };
        f(code, &data[pos..end], warnings);
        pos = end;
    }
}

fn tag_name(code: u16) -> &'static str {
    match code {
        2 => "DefineShape",
        22 => "DefineShape2",
        32 => "DefineShape3",
        83 => "DefineShape4",
        10 => "DefineFont",
        48 => "DefineFont2",
        75 => "DefineFont3",
        11 => "DefineText",
        33 => "DefineText2",
        20 => "DefineBitsLossless",
        36 => "DefineBitsLossless2",
        6 => "DefineBits",
        21 => "DefineBitsJPEG2",
        35 => "DefineBitsJPEG3",
        90 => "DefineBitsJPEG4",
        39 => "DefineSprite",
        _ => "tag",
    }
}

struct Parser {
    version: u8,
    dict: HashMap<u16, Character>,
    jpeg_tables: Option<Vec<u8>>,
    tag_counts: BTreeMap<u16, usize>,
    stats: ShapeStats,
    warnings: Vec<String>,
}

impl Parser {
    fn define(&mut self, code: u16, body: &[u8], tl: &mut Timeline) -> reader::Result<()> {
        *self.tag_counts.entry(code).or_default() += 1;
        if tl.control(code, body)? {
            return Ok(());
        }
        let mut r = Reader::new(body);
        match code {
            2 | 22 | 32 | 83 => {
                let ver = match code {
                    2 => 1,
                    22 => 2,
                    32 => 3,
                    _ => 4,
                };
                let (id, s) = shape::read_define_shape(&mut r, ver, self.version, &mut self.stats)?;
                self.dict.insert(id, Character::Shape(s));
            }
            20 | 36 => {
                let (id, p) = bitmap::read_lossless(&mut r, code == 36)?;
                self.dict.insert(id, Character::Bitmap(p));
            }
            8 => self.jpeg_tables = Some(body.to_vec()),
            6 | 21 | 35 | 90 => {
                let (id, p) = bitmap::read_jpeg(&mut r, code, self.jpeg_tables.as_deref())?;
                self.dict.insert(id, Character::Bitmap(p));
            }
            10 => {
                let id = r.u16()?;
                let table = r.rest();
                let mut glyphs = Vec::new();
                if table.len() >= 2 {
                    let first = usize::from(u16::from_le_bytes([table[0], table[1]]));
                    let n = first / 2;
                    let off = |i: usize| -> usize {
                        table.get(i * 2..i * 2 + 2).map_or(table.len(), |b| {
                            usize::from(u16::from_le_bytes([b[0], b[1]]))
                        })
                    };
                    for i in 0..n {
                        let (a, b) = (off(i), if i + 1 < n { off(i + 1) } else { table.len() });
                        let g = table
                            .get(a..b.max(a))
                            .ok_or(Error::Truncated)
                            .and_then(|d| shape::read_glyph(d, self.version))?;
                        glyphs.push(g);
                    }
                }
                self.dict
                    .insert(id, Character::Font(Font { glyphs, em: 1024.0 }));
            }
            48 | 75 => {
                let id = r.u16()?;
                let flags = r.u8()?;
                let _lang = r.u8()?;
                let name_len = r.u8()? as usize;
                r.skip(name_len)?;
                let n = r.u16()? as usize;
                let table = r.rest();
                let wide = flags & 0x08 != 0;
                let off = |i: usize| -> Option<usize> {
                    if wide {
                        table
                            .get(i * 4..i * 4 + 4)
                            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
                    } else {
                        table
                            .get(i * 2..i * 2 + 2)
                            .map(|b| usize::from(u16::from_le_bytes([b[0], b[1]])))
                    }
                };
                let mut glyphs = Vec::with_capacity(n);
                for i in 0..n {
                    // Entry n is CodeTableOffset, which ends the last glyph.
                    let (a, b) = (
                        off(i).ok_or(Error::Truncated)?,
                        off(i + 1).unwrap_or(table.len()),
                    );
                    let g = table
                        .get(a..b.max(a))
                        .ok_or(Error::Truncated)
                        .and_then(|d| shape::read_glyph(d, self.version))?;
                    glyphs.push(g);
                }
                let em = if code == 75 { 20480.0 } else { 1024.0 };
                self.dict.insert(id, Character::Font(Font { glyphs, em }));
            }
            11 | 33 => {
                let id = r.u16()?;
                let _bounds = r.rect()?;
                let matrix = r.matrix()?;
                let gbits = u32::from(r.u8()?);
                let abits = u32::from(r.u8()?);
                let mut glyphs = Vec::new();
                let (mut font, mut size, mut color) = (0u16, 0f32, [0, 0, 0, 255]);
                let (mut x, mut y) = (0f32, 0f32);
                loop {
                    let flags = r.u8()?;
                    if flags == 0 {
                        break;
                    }
                    if flags & 0x08 != 0 {
                        font = r.u16()?;
                    }
                    if flags & 0x04 != 0 {
                        color = if code == 11 { r.rgb()? } else { r.rgba()? };
                    }
                    if flags & 0x01 != 0 {
                        x = f32::from(r.i16()?);
                    }
                    if flags & 0x02 != 0 {
                        y = f32::from(r.i16()?);
                    }
                    if flags & 0x08 != 0 {
                        size = f32::from(r.u16()?);
                    }
                    let count = r.u8()?;
                    for _ in 0..count {
                        let index = r.ub(gbits)? as u16;
                        let adv = r.sb(abits)?;
                        glyphs.push(TextGlyph {
                            font,
                            index,
                            x,
                            y,
                            size,
                            color,
                        });
                        x += adv as f32;
                    }
                    r.align();
                }
                self.dict
                    .insert(id, Character::Text(Text { matrix, glyphs }));
            }
            39 => {
                let id = r.u16()?;
                let _frames = r.u16()?;
                let mut sub = Timeline::new();
                let body = r.rest();
                let mut err = None;
                let mut nested_warn = Vec::new();
                for_each_tag(body, &mut nested_warn, |code, b, _| {
                    if err.is_none() {
                        if let Err(e) = sub.control(code, b) {
                            err = Some(e);
                        }
                    }
                });
                self.warnings.extend(nested_warn);
                if let Some(e) = err {
                    return Err(e);
                }
                self.dict.insert(
                    id,
                    Character::Sprite(Sprite {
                        frames: sub.finish(),
                    }),
                );
            }
            7 | 34 | 37 | 46 | 84 => {
                let id = r.u16()?;
                self.dict.insert(id, Character::Unsupported);
                let what = match code {
                    7 | 34 => "button",
                    37 => "edit text",
                    _ => "morph shape",
                };
                self.warnings
                    .push(format!("character {id}: {what} not rendered"));
            }
            _ => {} // sound, actions, labels, background color, protect, ...
        }
        Ok(())
    }
}

impl Movie {
    /// Parse an SWF file (`FWS` uncompressed or `CWS` zlib-compressed).
    pub fn parse(data: &[u8]) -> Result<Movie, Error> {
        if data.len() < 8 {
            return Err(Error::NotSwf);
        }
        let version = data[3];
        let file_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let inflated;
        let body: &[u8] = match &data[..3] {
            b"FWS" => {
                let end = if file_len >= 8 {
                    file_len.min(data.len())
                } else {
                    data.len()
                };
                &data[8..end]
            }
            b"CWS" => {
                inflated = miniz_oxide::inflate::decompress_to_vec_zlib(&data[8..])
                    .map_err(|e| Error::Decompress(format!("{:?}", e.status)))?;
                &inflated
            }
            b"ZWS" => return Err(Error::Unsupported("LZMA-compressed (ZWS) movie")),
            _ => return Err(Error::NotSwf),
        };
        let mut r = Reader::new(body);
        let stage = r.rect()?;
        let frame_rate = f32::from(r.u16()?) / 256.0;
        let header_frames = r.u16()?;
        let tags = &body[r.pos()..];

        let mut warnings = Vec::new();
        let mut tl = Timeline::new();
        let mut p = Parser {
            version,
            dict: HashMap::new(),
            jpeg_tables: None,
            tag_counts: BTreeMap::new(),
            stats: ShapeStats::default(),
            warnings: Vec::new(),
        };
        let mut tag_warnings = Vec::new();
        for_each_tag(tags, &mut warnings, |code, b, _| {
            if let Err(e) = p.define(code, b, &mut tl) {
                let id = b.get(..2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
                tag_warnings.push(format!("{} (code {code}, id {id}): {e}", tag_name(code)));
            }
        });
        let Parser {
            dict,
            tag_counts,
            stats,
            warnings: mut pw,
            ..
        } = p;
        warnings.append(&mut pw);
        warnings.extend(tag_warnings);
        Ok(Movie {
            version,
            stage,
            frame_rate,
            header_frames,
            frames: tl.finish(),
            dict,
            tag_counts,
            stats,
            warnings,
            bounds: OnceLock::new(),
        })
    }

    /// Number of frames (ShowFrame count of the root timeline, at least 1).
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Frames per second from the header.
    pub fn frame_rate(&self) -> f32 {
        self.frame_rate
    }

    /// Stage rect in pixels (twips / 20): `(x_min, y_min, x_max, y_max)`.
    pub fn stage(&self) -> (f32, f32, f32, f32) {
        let [x0, y0, x1, y1] = self.stage;
        (
            x0 as f32 / 20.0,
            y0 as f32 / 20.0,
            x1 as f32 / 20.0,
            y1 as f32 / 20.0,
        )
    }

    /// SWF version byte from the header.
    pub fn version(&self) -> u8 {
        self.version
    }

    /// Frame count claimed by the header (may differ from [`Movie::frame_count`]).
    pub fn header_frame_count(&self) -> u16 {
        self.header_frames
    }

    /// Non-fatal problems met while parsing (skipped characters, truncated tags, ...).
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Number of tags seen per tag code (sprite-internal tags not included).
    pub fn tag_counts(&self) -> &BTreeMap<u16, usize> {
        &self.tag_counts
    }

    /// Fill/line style and edge usage summed over all shapes.
    pub fn shape_stats(&self) -> ShapeStats {
        self.stats
    }

    /// Render `frame` (0-based, wraps modulo [`Movie::frame_count`]) at `scale`
    /// (1.0 = stage pixels) over a transparent background.
    pub fn render_frame(&self, frame: usize, scale: f32) -> Rgba {
        self.render_frame_rect(frame, scale, self.stage())
    }

    /// Render `frame` cropped to `rect` (stage pixels at scale 1); the output is
    /// `ceil(rect size * scale)` pixels.
    pub fn render_frame_rect(&self, frame: usize, scale: f32, rect: (f32, f32, f32, f32)) -> Rgba {
        render::render(self, frame, scale, rect)
    }

    /// Tight bounding box (stage pixels, scale 1) of everything drawn in any frame;
    /// `(0, 0, 0, 0)` if the movie draws nothing.
    ///
    /// Exact geometric bounds, shrunk to the pixels a scale-1 render actually covers
    /// (traced art often contains stray sub-pixel specks far from the drawing). The
    /// first call renders every distinct frame once; the result is cached.
    pub fn content_bounds(&self) -> (f32, f32, f32, f32) {
        *self.bounds.get_or_init(|| render::content_bounds(self))
    }
}
