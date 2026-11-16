//! Selections (rectangle, lasso, magic wand), floating pieces that can be moved, scaled, rotated and
//! flipped, and gradients.

use crate::doc::Rect;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SelectMode {
    Rect,
    Lasso,
    Wand,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Combine {
    Replace,
    Add,
    Subtract,
}

/// Which pixels are selected (0 or 255 each).
#[derive(Clone)]
pub struct Selection {
    pub mask: Vec<u8>,
    pub w: usize,
    pub h: usize,
    pub bounds: Rect,
}

impl Selection {
    pub fn empty(w: usize, h: usize) -> Selection {
        Selection {
            mask: vec![0; w * h],
            w,
            h,
            bounds: Rect::empty(),
        }
    }

    pub fn all(w: usize, h: usize) -> Selection {
        Selection {
            mask: vec![255; w * h],
            w,
            h,
            bounds: Rect::full(w, h),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bounds.is_empty()
    }

    pub fn contains(&self, x: usize, y: usize) -> bool {
        x < self.w && y < self.h && self.mask[y * self.w + x] != 0
    }

    fn recompute_bounds(&mut self) {
        let mut b = Rect::empty();
        for y in 0..self.h {
            let row = &self.mask[y * self.w..(y + 1) * self.w];
            if let Some(first) = row.iter().position(|v| *v != 0) {
                let last = row.iter().rposition(|v| *v != 0).unwrap();
                b = b.union(&Rect {
                    x0: first,
                    y0: y,
                    x1: last + 1,
                    y1: y + 1,
                });
            }
        }
        self.bounds = b;
    }

    /// Merge another selection into this one.
    pub fn combine(&mut self, other: &Selection, how: Combine) {
        match how {
            Combine::Replace => self.mask.copy_from_slice(&other.mask),
            Combine::Add => {
                for (a, b) in self.mask.iter_mut().zip(&other.mask) {
                    *a = (*a).max(*b);
                }
            }
            Combine::Subtract => {
                for (a, b) in self.mask.iter_mut().zip(&other.mask) {
                    if *b != 0 {
                        *a = 0;
                    }
                }
            }
        }
        self.recompute_bounds();
    }

    pub fn invert(&mut self) {
        for v in self.mask.iter_mut() {
            *v = 255 - *v;
        }
        self.recompute_bounds();
    }

    pub fn rect(w: usize, h: usize, a: (f32, f32), b: (f32, f32)) -> Selection {
        let mut s = Selection::empty(w, h);
        let x0 = a.0.min(b.0).max(0.0).round() as usize;
        let y0 = a.1.min(b.1).max(0.0).round() as usize;
        let x1 = (a.0.max(b.0).round().max(0.0) as usize).min(w);
        let y1 = (a.1.max(b.1).round().max(0.0) as usize).min(h);
        for y in y0..y1 {
            s.mask[y * w + x0..y * w + x1.max(x0)].fill(255);
        }
        s.recompute_bounds();
        s
    }

    /// Inside of a closed freehand outline (even-odd rule).
    pub fn lasso(w: usize, h: usize, pts: &[(f32, f32)]) -> Selection {
        let mut s = Selection::empty(w, h);
        if pts.len() < 3 {
            return s;
        }
        let miny = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let maxy = (pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(h);
        let mut xs: Vec<f32> = vec![];
        for y in miny..maxy {
            let fy = y as f32 + 0.5;
            xs.clear();
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                if (a.1 <= fy && b.1 > fy) || (b.1 <= fy && a.1 > fy) {
                    xs.push(a.0 + (fy - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            for pair in xs.chunks(2) {
                if pair.len() == 2 {
                    let x0 = (pair[0].round().max(0.0) as usize).min(w);
                    let x1 = (pair[1].round().max(0.0) as usize).min(w);
                    if x1 > x0 {
                        s.mask[y * w + x0..y * w + x1].fill(255);
                    }
                }
            }
        }
        s.recompute_bounds();
        s
    }

    /// Connected area of similar colour around (x, y) in `px` (tolerance 0..=100).
    pub fn wand(px: &[u8], w: usize, h: usize, x: usize, y: usize, tolerance: f32) -> Selection {
        let mut s = Selection::empty(w, h);
        if x >= w || y >= h {
            return s;
        }
        let seed: [u8; 4] = px[(y * w + x) * 4..(y * w + x) * 4 + 4].try_into().unwrap();
        let tol = (tolerance / 100.0 * 255.0) as i32;
        let near = |i: usize| (0..4).all(|k| (px[i * 4 + k] as i32 - seed[k] as i32).abs() <= tol);
        let mut stack = vec![y * w + x];
        while let Some(i) = stack.pop() {
            if s.mask[i] != 0 || !near(i) {
                continue;
            }
            s.mask[i] = 255;
            let (cx, cy) = (i % w, i / w);
            if cx > 0 {
                stack.push(i - 1);
            }
            if cx + 1 < w {
                stack.push(i + 1);
            }
            if cy > 0 {
                stack.push(i - w);
            }
            if cy + 1 < h {
                stack.push(i + w);
            }
        }
        s.recompute_bounds();
        s
    }

    /// Erase the selected pixels of `px` (made transparent).
    pub fn clear(&self, px: &mut [u8]) {
        for (i, m) in self.mask.iter().enumerate() {
            if *m != 0 {
                px[i * 4..i * 4 + 4].fill(0);
            }
        }
    }

    /// Copy of the selected pixels inside the bounds: (pixels, width, height, x, y).
    pub fn copy(&self, px: &[u8]) -> Option<(Vec<u8>, usize, usize, usize, usize)> {
        let b = self.bounds;
        if b.is_empty() {
            return None;
        }
        let (bw, bh) = (b.x1 - b.x0, b.y1 - b.y0);
        let mut out = vec![0u8; bw * bh * 4];
        for y in 0..bh {
            for x in 0..bw {
                let i = (b.y0 + y) * self.w + b.x0 + x;
                if self.mask[i] != 0 {
                    out[(y * bw + x) * 4..(y * bw + x) * 4 + 4].copy_from_slice(&px[i * 4..i * 4 + 4]);
                }
            }
        }
        Some((out, bw, bh, b.x0, b.y0))
    }
}

/// A piece of picture being moved / scaled / rotated / flipped before it's put down.
pub struct Floating {
    pub id: usize,
    pub px: Vec<u8>,
    pub bw: usize,
    pub bh: usize,
    /// Centre position on the canvas.
    pub cx: f32,
    pub cy: f32,
    pub scale: f32,
    pub angle: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    /// The drawing before anything was lifted (for undo / cancel).
    pub before: Vec<u8>,
    /// The drawing without the lifted piece.
    pub base: Vec<u8>,
}

impl Floating {
    pub fn new(id: usize, piece: (Vec<u8>, usize, usize, usize, usize), before: Vec<u8>, base: Vec<u8>) -> Floating {
        let (px, bw, bh, x, y) = piece;
        Floating {
            id,
            px,
            bw,
            bh,
            cx: x as f32 + bw as f32 / 2.0,
            cy: y as f32 + bh as f32 / 2.0,
            scale: 1.0,
            angle: 0.0,
            flip_x: false,
            flip_y: false,
            before,
            base,
        }
    }

    /// Corners of the piece on the canvas, for drawing its frame.
    pub fn corners(&self) -> [(f32, f32); 4] {
        let (c, s) = (self.angle.to_radians().cos(), self.angle.to_radians().sin());
        let (hw, hh) = (self.bw as f32 / 2.0 * self.scale, self.bh as f32 / 2.0 * self.scale);
        [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|(x, y)| (self.cx + x * c - y * s, self.cy + x * s + y * c))
    }

    /// Draw the piece over `dst` (w×h RGBA). Returns the area it covers.
    pub fn render(&self, dst: &mut [u8], w: usize, h: usize) -> Rect {
        let cs = self.corners();
        let minx = cs.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let miny = cs.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let maxx = (cs.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(w);
        let maxy = (cs.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(h);
        if minx >= maxx || miny >= maxy || self.scale <= 0.0 {
            return Rect::empty();
        }
        let (c, s) = (self.angle.to_radians().cos(), self.angle.to_radians().sin());
        for y in miny..maxy {
            for x in minx..maxx {
                let dx = x as f32 + 0.5 - self.cx;
                let dy = y as f32 + 0.5 - self.cy;
                // back into the piece's own pixels
                let mut u = (dx * c + dy * s) / self.scale + self.bw as f32 / 2.0;
                let mut v = (-dx * s + dy * c) / self.scale + self.bh as f32 / 2.0;
                if self.flip_x {
                    u = self.bw as f32 - u;
                }
                if self.flip_y {
                    v = self.bh as f32 - v;
                }
                let sp = sample(&self.px, self.bw, self.bh, u - 0.5, v - 0.5);
                if sp[3] <= 0.0 {
                    continue;
                }
                let o = &mut dst[(y * w + x) * 4..(y * w + x) * 4 + 4];
                let a = sp[3] / 255.0;
                let ba = o[3] as f32 / 255.0;
                let oa = a + ba * (1.0 - a);
                for k in 0..3 {
                    o[k] = ((sp[k] * a + o[k] as f32 * ba * (1.0 - a)) / oa).round().clamp(0.0, 255.0) as u8;
                }
                o[3] = (oa * 255.0).round() as u8;
            }
        }
        Rect {
            x0: minx,
            y0: miny,
            x1: maxx,
            y1: maxy,
        }
    }
}

/// Bilinear sample with premultiplied blending, returns straight RGBA (0..255 floats).
fn sample(px: &[u8], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    if x < -1.0 || y < -1.0 || x > w as f32 || y > h as f32 {
        return [0.0; 4];
    }
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let mut acc = [0f32; 4];
    for (xi, yi, wgt) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x0 + 1, y0, fx * (1.0 - fy)),
        (x0, y0 + 1, (1.0 - fx) * fy),
        (x0 + 1, y0 + 1, fx * fy),
    ] {
        if xi < 0 || yi < 0 || xi >= w as i32 || yi >= h as i32 || wgt <= 0.0 {
            continue;
        }
        let i = (yi as usize * w + xi as usize) * 4;
        let a = px[i + 3] as f32;
        for k in 0..3 {
            acc[k] += px[i + k] as f32 * a * wgt;
        }
        acc[3] += a * wgt;
    }
    if acc[3] > 0.0 {
        for k in 0..3 {
            acc[k] /= acc[3];
        }
    }
    acc
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GradientKind {
    Linear,
    Radial,
}

/// Paint a gradient from colour `ca` at `a` to colour `cb` at `b` over `px`, only where `sel` allows
/// (everywhere without a selection). `mid` is where the halfway colour sits (0.5 = middle, the
/// original's default stops at 0 and 1 with midpoint 0.5). Returns the changed area.
#[allow(clippy::too_many_arguments)]
pub fn gradient(
    px: &mut [u8],
    w: usize,
    h: usize,
    a: (f32, f32),
    b: (f32, f32),
    ca: [u8; 3],
    cb: [u8; 3],
    kind: GradientKind,
    mid: f32,
    opacity: f32,
    sel: Option<&Selection>,
) -> Rect {
    let area = match sel {
        Some(s) if !s.is_empty() => s.bounds,
        _ => Rect::full(w, h),
    };
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = (dx * dx + dy * dy).max(1e-6);
    let len = len2.sqrt();
    let mid = mid.clamp(0.01, 0.99);
    let gamma = (0.5f32).ln() / mid.ln();
    let op = (opacity / 100.0).clamp(0.0, 1.0);
    for y in area.y0..area.y1 {
        for x in area.x0..area.x1 {
            if let Some(s) = sel {
                if !s.is_empty() && s.mask[y * w + x] == 0 {
                    continue;
                }
            }
            let (px_, py_) = (x as f32 + 0.5 - a.0, y as f32 + 0.5 - a.1);
            let t = match kind {
                GradientKind::Linear => (px_ * dx + py_ * dy) / len2,
                GradientKind::Radial => (px_ * px_ + py_ * py_).sqrt() / len,
            }
            .clamp(0.0, 1.0)
            .powf(gamma);
            let o = &mut px[(y * w + x) * 4..(y * w + x) * 4 + 4];
            let ba = o[3] as f32 / 255.0;
            let oa = op + ba * (1.0 - op);
            for k in 0..3 {
                let c = ca[k] as f32 + (cb[k] as f32 - ca[k] as f32) * t;
                o[k] = ((c * op + o[k] as f32 * ba * (1.0 - op)) / oa.max(1e-6)).round() as u8;
            }
            o[3] = (oa * 255.0).round() as u8;
        }
    }
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_lasso_wand() {
        let r = Selection::rect(100, 100, (10.0, 10.0), (30.0, 20.0));
        assert_eq!(
            r.bounds,
            Rect {
                x0: 10,
                y0: 10,
                x1: 30,
                y1: 20
            }
        );
        let l = Selection::lasso(100, 100, &[(10.0, 10.0), (90.0, 10.0), (50.0, 90.0)]);
        assert!(l.contains(50, 30) && !l.contains(5, 5));
        let mut px = vec![0u8; 100 * 100 * 4];
        for y in 0..100 {
            for x in 0..50 {
                px[(y * 100 + x) * 4..(y * 100 + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let wnd = Selection::wand(&px, 100, 100, 10, 10, 10.0);
        assert_eq!(
            wnd.bounds,
            Rect {
                x0: 0,
                y0: 0,
                x1: 50,
                y1: 100
            }
        );
        let mut s = r.clone();
        s.combine(&Selection::rect(100, 100, (20.0, 10.0), (30.0, 20.0)), Combine::Subtract);
        assert_eq!(s.bounds.x1, 20);
    }

    #[test]
    fn floating_moves_and_flips() {
        let (w, h) = (40, 20);
        let mut px = vec![0u8; w * h * 4];
        px[(5 * w + 2) * 4..(5 * w + 2) * 4 + 4].copy_from_slice(&[0, 255, 0, 255]);
        let sel = Selection::rect(w, h, (0.0, 0.0), (10.0, 10.0));
        let piece = sel.copy(&px).unwrap();
        let mut base = px.clone();
        sel.clear(&mut base);
        let mut f = Floating::new(0, piece, px.clone(), base.clone());
        f.cx += 20.0;
        let mut out = base.clone();
        f.render(&mut out, w, h);
        assert!(out[(5 * w + 22) * 4 + 1] > 200, "moved by 20 px");
        f.flip_x = true;
        let mut out = base.clone();
        f.render(&mut out, w, h);
        assert!(out[(5 * w + 27) * 4 + 1] > 200, "flipped inside its box");
    }

    #[test]
    fn gradient_goes_from_a_to_b() {
        let (w, h) = (101, 1);
        let mut px = vec![0u8; w * h * 4];
        gradient(
            &mut px,
            w,
            h,
            (0.0, 0.0),
            (101.0, 0.0),
            [0, 0, 0],
            [200, 100, 0],
            GradientKind::Linear,
            0.5,
            100.0,
            None,
        );
        assert!(px[0] < 5);
        assert!((px[50 * 4] as i32 - 100).abs() <= 3);
        assert!(px[100 * 4] > 195);
    }
}
