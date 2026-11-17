//! Photoshop (PSD) files with layers: writing one frame of a project, and reading the layers of a
//! PSD back. Our own reader and writer of the public format (8-bit RGB; raw or PackBits channels).

use crate::doc::{Blend, Doc};
use std::io::Write;
use std::path::Path;

/// One layer of a PSD: name, picture (straight RGBA, `w * h`), placement, opacity 0..=255, shown.
pub struct PsdLayer {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    pub opacity: u8,
    pub visible: bool,
    pub blend: Blend,
}

fn key(b: Blend) -> &'static [u8; 4] {
    match b {
        Blend::Color | Blend::Erase => b"norm",
        Blend::Multiply => b"mul ",
        Blend::Screen => b"scrn",
        Blend::Add => b"lddg",
        Blend::Overlay => b"over",
        Blend::Darken => b"dark",
        Blend::Lighten => b"lite",
        Blend::Difference => b"diff",
    }
}

fn blend_of(k: &[u8]) -> Blend {
    match k {
        b"mul " => Blend::Multiply,
        b"scrn" => Blend::Screen,
        b"lddg" => Blend::Add,
        b"over" => Blend::Overlay,
        b"dark" => Blend::Darken,
        b"lite" => Blend::Lighten,
        b"diff" => Blend::Difference,
        _ => Blend::Color,
    }
}

/// Write frame `frame` of `doc` as a PSD: a paper layer at the bottom, then every layer with its
/// name, opacity, visibility and blending (erase becomes normal), and the finished picture.
pub fn export(doc: &Doc, frame: usize, path: &Path) -> Result<(), String> {
    let (w, h) = (doc.w, doc.h);
    let mut layers = vec![PsdLayer {
        name: crate::i18n::tr("Papier").into(),
        x: 0,
        y: 0,
        w,
        h,
        rgba: std::iter::repeat_n([doc.paper[0], doc.paper[1], doc.paper[2], 255], w * h)
            .flatten()
            .collect(),
        opacity: 255,
        visible: true,
        blend: Blend::Color,
    }];
    for (li, l) in doc.layers.iter().enumerate() {
        let rgba = match doc.shown_drawing(li, frame) {
            Some(id) => doc.drawings[id].px.clone(),
            None => vec![0; w * h * 4],
        };
        layers.push(PsdLayer {
            name: l.name.clone(),
            x: 0,
            y: 0,
            w,
            h,
            rgba,
            opacity: (l.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            visible: l.visible,
            blend: l.blend,
        });
    }
    let composite = doc.render(frame);
    let (cw, ch) = doc.output_size();
    let bytes = write(&layers, w, h, &composite, cw, ch);
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

/// The bytes of a PSD with these layers (bottom first) and this finished picture.
pub fn write(layers: &[PsdLayer], w: usize, h: usize, composite: &[u8], cw: usize, ch: usize) -> Vec<u8> {
    // the composite has to have the document's size; with a camera its size differs, so the
    // picture is then the paper-composited layers at canvas size
    let (comp, w2, h2) = if cw == w && ch == h {
        (composite.to_vec(), w, h)
    } else {
        (flatten(layers, w, h), w, h)
    };
    let mut out = vec![];
    let be16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_be_bytes());
    let be32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_be_bytes());
    out.extend_from_slice(b"8BPS");
    be16(&mut out, 1);
    out.extend_from_slice(&[0; 6]);
    be16(&mut out, 3); // composite channels: R, G, B
    be32(&mut out, h2 as u32);
    be32(&mut out, w2 as u32);
    be16(&mut out, 8);
    be16(&mut out, 3); // RGB
    be32(&mut out, 0); // colour mode data
    be32(&mut out, 0); // image resources
                       // layer and mask information
    let mut info = vec![];
    info.extend_from_slice(&(layers.len() as i16).to_be_bytes());
    let mut data = vec![];
    for l in layers {
        let n = l.w * l.h;
        info.extend_from_slice(&l.y.to_be_bytes());
        info.extend_from_slice(&l.x.to_be_bytes());
        info.extend_from_slice(&(l.y + l.h as i32).to_be_bytes());
        info.extend_from_slice(&(l.x + l.w as i32).to_be_bytes());
        be16(&mut info, 4);
        for id in [-1i16, 0, 1, 2] {
            info.extend_from_slice(&id.to_be_bytes());
            be32(&mut info, (2 + n) as u32);
        }
        info.extend_from_slice(b"8BIM");
        info.extend_from_slice(key(l.blend));
        info.push(l.opacity);
        info.push(0); // clipping
        info.push(if l.visible { 0 } else { 0x02 });
        info.push(0);
        let name = l.name.as_bytes();
        let name = &name[..name.len().min(255)];
        let mut pascal = vec![name.len() as u8];
        pascal.extend_from_slice(name);
        while pascal.len() % 4 != 0 {
            pascal.push(0);
        }
        be32(&mut info, (8 + pascal.len()) as u32);
        be32(&mut info, 0); // mask data
        be32(&mut info, 0); // blending ranges
        info.extend_from_slice(&pascal);
        for c in [3usize, 0, 1, 2] {
            be16(&mut data, 0);
            data.extend(l.rgba.chunks_exact(4).map(|p| p[c]));
        }
    }
    info.extend_from_slice(&data);
    if info.len() % 2 == 1 {
        info.push(0);
    }
    let mut section = vec![];
    be32(&mut section, info.len() as u32);
    section.extend_from_slice(&info);
    be32(&mut section, 0); // global layer mask
    be32(&mut out, section.len() as u32);
    out.extend_from_slice(&section);
    // the finished picture, raw, planar
    be16(&mut out, 0);
    for c in 0..3 {
        out.extend(comp.chunks_exact(4).map(|p| p[c]));
    }
    let _ = out.flush();
    out
}

