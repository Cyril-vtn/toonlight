//! Animated camera: key points (position, zoom, rotation) on frames, interpolated between them,
//! used to frame the picture for playback preview and every export.
//!
//! Follows the reference's camera path (the reference reads "CameraPath": PointCount, then for each
//! "Point.%d." X, Y, Rotation, Zoom (default 1.0), and Profile, Mode (default 2), XLock, YLock,
//! RotationLock, ZoomLock, MotionBlur, LockAll) and its camera size ("CameraWidth", "CameraHeight").

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct CameraKey {
    pub frame: usize,
    /// Centre of the view on the canvas, in pixels.
    pub x: f32,
    pub y: f32,
    /// 1 = the camera sees `width × height` canvas pixels; 2 = twice closer.
    pub zoom: f32,
    /// Degrees.
    pub rotation: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Interpolation {
    Linear,
    /// Smooth curve through the points (Catmull-Rom); our reading of the default mode 2.
    Smooth,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Camera {
    pub enabled: bool,
    /// Size of the picture the camera makes (defaults to the canvas size).
    pub width: usize,
    pub height: usize,
    pub keys: Vec<CameraKey>,
    pub interpolation: Interpolation,
    /// Slow in / slow out between points (our stand-in for the reference's speed "Profile").
    pub ease: bool,
    /// Values the camera tool and panel leave alone ("XLock", "YLock", "RotationLock",
    /// "ZoomLock" in the reference's CameraPath, off by default).
    #[serde(default)]
    pub lock_x: bool,
    #[serde(default)]
    pub lock_y: bool,
    #[serde(default)]
    pub lock_rotation: bool,
    #[serde(default)]
    pub lock_zoom: bool,
}

impl Camera {
    pub fn new(w: usize, h: usize) -> Camera {
        Camera {
            enabled: false,
            width: w,
            height: h,
            keys: vec![],
            interpolation: Interpolation::Smooth,
            ease: false,
            lock_x: false,
            lock_y: false,
            lock_rotation: false,
            lock_zoom: false,
        }
    }

    /// The default view: the whole canvas.
    pub fn rest(w: usize, h: usize, frame: usize) -> CameraKey {
        CameraKey {
            frame,
            x: w as f32 / 2.0,
            y: h as f32 / 2.0,
            zoom: 1.0,
            rotation: 0.0,
        }
    }

    pub fn sort(&mut self) {
        self.keys.sort_by_key(|k| k.frame);
        self.keys.dedup_by_key(|k| k.frame);
    }

    /// Set (or add) the key at `frame`.
    pub fn set_key(&mut self, k: CameraKey) {
        if let Some(e) = self.keys.iter_mut().find(|e| e.frame == k.frame) {
            *e = k;
        } else {
            self.keys.push(k);
            self.sort();
        }
    }

    /// Camera position at a frame.
    pub fn at(&self, frame: usize, w: usize, h: usize) -> CameraKey {
        let ks = &self.keys;
        if ks.is_empty() {
            return Camera::rest(w, h, frame);
        }
        if frame <= ks[0].frame {
            return CameraKey { frame, ..ks[0] };
        }
        let last = ks[ks.len() - 1];
        if frame >= last.frame {
            return CameraKey { frame, ..last };
        }
        let i = ks.iter().rposition(|k| k.frame <= frame).unwrap();
        let (a, b) = (ks[i], ks[i + 1]);
        let mut t = (frame - a.frame) as f32 / (b.frame - a.frame).max(1) as f32;
        if self.ease {
            t = t * t * (3.0 - 2.0 * t);
        }
        let (x, y) = match self.interpolation {
            Interpolation::Linear => (a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t),
            Interpolation::Smooth => {
                let p0 = if i > 0 { ks[i - 1] } else { a };
                let p3 = if i + 2 < ks.len() { ks[i + 2] } else { b };
                (catmull(p0.x, a.x, b.x, p3.x, t), catmull(p0.y, a.y, b.y, p3.y, t))
            }
        };
        // zoom changes evenly in perceived size (geometric), rotation linearly
        let zoom = (a.zoom.max(0.01).ln() + (b.zoom.max(0.01).ln() - a.zoom.max(0.01).ln()) * t).exp();
        let rotation = a.rotation + (b.rotation - a.rotation) * t;
        CameraKey {
            frame,
            x,
            y,
            zoom,
            rotation,
        }
    }

    /// Corners of the camera's view on the canvas at a frame (for drawing its frame).
    pub fn corners(&self, frame: usize, w: usize, h: usize) -> [(f32, f32); 4] {
        let k = self.at(frame, w, h);
        let (hw, hh) = (self.width as f32 / 2.0 / k.zoom, self.height as f32 / 2.0 / k.zoom);
        let (c, s) = (k.rotation.to_radians().cos(), k.rotation.to_radians().sin());
        [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|(dx, dy)| (k.x + dx * c - dy * s, k.y + dx * s + dy * c))
    }

    /// Make the camera's picture from a composited canvas (opaque RGBA w×h). `paper` fills what
    /// lies outside the canvas.
    pub fn shoot(&self, frame: usize, canvas: &[u8], w: usize, h: usize, paper: [u8; 3]) -> Vec<u8> {
        let k = self.at(frame, w, h);
        let (cw, ch) = (self.width.max(1), self.height.max(1));
        let (c, s) = (k.rotation.to_radians().cos(), k.rotation.to_radians().sin());
        let mut out = vec![0u8; cw * ch * 4];
        // one band of rows per core
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(16);
        let per = if cw * ch < 160_000 { ch } else { ch.div_ceil(cores) };
        std::thread::scope(|sc| {
            for (b, band) in out.chunks_mut(per * cw * 4).enumerate() {
                sc.spawn(move || {
                    for (r, row) in band.chunks_exact_mut(cw * 4).enumerate() {
                        let y = b * per + r;
                        for x in 0..cw {
                            let dx = (x as f32 + 0.5 - cw as f32 / 2.0) / k.zoom;
                            let dy = (y as f32 + 0.5 - ch as f32 / 2.0) / k.zoom;
                            let sx = k.x + dx * c - dy * s - 0.5;
                            let sy = k.y + dx * s + dy * c - 0.5;
                            row[x * 4..x * 4 + 4].copy_from_slice(&sample(canvas, w, h, sx, sy, paper));
                        }
                    }
                });
            }
        });
        out
    }
}

fn catmull(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * (2.0 * p1 + (-p0 + p2) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

fn sample(px: &[u8], w: usize, h: usize, x: f32, y: f32, paper: [u8; 3]) -> [u8; 4] {
    let x0 = x.floor();
    let y0 = y.floor();
    let (fx, fy) = (x - x0, y - y0);
    let mut acc = [0f32; 3];
    for (xi, yi, wt) in [
        (x0 as i64, y0 as i64, (1.0 - fx) * (1.0 - fy)),
        (x0 as i64 + 1, y0 as i64, fx * (1.0 - fy)),
        (x0 as i64, y0 as i64 + 1, (1.0 - fx) * fy),
        (x0 as i64 + 1, y0 as i64 + 1, fx * fy),
    ] {
        let c = if xi < 0 || yi < 0 || xi >= w as i64 || yi >= h as i64 {
            [paper[0], paper[1], paper[2]]
        } else {
            let i = (yi as usize * w + xi as usize) * 4;
            [px[i], px[i + 1], px[i + 2]]
        };
        for k in 0..3 {
            acc[k] += c[k] as f32 * wt;
        }
    }
    [acc[0].round() as u8, acc[1].round() as u8, acc[2].round() as u8, 255]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_between_keys() {
        let mut c = Camera::new(100, 50);
        c.interpolation = Interpolation::Linear;
        c.set_key(CameraKey {
            frame: 0,
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
            rotation: 0.0,
        });
        c.set_key(CameraKey {
            frame: 10,
            x: 100.0,
            y: 50.0,
            zoom: 4.0,
            rotation: 90.0,
        });
        let m = c.at(5, 100, 50);
        assert!((m.x - 50.0).abs() < 1e-3 && (m.rotation - 45.0).abs() < 1e-3);
        assert!((m.zoom - 2.0).abs() < 1e-3, "zoom is geometric");
        assert_eq!(c.at(20, 100, 50).x, 100.0);
        c.interpolation = Interpolation::Smooth;
        assert!((c.at(5, 100, 50).x - 50.0).abs() < 1.0);
    }

    #[test]
    fn shoot_zooms_into_the_centre() {
        let (w, h) = (4, 4);
        let mut canvas = vec![255u8; w * h * 4];
        // centre 2×2 black
        for y in 1..3 {
            for x in 1..3 {
                canvas[(y * w + x) * 4..(y * w + x) * 4 + 3].fill(0);
            }
        }
        let mut c = Camera::new(4, 4);
        c.set_key(CameraKey {
            frame: 0,
            x: 2.0,
            y: 2.0,
            zoom: 2.0,
            rotation: 0.0,
        });
        let out = c.shoot(0, &canvas, w, h, [255, 255, 255]);
        assert!(out[0] < 128, "zoomed: corners are now black");
    }
}
