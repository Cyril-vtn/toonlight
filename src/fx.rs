//! Effects (FX) applied to a drawing: blur, mosaic, noise, wave, brightness / contrast,
//! hue / saturation, invert, and colour / luma keys. Each effect only changes the pixels the
//! selection allows (all of them without a selection).
//!
//! The reference's script commands give the arguments: gblur (x radius, y radius; y = x when 0), mosaic (x size, y size, flag), noise (2 values),
//! wave (6 values, one in percent). Its gaussian clamps
//! the radius to the picture size − 2 and weights a pixel at distance d by exp(−ln 128 · (d / r)²),
//! i.e. 1/128 at the radius, in 1/256 steps.

use crate::select::Selection;

#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Effect {
    Blur {
        rx: f32,
        ry: f32,
    },
    Mosaic {
        sx: u32,
        sy: u32,
    },
    Noise {
        amount: f32,
        mono: bool,
    },
    Wave {
        amp_x: f32,
        len_x: f32,
        amp_y: f32,
        len_y: f32,
        phase: f32,
    },
    BrightnessContrast {
        brightness: f32,
        contrast: f32,
    },
    HueSaturation {
        hue: f32,
        saturation: f32,
        lightness: f32,
    },
    Invert,
    ColorKey {
        color: [u8; 3],
        tolerance: f32,
    },
    LumaKey {
        threshold: f32,
        dark: bool,
    },
}

pub const ALL_EFFECTS: [Effect; 9] = [
    Effect::Blur { rx: 8.0, ry: 0.0 },
    Effect::Mosaic { sx: 16, sy: 16 },
    Effect::Noise { amount: 20.0, mono: true },
    Effect::Wave {
        amp_x: 10.0,
        len_x: 80.0,
        amp_y: 0.0,
        len_y: 80.0,
        phase: 0.0,
    },
    Effect::BrightnessContrast {
        brightness: 0.0,
        contrast: 0.0,
    },
    Effect::HueSaturation {
        hue: 0.0,
        saturation: 0.0,
        lightness: 0.0,
    },
    Effect::Invert,
    Effect::ColorKey {
        color: [0, 255, 0],
        tolerance: 20.0,
    },
    Effect::LumaKey {
        threshold: 50.0,
        dark: false,
    },
];

impl Effect {
    pub fn label(&self) -> &'static str {
        use crate::i18n::tr;
        match self {
            Effect::Blur { .. } => tr("Flou"),
            Effect::Mosaic { .. } => tr("Mosaïque"),
            Effect::Noise { .. } => tr("Bruit"),
            Effect::Wave { .. } => tr("Vague"),
            Effect::BrightnessContrast { .. } => tr("Luminosité / contraste"),
            Effect::HueSaturation { .. } => tr("Teinte / saturation"),
            Effect::Invert => tr("Négatif"),
            Effect::ColorKey { .. } => tr("Incrustation couleur"),
            Effect::LumaKey { .. } => tr("Incrustation luminance"),
        }
    }

    pub fn same_kind(&self, o: &Effect) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(o)
    }
}

impl Effect {
    /// Between two settings of the same effect, `t` of the way (0..=1); numbers move in a
    /// straight line, switches and colours take the first one's value until the end.
    pub fn lerp(&self, b: &Effect, t: f32) -> Effect {
        let l = |x: f32, y: f32| x + (y - x) * t;
        let li = |x: u32, y: u32| (x as f32 + (y as f32 - x as f32) * t).round().max(1.0) as u32;
        match (*self, *b) {
            (Effect::Blur { rx, ry }, Effect::Blur { rx: bx, ry: by }) => Effect::Blur {
                rx: l(rx, bx),
                ry: l(ry, by),
            },
            (Effect::Mosaic { sx, sy }, Effect::Mosaic { sx: bx, sy: by }) => Effect::Mosaic {
                sx: li(sx, bx),
                sy: li(sy, by),
            },
            (Effect::Noise { amount, mono }, Effect::Noise { amount: ba, .. }) => Effect::Noise {
                amount: l(amount, ba),
                mono,
            },
            (
                Effect::Wave {
                    amp_x,
                    len_x,
                    amp_y,
                    len_y,
                    phase,
                },
                Effect::Wave {
                    amp_x: a2,
                    len_x: l2,
                    amp_y: a3,
                    len_y: l3,
                    phase: p2,
                },
            ) => Effect::Wave {
                amp_x: l(amp_x, a2),
                len_x: l(len_x, l2),
                amp_y: l(amp_y, a3),
                len_y: l(len_y, l3),
                phase: l(phase, p2),
            },
            (
                Effect::BrightnessContrast { brightness, contrast },
                Effect::BrightnessContrast {
                    brightness: b2,
                    contrast: c2,
                },
            ) => Effect::BrightnessContrast {
                brightness: l(brightness, b2),
                contrast: l(contrast, c2),
            },
            (
                Effect::HueSaturation {
                    hue,
                    saturation,
                    lightness,
                },
                Effect::HueSaturation {
                    hue: h2,
                    saturation: s2,
                    lightness: l2,
                },
            ) => Effect::HueSaturation {
                hue: l(hue, h2),
                saturation: l(saturation, s2),
                lightness: l(lightness, l2),
            },
            (Effect::ColorKey { color, tolerance }, Effect::ColorKey { tolerance: t2, .. }) => Effect::ColorKey {
                color,
                tolerance: l(tolerance, t2),
            },
            (Effect::LumaKey { threshold, dark }, Effect::LumaKey { threshold: t2, .. }) => Effect::LumaKey {
                threshold: l(threshold, t2),
                dark,
            },
            (a, _) => a,
        }
    }
}

