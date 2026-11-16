//! Text tool: writes a line (or several) of text into a drawing, with border, italic, rotation,
//! letter spacing and opacity. Settings follow the original's text tool (the reference reads mode,
//! size, opacity, outline, smooth, border, border_col, glow, aspect, space, angle, italic, track,
//! font…; defaults in Data/defaulttools.conf [tooltype 9-1]).

use crate::doc::Rect;
use ab_glyph::{Font, FontVec, PxScale, ScaleFont};

#[derive(Clone, Debug)]
pub struct TextSettings {
    pub text: String,
    /// Letter height in pixels ("size=100.00").
    pub size: f32,
    /// Border around the letters ("borderon=1", "border=2", "bordercolor=255 0 0").
    pub border_on: bool,
    pub border: f32,
    pub border_color: [u8; 3],
    /// Slant in percent of the height ("italic=50.00", "italicon=0").
    pub italic_on: bool,
    pub italic: f32,
    /// Rotation in degrees ("rotate=0.00", "rotateon=0").
    pub rotate_on: bool,
    pub rotate: f32,
    /// Letter spacing in percent of normal ("track=100.00", "trackon=0").
    pub track_on: bool,
    pub track: f32,
    /// Opacity in percent ("opacity=50", "opacityon=0"; off means 100 %).
    pub opacity_on: bool,
    pub opacity: f32,
    /// Soft glow around the letters ("glowon=0", "glow=8", "glowcolor=0 255 0").
    pub glow_on: bool,
    pub glow: f32,
    pub glow_color: [u8; 3],
    /// Hollow letters: only an outline this wide ("outlineon=0", "outline=2").
    pub outline_on: bool,
    pub outline: f32,
}

impl Default for TextSettings {
    fn default() -> Self {
        TextSettings {
            text: "Texte".into(),
            size: 100.0,
            border_on: true,
            border: 2.0,
            border_color: [255, 0, 0],
            italic_on: false,
            italic: 50.0,
            rotate_on: false,
            rotate: 0.0,
            track_on: false,
            track: 100.0,
            opacity_on: false,
            opacity: 50.0,
            glow_on: false,
            glow: 8.0,
            glow_color: [0, 255, 0],
            outline_on: false,
            outline: 2.0,
        }
    }
}

/// The font the text tool uses: the interface's own font (shipped inside the program).
pub fn default_font() -> Option<FontVec> {
    let defs = eframe::egui::FontDefinitions::default();
    let fam = defs.families.get(&eframe::egui::FontFamily::Proportional)?;
    let name = fam.first()?;
    let data = defs.font_data.get(name)?;
    FontVec::try_from_vec_and_index(data.font.to_vec(), data.index).ok()
}

/// A font from a file (for Chinese / Japanese text, for example).
pub fn font_from_file(path: &std::path::Path, index: u32) -> Option<FontVec> {
    FontVec::try_from_vec_and_index(std::fs::read(path).ok()?, index).ok()
}

/// Coverage mask of the text (unrotated), with its size and the baseline of the first line.
fn layout_mask(font: &FontVec, s: &TextSettings) -> (Vec<f32>, usize, usize) {
    let scale = PxScale::from(s.size.max(4.0));
    let sf = font.as_scaled(scale);
    let track = if s.track_on { s.track / 100.0 } else { 1.0 };
    let line_h = sf.height() + sf.line_gap();
    let lines: Vec<&str> = s.text.lines().collect();
    let lines = if lines.is_empty() { vec![""] } else { lines };
    let mut width = 0f32;
    for l in &lines {
        let mut x = 0f32;
        for c in l.chars() {
            x += sf.h_advance(font.glyph_id(c)) * track;
        }
        width = width.max(x);
    }
    let pad = 2usize;
    let w = (width.ceil() as usize + pad * 2).max(1);
    let h = ((line_h * lines.len() as f32).ceil() as usize + pad * 2).max(1);
    let mut mask = vec![0f32; w * h];
    for (li, l) in lines.iter().enumerate() {
        let mut x = pad as f32;
        let base = pad as f32 + sf.ascent() + li as f32 * line_h;
        let mut prev: Option<ab_glyph::GlyphId> = None;
        for c in l.chars() {
            let id = font.glyph_id(c);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            let g = id.with_scale_and_position(scale, ab_glyph::point(x, base));
            if let Some(og) = font.outline_glyph(g) {
                let b = og.px_bounds();
                og.draw(|gx, gy, v| {
                    let px = b.min.x as i32 + gx as i32;
                    let py = b.min.y as i32 + gy as i32;
                    if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                        let m = &mut mask[py as usize * w + px as usize];
                        *m = m.max(v);
                    }
                });
            }
            x += sf.h_advance(id) * track;
            prev = Some(id);
        }
    }
    (mask, w, h)
}

