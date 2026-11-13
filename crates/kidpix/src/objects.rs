//! Things on the "plastic layer" above the picture: stickers, animations, movies and
//! added graphics. Selected objects show the original edit frame (`Data/Buttons.MHK`):
//! scale handles, flip handles for stickers, play-in-a-loop / frame-by-frame for
//! animations and movies.

use crate::assets::SpriteSet;
use crate::gfx::{self, Image};
use std::rc::Rc;

/// Side of a handle box (the Buttons.MHK cels are 18x18).
const H: i32 = 18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    ScaleTL,
    ScaleBR,
    FlipH,
    FlipV,
    Loop,
    Step,
}

#[derive(Clone)]
pub struct PageObj {
    pub frames: Vec<Image>,
    pub fps: f32,
    pub x: i32,
    pub y: i32,
    pub t: f32,
    /// Flash source (stickers/animations) so resizing re-renders sharply.
    pub source: Option<Rc<[u8]>>,
    pub zoom: f32,
    /// QuickTime movie (frames[0] is its poster frame).
    pub movie: Option<Rc<crate::movie::Movie>>,
    pub flip_h: bool,
    pub flip_v: bool,
    /// "Play in a Loop" handle toggled on.
    pub looping: bool,
    /// Frame shown when not playing ("Frame-by-Frame" handle advances it).
    pub frame_idx: usize,
}

impl PageObj {
    pub fn new(frames: Vec<Image>, fps: f32, x: i32, y: i32, source: Option<Rc<[u8]>>, movie: Option<Rc<crate::movie::Movie>>) -> PageObj {
        PageObj { frames, fps, x, y, t: 0.0, source, zoom: 1.0, movie, flip_h: false, flip_v: false, looping: false, frame_idx: 0 }
    }

    /// Animations and movies get play handles; stickers and pictures get flip handles.
    pub fn animated(&self) -> bool {
        self.movie.is_some() || self.frames.len() > 1
    }

    pub fn size(&self) -> (i32, i32) {
        (self.frames[0].width as i32, self.frames[0].height as i32)
    }

    fn frame_count(&self) -> usize {
        match &self.movie {
            Some(m) => m.frame_count(),
            None => self.frames.len(),
        }
    }

    pub fn step(&mut self) {
        let n = self.frame_count().max(1);
        self.frame_idx = (self.frame_idx + 1) % n;
    }

    /// The image to show now. `page_time` drives movies while the page plays.
    pub fn current(&self, playing: bool, page_time: f32) -> Image {
        let run = playing || self.looping;
        let img = match &self.movie {
            Some(m) => {
                let i = if run {
                    let t = if self.looping { self.t } else { page_time };
                    ((t * m.fps) as usize) % m.frame_count().max(1)
                } else {
                    self.frame_idx
                };
                let f = m.frame(i);
                if f.width as i32 != self.size().0 {
                    gfx::resize(&f, self.frames[0].width, self.frames[0].height)
                } else {
                    f
                }
            }
            None => {
                let i = if run && self.frames.len() > 1 {
                    (self.t * self.fps.max(1.0)) as usize % self.frames.len()
                } else {
                    self.frame_idx % self.frames.len()
                };
                self.frames[i].clone()
            }
        };
        if self.flip_h || self.flip_v {
            flip(&img, self.flip_h, self.flip_v)
        } else {
            img
        }
    }

    /// Opaque pixel at object-local (x, y)?
    pub fn hit(&self, x: i32, y: i32) -> bool {
        if self.movie.is_some() {
            return true;
        }
        let (w, h) = self.size();
        let sx = if self.flip_h { w - 1 - x } else { x };
        let sy = if self.flip_v { h - 1 - y } else { y };
        self.frames[0].get(sx, sy).is_some_and(|p| p >> 24 > 16)
    }