/// A saved effect stack (a preset): a name and the effects with their keys, in order.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FxPreset {
    pub name: String,
    pub effects: Vec<FxEntry>,
}

/// An effect stack as a file.
#[derive(serde::Serialize, serde::Deserialize)]
struct FxFile {
    format: String,
    version: u32,
    #[serde(default)]
    name: String,
    effects: Vec<FxEntry>,
}

const FX_FILE: &str = "toonlight-fx-stack";

/// An effect stack as the text of a file (JSON).
pub fn stack_to_json(name: &str, effects: &[FxEntry]) -> Result<String, String> {
    let f = FxFile {
        format: FX_FILE.into(),
        version: 1,
        name: name.to_string(),
        effects: effects.to_vec(),
    };
    serde_json::to_string_pretty(&f).map_err(|e| e.to_string())
}

/// Read an effect stack written by `stack_to_json`.
pub fn stack_from_json(text: &str) -> Result<FxPreset, String> {
    let f: FxFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if f.format != FX_FILE {
        return Err(crate::i18n::tr("Ce fichier n'est pas une pile d'effets.").into());
    }
    Ok(FxPreset {
        name: f.name,
        effects: f.effects,
    })
}

fn presets_path() -> Option<std::path::PathBuf> {
    crate::i18n::config_dir().map(|d| d.join("fx-presets.json"))
}

/// The effect stacks saved as presets, kept in the settings folder.
pub fn load_presets() -> Vec<FxPreset> {
    presets_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save_presets(presets: &[FxPreset]) {
    if let Some(p) = presets_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(b) = serde_json::to_vec_pretty(presets) {
            let _ = std::fs::write(p, b);
        }
    }
}

/// One effect of a layer's stack: its settings, keys (frame → settings) that animate it, and
/// whether it is on.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FxEntry {
    pub effect: Effect,
    #[serde(default)]
    pub keys: Vec<(usize, Effect)>,
    pub on: bool,
}

impl FxEntry {
    pub fn new(effect: Effect) -> FxEntry {
        FxEntry {
            effect,
            keys: vec![],
            on: true,
        }
    }

    /// The settings at a frame: the keys' values, in a straight line between two keys, held
    /// before the first and after the last; the plain settings without keys.
    pub fn at(&self, frame: usize) -> Effect {
        let mut keys = self.keys.clone();
        keys.sort_by_key(|k| k.0);
        match (keys.iter().rev().find(|k| k.0 <= frame), keys.iter().find(|k| k.0 >= frame)) {
            (None, None) => self.effect,
            (Some(a), None) => a.1,
            (None, Some(b)) => b.1,
            (Some(a), Some(b)) if a.0 == b.0 => a.1,
            (Some(a), Some(b)) => a.1.lerp(&b.1, (frame - a.0) as f32 / (b.0 - a.0) as f32),
        }
    }

    /// Set the settings at a frame: the key there (made if needed) when the effect has keys,
    /// else the plain settings.
    pub fn set_at(&mut self, frame: usize, fx: Effect) {
        if self.keys.is_empty() {
            self.effect = fx;
        } else if let Some(k) = self.keys.iter_mut().find(|k| k.0 == frame) {
            k.1 = fx;
        } else {
            self.keys.push((frame, fx));
            self.keys.sort_by_key(|k| k.0);
        }
    }
}