fn pad_mask(mask: &[f32], w: usize, h: usize, pad: usize) -> (Vec<f32>, usize, usize) {
    let (nw, nh) = (w + pad * 2, h + pad * 2);
    let mut out = vec![0f32; nw * nh];
    for y in 0..h {
        out[(y + pad) * nw + pad..(y + pad) * nw + pad + w].copy_from_slice(&mask[y * w..(y + 1) * w]);
    }
    (out, nw, nh)
}

/// Like `dilate`, fading out towards the radius (for the glow).
fn soft_dilate(mask: &[f32], w: usize, h: usize, r: f32) -> Vec<f32> {
    let ri = r.ceil() as i32;
    let mut out = vec![0f32; w * h];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut m = 0f32;
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    let d = ((dx * dx + dy * dy) as f32).sqrt();
                    if d > r {
                        continue;
                    }
                    let (sx, sy) = (x + dx, y + dy);
                    if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 {
                        let f = 1.0 - d / r;
                        m = m.max(mask[sy as usize * w + sx as usize] * f * f);
                    }
                }
            }
            out[y as usize * w + x as usize] = m;
        }
    }
    out
}

fn dilate(mask: &[f32], w: usize, h: usize, r: f32) -> Vec<f32> {
    let ri = r.ceil() as i32;
    let mut out = vec![0f32; w * h];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut m = 0f32;
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    let d = ((dx * dx + dy * dy) as f32).sqrt();
                    if d > r + 0.5 {
                        continue;
                    }
                    let (sx, sy) = (x + dx, y + dy);
                    if sx >= 0 && sy >= 0 && sx < w as i32 && sy < h as i32 {
                        let edge = (r + 0.5 - d).clamp(0.0, 1.0);
                        m = m.max(mask[sy as usize * w + sx as usize] * edge);
                    }
                }
            }
            out[y as usize * w + x as usize] = m;
        }
    }
    out
}

fn sample(mask: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    if x < -1.0 || y < -1.0 || x > w as f32 || y > h as f32 {
        return 0.0;
    }
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |xi: i32, yi: i32| -> f32 {
        if xi < 0 || yi < 0 || xi >= w as i32 || yi >= h as i32 {
            0.0
        } else {
            mask[yi as usize * w + xi as usize]
        }
    };
    let (xi, yi) = (x0 as i32, y0 as i32);
    let a = at(xi, yi) + (at(xi + 1, yi) - at(xi, yi)) * fx;
    let b = at(xi, yi + 1) + (at(xi + 1, yi + 1) - at(xi, yi + 1)) * fx;
    a + (b - a) * fy
}

