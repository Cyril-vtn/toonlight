//! Custom brushes (a picture stamped along the stroke) and saved tool presets.
//!
//! Like the reference's custom brush tool (tooltype 5 in its `cb_*.ini` files: width, height,
//! angle, flipx, flipy, opacity, stepval=12.50, step = stepval % of the brush's largest side), and
//! its tool panel buttons ("CTool:" for a tool preset, "CBTool:" for a custom brush, saved as
//! `cb_<panel>_<n>`).

use crate::brush::{Settings, Tool};
use crate::doc::Rect;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// The picture a custom brush stamps (straight RGBA).
#[derive(Clone)]
pub struct CustomBrush {
    pub px: Vec<u8>,
    pub w: usize,
    pub h: usize,
}

impl CustomBrush {
    /// Trim fully transparent borders.
    pub fn trimmed(px: Vec<u8>, w: usize, h: usize) -> Option<CustomBrush> {
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        for y in 0..h {
            for x in 0..w {
                if px[(y * w + x) * 4 + 3] > 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x + 1);
                    y1 = y1.max(y + 1);
                }
            }
        }
        if x0 >= x1 || y0 >= y1 {
            return None;
        }
        let (bw, bh) = (x1 - x0, y1 - y0);
        let mut out = vec![0u8; bw * bh * 4];
        for y in 0..bh {
            out[y * bw * 4..(y + 1) * bw * 4].copy_from_slice(&px[((y0 + y) * w + x0) * 4..((y0 + y) * w + x1) * 4]);
        }
        Some(CustomBrush { px: out, w: bw, h: bh })
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct CustomSettings {
    /// Size in percent of the picture.
    pub scale: f32,
    /// Degrees ("angle=0.00").
    pub angle: f32,
    /// Percent ("opacity=100").
    pub opacity: f32,
    /// Spacing in percent of the brush's largest side ("stepval=12.50").
    pub step: f32,
    /// Use the current colour instead of the picture's colours (keeps its shape).
    pub tint: bool,
    pub flip_x: bool,
    pub flip_y: bool,
}

impl Default for CustomSettings {
    fn default() -> Self {
        CustomSettings {
            scale: 100.0,
            angle: 0.0,
            opacity: 100.0,
            step: 12.5,
            tint: false,
            flip_x: false,
            flip_y: false,
        }
    }
}

/// Distance between stamps: StepVal % of the brush's largest side (in the shipped cb_12164.ini,
/// 12.50 % of 127 px gives step=15.88).
pub fn custom_spacing(b: &CustomBrush, s: &CustomSettings) -> f32 {
    (s.step * 0.01 * b.w.max(b.h) as f32 * s.scale / 100.0).max(1.0)
}

pub struct CustomStroke {
    pub id: usize,
    pub before: Vec<u8>,
    pub dirty: Rect,
    last: Option<(f32, f32)>,
    carry: f32,
    brush: Arc<CustomBrush>,
    s: CustomSettings,
    color: [u8; 3],
    clip: Option<Arc<Vec<u8>>>,
    w: usize,
    h: usize,
}

impl CustomStroke {
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        id: usize,
        before: &[u8],
        w: usize,
        h: usize,
        brush: Arc<CustomBrush>,
        s: CustomSettings,
        color: [u8; 3],
        clip: Option<Arc<Vec<u8>>>,
    ) -> CustomStroke {
        CustomStroke {
            id,
            before: before.to_vec(),
            dirty: Rect::empty(),
            last: None,
            carry: 0.0,
            brush,
            s,
            color,
            clip,
            w,
            h,
        }
    }

    pub fn to(&mut self, x: f32, y: f32, px: &mut [u8]) -> Rect {
        let step = custom_spacing(&self.brush, &self.s);
        let mut changed = Rect::empty();
        match self.last {
            None => changed = self.stamp(x, y, px),
            Some((lx, ly)) => {
                let (dx, dy) = (x - lx, y - ly);
                let len = (dx * dx + dy * dy).sqrt();
                let mut t = step - self.carry;
                if len > 0.0 {
                    while t <= len {
                        let r = self.stamp(lx + dx * t / len, ly + dy * t / len, px);
                        changed = changed.union(&r);
                        t += step;
                    }
                    self.carry = len - (t - step);
                }
            }
        }
        self.last = Some((x, y));
        self.dirty = self.dirty.union(&changed);
        changed
    }

    fn stamp(&mut self, cx: f32, cy: f32, px: &mut [u8]) -> Rect {
        let b = &self.brush;
        let sc = (self.s.scale / 100.0).max(0.01);
        let (c, s) = (self.s.angle.to_radians().cos(), self.s.angle.to_radians().sin());
        let (hw, hh) = (b.w as f32 / 2.0 * sc, b.h as f32 / 2.0 * sc);
        let r = (hw * hw + hh * hh).sqrt();
        let x0 = (cx - r).floor().max(0.0) as usize;
        let y0 = (cy - r).floor().max(0.0) as usize;
        let x1 = ((cx + r).ceil().max(0.0) as usize).min(self.w);
        let y1 = ((cy + r).ceil().max(0.0) as usize).min(self.h);
        if x0 >= x1 || y0 >= y1 {
            return Rect::empty();
        }
        let op = (self.s.opacity / 100.0).clamp(0.0, 1.0);
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * self.w + x;
                if let Some(cl) = &self.clip {
                    if cl[i] == 0 {
                        continue;
                    }
                }
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let mut u = (dx * c + dy * s) / sc + b.w as f32 / 2.0;
                let mut v = (-dx * s + dy * c) / sc + b.h as f32 / 2.0;
                if self.s.flip_x {
                    u = b.w as f32 - u;
                }
                if self.s.flip_y {
                    v = b.h as f32 - v;
                }
                if u < 0.0 || v < 0.0 || u >= b.w as f32 || v >= b.h as f32 {
                    continue;
                }
                let sp = &b.px[(v as usize * b.w + u as usize) * 4..(v as usize * b.w + u as usize) * 4 + 4];
                let a = sp[3] as f32 / 255.0 * op;
                if a <= 0.0 {
                    continue;
                }
                let col = if self.s.tint { self.color } else { [sp[0], sp[1], sp[2]] };
                let o = &mut px[i * 4..i * 4 + 4];
                let ba = o[3] as f32 / 255.0;
                let oa = a + ba * (1.0 - a);
                for k in 0..3 {
                    o[k] = ((col[k] as f32 * a + o[k] as f32 * ba * (1.0 - a)) / oa).round() as u8;
                }
                o[3] = (oa * 255.0).round() as u8;
            }
        }
        Rect { x0, y0, x1, y1 }
    }
}