/// A drawing through a layer's effects, in order, at a frame.
pub fn run_stack(stack: &[FxEntry], frame: usize, src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut cur = src.to_vec();
    for e in stack.iter().filter(|e| e.on) {
        let mut out = cur.clone();
        apply(&e.at(frame), &cur, &mut out, w, h, None);
        cur = out;
    }
    cur
}

fn allowed(sel: Option<&Selection>, i: usize) -> bool {
    match sel {
        Some(s) if !s.is_empty() => s.mask[i] != 0,
        _ => true,
    }
}

/// Apply `fx` to `px` (w×h straight RGBA), reading from `src` (the drawing before the effect).
pub fn apply(fx: &Effect, src: &[u8], px: &mut [u8], w: usize, h: usize, sel: Option<&Selection>) {
    match *fx {
        Effect::Blur { rx, ry } => {
            let ry = if ry <= 0.0 { rx } else { ry };
            let out = blur(src, w, h, rx, ry);
            put(&out, px, sel);
        }
        Effect::Mosaic { sx, sy } => {
            let (sx, sy) = (sx.max(1) as usize, sy.max(1) as usize);
            let mut out = src.to_vec();
            for by in (0..h).step_by(sy) {
                for bx in (0..w).step_by(sx) {
                    let (mut acc, mut n) = ([0f32; 4], 0f32);
                    for y in by..(by + sy).min(h) {
                        for x in bx..(bx + sx).min(w) {
                            let i = (y * w + x) * 4;
                            let a = src[i + 3] as f32;
                            for k in 0..3 {
                                acc[k] += src[i + k] as f32 * a;
                            }
                            acc[3] += a;
                            n += 1.0;
                        }
                    }
                    let c = if acc[3] > 0.0 {
                        [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3]]
                    } else {
                        [0.0; 3]
                    };
                    let a = acc[3] / n;
                    for y in by..(by + sy).min(h) {
                        for x in bx..(bx + sx).min(w) {
                            let i = (y * w + x) * 4;
                            out[i..i + 4].copy_from_slice(&[c[0] as u8, c[1] as u8, c[2] as u8, a.round() as u8]);
                        }
                    }
                }
            }
            put(&out, px, sel);
        }
        Effect::Noise { amount, mono } => {
            let mut out = src.to_vec();
            for i in 0..w * h {
                if out[i * 4 + 3] == 0 {
                    continue;
                }
                let n0 = (hash(i as u32, 1) - 0.5) * 2.0 * amount;
                for k in 0..3 {
                    let n = if mono {
                        n0
                    } else {
                        (hash(i as u32, 2 + k as u32) - 0.5) * 2.0 * amount
                    };
                    out[i * 4 + k] = (out[i * 4 + k] as f32 + n * 2.55).clamp(0.0, 255.0) as u8;
                }
            }
            put(&out, px, sel);
        }
        Effect::Wave {
            amp_x,
            len_x,
            amp_y,
            len_y,
            phase,
        } => {
            let mut out = vec![0u8; w * h * 4];
            let tau = std::f32::consts::TAU;
            let ph = phase.to_radians();
            for y in 0..h {
                for x in 0..w {
                    let sx = x as f32 + amp_x * (tau * y as f32 / len_x.max(1.0) + ph).sin();
                    let sy = y as f32 + amp_y * (tau * x as f32 / len_y.max(1.0) + ph).sin();
                    let (ix, iy) = (sx.round() as i64, sy.round() as i64);
                    if ix >= 0 && iy >= 0 && (ix as usize) < w && (iy as usize) < h {
                        let s = (iy as usize * w + ix as usize) * 4;
                        out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&src[s..s + 4]);
                    }
                }
            }
            put(&out, px, sel);
        }
        Effect::BrightnessContrast { brightness, contrast } => {
            let c = (100.0 + contrast.clamp(-100.0, 100.0)) / 100.0;
            let b = brightness.clamp(-100.0, 100.0) * 2.55;
            per_pixel(src, px, sel, |r, g, bl| {
                let f = |v: f32| ((v - 128.0) * c + 128.0 + b).clamp(0.0, 255.0);
                (f(r), f(g), f(bl))
            });
        }
        Effect::HueSaturation {
            hue,
            saturation,
            lightness,
        } => {
            per_pixel(src, px, sel, |r, g, b| {
                let (h0, s0, l0) = rgb_to_hsl(r / 255.0, g / 255.0, b / 255.0);
                let h1 = (h0 + hue / 360.0).rem_euclid(1.0);
                let s1 = (s0 * (1.0 + saturation / 100.0)).clamp(0.0, 1.0);
                let l1 = if lightness >= 0.0 {
                    l0 + (1.0 - l0) * lightness / 100.0
                } else {
                    l0 * (1.0 + lightness / 100.0)
                };
                let (r1, g1, b1) = hsl_to_rgb(h1, s1, l1.clamp(0.0, 1.0));
                (r1 * 255.0, g1 * 255.0, b1 * 255.0)
            });
        }
        Effect::Invert => per_pixel(src, px, sel, |r, g, b| (255.0 - r, 255.0 - g, 255.0 - b)),
        Effect::ColorKey { color, tolerance } => {
            let tol = tolerance / 100.0 * 441.7;
            for i in 0..w * h {
                if !allowed(sel, i) {
                    continue;
                }
                let s = &src[i * 4..i * 4 + 4];
                let d = (0..3).map(|k| (s[k] as f32 - color[k] as f32).powi(2)).sum::<f32>().sqrt();
                let o = &mut px[i * 4..i * 4 + 4];
                o.copy_from_slice(s);
                if d <= tol {
                    o[3] = 0;
                } else if d <= tol * 1.25 {
                    o[3] = (s[3] as f32 * (d - tol) / (tol * 0.25).max(1e-3)) as u8;
                }
            }
        }
        Effect::LumaKey { threshold, dark } => {
            let t = threshold / 100.0 * 255.0;
            for i in 0..w * h {
                if !allowed(sel, i) {
                    continue;
                }
                let s = &src[i * 4..i * 4 + 4];
                let l = 0.299 * s[0] as f32 + 0.587 * s[1] as f32 + 0.114 * s[2] as f32;
                let o = &mut px[i * 4..i * 4 + 4];
                o.copy_from_slice(s);
                if (dark && l < t) || (!dark && l > t) {
                    o[3] = 0;
                }
            }
        }
    }
}

