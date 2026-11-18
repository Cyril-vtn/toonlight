//! Storyboard sheets: a project's shots laid out on pages in a chosen style and written as a PDF
//! file (pages drawn as pictures, so any language prints the same everywhere), and the list of
//! shots as an EDL for editing programs. The web page export (`io::export_storyboard_html`)
//! follows the same options.

use crate::i18n::{tr, trf};
use crate::io::BoardShot;
use ab_glyph::{Font, FontVec, PxScale, ScaleFont};
use std::path::Path;

/// How the shots are laid out on a storyboard page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardStyle {
    /// Cards side by side, several rows per page.
    Grid,
    /// One shot per line: the picture on the left, the text beside it.
    Rows,
    /// One shot per page, as large as the page allows.
    Single,
}

pub const ALL_STYLES: [BoardStyle; 3] = [BoardStyle::Grid, BoardStyle::Rows, BoardStyle::Single];

impl BoardStyle {
    pub fn name(self) -> &'static str {
        match self {
            BoardStyle::Grid => "grid",
            BoardStyle::Rows => "rows",
            BoardStyle::Single => "single",
        }
    }

    pub fn from_name(n: &str) -> Option<BoardStyle> {
        ALL_STYLES.into_iter().find(|s| s.name() == n)
    }

    pub fn label(self) -> &'static str {
        match self {
            BoardStyle::Grid => tr("Grille"),
            BoardStyle::Rows => tr("Un plan par ligne"),
            BoardStyle::Single => tr("Un plan par page"),
        }
    }
}

/// What a storyboard export looks like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoardOptions {
    pub style: BoardStyle,
    /// Cards per row in the grid style (2 to 6).
    pub columns: usize,
    pub landscape: bool,
    /// US Letter paper instead of A4.
    pub letter: bool,
    /// Write each shot's notes.
    pub notes: bool,
    /// Write each shot's length and the time it starts at.
    pub timing: bool,
}

impl Default for BoardOptions {
    fn default() -> Self {
        BoardOptions {
            style: BoardStyle::Grid,
            columns: 3,
            landscape: false,
            letter: false,
            notes: true,
            timing: true,
        }
    }
}

impl BoardOptions {
    /// The page's width and height in points (1/72 inch).
    pub fn page(&self) -> (f32, f32) {
        let (w, h) = if self.letter { (612.0, 792.0) } else { (595.28, 841.89) };
        if self.landscape {
            (h, w)
        } else {
            (w, h)
        }
    }
}

/// A shot's timing line: its length in frames and seconds, and when it starts in the film.
pub fn timing_line(shots: &[BoardShot], i: usize) -> String {
    let fps = shots.first().map(|s| s.fps).unwrap_or(24.0).max(1.0);
    let start: usize = shots[..i].iter().map(|s| s.frames).sum();
    let s = &shots[i];
    format!(
        "{} · {:.1} s · {}",
        trf("{} images", &[&s.frames]),
        s.frames as f32 / s.fps.max(1.0),
        trf("début {}", &[&timecode(start, fps)])
    )
}

/// The line under the title: how many shots, frames and seconds the film has.
pub fn summary_line(shots: &[BoardShot]) -> String {
    let total: usize = shots.iter().map(|s| s.frames).sum();
    let fps = shots.first().map(|s| s.fps).unwrap_or(24.0).max(1.0);
    trf(
        "{} plans · {} images · {} s",
        &[&shots.len(), &total, &format!("{:.1}", total as f32 / fps)],
    )
}

