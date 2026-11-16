//! Sound track: a WAV or MP3 file played in time with the animation, with volume, offset, mute and
//! fade in / fade out — the fields the original keeps for each sound (Mute, Volume,
//! Offset, Locked, EnvelopeIn/In2/Out2/Out, FilePath). Like the original, the project keeps the
//! file's path, not the sound itself.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SoundTrack {
    pub path: PathBuf,
    /// 0..=2 (1 = as recorded). Default estimated: the original reads "Volume" with the current value as default.
    #[serde(default = "one")]
    pub volume: f32,
    /// Seconds after the first frame at which the sound starts (may be negative).
    #[serde(default)]
    pub offset: f32,
    #[serde(default)]
    pub mute: bool,
    /// Seconds of fade in and fade out (the original's envelope; 0 = none, estimated default).
    #[serde(default)]
    pub fade_in: f32,
    #[serde(default)]
    pub fade_out: f32,
}

fn one() -> f32 {
    1.0
}

impl SoundTrack {
    pub fn new(path: PathBuf) -> SoundTrack {
        SoundTrack {
            path,
            volume: 1.0,
            offset: 0.0,
            mute: false,
            fade_in: 0.0,
            fade_out: 0.0,
        }
    }
}

/// A decoded sound: interleaved f32 samples.
pub struct Clip {
    pub path: PathBuf,
    pub channels: u16,
    pub rate: u32,
    pub samples: Arc<Vec<f32>>,
}

impl Clip {
    pub fn load(path: &Path) -> Result<Clip, String> {
        use rodio::Source;
        let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let dec = rodio::Decoder::new(std::io::BufReader::new(f)).map_err(|e| e.to_string())?;
        let channels = dec.channels().max(1);
        let rate = dec.sample_rate().max(1);
        let samples: Vec<f32> = dec.convert_samples::<f32>().collect();
        if samples.is_empty() {
            return Err("empty sound".into());
        }
        Ok(Clip {
            path: path.to_path_buf(),
            channels,
            rate,
            samples: Arc::new(samples),
        })
    }

    pub fn duration(&self) -> f32 {
        self.samples.len() as f32 / self.channels as f32 / self.rate as f32
    }

    /// Loudest sample (0..1) heard during each frame.
    pub fn peaks_per_frame(&self, track: &SoundTrack, fps: f32, frames: usize) -> Vec<f32> {
        let ch = self.channels as usize;
        let total = self.samples.len() / ch;
        (0..frames)
            .map(|f| {
                let t0 = f as f32 / fps - track.offset;
                let t1 = (f + 1) as f32 / fps - track.offset;
                let a = (t0 * self.rate as f32).max(0.0) as usize;
                let b = ((t1 * self.rate as f32).max(0.0) as usize).min(total);
                if a >= b {
                    return 0.0;
                }
                let step = ((b - a) / 256).max(1);
                let mut m = 0f32;
                let mut i = a;
                while i < b {
                    for c in 0..ch {
                        m = m.max(self.samples[i * ch + c].abs());
                    }
                    i += step;
                }
                (m * track.volume * self.envelope(track, (a as f32) / self.rate as f32)).min(1.0)
            })
            .collect()
    }

    /// Fade gain at `t` seconds into the clip.
    pub fn envelope(&self, track: &SoundTrack, t: f32) -> f32 {
        let d = self.duration();
        let mut g = 1.0;
        if track.fade_in > 0.0 && t < track.fade_in {
            g *= (t / track.fade_in).clamp(0.0, 1.0);
        }
        if track.fade_out > 0.0 && t > d - track.fade_out {
            g *= ((d - t) / track.fade_out).clamp(0.0, 1.0);
        }
        g
    }

