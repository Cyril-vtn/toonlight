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
    /// Speed profile of the whole move, from the first key to the last: points [part of the time,
    /// part of the move], both 0..1, in time order. The ends [0, 0] and [1, 1] are implied; no
    /// point = an even speed.
    #[serde(default)]
    pub profile: Vec<[f32; 2]>,
    /// Motion blur: the part of a frame's time the shutter stays open, 0..1 (0 = none). While it
    /// is open the camera goes on moving, which smears the picture along its move.
    #[serde(default)]
    pub motion_blur: f32,
}

/// Ready-made speed profiles: even, slow start, slow arrival, slow start and arrival.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfilePreset {
    Even,
    SlowIn,
    SlowOut,
    SlowInOut,
}

pub const ALL_PROFILES: [ProfilePreset; 4] = [
    ProfilePreset::Even,
    ProfilePreset::SlowIn,
    ProfilePreset::SlowOut,
    ProfilePreset::SlowInOut,
];

impl ProfilePreset {
    pub fn name(self) -> &'static str {
        match self {
            ProfilePreset::Even => "even",
            ProfilePreset::SlowIn => "slow_in",
            ProfilePreset::SlowOut => "slow_out",
            ProfilePreset::SlowInOut => "slow_in_out",
        }
    }

    pub fn from_name(n: &str) -> Option<ProfilePreset> {
        ALL_PROFILES.into_iter().find(|p| p.name() == n)
    }

    /// The preset's points (without the two ends).
    pub fn points(self) -> Vec<[f32; 2]> {
        let at = |f: fn(f32) -> f32| [0.25f32, 0.5, 0.75].map(|t| [t, f(t)]).to_vec();
        match self {
            ProfilePreset::Even => vec![],
            ProfilePreset::SlowIn => at(|t| t * t),
            ProfilePreset::SlowOut => at(|t| 1.0 - (1.0 - t) * (1.0 - t)),
            ProfilePreset::SlowInOut => at(|t| t * t * (3.0 - 2.0 * t)),
        }
    }
}