fn put(out: &[u8], px: &mut [u8], sel: Option<&Selection>) {
    for i in 0..out.len() / 4 {
        if allowed(sel, i) {
            px[i * 4..i * 4 + 4].copy_from_slice(&out[i * 4..i * 4 + 4]);
        }
    }
}

fn per_pixel(src: &[u8], px: &mut [u8], sel: Option<&Selection>, f: impl Fn(f32, f32, f32) -> (f32, f32, f32)) {
    for i in 0..src.len() / 4 {
        if !allowed(sel, i) {
            continue;
        }
        let s = &src[i * 4..i * 4 + 4];
        let (r, g, b) = f(s[0] as f32, s[1] as f32, s[2] as f32);
        let o = &mut px[i * 4..i * 4 + 4];
        o[0] = r.round().clamp(0.0, 255.0) as u8;
        o[1] = g.round().clamp(0.0, 255.0) as u8;
        o[2] = b.round().clamp(0.0, 255.0) as u8;
        o[3] = s[3];
    }
}

/// −ln(128): a pixel at the blur radius weighs 1/128 of the centre.
pub const BLUR_FALLOFF: f32 = -4.852_030_3;

/// Weights of a 1-D gaussian of radius `r`, in 1/256 steps like the reference (256.0).
pub fn blur_weights(r: f32) -> Vec<f32> {
    let n = r.ceil().max(0.0) as i32;
    let mut k: Vec<f32> = (-n..=n)
        .map(|d| {
            let t = if r > 0.0 { d as f32 / r } else { 0.0 };
            ((BLUR_FALLOFF * t * t).exp() * 256.0).round().max(if d == 0 { 1.0 } else { 0.0 })
        })
        .collect();
    let sum: f32 = k.iter().sum();
    for v in k.iter_mut() {
        *v /= sum;
    }
    k
}