/// Paper and layers composited (normal blending), for a composite of the canvas size.
fn flatten(layers: &[PsdLayer], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![255u8; w * h * 4];
    for l in layers.iter().filter(|l| l.visible) {
        for y in 0..l.h {
            for x in 0..l.w {
                let (tx, ty) = (l.x + x as i32, l.y + y as i32);
                if tx < 0 || ty < 0 || tx as usize >= w || ty as usize >= h {
                    continue;
                }
                let s = &l.rgba[(y * l.w + x) * 4..(y * l.w + x) * 4 + 4];
                let a = s[3] as u32 * l.opacity as u32 / 255;
                let d = &mut out[(ty as usize * w + tx as usize) * 4..(ty as usize * w + tx as usize) * 4 + 4];
                for k in 0..3 {
                    d[k] = ((s[k] as u32 * a + d[k] as u32 * (255 - a)) / 255) as u8;
                }
            }
        }
    }
    out
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.i + n > self.b.len() {
            return Err(crate::i18n::tr("Fichier PSD incomplet.").into());
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        let s = self.take(2)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(self.u16()? as i16)
    }
    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
}

/// PackBits run-length decoding of one channel's rows.
fn unpack(src: &[u8], out_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(out_len);
    let mut i = 0;
    while i < src.len() && out.len() < out_len {
        let n = src[i] as i8;
        i += 1;
        if n >= 0 {
            let k = n as usize + 1;
            let end = (i + k).min(src.len());
            out.extend_from_slice(&src[i..end]);
            i = end;
        } else if n != -128 {
            let k = (1 - n as i32) as usize;
            if i < src.len() {
                out.extend(std::iter::repeat_n(src[i], k));
            }
            i += 1;
        }
    }
    out.resize(out_len, 0);
    out
}