/// A frame as an editing timecode, hours:minutes:seconds:frames, at a whole frame rate (23.976
/// counts as 24, without dropped frames).
pub fn timecode(frame: usize, fps: f32) -> String {
    let rate = (fps.round() as usize).max(1);
    let secs = frame / rate;
    format!("{:02}:{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60, frame % rate)
}

/// The list of shots as an edit decision list (CMX 3600): one cut per shot, in the film's order.
pub fn edl(title: &str, shots: &[BoardShot]) -> String {
    let fps = shots.first().map(|s| s.fps).unwrap_or(24.0).max(1.0);
    // the format is plain ASCII: accented letters lose their accent, anything else becomes "_"
    let plain = |t: &str| t.chars().map(ascii).collect::<String>();
    let mut out = format!("TITLE: {}\nFCM: NON-DROP FRAME\n\n", plain(title));
    let mut at = 0;
    for (i, s) in shots.iter().enumerate() {
        let (a, b) = (timecode(at, fps), timecode(at + s.frames, fps));
        out.push_str(&format!(
            "{:03}  AX       V     C        {} {} {} {}\n",
            i + 1,
            timecode(0, fps),
            timecode(s.frames, fps),
            a,
            b
        ));
        out.push_str(&format!("* FROM CLIP NAME: {}\n", plain(&s.name)));
        if let Some(note) = s.notes.lines().next().filter(|l| !l.trim().is_empty()) {
            out.push_str(&format!("* COMMENT: {}\n", plain(note)));
        }
        out.push('\n');
        at += s.frames;
    }
    out
}

/// A letter as plain ASCII: itself, its unaccented form (é → e, œ → oe), or "_".
fn ascii(c: char) -> String {
    if c.is_ascii() && !c.is_ascii_control() {
        return c.to_string();
    }
    let groups = [
        ("a", "àáâãäå"),
        ("c", "ç"),
        ("e", "èéêë"),
        ("i", "ìíîï"),
        ("n", "ñ"),
        ("o", "òóôõöø"),
        ("u", "ùúûü"),
        ("y", "ýÿ"),
        ("ae", "æ"),
        ("oe", "œ"),
        ("ss", "ß"),
    ];
    let lower: String = c.to_lowercase().collect();
    match groups.iter().find(|g| g.1.contains(lower.as_str())) {
        Some(g) if c.is_uppercase() => g.0.to_uppercase(),
        Some(g) => g.0.to_string(),
        None => "_".into(),
    }
}

// ---------- page layout ----------

/// Something drawn on a page, in points from the page's top-left corner.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A shot's picture, fitted inside the box [x, y, width, height].
    Picture { shot: usize, rect: [f32; 4] },
    /// A line of text: its left end, its baseline, the letter size and how dark it is (0 black).
    Text { x: f32, y: f32, size: f32, text: String, gray: u8 },
    /// A thin horizontal rule.
    Rule { x: f32, y: f32, width: f32 },
}