/// The most camera positions one picture mixes for motion blur.
pub const BLUR_SAMPLES: usize = 32;

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
            profile: vec![],
            motion_blur: 0.0,
        }
    }

    /// Put the profile's points in order and inside the square; points too close in time to the
    /// one before (or to an end) are dropped.
    pub fn clean_profile(&mut self) {
        self.profile.retain(|p| p[0].is_finite() && p[1].is_finite());
        self.profile.sort_by(|a, b| a[0].total_cmp(&b[0]));
        let mut last = 0.0f32;
        self.profile.retain_mut(|p| {
            p[1] = p[1].clamp(0.0, 1.0);
            let keep = p[0] >= last + 0.01 && p[0] <= 0.99;
            if keep {
                last = p[0];
            }
            keep
        });
    }

    /// The speed profile: the part of the move done (0..1) after a part of its time (0..1). A
    /// smooth curve through the points that never overshoots them.
    pub fn profile_at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if self.profile.is_empty() {
            return t;
        }
        let mut pts: Vec<(f32, f32)> = vec![(0.0, 0.0)];
        for p in &self.profile {
            // only points later than the one before count
            if p[0] > pts[pts.len() - 1].0 && p[0] < 1.0 {
                pts.push((p[0], p[1]));
            }
        }
        pts.push((1.0, 1.0));
        monotone(&pts, t).clamp(0.0, 1.0)
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
        self.at_time(frame as f32, w, h)
    }

    /// Camera position at a time in frames, which may fall between two frames (motion blur).
    pub fn at_time(&self, time: f32, w: usize, h: usize) -> CameraKey {
        let ks = &self.keys;
        let frame = time.round().max(0.0) as usize;
        if ks.is_empty() {
            return Camera::rest(w, h, frame);
        }
        let (start, end) = (ks[0].frame as f32, ks[ks.len() - 1].frame as f32);
        if time <= start {
            return CameraKey { frame, ..ks[0] };
        }
        if time >= end {
            return CameraKey { frame, ..ks[ks.len() - 1] };
        }
        // the speed profile says how far along the whole move the camera is at this time
        let time = if self.profile.is_empty() {
            time
        } else {
            (start + self.profile_at((time - start) / (end - start)) * (end - start)).clamp(start, end)
        };
        let i = ks.iter().rposition(|k| k.frame as f32 <= time).unwrap().min(ks.len() - 2);
        let (a, b) = (ks[i], ks[i + 1]);
        let mut t = (time - a.frame as f32) / (b.frame - a.frame).max(1) as f32;
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

    /// The camera positions a frame's picture is made of: one, or with motion blur several
    /// spread over the time the shutter is open around the frame (at most `most`), close enough
    /// that the picture moves about a pixel from one to the next.
    pub fn views(&self, frame: usize, w: usize, h: usize, most: usize) -> Vec<CameraKey> {
        let still = vec![self.at(frame, w, h)];
        let blur = self.motion_blur.clamp(0.0, 1.0);
        if blur <= 0.0 || self.keys.len() < 2 || most < 2 {
            return still;
        }
        let (t0, t1) = (frame as f32 - blur / 2.0, frame as f32 + blur / 2.0);
        let (a, b) = (self.at_time(t0, w, h), self.at_time(t1, w, h));
        // how far the picture's corners travel while the shutter is open, in its own pixels
        let (hw, hh) = (self.width as f32 / 2.0, self.height as f32 / 2.0);
        let travel = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)]
            .into_iter()
            .map(|(ox, oy)| {
                // the canvas point seen in this corner at the start, and where it is at the end
                let (ca, sa) = (a.rotation.to_radians().cos(), a.rotation.to_radians().sin());
                let (px, py) = (a.x + (ox * ca - oy * sa) / a.zoom, a.y + (ox * sa + oy * ca) / a.zoom);
                let (cb, sb) = (b.rotation.to_radians().cos(), b.rotation.to_radians().sin());
                let (dx, dy) = (px - b.x, py - b.y);
                let (qx, qy) = ((dx * cb + dy * sb) * b.zoom, (-dx * sb + dy * cb) * b.zoom);
                ((qx - ox).powi(2) + (qy - oy).powi(2)).sqrt()
            })
            .fold(0.0f32, f32::max);
        if travel < 0.5 {
            return still;
        }
        let n = (travel.ceil() as usize).clamp(2, most);
        (0..n)
            .map(|i| self.at_time(t0 + (t1 - t0) * (i as f32 + 0.5) / n as f32, w, h))
            .collect()
    }

    /// Make the camera's picture from a composited canvas (opaque RGBA w×h). `paper` fills what
    /// lies outside the canvas.
    pub fn shoot(&self, frame: usize, canvas: &[u8], w: usize, h: usize, paper: [u8; 3]) -> Vec<u8> {
        self.shoot_with(frame, canvas, w, h, paper, BLUR_SAMPLES)
    }

    /// `shoot` with at most `most` camera positions mixed for motion blur (fewer = faster, for
    /// the picture on screen).
    pub fn shoot_with(&self, frame: usize, canvas: &[u8], w: usize, h: usize, paper: [u8; 3], most: usize) -> Vec<u8> {
        let views = self.views(frame, w, h, most);
        if views.len() > 1 {
            return self.shoot_blurred(&views, canvas, w, h, paper);
        }
        let k = views[0];
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

    /// The picture as the average of what the camera sees from each of `views`.
    fn shoot_blurred(&self, views: &[CameraKey], canvas: &[u8], w: usize, h: usize, paper: [u8; 3]) -> Vec<u8> {
        let (cw, ch) = (self.width.max(1), self.height.max(1));
        let turns: Vec<(CameraKey, f32, f32)> = views
            .iter()
            .map(|k| (*k, k.rotation.to_radians().cos(), k.rotation.to_radians().sin()))
            .collect();
        let turns = &turns;
        let n = views.len() as u32;
        let mut out = vec![0u8; cw * ch * 4];
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(16);
        let per = if cw * ch < 16_000 { ch } else { ch.div_ceil(cores) };
        std::thread::scope(|sc| {
            for (b, band) in out.chunks_mut(per * cw * 4).enumerate() {
                sc.spawn(move || {
                    for (r, row) in band.chunks_exact_mut(cw * 4).enumerate() {
                        let y = b * per + r;
                        for x in 0..cw {
                            let mut sum = [0u32; 3];
                            for (k, c, s) in turns {
                                let dx = (x as f32 + 0.5 - cw as f32 / 2.0) / k.zoom;
                                let dy = (y as f32 + 0.5 - ch as f32 / 2.0) / k.zoom;
                                let p = sample(canvas, w, h, k.x + dx * c - dy * s - 0.5, k.y + dx * s + dy * c - 0.5, paper);
                                for i in 0..3 {
                                    sum[i] += p[i] as u32;
                                }
                            }
                            for i in 0..3 {
                                row[x * 4 + i] = ((sum[i] + n / 2) / n) as u8;
                            }
                            row[x * 4 + 3] = 255;
                        }
                    }
                });
            }
        });
        out
    }
}

