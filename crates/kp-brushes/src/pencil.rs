//! Plain pencil `RCircle` (RE_BRUSHES §4). Chalk/Crayon/Marker are pad brushes.

use crate::color::gdi_rgb5;
use crate::pad::walk_line;
use crate::raster::{fill_ellipse, fill_rect};
use crate::tool::{BrushImpl, Cx};
use mohawk::Image;

#[derive(Default)]
pub struct Pencil {
    last: (i32, i32),
    moved: bool,
    size: i32,
    step: i32,
}

impl Pencil {
    pub fn new() -> Pencil {
        Pencil::default()
    }

    fn dab(&self, cv: &mut Image, p: (i32, i32), cx: &Cx) {
        let c = gdi_rgb5(cx.ctx.color);
        let (l, t) = (p.0 - self.size / 2, p.1 - self.size / 2);
        let (r, b) = (l + self.size, t + self.size);
        if cx.ctx.pencil_square {
            fill_rect(cv, l, t, r, b, c);
        } else {
            fill_ellipse(cv, l, t, r, b, c);
        }
    }
}

impl BrushImpl for Pencil {
    fn down(&mut self, _cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        self.size = cx.size();
        self.step = (self.size / 3).clamp(1, 3);
        self.last = p;
        self.moved = false;
    }

    fn held(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if p == self.last {
            return;
        }
        if !self.moved {
            self.dab(cv, self.last, cx);
        }
        self.moved = true;
        let mut q = self.last;
        for (dx, dy) in walk_line(self.last, p, self.step) {
            q = (q.0 + dx, q.1 + dy);
            self.dab(cv, q, cx);
        }
        self.last = p;
    }

    fn up(&mut self, cv: &mut Image, p: (i32, i32), cx: &mut Cx) {
        if !self.moved {
            self.dab(cv, p, cx); // a click stamps once on mouse up [O]
        }
    }
}