/// Lay the shots out on pages. `measure` gives the width of a text at a letter size, in points.
pub fn layout(title: &str, shots: &[BoardShot], o: &BoardOptions, measure: &dyn Fn(&str, f32) -> f32) -> Vec<Vec<Item>> {
    const MARGIN: f32 = 36.0;
    const HEAD: f32 = 44.0;
    const GAP: f32 = 12.0;
    let (pw, ph) = o.page();
    let (cw, ch) = (pw - 2.0 * MARGIN, ph - 2.0 * MARGIN - HEAD - 14.0);
    let top = MARGIN + HEAD;
    // the pictures' shape: the first shot's
    let aspect = shots
        .first()
        .map(|s| s.picture.2 as f32 / s.picture.1.max(1) as f32)
        .unwrap_or(0.5625);
    // where each shot goes: (page, items)
    let mut pages: Vec<Vec<Item>> = vec![];
    let mut place = |page: usize, items: Vec<Item>| {
        while pages.len() <= page {
            pages.push(vec![]);
        }
        pages[page].extend(items);
    };
    let text = |x: f32, y: f32, size: f32, t: &str, gray: u8| Item::Text {
        x,
        y,
        size,
        text: t.to_string(),
        gray,
    };
    for (i, s) in shots.iter().enumerate() {
        let head = format!("{} · {}", i + 1, s.name);
        let timing = timing_line(shots, i);
        match o.style {
            BoardStyle::Grid => {
                let cols = o.columns.clamp(2, 6);
                let card_w = (cw - GAP * (cols - 1) as f32) / cols as f32;
                let pic_h = card_w * aspect;
                let note_lines = if o.notes { 3 } else { 0 };
                let card_h = pic_h + 16.0 + if o.timing { 10.0 } else { 0.0 } + note_lines as f32 * 9.5 + 6.0;
                let rows = (((ch + GAP) / (card_h + GAP)).floor() as usize).max(1);
                let (page, k) = (i / (cols * rows), i % (cols * rows));
                let x = MARGIN + (k % cols) as f32 * (card_w + GAP);
                let y = top + (k / cols) as f32 * (card_h + GAP);
                let mut items = vec![Item::Picture {
                    shot: i,
                    rect: [x, y, card_w, pic_h],
                }];
                let mut ty = y + pic_h + 12.0;
                items.push(text(x, ty, 9.0, &fit(&head, card_w, 9.0, measure), 30));
                if o.timing {
                    ty += 10.0;
                    items.push(text(x, ty, 7.0, &fit(&timing, card_w, 7.0, measure), 110));
                }
                for line in wrap(&s.notes, card_w, 7.5, measure).into_iter().take(note_lines) {
                    ty += 9.5;
                    items.push(text(x, ty, 7.5, &line, 60));
                }
                place(page, items);
            }
            BoardStyle::Rows => {
                let pic_w = cw * 0.36;
                let pic_h = pic_w * aspect;
                let row_h = pic_h.max(60.0);
                let rows = (((ch + GAP) / (row_h + GAP)).floor() as usize).max(1);
                let (page, k) = (i / rows, i % rows);
                let y = top + k as f32 * (row_h + GAP);
                let tx = MARGIN + pic_w + 14.0;
                let tw = cw - pic_w - 14.0;
                let mut items = vec![Item::Picture {
                    shot: i,
                    rect: [MARGIN, y, pic_w, pic_h],
                }];
                let mut ty = y + 11.0;
                items.push(text(tx, ty, 11.0, &fit(&head, tw, 11.0, measure), 30));
                if o.timing {
                    ty += 12.0;
                    items.push(text(tx, ty, 8.0, &fit(&timing, tw, 8.0, measure), 110));
                }
                if o.notes {
                    ty += 4.0;
                    let room = ((y + row_h - ty) / 11.5).floor().max(0.0) as usize;
                    for line in wrap(&s.notes, tw, 9.0, measure).into_iter().take(room) {
                        ty += 11.5;
                        items.push(text(tx, ty, 9.0, &line, 60));
                    }
                }
                items.push(Item::Rule {
                    x: MARGIN,
                    y: y + row_h + GAP / 2.0,
                    width: cw,
                });
                place(page, items);
            }
            BoardStyle::Single => {
                let pic_h = (cw * aspect).min(ch * 0.68);
                let mut items = vec![Item::Picture {
                    shot: i,
                    rect: [MARGIN, top, cw, pic_h],
                }];
                let mut ty = top + pic_h + 20.0;
                items.push(text(MARGIN, ty, 14.0, &fit(&head, cw, 14.0, measure), 30));
                if o.timing {
                    ty += 14.0;
                    items.push(text(MARGIN, ty, 9.0, &fit(&timing, cw, 9.0, measure), 110));
                }
                if o.notes {
                    ty += 6.0;
                    let room = ((top + ch - ty) / 14.0).floor().max(0.0) as usize;
                    for line in wrap(&s.notes, cw, 11.0, measure).into_iter().take(room) {
                        ty += 14.0;
                        items.push(text(MARGIN, ty, 11.0, &line, 60));
                    }
                }
                place(i, items);
            }
        }
    }
    if pages.is_empty() {
        pages.push(vec![]);
    }
    // every page: the title, the summary under it, and its number at the bottom
    let n = pages.len();
    let summary = summary_line(shots);
    for (k, page) in pages.iter_mut().enumerate() {
        page.push(text(MARGIN, MARGIN + 16.0, 16.0, &fit(title, cw, 16.0, measure), 20));
        page.push(text(MARGIN, MARGIN + 30.0, 8.5, &fit(&summary, cw, 8.5, measure), 110));
        page.push(Item::Rule {
            x: MARGIN,
            y: MARGIN + 36.0,
            width: cw,
        });
        let number = format!("{} / {}", k + 1, n);
        let w = measure(&number, 8.0);
        page.push(text(pw - MARGIN - w, ph - MARGIN + 6.0, 8.0, &number, 110));
    }
    pages
}