/// Read the layers of an 8-bit RGB PSD (raw or PackBits), bottom first, with the canvas size.
pub fn read(path: &Path) -> Result<(Vec<PsdLayer>, usize, usize), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut r = Reader { b: &bytes, i: 0 };
    if r.take(4)? != b"8BPS" || r.u16()? != 1 {
        return Err(crate::i18n::tr("Ce fichier n'est pas un PSD lisible (PSD version 1 seulement).").into());
    }
    r.take(6)?;
    let _channels = r.u16()?;
    let h = r.u32()? as usize;
    let w = r.u32()? as usize;
    let depth = r.u16()?;
    let mode = r.u16()?;
    if depth != 8 || mode != 3 {
        return Err(crate::i18n::tr("Seuls les PSD RVB 8 bits sont lus.").into());
    }
    let n = r.u32()? as usize;
    r.take(n)?;
    let n = r.u32()? as usize;
    r.take(n)?;
    let _section = r.u32()?;
    let info_len = r.u32()? as usize;
    if info_len == 0 {
        return Ok((vec![], w, h));
    }
    let count = r.i16()?.unsigned_abs() as usize;
    struct Rec {
        name: String,
        top: i32,
        left: i32,
        bottom: i32,
        right: i32,
        chans: Vec<(i16, usize)>,
        opacity: u8,
        visible: bool,
        blend: Blend,
    }
    let mut recs = vec![];
    for _ in 0..count {
        let (top, left, bottom, right) = (r.i32()?, r.i32()?, r.i32()?, r.i32()?);
        let nc = r.u16()? as usize;
        let mut chans = vec![];
        for _ in 0..nc {
            chans.push((r.i16()?, r.u32()? as usize));
        }
        r.take(4)?;
        let blend = blend_of(r.take(4)?);
        let opacity = r.u8()?;
        let _clip = r.u8()?;
        let flags = r.u8()?;
        let _ = r.u8()?;
        let extra = r.u32()? as usize;
        let start = r.i;
        let mask = r.u32()? as usize;
        r.take(mask)?;
        let ranges = r.u32()? as usize;
        r.take(ranges)?;
        let nl = r.u8()? as usize;
        let name = String::from_utf8_lossy(r.take(nl)?).to_string();
        r.i = start + extra;
        recs.push(Rec {
            name,
            top,
            left,
            bottom,
            right,
            chans,
            opacity,
            visible: flags & 0x02 == 0,
            blend,
        });
    }
    let mut layers = vec![];
    for rec in recs {
        let lw = (rec.right - rec.left).max(0) as usize;
        let lh = (rec.bottom - rec.top).max(0) as usize;
        let mut rgba = vec![255u8; lw * lh * 4];
        for (id, len) in &rec.chans {
            let data = r.take(*len)?;
            if lw == 0 || lh == 0 || data.len() < 2 {
                continue;
            }
            let comp = u16::from_be_bytes([data[0], data[1]]);
            let plane = match comp {
                0 => {
                    let mut p = data[2..].to_vec();
                    p.resize(lw * lh, 0);
                    p
                }
                1 => {
                    // row byte counts, then the rows
                    let rows = 2 + lh * 2;
                    let body = data.get(rows..).unwrap_or(&[]);
                    unpack(body, lw * lh)
                }
                _ => return Err(crate::i18n::tr("Compression PSD non prise en charge (seulement brut ou PackBits).").into()),
            };
            let c = match id {
                -1 => 3,
                0 => 0,
                1 => 1,
                2 => 2,
                _ => continue,
            };
            for (k, v) in plane.iter().enumerate().take(lw * lh) {
                rgba[k * 4 + c] = *v;
            }
        }
        // a layer without a transparency channel is opaque, as filled above
        layers.push(PsdLayer {
            name: rec.name,
            x: rec.left,
            y: rec.top,
            w: lw,
            h: lh,
            rgba,
            opacity: rec.opacity,
            visible: rec.visible,
            blend: rec.blend,
        });
    }
    Ok((layers, w, h))
}

/// Place a PSD layer's picture on a canvas of `w * h` (pixels outside are cut).
pub fn place(l: &PsdLayer, w: usize, h: usize) -> Vec<u8> {
    let mut px = vec![0u8; w * h * 4];
    for y in 0..l.h {
        let ty = l.y + y as i32;
        if ty < 0 || ty as usize >= h {
            continue;
        }
        for x in 0..l.w {
            let tx = l.x + x as i32;
            if tx < 0 || tx as usize >= w {
                continue;
            }
            let s = (y * l.w + x) * 4;
            let d = (ty as usize * w + tx as usize) * 4;
            px[d..d + 4].copy_from_slice(&l.rgba[s..s + 4]);
        }
    }
    px
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_survive_a_round_trip() {
        let mut d = Doc::new(3, 2, 12.0);
        d.layers[0].name = "Ink".into();
        d.layers[0].opacity = 0.5;
        let id = d.layers[0].cels[0].unwrap();
        d.drawings[id].px[0..4].copy_from_slice(&[10, 20, 30, 200]);
        let dir = std::env::temp_dir().join(format!("toonlight-psd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.psd");
        export(&d, 0, &p).unwrap();
        let (layers, w, h) = read(&p).unwrap();
        assert_eq!((w, h), (3, 2));
        assert_eq!(layers.len(), 2, "paper + one layer");
        assert_eq!(layers[1].name, "Ink");
        assert_eq!(layers[1].opacity, 128);
        assert_eq!(&layers[1].rgba[0..4], &[10, 20, 30, 200]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn packbits_decodes_runs_and_literals() {
        // 3 literal bytes, then a run of 4 × 9
        assert_eq!(unpack(&[2, 1, 2, 3, 0xFD, 9], 7), vec![1, 2, 3, 9, 9, 9, 9]);
    }
}
