//! Saving and opening projects (our own format), and exporting frames as PNG pictures.

use crate::doc::{Doc, Drawing, Layer, LightTable, Paper};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::Path;

const MAGIC: &[u8; 8] = b"ANMCRFT1";
/// Magic of files written before the app was renamed (still read).
const OLD_MAGIC: &[u8; 8] = b"ATANIM01";
/// A project with several clips: a small JSON list of the clips (name, notes), then each clip as
/// an embedded ANMCRFT1 stream.
const MAGIC2: &[u8; 8] = b"ANMCRFT2";
const OLD_MAGIC2: &[u8; 8] = b"ATANIM02";

/// One clip of a project, as saved: its name, its notes (storyboard text) and its animation.
pub struct ClipFile {
    pub name: String,
    pub notes: String,
    pub doc: Doc,
}

#[derive(Serialize, Deserialize)]
struct ClipInfo {
    name: String,
    #[serde(default)]
    notes: String,
}

#[derive(Serialize, Deserialize)]
struct Header {
    w: usize,
    h: usize,
    fps: f32,
    frames: usize,
    paper: [u8; 3],
    #[serde(default)]
    paper_texture: Paper,
    /// The first sound track: the only one in files written before several tracks existed, and
    /// still written so that those versions open the project with its first sound.
    #[serde(default)]
    sound: Option<crate::sound::SoundTrack>,
    #[serde(default)]
    sounds: Vec<crate::sound::SoundTrack>,
    #[serde(default = "crate::doc::default_palette")]
    palette: Vec<[u8; 3]>,
    #[serde(default)]
    camera: Option<crate::camera::Camera>,
    light: LightTable,
    layers: Vec<Layer>,
    drawings: usize,
    #[serde(default)]
    bookmarks: Vec<usize>,
    #[serde(default)]
    mark_in: Option<usize>,
    #[serde(default)]
    mark_out: Option<usize>,
    #[serde(default)]
    notes: std::collections::BTreeMap<usize, String>,
}

pub fn png_bytes(px: &[u8], w: usize, h: usize) -> Result<Vec<u8>, String> {
    let mut out = vec![];
    let enc = image::codecs::png::PngEncoder::new(&mut out);
    image::ImageEncoder::write_image(enc, px, w as u32, h as u32, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn save(doc: &Doc, path: &Path) -> Result<(), String> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    write_doc(doc, &mut f)?;
    f.flush().map_err(|e| e.to_string())
}

/// Save several clips in one project file.
pub fn save_project(clips: &[(&Doc, &str, &str)], path: &Path) -> Result<(), String> {
    if clips.len() == 1 && clips[0].1.is_empty() && clips[0].2.is_empty() {
        return save(clips[0].0, path);
    }
    let info: Vec<ClipInfo> = clips
        .iter()
        .map(|c| ClipInfo {
            name: c.1.to_string(),
            notes: c.2.to_string(),
        })
        .collect();
    let json = serde_json::to_vec(&info).map_err(|e| e.to_string())?;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    f.write_all(MAGIC2).map_err(|e| e.to_string())?;
    f.write_all(&(json.len() as u32).to_le_bytes()).map_err(|e| e.to_string())?;
    f.write_all(&json).map_err(|e| e.to_string())?;
    for c in clips {
        write_doc(c.0, &mut f)?;
    }
    f.flush().map_err(|e| e.to_string())
}

/// A full copy of an animation (through the file format).
pub fn duplicate(doc: &Doc) -> Result<Doc, String> {
    let mut buf = vec![];
    write_doc(doc, &mut buf)?;
    let mut r = std::io::Cursor::new(buf);
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic).map_err(|e| e.to_string())?;
    read_doc(&mut r)
}