/// A text cut with "…" when it is wider than `width`.
fn fit(t: &str, width: f32, size: f32, measure: &dyn Fn(&str, f32) -> f32) -> String {
    let line = t.lines().next().unwrap_or("");
    if measure(line, size) <= width {
        return line.to_string();
    }
    let mut out = String::new();
    for c in line.chars() {
        out.push(c);
        if measure(&format!("{out}…"), size) > width {
            out.pop();
            break;
        }
    }
    format!("{}…", out.trim_end())
}

/// A text broken into lines no wider than `width`: at spaces, or inside a word that is too long
/// by itself (and in writing without spaces).
pub fn wrap(t: &str, width: f32, size: f32, measure: &dyn Fn(&str, f32) -> f32) -> Vec<String> {
    let mut lines = vec![];
    for para in t.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if measure(&candidate, size) <= width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // the word alone: whole if it fits, else cut letter by letter
            for c in word.chars() {
                line.push(c);
                if measure(&line, size) > width && line.chars().count() > 1 {
                    line.pop();
                    lines.push(std::mem::take(&mut line));
                    line.push(c);
                }
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

// ---------- drawing the pages ----------

/// The fonts a sheet is written with: the interface's own, then one with Chinese and Japanese
/// characters when the computer has one. A letter is drawn with the first font that has it.
pub fn sheet_fonts() -> Vec<FontVec> {
    let mut fonts: Vec<FontVec> = crate::text::default_font().into_iter().collect();
    let japanese = crate::i18n::lang() == crate::i18n::Lang::Ja;
    if let Some((path, index)) = crate::i18n::find_cjk_font(japanese) {
        fonts.extend(crate::text::font_from_file(&path, index));
    }
    fonts
}

/// The font that has a letter (the first one when none does).
fn font_for(fonts: &[FontVec], c: char) -> Option<&FontVec> {
    fonts.iter().find(|f| f.glyph_id(c).0 != 0).or(fonts.first())
}

/// Letter height in pixels for a size in points: sizes are "font sizes" (the em), which is a
/// little less than the full height of a line.
fn px_scale(font: &FontVec, size: f32) -> PxScale {
    let em = font.units_per_em().unwrap_or(1000.0).max(1.0);
    PxScale::from(size * font.height_unscaled() / em)
}

/// Width of a text at a letter size (in the size's own unit).
pub fn text_width(fonts: &[FontVec], t: &str, size: f32) -> f32 {
    t.chars()
        .filter_map(|c| {
            let f = font_for(fonts, c)?;
            Some(f.as_scaled(px_scale(f, size)).h_advance(f.glyph_id(c)))
        })
        .sum()
}

/// An opaque RGB picture being drawn.
struct Sheet {
    px: Vec<u8>,
    w: usize,
    h: usize,
}

impl Sheet {
    fn fill(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, gray: u8) {
        let (x0, x1) = (
            (x0.round().max(0.0) as usize).min(self.w),
            (x1.round().max(0.0) as usize).min(self.w),
        );
        let (y0, y1) = (
            (y0.round().max(0.0) as usize).min(self.h),
            (y1.round().max(0.0) as usize).min(self.h),
        );
        for y in y0..y1 {
            self.px[(y * self.w + x0) * 3..(y * self.w + x1) * 3].fill(gray);
        }
    }

    fn text(&mut self, fonts: &[FontVec], mut x: f32, y: f32, size: f32, t: &str, gray: u8) {
        for c in t.chars() {
            let Some(f) = font_for(fonts, c) else { return };
            let scale = px_scale(f, size);
            let id = f.glyph_id(c);
            if let Some(og) = f.outline_glyph(id.with_scale_and_position(scale, ab_glyph::point(x, y))) {
                let b = og.px_bounds();
                og.draw(|gx, gy, v| {
                    let (px, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                    if px >= 0 && py >= 0 && (px as usize) < self.w && (py as usize) < self.h {
                        let i = (py as usize * self.w + px as usize) * 3;
                        for k in 0..3 {
                            let old = self.px[i + k] as f32;
                            self.px[i + k] = (old + (gray as f32 - old) * v.clamp(0.0, 1.0)).round() as u8;
                        }
                    }
                });
            }
            x += f.as_scaled(scale).h_advance(id);
        }
    }

    /// A shot's picture fitted inside a box and centred there, with a thin frame.
    fn picture(&mut self, shot: &BoardShot, rect: [f32; 4]) {
        let (px, w, h) = &shot.picture;
        let Some(img) = image::RgbaImage::from_raw(*w as u32, *h as u32, px.clone()) else {
            return;
        };
        let k = (rect[2] / *w as f32).min(rect[3] / *h as f32);
        let (tw, th) = (((*w as f32 * k).round() as u32).max(1), ((*h as f32 * k).round() as u32).max(1));
        let small = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
        let x0 = (rect[0] + (rect[2] - tw as f32) / 2.0).round().max(0.0) as usize;
        let y0 = (rect[1] + (rect[3] - th as f32) / 2.0).round().max(0.0) as usize;
        self.fill(
            x0 as f32 - 1.0,
            y0 as f32 - 1.0,
            (x0 + tw as usize) as f32 + 1.0,
            (y0 + th as usize) as f32 + 1.0,
            150,
        );
        for y in 0..th as usize {
            for x in 0..tw as usize {
                if x0 + x < self.w && y0 + y < self.h {
                    let p = small.get_pixel(x as u32, y as u32).0;
                    let i = ((y0 + y) * self.w + x0 + x) * 3;
                    self.px[i..i + 3].copy_from_slice(&p[..3]);
                }
            }
        }
    }
}

/// Draw a laid-out page as an RGB picture at `dpi` dots per inch.
pub fn draw_page(items: &[Item], shots: &[BoardShot], page: (f32, f32), dpi: f32, fonts: &[FontVec]) -> (Vec<u8>, usize, usize) {
    let k = dpi / 72.0;
    let (w, h) = ((page.0 * k).round() as usize, (page.1 * k).round() as usize);
    let mut sheet = Sheet {
        px: vec![255; w * h * 3],
        w,
        h,
    };
    for item in items {
        match item {
            Item::Picture { shot, rect } => {
                if let Some(s) = shots.get(*shot) {
                    sheet.picture(s, rect.map(|v| v * k));
                }
            }
            Item::Text { x, y, size, text, gray } => sheet.text(fonts, x * k, y * k, size * k, text, *gray),
            Item::Rule { x, y, width } => sheet.fill(x * k, y * k, (x + width) * k, y * k + (0.6 * k).max(1.0), 200),
        }
    }
    (sheet.px, w, h)
}

// ---------- PDF ----------

/// A PDF file whose pages are JPEG pictures (bytes, width, height in pixels), each filling a
/// page of `page` points.
pub fn pdf_from_jpegs(pages: &[(Vec<u8>, usize, usize)], page: (f32, f32), title: &str) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    // objects: 1 catalog, 2 page list, 3 information, then three per page (page, drawing, picture)
    let count = 3 + pages.len() * 3;
    let mut offsets = vec![0usize; count + 1];
    let mut object = |out: &mut Vec<u8>, n: usize, body: &[u8]| {
        offsets[n] = out.len();
        out.extend(format!("{n} 0 obj\n").as_bytes());
        out.extend(body);
        out.extend(b"\nendobj\n");
    };
    object(&mut out, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..pages.len()).map(|i| format!("{} 0 R", 4 + i * 3)).collect();
    object(
        &mut out,
        2,
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).as_bytes(),
    );
    // the title as UTF-16, so any language is kept
    let hex: String = std::iter::once(0xFEFFu16)
        .chain(title.encode_utf16())
        .map(|u| format!("{u:04X}"))
        .collect();
    object(&mut out, 3, format!("<< /Title <{hex}> /Producer (Toonlight) >>").as_bytes());
    for (i, (jpeg, w, h)) in pages.iter().enumerate() {
        let n = 4 + i * 3;
        object(
            &mut out,
            n,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /XObject << /Im0 {} 0 R >> >> /Contents {} 0 R >>",
                page.0,
                page.1,
                n + 2,
                n + 1
            )
            .as_bytes(),
        );
        let draw = format!("q {:.2} 0 0 {:.2} 0 0 cm /Im0 Do Q", page.0, page.1);
        object(
            &mut out,
            n + 1,
            format!("<< /Length {} >>\nstream\n{draw}\nendstream", draw.len()).as_bytes(),
        );
        let mut body = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpeg.len()
        )
        .into_bytes();
        body.extend(jpeg);
        body.extend(b"\nendstream");
        object(&mut out, n + 2, &body);
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", count + 1).as_bytes());
    for o in &offsets[1..] {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            count + 1
        )
        .as_bytes(),
    );
    out
}