/// Separable gaussian blur (alpha-weighted), radius clamped to the picture size − 2.
pub fn blur(src: &[u8], w: usize, h: usize, rx: f32, ry: f32) -> Vec<u8> {
    let rx = rx.clamp(0.0, (w as f32 - 2.0).max(0.0));
    let ry = ry.clamp(0.0, (h as f32 - 2.0).max(0.0));
    // premultiplied floats
    let mut a: Vec<[f32; 4]> = src
        .chunks_exact(4)
        .map(|p| {
            let al = p[3] as f32 / 255.0;
            [p[0] as f32 * al, p[1] as f32 * al, p[2] as f32 * al, p[3] as f32]
        })
        .collect();
    for (r, horizontal) in [(rx, true), (ry, false)] {
        if r < 0.5 {
            continue;
        }
        let k = blur_weights(r);
        let n = (k.len() / 2) as i64;
        let mut out = vec![[0f32; 4]; w * h];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0f32; 4];
                for (j, wt) in k.iter().enumerate() {
                    let d = j as i64 - n;
                    let (sx, sy) = if horizontal {
                        ((x as i64 + d).clamp(0, w as i64 - 1) as usize, y)
                    } else {
                        (x, (y as i64 + d).clamp(0, h as i64 - 1) as usize)
                    };
                    let p = a[sy * w + sx];
                    for c in 0..4 {
                        acc[c] += p[c] * wt;
                    }
                }
                out[y * w + x] = acc;
            }
        }
        a = out;
    }
    let mut res = vec![0u8; w * h * 4];
    for (i, p) in a.iter().enumerate() {
        let al = p[3] / 255.0;
        if al > 0.0 {
            for k in 0..3 {
                res[i * 4 + k] = (p[k] / al).round().clamp(0.0, 255.0) as u8;
            }
        }
        res[i * 4 + 3] = p[3].round().clamp(0.0, 255.0) as u8;
    }
    res
}

fn hash(i: u32, salt: u32) -> f32 {
    let mut n = i.wrapping_mul(374761393) ^ salt.wrapping_mul(668265263);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s <= 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_effect_stack_survives_a_file() {
        let mut blur = FxEntry::new(Effect::Blur { rx: 3.0, ry: 0.0 });
        blur.keys = vec![(0, Effect::Blur { rx: 3.0, ry: 0.0 }), (12, Effect::Blur { rx: 9.0, ry: 0.0 })];
        let mut invert = FxEntry::new(Effect::Invert);
        invert.on = false;
        let stack = vec![blur, invert];
        let text = stack_to_json("Soft glow", &stack).unwrap();
        let back = stack_from_json(&text).unwrap();
        assert_eq!(
            (back.name.as_str(), &back.effects),
            ("Soft glow", &stack),
            "effects, keys and switches come back"
        );
        assert!(stack_from_json("{\"format\":\"something-else\",\"version\":1,\"effects\":[]}").is_err());
        assert!(stack_from_json("not a file").is_err());
    }

    #[test]
    fn keys_animate_an_effect() {
        let mut e = FxEntry::new(Effect::Blur { rx: 0.0, ry: 0.0 });
        e.keys = vec![(0, Effect::Blur { rx: 0.0, ry: 0.0 }), (10, Effect::Blur { rx: 20.0, ry: 0.0 })];
        assert_eq!(e.at(5), Effect::Blur { rx: 10.0, ry: 0.0 });
        assert_eq!(e.at(30), Effect::Blur { rx: 20.0, ry: 0.0 }, "held after the last key");
        e.set_at(5, Effect::Blur { rx: 3.0, ry: 0.0 });
        assert_eq!(e.keys.len(), 3);
        let src = vec![255u8; 4 * 4 * 4];
        let inv = vec![FxEntry::new(Effect::Invert)];
        assert_eq!(&run_stack(&inv, 0, &src, 4, 4)[0..3], &[0, 0, 0]);
    }

    #[test]
    fn blur_weight_at_radius_is_one_128th() {
        let k = blur_weights(10.0);
        let centre = k[10];
        let edge = k[0];
        assert!((edge / centre - 1.0 / 128.0).abs() < 0.002);
    }

    #[test]
    fn effects_change_pixels_inside_the_selection_only() {
        let (w, h) = (20, 10);
        let mut src = vec![0u8; w * h * 4];
        for i in 0..w * h {
            src[i * 4..i * 4 + 4].copy_from_slice(&[200, 50, 50, 255]);
        }
        let sel = Selection::rect(w, h, (0.0, 0.0), (10.0, 10.0));
        let mut px = src.clone();
        apply(&Effect::Invert, &src, &mut px, w, h, Some(&sel));
        assert_eq!(&px[0..4], &[55, 205, 205, 255]);
        assert_eq!(&px[(15) * 4..(15) * 4 + 4], &[200, 50, 50, 255]);
        let mut px = src.clone();
        apply(
            &Effect::ColorKey {
                color: [200, 50, 50],
                tolerance: 5.0,
            },
            &src,
            &mut px,
            w,
            h,
            None,
        );
        assert_eq!(px[3], 0);
        let mut px = src.clone();
        apply(
            &Effect::HueSaturation {
                hue: 120.0,
                saturation: 0.0,
                lightness: 0.0,
            },
            &src,
            &mut px,
            w,
            h,
            None,
        );
        assert!(px[1] > px[0], "red turned green");
    }
}