/// A smooth curve through points in x order that never goes past them between two points (a
/// monotone cubic), at `x`.
fn monotone(pts: &[(f32, f32)], x: f32) -> f32 {
    let n = pts.len();
    if n < 2 {
        return pts.first().map(|p| p.1).unwrap_or(x);
    }
    let h: Vec<f32> = (0..n - 1).map(|i| (pts[i + 1].0 - pts[i].0).max(1e-6)).collect();
    let d: Vec<f32> = (0..n - 1).map(|i| (pts[i + 1].1 - pts[i].1) / h[i]).collect();
    // the slope at each point
    let mut m = vec![0.0f32; n];
    if n == 2 {
        m[0] = d[0];
        m[1] = d[0];
    } else {
        for i in 1..n - 1 {
            m[i] = if d[i - 1] * d[i] <= 0.0 {
                0.0
            } else {
                let (w1, w2) = (2.0 * h[i] + h[i - 1], h[i] + 2.0 * h[i - 1]);
                (w1 + w2) / (w1 / d[i - 1] + w2 / d[i])
            };
        }
        // at an end: from the two nearest stretches, kept on the side the curve goes
        let end = |h0: f32, h1: f32, d0: f32, d1: f32| {
            let s = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
            if s * d0 <= 0.0 {
                0.0
            } else if d0 * d1 <= 0.0 && s.abs() > 3.0 * d0.abs() {
                3.0 * d0
            } else {
                s
            }
        };
        m[0] = end(h[0], h[1], d[0], d[1]);
        m[n - 1] = end(h[n - 2], h[n - 3], d[n - 2], d[n - 3]);
    }
    let i = pts.iter().rposition(|p| p.0 <= x).unwrap_or(0).min(n - 2);
    let t = ((x - pts[i].0) / h[i]).clamp(0.0, 1.0);
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * pts[i].1
        + (t3 - 2.0 * t2 + t) * h[i] * m[i]
        + (-2.0 * t3 + 3.0 * t2) * pts[i + 1].1
        + (t3 - t2) * h[i] * m[i + 1]
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

    fn key(frame: usize, x: f32) -> CameraKey {
        CameraKey {
            frame,
            x,
            y: 4.0,
            zoom: 1.0,
            rotation: 0.0,
        }
    }

    #[test]
    fn the_speed_profile_bends_the_time() {
        let mut c = Camera::new(8, 8);
        c.interpolation = Interpolation::Linear;
        c.set_key(key(0, 0.0));
        c.set_key(key(10, 100.0));
        c.set_key(key(20, 200.0));
        // no profile: an even speed, and times between frames
        assert!((c.at(5, 8, 8).x - 50.0).abs() < 1e-3);
        assert!((c.at_time(2.5, 8, 8).x - 25.0).abs() < 1e-3);
        assert_eq!(c.at_time(-3.0, 8, 8).x, 0.0);
        // a slow start: late at first, there at the end, over the whole move (both stretches)
        c.profile = ProfilePreset::SlowIn.points();
        assert!((c.profile_at(0.0), c.profile_at(1.0)) == (0.0, 1.0));
        assert!((c.profile_at(0.5) - 0.25).abs() < 1e-3, "through its points");
        assert!(c.profile_at(0.1) < 0.03, "it starts from rest");
        assert!((c.at(10, 8, 8).x - 50.0).abs() < 0.5, "half the time, a quarter of the move");
        assert_eq!(c.at(20, 8, 8).x, 200.0);
        let xs: Vec<f32> = (0..=20).map(|f| c.at(f, 8, 8).x).collect();
        assert!(xs.windows(2).all(|p| p[1] >= p[0]), "never backwards: {xs:?}");
        // slow arrival is its mirror; slow start and arrival crosses the middle
        c.profile = ProfilePreset::SlowOut.points();
        assert!((c.profile_at(0.5) - 0.75).abs() < 1e-3);
        c.profile = ProfilePreset::SlowInOut.points();
        assert!((c.profile_at(0.5) - 0.5).abs() < 1e-3 && c.profile_at(0.2) < 0.2 && c.profile_at(0.8) > 0.8);
        // a profile given out of order, with points outside, is put right
        c.profile = vec![[0.9, 2.0], [0.2, 0.5], [0.2, 0.1], [-1.0, 0.3], [f32::NAN, 0.0]];
        c.clean_profile();
        assert_eq!(c.profile, vec![[0.2, 0.5], [0.9, 1.0]]);
        assert_eq!(ProfilePreset::from_name("slow_in_out"), Some(ProfilePreset::SlowInOut));
    }

    #[test]
    fn motion_blur_smears_a_moving_camera() {
        // a white canvas with a black left half; the camera pans across the edge
        let (w, h) = (64, 8);
        let mut canvas = vec![255u8; w * h * 4];
        for y in 0..h {
            for x in 0..w / 2 {
                canvas[(y * w + x) * 4..(y * w + x) * 4 + 3].fill(0);
            }
        }
        let mut c = Camera::new(16, 8);
        c.interpolation = Interpolation::Linear;
        c.set_key(key(0, 24.0));
        c.set_key(key(2, 40.0)); // 8 pixels per frame; at frame 1 the edge is in the middle
        let sharp = c.shoot(1, &canvas, w, h, [255, 255, 255]);
        let row = |px: &[u8]| (0..16).map(|x| px[(4 * 16 + x) * 4]).collect::<Vec<u8>>();
        assert_eq!(row(&sharp)[..8], [0; 8], "{:?}", row(&sharp));
        assert_eq!(row(&sharp)[8..], [255; 8]);
        assert_eq!(c.views(1, w, h, BLUR_SAMPLES).len(), 1);
        c.motion_blur = 1.0;
        assert_eq!(c.views(1, w, h, BLUR_SAMPLES).len(), 8, "one position per pixel of travel");
        assert_eq!(c.views(1, w, h, 4).len(), 4);
        let blurred = row(&c.shoot(1, &canvas, w, h, [255, 255, 255]));
        assert_eq!((blurred[0], blurred[15]), (0, 255), "far from the edge nothing changes");
        assert!(
            blurred[4..12].windows(2).all(|p| p[1] > p[0]),
            "a ramp across the edge: {blurred:?}"
        );
        assert!((blurred[7] as i32 + blurred[8] as i32 - 255).abs() <= 2, "even on both sides");
        // half the shutter: half the smear
        c.motion_blur = 0.5;
        let half = row(&c.shoot(1, &canvas, w, h, [255, 255, 255]));
        assert_eq!((half[5], half[10]), (0, 255));
        assert!(half[7] > 0 && half[8] < 255);
        // a still camera stays sharp whatever the blur
        c.keys.truncate(1);
        c.motion_blur = 1.0;
        assert_eq!(c.views(0, w, h, BLUR_SAMPLES).len(), 1);
    }
}