/// Dots per inch of the PDF's pages: sharp in print, light enough to send.
const PDF_DPI: f32 = 150.0;

/// Write the storyboard as a PDF file. Returns how many pages it has.
pub fn export_pdf(title: &str, shots: &[BoardShot], o: &BoardOptions, path: &Path) -> Result<usize, String> {
    let fonts = sheet_fonts();
    if fonts.is_empty() {
        return Err("no font to write the sheet with".into());
    }
    let measure = |t: &str, size: f32| text_width(&fonts, t, size);
    let pages = layout(title, shots, o, &measure);
    let mut jpegs = vec![];
    for items in &pages {
        let (px, w, h) = draw_page(items, shots, o.page(), PDF_DPI, &fonts);
        let mut jpeg = vec![];
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
            .encode(&px, w as u32, h as u32, image::ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())?;
        jpegs.push((jpeg, w, h));
    }
    std::fs::write(path, pdf_from_jpegs(&jpegs, o.page(), title)).map_err(|e| e.to_string())?;
    Ok(pages.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shots(n: usize) -> Vec<BoardShot> {
        (0..n)
            .map(|i| BoardShot {
                name: format!("Shot {}", i + 1),
                notes: "A long note that has to be broken into several lines to fit under its picture.".into(),
                frames: 24,
                fps: 24.0,
                picture: (vec![200; 32 * 18 * 4], 32, 18),
            })
            .collect()
    }

    /// Every letter is as wide as half its size: enough to test the layout without a font.
    fn measure(t: &str, size: f32) -> f32 {
        t.chars().count() as f32 * size * 0.5
    }

    #[test]
    fn timecodes_and_edl() {
        assert_eq!(timecode(0, 24.0), "00:00:00:00");
        assert_eq!(timecode(24 * 3661 + 5, 24.0), "01:01:01:05");
        assert_eq!(timecode(30, 23.976), "00:00:01:06", "23.976 counts as 24");
        let mut s = shots(2);
        s[1].name = "Forêt 2".into();
        s[1].notes = String::new();
        let e = edl("Film", &s);
        assert!(e.starts_with("TITLE: Film\nFCM: NON-DROP FRAME\n"));
        assert!(e.contains("001  AX       V     C        00:00:00:00 00:00:01:00 00:00:00:00 00:00:01:00\n"));
        assert!(e.contains("002  AX       V     C        00:00:00:00 00:00:01:00 00:00:01:00 00:00:02:00\n"));
        assert!(e.contains("* FROM CLIP NAME: Foret 2\n"), "plain ASCII only: {e}");
        assert_eq!("Élan cœur 森 ß".chars().map(ascii).collect::<String>(), "Elan coeur _ ss");
        assert_eq!(e.matches("* COMMENT:").count(), 1, "a shot without notes has no comment");
    }

    #[test]
    fn text_is_wrapped_and_cut() {
        assert_eq!(wrap("one two three", 20.0, 8.0, &measure), vec!["one", "two", "three"]);
        assert_eq!(wrap("one two three", 28.0, 8.0, &measure), vec!["one two", "three"]);
        assert_eq!(wrap("a\n\nb", 100.0, 8.0, &measure), vec!["a", "b"]);
        // a word too long by itself is cut; so is writing without spaces
        assert_eq!(wrap("abcdefgh", 16.0, 8.0, &measure), vec!["abcd", "efgh"]);
        assert_eq!(fit("abcdefgh", 16.0, 8.0, &measure), "abc…");
        assert_eq!(fit("abc", 16.0, 8.0, &measure), "abc");
    }

    #[test]
    fn styles_fill_their_pages() {
        let s = shots(14);
        let pictures = |page: &Vec<Item>| page.iter().filter(|i| matches!(i, Item::Picture { .. })).count();
        for (style, per_page) in [(BoardStyle::Grid, 12), (BoardStyle::Rows, 6), (BoardStyle::Single, 1)] {
            let o = BoardOptions {
                style,
                ..Default::default()
            };
            let pages = layout("Film", &s, &o, &measure);
            assert_eq!(pictures(&pages[0]), per_page, "{style:?}");
            assert_eq!(pages.iter().map(pictures).sum::<usize>(), 14, "every shot is there");
            assert_eq!(pages.len(), 14usize.div_ceil(per_page));
            // everything stays on the page, and each page has its title and number
            let (pw, ph) = o.page();
            for (k, page) in pages.iter().enumerate() {
                for item in page {
                    let (x, y, w, h) = match item {
                        Item::Picture { rect, .. } => (rect[0], rect[1], rect[2], rect[3]),
                        Item::Text { x, y, size, text, .. } => (*x, *y - size, measure(text, *size), *size),
                        Item::Rule { x, y, width } => (*x, *y, *width, 1.0),
                    };
                    assert!(x >= 0.0 && y >= 0.0 && x + w <= pw + 0.5 && y + h <= ph + 0.5, "{style:?} {item:?}");
                }
                let number = format!("{} / {}", k + 1, pages.len());
                assert!(page.iter().any(|i| matches!(i, Item::Text { text, .. } if *text == number)));
                assert!(page.iter().any(|i| matches!(i, Item::Text { text, .. } if text == "Film")));
            }
        }
        // options: more columns, no notes, no timing, the page turned
        let o = BoardOptions {
            columns: 5,
            notes: false,
            timing: false,
            landscape: true,
            letter: true,
            ..Default::default()
        };
        assert_eq!(o.page(), (792.0, 612.0));
        let pages = layout("Film", &s, &o, &measure);
        let texts: Vec<&str> = pages[0]
            .iter()
            .filter_map(|i| match i {
                Item::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(!texts.iter().any(|t| t.contains("note")), "{texts:?}");
        assert!(pictures(&pages[0]) >= 10);
        assert_eq!(layout("Film", &[], &o, &measure).len(), 1, "no shot: one empty page");
    }

    #[test]
    fn the_pdf_is_well_formed() {
        let dir = std::env::temp_dir().join(format!("toonlight-board-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("board.pdf");
        let o = BoardOptions {
            style: BoardStyle::Rows,
            ..Default::default()
        };
        let pages = export_pdf("Été 東京", &shots(8), &o, &file).unwrap();
        assert_eq!(pages, 2);
        let pdf = std::fs::read(&file).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4") && text.trim_end().ends_with("%%EOF"));
        assert_eq!(text.matches("/Type /Page ").count(), 2);
        assert_eq!(text.matches("/Filter /DCTDecode").count(), 2);
        assert!(text.contains("/MediaBox [0 0 595.28 841.89]"));
        // the table at the end says where each object starts
        let xref = text.rfind("startxref\n").unwrap();
        let at: usize = text[xref + 10..].lines().next().unwrap().parse().unwrap();
        assert!(pdf[at..].starts_with(b"xref\n0 10\n"));
        let table = String::from_utf8_lossy(&pdf[at..]).to_string();
        for (n, line) in table.lines().skip(3).take(9).enumerate() {
            let offset: usize = line[..10].parse().unwrap();
            assert!(
                pdf[offset..].starts_with(format!("{} 0 obj\n", n + 1).as_bytes()),
                "object {}",
                n + 1
            );
        }
        // the pages are real pictures of the right size, and something is drawn on them
        let fonts = sheet_fonts();
        let m = |t: &str, size: f32| text_width(&fonts, t, size);
        let laid = layout("Film", &shots(2), &o, &m);
        let (px, w, h) = draw_page(&laid[0], &shots(2), o.page(), 72.0, &fonts);
        assert_eq!((w, h, px.len()), (595, 842, 595 * 842 * 3));
        assert!(px.iter().any(|v| *v < 80), "text and frames are drawn");
        assert!(text_width(&fonts, "Toonlight", 12.0) > 30.0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
