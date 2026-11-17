//! The animation document: layers, frames (with holds), drawings, light table and undo.

use serde::{Deserialize, Serialize};

/// A drawing: straight (non-premultiplied) RGBA pixels.
#[derive(Clone)]
pub struct Drawing {
    pub px: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: usize,
    pub y0: usize,
    pub x1: usize, // exclusive
    pub y1: usize, // exclusive
}

impl Rect {
    pub fn empty() -> Self {
        Rect {
            x0: usize::MAX,
            y0: usize::MAX,
            x1: 0,
            y1: 0,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }
    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        Rect {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }
    pub fn full(w: usize, h: usize) -> Rect {
        Rect {
            x0: 0,
            y0: 0,
            x1: w,
            y1: h,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    /// Light table on for this layer.
    pub onion: bool,
    /// One cell per frame: `Some(id)` = a drawing starts here; `None` = hold the previous drawing.
    pub cels: Vec<Option<usize>>,
    /// How the layer mixes with the layers under it.
    #[serde(default)]
    pub blend: Blend,
    /// Colour group (label) of the layer, shown in the timeline and exported.
    #[serde(default = "default_group")]
    pub group: [u8; 3],
    /// The layer's drawing protects the other layers from paint ("Layer: Toggle Stencil").
    #[serde(default)]
    pub stencil: bool,
    /// What the layer shows before its first drawing ("Pre Behavior").
    #[serde(default)]
    pub pre: Behavior,
    /// What the layer shows after `end` ("Post Behavior"; new layers hold, [LayerDefault]
    /// PostBehavior = 3).
    #[serde(default = "default_post")]
    pub post: Behavior,
    /// Last frame of the layer's own animation; None = it holds to the end of the project.
    #[serde(default)]
    pub end: Option<usize>,
    /// Instance names, by drawing id ("Instance Name"); unnamed drawings show their number.
    #[serde(default)]
    pub names: std::collections::BTreeMap<usize, String>,
    /// The layer's effects, applied in order when the picture is made (not to the drawings).
    #[serde(default)]
    pub fx: Vec<crate::fx::FxEntry>,
    /// Image marks, by drawing id: a colour number 1-12 (the colours of `GROUPS`) shown on the
    /// drawing in the timeline, to tell keys, breakdowns and in-betweens apart.
    #[serde(default)]
    pub marks: std::collections::BTreeMap<usize, u8>,
    /// Selected together with the current layer, for actions on several layers (not saved).
    #[serde(skip)]
    pub selected: bool,
}

impl Layer {
    /// A new, empty layer with the defaults of new layers (no colour group, holds after its end).
    pub fn new(name: String, cels: Vec<Option<usize>>) -> Layer {
        Layer {
            name,
            visible: true,
            locked: false,
            opacity: 1.0,
            onion: true,
            cels,
            blend: Blend::Color,
            // a new layer is in group 0, no colour ([LayerDefault] Group = 0)
            group: default_group(),
            stencil: false,
            pre: Behavior::None,
            post: Behavior::Hold,
            end: None,
            names: Default::default(),
            fx: vec![],
            marks: Default::default(),
            selected: false,
        }
    }
}

/// What a layer shows outside its own frames, numbered like the reference (
/// 0 none, 1 repeat, 2 pingpong, 3 hold).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Behavior {
    #[default]
    None,
    Repeat,
    PingPong,
    Hold,
}

fn default_post() -> Behavior {
    Behavior::Hold
}

pub const ALL_BEHAVIORS: [Behavior; 4] = [Behavior::None, Behavior::Repeat, Behavior::PingPong, Behavior::Hold];

impl Behavior {
    #[allow(dead_code)] // the number other programs use for it, kept for file exchange
    pub fn id(self) -> u32 {
        match self {
            Behavior::None => 0,
            Behavior::Repeat => 1,
            Behavior::PingPong => 2,
            Behavior::Hold => 3,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Behavior::None => "none",
            Behavior::Repeat => "repeat",
            Behavior::PingPong => "pingpong",
            Behavior::Hold => "hold",
        }
    }
    pub fn from_name(n: &str) -> Option<Behavior> {
        ALL_BEHAVIORS.iter().copied().find(|b| b.name().eq_ignore_ascii_case(n))
    }
    pub fn label(self) -> &'static str {
        use crate::i18n::tr;
        match self {
            Behavior::None => tr("Rien"),
            Behavior::Repeat => tr("Répéter"),
            Behavior::PingPong => tr("Aller-retour"),
            Behavior::Hold => tr("Tenir"),
        }
    }
}

/// Colours offered at start (our choice).
pub const DEFAULT_PALETTE: [[u8; 3]; 24] = [
    [0, 0, 0],
    [64, 64, 64],
    [128, 128, 128],
    [192, 192, 192],
    [255, 255, 255],
    [120, 72, 40],
    [200, 30, 40],
    [240, 90, 40],
    [250, 160, 30],
    [250, 220, 60],
    [160, 210, 60],
    [40, 160, 70],
    [30, 160, 150],
    [40, 140, 220],
    [30, 70, 180],
    [90, 50, 170],
    [170, 60, 170],
    [240, 110, 170],
    [255, 205, 170],
    [230, 170, 120],
    [170, 120, 80],
    [210, 230, 250],
    [140, 170, 200],
    [60, 80, 100],
];

pub fn default_palette() -> Vec<[u8; 3]> {
    DEFAULT_PALETTE.to_vec()
}

pub fn default_group() -> [u8; 3] {
    [120, 140, 140]
}

/// Areas (pixels) from which a frame is composed on several cores at once (estimated: below it
/// starting threads costs more than it saves).
pub const PARALLEL_AREA: usize = 160_000;

/// Most memory kept for effect pictures (estimated: 1 GiB, about 130 full-HD pictures).
pub const FX_CACHE_BYTES: usize = 1 << 30;

/// Colour groups for layers: group 0 is "no colour" (the reference takes a theme colour for it),
/// groups 1-12 are the reference's defaults (0xffRRGGBB), read back from
/// [GUIObjectColor] Color.01-Color.12 when the person changed them.
pub const GROUPS: [[u8; 3]; 13] = [
    [120, 140, 140],
    [0xa7, 0x37, 0x37],
    [0xc3, 0x73, 0x28],
    [0xd6, 0xb7, 0x16],
    [0xa5, 0xb3, 0x39],
    [0x52, 0x9d, 0x4f],
    [0x47, 0x8e, 0xa5],
    [0x40, 0x67, 0xac],
    [0x3c, 0x31, 0xbb],
    [0x6c, 0x42, 0xaa],
    [0xa1, 0x4b, 0x8c],
    [0x6f, 0x1d, 0x6c],
    [0xff, 0xff, 0xff],
];

/// Names of the groups (group 0 = "no colour"; the reference's own names are interface texts
/// 0xa1232d..0xa12338, ours name the colours, estimated).
pub const GROUP_NAMES: [&str; 13] = [
    "Sans couleur",
    "Rouge",
    "Orange",
    "Jaune",
    "Vert clair",
    "Vert",
    "Bleu-vert",
    "Bleu",
    "Indigo",
    "Violet",
    "Rose",
    "Prune",
    "Blanc",
];

/// The group number (0-12) of a colour, when it is one of the groups.
pub fn group_index(c: [u8; 3]) -> Option<usize> {
    GROUPS.iter().position(|g| *g == c)
}

/// Layer blending modes. Names follow the ones the After Effects import script shipped with the
/// After Effects JSON import script understands ("BlendingMatch").
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
pub enum Blend {
    #[default]
    Color,
    Multiply,
    Screen,
    Add,
    Overlay,
    Darken,
    Lighten,
    Difference,
    Erase,
}

pub const ALL_BLENDS: [Blend; 9] = [
    Blend::Color,
    Blend::Multiply,
    Blend::Screen,
    Blend::Add,
    Blend::Overlay,
    Blend::Darken,
    Blend::Lighten,
    Blend::Difference,
    Blend::Erase,
];

impl Blend {
    pub fn label(self) -> &'static str {
        match self {
            Blend::Color => crate::i18n::tr("Normal"),
            Blend::Multiply => crate::i18n::tr("Produit"),
            Blend::Screen => crate::i18n::tr("Superposition claire"),
            Blend::Add => crate::i18n::tr("Addition"),
            Blend::Overlay => crate::i18n::tr("Incrustation"),
            Blend::Darken => crate::i18n::tr("Assombrir"),
            Blend::Lighten => crate::i18n::tr("Éclaircir"),
            Blend::Difference => crate::i18n::tr("Différence"),
            Blend::Erase => crate::i18n::tr("Gommer dessous"),
        }
    }