fn write_doc(doc: &Doc, f: &mut impl Write) -> Result<(), String> {
    // Only keep drawings still used by a layer.
    let mut used: Vec<usize> = doc.layers.iter().flat_map(|l| l.cels.iter().flatten().copied()).collect();
    used.sort();
    used.dedup();
    let remap = |id: usize| used.binary_search(&id).unwrap();
    let layers: Vec<Layer> = doc
        .layers
        .iter()
        .map(|l| {
            let mut l = l.clone();
            for c in l.cels.iter_mut() {
                *c = c.map(remap);
            }
            l.names = l
                .names
                .iter()
                .filter_map(|(id, n)| used.binary_search(id).ok().map(|i| (i, n.clone())))
                .collect();
            l.marks = l
                .marks
                .iter()
                .filter_map(|(id, m)| used.binary_search(id).ok().map(|i| (i, *m)))
                .collect();
            l
        })
        .collect();
    let header = Header {
        w: doc.w,
        h: doc.h,
        fps: doc.fps,
        frames: doc.frames,
        paper: doc.paper,
        paper_texture: doc.paper_texture,
        sound: doc.sounds.first().cloned(),
        sounds: doc.sounds.clone(),
        palette: doc.palette.clone(),
        camera: Some(doc.camera.clone()),
        light: doc.light.clone(),
        layers,
        drawings: used.len(),
        bookmarks: doc.bookmarks.clone(),
        mark_in: doc.mark_in,
        mark_out: doc.mark_out,
        notes: doc.notes.clone(),
    };
    let json = serde_json::to_vec(&header).map_err(|e| e.to_string())?;
    let mut write = |b: &[u8]| f.write_all(b).map_err(|e| e.to_string());
    write(MAGIC)?;
    write(&(json.len() as u32).to_le_bytes())?;
    write(&json)?;
    for id in used {
        let png = png_bytes(&doc.drawings[id].px, doc.w, doc.h)?;
        write(&(png.len() as u32).to_le_bytes())?;
        write(&png)?;
    }
    Ok(())
}

/// Open a project's first clip.
#[cfg(test)]
pub fn open(path: &Path) -> Result<Doc, String> {
    Ok(open_project(path)?.remove(0).doc)
}

/// Whether "Save" can write straight to `path`: only a project file (our extension) that still
/// exists; anything else (an older-format file, a file moved or deleted since) goes through
/// "Save as".
pub fn saves_in_place(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("toonlight") || e.eq_ignore_ascii_case("animcraft"))
        && path.is_file()
}

/// A project file name that is still free next to `path`: the extension is added when missing, a
/// trailing number in brackets ("walk(3)") is dropped, then "name", "name(2)", "name(3)"… are tried
/// until one doesn't exist.
pub fn free_name(path: &Path) -> std::path::PathBuf {
    let ext = "toonlight";
    let dir = path.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let mut stem = match path.extension().and_then(|e| e.to_str()) {
        Some(e) if [ext, "animcraft", "atanim"].iter().any(|x| e.eq_ignore_ascii_case(x)) => {
            path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
        }
        _ => path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
    };
    if stem.ends_with(')') {
        if let Some(i) = stem.rfind('(') {
            let digits = &stem[i + 1..stem.len() - 1];
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                stem.truncate(i);
            }
        }
    }
    if stem.is_empty() {
        stem = "animation".into();
    }
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    (2..).map(|n| dir.join(format!("{stem}({n}).{ext}"))).find(|p| !p.exists()).unwrap()
}

/// Open every clip of a project (one for files with a single animation).
pub fn open_project(path: &Path) -> Result<Vec<ClipFile>, String> {
    let mut f = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?);
    let mut magic = [0u8; 8];
    f.read_exact(&mut magic).map_err(|e| e.to_string())?;
    if &magic == MAGIC || &magic == OLD_MAGIC {
        return Ok(vec![ClipFile {
            name: String::new(),
            notes: String::new(),
            doc: read_doc(&mut f)?,
        }]);
    }
    if &magic != MAGIC2 && &magic != OLD_MAGIC2 {
        return Err(crate::i18n::tr("Ce fichier n'est pas un projet d'animation.").into());
    }
    let mut len = [0u8; 4];
    f.read_exact(&mut len).map_err(|e| e.to_string())?;
    let mut json = vec![0u8; u32::from_le_bytes(len) as usize];
    f.read_exact(&mut json).map_err(|e| e.to_string())?;
    let info: Vec<ClipInfo> = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for c in info {
        f.read_exact(&mut magic).map_err(|e| e.to_string())?;
        if &magic != MAGIC && &magic != OLD_MAGIC {
            return Err(crate::i18n::tr("Ce fichier n'est pas un projet d'animation.").into());
        }
        out.push(ClipFile {
            name: c.name,
            notes: c.notes,
            doc: read_doc(&mut f)?,
        });
    }
    if out.is_empty() {
        return Err(crate::i18n::tr("Le projet n'a aucun calque.").into());
    }
    Ok(out)
}