    /// Samples to play when the animation starts at `start` seconds.
    fn from_time(&self, track: &SoundTrack, start: f32) -> Vec<f32> {
        let ch = self.channels as usize;
        let rel = start - track.offset;
        let mut out = vec![];
        if rel < 0.0 {
            let silence = ((-rel) * self.rate as f32) as usize * ch;
            out.resize(silence, 0.0);
        }
        let first = ((rel.max(0.0)) * self.rate as f32) as usize * ch;
        let need_env = track.fade_in > 0.0 || track.fade_out > 0.0;
        for (k, s) in self.samples.iter().enumerate().skip(first) {
            let g = if need_env {
                self.envelope(track, (k / ch) as f32 / self.rate as f32)
            } else {
                1.0
            };
            out.push(s * g);
        }
        out
    }
}

/// Plays a clip; the audio device is opened the first time it's needed.
#[derive(Default)]
pub struct Player {
    out: Option<(rodio::OutputStream, rodio::OutputStreamHandle)>,
    sink: Option<rodio::Sink>,
}

impl Player {
    pub fn play(&mut self, clip: &Clip, track: &SoundTrack, start: f32) -> Result<(), String> {
        self.stop();
        if track.mute {
            return Ok(());
        }
        if self.out.is_none() {
            self.out = Some(rodio::OutputStream::try_default().map_err(|e| e.to_string())?);
        }
        let handle = &self.out.as_ref().unwrap().1;
        let sink = rodio::Sink::try_new(handle).map_err(|e| e.to_string())?;
        let data = clip.from_time(track, start);
        if data.is_empty() {
            return Ok(());
        }
        sink.set_volume(track.volume.clamp(0.0, 2.0));
        sink.append(rodio::buffer::SamplesBuffer::new(clip.channels, clip.rate, data));
        sink.play();
        self.sink = Some(sink);
        Ok(())
    }

    /// Play only `secs` seconds from `start` (audio scrubbing).
    pub fn play_for(&mut self, clip: &Clip, track: &SoundTrack, start: f32, secs: f32) -> Result<(), String> {
        self.stop();
        if track.mute {
            return Ok(());
        }
        if self.out.is_none() {
            self.out = Some(rodio::OutputStream::try_default().map_err(|e| e.to_string())?);
        }
        let handle = &self.out.as_ref().unwrap().1;
        let sink = rodio::Sink::try_new(handle).map_err(|e| e.to_string())?;
        let mut data = clip.from_time(track, start);
        let n = (secs.max(0.0) * clip.rate as f32) as usize * clip.channels as usize;
        data.truncate(n);
        if data.is_empty() {
            return Ok(());
        }
        sink.set_volume(track.volume.clamp(0.0, 2.0));
        sink.append(rodio::buffer::SamplesBuffer::new(clip.channels, clip.rate, data));
        sink.play();
        self.sink = Some(sink);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(seconds: f32) -> Clip {
        let rate = 1000;
        let n = (seconds * rate as f32) as usize;
        Clip {
            path: "t.wav".into(),
            channels: 1,
            rate,
            samples: Arc::new((0..n).map(|i| if i % 2 == 0 { 0.5 } else { -0.5 }).collect()),
        }
    }

    #[test]
    fn peaks_follow_offset_and_volume() {
        let c = clip(1.0);
        let mut t = SoundTrack::new("t.wav".into());
        t.offset = 0.5;
        let p = c.peaks_per_frame(&t, 10.0, 20);
        assert_eq!(p[0], 0.0); // before the sound starts
        assert!((p[6] - 0.5).abs() < 1e-6);
        assert_eq!(p[19], 0.0); // after it ends
        t.volume = 2.0;
        assert!((c.peaks_per_frame(&t, 10.0, 20)[6] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fades_and_start_time() {
        let c = clip(2.0);
        let mut t = SoundTrack::new("t.wav".into());
        t.fade_in = 1.0;
        assert!(c.envelope(&t, 0.25) < 0.3);
        assert_eq!(c.envelope(&t, 1.5), 1.0);
        t.offset = 1.0;
        // starting 0.5 s before the sound: 0.5 s of silence first
        assert_eq!(c.from_time(&t, 0.5).len(), 500 + 2000);
    }
}