/// A saved tool setting, shown as a button.
#[derive(Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub tool: Tool,
    pub settings: Settings,
    #[serde(default)]
    pub custom: Option<CustomSettings>,
    /// PNG file of a custom brush, in the presets folder.
    #[serde(default)]
    pub image: Option<String>,
}

pub fn presets_dir() -> Option<PathBuf> {
    crate::i18n::config_dir().map(|d| d.join("presets"))
}

/// The presets shipped at first start: the size bins of the reference's Data/defaulttools.conf
/// (airbrush 10-150, pen 20-80, wet brush 20-80, oil brush 30, pencil 30).
pub fn default_presets() -> Vec<Preset> {
    let mut out = vec![];
    let mut add = |tool: Tool, sizes: &[f32]| {
        for r in sizes {
            let mut s = tool.defaults();
            s.radius = *r;
            out.push(Preset {
                name: format!("{} {r:.0}", tool.label()),
                tool,
                settings: s,
                custom: None,
                image: None,
            });
        }
    };
    add(Tool::Pencil, &[30.0]);
    add(Tool::Pen, &[20.0, 40.0, 60.0, 80.0]);
    add(Tool::Airbrush, &[10.0, 20.0, 30.0, 40.0, 50.0, 75.0, 100.0, 150.0]);
    add(Tool::Oil, &[30.0]);
    add(Tool::Wet, &[20.0, 40.0, 60.0, 80.0]);
    out
}

pub fn load_presets() -> Vec<Preset> {
    presets_dir()
        .and_then(|d| std::fs::read(d.join("presets.json")).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(default_presets)
}

pub fn save_presets(list: &[Preset]) {
    if let Some(d) = presets_dir() {
        let _ = std::fs::create_dir_all(&d);
        if let Ok(b) = serde_json::to_vec_pretty(list) {
            let _ = std::fs::write(d.join("presets.json"), b);
        }
    }
}

/// Save a custom brush picture next to the presets; returns its file name.
pub fn save_brush_image(b: &CustomBrush) -> Option<String> {
    let d = presets_dir()?;
    std::fs::create_dir_all(&d).ok()?;
    let name = format!(
        "cb_{}.png",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_millis()
    );
    crate::io::save_png(&d.join(&name), &b.px, b.w, b.h).ok()?;
    Some(name)
}

pub fn load_brush_image(name: &str) -> Option<CustomBrush> {
    let p = presets_dir()?.join(name);
    let (px, w, h) = crate::io::load_picture(&p).ok()?;
    Some(CustomBrush { px, w, h })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_matches_the_shipped_brush() {
        // cb_12164.ini: width=126 height=127 stepval=12.50 step=15.88
        let b = CustomBrush {
            px: vec![255; 126 * 127 * 4],
            w: 126,
            h: 127,
        };
        let s = custom_spacing(&b, &CustomSettings::default());
        assert!((s - 15.875).abs() < 0.01);
    }

    #[test]
    fn stamps_along_a_line() {
        let mut dot = vec![0u8; 5 * 5 * 4];
        for i in 0..25 {
            dot[i * 4..i * 4 + 4].copy_from_slice(&[0, 0, 255, 255]);
        }
        let b = Arc::new(CustomBrush::trimmed(dot, 5, 5).unwrap());
        let (w, h) = (100, 20);
        let mut px = vec![0u8; w * h * 4];
        let mut st = CustomStroke::begin(0, &px, w, h, b, CustomSettings::default(), [0, 0, 0], None);
        st.to(10.0, 10.0, &mut px);
        st.to(90.0, 10.0, &mut px);
        assert!(px[(10 * w + 50) * 4 + 2] > 200, "stamped in the middle of the line");
        assert_eq!(px[(2 * w + 50) * 4 + 3], 0, "nothing far from the line");
    }
}