    /// Name written in the After Effects JSON export.
    pub fn json_name(self) -> &'static str {
        match self {
            Blend::Color => "Color",
            Blend::Multiply => "Multiply",
            Blend::Screen => "Screen",
            Blend::Add => "Add",
            Blend::Overlay => "Overlay",
            Blend::Darken => "Darken",
            Blend::Lighten => "Lighten",
            Blend::Difference => "Difference",
            Blend::Erase => "Erase",
        }
    }

    #[inline]
    fn mix(self, s: u32, d: u32) -> u32 {
        match self {
            Blend::Color | Blend::Erase => s,
            Blend::Multiply => s * d / 255,
            Blend::Screen => 255 - (255 - s) * (255 - d) / 255,
            Blend::Add => (s + d).min(255),
            Blend::Overlay => {
                if d < 128 {
                    2 * s * d / 255
                } else {
                    255 - 2 * (255 - s) * (255 - d) / 255
                }
            }
            Blend::Darken => s.min(d),
            Blend::Lighten => s.max(d),
            Blend::Difference => s.abs_diff(d),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LightTable {
    pub active: bool,
    pub prev_count: usize,
    pub next_count: usize,
    pub prev_color: [u8; 3],
    pub next_color: [u8; 3],
    pub use_prev_color: bool,
    pub use_next_color: bool,
    pub front: bool,
    /// Global opacity, 0..=100.
    pub opacity: f32,
    /// Opacity for the 1st, 2nd ... image on each side, 0..=100 (10 slots).
    pub ranges: [f32; 10],
}

impl Default for LightTable {
    fn default() -> Self {
        // Measured values; range opacities and counts estimated.
        LightTable {
            active: true,
            prev_count: 2,
            next_count: 2,
            prev_color: [0, 192, 0],
            next_color: [255, 128, 0],
            use_prev_color: true,
            use_next_color: true,
            front: false,
            opacity: 100.0,
            ranges: [50.0, 30.0, 15.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }
}

enum Undo {
    Pixels {
        id: usize,
        rect: Rect,
        data: Vec<u8>,
    },
    Structure {
        layers: Vec<Layer>,
        frames: usize,
        current_layer: usize,
        frame: usize,
    },
}

impl Undo {
    /// Bytes this step keeps.
    fn bytes(&self) -> usize {
        match self {
            Undo::Pixels { data, .. } => data.len(),
            Undo::Structure { layers, .. } => layers.iter().map(|l| l.cels.len() * 16 + l.name.len() + 64).sum(),
        }
    }
}

pub struct Doc {
    pub w: usize,
    pub h: usize,
    pub fps: f32,
    pub frames: usize,
    pub layers: Vec<Layer>,
    pub drawings: Vec<Drawing>,
    pub paper: [u8; 3],
    pub paper_texture: Paper,
    pub sound: Option<crate::sound::SoundTrack>,
    pub palette: Vec<[u8; 3]>,
    pub camera: crate::camera::Camera,
    pub light: LightTable,
    pub current_layer: usize,
    pub frame: usize,
    /// Bookmarked frames ("Bookmarks: Toggle"), in order.
    pub bookmarks: Vec<usize>,
    /// Play range ("Mark In" / "Mark Out"), inclusive.
    pub mark_in: Option<usize>,
    pub mark_out: Option<usize>,
    /// Timeline notes: a text at a frame (dialogue, timing, a reminder), in frame order.
    pub notes: std::collections::BTreeMap<usize, String>,
    /// Drawings through their layer's effects: (layer, frame) → (fingerprint, picture).
    /// Pictures with their layer's effects, by what they are made of (drawing, its pixels and the
    /// effect settings at that frame), so a held drawing is worked out once.
    fx_cache: std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<Vec<u8>>>>,
    /// Painting on a held cell makes a new drawing there, a copy of the held one
    /// ([LayerDefault] AutoBreakInstance, on by default).
    pub auto_break: bool,
    /// Painting after a layer's end makes the layer longer; off, the app asks first
    /// ([LayerDefault] AutoStretchLayer, on by default;
    /// the reference's question is "Stretch Layer ?").
    pub auto_stretch: bool,
    /// Set when painting was refused after a layer's end: (layer, frame) to ask about.
    pub stretch_asked: Option<(usize, usize)>,
    undo: Vec<Undo>,
    redo: Vec<Undo>,
    pub dirty: bool,
}

impl Doc {
    pub fn new(w: usize, h: usize, fps: f32) -> Doc {
        let mut d = Doc {
            w,
            h,
            fps,
            frames: 1,
            layers: vec![],
            drawings: vec![],
            paper: [255, 255, 255],
            paper_texture: Paper::Smooth,
            sound: None,
            palette: DEFAULT_PALETTE.to_vec(),
            camera: crate::camera::Camera::new(w, h),
            light: LightTable::default(),
            current_layer: 0,
            frame: 0,
            bookmarks: vec![],
            mark_in: None,
            mark_out: None,
            notes: Default::default(),
            fx_cache: Default::default(),
            auto_break: crate::prefs::AUTO_BREAK.load(std::sync::atomic::Ordering::Relaxed),
            auto_stretch: crate::prefs::AUTO_STRETCH.load(std::sync::atomic::Ordering::Relaxed),
            stretch_asked: None,
            undo: vec![],
            redo: vec![],
            dirty: false,
        };
        let id = d.new_drawing();
        d.layers.push(Layer::new("Anim 1".into(), vec![Some(id)]));
        d
    }

    pub fn new_drawing(&mut self) -> usize {
        self.drawings.push(Drawing {
            px: vec![0; self.w * self.h * 4],
        });
        self.drawings.len() - 1
    }

    /// Index of the cell holding the drawing shown at `frame` (the key itself), if any.
    pub fn key_index(&self, layer: usize, frame: usize) -> Option<usize> {
        let cels = &self.layers[layer].cels;
        let mut f = frame.min(cels.len().saturating_sub(1));
        loop {
            if cels.get(f).copied().flatten().is_some() {
                return Some(f);
            }
            if f == 0 {
                return None;
            }
            f -= 1;
        }
    }

    pub fn drawing_at(&self, layer: usize, frame: usize) -> Option<usize> {
        self.key_index(layer, frame).and_then(|k| self.layers[layer].cels[k])
    }

    /// The frame of a layer's own timing shown at `frame`, after its pre / post behaviours.
    pub fn source_frame(&self, li: usize, frame: usize) -> Option<usize> {
        let l = &self.layers[li];
        let start = l.cels.iter().position(|c| c.is_some())?;
        let end = l.end.unwrap_or(usize::MAX).min(self.frames.saturating_sub(1)).max(start);
        if frame >= start && frame <= end {
            return Some(frame);
        }
        let (b, edge) = if frame < start { (l.pre, start) } else { (l.post, end) };
        let len = (end - start + 1) as i64;
        let d = frame as i64 - start as i64;
        match b {
            Behavior::None => None,
            Behavior::Hold => Some(edge),
            Behavior::Repeat => Some(start + d.rem_euclid(len) as usize),
            Behavior::PingPong => {
                if len == 1 {
                    return Some(start);
                }
                let period = 2 * (len - 1);
                let p = d.rem_euclid(period);
                Some(start + if p < len { p } else { period - p } as usize)
            }
        }
    }

    /// The drawing a layer shows at `frame` (behaviours included).
    pub fn shown_drawing(&self, li: usize, frame: usize) -> Option<usize> {
        self.source_frame(li, frame).and_then(|f| self.drawing_at(li, f))
    }

    /// The custom name of the drawing shown at a cell, if it has one.
    pub fn instance_name(&self, li: usize, frame: usize) -> Option<String> {
        let id = self.drawing_at(li, frame)?;
        self.layers[li].names.get(&id).cloned()
    }

    /// The number of the drawing shown at a cell (1 = the layer's first drawing).
    pub fn instance_number(&self, li: usize, frame: usize) -> Option<usize> {
        let id = self.drawing_at(li, frame)?;
        self.drawing_numbers(li).get(&id).copied()
    }

    /// The number of each drawing of a layer (1 = the first one to appear); a drawing used again
    /// later keeps its number.
    pub fn drawing_numbers(&self, li: usize) -> std::collections::HashMap<usize, usize> {
        let mut m = std::collections::HashMap::new();
        for id in self.layers[li].cels.iter().flatten() {
            let n = m.len() + 1;
            m.entry(*id).or_insert(n);
        }
        m
    }

    /// Exposure sheet entry: a drawing number shows that drawing at the cell (a number past the
    /// last drawing makes a new, empty one); "-" or nothing holds the drawing before it.
    pub fn set_cell_number(&mut self, li: usize, f: usize, text: &str) -> bool {
        if self.layers[li].locked || f >= self.frames {
            return false;
        }
        let t = text.trim();
        let cel = if t.is_empty() || t == "-" || t == "|" {
            None
        } else {
            let Ok(n) = t.parse::<usize>() else { return false };
            if n == 0 {
                return false;
            }
            let numbers = self.drawing_numbers(li);
            numbers.iter().find(|(_, k)| **k == n).map(|(id, _)| *id)
        };
        self.push_structure_undo();
        let cel = match (cel, t.parse::<usize>()) {
            (None, Ok(n)) if n > 0 => Some(self.new_drawing()),
            (c, _) => c,
        };
        self.layers[li].cels[f] = cel;
        self.dirty = true;
        true
    }

    pub fn set_instance_name(&mut self, li: usize, frame: usize, name: &str) {
        let Some(id) = self.drawing_at(li, frame) else { return };
        let names = &mut self.layers[li].names;
        if name.trim().is_empty() {
            names.remove(&id);
        } else {
            names.insert(id, name.to_string());
        }
        self.dirty = true;
    }

    /// Keys (frame index, drawing id) of a layer, in order.
    pub fn keys(&self, layer: usize) -> Vec<(usize, usize)> {
        self.layers[layer]
            .cels
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.map(|id| (i, id)))
            .collect()
    }

    /// Drawing to paint into at the current place; creates one if the layer has nothing there yet.
    pub fn target_drawing(&mut self) -> Option<usize> {
        let l = self.current_layer;
        if self.layers[l].locked {
            return None;
        }
        // painting after the layer's end makes its animation longer (or asks, auto stretch off)
        if self.layers[l].end.is_some_and(|e| self.frame > e) {
            if !self.auto_stretch {
                self.stretch_asked = Some((l, self.frame));
                return None;
            }
            self.push_structure_undo();
            self.layers[l].end = Some(self.frame);
        }
        let f = self.frame;
        if let Some(id) = self.drawing_at(l, f) {
            if !self.auto_break || self.layers[l].cels[f].is_some() {
                return Some(id);
            }
            // auto break instance: the hold becomes a drawing of its own
            self.push_structure_undo();
            let d = self.drawings[id].clone();
            self.drawings.push(d);
            let new = self.drawings.len() - 1;
            self.layers[l].cels[f] = Some(new);
            return Some(new);
        }
        self.push_structure_undo();
        let id = self.new_drawing();
        let f = self.frame;
        self.layers[l].cels[f] = Some(id);
        Some(id)
    }

    // ---------- undo ----------
    fn push(&mut self, u: Undo) {
        self.undo.push(u);
        let mut total: usize = self.undo.iter().map(Undo::bytes).sum();
        let limit = crate::prefs::UNDO_LIMIT.load(std::sync::atomic::Ordering::Relaxed);
        while total > limit && self.undo.len() > 1 {
            total -= self.undo[0].bytes();
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    /// Where paint may go on the current layer given the layers marked as stencils: 255 = free,
    /// 0 = protected (inside a stencil's drawing, or outside it when `invert`). None = no stencil.
    pub fn stencil_mask(&self, invert: bool) -> Option<Vec<u8>> {
        let n = self.w * self.h;
        let mut mask: Option<Vec<u8>> = None;
        for (li, l) in self.layers.iter().enumerate() {
            if !l.stencil || !l.visible || li == self.current_layer {
                continue;
            }
            let Some(id) = self.drawing_at(li, self.frame) else { continue };
            let px = &self.drawings[id].px;
            let m = mask.get_or_insert_with(|| vec![255; n]);
            for i in 0..n {
                if (px[i * 4 + 3] >= 128) != invert {
                    m[i] = 0;
                }
            }
        }
        mask
    }

    pub fn push_pixel_undo(&mut self, id: usize, rect: Rect, before: &[u8]) {
        if rect.is_empty() {
            return;
        }
        let data = copy_rect(before, self.w, rect);
        self.push(Undo::Pixels { id, rect, data });
    }

    pub fn push_structure_undo(&mut self) {
        let u = Undo::Structure {
            layers: self.layers.clone(),
            frames: self.frames,
            current_layer: self.current_layer,
            frame: self.frame,
        };
        self.push(u);
    }

    fn apply(&mut self, u: Undo) -> Undo {
        match u {
            Undo::Pixels { id, rect, data } => {
                let now = copy_rect(&self.drawings[id].px, self.w, rect);
                paste_rect(&mut self.drawings[id].px, self.w, rect, &data);
                Undo::Pixels { id, rect, data: now }
            }
            Undo::Structure {
                layers,
                frames,
                current_layer,
                frame,
            } => {
                let now = Undo::Structure {
                    layers: std::mem::replace(&mut self.layers, layers),
                    frames: self.frames,
                    current_layer: self.current_layer,
                    frame: self.frame,
                };
                self.frames = frames;
                self.current_layer = current_layer.min(self.layers.len() - 1);
                self.frame = frame.min(self.frames - 1);
                now
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) -> bool {
        if let Some(u) = self.undo.pop() {
            let r = self.apply(u);
            self.redo.push(r);
            self.dirty = true;
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        if let Some(u) = self.redo.pop() {
            let r = self.apply(u);
            self.undo.push(r);
            self.dirty = true;
            true
        } else {
            false
        }
    }
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    // ---------- timeline editing ----------
    fn pad_all(&mut self) {
        let n = self.layers.iter().map(|l| l.cels.len()).max().unwrap_or(1).max(1);
        self.frames = n;
        for l in &mut self.layers {
            while l.cels.len() < n {
                l.cels.push(None);
            }
        }
        self.frame = self.frame.min(self.frames - 1);
    }

    /// Insert a cell after the current frame in the current layer and move there.
    fn insert_after(&mut self, cel: Option<usize>) {
        let l = self.current_layer;
        let at = self.frame + 1;
        let n = self.layers[l].cels.len();
        self.layers[l].cels.insert(at.min(n), cel);
        self.pad_all();
        self.frame = at.min(self.frames - 1);
    }

    pub fn add_blank_after(&mut self) {
        if self.layers[self.current_layer].locked {
            return;
        }
        self.push_structure_undo();
        let id = self.new_drawing();
        self.insert_after(Some(id));
    }

    pub fn duplicate_after(&mut self) {
        let l = self.current_layer;
        if self.layers[l].locked {
            return;
        }
        self.push_structure_undo();
        let id = match self.drawing_at(l, self.frame) {
            Some(src) => {
                let d = self.drawings[src].clone();
                self.drawings.push(d);
                self.drawings.len() - 1
            }
            None => self.new_drawing(),
        };
        self.insert_after(Some(id));
    }

    /// Hold the current drawing `n` more frames ("Add Exposures…").
    pub fn expose_more_n(&mut self, n: usize) {
        let l = self.current_layer;
        if self.layers[l].locked || n == 0 {
            return;
        }
        self.push_structure_undo();
        let at = (self.frame + 1).min(self.layers[l].cels.len());
        for _ in 0..n {
            self.layers[l].cels.insert(at, None);
        }
        self.pad_all();
    }

    /// Hold the current drawing up to `n` frames less ("Number of Exposures to remove").
    pub fn expose_less_n(&mut self, n: usize) {
        let l = self.current_layer;
        if self.layers[l].locked || n == 0 {
            return;
        }
        let Some(k) = self.key_index(l, self.frame) else { return };
        let mut end = k;
        while end + 1 < self.layers[l].cels.len() && self.layers[l].cels[end + 1].is_none() {
            end += 1;
        }
        let take = n.min(end - k);
        if take == 0 {
            return;
        }
        self.push_structure_undo();
        for _ in 0..take {
            self.layers[l].cels.remove(end);
            self.layers[l].cels.push(None);
            end -= 1;
        }
        self.trim();
    }

    /// Every drawing from cell `a` to cell `b` of a layer is held exactly `n` frames
    /// ("Layer: Set Exposure Value"). Returns the last cell of the new run.
    pub fn set_exposure(&mut self, li: usize, a: usize, b: usize, n: usize) -> Option<usize> {
        if self.layers[li].locked {
            return None;
        }
        let n = n.max(1);
        let len = self.layers[li].cels.len();
        let b = b.min(len - 1);
        let first = match self.key_index(li, a) {
            Some(k) => k,
            None => (a..=b).find(|f| self.layers[li].cels[*f].is_some())?,
        };
        let cels = &self.layers[li].cels;
        let ids: Vec<usize> = (first..=b).filter_map(|f| cels[f]).collect();
        let mut end = b;
        while end + 1 < len && cels[end + 1].is_none() {
            end += 1;
        }
        self.push_structure_undo();
        let mut seq = vec![];
        for id in ids {
            seq.push(Some(id));
            seq.extend(std::iter::repeat_n(None, n - 1));
        }
        let old = end - first + 1;
        let new_len = seq.len();
        let cels = &mut self.layers[li].cels;
        cels.splice(first..=end, seq);
        if new_len < old {
            cels.extend(std::iter::repeat_n(None, old - new_len));
        }
        self.pad_all();
        Some(first + new_len - 1)
    }

    /// Move cells `a..=b` of a layer one frame left or right. Returns false when it can't.
    pub fn move_cells(&mut self, li: usize, a: usize, b: usize, right: bool) -> bool {
        if self.layers[li].locked || b < a || (!right && a == 0) {
            return false;
        }
        self.push_structure_undo();
        let cels = &mut self.layers[li].cels;
        if right {
            if b + 1 >= cels.len() {
                cels.push(None);
            }
            let c = cels.remove(b + 1);
            cels.insert(a, c);
        } else {
            let c = cels.remove(a - 1);
            cels.insert(b, c);
        }
        self.pad_all();
        true
    }

    /// Remove cells `a..=b` of a layer; later cells move left.
    pub fn delete_cells(&mut self, li: usize, a: usize, b: usize) {
        if self.layers[li].locked || b < a {
            return;
        }
        self.push_structure_undo();
        let cels = &mut self.layers[li].cels;
        let b = b.min(cels.len() - 1);
        cels.drain(a..=b);
        cels.extend(std::iter::repeat_n(None, b - a + 1));
        self.trim();
    }

    /// Insert copies of `cells` (drawing ids or holds) after the current frame of the current layer.
    pub fn paste_cells(
        &mut self,
        cells: &[Option<usize>],
        names: &std::collections::BTreeMap<usize, String>,
        marks: &std::collections::BTreeMap<usize, u8>,
    ) {
        let l = self.current_layer;
        if self.layers[l].locked || cells.is_empty() {
            return;
        }
        self.push_structure_undo();
        let mut copies = vec![];
        for c in cells {
            copies.push(match c {
                Some(id) if *id < self.drawings.len() => {
                    let d = self.drawings[*id].clone();
                    self.drawings.push(d);
                    let new = self.drawings.len() - 1;
                    if let Some(n) = names.get(id) {
                        self.layers[l].names.insert(new, n.clone());
                    }
                    if let Some(m) = marks.get(id) {
                        self.layers[l].marks.insert(new, *m);
                    }
                    Some(new)
                }
                _ => None,
            });
        }
        let at = (self.frame + 1).min(self.layers[l].cels.len());
        let n = copies.len();
        self.layers[l].cels.splice(at..at, copies);
        self.pad_all();
        self.frame = (at + n - 1).min(self.frames - 1);
    }

    /// Bookmark a frame, or remove its bookmark ("Bookmarks: Toggle").
    pub fn toggle_bookmark(&mut self, f: usize) {
        match self.bookmarks.binary_search(&f) {
            Ok(i) => {
                self.bookmarks.remove(i);
            }
            Err(i) => self.bookmarks.insert(i, f),
        }
        self.dirty = true;
    }

    pub fn next_bookmark(&self, from: usize) -> Option<usize> {
        self.bookmarks.iter().copied().find(|b| *b > from)
    }

    pub fn prev_bookmark(&self, from: usize) -> Option<usize> {
        self.bookmarks.iter().rev().copied().find(|b| *b < from)
    }

    /// The mark (1-12) of the drawing shown at a cell, if it has one.
    pub fn mark_at(&self, li: usize, frame: usize) -> Option<u8> {
        let id = self.drawing_at(li, frame)?;
        self.layers[li].marks.get(&id).copied()
    }

    /// Mark the drawing shown at a cell with a colour number 1-12; 0 removes the mark.
    /// Returns false when there is no drawing there or the layer is locked.
    pub fn set_mark(&mut self, li: usize, frame: usize, mark: u8) -> bool {
        if self.layers[li].locked {
            return false;
        }
        let Some(id) = self.drawing_at(li, frame) else { return false };
        let mark = mark.min(12);
        let now = self.layers[li].marks.get(&id).copied().unwrap_or(0);
        if now == mark {
            return true;
        }
        self.push_structure_undo();
        if mark == 0 {
            self.layers[li].marks.remove(&id);
        } else {
            self.layers[li].marks.insert(id, mark);
        }
        true
    }

    /// The frame where the next marked drawing of a layer starts, after `from`.
    pub fn next_mark(&self, li: usize, from: usize) -> Option<usize> {
        let l = &self.layers[li];
        self.keys(li)
            .into_iter()
            .find(|(f, id)| *f > from && l.marks.contains_key(id))
            .map(|k| k.0)
    }

    /// The frame where the previous marked drawing of a layer starts, before the drawing shown
    /// at `from`.
    pub fn prev_mark(&self, li: usize, from: usize) -> Option<usize> {
        let l = &self.layers[li];
        let here = self.key_index(li, from).unwrap_or(from);
        self.keys(li)
            .into_iter()
            .rev()
            .find(|(f, id)| *f < here && l.marks.contains_key(id))
            .map(|k| k.0)
    }

    /// Write a note at a frame of the timeline; an empty text removes it.
    pub fn set_note(&mut self, frame: usize, text: &str) {
        if text.trim().is_empty() {
            self.notes.remove(&frame);
        } else {
            self.notes.insert(frame, text.trim().to_string());
        }
        self.dirty = true;
    }

    /// The frame of the next note after `from`, and of the previous one before it.
    pub fn next_note(&self, from: usize) -> Option<usize> {
        self.notes.range(from + 1..).next().map(|n| *n.0)
    }

    pub fn prev_note(&self, from: usize) -> Option<usize> {
        self.notes.range(..from).next_back().map(|n| *n.0)
    }

    /// Insert `n` in-between drawings after the drawing shown at `frame`, just before the next
    /// drawing of the layer. With `mix`, each one is a cross-fade of the two drawings around it
    /// (a guide to draw over); without, they are blank. Returns the frame of the first one, or
    /// None when the layer is locked or has no drawing after this one.
    pub fn insert_tweens(&mut self, li: usize, frame: usize, n: usize, mix: bool) -> Option<usize> {
        if self.layers[li].locked || n == 0 {
            return None;
        }
        let k = self.key_index(li, frame)?;
        let a = self.layers[li].cels[k]?;
        let (at, b) = self.keys(li).into_iter().find(|(f, _)| *f > k)?;
        self.push_structure_undo();
        let mut cells = Vec::with_capacity(n);
        for i in 1..=n {
            let id = if mix {
                let t = i as f32 / (n + 1) as f32;
                let px = cross_fade(&self.drawings[a].px, &self.drawings[b].px, t);
                self.drawings.push(Drawing { px });
                self.drawings.len() - 1
            } else {
                self.new_drawing()
            };
            cells.push(Some(id));
        }
        self.layers[li].cels.splice(at..at, cells);
        self.pad_all();
        Some(at)
    }

    /// Frames played: from mark in to mark out (inclusive), else the whole project.
    pub fn play_range(&self) -> (usize, usize) {
        let last = self.frames.saturating_sub(1);
        let lo = self.mark_in.unwrap_or(0).min(last);
        let hi = self.mark_out.unwrap_or(last).min(last).max(lo);
        (lo, hi)
    }

    /// Hold the current drawing one more frame.
    pub fn expose_more(&mut self) {
        if self.layers[self.current_layer].locked {
            return;
        }
        self.push_structure_undo();
        let l = self.current_layer;
        let at = self.frame + 1;
        let n = self.layers[l].cels.len();
        self.layers[l].cels.insert(at.min(n), None);
        self.pad_all();
    }

    /// Hold the current drawing one frame less (removes a hold after it).
    pub fn expose_less(&mut self) {
        let l = self.current_layer;
        if self.layers[l].locked {
            return;
        }
        let Some(k) = self.key_index(l, self.frame) else { return };
        // last hold of this drawing
        let mut end = k;
        while end + 1 < self.layers[l].cels.len() && self.layers[l].cels[end + 1].is_none() {
            end += 1;
        }
        if end == k {
            return;
        }
        self.push_structure_undo();
        self.layers[l].cels.remove(end);
        self.layers[l].cels.push(None);
        self.trim();
    }

    /// Remove the current cell of the current layer; later cells move left.
    pub fn delete_cell(&mut self) {
        let l = self.current_layer;
        if self.layers[l].locked {
            return;
        }
        self.push_structure_undo();
        let f = self.frame;
        if f < self.layers[l].cels.len() {
            self.layers[l].cels.remove(f);
        }
        self.layers[l].cels.push(None);
        self.trim();
    }

    /// The project keeps its length; a layer that lost a cell holds its last drawing to the end.
    fn trim(&mut self) {
        for l in &mut self.layers {
            l.cels.truncate(self.frames);
        }
        self.frame = self.frame.min(self.frames - 1);
    }

    /// Make the project at least `n` frames long (by holding).
    pub fn set_length(&mut self, n: usize) {
        let n = n.max(1);
        if n == self.frames {
            return;
        }
        self.push_structure_undo();
        for l in &mut self.layers {
            l.cels.resize(n, None);
        }
        self.frames = n;
        self.frame = self.frame.min(n - 1);
    }

    pub fn add_layer(&mut self) {
        self.push_structure_undo();
        let n = self.layers.len() + 1;
        let mut cels = vec![None; self.frames];
        let id = self.new_drawing();
        cels[0] = Some(id);
        let at = self.current_layer + 1;
        self.layers.insert(at, Layer::new(format!("Anim {n}"), cels));
        self.current_layer = at;
    }

    pub fn duplicate_layer(&mut self) {
        self.push_structure_undo();
        let mut l = self.layers[self.current_layer].clone();
        let mut names = std::collections::BTreeMap::new();
        let mut marks = std::collections::BTreeMap::new();
        for c in l.cels.iter_mut() {
            if let Some(id) = *c {
                let d = self.drawings[id].clone();
                self.drawings.push(d);
                let new = self.drawings.len() - 1;
                if let Some(n) = l.names.get(&id) {
                    names.insert(new, n.clone());
                }
                if let Some(m) = l.marks.get(&id) {
                    marks.insert(new, *m);
                }
                *c = Some(new);
            }
        }
        l.names = names;
        l.marks = marks;
        l.selected = false;
        l.name = crate::i18n::trf("{} copie", &[&l.name]);
        let at = self.current_layer + 1;
        self.layers.insert(at, l);
        self.current_layer = at;
    }

    /// Merge the current layer down onto the layer under it ("LayerMergeCurrentAndBelow").
    /// Returns false when there is no layer under it or one of them is locked.
    pub fn merge_down(&mut self, opts: MergeOptions) -> bool {
        let up = self.current_layer;
        if up == 0 || self.layers[up].locked || self.layers[up - 1].locked {
            return false;
        }
        self.push_structure_undo();
        self.merge_into(up, up - 1, opts);
        self.current_layer = up - 1;
        true
    }

    /// Merge every visible layer into the lowest visible one ("LayerMergeVisible"). Hidden layers
    /// stay as they are. Returns false when fewer than two visible layers can be merged.
    pub fn merge_visible(&mut self, opts: MergeOptions) -> bool {
        let vis: Vec<usize> = (0..self.layers.len()).filter(|i| self.layers[*i].visible).collect();
        if vis.len() < 2 || vis.iter().any(|i| self.layers[*i].locked) {
            return false;
        }
        self.push_structure_undo();
        let bottom = vis[0];
        // from the top down, so each merge sees the layers under it already merged
        for k in (1..vis.len()).rev() {
            let up = vis[k];
            let below = vis[k - 1];
            self.merge_into(up, below, opts);
        }
        self.current_layer = bottom;
        true
    }

    /// Paint layer `up` onto layer `low` (low < up) frame by frame, with `up`'s opacity and
    /// blending, then remove `up`. Both layers' timing (holds, before / after behaviours) is
    /// followed: a new drawing starts wherever either layer shows another drawing.
    fn merge_into(&mut self, up: usize, low: usize, opts: MergeOptions) {
        let n = self.frames;
        let (opacity, mode) = (self.layers[up].opacity, self.layers[up].blend);
        let mut cels: Vec<Option<usize>> = Vec::with_capacity(n);
        let mut names = std::collections::BTreeMap::new();
        let mut marks = std::collections::BTreeMap::new();
        let mut prev: Option<(Option<usize>, Option<usize>)> = None;
        let mut cache: std::collections::HashMap<(Option<usize>, Option<usize>), usize> = Default::default();
        for f in 0..n {
            let pair = (self.shown_drawing(low, f), self.shown_drawing(up, f));
            if prev == Some(pair) {
                cels.push(None);
                continue;
            }
            prev = Some(pair);
            let id = match pair {
                (Some(l), None) => l,
                _ => {
                    if let Some(id) = cache.get(&pair) {
                        *id
                    } else {
                        let mut px = match pair.0 {
                            Some(l) => self.drawings[l].px.clone(),
                            None => vec![0; self.w * self.h * 4],
                        };
                        if let Some(u) = pair.1 {
                            blend_layer(&mut px, &self.drawings[u].px, opacity, mode);
                        }
                        self.drawings.push(Drawing { px });
                        let id = self.drawings.len() - 1;
                        cache.insert(pair, id);
                        id
                    }
                }
            };
            if opts.keep_instance_names {
                // both named: "upper-lower", like the reference (separator "-")
                let ln = pair.0.and_then(|l| self.layers[low].names.get(&l)).cloned();
                let un = pair.1.and_then(|u| self.layers[up].names.get(&u)).cloned();
                let name = match (un, ln) {
                    (Some(u), Some(l)) => Some(format!("{u}-{l}")),
                    (u, l) => u.or(l),
                };
                if let Some(n) = name {
                    names.insert(id, n);
                }
            }
            if opts.keep_image_marks {
                // the marks of both layers are kept, the upper layer's on top
                let lm = pair.0.and_then(|l| self.layers[low].marks.get(&l)).copied();
                let um = pair.1.and_then(|u| self.layers[up].marks.get(&u)).copied();
                if let Some(m) = um.or(lm) {
                    marks.insert(id, m);
                }
            }
            cels.push(Some(id));
        }
        let up_group = self.layers[up].group;
        let l = &mut self.layers[low];
        l.cels = cels;
        // the behaviours are now drawn into the frames
        l.end = None;
        l.pre = Behavior::None;
        l.post = Behavior::Hold;
        l.names = names;
        l.marks = marks;
        // keep colour group: the upper layer's group when it has one; otherwise none
        if opts.keep_color_group {
            if up_group != default_group() {
                l.group = up_group;
            }
        } else {
            l.group = default_group();
        }
        self.layers.remove(up);
        self.dirty = true;
    }

    /// A new project from a template: size, frame rate, length and layers (top first).
    pub fn from_template(t: &Template) -> Doc {
        let mut d = Doc::new(t.w, t.h, t.fps);
        d.set_length(t.frames.max(1));
        let names: Vec<&str> = t.layers.iter().rev().copied().collect();
        d.layers[0].name = crate::i18n::tr(names[0]).to_string();
        for n in &names[1..] {
            d.add_layer();
            let c = d.current_layer;
            d.layers[c].name = crate::i18n::tr(n).to_string();
        }
        d.clear_history();
        d.dirty = false;
        d
    }

    /// Repeat cells `a..=b` of a layer `times` more times right after them ("Exposure: Repeat
    /// Images" loop / ping-pong / random). Returns the last cell of the repeats.
    pub fn repeat_cells(&mut self, li: usize, a: usize, b: usize, times: usize, mode: RepeatMode) -> Option<usize> {
        if self.layers[li].locked || times == 0 || b < a {
            return None;
        }
        let b = b.min(self.frames - 1);
        // on a single cell, like the reference (checks the frame starts a drawing):
        // only where a drawing starts
        if a == b && self.layers[li].cels.get(a).copied().flatten().is_none() {
            return None;
        }
        // what each cell shows, so holds survive any order
        let seq: Vec<Option<usize>> = (a..=b).map(|f| self.drawing_at(li, f)).collect();
        if seq.iter().all(|s| s.is_none()) {
            return None;
        }
        let mut out: Vec<Option<usize>> = vec![];
        let mut seed = ((a as u64 + 1) * 0x9E37_79B9_7F4A_7C15) ^ (b as u64 + times as u64);
        for k in 1..=times {
            let mut part: Vec<Option<usize>> = match mode {
                RepeatMode::Loop => seq.clone(),
                // back and forth, without doubling the turning drawing
                RepeatMode::PingPong => {
                    let mut p = seq.clone();
                    if k % 2 == 1 {
                        p.reverse();
                    }
                    if p.len() > 1 {
                        p.remove(0);
                    }
                    p
                }
                RepeatMode::Random => {
                    let mut p = seq.clone();
                    for i in (1..p.len()).rev() {
                        seed ^= seed << 13;
                        seed ^= seed >> 7;
                        seed ^= seed << 17;
                        p.swap(i, (seed % (i as u64 + 1)) as usize);
                    }
                    p
                }
            };
            out.append(&mut part);
        }
        self.push_structure_undo();
        // a cell starts a drawing only where the drawing changes; elsewhere it holds
        let mut last = seq.last().copied().flatten();
        let cels: Vec<Option<usize>> = out
            .into_iter()
            .map(|d| {
                if d == last {
                    None
                } else {
                    last = d;
                    d
                }
            })
            .collect();
        let n = cels.len();
        let at = (b + 1).min(self.layers[li].cels.len());
        self.layers[li].cels.splice(at..at, cels);
        self.pad_all();
        Some(at + n - 1)
    }

    /// The undo steps (oldest first) and the redo steps (next first): true = a drawing changed,
    /// false = layers or frames changed.
    pub fn history(&self) -> (Vec<bool>, Vec<bool>) {
        let kind = |u: &Undo| matches!(u, Undo::Pixels { .. });
        (self.undo.iter().map(kind).collect(), self.redo.iter().rev().map(kind).collect())
    }

    /// A copy of a layer with its drawings ("Layer: Copy"), to paste in this or another project.
    pub fn copy_layer(&self, li: usize) -> LayerCopy {
        let layer = self.layers[li].clone();
        let mut drawings = std::collections::BTreeMap::new();
        for id in layer.cels.iter().flatten() {
            drawings.entry(*id).or_insert_with(|| self.drawings[*id].clone());
        }
        LayerCopy {
            layer,
            drawings,
            w: self.w,
            h: self.h,
        }
    }

    /// Paste a copied layer above the current one, with copies of its drawings (pictures of
    /// another size are placed at the top-left, cut or padded).
    pub fn paste_layer(&mut self, c: &LayerCopy) {
        self.push_structure_undo();
        let mut map = std::collections::HashMap::new();
        for (old, d) in &c.drawings {
            let px = if (c.w, c.h) == (self.w, self.h) {
                d.px.clone()
            } else {
                let mut px = vec![0u8; self.w * self.h * 4];
                for y in 0..c.h.min(self.h) {
                    let n = c.w.min(self.w) * 4;
                    px[y * self.w * 4..y * self.w * 4 + n].copy_from_slice(&d.px[y * c.w * 4..y * c.w * 4 + n]);
                }
                px
            };
            self.drawings.push(Drawing { px });
            map.insert(*old, self.drawings.len() - 1);
        }
        let mut l = c.layer.clone();
        l.cels = l.cels.iter().map(|cel| cel.map(|id| map[&id])).collect();
        l.names = l.names.iter().filter_map(|(id, n)| map.get(id).map(|m| (*m, n.clone()))).collect();
        l.marks = l.marks.iter().filter_map(|(id, k)| map.get(id).map(|m| (*m, *k))).collect();
        l.locked = false;
        l.selected = false;
        let at = (self.current_layer + 1).min(self.layers.len());
        self.layers.insert(at, l);
        self.current_layer = at;
        self.pad_all();
        let n = self.frames;
        for l in &mut self.layers {
            l.cels.resize(n, None);
        }
        self.dirty = true;
    }

    /// The layers an action applies to: the current layer and every layer selected with it,
    /// from the bottom up.
    pub fn target_layers(&self) -> Vec<usize> {
        (0..self.layers.len())
            .filter(|&i| i == self.current_layer || self.layers[i].selected)
            .collect()
    }

    /// Make a layer the current one. When it is one of the chosen layers, they all stay chosen;
    /// otherwise the choice is dropped.
    pub fn focus_layer(&mut self, li: usize) {
        if li == self.current_layer || li >= self.layers.len() {
            return;
        }
        if self.layers[li].selected {
            let old = self.current_layer;
            self.layers[old].selected = true;
        } else {
            self.select_layers(false);
        }
        self.current_layer = li;
    }

    /// Select every layer, or none (the current layer always counts as chosen).
    pub fn select_layers(&mut self, on: bool) {
        for l in &mut self.layers {
            l.selected = on;
        }
    }

    /// First frame and last frame of a layer's own animation: from its first drawing to its end
    /// (the project's end when it has none). None for a layer without drawings.
    pub fn layer_span(&self, li: usize) -> Option<(usize, usize)> {
        let l = &self.layers[li];
        let start = l.cels.iter().position(|c| c.is_some())?;
        Some((start, l.end.unwrap_or(self.frames - 1).min(self.frames - 1).max(start)))
    }

    /// Move the whole animation of `layers` in time: `by` frames later (+) or earlier (−). A
    /// move earlier stops when a layer's first drawing reaches the first frame, so no drawing is
    /// lost; the project gets longer when a drawing would pass its end. Returns the frames
    /// really moved (0 when nothing can move or a layer is locked).
    pub fn shift_layers(&mut self, layers: &[usize], by: i64) -> i64 {
        let spans: Vec<(usize, usize)> = layers.iter().filter_map(|&li| self.layer_span(li).map(|s| (li, s.0))).collect();
        if spans.is_empty() || by == 0 || layers.iter().any(|&li| self.layers[li].locked) {
            return 0;
        }
        // the same move for every layer, so they stay in step
        let room = spans.iter().map(|s| s.1).min().unwrap_or(0) as i64;
        let by = by.max(-room);
        if by == 0 {
            return 0;
        }
        self.push_structure_undo();
        let n = by.unsigned_abs() as usize;
        for &(li, _) in &spans {
            let last_key = self.layers[li].cels.iter().rposition(|c| c.is_some()).unwrap_or(0);
            let l = &mut self.layers[li];
            if by > 0 {
                l.cels.splice(0..0, std::iter::repeat_n(None, n));
                l.end = l.end.map(|e| e + n);
                // holds pushed past the project's end are dropped; drawings (and the layer's end) are kept
                let keep = l.end.unwrap_or(0).max(last_key + n) + 1;
                l.cels.truncate(keep.max(self.frames));
            } else {
                l.cels.drain(0..n);
                l.cels.extend(std::iter::repeat_n(None, n));
                l.end = l.end.map(|e| e.saturating_sub(n));
            }
        }
        self.pad_all();
        self.dirty = true;
        by
    }

    /// Slide a layer's drawings inside its own frames, `by` frames later (+) or earlier (−): the
    /// layer still starts and ends on the same frames. Later, the first drawing is held longer
    /// and what passes the end is cut; earlier, the first frames are cut and the last drawing is
    /// held longer. Returns false when there is nothing to slide or the layer is locked.
    pub fn slide_layer(&mut self, li: usize, by: i64) -> bool {
        let Some((start, end)) = self.layer_span(li) else { return false };
        let len = end - start + 1;
        let n = (by.unsigned_abs() as usize).min(len - 1);
        if self.layers[li].locked || n == 0 {
            return false;
        }
        self.push_structure_undo();
        let mut window: Vec<Option<usize>> = self.layers[li].cels[start..=end].to_vec();
        if by > 0 {
            window.splice(1..1, std::iter::repeat_n(None, n));
            window.truncate(len);
        } else {
            // what the layer showed `n` frames later now shows at its first frame
            let first = self.drawing_at(li, start + n);
            window.drain(0..n);
            window[0] = first;
            window.extend(std::iter::repeat_n(None, n));
        }
        self.layers[li].cels.splice(start..=end, window);
        self.dirty = true;
        true
    }

    /// Split a layer in two at frame `at`: the layer keeps what comes before, and a new layer
    /// above it takes the frames from `at` on (starting with the drawing shown there). Returns
    /// false when `at` is not inside the layer's animation or the layer is locked.
    pub fn split_layer(&mut self, li: usize, at: usize) -> bool {
        let Some((start, end)) = self.layer_span(li) else { return false };
        if self.layers[li].locked || at <= start || at > end {
            return false;
        }
        let Some(shown) = self.drawing_at(li, at) else { return false };
        self.push_structure_undo();
        let old = self.layers[li].clone();
        let before: std::collections::HashSet<usize> = old.cels[..at].iter().flatten().copied().collect();
        // the second part gets its own copy of any drawing the first part still shows
        let mut copies: std::collections::HashMap<usize, usize> = Default::default();
        let mut second = Layer::new(format!("{} (2)", old.name), vec![None; old.cels.len()]);
        let mut tail: Vec<(usize, usize)> = vec![(at, shown)];
        tail.extend((at + 1..old.cels.len()).filter_map(|f| old.cels[f].map(|id| (f, id))));
        for (f, id) in tail {
            let new = if before.contains(&id) {
                *copies.entry(id).or_insert_with(|| {
                    let d = self.drawings[id].clone();
                    self.drawings.push(d);
                    self.drawings.len() - 1
                })
            } else {
                id
            };
            second.cels[f] = Some(new);
            if let Some(n) = old.names.get(&id) {
                second.names.insert(new, n.clone());
            }
            if let Some(m) = old.marks.get(&id) {
                second.marks.insert(new, *m);
            }
        }
        second.visible = old.visible;
        second.opacity = old.opacity;
        second.onion = old.onion;
        second.blend = old.blend;
        second.group = old.group;
        second.stencil = old.stencil;
        second.fx = old.fx.clone();
        // the second part takes over at `at`: nothing of it before, the old ending after
        second.pre = Behavior::None;
        second.post = old.post;
        second.end = old.end;
        let first = &mut self.layers[li];
        for c in first.cels[at..].iter_mut() {
            *c = None;
        }
        first.end = Some(at - 1);
        first.post = Behavior::None;
        self.layers.insert(li + 1, second);
        self.current_layer = li + 1;
        self.dirty = true;
        true
    }

    /// Remove `layers` (at least one layer always stays). Returns how many were removed.
    pub fn delete_layers(&mut self, layers: &[usize]) -> usize {
        let mut gone: Vec<usize> = layers.iter().copied().filter(|&i| i < self.layers.len()).collect();
        gone.sort_unstable();
        gone.dedup();
        if gone.len() >= self.layers.len() {
            gone.remove(0);
        }
        if gone.is_empty() {
            return 0;
        }
        self.push_structure_undo();
        for &i in gone.iter().rev() {
            self.layers.remove(i);
        }
        let below = gone.iter().filter(|&&i| i < self.current_layer).count();
        self.current_layer = (self.current_layer - below).min(self.layers.len() - 1);
        self.dirty = true;
        gone.len()
    }

    /// Merge `layers` into the lowest of them ("merge selected"); the other layers stay as they
    /// are. Returns false with fewer than two layers or a locked one.
    pub fn merge_layers(&mut self, layers: &[usize], opts: MergeOptions) -> bool {
        let mut sel: Vec<usize> = layers.iter().copied().filter(|&i| i < self.layers.len()).collect();
        sel.sort_unstable();
        sel.dedup();
        if sel.len() < 2 || sel.iter().any(|&i| self.layers[i].locked) {
            return false;
        }
        self.push_structure_undo();
        // from the top down: each merge removes the upper layer, which is above everything left to merge
        for k in (1..sel.len()).rev() {
            self.merge_into(sel[k], sel[k - 1], opts);
        }
        self.current_layer = sel[0];
        self.layers[sel[0]].selected = false;
        true
    }

    /// Stretch (or shrink) a layer's animation to `new_len` frames: every drawing starts at the
    /// proportional place ("Layer: Stretch"). Returns false when there is nothing to stretch.
    pub fn stretch_layer(&mut self, li: usize, new_len: usize) -> bool {
        let l = &self.layers[li];
        if l.locked || new_len == 0 {
            return false;
        }
        let Some(start) = l.cels.iter().position(|c| c.is_some()) else {
            return false;
        };
        let end = l.end.unwrap_or(self.frames - 1).max(start);
        let old_len = end - start + 1;
        if old_len == new_len {
            return false;
        }
        let keys: Vec<(usize, usize)> = (start..=end).filter_map(|f| l.cels[f].map(|id| (f - start, id))).collect();
        self.push_structure_undo();
        let total = (start + new_len).max(self.frames);
        let mut cels: Vec<Option<usize>> = vec![None; total];
        for (r, id) in keys {
            let f = start + ((r as f32) * new_len as f32 / old_len as f32).round() as usize;
            // when shrinking, two drawings can land on one frame: the first one stays
            if f < start + new_len && cels[f].is_none() {
                cels[f] = Some(id);
            }
        }
        // cells after the layer's own frames are kept after the new end
        let tail: Vec<Option<usize>> = self.layers[li].cels.iter().skip(end + 1).copied().collect();
        for (k, c) in tail.into_iter().enumerate() {
            if let Some(slot) = cels.get_mut(start + new_len + k) {
                *slot = c;
            }
        }
        let l = &mut self.layers[li];
        l.cels = cels;
        if l.end.is_some() || new_len < old_len {
            l.end = Some(start + new_len - 1);
        }
        self.pad_all();
        self.dirty = true;
        true
    }

    /// A drawing through its layer's effects at a frame (cached until the drawing or the effects change).
    /// What a layer's effect picture is made of: the drawing, its pixels and the effect settings.
    fn fx_mark(&self, li: usize, id: usize, frame: usize) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hs = std::collections::hash_map::DefaultHasher::new();
        id.hash(&mut hs);
        self.drawings[id].px.hash(&mut hs);
        for e in self.layers[li].fx.iter().filter(|e| e.on) {
            format!("{:?}", e.at(frame)).hash(&mut hs);
        }
        hs.finish()
    }

    fn with_fx(&self, li: usize, id: usize, frame: usize) -> std::sync::Arc<Vec<u8>> {
        let mark = self.fx_mark(li, id, frame);
        if let Some(px) = self.fx_cache.lock().unwrap().get(&mark) {
            return px.clone();
        }
        let px = std::sync::Arc::new(crate::fx::run_stack(
            &self.layers[li].fx,
            frame,
            &self.drawings[id].px,
            self.w,
            self.h,
        ));
        let mut cache = self.fx_cache.lock().unwrap();
        if (cache.len() + 1) * self.w * self.h * 4 > FX_CACHE_BYTES {
            cache.clear();
        }
        cache.insert(mark, px.clone());
        px
    }

    /// Work out ahead, on every core at once, the effect pictures `frames` will need (for
    /// playback). At most one picture per core is made per call, so the window never waits long.
    pub fn prewarm_fx(&self, frames: &[usize]) {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(16);
        let mut jobs: Vec<(usize, usize, usize, u64)> = vec![];
        for &f in frames {
            for li in 0..self.layers.len() {
                let l = &self.layers[li];
                if !l.visible || !l.fx.iter().any(|e| e.on) {
                    continue;
                }
                let Some(id) = self.source_frame(li, f).and_then(|g| self.drawing_at(li, g)) else {
                    continue;
                };
                let mark = self.fx_mark(li, id, f);
                if jobs.iter().any(|j| j.3 == mark) || self.fx_cache.lock().unwrap().contains_key(&mark) {
                    continue;
                }
                jobs.push((li, id, f, mark));
                if jobs.len() >= cores {
                    break;
                }
            }
            if jobs.len() >= cores {
                break;
            }
        }
        if jobs.is_empty() {
            return;
        }
        std::thread::scope(|sc| {
            for &(li, id, f, _) in &jobs {
                sc.spawn(move || {
                    self.with_fx(li, id, f);
                });
            }
        });
    }

    /// Paint a layer's effects into its drawings for good and empty its stack ("FXStackApply").
    pub fn apply_fx_stack(&mut self, li: usize) -> bool {
        if self.layers[li].fx.iter().all(|e| !e.on) || self.layers[li].locked {
            return false;
        }
        self.push_structure_undo();
        let stack = self.layers[li].fx.clone();
        let mut done = std::collections::HashMap::new();
        for f in 0..self.frames {
            let Some(key) = self.key_index(li, f) else { continue };
            let Some(id) = self.layers[li].cels[key] else { continue };
            if done.contains_key(&(id, key)) {
                continue;
            }
            let px = crate::fx::run_stack(&stack, key, &self.drawings[id].px, self.w, self.h);
            self.drawings.push(Drawing { px });
            let new = self.drawings.len() - 1;
            done.insert((id, key), new);
            let l = &mut self.layers[li];
            l.cels[key] = Some(new);
            // the drawing's name and mark follow it
            if let Some(n) = l.names.get(&id).cloned() {
                l.names.insert(new, n);
            }
            if let Some(m) = l.marks.get(&id).copied() {
                l.marks.insert(new, m);
            }
        }
        self.layers[li].fx.clear();
        self.fx_cache.lock().unwrap().clear();
        self.dirty = true;
        true
    }

    pub fn delete_layer(&mut self) {
        if self.layers.len() <= 1 {
            return;
        }
        self.push_structure_undo();
        self.layers.remove(self.current_layer);
        self.current_layer = self.current_layer.min(self.layers.len() - 1);
    }

    pub fn move_layer(&mut self, up: bool) {
        let i = self.current_layer;
        let j = if up { i + 1 } else { i.wrapping_sub(1) };
        if j >= self.layers.len() {
            return;
        }
        self.push_structure_undo();
        self.layers.swap(i, j);
        self.current_layer = j;
    }

    // ---------- pictures ----------

    /// Size of the finished pictures: the camera's when it's on, else the canvas.
    pub fn output_size(&self) -> (usize, usize) {
        if self.camera.enabled {
            (self.camera.width.max(1), self.camera.height.max(1))
        } else {
            (self.w, self.h)
        }
    }

    /// The finished picture of a frame (no light table), through the camera when it's on.
    pub fn render(&self, frame: usize) -> Vec<u8> {
        let mut buf = vec![0u8; self.w * self.h * 4];
        self.composite(frame, false, Rect::full(self.w, self.h), &mut buf);
        if self.camera.enabled {
            self.camera.shoot(frame, &buf, self.w, self.h, self.paper)
        } else {
            buf
        }
    }

    /// Composite `frame` into `out` (opaque RGBA, w*h*4) inside `rect`.
    /// Compose the finished picture of `frame` inside `rect` into `out` (the whole canvas).
    /// Big areas are split into bands of rows composed at the same time on every core; each layer's
    /// effect stack is worked out once, before the split.
    pub fn composite(&self, frame: usize, onion: bool, rect: Rect, out: &mut [u8]) {
        if rect.is_empty() {
            return;
        }
        let w = self.w;
        let fx: Vec<Option<std::sync::Arc<Vec<u8>>>> = (0..self.layers.len())
            .map(|li| {
                let l = &self.layers[li];
                if !l.visible || !l.fx.iter().any(|e| e.on) {
                    return None;
                }
                let id = self.source_frame(li, frame).and_then(|f| self.drawing_at(li, f))?;
                Some(self.with_fx(li, id, frame))
            })
            .collect();
        let rows = rect.y1 - rect.y0;
        let area = rows * (rect.x1 - rect.x0);
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(16);
        if cores < 2 || area < PARALLEL_AREA || rows < 2 * cores {
            let base = rect.y0 * w * 4;
            self.composite_band(frame, onion, rect, &mut out[base..rect.y1 * w * 4], base, &fx);
            return;
        }
        let per = rows.div_ceil(cores);
        let mut rest = &mut out[rect.y0 * w * 4..rect.y1 * w * 4];
        std::thread::scope(|sc| {
            let mut y = rect.y0;
            while y < rect.y1 {
                let y1 = (y + per).min(rect.y1);
                let (band, tail) = std::mem::take(&mut rest).split_at_mut((y1 - y) * w * 4);
                rest = tail;
                let r = Rect {
                    x0: rect.x0,
                    y0: y,
                    x1: rect.x1,
                    y1,
                };
                let fx = &fx;
                sc.spawn(move || self.composite_band(frame, onion, r, band, y * w * 4, fx));
                y = y1;
            }
        });
    }

    /// One band of rows: `out` holds the canvas bytes from `base` (rows rect.y0..rect.y1).
    fn composite_band(&self, frame: usize, onion: bool, rect: Rect, out: &mut [u8], base: usize, fx: &[Option<std::sync::Arc<Vec<u8>>>]) {
        let w = self.w;
        for y in rect.y0..rect.y1 {
            let row = &mut out[(y * w + rect.x0) * 4 - base..(y * w + rect.x1) * 4 - base];
            for (i, p) in row.chunks_exact_mut(4).enumerate() {
                let k = 1.0 + self.paper_texture.shade(rect.x0 + i, y);
                p[0] = (self.paper[0] as f32 * k).clamp(0.0, 255.0) as u8;
                p[1] = (self.paper[1] as f32 * k).clamp(0.0, 255.0) as u8;
                p[2] = (self.paper[2] as f32 * k).clamp(0.0, 255.0) as u8;
                p[3] = 255;
            }
        }
        let lt = &self.light;
        for (li, layer) in self.layers.iter().enumerate() {
            if !layer.visible {
                continue;
            }
            let src = self.source_frame(li, frame);
            let cur = src.and_then(|f| self.drawing_at(li, f));
            let mut onions: Vec<(usize, Option<[u8; 3]>, f32)> = vec![];
            if onion
                && lt.active
                && layer.onion
                && (src.is_some() || frame < self.layers[li].cels.iter().position(|c| c.is_some()).unwrap_or(0))
            {
                let keys = self.keys(li);
                let pos = src.and_then(|f| self.key_index(li, f));
                if let Some(pos) = pos {
                    let ki = keys.iter().position(|(f, _)| *f == pos).unwrap_or(0);
                    for k in 1..=lt.prev_count.min(10) {
                        if ki >= k {
                            let a = lt.ranges[k - 1] / 100.0 * lt.opacity / 100.0;
                            let tint = if lt.use_prev_color { Some(lt.prev_color) } else { None };
                            onions.push((keys[ki - k].1, tint, a));
                        }
                    }
                    for k in 1..=lt.next_count.min(10) {
                        if ki + k < keys.len() {
                            let a = lt.ranges[k - 1] / 100.0 * lt.opacity / 100.0;
                            let tint = if lt.use_next_color { Some(lt.next_color) } else { None };
                            onions.push((keys[ki + k].1, tint, a));
                        }
                    }
                } else if let Some((_, id)) = keys.first() {
                    // before the first drawing: show the next one
                    let tint = if lt.use_next_color { Some(lt.next_color) } else { None };
                    onions.push((*id, tint, lt.ranges[0] / 100.0 * lt.opacity / 100.0));
                }
            }
            let draw_onions = |out: &mut [u8]| {
                for (id, tint, a) in onions.iter().rev() {
                    if *a > 0.0 && Some(*id) != cur {
                        blend(
                            out,
                            base,
                            &self.drawings[*id].px,
                            w,
                            rect,
                            *a * layer.opacity,
                            *tint,
                            Blend::Color,
                            self.paper,
                        );
                    }
                }
            };
            if !lt.front {
                draw_onions(out);
            }
            if let Some(id) = cur {
                match &fx[li] {
                    Some(px) => blend(out, base, px, w, rect, layer.opacity, None, layer.blend, self.paper),
                    None => blend(
                        out,
                        base,
                        &self.drawings[id].px,
                        w,
                        rect,
                        layer.opacity,
                        None,
                        layer.blend,
                        self.paper,
                    ),
                }
            }
            if lt.front {
                draw_onions(out);
            }
        }
    }
}

/// A layer copied with its drawings (by id) and the size of the project it came from.
#[derive(Clone)]
pub struct LayerCopy {
    pub layer: Layer,
    pub drawings: std::collections::BTreeMap<usize, Drawing>,
    pub w: usize,
    pub h: usize,
}

/// How "Repeat images" orders the copies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RepeatMode {
    Loop,
    PingPong,
    Random,
}

impl RepeatMode {
    /// The reference's instance-mode number (actions InstanceModeRepeat → 1,
    /// ping-pong → 4, random → 5).
    #[allow(dead_code)] // the number other programs use for it, kept for file exchange
    pub fn id(self) -> u32 {
        match self {
            RepeatMode::Loop => 1,
            RepeatMode::PingPong => 4,
            RepeatMode::Random => 5,
        }
    }
}

/// A project template: name (French key), size, frame rate, length, layer names from the top.
pub struct Template {
    pub name: &'static str,
    pub w: usize,
    pub h: usize,
    pub fps: f32,
    pub frames: usize,
    pub layers: &'static [&'static str],
}

const FIVE: &[&str] = &["Premier plan", "Trait", "Ombre", "Couleur", "Décor"];
const TWO: &[&str] = &["Animation", "Décor"];

/// The reference's templates (Default Config/templates): sizes, rates, lengths and layers.
pub const TEMPLATES: [Template; 19] = [
    Template {
        name: "Dessin animé simple 2 s",
        w: 800,
        h: 600,
        fps: 30.0,
        frames: 60,
        layers: FIVE,
    },
    Template {
        name: "Dessin animé simple 5 s",
        w: 800,
        h: 600,
        fps: 30.0,
        frames: 150,
        layers: FIVE,
    },
    Template {
        name: "Dessin animé simple 10 s",
        w: 800,
        h: 600,
        fps: 30.0,
        frames: 300,
        layers: FIVE,
    },
    Template {
        name: "Film en doublé 2 s, 2 calques",
        w: 1280,
        h: 720,
        fps: 12.0,
        frames: 24,
        layers: TWO,
    },
    Template {
        name: "Film en doublé 5 s, 2 calques",
        w: 1280,
        h: 720,
        fps: 12.0,
        frames: 60,
        layers: TWO,
    },
    Template {
        name: "Film en doublé 10 s, 2 calques",
        w: 1280,
        h: 720,
        fps: 12.0,
        frames: 120,
        layers: TWO,
    },
    Template {
        name: "Film en doublé 2 s, 5 calques",
        w: 1280,
        h: 720,
        fps: 12.0,
        frames: 24,
        layers: FIVE,
    },
    Template {
        name: "Film en doublé 5 s, 5 calques",
        w: 1280,
        h: 720,
        fps: 12.0,
        frames: 60,
        layers: FIVE,
    },
    Template {
        name: "Film en doublé 10 s, 5 calques",
        w: 1280,
        h: 720,
        fps: 12.0,
        frames: 120,
        layers: FIVE,
    },
    Template {
        name: "TV PAL 16:9 en doublé 2 s, 2 calques",
        w: 720,
        h: 405,
        fps: 12.5,
        frames: 25,
        layers: TWO,
    },
    Template {
        name: "TV PAL 16:9 en doublé 10 s, 2 calques",
        w: 720,
        h: 405,
        fps: 12.5,
        frames: 125,
        layers: TWO,
    },
    Template {
        name: "TV PAL 16:9 en doublé 2 s, 5 calques",
        w: 720,
        h: 405,
        fps: 12.5,
        frames: 25,
        layers: FIVE,
    },
    Template {
        name: "TV PAL 16:9 en doublé 10 s, 5 calques",
        w: 720,
        h: 405,
        fps: 12.5,
        frames: 125,
        layers: FIVE,
    },
    Template {
        name: "TV PAL 4:3 en doublé 2 s, 2 calques",
        w: 768,
        h: 576,
        fps: 12.5,
        frames: 25,
        layers: TWO,
    },
    Template {
        name: "TV PAL 4:3 en doublé 10 s, 2 calques",
        w: 768,
        h: 576,
        fps: 12.5,
        frames: 125,
        layers: TWO,
    },
    Template {
        name: "TV PAL 4:3 en doublé 2 s, 5 calques",
        w: 768,
        h: 576,
        fps: 12.5,
        frames: 25,
        layers: FIVE,
    },
    Template {
        name: "TV PAL 4:3 en doublé 10 s, 5 calques",
        w: 768,
        h: 576,
        fps: 12.5,
        frames: 125,
        layers: FIVE,
    },
    Template {
        name: "Bannière web 10 i/s",
        w: 480,
        h: 60,
        fps: 10.0,
        frames: 50,
        layers: TWO,
    },
    Template {
        name: "Bannière web 15 i/s",
        w: 480,
        h: 60,
        fps: 15.0,
        frames: 75,
        layers: TWO,
    },
];

/// What a merge keeps, like the reference's `[LayerMerge]` settings (defaults:
/// KeepColorGroup 1, KeepImageMark 1, KeepInstanceName 1, Mode 0, Stamp 0, Erase 0).
#[derive(Clone, Copy, Debug)]
pub struct MergeOptions {
    /// The merged layer keeps a colour group: the upper layer's when it has one, else the lower
    /// layer's (off: no group).
    pub keep_color_group: bool,
    /// The merged drawings keep their names ("upper-lower" when both have one).
    pub keep_instance_names: bool,
    /// The merged drawings keep their image marks (the upper layer's when both have one).
    pub keep_image_marks: bool,
}

impl Default for MergeOptions {
    fn default() -> Self {
        MergeOptions {
            keep_color_group: true,
            keep_instance_names: true,
            keep_image_marks: true,
        }
    }
}

/// A frame as a time: whole seconds, then the frames into that second ("2:05").
pub fn timecode(frame: usize, fps: f32) -> String {
    let fps = fps.max(1.0);
    let sec = (frame as f32 / fps).floor();
    let rest = frame.saturating_sub((sec * fps).ceil() as usize);
    format!("{}:{:02}", sec as usize, rest)
}

/// A picture between `a` and `b` (straight-alpha RGBA): `t` = 0 gives `a`, 1 gives `b`.
pub fn cross_fade(a: &[u8], b: &[u8], t: f32) -> Vec<u8> {
    let t = t.clamp(0.0, 1.0);
    let mut out = vec![0u8; a.len()];
    for ((o, p), q) in out.chunks_exact_mut(4).zip(a.chunks_exact(4)).zip(b.chunks_exact(4)) {
        let wa = p[3] as f32 * (1.0 - t);
        let wb = q[3] as f32 * t;
        let alpha = wa + wb;
        if alpha <= 0.0 {
            continue;
        }
        for k in 0..3 {
            o[k] = ((p[k] as f32 * wa + q[k] as f32 * wb) / alpha).round() as u8;
        }
        o[3] = alpha.round().min(255.0) as u8;
    }
    out
}

/// Paint `src` over `dst` (both straight-alpha RGBA) with an opacity and a blending mode.
fn blend_layer(dst: &mut [u8], src: &[u8], opacity: f32, mode: Blend) {
    let op = opacity.clamp(0.0, 1.0);
    for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let sa = s[3] as f32 / 255.0 * op;
        if sa <= 0.0 {
            continue;
        }
        let da = d[3] as f32 / 255.0;
        if mode == Blend::Erase {
            d[3] = (da * (1.0 - sa) * 255.0).round() as u8;
            continue;
        }
        let oa = sa + da * (1.0 - sa);
        for k in 0..3 {
            let sc = if da > 0.0 {
                mode.mix(s[k] as u32, d[k] as u32) as f32
            } else {
                s[k] as f32
            };
            d[k] = ((sc * sa + d[k] as f32 * da * (1.0 - sa)) / oa).round().clamp(0.0, 255.0) as u8;
        }
        d[3] = (oa * 255.0).round() as u8;
    }
}

#[allow(clippy::too_many_arguments)]
fn blend(out: &mut [u8], base: usize, src: &[u8], w: usize, rect: Rect, opacity: f32, tint: Option<[u8; 3]>, mode: Blend, paper: [u8; 3]) {
    let op = (opacity.clamp(0.0, 1.0) * 256.0) as u32;
    for y in rect.y0..rect.y1 {
        let a = (y * w + rect.x0) * 4;
        let b = (y * w + rect.x1) * 4;
        let d = &mut out[a - base..b - base];
        let s = &src[a..b];
        for (dp, sp) in d.chunks_exact_mut(4).zip(s.chunks_exact(4)) {
            let sa = (sp[3] as u32 * op) >> 8;
            if sa == 0 {
                continue;
            }
            let src = match tint {
                Some(t) => [t[0] as u32, t[1] as u32, t[2] as u32],
                None => [sp[0] as u32, sp[1] as u32, sp[2] as u32],
            };
            let ia = 255 - sa;
            for k in 0..3 {
                let dk = dp[k] as u32;
                let target = if mode == Blend::Erase {
                    paper[k] as u32
                } else {
                    mode.mix(src[k], dk)
                };
                dp[k] = ((target * sa + dk * ia) / 255) as u8;
            }
        }
    }
}

/// Paper textures for the background (procedural, our own; the original ships 120 paper pictures).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
pub enum Paper {
    #[default]
    Smooth,
    Grain,
    Canvas,
    Watercolor,
}

pub const ALL_PAPERS: [Paper; 4] = [Paper::Smooth, Paper::Grain, Paper::Canvas, Paper::Watercolor];

impl Paper {
    pub fn label(self) -> &'static str {
        match self {
            Paper::Smooth => crate::i18n::tr("Lisse"),
            Paper::Grain => crate::i18n::tr("Grain fin"),
            Paper::Canvas => crate::i18n::tr("Toile"),
            Paper::Watercolor => crate::i18n::tr("Aquarelle"),
        }
    }

    /// Brightness change at a pixel, -1..=1.
    pub fn shade(self, x: usize, y: usize) -> f32 {
        match self {
            Paper::Smooth => 0.0,
            Paper::Grain => (hash2(x as i32, y as i32) - 0.5) * 0.12,
            Paper::Canvas => {
                let u = (x as f32 * 0.9).sin();
                let v = (y as f32 * 0.9).sin();
                let weave = if ((x / 4) + (y / 4)) % 2 == 0 { u } else { v };
                weave * 0.05 + (hash2(x as i32, y as i32) - 0.5) * 0.04
            }
            Paper::Watercolor => {
                let n = value_noise(x as f32 / 48.0, y as f32 / 48.0) * 0.6
                    + value_noise(x as f32 / 13.0, y as f32 / 13.0) * 0.3
                    + hash2(x as i32, y as i32) * 0.1;
                (n - 0.5) * 0.16
            }
        }
    }
}

fn hash2(x: i32, y: i32) -> f32 {
    let mut n = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
}

fn value_noise(x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (sx, sy) = (s(fx), s(fy));
    let a = hash2(xi, yi) + (hash2(xi + 1, yi) - hash2(xi, yi)) * sx;
    let b = hash2(xi, yi + 1) + (hash2(xi + 1, yi + 1) - hash2(xi, yi + 1)) * sx;
    a + (b - a) * sy
}

pub fn copy_rect(px: &[u8], w: usize, r: Rect) -> Vec<u8> {
    let mut v = Vec::with_capacity((r.x1 - r.x0) * (r.y1 - r.y0) * 4);
    for y in r.y0..r.y1 {
        v.extend_from_slice(&px[(y * w + r.x0) * 4..(y * w + r.x1) * 4]);
    }
    v
}

pub fn paste_rect(px: &mut [u8], w: usize, r: Rect, data: &[u8]) {
    let rw = (r.x1 - r.x0) * 4;
    for (i, y) in (r.y0..r.y1).enumerate() {
        px[(y * w + r.x0) * 4..(y * w + r.x1) * 4].copy_from_slice(&data[i * rw..(i + 1) * rw]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn compose_speed() {
        let (w, h) = (1920, 1080);
        let mut d = Doc::new(w, h, 24.0);
        for _ in 0..7 {
            d.add_layer();
        }
        for li in 0..8 {
            let id = d.new_drawing();
            d.layers[li].cels[0] = Some(id);
            for (i, p) in d.drawings[id].px.chunks_exact_mut(4).enumerate() {
                p[3] = ((i * (li + 1)) % 200) as u8;
                p[0] = li as u8 * 30;
            }
        }
        let fx = vec![None; 8];
        let mut out = vec![0u8; w * h * 4];
        let t = std::time::Instant::now();
        for _ in 0..10 {
            d.composite_band(0, false, Rect::full(w, h), &mut out, 0, &fx);
        }
        let one = t.elapsed().as_secs_f64() * 100.0;
        let t = std::time::Instant::now();
        for _ in 0..10 {
            d.composite(0, false, Rect::full(w, h), &mut out);
        }
        let par = t.elapsed().as_secs_f64() * 100.0;
        println!("1920x1080, 8 layers: one core {one:.1} ms, all cores {par:.1} ms per frame");
    }

    #[test]
    fn parallel_compose_matches_one_core() {
        let (w, h) = (640, 480);
        let mut d = Doc::new(w, h, 24.0);
        d.add_layer();
        d.add_layer();
        for li in 0..3 {
            let id = d.layers[li].cels[0].unwrap_or_else(|| d.new_drawing());
            d.layers[li].cels[0] = Some(id);
            for (i, p) in d.drawings[id].px.chunks_exact_mut(4).enumerate() {
                let v = (i * (li + 3) * 7919) as u32;
                p.copy_from_slice(&[(v >> 3) as u8, (v >> 7) as u8, (v >> 11) as u8, ((v >> 5) & 0xff) as u8]);
            }
        }
        d.layers[1].blend = Blend::Multiply;
        d.layers[2].opacity = 0.6;
        d.layers[2].fx.push(crate::fx::FxEntry {
            effect: crate::fx::Effect::Invert,
            keys: vec![],
            on: true,
        });
        let mut par = vec![0u8; w * h * 4];
        d.composite(0, true, Rect::full(w, h), &mut par);
        let fx: Vec<_> = (0..3)
            .map(|li| {
                if li == 2 {
                    Some(d.with_fx(2, d.layers[2].cels[0].unwrap(), 0))
                } else {
                    None
                }
            })
            .collect();
        let mut one = vec![0u8; w * h * 4];
        d.composite_band(0, true, Rect::full(w, h), &mut one, 0, &fx);
        assert!(par == one, "the bands must give the same picture");
        // a small area (one core) inside the canvas
        let r = Rect {
            x0: 10,
            y0: 20,
            x1: 50,
            y1: 60,
        };
        let mut part = par.clone();
        d.composite(0, true, r, &mut part);
        assert!(part == par);
    }

    fn doc4() -> Doc {
        // one layer, drawings at 0, 1, 2, 3, project 8 frames
        let mut d = Doc::new(4, 4, 12.0);
        d.set_length(8);
        for f in 1..4 {
            let id = d.new_drawing();
            d.layers[0].cels[f] = Some(id);
        }
        d
    }

    #[test]
    fn merging_down_paints_the_upper_layer_into_the_lower() {
        let mut d = Doc::new(2, 1, 12.0);
        d.set_length(3);
        // lower: red on the left pixel, held 3 frames
        d.drawings[d.layers[0].cels[0].unwrap()].px[0..4].copy_from_slice(&[255, 0, 0, 255]);
        d.add_layer();
        // upper: blue on the right pixel at frame 0, a new empty drawing at frame 2
        let up0 = d.layers[1].cels[0].unwrap();
        d.drawings[up0].px[4..8].copy_from_slice(&[0, 0, 255, 255]);
        let empty = d.new_drawing();
        d.layers[1].cels[2] = Some(empty);
        d.layers[1].group = [1, 2, 3];
        assert!(d.merge_down(MergeOptions::default()));
        assert_eq!(d.layers.len(), 1);
        let px = |d: &Doc, f| d.drawings[d.drawing_at(0, f).unwrap()].px.clone();
        assert_eq!(px(&d, 0), vec![255, 0, 0, 255, 0, 0, 255, 255]);
        assert_eq!(px(&d, 1), px(&d, 0), "frame 1 holds");
        assert_eq!(&px(&d, 2)[4..8], &[0, 0, 0, 0], "frame 2 shows the upper layer's empty drawing");
        assert_eq!(
            d.layers[0].group,
            [1, 2, 3],
            "the upper layer's colour group is taken when it has one"
        );
        assert!(d.undo());
        assert_eq!(d.layers.len(), 2);
    }

    #[test]
    fn typing_numbers_in_the_exposure_sheet() {
        let mut d = doc4(); // drawings 1..4 at frames 0..3
        let first = d.drawing_at(0, 0).unwrap();
        assert!(d.set_cell_number(0, 5, "1"));
        assert_eq!(d.drawing_at(0, 5), Some(first));
        assert_eq!(d.instance_number(0, 5), Some(1), "a reused drawing keeps its number");
        assert!(d.set_cell_number(0, 6, "9"));
        assert_ne!(d.drawing_at(0, 6), Some(first));
        assert_eq!(d.instance_number(0, 6), Some(5), "a new drawing takes the next number");
        assert!(d.set_cell_number(0, 7, "-"));
        assert_eq!(d.drawing_at(0, 7), d.drawing_at(0, 6), "a dash holds");
        assert!(!d.set_cell_number(0, 7, "abc"));
    }

    #[test]
    fn a_layer_effect_changes_the_picture_not_the_drawing() {
        let mut d = Doc::new(2, 1, 12.0);
        let id = d.layers[0].cels[0].unwrap();
        d.drawings[id].px.copy_from_slice(&[255, 255, 255, 255, 255, 255, 255, 255]);
        d.layers[0].fx.push(crate::fx::FxEntry::new(crate::fx::Effect::Invert));
        let pic = d.render(0);
        assert_eq!(&pic[0..3], &[0, 0, 0], "the effect shows");
        assert_eq!(&d.drawings[id].px[0..3], &[255, 255, 255], "the drawing is unchanged");
        assert!(d.apply_fx_stack(0));
        assert!(d.layers[0].fx.is_empty());
        assert_eq!(&d.drawings[d.drawing_at(0, 0).unwrap()].px[0..3], &[0, 0, 0], "applied for good");
    }

    #[test]
    fn layers_copy_paste_and_stretch() {
        let mut d = doc4(); // drawings at 0..=3 of 8 frames
        let c = d.copy_layer(0);
        d.paste_layer(&c);
        assert_eq!(d.layers.len(), 2);
        assert_ne!(d.drawing_at(1, 0), d.drawing_at(0, 0), "the pasted layer has its own drawings");
        d.layers[0].end = Some(3);
        assert!(d.stretch_layer(0, 8));
        let keys: Vec<usize> = d.keys(0).iter().map(|k| k.0).collect();
        assert_eq!(keys, vec![0, 2, 4, 6], "four drawings over eight frames");
        assert_eq!(d.layers[0].end, Some(7));
        assert!(d.stretch_layer(0, 2));
        assert_eq!(d.keys(0).len(), 2, "shrinking drops the drawings that land on the same frame");
    }

    #[test]
    fn templates_and_repeats() {
        let d = Doc::from_template(&TEMPLATES[2]);
        assert_eq!((d.w, d.h, d.frames), (800, 600, 300));
        assert_eq!(d.layers.len(), 5);
        assert!(!d.can_undo());
        let mut d = doc4(); // drawings at 0, 1, 2, 3 of 8 frames
        let steps = d.history().0.len();
        let end = d.repeat_cells(0, 0, 3, 1, RepeatMode::PingPong).unwrap();
        assert_eq!(end, 6);
        let shown: Vec<usize> = (0..7).map(|f| d.drawing_at(0, f).unwrap()).collect();
        let k: Vec<usize> = d.keys(0).iter().take(4).map(|x| x.1).collect();
        assert_eq!(
            shown[4..7].to_vec(),
            vec![k[2], k[1], k[0]],
            "back without doubling the last drawing"
        );
        let (undo, redo) = d.history();
        assert_eq!((undo.len(), redo.len()), (steps + 1, 0), "one undo step for the repeat");
        // a single held cell is refused, as in the reference
        let held = d.frames - 1;
        assert!(d.layers[0].cels[held].is_none());
        assert!(d.repeat_cells(0, held, held, 1, RepeatMode::Loop).is_none());
        assert_eq!(RepeatMode::PingPong.id(), 4);
    }

    #[test]
    fn painting_on_a_hold_breaks_it() {
        let mut d = Doc::new(4, 4, 12.0);
        d.set_length(3);
        d.frame = 2;
        let held = d.drawing_at(0, 2).unwrap();
        let id = d.target_drawing().unwrap();
        assert_ne!(id, held);
        assert_eq!(d.keys(0).len(), 2);
        d.auto_break = false;
        d.frame = 1;
        assert_eq!(d.target_drawing(), Some(held));
        assert_eq!(d.layers[0].post, Behavior::Hold);
    }

    #[test]
    fn painting_after_the_end_stretches_or_asks() {
        let mut d = doc4();
        d.layers[0].end = Some(3);
        d.frame = 5;
        assert!(d.target_drawing().is_some());
        assert_eq!(d.layers[0].end, Some(5));
        d.auto_stretch = false;
        d.frame = 7;
        assert!(d.target_drawing().is_none());
        assert_eq!(d.stretch_asked, Some((0, 7)));
        assert_eq!(d.layers[0].end, Some(5));
    }

    #[test]
    fn behaviours_after_the_end() {
        let mut d = doc4();
        d.layers[0].end = Some(3);
        let k = |d: &Doc, f| d.source_frame(0, f);
        assert_eq!(k(&d, 6), Some(3)); // new layers hold after their end
        d.layers[0].post = Behavior::None;
        assert_eq!(k(&d, 5), None);
        d.layers[0].post = Behavior::Hold;
        assert_eq!(k(&d, 6), Some(3));
        d.layers[0].post = Behavior::Repeat;
        assert_eq!((4..8).map(|f| k(&d, f).unwrap()).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
        d.layers[0].post = Behavior::PingPong;
        assert_eq!((4..8).map(|f| k(&d, f).unwrap()).collect::<Vec<_>>(), vec![2, 1, 0, 1]);
        assert_eq!(Behavior::PingPong.id(), 2);
    }

    #[test]
    fn marks_notes_and_tweens() {
        let mut d = doc4(); // drawings at 0, 1, 2, 3 of 8 frames
        assert!(d.set_mark(0, 2, 1));
        assert!(d.set_mark(0, 5, 3), "a held cell marks the drawing it shows");
        assert_eq!(d.mark_at(0, 3), Some(3));
        assert_eq!(d.mark_at(0, 7), Some(3));
        assert_eq!(d.next_mark(0, 0), Some(2));
        assert_eq!(d.next_mark(0, 2), Some(3));
        assert_eq!(d.prev_mark(0, 7), Some(2), "from a hold, the drawing before the one shown");
        assert_eq!(d.prev_mark(0, 2), None);
        assert!(d.set_mark(0, 2, 0));
        assert_eq!(d.mark_at(0, 2), None);
        assert!(d.undo());
        assert_eq!(d.mark_at(0, 2), Some(1), "a mark can be undone");
        // notes
        d.set_note(4, " hello ");
        d.set_note(1, "first");
        assert_eq!(d.notes.get(&4).map(String::as_str), Some("hello"));
        assert_eq!((d.next_note(1), d.prev_note(4), d.next_note(4)), (Some(4), Some(1), None));
        d.set_note(4, "");
        assert!(!d.notes.contains_key(&4));
        // tweens: two half-way pictures between the drawings at 0 and 1
        let (a, b) = (d.drawing_at(0, 0).unwrap(), d.drawing_at(0, 1).unwrap());
        d.drawings[a].px[0..4].copy_from_slice(&[200, 0, 0, 255]);
        d.drawings[b].px[0..4].copy_from_slice(&[0, 100, 0, 255]);
        d.drawings[b].px[4..8].copy_from_slice(&[0, 0, 90, 255]);
        let frames = d.frames;
        assert_eq!(d.insert_tweens(0, 0, 3, true), Some(1));
        assert_eq!(d.frames, frames + 3);
        assert_eq!(d.drawing_at(0, 4), Some(b), "the next drawing moved after the tweens");
        let mid = &d.drawings[d.drawing_at(0, 2).unwrap()].px;
        assert_eq!(&mid[0..4], &[100, 50, 0, 255], "half of each where both are drawn");
        assert_eq!(&mid[4..8], &[0, 0, 90, 128], "half as opaque where only one is drawn");
        assert!(
            d.insert_tweens(0, d.frames - 1, 1, false).is_none(),
            "no drawing after the last one"
        );
        assert_eq!(timecode(0, 24.0), "0:00");
        assert_eq!(timecode(23, 24.0), "0:23");
        assert_eq!(timecode(53, 24.0), "2:05");
        assert_eq!(timecode(13, 12.5), "1:00", "the first whole frame of the second second at 12.5 fps");
        assert!(d.undo());
        assert_eq!(d.frames, frames);
    }

    #[test]
    fn shifting_sliding_and_splitting_layers() {
        let keys = |d: &Doc, li: usize| d.keys(li).iter().map(|k| k.0).collect::<Vec<_>>();
        let mut d = doc4(); // drawings at 0, 1, 2, 3 of 8 frames
        assert_eq!(d.shift_layers(&[0], 2), 2);
        assert_eq!(keys(&d, 0), vec![2, 3, 4, 5]);
        assert_eq!(d.frames, 8, "the holds at the end make room");
        assert_eq!(d.shift_layers(&[0], 5), 5);
        assert_eq!(
            (keys(&d, 0), d.frames),
            (vec![7, 8, 9, 10], 11),
            "the project gets longer to keep the drawings"
        );
        assert_eq!(d.shift_layers(&[0], -100), -7, "earlier stops at the first frame");
        assert_eq!(keys(&d, 0), vec![0, 1, 2, 3]);
        assert_eq!(d.shift_layers(&[0], -1), 0);
        d.layers[0].end = Some(5);
        d.shift_layers(&[0], 1);
        assert_eq!(d.layers[0].end, Some(6), "the layer's end moves with it");
        // two layers move together, by what the tightest one allows
        let mut d = doc4();
        d.add_layer(); // its drawing is at frame 0
        d.shift_layers(&[0, 1], 3);
        assert_eq!(d.shift_layers(&[0, 1], -5), -3);
        assert_eq!((keys(&d, 0)[0], keys(&d, 1)[0]), (0, 0));
        // slide: the content moves inside the same frames
        let mut d = doc4();
        d.layers[0].end = Some(5);
        let ids: Vec<usize> = d.keys(0).iter().map(|k| k.1).collect();
        assert!(d.slide_layer(0, 1));
        assert_eq!(keys(&d, 0), vec![0, 2, 3, 4]);
        assert_eq!(d.layers[0].end, Some(5));
        assert!(d.slide_layer(0, -2));
        assert_eq!(d.drawing_at(0, 0), Some(ids[1]), "what showed two frames later is now first");
        assert_eq!(keys(&d, 0), vec![0, 1, 2]);
        assert_eq!(d.drawing_at(0, 5), Some(ids[3]), "the last drawing is held to the end");
        assert!(!d.slide_layer(0, 0));
        // split
        let mut d = doc4();
        let last = d.drawing_at(0, 3).unwrap();
        d.layers[0].names.insert(last, "last".into());
        assert!(!d.split_layer(0, 0), "nothing before the first drawing");
        assert!(d.split_layer(0, 5), "splitting inside a hold");
        assert_eq!(d.layers.len(), 2);
        assert_eq!((d.layers[0].end, d.current_layer), (Some(4), 1));
        assert_eq!(keys(&d, 1), vec![5]);
        assert_ne!(
            d.drawing_at(1, 5),
            d.drawing_at(0, 4),
            "the second part has its own copy of the held drawing"
        );
        assert_eq!(d.instance_name(1, 5).as_deref(), Some("last"));
        assert_eq!(d.shown_drawing(0, 6), None, "the first part shows nothing after its end");
        assert_eq!(d.shown_drawing(1, 2), None, "the second part shows nothing before the split");
        assert!(d.undo());
        assert_eq!(d.layers.len(), 1);
        assert!(d.split_layer(0, 2), "splitting on a drawing moves it");
        assert_eq!((keys(&d, 0), keys(&d, 1)), (vec![0, 1], vec![2, 3]));
    }

    #[test]
    fn several_layers_at_once() {
        let mut d = Doc::new(2, 1, 12.0);
        d.drawings[d.layers[0].cels[0].unwrap()].px[0..4].copy_from_slice(&[255, 0, 0, 255]);
        d.add_layer();
        d.add_layer();
        d.drawings[d.layers[2].cels[0].unwrap()].px[4..8].copy_from_slice(&[0, 0, 255, 255]);
        d.add_layer();
        assert_eq!(d.target_layers(), vec![3], "the current layer alone");
        d.layers[0].selected = true;
        d.current_layer = 2;
        assert_eq!(d.target_layers(), vec![0, 2]);
        assert!(d.merge_layers(&[0, 2], MergeOptions::default()));
        assert_eq!((d.layers.len(), d.current_layer), (3, 0), "layers 1 and 3 are untouched");
        let px = &d.drawings[d.drawing_at(0, 0).unwrap()].px;
        assert_eq!(px, &vec![255, 0, 0, 255, 0, 0, 255, 255]);
        assert!(!d.merge_layers(&[1], MergeOptions::default()));
        d.select_layers(true);
        let all = d.target_layers();
        assert_eq!(d.delete_layers(&all), 2, "one layer always stays");
        assert_eq!(d.layers.len(), 1);
        assert!(d.undo());
        assert_eq!(d.layers.len(), 3);
    }

    #[test]
    fn set_exposure_and_move_cells() {
        let mut d = doc4();
        let last = d.set_exposure(0, 0, 3, 2).unwrap();
        assert_eq!(last, 7);
        let keys: Vec<usize> = d.keys(0).iter().map(|k| k.0).collect();
        assert_eq!(keys, vec![0, 2, 4, 6]);
        // the block 2..=3 moves to 3..=4; the drawing that was at 4 goes before it
        assert!(d.move_cells(0, 2, 3, true));
        let keys: Vec<usize> = d.keys(0).iter().map(|k| k.0).collect();
        assert_eq!(keys, vec![0, 2, 3, 6]);
        d.toggle_bookmark(5);
        d.toggle_bookmark(2);
        assert_eq!(d.next_bookmark(2), Some(5));
        assert_eq!(d.prev_bookmark(5), Some(2));
        d.mark_in = Some(2);
        d.mark_out = Some(4);
        assert_eq!(d.play_range(), (2, 4));
    }
}