/// Read one animation (after its 8-byte mark).
fn read_doc(f: &mut impl Read) -> Result<Doc, String> {
    let mut len = [0u8; 4];
    f.read_exact(&mut len).map_err(|e| e.to_string())?;
    let mut json = vec![0u8; u32::from_le_bytes(len) as usize];
    f.read_exact(&mut json).map_err(|e| e.to_string())?;
    let h: Header = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
    let mut doc = Doc::new(h.w, h.h, h.fps);
    doc.drawings.clear();
    for _ in 0..h.drawings {
        f.read_exact(&mut len).map_err(|e| e.to_string())?;
        let mut png = vec![0u8; u32::from_le_bytes(len) as usize];
        f.read_exact(&mut png).map_err(|e| e.to_string())?;
        let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?
            .to_rgba8();
        if img.width() as usize != h.w || img.height() as usize != h.h {
            return Err(crate::i18n::tr("Une image du projet n'a pas la bonne taille.").into());
        }
        doc.drawings.push(Drawing { px: img.into_raw() });
    }
    doc.frames = h.frames.max(1);
    doc.paper = h.paper;
    doc.paper_texture = h.paper_texture;
    doc.sounds = if h.sounds.is_empty() {
        h.sound.into_iter().collect()
    } else {
        h.sounds
    };
    doc.palette = h.palette;
    if let Some(c) = h.camera {
        doc.camera = c;
    }
    doc.light = h.light;
    doc.layers = h.layers;
    doc.bookmarks = h.bookmarks;
    doc.mark_in = h.mark_in;
    doc.mark_out = h.mark_out;
    doc.notes = h.notes;
    if doc.layers.is_empty() {
        return Err(crate::i18n::tr("Le projet n'a aucun calque.").into());
    }
    for l in &mut doc.layers {
        l.cels.resize(doc.frames, None);
        for c in l.cels.iter_mut() {
            if let Some(id) = *c {
                if id >= doc.drawings.len() {
                    *c = None;
                }
            }
        }
    }
    doc.current_layer = doc.layers.len() - 1;
    doc.dirty = false;
    Ok(doc)
}

