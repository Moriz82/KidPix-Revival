//! Tests on SWF movies assembled in code (no Kid Pix data needed).

use kp_swf::{Error, Movie, Rgba};

#[derive(Default)]
struct W {
    buf: Vec<u8>,
    acc: u8,
    n: u8,
}

impl W {
    fn ub(&mut self, bits: u32, v: u32) {
        for i in (0..bits).rev() {
            self.acc = (self.acc << 1) | ((v >> i) & 1) as u8;
            self.n += 1;
            if self.n == 8 {
                self.buf.push(self.acc);
                self.acc = 0;
                self.n = 0;
            }
        }
    }
    fn sb(&mut self, bits: u32, v: i32) {
        self.ub(bits, v as u32 & ((1u64 << bits) - 1) as u32);
    }
    fn align(&mut self) {
        if self.n > 0 {
            let pad = 8 - u32::from(self.n);
            self.ub(pad, 0);
        }
    }
    fn u8(&mut self, v: u8) {
        self.align();
        self.buf.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.align();
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, b: &[u8]) {
        self.align();
        self.buf.extend_from_slice(b);
    }
    fn finish(mut self) -> Vec<u8> {
        self.align();
        self.buf
    }
}

fn sbits(vals: &[i32]) -> u32 {
    vals.iter()
        .map(|&v| {
            33 - if v < 0 {
                (!v).leading_zeros()
            } else {
                v.leading_zeros()
            }
        })
        .max()
        .unwrap_or(1)
        .max(2)
}

fn rect(w: &mut W, x0: i32, y0: i32, x1: i32, y1: i32) {
    w.align();
    let n = sbits(&[x0, x1, y0, y1]);
    w.ub(5, n);
    for v in [x0, x1, y0, y1] {
        w.sb(n, v);
    }
    w.align();
}

/// Matrix with scale (16.16) and translation (twips).
fn matrix(w: &mut W, sx: f32, sy: f32, tx: i32, ty: i32) {
    w.align();
    if sx != 1.0 || sy != 1.0 {
        let (a, b) = ((sx * 65536.0) as i32, (sy * 65536.0) as i32);
        let n = sbits(&[a, b]);
        w.ub(1, 1);
        w.ub(5, n);
        w.sb(n, a);
        w.sb(n, b);
    } else {
        w.ub(1, 0);
    }
    w.ub(1, 0);
    let n = sbits(&[tx, ty]);
    w.ub(5, n);
    w.sb(n, tx);
    w.sb(n, ty);
    w.align();
}

