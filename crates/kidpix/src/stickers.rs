//! Flash sticker/animation rendering (via the kp-swf crate).

use crate::gfx::Image;

/// Default largest side of a newly placed sticker, in canvas pixels.
pub const MAX_SIDE: f32 = 260.0;

/// Render a sticker (`animated == false`: first frame only) or an animation at `zoom`
/// times its default size. Frames are cropped to the drawn content.
pub fn render(swf: &[u8], animated: bool, zoom: f32) -> Option<(Vec<Image>, f32)> {
    let movie = kp_swf::Movie::parse(swf).ok()?;
    let (x0, y0, x1, y1) = movie.content_bounds();
    let (w, h) = (x1 - x0, y1 - y0);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let scale = (MAX_SIDE / w.max(h)).min(1.0) * zoom.clamp(0.1, 6.0);
    let count = if animated { movie.frame_count().max(1) } else { 1 };
    let frames: Vec<Image> = (0..count)
        .map(|f| {
            let r = movie.render_frame_rect(f, scale, (x0, y0, x1, y1));
            Image::from_rgba8(r.width, r.height, &r.pixels)
        })
        .filter(|i| i.width > 0 && i.height > 0)
        .collect();
    if frames.is_empty() {
        return None;
    }
    Some((frames, movie.frame_rate().max(1.0)))
}