/// The film: every clip one after the other, each frame at the first clip's picture size.
fn film_frames<'a>(clips: &'a [&'a Doc]) -> impl Iterator<Item = Vec<u8>> + 'a {
    let (fw, fh) = clips[0].output_size();
    clips.iter().flat_map(move |d| {
        (0..d.frames).map(move |f| {
            let px = d.render(f);
            let (ow, oh) = d.output_size();
            if (ow, oh) == (fw, fh) {
                px
            } else {
                // fit the other size inside, on the clip's paper colour
                let mut out = vec![0u8; fw * fh * 4];
                for p in out.chunks_exact_mut(4) {
                    p.copy_from_slice(&[d.paper[0], d.paper[1], d.paper[2], 255]);
                }
                let fitted = fit_picture(px, ow, oh, fw, fh);
                for (o, s) in out.chunks_exact_mut(4).zip(fitted.chunks_exact(4)) {
                    if s[3] > 0 {
                        o.copy_from_slice(s);
                    }
                }
                out
            }
        })
    })
}

/// Export every clip one after the other as numbered PNG pictures.
pub fn export_film_frames(clips: &[&Doc], dir: &Path) -> Result<usize, String> {
    let (fw, fh) = clips[0].output_size();
    let mut n = 0;
    for px in film_frames(clips) {
        n += 1;
        std::fs::write(dir.join(format!("film_{n:05}.png")), png_bytes(&px, fw, fh)?).map_err(|e| e.to_string())?;
    }
    Ok(n)
}

/// Export every clip one after the other as one MP4, at the first clip's frame rate, with each
/// clip's sound tracks placed where the clip starts and cut where it ends.
pub fn export_film_video(clips: &[&Doc], path: &Path, ffmpeg: &Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let (fw, fh) = clips[0].output_size();
    let fps = clips[0].fps.max(1.0);
    let mut placed = vec![];
    let mut start = 0.0;
    for d in clips {
        let length = d.frames as f32 / fps;
        placed.extend(d.sounds.iter().map(|t| PlacedSound { track: t, start, length }));
        start += length;
    }
    let mut child = Command::new(ffmpeg)
        .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
        .arg(format!("{fw}x{fh}"))
        .args(["-r", &format!("{}", clips[0].fps), "-i", "-"])
        .args(av_args(&placed, start))
        .arg(path)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    {
        let stdin = child.stdin.as_mut().ok_or("ffmpeg")?;
        for px in film_frames(clips) {
            stdin.write_all(&px).map_err(|e| e.to_string())?;
        }
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Write every frame (without the light table) as numbered PNG pictures in `dir`.
pub fn export_frames(doc: &Doc, dir: &Path) -> Result<usize, String> {
    let (ow, oh) = doc.output_size();
    for f in 0..doc.frames {
        let buf = doc.render(f);
        let png = png_bytes(&buf, ow, oh)?;
        std::fs::write(dir.join(format!("image_{:04}.png", f + 1)), png).map_err(|e| e.to_string())?;
    }
    Ok(doc.frames)
}

/// Export every frame as numbered pictures in another format: "tiff", "tga", "jpeg" or "bmp"
/// (JPEG and BMP without transparency; the frames are opaque anyway).
pub fn export_frames_as(doc: &Doc, dir: &Path, format: &str) -> Result<usize, String> {
    let (ow, oh) = doc.output_size();
    let (fmt, ext) = match format {
        "tiff" => (image::ImageFormat::Tiff, "tif"),
        "tga" => (image::ImageFormat::Tga, "tga"),
        "jpeg" => (image::ImageFormat::Jpeg, "jpg"),
        "bmp" => (image::ImageFormat::Bmp, "bmp"),
        _ => (image::ImageFormat::Png, "png"),
    };
    for f in 0..doc.frames {
        let buf = doc.render(f);
        let path = dir.join(format!("image_{:04}.{ext}", f + 1));
        let res = if matches!(fmt, image::ImageFormat::Jpeg | image::ImageFormat::Bmp) {
            let rgb: Vec<u8> = buf.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
            image::save_buffer_with_format(&path, &rgb, ow as u32, oh as u32, image::ExtendedColorType::Rgb8, fmt)
        } else {
            image::save_buffer_with_format(&path, &buf, ow as u32, oh as u32, image::ExtendedColorType::Rgba8, fmt)
        };
        res.map_err(|e| e.to_string())?;
    }
    Ok(doc.frames)
}

/// Read a palette in the GIMP format (.gpl): one "R G B name" line per colour.
pub fn load_palette(path: &Path) -> Result<Vec<[u8; 3]>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with("GIMP") || l.contains(':') {
            continue;
        }
        let nums: Vec<u8> = l
            .split_whitespace()
            .take(3)
            .filter_map(|n| n.parse::<u16>().ok().map(|v| v.min(255) as u8))
            .collect();
        if nums.len() == 3 {
            out.push([nums[0], nums[1], nums[2]]);
        }
    }
    if out.is_empty() {
        return Err("no colours in this file".into());
    }
    Ok(out)
}

pub fn save_palette(path: &Path, colours: &[[u8; 3]]) -> Result<(), String> {
    let mut s = String::from("GIMP Palette\nName: Toonlight\nColumns: 6\n#\n");
    for c in colours {
        s.push_str(&format!(
            "{:3} {:3} {:3}\t#{:02x}{:02x}{:02x}\n",
            c[0], c[1], c[2], c[0], c[1], c[2]
        ));
    }
    std::fs::write(path, s).map_err(|e| e.to_string())
}

/// Decode a picture file (PNG or JPEG) to RGBA.
pub fn load_picture(path: &Path) -> Result<(Vec<u8>, usize, usize), String> {
    let img = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    Ok((img.into_raw(), w, h))
}

/// A picture scaled to fit inside w×h (never enlarged) and centred on a transparent w×h canvas.
pub fn fit_picture(px: Vec<u8>, pw: usize, ph: usize, w: usize, h: usize) -> Vec<u8> {
    let scale = (w as f32 / pw as f32).min(h as f32 / ph as f32).min(1.0);
    let (tw, th) = (((pw as f32 * scale) as u32).max(1), ((ph as f32 * scale) as u32).max(1));
    let img = image::RgbaImage::from_raw(pw as u32, ph as u32, px).unwrap_or_default();
    let img = if scale < 1.0 {
        image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut out = vec![0u8; w * h * 4];
    let (ox, oy) = ((w - img.width() as usize) / 2, (h - img.height() as usize) / 2);
    for y in 0..img.height() as usize {
        let src = &img.as_raw()[y * img.width() as usize * 4..(y + 1) * img.width() as usize * 4];
        let d = ((oy + y) * w + ox) * 4;
        out[d..d + src.len()].copy_from_slice(src);
    }
    out
}

pub fn save_png(path: &Path, px: &[u8], w: usize, h: usize) -> Result<(), String> {
    let png = png_bytes(px, w, h)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    std::fs::write(path, png).map_err(|e| e.to_string())
}

/// File-name-safe version of a layer name.
fn safe_name(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if t.is_empty() {
        "calque".into()
    } else {
        t
    }
}

/// Export the layer structure as JSON + pictures, in the layout the After Effects import script
/// shipped with the original reads (project.clip.layers[].link[].file / images).
pub fn export_after_effects(doc: &Doc, dir: &Path, name: &str) -> Result<std::path::PathBuf, String> {
    use serde_json::json;
    let mut layers = vec![];
    let mut used_names: Vec<String> = vec![];
    // the script expects the top layer first
    for (li, layer) in doc.layers.iter().enumerate().rev() {
        let mut lname = safe_name(&layer.name);
        while used_names.contains(&lname) {
            lname.push('_');
        }
        used_names.push(lname.clone());
        let keys = doc.keys(li);
        if keys.is_empty() {
            continue;
        }
        std::fs::create_dir_all(dir.join(&lname)).map_err(|e| e.to_string())?;
        let mut link = vec![];
        for (n, (start, id)) in keys.iter().enumerate() {
            let end = keys.get(n + 1).map(|k| k.0).unwrap_or(doc.frames);
            let file = format!("{lname}/{lname}_{:04}.png", n + 1);
            std::fs::write(dir.join(&file), png_bytes(&doc.drawings[*id].px, doc.w, doc.h)?).map_err(|e| e.to_string())?;
            let images: Vec<usize> = (*start..end).collect();
            link.push(json!({"file": file, "images": images, "instance-name": format!("{}", n + 1)}));
        }
        layers.push(json!({
            "name": layer.name,
            "start": keys[0].0,
            "end": doc.frames - 1,
            "opacity": (layer.opacity * 255.0).round() as i32,
            "visible": if layer.visible { "true" } else { "false" },
            "blending-mode": layer.blend.json_name(),
            "pre-behavior": 0,
            "post-behavior": 0,
            "group": {"red": layer.group[0], "green": layer.group[1], "blue": layer.group[2]},
            "link": link,
        }));
    }
    let data = json!({
        "project": {
            "clip": {
                "name": name,
                "width": doc.w,
                "height": doc.h,
                "pixelaspectratio": 1.0,
                "framerate": doc.fps,
                "image-count": doc.frames,
                "bg": {"red": doc.paper[0], "green": doc.paper[1], "blue": doc.paper[2]},
                "layers": layers,
                "camera": {"points": camera_points(doc)},
            },
            "camera": {"width": doc.output_size().0, "height": doc.output_size().1, "pixelaspectratio": 1.0, "framerate": doc.fps},
        }
    });
    let path = dir.join(format!("{}.json", safe_name(name)));
    std::fs::write(&path, serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Camera points for the After Effects script: one per frame (the script spreads its points evenly
/// over the clip), x / y the centre, scale = 1 / zoom (the script moves the camera back by
/// 1000 × scale) and angle (the script rotates by −angle).
fn camera_points(doc: &Doc) -> Vec<serde_json::Value> {
    if !doc.camera.enabled {
        return vec![serde_json::json!({"x": doc.w as f32 / 2.0, "y": doc.h as f32 / 2.0, "scale": 1.0, "angle": 0.0})];
    }
    (0..doc.frames)
        .map(|f| {
            let k = doc.camera.at(f, doc.w, doc.h);
            serde_json::json!({"x": k.x, "y": k.y, "scale": 1.0 / k.zoom.max(0.01), "angle": k.rotation})
        })
        .collect()
}

/// Export the animation as an animated GIF (looping), at the project's frame rate.
pub fn export_gif(doc: &Doc, path: &Path, max_side: usize) -> Result<(), String> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let (ow, oh) = doc.output_size();
    let scale = (max_side as f32 / ow.max(oh) as f32).min(1.0);
    let (tw, th) = (((ow as f32 * scale) as u32).max(1), ((oh as f32 * scale) as u32).max(1));
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = GifEncoder::new_with_speed(std::io::BufWriter::new(file), 10);
    enc.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;
    let delay = image::Delay::from_numer_denom_ms(1000, doc.fps.max(1.0).round() as u32);
    for f in 0..doc.frames {
        let buf = doc.render(f);
        let img = image::RgbaImage::from_raw(ow as u32, oh as u32, buf).ok_or("image")?;
        let img = if scale < 1.0 {
            image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle)
        } else {
            img
        };
        enc.encode_frame(image::Frame::from_parts(img, 0, 0, delay))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Where FFmpeg is, if it's installed (used for video export).
pub fn find_ffmpeg() -> Option<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "C:\\ffmpeg\\bin"] {
        dirs.push(d.into());
    }
    for d in dirs {
        for n in ["ffmpeg", "ffmpeg.exe"] {
            let p = d.join(n);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// A sound track placed in a video: the shot it belongs to starts at `start` seconds and lasts
/// `length` seconds (the sound is not heard outside its shot).
pub struct PlacedSound<'a> {
    pub track: &'a crate::sound::SoundTrack,
    pub start: f32,
    pub length: f32,
}

/// The FFmpeg filter that places one sound (input number `input`) and names the result
/// `[a<input>]`: fades in the sound's own time, the part before the shot's first frame cut off
/// (negative offset), volume, the end cut at the shot's end, then silence up to its place.
/// `duration` is the sound file's length when known (needed for the fade out).
pub fn sound_filter(s: &PlacedSound, input: usize, duration: Option<f32>) -> String {
    let t = s.track;
    let mut f: Vec<String> = vec![];
    if t.fade_in > 0.0 {
        f.push(format!("afade=t=in:st=0:d={}", t.fade_in));
    }
    if let Some(d) = duration.filter(|_| t.fade_out > 0.0) {
        f.push(format!("afade=t=out:st={}:d={}", (d - t.fade_out).max(0.0), t.fade_out));
    }
    if t.offset < 0.0 {
        f.push(format!("atrim=start={}", -t.offset));
        f.push("asetpts=PTS-STARTPTS".into());
    }
    f.push(format!("volume={}", t.volume));
    let late = t.offset.max(0.0);
    f.push(format!("atrim=end={}", (s.length - late).max(0.0)));
    f.push("aformat=sample_rates=48000:channel_layouts=stereo".into());
    let delay = ((s.start + late) * 1000.0).round() as u64;
    if delay > 0 {
        f.push(format!("adelay={delay}:all=1"));
    }
    format!("[{input}:a]{}[a{input}]", f.join(","))
}

/// FFmpeg arguments for the pictures (input 0) and the sounds: every sound that is not muted and
/// whose file exists is placed, then they are mixed as they are (no automatic level change). The
/// video lasts `total` seconds whatever the sounds' lengths.
fn av_args(sounds: &[PlacedSound], total: f32) -> Vec<String> {
    let pad = "pad=ceil(iw/2)*2:ceil(ih/2)*2";
    let video = ["-c:v", "libx264", "-pix_fmt", "yuv420p"].map(String::from);
    let heard: Vec<&PlacedSound> = sounds
        .iter()
        .filter(|s| !s.track.mute && s.track.path.is_file() && s.length > s.track.offset.max(0.0))
        .collect();
    if heard.is_empty() {
        let mut a = video.to_vec();
        a.extend(["-vf".into(), pad.into()]);
        return a;
    }
    let mut a: Vec<String> = vec![];
    let mut graph = vec![format!("[0:v]{pad}[v]")];
    let mut mix = String::new();
    for (k, s) in heard.iter().enumerate() {
        a.extend(["-i".into(), s.track.path.display().to_string()]);
        // the file's length is only needed to place a fade out
        let duration = if s.track.fade_out > 0.0 {
            crate::sound::Clip::load(&s.track.path).ok().map(|c| c.duration())
        } else {
            None
        };
        graph.push(sound_filter(s, k + 1, duration));
        mix.push_str(&format!("[a{}]", k + 1));
    }
    graph.push(if heard.len() == 1 {
        format!("{mix}anull[aout]")
    } else {
        format!("{mix}amix=inputs={}:normalize=0:duration=longest[aout]", heard.len())
    });
    a.extend(["-filter_complex".into(), graph.join(";")]);
    a.extend(["-map", "[v]", "-map", "[aout]"].map(String::from));
    a.extend(video);
    a.extend(["-c:a".into(), "aac".into(), "-t".into(), format!("{total}")]);
    a
}

/// Export an MP4 (H.264) video through FFmpeg, feeding it raw frames (and the sound tracks).
pub fn export_video(doc: &Doc, path: &Path, ffmpeg: &Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new(ffmpeg)
        .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
        .arg({
            let (ow, oh) = doc.output_size();
            format!("{ow}x{oh}")
        })
        .args(["-r", &format!("{}", doc.fps), "-i", "-"])
        .args({
            let length = doc.frames as f32 / doc.fps.max(1.0);
            let placed: Vec<PlacedSound> = doc
                .sounds
                .iter()
                .map(|t| PlacedSound {
                    track: t,
                    start: 0.0,
                    length,
                })
                .collect();
            av_args(&placed, length)
        })
        .arg(path)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    {
        let stdin = child.stdin.as_mut().ok_or("ffmpeg")?;
        for f in 0..doc.frames {
            let buf = doc.render(f);
            stdin.write_all(&buf).map_err(|e| e.to_string())?;
        }
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// One shot of a storyboard page: name, notes, length in frames, frame rate, its first picture.
pub struct BoardShot {
    pub name: String,
    pub notes: String,
    pub frames: usize,
    pub fps: f32,
    pub picture: (Vec<u8>, usize, usize),
}

fn html_escape(t: &str) -> String {
    t.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\n', "<br>")
}

/// Write a storyboard page: `index.html` and one picture per shot in `dir`. Opens in any browser
/// and prints to PDF from there.
pub fn export_storyboard_html(title: &str, shots: &[BoardShot], dir: &Path) -> Result<std::path::PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let total: usize = shots.iter().map(|s| s.frames).sum();
    let fps = shots.first().map(|s| s.fps).unwrap_or(24.0).max(1.0);
    let mut cards = String::new();
    for (i, s) in shots.iter().enumerate() {
        let (px, w, h) = &s.picture;
        let img = image::RgbaImage::from_raw(*w as u32, *h as u32, px.clone()).ok_or("bad picture")?;
        let k = (640.0 / (*w).max(*h) as f32).min(1.0);
        let (tw, th) = (((*w as f32 * k) as u32).max(1), ((*h as f32 * k) as u32).max(1));
        let small = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
        let file = format!("shot-{:03}.png", i + 1);
        small.save(dir.join(&file)).map_err(|e| e.to_string())?;
        cards.push_str(&format!(
            "<figure class=\"card\"><img src=\"{file}\" alt=\"\"><figcaption><b>{} · {}</b><span>{} · {:.1} s</span><p>{}</p></figcaption></figure>\n",
            i + 1,
            html_escape(&s.name),
            crate::i18n::trf("{} images", &[&s.frames]),
            s.frames as f32 / s.fps.max(1.0),
            html_escape(&s.notes)
        ));
    }
    let html = format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{t}</title><style>\n\
         body{{font-family:system-ui,sans-serif;margin:24px;color:#222}}h1{{margin:0 0 4px}}.sub{{color:#666;margin:0 0 20px}}\n\
         .grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:18px}}\n\
         .card{{margin:0;border:1px solid #ddd;border-radius:8px;overflow:hidden;break-inside:avoid}}\n\
         .card img{{width:100%;display:block;background:#fff}}figcaption{{padding:8px 10px}}\n\
         figcaption span{{float:right;color:#777;font-size:.9em}}figcaption p{{margin:6px 0 0;font-size:.92em;white-space:normal}}\n\
         @media print{{body{{margin:8mm}}}}</style></head><body>\n<h1>{t}</h1><p class=\"sub\">{sub}</p>\n<div class=\"grid\">\n{cards}</div></body></html>\n",
        t = html_escape(title),
        sub = crate::i18n::trf("{} plans · {} images · {} s", &[&shots.len(), &total, &format!("{:.1}", total as f32 / fps)]),
        cards = cards
    );
    let index = dir.join("index.html");
    std::fs::write(&index, html).map_err(|e| e.to_string())?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::{Settings, Stroke, Tool};

    fn small_doc() -> Doc {
        let mut d = Doc::new(64, 48, 12.0);
        d.set_length(4);
        for f in 0..4 {
            let id = if f == 0 { d.layers[0].cels[0].unwrap() } else { d.new_drawing() };
            if f != 2 {
                d.layers[0].cels[f] = Some(id);
            }
            let mut st = Stroke::begin(
                id,
                &d.drawings[id].px,
                64,
                48,
                Tool::Pen,
                [200, 0, 0],
                Settings::new(4.0, 100.0, 100.0),
            );
            st.to(10.0 + f as f32 * 10.0, 20.0, &mut d.drawings[id].px);
        }
        d
    }

    #[test]
    fn exports_work() {
        let dir = std::env::temp_dir().join(format!("toonlight-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = small_doc();
        let json = export_after_effects(&d, &dir, "test").unwrap();
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&json).unwrap()).unwrap();
        let layer = &v["project"]["clip"]["layers"][0];
        assert_eq!(v["project"]["clip"]["image-count"], 4);
        assert_eq!(layer["link"].as_array().unwrap().len(), 3);
        assert_eq!(layer["link"][1]["images"], serde_json::json!([1, 2]));
        assert!(dir.join(layer["link"][0]["file"].as_str().unwrap()).is_file());
        export_gif(&d, &dir.join("t.gif"), 64).unwrap();
        assert!(std::fs::metadata(dir.join("t.gif")).unwrap().len() > 100);
        save(&d, &dir.join("t.toonlight")).unwrap();
        // other picture formats
        let pics = dir.join("pics");
        std::fs::create_dir_all(&pics).unwrap();
        for f in ["tiff", "tga", "jpeg", "bmp"] {
            assert_eq!(export_frames_as(&d, &pics, f).unwrap(), d.frames);
        }
        assert!(load_picture(&pics.join("image_0001.tif")).is_ok());
        assert!(load_picture(&pics.join("image_0001.tga")).is_ok());
        // "Save" writes in place only to an existing project file
        assert!(saves_in_place(&dir.join("t.toonlight")));
        assert!(!saves_in_place(&dir.join("missing.toonlight")));
        assert!(!saves_in_place(&dir.join("t.atanim")));
        // a free name: the next number in brackets after the existing file
        assert_eq!(free_name(&dir.join("t.toonlight")), dir.join("t(2).toonlight"));
        assert_eq!(free_name(&dir.join("t(7)")), dir.join("t(2).toonlight"));
        assert_eq!(free_name(&dir.join("new")), dir.join("new.toonlight"));
        let d2 = duplicate(&d).unwrap();
        save_project(&[(&d, "One", "notes"), (&d2, "Two", "")], &dir.join("p.toonlight")).unwrap();
        let clips = open_project(&dir.join("p.toonlight")).unwrap();
        assert_eq!(clips.len(), 2);
        assert_eq!(clips[1].name, "Two");
        assert_eq!(clips[0].notes, "notes");
        assert_eq!(clips[1].doc.keys(0).len(), 3);
        assert_eq!(export_film_frames(&[&clips[0].doc, &clips[1].doc], &dir).unwrap(), 8);
        let back = open(&dir.join("t.toonlight")).unwrap();
        assert_eq!(back.frames, 4);
        assert_eq!(back.keys(0).len(), 3);
    }

    #[test]
    fn sounds_are_saved_and_placed_in_the_film() {
        use crate::sound::SoundTrack;
        let dir = std::env::temp_dir().join(format!("toonlight-sound-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut d = small_doc();
        let mut a = SoundTrack::new("a.wav".into());
        a.offset = 0.5;
        a.volume = 0.8;
        let mut b = SoundTrack::new("b.wav".into());
        b.offset = -1.0;
        b.fade_in = 0.25;
        b.fade_out = 0.5;
        d.sounds = vec![a.clone(), b.clone()];
        save(&d, &dir.join("s.toonlight")).unwrap();
        let back = open(&dir.join("s.toonlight")).unwrap();
        assert_eq!(back.sounds, vec![a.clone(), b.clone()], "every track comes back");
        // a file from before several tracks: only "sound"
        let old: Header = serde_json::from_str(
            r#"{"w":4,"h":4,"fps":12.0,"frames":1,"paper":[255,255,255],"light":{"active":true,"prev_count":2,"next_count":2,"prev_color":[0,192,0],"next_color":[255,128,0],"use_prev_color":true,"use_next_color":true,"front":false,"opacity":100.0,"ranges":[50.0,30.0,15.0,0.0,0.0,0.0,0.0,0.0,0.0,0.0]},"layers":[],"drawings":0,"sound":{"path":"old.wav"}}"#,
        )
        .unwrap();
        assert!(old.sounds.is_empty() && old.sound.unwrap().path.ends_with("old.wav"));
        // the second shot starts 2 s into the film and lasts 3 s
        let s = PlacedSound {
            track: &a,
            start: 2.0,
            length: 3.0,
        };
        assert_eq!(
            sound_filter(&s, 1, None),
            "[1:a]volume=0.8,atrim=end=2.5,aformat=sample_rates=48000:channel_layouts=stereo,adelay=2500:all=1[a1]"
        );
        let s = PlacedSound {
            track: &b,
            start: 0.0,
            length: 3.0,
        };
        assert_eq!(
            sound_filter(&s, 2, Some(4.0)),
            "[2:a]afade=t=in:st=0:d=0.25,afade=t=out:st=3.5:d=0.5,atrim=start=1,asetpts=PTS-STARTPTS,volume=1,atrim=end=3,aformat=sample_rates=48000:channel_layouts=stereo[a2]"
        );
        // sounds whose file is missing are left out: only the pictures are encoded
        let args = av_args(&[s], 3.0);
        assert!(args.contains(&"-vf".to_string()) && !args.contains(&"-filter_complex".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spacing_and_strength_follow_the_sheet() {
        use crate::brush::{dab_spacing, dab_strength};
        assert_eq!(dab_spacing(Tool::Airbrush, 10.0), 1.0);
        assert_eq!(dab_spacing(Tool::Pencil, 30.0), 3.0);
        assert_eq!(dab_spacing(Tool::Airbrush, 150.0), 18.0);
        assert!((dab_strength(30.0) - 0.09).abs() < 1e-6);
    }
}