fn tag(out: &mut Vec<u8>, code: u16, body: &[u8]) {
    out.extend_from_slice(&((code << 6) | 0x3f).to_le_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
}

fn swf(width_px: i32, height_px: i32, frames: u16, tags: &[u8]) -> Vec<u8> {
    let mut w = W::default();
    rect(&mut w, 0, 0, width_px * 20, height_px * 20);
    w.u16(12 << 8);
    w.u16(frames);
    let mut body = w.finish();
    body.extend_from_slice(tags);
    body.extend_from_slice(&[0, 0]);
    let mut out = b"FWS\x04".to_vec();
    out.extend_from_slice(&((body.len() + 8) as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

enum Fill {
    Solid([u8; 4]),
    /// Linear gradient spanning `x0..x1` twips horizontally.
    Linear(i32, i32, [u8; 4], [u8; 4]),
    /// Clipped bitmap fill; bitmap pixels scaled by the given factor (twips per pixel).
    Bitmap(u16, f32),
}

enum Rec {
    Style {
        mv: Option<(i32, i32)>,
        f0: Option<u32>,
        f1: Option<u32>,
        line: Option<u32>,
    },
    Line(i32, i32),
}

fn style(mv: Option<(i32, i32)>, f0: Option<u32>, f1: Option<u32>, line: Option<u32>) -> Rec {
    Rec::Style { mv, f0, f1, line }
}

/// DefineShape3 (RGBA colors) with the given styles and records.
fn define_shape(id: u16, fills: &[Fill], lines: &[(u16, [u8; 4])], recs: &[Rec]) -> Vec<u8> {
    let mut w = W::default();
    w.u16(id);
    rect(&mut w, -2000, -2000, 8000, 8000);
    w.u8(fills.len() as u8);
    for f in fills {
        match *f {
            Fill::Solid(c) => {
                w.u8(0);
                w.bytes(&c);
            }
            Fill::Linear(x0, x1, c0, c1) => {
                w.u8(0x10);
                // Map gradient square -16384..16384 onto x0..x1.
                let s = (x1 - x0) as f32 / 32768.0;
                matrix(&mut w, s, s, (x0 + x1) / 2, 0);
                w.u8(2);
                w.u8(0);
                w.bytes(&c0);
                w.u8(255);
                w.bytes(&c1);
            }
            Fill::Bitmap(id, k) => {
                w.u8(0x41);
                w.u16(id);
                matrix(&mut w, k, k, 0, 0);
            }
        }
    }
    w.u8(lines.len() as u8);
    for (width, c) in lines {
        w.u16(*width);
        w.bytes(c);
    }
    w.u8(0x44); // 4 fill bits, 4 line bits
    for r in recs {
        match *r {
            Rec::Style { mv, f0, f1, line } => {
                w.ub(1, 0);
                let flags = u32::from(line.is_some()) << 3
                    | u32::from(f1.is_some()) << 2
                    | u32::from(f0.is_some()) << 1
                    | u32::from(mv.is_some());
                w.ub(5, flags);
                if let Some((x, y)) = mv {
                    let n = sbits(&[x, y]);
                    w.ub(5, n);
                    w.sb(n, x);
                    w.sb(n, y);
                }
                for v in [f0, f1, line].into_iter().flatten() {
                    w.ub(4, v);
                }
            }
            Rec::Line(dx, dy) => {
                let n = sbits(&[dx, dy]);
                w.ub(1, 1);
                w.ub(1, 1);
                w.ub(4, n - 2);
                w.ub(1, 1);
                w.sb(n, dx);
                w.sb(n, dy);
            }
        }
    }
    w.ub(6, 0);
    w.finish()
}

/// Rectangle path (clockwise in screen space) as records with fill1 = `fill`.
fn rect_recs(x: i32, y: i32, w: i32, h: i32, f0: Option<u32>, f1: Option<u32>) -> Vec<Rec> {
    vec![
        style(Some((x, y)), f0, f1, None),
        Rec::Line(w, 0),
        Rec::Line(0, h),
        Rec::Line(-w, 0),
        Rec::Line(0, -h),
    ]
}

struct Place {
    depth: u16,
    id: Option<u16>,
    is_move: bool,
    m: Option<(f32, f32, i32, i32)>,
    cx: Option<([i32; 4], [i32; 4])>,
    clip: Option<u16>,
}

fn place(depth: u16, id: u16) -> Place {
    Place {
        depth,
        id: Some(id),
        is_move: false,
        m: Some((1.0, 1.0, 0, 0)),
        cx: None,
        clip: None,
    }
}

fn place_tag(out: &mut Vec<u8>, p: &Place) {
    let mut w = W::default();
    let flags = u8::from(p.is_move)
        | u8::from(p.id.is_some()) << 1
        | u8::from(p.m.is_some()) << 2
        | u8::from(p.cx.is_some()) << 3
        | u8::from(p.clip.is_some()) << 6;
    w.u8(flags);
    w.u16(p.depth);
    if let Some(id) = p.id {
        w.u16(id);
    }
    if let Some((sx, sy, tx, ty)) = p.m {
        matrix(&mut w, sx, sy, tx, ty);
    }
    if let Some((mul, add)) = p.cx {
        let all: Vec<i32> = mul.iter().chain(add.iter()).copied().collect();
        let n = sbits(&all);
        w.ub(1, 1);
        w.ub(1, 1);
        w.ub(4, n);
        for v in mul.iter().chain(add.iter()) {
            w.sb(n, *v);
        }
        w.align();
    }
    if let Some(c) = p.clip {
        w.u16(c);
    }
    tag(out, 26, &w.finish());
}

fn show_frame(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x40, 0x00]);
}

fn px(img: &Rgba, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width + x) * 4) as usize;
    img.pixels[i..i + 4].try_into().unwrap()
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// 100x100 px red square at the origin of a 200x200 stage.
fn red_square_movie(f0: Option<u32>, f1: Option<u32>, ccw: bool) -> Movie {
    let recs = if ccw {
        vec![
            style(Some((0, 0)), f0, f1, None),
            Rec::Line(0, 2000),
            Rec::Line(2000, 0),
            Rec::Line(0, -2000),
            Rec::Line(-2000, 0),
        ]
    } else {
        rect_recs(0, 0, 2000, 2000, f0, f1)
    };
    let mut t = Vec::new();
    tag(
        &mut t,
        32,
        &define_shape(1, &[Fill::Solid(RED)], &[], &recs),
    );
    place_tag(&mut t, &place(1, 1));
    show_frame(&mut t);
    Movie::parse(&swf(200, 200, 1, &t)).unwrap()
}

#[test]
fn rejects_non_swf() {
    assert!(matches!(Movie::parse(b"hello world"), Err(Error::NotSwf)));
    assert!(matches!(Movie::parse(b"FWS"), Err(Error::NotSwf)));
    assert!(Movie::parse(b"FWS\x04\x10\x00\x00\x00\xff").is_err());
    assert!(matches!(
        Movie::parse(b"ZWS\x0d\x10\x00\x00\x00\x00\x00\x00\x00"),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn header_and_square() {
    let m = red_square_movie(None, Some(1), false);
    assert_eq!(m.stage(), (0.0, 0.0, 200.0, 200.0));
    assert_eq!(m.frame_count(), 1);
    assert_eq!(m.frame_rate(), 12.0);
    assert!(m.warnings().is_empty(), "{:?}", m.warnings());
    let img = m.render_frame(0, 1.0);
    assert_eq!((img.width, img.height), (200, 200));
    assert_eq!(px(&img, 50, 50), RED);
    assert_eq!(px(&img, 0, 0), RED);
    assert_eq!(px(&img, 99, 99), RED);
    assert_eq!(px(&img, 100, 50)[3], 0);
    assert_eq!(px(&img, 150, 150), [0, 0, 0, 0]);
    assert_eq!(img.opaque_bounds(), Some((0, 0, 100, 100)));
    assert_eq!(m.content_bounds(), (0.0, 0.0, 100.0, 100.0));
}

#[test]
fn fill_side_rules() {
    // fill0 on a counter-clockwise loop covers the same interior as fill1 clockwise.
    let a = red_square_movie(None, Some(1), false).render_frame(0, 1.0);
    let b = red_square_movie(Some(1), None, true).render_frame(0, 1.0);
    assert_eq!(a, b);
    // Same style on both sides of every edge: nothing to fill.
    let c = red_square_movie(Some(1), Some(1), false).render_frame(0, 1.0);
    assert!(c.opaque_bounds().is_none());
}

#[test]
fn scale_and_crop() {
    let m = red_square_movie(None, Some(1), false);
    let img = m.render_frame(0, 0.5);
    assert_eq!((img.width, img.height), (100, 100));
    assert_eq!(img.opaque_bounds(), Some((0, 0, 50, 50)));
    let crop = m.render_frame_rect(0, 2.0, (25.0, 25.0, 75.0, 75.0));
    assert_eq!((crop.width, crop.height), (100, 100));
    assert!(crop.pixels.chunks_exact(4).all(|p| p == RED));
    let edge = m.render_frame_rect(0, 1.0, (50.0, 50.0, 150.5, 150.0));
    assert_eq!((edge.width, edge.height), (101, 100));
    assert_eq!(edge.opaque_bounds(), Some((0, 0, 50, 50)));
    // Degenerate requests give empty images instead of panicking.
    assert_eq!(m.render_frame(0, 0.0).pixels.len(), 0);
    assert_eq!(m.render_frame_rect(0, 1.0, (5.0, 5.0, 5.0, 9.0)).width, 0);
    assert_eq!(m.render_frame(0, f32::NAN).pixels.len(), 0);
}

#[test]
fn shared_edge_has_no_seam() {
    // Two fills meeting at x = 50.5 px: red on the west, blue on the east.
    let recs = vec![
        // Shared edge drawn downwards: fill0 is on the traveller's left (east, blue),
        // fill1 on the right (west, red).
        style(Some((1010, 0)), Some(2), Some(1), None),
        Rec::Line(0, 2000),
        // outer boundary of red (red on the right of travel when going clockwise)
        style(None, Some(0), Some(1), None),
        Rec::Line(-1010, 0),
        Rec::Line(0, -2000),
        Rec::Line(1010, 0),
        // outer boundary of blue
        style(None, Some(0), Some(2), None),
        Rec::Line(990, 0),
        Rec::Line(0, 2000),
        Rec::Line(-990, 0),
    ];
    let mut t = Vec::new();
    tag(
        &mut t,
        32,
        &define_shape(1, &[Fill::Solid(RED), Fill::Solid(BLUE)], &[], &recs),
    );
    place_tag(&mut t, &place(1, 1));
    show_frame(&mut t);
    let m = Movie::parse(&swf(100, 100, 1, &t)).unwrap();
    let img = m.render_frame(0, 1.0);
    assert_eq!(px(&img, 10, 10), RED);
    assert_eq!(px(&img, 90, 10), BLUE);
    for y in 0..100 {
        let p = px(&img, 50, y);
        assert_eq!(p[3], 255, "seam at row {y}: {p:?}");
        assert!(
            p[0] > 64 && p[2] > 64,
            "edge pixel should mix colors: {p:?}"
        );
    }
}

#[test]
fn transform_color_transform_and_frames() {
    let mut t = Vec::new();
    tag(
        &mut t,
        32,
        &define_shape(
            1,
            &[Fill::Solid(RED)],
            &[],
            &rect_recs(0, 0, 400, 400, None, Some(1)),
        ),
    );
    // frame 0: translated by (40, 20) px and scaled 2x
    let mut p = place(1, 1);
    p.m = Some((2.0, 2.0, 800, 400));
    place_tag(&mut t, &p);
    show_frame(&mut t);
    // frame 1: move + color transform red -> green
    place_tag(
        &mut t,
        &Place {
            depth: 1,
            id: None,
            is_move: true,
            m: None,
            cx: Some(([0, 256, 256, 256], [0, 255, 0, 0])),
            clip: None,
        },
    );
    show_frame(&mut t);
    // frame 2: removed
    tag(&mut t, 28, &1u16.to_le_bytes());
    show_frame(&mut t);
    let m = Movie::parse(&swf(100, 100, 3, &t)).unwrap();
    assert_eq!(m.frame_count(), 3);
    let f0 = m.render_frame(0, 1.0);
    assert_eq!(f0.opaque_bounds(), Some((40, 20, 80, 60)));
    assert_eq!(px(&f0, 60, 40), RED);
    let f1 = m.render_frame(1, 1.0);
    assert_eq!(f1.opaque_bounds(), Some((40, 20, 80, 60)));
    assert_eq!(px(&f1, 60, 40), [0, 255, 0, 255]);
    assert!(m.render_frame(2, 1.0).opaque_bounds().is_none());
    // frame index wraps
    assert_eq!(m.render_frame(3, 1.0), f0);
    assert_eq!(m.content_bounds(), (40.0, 20.0, 80.0, 60.0));
}

#[test]
fn sprite_follows_root_frame() {
    let mut t = Vec::new();
    tag(
        &mut t,
        32,
        &define_shape(
            1,
            &[Fill::Solid(BLUE)],
            &[],
            &rect_recs(0, 0, 200, 200, None, Some(1)),
        ),
    );
    // Sprite 2: frame 0 square at x=0, frame 1 at x=50px.
    let mut st = Vec::new();
    {
        let mut w = W::default();
        w.u16(2);
        w.u16(2);
        let mut inner = Vec::new();
        place_tag(&mut inner, &place(1, 1));
        show_frame(&mut inner);
        let mut mv = place(1, 1);
        mv.id = None;
        mv.is_move = true;
        mv.m = Some((1.0, 1.0, 1000, 0));
        place_tag(&mut inner, &mv);
        show_frame(&mut inner);
        inner.extend_from_slice(&[0, 0]);
        w.bytes(&inner);
        st.extend(w.finish());
    }
    tag(&mut t, 39, &st);
    let mut p = place(1, 2);
    p.m = Some((1.0, 1.0, 0, 400));
    place_tag(&mut t, &p);
    for _ in 0..4 {
        show_frame(&mut t);
    }
    let m = Movie::parse(&swf(100, 100, 4, &t)).unwrap();
    assert_eq!(m.frame_count(), 4);
    assert_eq!(
        m.render_frame(0, 1.0).opaque_bounds(),
        Some((0, 20, 10, 30))
    );
    assert_eq!(
        m.render_frame(1, 1.0).opaque_bounds(),
        Some((50, 20, 60, 30))
    );
    assert_eq!(
        m.render_frame(2, 1.0).opaque_bounds(),
        Some((0, 20, 10, 30))
    );
    assert_eq!(
        m.render_frame(3, 1.0).opaque_bounds(),
        Some((50, 20, 60, 30))
    );
    assert_eq!(m.content_bounds(), (0.0, 20.0, 60.0, 30.0));
}

#[test]
fn clip_depth_masks_layers() {
    let mut t = Vec::new();
    // mask: 50x50 px square; content: 100x100 px red square
    tag(
        &mut t,
        32,
        &define_shape(
            1,
            &[Fill::Solid(BLUE)],
            &[],
            &rect_recs(0, 0, 1000, 1000, None, Some(1)),
        ),
    );
    tag(
        &mut t,
        32,
        &define_shape(
            2,
            &[Fill::Solid(RED)],
            &[],
            &rect_recs(0, 0, 2000, 2000, None, Some(1)),
        ),
    );
    let mut clip = place(1, 1);
    clip.clip = Some(5);
    place_tag(&mut t, &clip);
    place_tag(&mut t, &place(2, 2));
    // depth 6 is outside the clip range
    let mut p = place(6, 2);
    p.m = Some((0.5, 0.5, 1600, 1600));
    place_tag(&mut t, &p);
    show_frame(&mut t);
    let m = Movie::parse(&swf(200, 200, 1, &t)).unwrap();
    let img = m.render_frame(0, 1.0);
    assert_eq!(px(&img, 25, 25), RED);
    assert_eq!(px(&img, 75, 75)[3], 0);
    assert_eq!(px(&img, 100, 100), RED);
    assert_eq!(m.content_bounds(), (0.0, 0.0, 130.0, 130.0));
}

#[test]
fn gradient_and_lines() {
    let mut recs = rect_recs(0, 0, 2000, 1000, None, Some(1));
    // a 2px-wide horizontal black line under the gradient box
    recs.push(style(Some((0, 1400)), Some(0), Some(0), Some(1)));
    recs.push(Rec::Line(2000, 0));
    let mut t = Vec::new();
    tag(
        &mut t,
        32,
        &define_shape(
            1,
            &[Fill::Linear(0, 2000, RED, BLUE)],
            &[(40, [0, 0, 0, 255])],
            &recs,
        ),
    );
    place_tag(&mut t, &place(1, 1));
    show_frame(&mut t);
    let m = Movie::parse(&swf(100, 100, 1, &t)).unwrap();
    let img = m.render_frame(0, 1.0);
    let l = px(&img, 2, 25);
    let r = px(&img, 97, 25);
    let mid = px(&img, 50, 25);
    assert!(l[0] > 240 && l[2] < 15, "{l:?}");
    assert!(r[2] > 240 && r[0] < 15, "{r:?}");
    assert!(
        (100..160).contains(&mid[0]) && (100..160).contains(&mid[2]),
        "{mid:?}"
    );
    assert_eq!(px(&img, 50, 70), [0, 0, 0, 255]);
    assert_eq!(px(&img, 50, 69), [0, 0, 0, 255]);
    assert_eq!(px(&img, 50, 72)[3], 0);
    // round caps extend the 0..100px line by half its width (1px) on both sides
    let (x0, y0, x1, y1) = m.content_bounds();
    assert!(
        (x0 + 1.0).abs() < 1e-3 && (x1 - 101.0).abs() < 1e-3,
        "{x0} {x1}"
    );
    assert!(y0.abs() < 1e-3 && (y1 - 71.0).abs() < 1e-3, "{y0} {y1}");
}

#[test]
fn deterministic() {
    let m = red_square_movie(None, Some(1), false);
    let a = m.render_frame_rect(0, 1.37, (3.3, 7.1, 120.9, 111.2));
    let b = m.render_frame_rect(0, 1.37, (3.3, 7.1, 120.9, 111.2));
    assert_eq!(a, b);
}

#[test]
fn lossless_bitmap_fill() {
    // 2x2 ARGB bitmap: red, green / blue, half-transparent white (premultiplied).
    let argb: [u8; 16] = [
        255, 255, 0, 0, 255, 0, 255, 0, //
        255, 0, 0, 255, 128, 128, 128, 128,
    ];
    let mut b = vec![];
    b.extend_from_slice(&7u16.to_le_bytes());
    b.push(5);
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend(miniz_oxide::deflate::compress_to_vec_zlib(&argb, 6));
    let mut t = Vec::new();
    tag(&mut t, 36, &b);
    // 40x40 px square filled with the bitmap scaled so each bitmap pixel is 20x20 px.
    tag(
        &mut t,
        32,
        &define_shape(
            1,
            &[Fill::Bitmap(7, 400.0)],
            &[],
            &rect_recs(0, 0, 800, 800, None, Some(1)),
        ),
    );
    place_tag(&mut t, &place(1, 1));
    show_frame(&mut t);
    let m = Movie::parse(&swf(40, 40, 1, &t)).unwrap();
    assert!(m.warnings().is_empty(), "{:?}", m.warnings());
    let img = m.render_frame(0, 1.0);
    assert_eq!(px(&img, 5, 5), RED);
    assert_eq!(px(&img, 30, 5), [0, 255, 0, 255]);
    assert_eq!(px(&img, 5, 30), BLUE);
    let w = px(&img, 30, 30);
    assert!((126..=130).contains(&w[3]) && w[0] > 250, "{w:?}");
}

#[test]
fn movie_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<Movie>();
    check::<Rgba>();
    check::<Error>();
}
