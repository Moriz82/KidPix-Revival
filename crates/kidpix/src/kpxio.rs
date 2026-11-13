//! Native `.kpx` pages (docs/RE_SHELL.md §8) through the `kp-kpx` crate: the easel's
//! picture, stickers / animations (tSWF), text boxes (tTXT) and page sound (tWAV).
//! Objects the engine doesn't edit (tMBM bitmaps, tMOV movies, unknown resources) are
//! carried over unchanged from the page that was opened.

use crate::easel::{Easel, PageObj};
use crate::gfx::Image;
use crate::textbox::TextBox;
use kp_kpx as kpx;
use std::rc::Rc;

/// tTXT style bits [I]: bold 1, italic 2, underline 4 (the LOGFONT-like order).
const BOLD: u16 = 1;
const ITALIC: u16 = 2;
const UNDERLINE: u16 = 4;

/// Render a sticker SWF to fit a displayed size.
fn render_sticker(swf: &[u8], size: Option<(u16, u16)>) -> Option<(Vec<Image>, f32, f32)> {
    let movie = kp_swf::Movie::parse(swf).ok()?;
    let animated = movie.frame_count() > 1;
    let (x0, y0, x1, y1) = movie.content_bounds();
    let (w, h) = (x1 - x0, y1 - y0);
    let base = (crate::stickers::MAX_SIDE / w.max(h).max(1.0)).min(1.0);
    let zoom = match size {
        Some((tw, th)) if tw > 0 && th > 0 => ((tw as f32 / w.max(1.0)).min(th as f32 / h.max(1.0)) / base).max(0.05),
        _ => 1.0,
    };
    let (frames, fps) = crate::stickers::render(swf, animated, zoom)?;
    Some((frames, fps, zoom))
}

impl Easel {
    /// Load a page read from a `.kpx` file.
    pub fn load_kpx(&mut self, page: kpx::Page) {
        self.new_picture();
        let mut bg = page.background.image.clone();
        if bg.width != crate::paint::CANVAS_W || bg.height != crate::paint::CANVAS_H {
            bg = crate::gfx::resize(&bg, crate::paint::CANVAS_W, crate::paint::CANVAS_H);
        }
        self.canvas.img = bg;
        let face_of = |name: &str, fonts: &crate::ui::Fonts| fonts.find(name).unwrap_or_else(|| fonts.ui_face());
        for o in &page.objects {
            match &o.kind {
                kpx::ObjectKind::Sticker(swf) => {
                    let place = o.sticker_placement();
                    let size = place.size().or(Some((o.rect.width().max(1) as u16, o.rect.height().max(1) as u16)));
                    if let Some((frames, fps, zoom)) = render_sticker(swf, size) {
                        let mut obj = PageObj::new(frames, fps, o.rect.left as i32, o.rect.top as i32, Some(Rc::from(swf.as_slice())), None);
                        obj.zoom = zoom;
                        self.objects.push(obj);
                    }
                }
                kpx::ObjectKind::Text(t) => {
                    let color = match t.rgb() {
                        Some((r, g, b)) => crate::gfx::rgb(r, g, b),
                        None => crate::gfx::rgb(0, 0, 0),
                    };
                    self.texts.push(TextBox {
                        x: t.rect.left as i32,
                        y: t.rect.top as i32,
                        text: t.text.replace("\r\n", "\n").replace('\r', "\n"),
                        font: face_of(&t.face_name(), &self.fonts),
                        size: t.size.max(6) as f32,
                        bold: t.style & BOLD != 0,
                        italic: t.style & ITALIC != 0,
                        underline: t.style & UNDERLINE != 0,
                        align: (t.justification.min(2)) as u8,
                        color,
                    });
                }
                _ => {}
            }
        }
        if let Some(s) = page.sound.as_ref().and_then(|s| s.decode().ok()) {
            self.set_page_sound(std::sync::Arc::new(s));
        }
        self.kpx_base = Some(page);
        self.canvas.dirty = false;
    }

    /// The current picture as a `.kpx` page (unchanged parts keep their original bytes).
    pub fn to_kpx(&mut self) -> kpx::Page {
        self.commit_text_now();
        let composite = self.flattened();
        let mut page = self.kpx_base.clone().unwrap_or_else(kpx::Page::new_blank);
        if page.background.image != self.canvas.img {
            page.background = kpx::Raster::new(self.canvas.img.clone());
        }
        let same_composite = page.composite.as_ref().is_some_and(|c| c.image == composite);
        if !same_composite {
            page.set_composite(composite);
        }
        // Rebuild stickers and text; keep the objects the engine doesn't edit.
        page.objects.retain(|o| !matches!(o.kind, kpx::ObjectKind::Sticker(_) | kpx::ObjectKind::Text(_)));
        for o in &self.objects {
            let Some(src) = &o.source else { continue };
            let (w, h) = o.size();
            let rect = kpx::Rect::from_xywh(o.x as i16, o.y as i16, w as i16, h as i16);
            let place = kpx::StickerPlacement { width: w.clamp(0, 2047) as u16, height: h.clamp(0, 2047) as u16, state: 0, phase: (o.frame_idx & 0xff) as u8 };
            let id = page.next_id(kpx::T_SWF);
            page.objects.push(kpx::Object::sticker(id, rect, src.to_vec(), place));
        }
        for t in &self.texts {
            let (x, y, w, h) = t.rect(&self.fonts);
            let rect = kpx::Rect::from_xywh(x as i16, y as i16, w as i16, h as i16);
            let face = self.fonts.faces.get(t.font).map(|f| f.0.clone()).unwrap_or_default();
            let mut b = kpx::TextBox::new(rect, t.text.replace('\n', "\r"), &face, t.size.round() as u16);
            b.style = if t.bold { BOLD } else { 0 } | if t.italic { ITALIC } else { 0 } | if t.underline { UNDERLINE } else { 0 };
            b.justification = t.align as u16;
            let (_, r, g, bl) = crate::gfx::split(t.color);
            b.set_rgb(r as u8, g as u8, bl as u8);
            let id = page.next_id(kpx::T_TXT);
            page.objects.push(kpx::Object::text(id, b));
        }
        // Page sound: keep the original resource when it is still the same sound.
        match self.page_sound_ref() {
            None => page.sound = None,
            Some(s) => {
                let same = page.sound.as_ref().and_then(|p| p.decode().ok()).is_some_and(|d| d.samples == s.samples && d.sample_rate == s.sample_rate);
                if !same {
                    let mono: Vec<i16> = if s.channels > 1 {
                        s.samples.chunks(s.channels as usize).map(|c| (c.iter().map(|&v| v as i32).sum::<i32>() / c.len() as i32) as i16).collect()
                    } else {
                        s.samples.clone()
                    };
                    page.sound = Some(kpx::PageSound::encode(s.sample_rate.min(u16::MAX as u32) as u16, &mono));
                }
            }
        }
        page
    }
}

pub fn read(path: &std::path::Path) -> Result<kpx::Page, String> {
    kpx::read_file(path).map_err(|e| e.to_string())
}

pub fn write(page: &kpx::Page, path: &std::path::Path) -> Result<(), String> {
    kpx::write_file(page, path).map_err(|e| e.to_string())
}