/// Write the text with its top-left corner at (x, y) into `px` (w×h RGBA). Returns the changed area.
pub fn stamp(px: &mut [u8], w: usize, h: usize, x: f32, y: f32, color: [u8; 3], s: &TextSettings, font: &FontVec) -> Rect {
    let (mask, mw, mh) = layout_mask(font, s);
    // room around the letters for the border and the glow
    let pad = (if s.border_on { s.border } else { 0.0 } + if s.glow_on { s.glow } else { 0.0 }).ceil() as usize + 1;
    let (mask, mw, mh) = pad_mask(&mask, mw, mh, pad);
    let x = x - pad as f32;
    let y = y - pad as f32;
    let mask = if s.outline_on && s.outline > 0.0 {
        // keep only a band of `outline` pixels inside the letters
        let inv: Vec<f32> = mask.iter().map(|v| 1.0 - v).collect();
        let grown = dilate(&inv, mw, mh, s.outline);
        mask.iter().zip(&grown).map(|(m, g)| m.min(*g)).collect()
    } else {
        mask
    };
    let border = if s.border_on && s.border > 0.0 {
        Some(dilate(&mask, mw, mh, s.border))
    } else {
        None
    };
    let glow = if s.glow_on && s.glow > 0.0 {
        Some(soft_dilate(&mask, mw, mh, s.glow + if s.border_on { s.border } else { 0.0 }))
    } else {
        None
    };
    let angle = if s.rotate_on { s.rotate.to_radians() } else { 0.0 };
    let slant = if s.italic_on { s.italic / 100.0 * 0.5 } else { 0.0 };
    let opacity = if s.opacity_on { s.opacity / 100.0 } else { 1.0 };
    let (ca, sa) = (angle.cos(), angle.sin());
    // destination bounds: transform the mask corners
    let corners = [(0.0, 0.0), (mw as f32, 0.0), (0.0, mh as f32), (mw as f32, mh as f32)];
    let fwd = |u: f32, v: f32| {
        let u2 = u + slant * (mh as f32 - v);
        (x + u2 * ca - v * sa, y + u2 * sa + v * ca)
    };
    let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (u, v) in corners {
        let (dx, dy) = fwd(u, v);
        minx = minx.min(dx);
        miny = miny.min(dy);
        maxx = maxx.max(dx);
        maxy = maxy.max(dy);
    }
    let x0 = (minx.floor() - 2.0).max(0.0) as usize;
    let y0 = (miny.floor() - 2.0).max(0.0) as usize;
    let x1 = ((maxx.ceil() + 2.0).max(0.0) as usize).min(w);
    let y1 = ((maxy.ceil() + 2.0).max(0.0) as usize).min(h);
    if x0 >= x1 || y0 >= y1 {
        return Rect::empty();
    }
    for py in y0..y1 {
        for pxx in x0..x1 {
            // inverse transform: rotate back, then undo the slant
            let dx = pxx as f32 + 0.5 - x;
            let dy = py as f32 + 0.5 - y;
            let u2 = dx * ca + dy * sa;
            let v = -dx * sa + dy * ca;
            let u = u2 - slant * (mh as f32 - v);
            let t = sample(&mask, mw, mh, u - 0.5, v - 0.5);
            let b = border.as_ref().map(|b| sample(b, mw, mh, u - 0.5, v - 0.5)).unwrap_or(0.0);
            let gl = glow.as_ref().map(|g| sample(g, mw, mh, u - 0.5, v - 0.5)).unwrap_or(0.0);
            if t <= 0.0 && b <= 0.0 && gl <= 0.0 {
                continue;
            }
            let o = &mut px[(py * w + pxx) * 4..(py * w + pxx) * 4 + 4];
            for (cov, col) in [(gl, s.glow_color), (b, s.border_color), (t, color)] {
                let a = (cov * opacity).clamp(0.0, 1.0);
                if a <= 0.0 {
                    continue;
                }
                let ba = o[3] as f32 / 255.0;
                let oa = a + ba * (1.0 - a);
                for k in 0..3 {
                    o[k] = ((col[k] as f32 * a + o[k] as f32 * ba * (1.0 - a)) / oa).round() as u8;
                }
                o[3] = (oa * 255.0).round() as u8;
            }
        }
    }
    Rect { x0, y0, x1, y1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_writes_pixels() {
        let font = default_font().expect("built-in font");
        let (w, h) = (300, 120);
        let mut px = vec![0u8; w * h * 4];
        let s = TextSettings {
            text: "Abc".into(),
            size: 60.0,
            ..Default::default()
        };
        let r = stamp(&mut px, w, h, 10.0, 10.0, [0, 0, 255], &s, &font);
        assert!(!r.is_empty());
        let blue = px.chunks(4).filter(|p| p[2] > 200 && p[3] > 200 && p[0] < 50).count();
        let red = px.chunks(4).filter(|p| p[0] > 200 && p[3] > 100 && p[2] < 50).count();
        assert!(blue > 100, "letters drawn");
        assert!(red > 50, "border drawn");
    }
}