    fn handle_boxes(&self) -> [(Handle, i32, i32); 4] {
        let (w, h) = self.size();
        let (x0, y0, x1, y1) = (self.x, self.y, self.x + w, self.y + h);
        let (tr, bl) = if self.animated() { (Handle::Loop, Handle::Step) } else { (Handle::FlipH, Handle::FlipV) };
        [(Handle::ScaleTL, x0 - H, y0 - H), (tr, x1, y0 - H), (bl, x0 - H, y1), (Handle::ScaleBR, x1, y1)]
    }

    /// Handle under canvas-local (x, y).
    pub fn handle_at(&self, x: i32, y: i32) -> Option<Handle> {
        self.handle_boxes().into_iter().find(|&(_, hx, hy)| x >= hx && y >= hy && x < hx + H && y < hy + H).map(|(h, _, _)| h)
    }

    /// Re-render at a new zoom around the centre.
    pub fn rescale(&mut self, zoom: f32) {
        let zoom = zoom.clamp(0.2, 5.0);
        let (ow, oh) = self.size();
        let (cx, cy) = (self.x + ow / 2, self.y + oh / 2);
        let frames = match &self.source {
            Some(src) => crate::stickers::render(src, self.frames.len() > 1, zoom).map(|r| r.0),
            None => {
                let f = zoom / self.zoom;
                Some(
                    self.frames
                        .iter()
                        .map(|im| gfx::resize(im, ((im.width as f32 * f) as u32).max(4), ((im.height as f32 * f) as u32).max(4)))
                        .collect(),
                )
            }
        };
        if let Some(frames) = frames {
            self.frames = frames;
            self.zoom = zoom;
            let (nw, nh) = self.size();
            self.x = cx - nw / 2;
            self.y = cy - nh / 2;
        }
    }

    /// The edit frame with its handles, in screen coordinates (offset = canvas origin).
    pub fn draw_frame(&self, frame: &mut Image, ox: i32, oy: i32, handles: Option<&SpriteSet>, mouse: (i32, i32)) {
        let (w, h) = self.size();
        dotted(frame, ox + self.x - 1, oy + self.y - 1, w + 2, h + 2);
        for (kind, hx, hy) in self.handle_boxes() {
            let (sx, sy) = (ox + hx, oy + hy);
            let hover = mouse.0 >= sx && mouse.1 >= sy && mouse.0 < sx + H && mouse.1 < sy + H;
            // Buttons.MHK cels (normal, hover).
            let cel = match kind {
                Handle::ScaleBR => 5,
                Handle::ScaleTL => 11,
                Handle::Loop => 13,
                Handle::Step => 15,
                Handle::FlipH => 17,
                Handle::FlipV => 19,
            } + hover as u16;
            match handles.and_then(|s| s.cel(cel)) {
                Some(img) => gfx::blit(frame, img, sx, sy),
                None => {
                    gfx::fill_rect(frame, sx, sy, H, H, if hover { 0xffff_80c0 } else { 0xffe0_40a0 });
                    gfx::stroke_rect(frame, sx, sy, H, H, 1, 0xff00_0000);
                }
            }
        }
    }
}

pub fn flip(img: &Image, h: bool, v: bool) -> Image {
    let mut out = Image::new(img.width, img.height);
    let (w, hh) = (img.width as usize, img.height as usize);
    for y in 0..hh {
        for x in 0..w {
            let sx = if h { w - 1 - x } else { x };
            let sy = if v { hh - 1 - y } else { y };
            out.pixels[y * w + x] = img.pixels[sy * w + sx];
        }
    }
    out
}

fn dotted(frame: &mut Image, x: i32, y: i32, w: i32, h: i32) {
    for i in 0..w {
        let c = if (i / 3) % 2 == 0 { 0xff00_0000 } else { 0xffff_ffff };
        frame.put(x + i, y, c);
        frame.put(x + i, y + h - 1, c);
    }
    for i in 0..h {
        let c = if (i / 3) % 2 == 0 { 0xff00_0000 } else { 0xffff_ffff };
        frame.put(x, y + i, c);
        frame.put(x + w - 1, y + i, c);
    }
}
