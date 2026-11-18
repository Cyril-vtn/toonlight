//! The window: menus, tool panel, canvas, light-table panel and timeline.

use crate::brush::{self, Settings, Stroke, Tool, ALL_TOOLS};
use crate::custom::{CustomBrush, CustomSettings, CustomStroke, Preset};
use crate::doc::{copy_rect, Doc, Rect};
use crate::fx::{Effect, ALL_EFFECTS};
use crate::i18n::{self, tr, trf};
use crate::io;
use crate::layout::{PanelId, Side, ALL_PANELS};
use crate::select::{Combine, Floating, GradientKind, SelectMode, Selection};
use crate::sound::{Clip, Player, SoundTrack};
use crate::text::{self as textmod, TextSettings};

/// A change to the current room's panels, applied at the end of the frame.
#[derive(Clone, Copy, Debug)]
enum LayoutOp {
    Place(PanelId, Side, usize),
    Float(PanelId, [f32; 2]),
    Hide(PanelId),
}

/// Width of a frame in the timeline, in points: default, and the zoom limits.
const TL_CELL: f32 = crate::layout::TIMELINE_CELL;
const TL_CELL_MIN: f32 = 2.0;
const TL_CELL_MAX: f32 = 64.0;

/// What "Fit" shows across the timeline's width.
#[derive(Clone, Copy)]
enum TlFit {
    Clip,
    Selection,
    Sound,
}

/// Open the shortcuts window at start (`--shortcuts`, for checks).
pub static OPEN_SHORTCUTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Open the colour group editor at start (`--groups`, for checks).
pub static OPEN_GROUPS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Start option `--history`: open the undo history window.
pub static OPEN_HISTORY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Start option `--tool <name>`: the tool selected at start (pen, eraser, camera…).
pub static START_TOOL: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
/// Start option `--commands <file>`: AI commands (one JSON-RPC message per line) run on the
/// project once it is open, to set up a state for a check or a screenshot.
pub static START_COMMANDS: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Open the storyboard window at start (`--storyboard`, for checks).
pub static OPEN_STORYBOARD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A clip (shot) of the project. The active clip's animation lives in `AnimApp::doc`.
struct ClipSlot {
    name: String,
    notes: String,
    doc: Option<Doc>,
    thumb: Option<egui::TextureHandle>,
}

impl ClipSlot {
    fn new(name: String, doc: Option<Doc>) -> ClipSlot {
        ClipSlot {
            name,
            notes: String::new(),
            doc,
            thumb: None,
        }
    }
}

/// An effect being set up, previewed on the current drawing.
struct FxDialog {
    fx: Effect,
    all_layer: bool,
    id: usize,
    before: Vec<u8>,
}
use eframe::egui::{self, pos2, vec2, Color32, Key, Pos2, Sense, Stroke as Line, StrokeKind, Vec2};
use std::path::PathBuf;
use std::time::Instant;

/// Page sizes from Data/pagesize.conf (name, width, height, fps).
const PAGE_SIZES: [(&str, usize, usize, f32); 7] = [
    ("HD 1080", 1920, 1080, 24.0),
    ("HD 720", 1280, 720, 24.0),
    ("VGA", 640, 480, 30.0),
    ("SVGA", 800, 600, 30.0),
    ("XGA", 1024, 768, 30.0),
    ("WUXGA", 1920, 1200, 30.0),
    ("QXGA", 2048, 1536, 30.0),
];

#[derive(Clone, Copy)]
struct Theme {
    name: &'static str,
    dark: bool,
    panel: Color32,
    widget: Color32,
    hover: Color32,
    selected: Color32,
    text: Color32,
    text_bg: Color32,
    doc_bg: Color32,
    border: Color32,
    cell: Color32,
}

/// Colours from the shipped colour schemes (colorschemes/Orage.ini and Acier.ini).
const THEMES: [Theme; 2] = [
    Theme {
        name: "Orage (sombre)",
        dark: true,
        panel: Color32::from_rgb(72, 72, 72),
        widget: Color32::from_rgb(92, 92, 92),
        hover: Color32::from_rgb(110, 110, 110),
        selected: Color32::from_rgb(96, 127, 128),
        text: Color32::from_rgb(200, 200, 200),
        text_bg: Color32::from_rgb(62, 82, 82),
        doc_bg: Color32::from_rgb(52, 52, 52),
        border: Color32::from_rgb(52, 52, 52),
        cell: Color32::from_rgb(77, 101, 101),
    },
    Theme {
        name: "Acier (clair)",
        dark: false,
        panel: Color32::from_rgb(161, 161, 161),
        widget: Color32::from_rgb(188, 188, 188),
        hover: Color32::from_rgb(199, 199, 199),
        selected: Color32::from_rgb(135, 163, 163),
        text: Color32::from_rgb(0, 0, 0),
        text_bg: Color32::from_rgb(221, 221, 221),
        doc_bg: Color32::from_rgb(102, 102, 102),
        border: Color32::from_rgb(120, 120, 120),
        cell: Color32::from_rgb(103, 135, 135),
    },
];

struct NewDialog {
    /// A template (index in `TEMPLATES`), or none.
    template: Option<usize>,
    preset: usize,
    w: usize,
    h: usize,
    fps: f32,
}

enum Shot {
    None,
    Counting(PathBuf, u32),
    Waiting(PathBuf),
}

pub struct AnimApp {
    doc: Doc,
    tool: Tool,
    settings: Vec<Settings>,
    color: [u8; 3],
    color2: [u8; 3],
    zoom: f32,
    pan: Vec2,
    fit_pending: bool,
    tex: Option<egui::TextureHandle>,
    comp: Vec<u8>,
    needs_full: bool,
    shown: (usize, bool),
    stroke: Option<Stroke>,
    shape_start: Option<(f32, f32)>,
    playing: bool,
    looping: bool,
    last_tick: Instant,
    theme: usize,
    tl_scroll: Vec2,
    path: Option<PathBuf>,
    file_stamp: Option<std::time::SystemTime>,
    last_watch: Instant,
    show_ai: bool,
    show_xsheet: bool,
    /// The shot's sound files, decoded (one per sound track, None when a file can't be read),
    /// the paths they were read from, and each track's loudness per frame for the timeline.
    audio: Vec<Option<Clip>>,
    audio_paths: Vec<PathBuf>,
    player: Player,
    peaks: Vec<Vec<f32>>,
    peaks_key: String,
    text: TextSettings,
    font: Option<ab_glyph::FontVec>,
    pressure: f32,
    sim_pressure: bool,
    /// The pen's values when the system reports a tablet (macOS: pressure, tilt, rotation, eraser
    /// end), and the tool to come back to after a stroke made with the pen's eraser end.
    pen: Option<crate::tablet::Pen>,
    eraser_back: Option<Tool>,
    /// Frame to come back to after flipping ("comebacktoorigin=1", Data/flips.conf).
    flip_origin: Option<usize>,
    show_grid: bool,
    grid_size: f32,
    show_thirds: bool,
    show_center: bool,
    selection: Option<Selection>,
    sel_mode: SelectMode,
    sel_combine: Combine,
    sel_drag: Option<Vec<(f32, f32)>>,
    sel_tex: Option<egui::TextureHandle>,
    sel_dirty: bool,
    wand_tol: f32,
    floating: Option<Floating>,
    float_drag: Option<(f32, f32)>,
    clipboard: Option<(Vec<u8>, usize, usize, usize, usize)>,
    /// A fingerprint of the picture this app last put on the system clipboard, to tell our own
    /// copy (pasted back in place) from a picture copied in another program (pasted centred).
    sys_clip_mark: Option<u64>,
    grad_kind: GradientKind,
    grad_mid: f32,
    grad_start: Option<(f32, f32)>,
    custom_brush: Option<std::sync::Arc<CustomBrush>>,
    custom_tex: Option<egui::TextureHandle>,
    custom_set: CustomSettings,
    custom_stroke: Option<CustomStroke>,
    presets: Vec<Preset>,
    preset_name: String,
    fx_dialog: Option<FxDialog>,
    /// Show the canvas through the camera (view only).
    cam_view: bool,
    cam_drag: Option<(f32, f32)>,
    clips: Vec<ClipSlot>,
    active_clip: usize,
    show_storyboard: bool,
    /// Play every clip one after the other.
    play_film: bool,
    keymap: crate::keymap::KeyMap,
    show_shortcuts: bool,
    shortcut_filter: String,
    /// Waiting for a key for this action (true = add, not replace).
    capture: Option<(crate::keymap::Action, bool)>,
    /// View rotation in degrees (steps of 15°, "Zoom: Rotate +15") and mirror ("Zoom: Flip horizontally").
    view_rot: f32,
    view_flip: bool,
    /// Paint only on existing pixels ("Tools: Toggle Preserve Transparency").
    preserve: bool,
    /// Paint inside the stencil layers' drawings instead of around them.
    stencil_invert: bool,
    /// Line smoothing ("Line Smoothing": Type 1 average, 2 pulled string, 3 average points; Value;
    /// IsActive; IsRealTime; IsHudActivated; CatchUp).
    smooth_on: bool,
    smooth_type: u8,
    smooth_value: f32,
    smooth_catchup: bool,
    smooth_hud: bool,
    smooth_pos: Option<(f32, f32)>,
    smooth_hist: std::collections::VecDeque<(f32, f32)>,
    smooth_raw: Option<(f32, f32)>,
    /// Rooms (workspaces) and the current one; panel moves asked this frame.
    rooms: Vec<crate::layout::Room>,
    room: usize,
    layout_ops: Vec<LayoutOp>,
    layout_dirty: bool,
    left_overlay: Option<egui::Rect>,
    right_overlay: Option<egui::Rect>,
    tl_open: bool,
    room_name_edit: String,
    /// Cells selected in the timeline: (layer, first frame, last frame).
    cell_sel: Option<(usize, usize, usize)>,
    cell_anchor: usize,
    /// Cells copied from the timeline, with their instance names and image marks.
    cell_clip: Vec<Option<usize>>,
    cell_clip_names: std::collections::BTreeMap<usize, String>,
    cell_clip_marks: std::collections::BTreeMap<usize, u8>,
    /// Timeline zoom: the width of a frame, the width last drawn (to keep the frame under the
    /// pointer in place when it changes), a fit asked for, and the frame last scrolled to.
    tl_cell: f32,
    tl_cell_shown: f32,
    tl_fit: Option<TlFit>,
    tl_frame_seen: usize,
    /// The timeline's ruler shows seconds:frames instead of frame numbers.
    tl_timecode: bool,
    /// The notes row of the timeline, and the note being written: (frame, text).
    tl_notes: bool,
    note_edit: Option<(usize, String)>,
    /// The colour (1-12) "Mark the image" gives, and how "Insert in-betweens" works.
    mark_pick: u8,
    tween_n: usize,
    tween_mix: bool,
    /// How many exposures "Exposure +/−" add or remove, and the value "Set exposure" gives.
    expo_n: usize,
    /// Audio scrubbing: 0 never, 1 while dragging in the timeline, 2 always (reference "ScrubMode",
    /// default 1), for `scrub_secs` ("ScrubDuration", 0.1 s).
    scrub_mode: u8,
    scrub_secs: f32,
    scrub_last: usize,
    tl_drag: bool,
    tl_menu_frame: usize,
    /// The current mouse / pen press has already started a tool.
    tool_pressed: bool,
    /// Preferences (Edit > Preferences…) and their window.
    prefs: crate::prefs::Prefs,
    show_prefs: bool,
    /// When the last crash-recovery copy was written.
    last_recovery: Instant,
    /// A recovery copy found at start, offered back.
    recovery_offer: Option<PathBuf>,
    /// The storyboard's HTML export was asked for (done after the window is drawn).
    export_board_pending: bool,
    /// How many times "Repeat" copies the cells.
    repeat_n: usize,
    show_history: bool,
    /// A layer copied with "Copy layer" (with its drawings).
    layer_clip: Option<crate::doc::LayerCopy>,
    /// Colour group panel: the groups it lists (reference +0x135, all on) and the protected ones
    /// (+0xad).
    group_listed: [bool; 13],
    group_protect: [bool; 13],
    /// Groups whose layers the timeline shows (all on at start).
    group_in_timeline: [bool; 13],
    /// The colour group editor: the table being changed, applied in one go.
    group_edit: Option<Vec<crate::doc::Group>>,
    /// The length "Stretch layer" gives, and the frames "Shift" and "Slide" move by.
    stretch_len: usize,
    shift_n: i64,
    /// The time ratio "Stretch" applies, in percent, and the exposure "Retime" gives each drawing.
    stretch_pct: f32,
    retime_n: usize,
    /// The layers' small pictures in the timeline, by layer: what each was made from, and it.
    thumbs: std::collections::HashMap<usize, (ThumbKey, egui::TextureHandle)>,
    /// Counters that change when the picture does: any change, and paint on the current drawing.
    full_stamp: u64,
    paint_stamp: u64,
    /// AI commands arriving from `toonlight --mcp` (the bridge), and the tool state kept between them.
    bridge: Option<crate::mcp::BridgeRequests>,
    bridge_srv: Option<crate::mcp::Server>,
    /// The layer effect stack window, the effect picked in it, the effect stacks saved as
    /// presets and the name the next one gets.
    show_fx_stack: bool,
    fx_pick: usize,
    fx_presets: Vec<crate::fx::FxPreset>,
    fx_preset_name: String,
    /// The exposure-sheet cell being typed in: (layer, frame, text).
    xs_edit: Option<(usize, usize, String)>,
    status: String,
    new_dialog: Option<NewDialog>,
    shot: Shot,
    cursor_px: Option<(f32, f32)>,
}

impl AnimApp {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, open: Option<PathBuf>) -> Self {
        i18n::set_lang(i18n::load_lang());
        let prefs = crate::prefs::load();
        prefs.apply_globals();
        crate::prefs::load_default_groups();
        install_fonts(&cc.egui_ctx);
        crate::tablet::install();
        let mut app = AnimApp {
            doc: sample_doc(),
            tool: Tool::Pen,
            settings: ALL_TOOLS.iter().map(|t| t.defaults()).collect(),
            color: [20, 20, 30],
            color2: [255, 255, 255],
            zoom: 0.5,
            pan: Vec2::ZERO,
            fit_pending: true,
            tex: None,
            comp: vec![],
            needs_full: true,
            shown: (usize::MAX, false),
            stroke: None,
            shape_start: None,
            playing: false,
            looping: true,
            last_tick: Instant::now(),
            theme: 0,
            tl_scroll: Vec2::ZERO,
            path: None,
            file_stamp: None,
            last_watch: Instant::now(),
            show_ai: false,
            show_xsheet: false,
            audio: vec![],
            audio_paths: vec![],
            player: Player::default(),
            peaks: vec![],
            peaks_key: String::new(),
            text: TextSettings {
                text: tr("Texte").into(),
                ..TextSettings::default()
            },
            font: None,
            pressure: 1.0,
            sim_pressure: false,
            pen: None,
            eraser_back: None,
            flip_origin: None,
            show_grid: false,
            grid_size: 100.0,
            show_thirds: false,
            show_center: false,
            selection: None,
            sel_mode: SelectMode::Rect,
            sel_combine: Combine::Replace,
            sel_drag: None,
            sel_tex: None,
            sel_dirty: false,
            wand_tol: 15.0,
            floating: None,
            float_drag: None,
            clipboard: None,
            sys_clip_mark: None,
            grad_kind: GradientKind::Linear,
            grad_mid: 0.5,
            grad_start: None,
            custom_brush: None,
            custom_tex: None,
            custom_set: CustomSettings::default(),
            custom_stroke: None,
            presets: crate::custom::load_presets(),
            preset_name: String::new(),
            fx_dialog: None,
            cam_view: false,
            cam_drag: None,
            clips: vec![ClipSlot::new(format!("{} 1", tr("Plan")), None)],
            active_clip: 0,
            show_storyboard: OPEN_STORYBOARD.load(std::sync::atomic::Ordering::Relaxed),
            play_film: false,
            keymap: crate::keymap::load(),
            show_shortcuts: OPEN_SHORTCUTS.load(std::sync::atomic::Ordering::Relaxed),
            shortcut_filter: String::new(),
            capture: None,
            view_rot: 0.0,
            view_flip: false,
            preserve: false,
            stencil_invert: false,
            smooth_on: false,
            smooth_type: 2,
            smooth_value: 20.0,
            smooth_catchup: true,
            smooth_hud: true,
            smooth_pos: None,
            smooth_hist: std::collections::VecDeque::new(),
            smooth_raw: None,
            rooms: crate::layout::load().0,
            room: crate::layout::load().1,
            layout_ops: vec![],
            layout_dirty: false,
            left_overlay: None,
            right_overlay: None,
            tl_open: false,
            room_name_edit: String::new(),
            cell_sel: None,
            cell_anchor: 0,
            cell_clip: vec![],
            cell_clip_names: Default::default(),
            cell_clip_marks: Default::default(),
            tl_cell: TL_CELL,
            tl_cell_shown: TL_CELL,
            tl_fit: None,
            tl_frame_seen: usize::MAX,
            tl_timecode: false,
            tl_notes: true,
            note_edit: None,
            mark_pick: 1,
            tween_n: 1,
            tween_mix: true,
            expo_n: 1,
            scrub_mode: 1,
            scrub_secs: 0.1,
            scrub_last: 0,
            tl_drag: false,
            tl_menu_frame: 0,
            tool_pressed: false,
            prefs: crate::prefs::Prefs::default(),
            show_prefs: false,
            last_recovery: Instant::now(),
            recovery_offer: None,
            export_board_pending: false,
            repeat_n: 1,
            show_history: OPEN_HISTORY.load(std::sync::atomic::Ordering::Relaxed),
            xs_edit: None,
            layer_clip: None,
            group_listed: [true; 13],
            group_protect: [false; 13],
            group_in_timeline: [true; 13],
            group_edit: None,
            stretch_len: 24,
            shift_n: 1,
            stretch_pct: 200.0,
            retime_n: 2,
            thumbs: Default::default(),
            full_stamp: 0,
            paint_stamp: 0,
            show_fx_stack: false,
            bridge: None,
            bridge_srv: None,
            fx_pick: 0,
            fx_presets: crate::fx::load_presets(),
            fx_preset_name: String::new(),
            status: tr("Exemple chargé : une balle qui rebondit. Fichier > Nouveau pour une page blanche.").into(),
            new_dialog: None,
            shot: match screenshot {
                Some(p) => Shot::Counting(p, 0),
                None => Shot::None,
            },
            cursor_px: None,
        };
        if let Some(p) = open {
            app.open_path(p);
        }
        if let Some(name) = START_TOOL.lock().unwrap().take() {
            match ALL_TOOLS.iter().find(|t| format!("{t:?}").eq_ignore_ascii_case(&name)) {
                Some(t) => app.tool = *t,
                None => eprintln!("--tool: no tool named {name}"),
            }
        }
        if let Some(p) = START_COMMANDS.lock().unwrap().take() {
            match std::fs::read_to_string(&p) {
                Ok(text) => {
                    let lines: Vec<String> = text.lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
                    for answer in app.run_commands(&lines) {
                        if answer.contains("\"isError\":true") || answer.contains("\"error\"") {
                            eprintln!("--commands: {answer}");
                        }
                    }
                }
                Err(e) => eprintln!("--commands: {}: {e}", p.display()),
            }
        }
        if OPEN_GROUPS.load(std::sync::atomic::Ordering::Relaxed) {
            app.group_edit = Some(app.doc.groups.clone());
        }
        apply_theme(&cc.egui_ctx, &THEMES[app.theme]);
        if matches!(app.shot, Shot::None) {
            let ctx = cc.egui_ctx.clone();
            app.bridge = crate::mcp::start_bridge(move || ctx.request_repaint());
        }
        app.tl_cell = app.rooms[app.room].timeline_cell.clamp(TL_CELL_MIN, TL_CELL_MAX);
        app.tl_cell_shown = app.tl_cell;
        app.scrub_mode = prefs.scrub_mode;
        app.scrub_secs = prefs.scrub_secs;
        app.prefs = prefs;
        if matches!(app.shot, Shot::None) {
            if let Some(r) = crate::prefs::recovery_path().filter(|r| r.is_file()) {
                app.recovery_offer = Some(r);
            }
        }
        app
    }

    fn set(&self) -> Settings {
        self.settings[self.tool.index()]
    }

    fn set_mut(&mut self) -> &mut Settings {
        let i = self.tool.index();
        &mut self.settings[i]
    }

    fn replace_doc(&mut self, doc: Doc) {
        self.doc = doc;
        self.tex = None;
        self.comp.clear();
        self.needs_full = true;
        self.fit_pending = true;
        self.stroke = None;
        self.playing = false;
        self.player.stop();
        self.tl_scroll = Vec2::ZERO;
        self.load_clip();
    }

    /// Decode the shot's sound files (the project keeps their paths, not the sounds).
    fn load_clip(&mut self) {
        self.peaks_key.clear();
        self.audio_paths = self.doc.sounds.iter().map(|t| t.path.clone()).collect();
        let mut audio = vec![];
        for t in &self.doc.sounds {
            match Clip::load(&t.path) {
                Ok(c) => audio.push(Some(c)),
                Err(e) => {
                    self.status = trf("Son introuvable ou illisible : {}", &[&e]);
                    audio.push(None);
                }
            }
        }
        self.audio = audio;
    }

    /// Decode the sounds again when the shot's list of sound files changed (another shot, or an
    /// AI command).
    fn sync_audio(&mut self) {
        if !self.doc.sounds.iter().map(|t| &t.path).eq(self.audio_paths.iter()) {
            self.load_clip();
        }
    }

    /// Pick a sound file: a new sound track, or the new file of track `replace`.
    fn import_sound(&mut self, replace: Option<usize>) {
        if let Some(p) = rfd::FileDialog::new().add_filter(tr("Son"), &["wav", "mp3"]).pick_file() {
            match Clip::load(&p) {
                Ok(c) => {
                    self.sync_audio();
                    let secs = c.duration();
                    match replace.filter(|i| *i < self.doc.sounds.len()) {
                        Some(i) => {
                            self.doc.sounds[i].path = p.clone();
                            self.audio[i] = Some(c);
                            self.audio_paths[i] = p.clone();
                        }
                        None => {
                            self.doc.sounds.push(SoundTrack::new(p.clone()));
                            self.audio.push(Some(c));
                            self.audio_paths.push(p.clone());
                        }
                    }
                    self.doc.dirty = true;
                    self.peaks_key.clear();
                    self.status = trf("Son importé : {} ({} s)", &[&p.display(), &format!("{secs:.1}")]);
                }
                Err(e) => self.status = trf("Son introuvable ou illisible : {}", &[&e]),
            }
        }
    }

    /// Play the shot's sound tracks from the current frame; with `secs`, only that long.
    fn play_sounds(&mut self, secs: Option<f32>) -> Result<(), String> {
        let pairs: Vec<(&Clip, &SoundTrack)> = self
            .audio
            .iter()
            .zip(&self.doc.sounds)
            .filter_map(|(c, t)| c.as_ref().map(|c| (c, t)))
            .collect();
        let start = self.doc.frame as f32 / self.doc.fps.max(1.0);
        self.player.play(&pairs, start, secs)
    }

    /// Audio scrubbing: a short piece of the sound when the frame changes by hand.
    fn scrub(&mut self) {
        if self.doc.frame == self.scrub_last {
            return;
        }
        self.scrub_last = self.doc.frame;
        let go = self.scrub_mode == 2 || (self.scrub_mode == 1 && self.tl_drag);
        if !go {
            return;
        }
        let _ = self.play_sounds(Some(self.scrub_secs));
    }

    /// The cells the timing buttons act on: the selection, else the current cell.
    fn cell_range(&self) -> (usize, usize, usize) {
        match self.cell_sel {
            Some((l, a, b)) if l < self.doc.layers.len() => (l, a.min(self.doc.frames - 1), b.min(self.doc.frames - 1)),
            _ => (self.doc.current_layer, self.doc.frame, self.doc.frame),
        }
    }

    fn move_cells(&mut self, right: bool) {
        let (l, a, b) = self.cell_range();
        if self.doc.move_cells(l, a, b, right) {
            let (a, b) = if right { (a + 1, b + 1) } else { (a - 1, b - 1) };
            if self.cell_sel.is_some() {
                self.cell_sel = Some((l, a, b));
            }
            self.doc.current_layer = l;
            self.doc.frame = if right {
                (self.doc.frame + 1).min(self.doc.frames - 1)
            } else {
                self.doc.frame.saturating_sub(1)
            };
            self.needs_full = true;
        }
    }

    /// Timing tools of the timeline: cells, bookmarks and play range, layer behaviours, drawing name.
    fn timing_bar(&mut self, ui: &mut egui::Ui) {
        if ui.button("◀").on_hover_text(tr("Décaler les cellules à gauche")).clicked() {
            self.move_cells(false);
        }
        if ui.button("▶").on_hover_text(tr("Décaler les cellules à droite")).clicked() {
            self.move_cells(true);
        }
        ui.menu_button(tr("Cellules"), |ui| {
            ui.label(tr("Maj + clic dans la timeline pour en choisir plusieurs."));
            ui.horizontal(|ui| {
                ui.label(tr("Répéter"));
                ui.add(egui::DragValue::new(&mut self.repeat_n).range(1..=99));
                ui.label(tr("fois :"));
                for (mode, label) in [
                    (crate::doc::RepeatMode::Loop, tr("en boucle")),
                    (crate::doc::RepeatMode::PingPong, tr("aller-retour")),
                    (crate::doc::RepeatMode::Random, tr("au hasard")),
                ] {
                    if ui.button(label).clicked() {
                        let (l, a, b) = self.cell_range();
                        if self.doc.repeat_cells(l, a, b, self.repeat_n, mode).is_some() {
                            self.needs_full = true;
                        }
                        ui.close_menu();
                    }
                }
            });
            if ui.button(tr("Copier les cellules")).clicked() {
                let (l, a, b) = self.cell_range();
                self.cell_clip = self.doc.layers[l].cels[a..=b].to_vec();
                if self.cell_clip.first() == Some(&None) {
                    // a run starting on a hold copies the drawing it holds
                    self.cell_clip[0] = self.doc.drawing_at(l, a);
                }
                self.cell_clip_names = self.doc.layers[l].names.clone();
                self.cell_clip_marks = self.doc.layers[l].marks.clone();
                self.status = trf("{} cellules copiées.", &[&self.cell_clip.len()]);
                ui.close_menu();
            }
            if ui
                .add_enabled(!self.cell_clip.is_empty(), egui::Button::new(tr("Coller les cellules")))
                .clicked()
            {
                let clip = self.cell_clip.clone();
                let names = self.cell_clip_names.clone();
                let marks = self.cell_clip_marks.clone();
                self.doc.paste_cells(&clip, &names, &marks);
                self.cell_sel = None;
                self.needs_full = true;
                ui.close_menu();
            }
            if ui.button(tr("Tout désélectionner")).clicked() {
                self.cell_sel = None;
                ui.close_menu();
            }
        });
        ui.menu_button(tr("Repères"), |ui| {
            let f = self.doc.frame;
            if ui.button(tr("Signet ici (ajouter / retirer)")).clicked() {
                self.doc.toggle_bookmark(f);
            }
            if ui.button(tr("Signet précédent")).clicked() {
                if let Some(b) = self.doc.prev_bookmark(f) {
                    self.doc.frame = b;
                }
            }
            if ui.button(tr("Signet suivant")).clicked() {
                if let Some(b) = self.doc.next_bookmark(f) {
                    self.doc.frame = b;
                }
            }
            if ui.button(tr("Retirer tous les signets")).clicked() {
                self.doc.bookmarks.clear();
            }
            ui.separator();
            if ui.button(tr("Début de lecture ici")).clicked() {
                self.doc.mark_in = Some(f);
            }
            if ui.button(tr("Fin de lecture ici")).clicked() {
                self.doc.mark_out = Some(f);
            }
            if ui.button(tr("Effacer début et fin de lecture")).clicked() {
                self.doc.mark_in = None;
                self.doc.mark_out = None;
            }
            ui.separator();
            if ui.button(tr("Note ici…")).clicked() {
                self.edit_note(f);
                ui.close_menu();
            }
            if ui.button(tr("Note précédente")).clicked() {
                if let Some(n) = self.doc.prev_note(f) {
                    self.doc.frame = n;
                }
            }
            if ui.button(tr("Note suivante")).clicked() {
                if let Some(n) = self.doc.next_note(f) {
                    self.doc.frame = n;
                }
            }
            ui.checkbox(&mut self.tl_notes, tr("Afficher les notes"));
        });
        let cl = self.doc.current_layer;
        ui.menu_button(tr("Avant / après"), |ui| {
            let l = &mut self.doc.layers[cl];
            ui.label(tr("Avant le premier dessin :"));
            for b in crate::doc::ALL_BEHAVIORS {
                if ui.selectable_value(&mut l.pre, b, b.label()).changed() {
                    self.needs_full = true;
                }
            }
            ui.separator();
            ui.label(tr("Après la fin du calque :"));
            for b in crate::doc::ALL_BEHAVIORS {
                if ui.selectable_value(&mut l.post, b, b.label()).changed() {
                    self.needs_full = true;
                }
            }
            ui.separator();
            let f = self.doc.frame;
            match l.end {
                Some(e) => {
                    ui.label(trf("Fin du calque : image {}", &[&(e + 1)]));
                }
                None => {
                    ui.label(tr("Le calque dure jusqu'au bout."));
                }
            }
            if ui.button(tr("Fin du calque ici")).clicked() {
                l.end = Some(f);
                self.needs_full = true;
            }
            if ui.button(tr("Jusqu'au bout")).clicked() {
                l.end = None;
                self.needs_full = true;
            }
        });
        let f = self.doc.frame;
        if self.doc.drawing_at(cl, f).is_some() {
            let mut name = self.doc.instance_name(cl, f).unwrap_or_default();
            let hint = self.doc.instance_number(cl, f).map(|n| n.to_string()).unwrap_or_default();
            if ui
                .add(egui::TextEdit::singleline(&mut name).hint_text(hint).desired_width(60.0))
                .on_hover_text(tr("Nom du dessin"))
                .changed()
            {
                self.doc.set_instance_name(cl, f, &name);
            }
        }
    }

    /// Open the note of a frame to write or change it.
    fn edit_note(&mut self, f: usize) {
        let text = self.doc.notes.get(&f).cloned().unwrap_or_default();
        self.note_edit = Some((f, text));
        self.tl_notes = true;
    }

    /// The small window where a timeline note is written.
    fn note_window(&mut self, ctx: &egui::Context) {
        let Some((f, mut text)) = self.note_edit.take() else { return };
        let mut open = true;
        let mut done = false;
        let mut remove = false;
        egui::Window::new(trf("Note de l'image {}", &[&(f + 1)]))
            .id(egui::Id::new("timeline-note"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let field = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(3).desired_width(320.0));
                if !field.has_focus() && !field.lost_focus() && ui.memory(|m| m.focused().is_none()) {
                    field.request_focus();
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("Valider")).clicked() {
                        done = true;
                    }
                    if ui.button(tr("Retirer la note")).clicked() {
                        remove = true;
                    }
                });
            });
        if remove {
            self.doc.set_note(f, "");
        } else if done {
            self.doc.set_note(f, &text);
        } else if open {
            self.note_edit = Some((f, text));
        }
    }

    /// Mark the drawing shown at a cell (0 removes the mark), with a word when it can't be done.
    fn set_mark(&mut self, li: usize, f: usize, mark: u8) {
        if mark > 0 {
            self.mark_pick = mark;
        }
        if !self.doc.set_mark(li, f, mark) {
            self.status = tr("Pas de dessin à marquer ici (ou calque verrouillé).").into();
        }
    }

    /// The colours to mark an image with, and the way to the next and previous marked images.
    fn mark_menu(&mut self, ui: &mut egui::Ui, li: usize, f: usize) {
        let now = self.doc.mark_at(li, f).unwrap_or(0);
        ui.horizontal(|ui| {
            for m in 1..=12u8 {
                let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                let resp = resp.on_hover_text(format!("{m}: {}", tr(crate::doc::GROUP_NAMES[m as usize])));
                ui.painter().rect_filled(r, 3.0, c32(crate::doc::GROUPS[m as usize]));
                if now == m {
                    ui.painter()
                        .rect_stroke(r, 3.0, Line::new(2.0_f32, Color32::WHITE), StrokeKind::Inside);
                }
                if resp.clicked() {
                    self.set_mark(li, f, m);
                    ui.close_menu();
                }
            }
        });
        if ui.add_enabled(now != 0, egui::Button::new(tr("Retirer la marque"))).clicked() {
            self.set_mark(li, f, 0);
            ui.close_menu();
        }
    }

    /// Go to the next (or previous) marked image of the current layer.
    fn step_mark(&mut self, forward: bool) {
        let (l, f) = (self.doc.current_layer, self.doc.frame);
        let to = if forward {
            self.doc.next_mark(l, f)
        } else {
            self.doc.prev_mark(l, f)
        };
        match to {
            Some(t) => self.doc.frame = t,
            None => self.status = tr("Pas d'autre image marquée de ce côté sur ce calque.").into(),
        }
    }

    /// Delete the current layer and the layers chosen with it.
    fn delete_layers(&mut self) {
        self.commit_floating();
        let targets = self.doc.target_layers();
        if self.doc.delete_layers(&targets) == 0 {
            self.status = tr("Le dernier calque ne peut pas être supprimé.").into();
        }
        self.needs_full = true;
    }

    /// Move the current layer (and the layers chosen with it) in time.
    fn shift_layers(&mut self, by: i64) {
        let targets = self.doc.target_layers();
        let moved = self.doc.shift_layers(&targets, by);
        if moved == 0 {
            self.status = tr("Impossible de décaler : calque verrouillé, vide, ou déjà au début.").into();
        } else {
            self.status = trf("Calque décalé de {} images.", &[&moved]);
            self.needs_full = true;
        }
    }

    fn slide_layer(&mut self, by: i64) {
        let l = self.doc.current_layer;
        if self.doc.slide_layer(l, by) {
            self.needs_full = true;
        } else {
            self.status = tr("Rien à faire glisser sur ce calque (ou calque verrouillé).").into();
        }
    }

    /// Split the current layer in two at a frame.
    fn split_layer(&mut self, at: usize) {
        self.commit_floating();
        let l = self.doc.current_layer;
        if self.doc.split_layer(l, at) {
            self.cell_sel = None;
            self.needs_full = true;
        } else {
            self.status = tr("Placez-vous après le premier dessin du calque, avant sa fin, pour le scinder.").into();
        }
    }

    /// Stretch the current layer and the layers chosen with it by the time ratio.
    fn stretch_by_ratio(&mut self) {
        self.commit_floating();
        let targets = self.doc.target_layers();
        let n = self.doc.stretch_layers_by(&targets, self.stretch_pct / 100.0);
        if n == 0 {
            self.status = tr("Rien à étirer sur ce calque.").into();
        } else {
            self.status = trf("{} calque(s) étiré(s) à {} %.", &[&n, &self.stretch_pct.round()]);
            self.cell_sel = None;
            self.needs_full = true;
        }
    }

    /// Give every drawing of the current layer the same exposure.
    fn retime_layer(&mut self) {
        self.commit_floating();
        let l = self.doc.current_layer;
        match self.doc.retime_layer(l, self.retime_n) {
            Some(last) => {
                self.status = trf("Le calque finit maintenant à l'image {}.", &[&(last + 1)]);
                self.cell_sel = None;
                self.needs_full = true;
            }
            None => self.status = tr("Rien à recaler sur ce calque (ou calque verrouillé).").into(),
        }
    }

    /// Make the current layer start at a frame.
    fn trim_layer_start(&mut self, at: usize) {
        self.commit_floating();
        let l = self.doc.current_layer;
        if self.doc.trim_layer_start(l, at) {
            self.cell_sel = None;
            self.needs_full = true;
        } else {
            self.status = tr("Placez-vous après le premier dessin du calque, avant sa fin, pour y mettre son début.").into();
        }
    }

    /// Fold or unfold layers in the timeline: the current layer and the layers chosen with it
    /// (`all` false, they follow the current layer), or every layer.
    fn collapse_layers(&mut self, all: bool, on: Option<bool>) {
        let on = on.unwrap_or(!self.doc.layers[self.doc.current_layer].collapsed);
        let targets = if all {
            (0..self.doc.layers.len()).collect()
        } else {
            self.doc.target_layers()
        };
        for l in targets {
            self.doc.layers[l].collapsed = on;
        }
        self.doc.dirty = true;
    }

    /// Put an undo mark on the state the project is in, or remove it.
    fn toggle_undo_mark(&mut self) {
        let step = self.doc.history_step();
        if step == 0 {
            self.status = tr("Rien à marquer : l'historique est vide.").into();
        } else if self.doc.toggle_undo_mark(step) {
            self.status = tr("Marque d'annulation posée.").into();
        } else {
            self.status = tr("Marque d'annulation retirée.").into();
        }
    }

    /// Go back to the previous undo mark, or forward to the next one.
    fn go_undo_mark(&mut self, forward: bool) {
        if self.stroke.is_some() {
            return;
        }
        match self.doc.undo_mark_from_here(forward) {
            Some(step) => {
                self.cancel_floating();
                self.doc.go_to_step(step);
                self.needs_full = true;
                self.status = trf("Projet remis à la marque de l'étape {}.", &[&step]);
            }
            None if forward => self.status = tr("Pas de marque d'annulation après cet état.").into(),
            None => self.status = tr("Pas de marque d'annulation avant cet état.").into(),
        }
    }

    /// Put the chosen layers end to end in one layer.
    fn join_selected(&mut self) {
        self.commit_floating();
        let targets = self.doc.target_layers();
        if !self.doc.join_layers(&targets) {
            self.status = tr("Choisissez au moins deux calques non verrouillés qui ont des dessins.").into();
        }
        self.cell_sel = None;
        self.needs_full = true;
    }

    fn merge_selected(&mut self) {
        self.commit_floating();
        let targets = self.doc.target_layers();
        if !self.doc.merge_layers(&targets, crate::doc::MergeOptions::default()) {
            self.status = tr("Choisissez au moins deux calques non verrouillés (Ctrl + clic sur leur nom).").into();
        }
        self.needs_full = true;
    }

    fn insert_tweens(&mut self) {
        let (l, f) = (self.doc.current_layer, self.doc.frame);
        match self.doc.insert_tweens(l, f, self.tween_n, self.tween_mix) {
            Some(at) => {
                self.doc.frame = at;
                self.cell_sel = None;
                self.needs_full = true;
                self.status = trf("{} intervalles insérés.", &[&self.tween_n]);
            }
            None => self.status = tr("Il faut un dessin après celui-ci sur le calque (non verrouillé).").into(),
        }
    }

    /// Zoom the timeline in or out by a factor.
    fn tl_zoom(&mut self, k: f32) {
        self.tl_cell = (self.tl_cell * k).clamp(TL_CELL_MIN, TL_CELL_MAX);
    }

    /// A frame as the timeline's ruler writes it: its number, or seconds:frames.
    fn frame_label(&self, f: usize) -> String {
        if self.tl_timecode {
            crate::doc::timecode(f, self.doc.fps)
        } else {
            format!("{}", f + 1)
        }
    }

    fn start_sound(&mut self) {
        if let Err(e) = self.play_sounds(None) {
            self.status = trf("Lecture du son impossible : {}", &[&e]);
        }
    }

    /// Loudness of each sound track at each frame, for the timeline.
    fn update_peaks(&mut self) {
        self.sync_audio();
        let key = format!("{:?}{}{}", self.doc.sounds, self.doc.fps, self.doc.frames);
        if key != self.peaks_key {
            let (fps, frames) = (self.doc.fps.max(1.0), self.doc.frames);
            self.peaks = self
                .audio
                .iter()
                .zip(&self.doc.sounds)
                .map(|(c, t)| c.as_ref().map(|c| c.peaks_per_frame(t, fps, frames)).unwrap_or_default())
                .collect();
            self.peaks_key = key;
        }
    }

    fn xsheet_window(&mut self, ctx: &egui::Context) {
        if !self.show_xsheet {
            return;
        }
        let mut open = true;
        let mut go: Option<(usize, usize)> = None;
        let mut edit_start: Option<(usize, usize, String)> = None;
        let mut edit_done: Option<(usize, usize, String)> = None;
        let mut edit_cancel = false;
        egui::Window::new(tr("Feuille d'exposition"))
            .open(&mut open)
            .default_size([520.0, 520.0])
            .show(ctx, |ui| {
                ui.small(tr(
                    "Une ligne par image, une colonne par calque : le numéro du dessin quand il commence, un trait tant qu'il est tenu.",
                ));
                let nl = self.doc.layers.len();
                let has_sound = self.peaks.iter().any(|p| !p.is_empty());
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("xsheet").striped(true).min_col_width(56.0).show(ui, |ui| {
                        ui.strong(tr("Image"));
                        if has_sound {
                            ui.strong(tr("Son"));
                        }
                        for li in (0..nl).rev() {
                            ui.strong(&self.doc.layers[li].name);
                        }
                        ui.end_row();
                        let numbers: Vec<std::collections::HashMap<usize, usize>> =
                            (0..nl).map(|li| self.doc.drawing_numbers(li)).collect();
                        for f in 0..self.doc.frames {
                            let cur_f = f == self.doc.frame;
                            let lab = egui::RichText::new(format!("{}", f + 1));
                            ui.label(if cur_f {
                                lab.strong().color(Color32::from_rgb(230, 70, 60))
                            } else {
                                lab
                            });
                            if has_sound {
                                // the loudest of the tracks at this frame
                                let v = self.peaks.iter().filter_map(|p| p.get(f)).fold(0.0f32, |m, v| m.max(*v));
                                let (r, _) = ui.allocate_exact_size(vec2(50.0, 14.0), Sense::hover());
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(r.min, vec2(50.0 * v, 14.0)),
                                    1.0,
                                    Color32::from_rgb(90, 160, 220),
                                );
                            }
                            for li in (0..nl).rev() {
                                if let Some((el, ef, text)) = &mut self.xs_edit {
                                    if *el == li && *ef == f {
                                        let r = ui.add(egui::TextEdit::singleline(text).desired_width(44.0));
                                        r.request_focus();
                                        if ui.input(|i| i.key_pressed(Key::Enter)) {
                                            edit_done = Some((li, f, text.clone()));
                                        } else if ui.input(|i| i.key_pressed(Key::Escape)) || r.lost_focus() {
                                            edit_cancel = true;
                                        }
                                        continue;
                                    }
                                }
                                let txt = match self.doc.layers[li].cels.get(f).copied().flatten() {
                                    Some(id) => numbers[li].get(&id).map(|n| format!("{n}")).unwrap_or_default(),
                                    None if self.doc.key_index(li, f).is_some() => "|".into(),
                                    None => String::new(),
                                };
                                let sel = cur_f && li == self.doc.current_layer;
                                let r = ui
                                    .add(egui::SelectableLabel::new(sel, txt.clone()))
                                    .on_hover_text(tr("Double-clic : taper le numéro du dessin (« - » pour tenir)"));
                                if r.double_clicked() {
                                    edit_start = Some((li, f, if txt == "|" { String::new() } else { txt }));
                                } else if r.clicked() {
                                    go = Some((li, f));
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
            });
        if let Some((l, f)) = go {
            self.doc.current_layer = l;
            self.doc.frame = f;
        }
        if let Some(e) = edit_start {
            self.doc.current_layer = e.0;
            self.doc.frame = e.1;
            self.xs_edit = Some(e);
        }
        if let Some((l, f, text)) = edit_done {
            if !self.doc.set_cell_number(l, f, &text) {
                self.status = tr("Tapez un numéro de dessin, ou « - » pour tenir le dessin d'avant.").into();
            }
            self.xs_edit = None;
            self.needs_full = true;
        } else if edit_cancel {
            self.xs_edit = None;
        }
        self.show_xsheet = open;
    }

    fn open_path(&mut self, p: PathBuf) {
        match io::open_project(&p) {
            Ok(mut list) => {
                let first = list.remove(0);
                let mut clips = vec![ClipSlot {
                    name: first.name,
                    notes: first.notes,
                    doc: None,
                    thumb: None,
                }];
                for c in list {
                    clips.push(ClipSlot {
                        name: c.name,
                        notes: c.notes,
                        doc: Some(c.doc),
                        thumb: None,
                    });
                }
                for (i, c) in clips.iter_mut().enumerate() {
                    if c.name.is_empty() {
                        c.name = format!("{} {}", tr("Plan"), i + 1);
                    }
                }
                self.clips = clips;
                self.active_clip = 0;
                self.replace_doc(first.doc);
                self.status = trf("Projet ouvert : {}", &[&p.display()]);
                self.file_stamp = modified(&p);
                self.path = Some(p);
            }
            Err(e) => self.status = trf("Impossible d'ouvrir le projet : {}", &[&e]),
        }
    }

    /// Reload the open project when another program (an AI through MCP, for example) saved it,
    /// unless there are unsaved changes here.
    fn watch_file(&mut self, ctx: &egui::Context) {
        let Some(p) = self.path.clone() else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(1000));
        if self.last_watch.elapsed().as_millis() < 900 || self.stroke.is_some() {
            return;
        }
        self.last_watch = Instant::now();
        let now = modified(&p);
        if now.is_some() && now != self.file_stamp {
            self.file_stamp = now;
            if self.doc.dirty {
                self.status =
                    tr("Le fichier a été modifié par un autre programme ; vos changements non enregistrés sont gardés ici.").into();
            } else if let Ok(mut list) = io::open_project(&p) {
                let (frame, layer) = (self.doc.frame, self.doc.current_layer);
                let active = self.active_clip.min(list.len() - 1);
                let mut clips = vec![];
                let mut current = None;
                for (i, c) in list.drain(..).enumerate() {
                    let name = if c.name.is_empty() {
                        format!("{} {}", tr("Plan"), i + 1)
                    } else {
                        c.name
                    };
                    if i == active {
                        current = Some(c.doc);
                        clips.push(ClipSlot {
                            name,
                            notes: c.notes,
                            doc: None,
                            thumb: None,
                        });
                    } else {
                        clips.push(ClipSlot {
                            name,
                            notes: c.notes,
                            doc: Some(c.doc),
                            thumb: None,
                        });
                    }
                }
                self.clips = clips;
                self.active_clip = active;
                self.doc = current.expect("active clip");
                self.doc.frame = frame.min(self.doc.frames - 1);
                self.doc.current_layer = layer.min(self.doc.layers.len() - 1);
                self.comp.clear();
                self.needs_full = true;
                self.status = tr("Projet rechargé : il a été modifié par un autre programme.").into();
            }
        }
    }

    fn ai_window(&mut self, ctx: &egui::Context) {
        if !self.show_ai {
            return;
        }
        let exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "toonlight".into());
        let config = serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": {"toonlight": {"command": exe, "args": ["--mcp"]}}
        }))
        .unwrap_or_default();
        let claude_code = format!("claude mcp add toonlight -- \"{exe}\" --mcp");
        let mut open = self.show_ai;
        let mut copied = false;
        egui::Window::new(tr("Connecter une IA (MCP)")).open(&mut open).collapsible(false).default_width(560.0).show(ctx, |ui| {
            ui.label(tr("Toonlight peut être piloté par un assistant IA compatible MCP (Claude, Cursor, ChatGPT et d'autres) : créer des calques et des images, dessiner, remplir, exporter, et regarder le résultat."));
            ui.label(tr("Tant que cette fenêtre est ouverte, l'IA travaille en direct sur le projet ouvert ici (chaque changement s'annule avec Annuler). Fenêtre fermée, elle travaille sur son propre projet."));
            ui.add_space(6.0);
            ui.strong(tr("Claude Desktop, Cursor et la plupart des applications : ajoutez ceci à leur fichier de configuration MCP"));
            ui.add(egui::TextEdit::multiline(&mut config.as_str()).code_editor().desired_rows(7).desired_width(f32::INFINITY));
            if ui.button(tr("Copier la configuration")).clicked() {
                ui.ctx().copy_text(config.clone());
                copied = true;
            }
            ui.add_space(6.0);
            ui.strong(tr("Claude Code : tapez cette commande dans un terminal"));
            ui.add(egui::TextEdit::singleline(&mut claude_code.as_str()).code_editor().desired_width(f32::INFINITY));
            if ui.button(tr("Copier la commande")).clicked() {
                ui.ctx().copy_text(claude_code.clone());
                copied = true;
            }
            ui.add_space(6.0);
            ui.label(tr("Pour voir le travail de l'IA en direct : enregistrez votre projet ici, puis demandez-lui d'ouvrir et d'enregistrer ce même fichier. La fenêtre le recharge dès qu'il change (si vous n'avez pas de changements non enregistrés)."));
            if let Some(p) = &self.path {
                ui.horizontal(|ui| {
                    ui.label(tr("Fichier ouvert :"));
                    ui.monospace(p.display().to_string());
                });
            }
        });
        if copied {
            self.status = tr("Copié.").into();
        }
        self.show_ai = open;
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter(tr("Projet d'animation"), &["toonlight", "animcraft", "atanim"])
            .pick_file()
        {
            self.open_path(p);
        }
    }

    fn save(&mut self, ask: bool) {
        // like the reference: "Save" goes straight to the file only when it is an existing project
        // file; otherwise it asks where to save
        let in_place = !ask && self.path.as_deref().is_some_and(io::saves_in_place);
        let p = if !in_place {
            // offer a name that is still free, after the open project's
            let suggest = io::free_name(&self.path.clone().unwrap_or_else(|| std::path::PathBuf::from("animation")));
            let mut dlg = rfd::FileDialog::new().add_filter(tr("Projet d'animation"), &["toonlight"]);
            if let Some(d) = self.path.as_ref().and_then(|p| p.parent()) {
                dlg = dlg.set_directory(d);
            }
            dlg.set_file_name(suggest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
                .save_file()
        } else {
            self.path.clone()
        };
        if let Some(mut p) = p {
            if p.extension().is_none() {
                p.set_extension("toonlight");
            }
            // a backup copy of the version being replaced, like the reference's AutoBackup
            if self.prefs.auto_backup && p.is_file() {
                if let Err(e) = crate::prefs::back_up(&p, self.prefs.backup_folder.as_deref()) {
                    self.status = trf("Copie de sauvegarde impossible : {}", &[&e]);
                }
            }
            let list = self.clip_list();
            match io::save_project(&list, &p) {
                Ok(()) => {
                    self.doc.dirty = false;
                    self.remove_recovery();
                    self.status = trf("Enregistré : {}", &[&p.display()]);
                    self.file_stamp = modified(&p);
                    self.path = Some(p);
                }
                Err(e) => self.status = trf("Échec de l'enregistrement : {}", &[&e]),
            }
        }
    }

    /// Every clip of the project, as the save writes them.
    fn clip_list(&self) -> Vec<(&Doc, &str, &str)> {
        self.clips
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let d = if i == self.active_clip {
                    &self.doc
                } else {
                    c.doc.as_ref().expect("inactive clip")
                };
                // a single clip with its default name is saved as a plain animation
                let name = if self.clips.len() == 1 && c.notes.is_empty() {
                    ""
                } else {
                    c.name.as_str()
                };
                (d, name, c.notes.as_str())
            })
            .collect()
    }

    /// Run the AI commands that came through the bridge on the open project (they can be undone).
    fn bridge_tick(&mut self) {
        let Some(rx) = &self.bridge else { return };
        let requests: Vec<(String, std::sync::mpsc::Sender<String>)> = rx.try_iter().collect();
        if requests.is_empty() {
            return;
        }
        let lines: Vec<String> = requests.iter().map(|r| r.0.clone()).collect();
        let answers = self.run_commands(&lines);
        for ((_, reply), answer) in requests.into_iter().zip(answers) {
            let _ = reply.send(answer);
        }
        self.status = tr("Une IA a modifié le projet (Annuler pour revenir en arrière).").into();
    }

    /// Run AI commands (one JSON-RPC message per line, as `toonlight --mcp` reads them) on the
    /// open project, and give back their answers.
    fn run_commands(&mut self, lines: &[String]) -> Vec<String> {
        self.commit_floating();
        let mut srv = self
            .bridge_srv
            .take()
            .unwrap_or_else(|| crate::mcp::Server::new(Doc::new(1, 1, 1.0)));
        srv.doc = std::mem::replace(&mut self.doc, Doc::new(1, 1, 1.0));
        srv.path = self.path.clone();
        srv.clips = self
            .clips
            .iter_mut()
            .map(|c| (c.name.clone(), c.notes.clone(), c.doc.take()))
            .collect();
        srv.active = self.active_clip;
        let answers = lines.iter().map(|line| srv.handle_line(line).unwrap_or_default()).collect();
        self.doc = std::mem::replace(&mut srv.doc, Doc::new(1, 1, 1.0));
        self.path = srv.path.clone();
        self.clips = srv
            .clips
            .drain(..)
            .map(|(name, notes, doc)| ClipSlot {
                name,
                notes,
                doc,
                thumb: None,
            })
            .collect();
        self.active_clip = srv.active.min(self.clips.len().saturating_sub(1));
        if let Some(p) = &self.path {
            self.file_stamp = modified(p);
        }
        self.bridge_srv = Some(srv);
        self.needs_full = true;
        answers
    }

    /// While there are unsaved changes, write a recovery copy every few minutes.
    fn recovery_tick(&mut self) {
        let every = self.prefs.recovery_minutes as u64 * 60;
        if every == 0 || !self.doc.dirty || self.last_recovery.elapsed().as_secs() < every || self.stroke.is_some() {
            return;
        }
        self.last_recovery = Instant::now();
        if let Some(p) = crate::prefs::recovery_path() {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let list = self.clip_list();
            let _ = io::save_project(&list, &p);
        }
    }

    fn remove_recovery(&mut self) {
        if let Some(p) = crate::prefs::recovery_path() {
            let _ = std::fs::remove_file(p);
        }
    }

    /// Offer back the work found in the recovery copy at start.
    fn recovery_window(&mut self, ctx: &egui::Context) {
        let Some(p) = self.recovery_offer.clone() else { return };
        let mut answer = None;
        egui::Window::new(tr("Travail non enregistré retrouvé"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(tr(
                    "Toonlight s'est fermé sans enregistrer. Une copie de votre travail a été gardée.",
                ));
                ui.horizontal(|ui| {
                    if ui.button(tr("Rouvrir la copie")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(tr("Ignorer")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        if let Some(yes) = answer {
            self.recovery_offer = None;
            if yes {
                self.open_path(p);
                // it isn't saved anywhere yet: Save asks where
                self.path = None;
                self.doc.dirty = true;
            } else {
                self.remove_recovery();
            }
        }
    }

    /// Effects > Layer effects: a stack of effects on the current layer, shown live, animated with
    /// keys, applied for good when wanted.
    fn fx_stack_window(&mut self, ctx: &egui::Context) {
        if !self.show_fx_stack {
            return;
        }
        let mut open = true;
        let l = self.doc.current_layer;
        let f = self.doc.frame;
        let color_a = self.color;
        let before = self.doc.layers[l].fx.clone();
        let mut apply = false;
        let title = trf("Effets du calque « {} »", &[&self.doc.layers[l].name]);
        egui::Window::new(title)
            .id(egui::Id::new("fx-stack"))
            .open(&mut open)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.small(tr(
                    "Les effets s'appliquent dans l'ordre à chaque image du calque, sans changer les dessins.",
                ));
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("fx-add")
                        .selected_text(ALL_EFFECTS[self.fx_pick].label())
                        .show_ui(ui, |ui| {
                            for (i, fx) in ALL_EFFECTS.iter().enumerate() {
                                ui.selectable_value(&mut self.fx_pick, i, fx.label());
                            }
                        });
                    if ui.button(tr("+ Ajouter")).clicked() {
                        self.doc.layers[l].fx.push(crate::fx::FxEntry::new(ALL_EFFECTS[self.fx_pick]));
                    }
                    // saved stacks: taking one replaces the layer's stack (one undo step)
                    let mut take: Option<Vec<crate::fx::FxEntry>> = None;
                    ui.menu_button(tr("Préréglages"), |ui| {
                        let mut drop = None;
                        for (i, p) in self.fx_presets.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui
                                    .button(&p.name)
                                    .on_hover_text(trf("{} effets : remplace la pile du calque", &[&p.effects.len()]))
                                    .clicked()
                                {
                                    take = Some(p.effects.clone());
                                    ui.close_menu();
                                }
                                if ui.small_button("✕").on_hover_text(tr("Supprimer ce préréglage")).clicked() {
                                    drop = Some(i);
                                }
                            });
                        }
                        if let Some(i) = drop {
                            self.fx_presets.remove(i);
                            crate::fx::save_presets(&self.fx_presets);
                        }
                        if !self.fx_presets.is_empty() {
                            ui.separator();
                        }
                        let stack = self.doc.layers[l].fx.clone();
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.fx_preset_name)
                                    .hint_text(tr("Nom"))
                                    .desired_width(120.0),
                            );
                            let name = self.fx_preset_name.trim().to_string();
                            if ui
                                .add_enabled(!stack.is_empty() && !name.is_empty(), egui::Button::new(tr("Enregistrer la pile")))
                                .on_hover_text(tr("Garde les effets de ce calque, avec leurs clés, sous ce nom"))
                                .clicked()
                            {
                                // a preset of the same name is replaced
                                self.fx_presets.retain(|p| p.name != name);
                                self.fx_presets.push(crate::fx::FxPreset {
                                    name,
                                    effects: stack.clone(),
                                });
                                crate::fx::save_presets(&self.fx_presets);
                                self.fx_preset_name.clear();
                                ui.close_menu();
                            }
                        });
                        ui.separator();
                        if ui
                            .add_enabled(!stack.is_empty(), egui::Button::new(tr("Enregistrer dans un fichier…")))
                            .clicked()
                        {
                            ui.close_menu();
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter(tr("Pile d'effets"), &["json"])
                                .set_file_name("effects.json")
                                .save_file()
                            {
                                let name = p.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                if let Err(e) =
                                    crate::fx::stack_to_json(&name, &stack).and_then(|t| std::fs::write(&p, t).map_err(|e| e.to_string()))
                                {
                                    self.status = trf("Échec de l'enregistrement : {}", &[&e]);
                                }
                            }
                        }
                        if ui.button(tr("Charger un fichier…")).clicked() {
                            ui.close_menu();
                            if let Some(p) = rfd::FileDialog::new().add_filter(tr("Pile d'effets"), &["json"]).pick_file() {
                                match std::fs::read_to_string(&p)
                                    .map_err(|e| e.to_string())
                                    .and_then(|t| crate::fx::stack_from_json(&t))
                                {
                                    Ok(preset) => take = Some(preset.effects),
                                    Err(e) => self.status = trf("Échec de l'ouverture : {}", &[&e]),
                                }
                            }
                        }
                    });
                    if let Some(effects) = take {
                        self.doc.push_structure_undo();
                        self.doc.layers[l].fx = effects;
                    }
                });
                ui.separator();
                let mut action: Option<(usize, i32)> = None;
                for (i, e) in self.doc.layers[l].fx.iter_mut().enumerate() {
                    egui::CollapsingHeader::new(format!("{}. {}", i + 1, e.effect.label()))
                        .id_salt(("fx-e", l, i))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut e.on, tr("Actif"));
                                if ui.small_button("▲").clicked() {
                                    action = Some((i, -1));
                                }
                                if ui.small_button("▼").clicked() {
                                    action = Some((i, 1));
                                }
                                if ui.small_button("✕").clicked() {
                                    action = Some((i, 0));
                                }
                            });
                            let mut cur = e.at(f);
                            if fx_params_ui(ui, &mut cur, color_a) {
                                e.set_at(f, cur);
                            }
                            ui.horizontal_wrapped(|ui| {
                                let here = e.keys.iter().any(|k| k.0 == f);
                                if !here && ui.small_button(tr("Clé à cette image")).clicked() {
                                    if e.keys.is_empty() {
                                        // the plain settings become the first key, at the first frame
                                        e.keys.push((0, e.effect));
                                    }
                                    e.keys.push((f, cur));
                                    e.keys.sort_by_key(|k| k.0);
                                    e.keys.dedup_by_key(|k| k.0);
                                }
                                if here && ui.small_button(tr("Supprimer la clé")).clicked() {
                                    e.keys.retain(|k| k.0 != f);
                                }
                                if !e.keys.is_empty() {
                                    let list: Vec<String> = e.keys.iter().map(|k| (k.0 + 1).to_string()).collect();
                                    ui.small(trf("Clés aux images : {}", &[&list.join(", ")]));
                                }
                            });
                        });
                }
                if let Some((i, dir)) = action {
                    let fx = &mut self.doc.layers[l].fx;
                    match dir {
                        0 => {
                            fx.remove(i);
                        }
                        -1 if i > 0 => fx.swap(i, i - 1),
                        1 if i + 1 < fx.len() => fx.swap(i, i + 1),
                        _ => {}
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("Appliquer aux dessins"))
                        .on_hover_text(tr("Peint les effets dans les dessins pour de bon et vide la pile"))
                        .clicked()
                    {
                        apply = true;
                    }
                    if ui.button(tr("Tout supprimer")).clicked() {
                        self.doc.layers[l].fx.clear();
                    }
                });
            });
        if apply && !self.doc.apply_fx_stack(l) {
            self.status = tr("Aucun effet actif à appliquer, ou le calque est verrouillé.").into();
        }
        if self.doc.layers.get(l).map(|x| &x.fx) != Some(&before) {
            self.doc.dirty = true;
            self.needs_full = true;
        }
        if apply {
            self.needs_full = true;
        }
        if !open {
            self.show_fx_stack = false;
        }
    }

    /// Edit > Undo history: every step; clicking one goes back (or forward) to it.
    fn history_window(&mut self, ctx: &egui::Context) {
        if !self.show_history {
            return;
        }
        let mut open = true;
        let (undo, redo) = self.doc.history();
        let (marks, redo_marks) = self.doc.history_marks();
        let mut target: Option<usize> = None;
        let mut mark: Option<usize> = None;
        let mut jump: Option<bool> = None;
        egui::Window::new(tr("Historique d'annulation"))
            .open(&mut open)
            .default_size([280.0, 380.0])
            .show(ctx, |ui| {
                ui.small(tr("Cliquez une étape pour revenir à cet état."));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!undo.is_empty(), egui::Button::new(tr("Marquer cet état")))
                        .on_hover_text(tr(
                            "Pose une marque sur l'état actuel (ou la retire) : un endroit où revenir d'un seul coup, par exemple avant un essai.",
                        ))
                        .clicked()
                    {
                        mark = Some(undo.len());
                    }
                    if ui.button("◀").on_hover_text(tr("Revenir à la marque d'annulation précédente")).clicked() {
                        jump = Some(false);
                    }
                    if ui.button("▶").on_hover_text(tr("Aller à la marque d'annulation suivante")).clicked() {
                        jump = Some(true);
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if ui.selectable_label(undo.is_empty(), tr("Début")).clicked() {
                        target = Some(0);
                    }
                    let steps = undo.iter().zip(&marks).map(|s| (s, true)).chain(redo.iter().zip(&redo_marks).map(|s| (s, false)));
                    for (i, ((drawing, marked), done)) in steps.enumerate() {
                        let step = i + 1;
                        ui.horizontal(|ui| {
                            // the mark: a flag, clicked to put it or remove it
                            let (r, flag) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::click());
                            let flag = flag.on_hover_text(tr("Marque d'annulation (poser / retirer)"));
                            let col = if *marked {
                                Color32::from_rgb(240, 170, 40)
                            } else if flag.hovered() {
                                ui.visuals().weak_text_color()
                            } else {
                                ui.visuals().weak_text_color().gamma_multiply(0.35)
                            };
                            let p = ui.painter();
                            let x = r.left() + 4.0;
                            p.line_segment([pos2(x, r.top() + 3.0), pos2(x, r.bottom() - 2.0)], Line::new(1.5_f32, col));
                            p.add(egui::Shape::convex_polygon(
                                vec![pos2(x, r.top() + 3.0), pos2(x + 9.0, r.top() + 6.5), pos2(x, r.top() + 10.0)],
                                if *marked { col } else { Color32::TRANSPARENT },
                                Line::new(1.0_f32, col),
                            ));
                            if flag.clicked() {
                                mark = Some(step);
                            }
                            let label = format!("{}. {}", step, if *drawing { tr("Dessin") } else { tr("Calques et images") });
                            let text = if done { egui::RichText::new(label) } else { egui::RichText::new(label).weak() };
                            if ui.add(egui::SelectableLabel::new(done && step == undo.len(), text)).clicked() {
                                target = Some(step);
                            }
                        });
                    }
                });
            });
        if let Some(step) = mark {
            self.doc.toggle_undo_mark(step);
        }
        if let Some(forward) = jump {
            self.go_undo_mark(forward);
        }
        if let Some(t) = target {
            self.commit_floating();
            self.doc.go_to_step(t);
            self.needs_full = true;
        }
        if !open {
            self.show_history = false;
        }
    }

    /// Edit > Preferences…
    fn prefs_window(&mut self, ctx: &egui::Context) {
        if !self.show_prefs {
            return;
        }
        let mut open = true;
        let before = self.prefs.clone();
        egui::Window::new(tr("Préférences"))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                let p = &mut self.prefs;
                ui.strong(tr("Enregistrement"));
                ui.checkbox(
                    &mut p.auto_backup,
                    tr("Garder une copie de la version précédente à chaque enregistrement"),
                );
                ui.horizontal(|ui| {
                    ui.label(tr("Dossier des copies :"));
                    let shown = p
                        .backup_folder
                        .as_ref()
                        .map(|f| f.display().to_string())
                        .unwrap_or_else(|| tr("à côté du projet").into());
                    ui.monospace(shown);
                    if ui.small_button(tr("Choisir…")).clicked() {
                        if let Some(f) = rfd::FileDialog::new().pick_folder() {
                            p.backup_folder = Some(f);
                        }
                    }
                    if p.backup_folder.is_some() && ui.small_button(tr("À côté du projet")).clicked() {
                        p.backup_folder = None;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Copie de secours toutes les"));
                    ui.add(egui::DragValue::new(&mut p.recovery_minutes).range(0..=60));
                    ui.label(tr("minutes (0 = jamais)"));
                });
                ui.separator();
                ui.strong(tr("Mémoire"));
                ui.horizontal(|ui| {
                    ui.label(tr("Annulation :"));
                    ui.add(egui::DragValue::new(&mut p.undo_mb).range(16..=8192).suffix(" Mo"));
                });
                ui.separator();
                ui.strong(tr("Animation"));
                ui.checkbox(&mut p.auto_break, tr("Peindre sur une tenue crée un dessin"));
                ui.checkbox(&mut p.auto_stretch, tr("Allonger le calque tout seul"));
                ui.separator();
                ui.strong(tr("Son"));
                ui.horizontal(|ui| {
                    ui.label(tr("Son en glissant"));
                    ui.radio_value(&mut p.scrub_mode, 0, tr("Jamais"));
                    ui.radio_value(&mut p.scrub_mode, 1, tr("En glissant"));
                    ui.radio_value(&mut p.scrub_mode, 2, tr("Toujours"));
                    ui.add(egui::DragValue::new(&mut p.scrub_secs).range(0.02..=2.0).speed(0.01).suffix(" s"));
                });
                ui.separator();
                if ui.button(tr("Valeurs par défaut")).clicked() {
                    *p = crate::prefs::Prefs::default();
                }
            });
        if self.prefs != before {
            self.prefs.apply_globals();
            self.doc.auto_break = self.prefs.auto_break;
            self.doc.auto_stretch = self.prefs.auto_stretch;
            self.scrub_mode = self.prefs.scrub_mode;
            self.scrub_secs = self.prefs.scrub_secs;
            crate::prefs::save(&self.prefs);
        }
        if !open {
            self.show_prefs = false;
        }
    }

    fn export(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            match io::export_frames(&self.doc, &dir) {
                Ok(n) => self.status = trf("{} images exportées dans {}", &[&n, &dir.display()]),
                Err(e) => self.status = trf("Échec de l'export : {}", &[&e]),
            }
        }
    }

    fn project_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "animation".into())
    }

    fn export_ae(&mut self) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title(tr("Dossier d'export (JSON + images)"))
            .pick_folder()
        {
            match io::export_after_effects(&self.doc, &dir, &self.project_name()) {
                Ok(p) => self.status = trf("Structure exportée pour After Effects : {}", &[&p.display()]),
                Err(e) => self.status = trf("Échec de l'export : {}", &[&e]),
            }
        }
    }

    fn export_gif(&mut self) {
        if let Some(mut p) = rfd::FileDialog::new()
            .add_filter(tr("GIF animé"), &["gif"])
            .set_file_name(format!("{}.gif", self.project_name()))
            .save_file()
        {
            if p.extension().is_none() {
                p.set_extension("gif");
            }
            match io::export_gif(&self.doc, &p, 960) {
                Ok(()) => self.status = trf("GIF exporté : {}", &[&p.display()]),
                Err(e) => self.status = trf("Échec de l'export : {}", &[&e]),
            }
        }
    }

    fn export_video(&mut self) {
        let Some(ff) = io::find_ffmpeg() else {
            self.status = tr("L'export vidéo a besoin de FFmpeg (gratuit, ffmpeg.org) installé sur l'ordinateur.").into();
            return;
        };
        if let Some(mut p) = rfd::FileDialog::new()
            .add_filter(tr("Vidéo MP4"), &["mp4"])
            .set_file_name(format!("{}.mp4", self.project_name()))
            .save_file()
        {
            if p.extension().is_none() {
                p.set_extension("mp4");
            }
            match io::export_video(&self.doc, &p, &ff) {
                Ok(()) => self.status = trf("Vidéo exportée : {}", &[&p.display()]),
                Err(e) => self.status = trf("Échec de l'export vidéo : {}", &[&e]),
            }
        }
    }

    fn clear_drawing(&mut self) {
        let l = self.doc.current_layer;
        if self.doc.layers[l].locked {
            return;
        }
        if let Some(id) = self.doc.drawing_at(l, self.doc.frame) {
            let before = self.doc.drawings[id].px.clone();
            self.doc.drawings[id].px.fill(0);
            self.doc.push_pixel_undo(id, Rect::full(self.doc.w, self.doc.h), &before);
            self.needs_full = true;
        }
    }

    fn step_frame(&mut self, d: i32) {
        let n = self.doc.frames as i32;
        let f = self.doc.frame as i32 + d;
        self.doc.frame = if self.looping { f.rem_euclid(n) } else { f.clamp(0, n - 1) } as usize;
    }

    // ---------- canvas picture ----------
    fn refresh(&mut self, ctx: &egui::Context) {
        let (w, h) = (self.doc.w, self.doc.h);
        let onion = !self.playing && !self.viewing_camera();
        if self.comp.len() != w * h * 4 {
            self.comp = vec![0; w * h * 4];
            self.needs_full = true;
        }
        if self.needs_full {
            // the picture itself changed (not only the frame shown)
            self.full_stamp += 1;
        }
        let key = (self.doc.frame, onion);
        if self.shown != key {
            self.needs_full = true;
        }
        if self.needs_full || self.tex.is_none() {
            self.doc.composite(self.doc.frame, onion, Rect::full(w, h), &mut self.comp);
            let img = if self.viewing_camera() {
                let c = &self.doc.camera;
                // on screen, motion blur mixes fewer camera positions than an export, to stay quick
                let most = if self.playing { 8 } else { 16 };
                let shot = c.shoot_with(self.doc.frame, &self.comp, w, h, self.doc.paper, most);
                egui::ColorImage::from_rgba_unmultiplied([c.width.max(1), c.height.max(1)], &shot)
            } else {
                egui::ColorImage::from_rgba_unmultiplied([w, h], &self.comp)
            };
            match &mut self.tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => self.tex = Some(ctx.load_texture("canvas", img, egui::TextureOptions::LINEAR)),
            }
            self.needs_full = false;
            self.shown = key;
        }
    }

    fn viewing_camera(&self) -> bool {
        self.cam_view && self.doc.camera.enabled
    }

    fn refresh_rect(&mut self, r: Rect) {
        if r.is_empty() || self.tex.is_none() {
            return;
        }
        self.paint_stamp += 1;
        if self.viewing_camera() {
            self.needs_full = true;
            return;
        }
        let onion = !self.playing;
        self.doc.composite(self.doc.frame, onion, r, &mut self.comp);
        let sub = copy_rect(&self.comp, self.doc.w, r);
        let img = egui::ColorImage::from_rgba_unmultiplied([r.x1 - r.x0, r.y1 - r.y0], &sub);
        if let Some(t) = &mut self.tex {
            t.set_partial([r.x0, r.y0], img, egui::TextureOptions::LINEAR);
        }
    }

    // ---------- panels ----------
    fn menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                // Menu bar in the reference's order: File, Edit, Project, Clip, Layer,
                // Image, Effects, View, Windows, Help.
                ui.menu_button(tr("Fichier"), |ui| {
                    if ui.button(tr("Nouveau projet…")).clicked() {
                        self.new_dialog = Some(NewDialog {
                            template: None,
                            preset: 0,
                            w: 1920,
                            h: 1080,
                            fps: 24.0,
                        });
                        ui.close_menu();
                    }
                    if ui.button(tr("Ouvrir…")).clicked() {
                        ui.close_menu();
                        self.open_dialog();
                    }
                    if ui.button(tr("Enregistrer")).clicked() {
                        ui.close_menu();
                        self.save(false);
                    }
                    if ui.button(tr("Enregistrer sous…")).clicked() {
                        ui.close_menu();
                        self.save(true);
                    }
                    ui.separator();
                    ui.menu_button(tr("Importer"), |ui| {
                        if ui.button(tr("Importer une image (PNG, JPEG)…")).clicked() {
                            ui.close_menu();
                            self.import_picture();
                        }
                        if ui.button(tr("Importer une suite d'images (nouveau calque)…")).clicked() {
                            ui.close_menu();
                            self.import_sequence();
                        }
                        if ui.button(tr("Importer un PSD (ses calques deviennent des calques)…")).clicked() {
                            ui.close_menu();
                            self.import_psd();
                        }
                        let ff = io::find_ffmpeg().is_some();
                        if ui
                            .button(tr("Importer une vidéo (nouveau calque)…"))
                            .on_hover_text(if ff {
                                tr("Avec FFmpeg")
                            } else {
                                tr("Nécessite FFmpeg (ffmpeg.org)")
                            })
                            .clicked()
                        {
                            ui.close_menu();
                            self.import_video();
                        }
                        if ui.button(tr("Importer un son (WAV, MP3)…")).clicked() {
                            ui.close_menu();
                            self.import_sound(None);
                        }
                    });
                    ui.menu_button(tr("Exporter"), |ui| {
                        if ui.button(tr("Film complet (tous les plans) en PNG…")).clicked() {
                            ui.close_menu();
                            self.export_film(false);
                        }
                        if ui.button(tr("Film complet (tous les plans) en MP4…")).clicked() {
                            ui.close_menu();
                            self.export_film(true);
                        }
                        ui.separator();
                        if ui.button(tr("Suite d'images PNG…")).clicked() {
                            ui.close_menu();
                            self.export();
                        }
                        ui.menu_button(tr("Suite d'images dans un autre format"), |ui| {
                            for (fmt, label) in [("tiff", "TIFF"), ("tga", "TGA"), ("jpeg", "JPEG"), ("bmp", "BMP")] {
                                if ui.button(format!("{label}…")).clicked() {
                                    ui.close_menu();
                                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                        match io::export_frames_as(&self.doc, &dir, fmt) {
                                            Ok(n) => self.status = trf("{} images exportées dans {}", &[&n, &dir.display()]),
                                            Err(e) => self.status = trf("Échec de l'export : {}", &[&e]),
                                        }
                                    }
                                }
                            }
                        });
                        if ui.button(tr("GIF animé…")).clicked() {
                            ui.close_menu();
                            self.export_gif();
                        }
                        if ui.button(tr("Image PSD avec ses calques (image courante)…")).clicked() {
                            ui.close_menu();
                            self.export_psd();
                        }
                        let ff = io::find_ffmpeg().is_some();
                        if ui
                            .button(tr("Vidéo MP4…"))
                            .on_hover_text(if ff {
                                tr("Avec FFmpeg")
                            } else {
                                tr("Nécessite FFmpeg (ffmpeg.org)")
                            })
                            .clicked()
                        {
                            ui.close_menu();
                            self.export_video();
                        }
                        if ui
                            .button(tr("Structure des calques pour After Effects (JSON + images)…"))
                            .on_hover_text(tr("Lisible par le script d'import After Effects (JSON)"))
                            .clicked()
                        {
                            ui.close_menu();
                            self.export_ae();
                        }
                    });
                    ui.separator();
                    if ui.button(tr("Quitter")).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button(tr("Édition"), |ui| {
                    if ui.add_enabled(self.doc.can_undo(), egui::Button::new(tr("Annuler"))).clicked() {
                        self.doc.undo();
                        self.needs_full = true;
                        ui.close_menu();
                    }
                    if ui.add_enabled(self.doc.can_redo(), egui::Button::new(tr("Rétablir"))).clicked() {
                        self.doc.redo();
                        self.needs_full = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(tr("Historique d'annulation…")).clicked() {
                        self.show_history = true;
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(self.doc.can_undo(), egui::Button::new(tr("Marquer cet état")))
                        .on_hover_text(tr(
                            "Pose une marque sur l'état actuel (ou la retire) : un endroit où revenir d'un seul coup, par exemple avant un essai.",
                        ))
                        .clicked()
                    {
                        self.toggle_undo_mark();
                        ui.close_menu();
                    }
                    if ui.button(tr("Revenir à la marque d'annulation précédente")).clicked() {
                        self.go_undo_mark(false);
                        ui.close_menu();
                    }
                    if ui.button(tr("Aller à la marque d'annulation suivante")).clicked() {
                        self.go_undo_mark(true);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(tr("Préférences…")).clicked() {
                        self.show_prefs = true;
                        ui.close_menu();
                    }
                    ui.menu_button(tr("Préférences"), |ui| {
                        ui.menu_button(tr("Langue"), |ui| {
                            for l in i18n::ALL_LANGS {
                                if ui.radio(i18n::lang() == l, l.name()).clicked() {
                                    i18n::set_lang(l);
                                    i18n::save_lang(l);
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.menu_button(tr("Thème"), |ui| {
                            for (i, t) in THEMES.iter().enumerate() {
                                if ui.radio(self.theme == i, tr(t.name)).clicked() {
                                    self.theme = i;
                                    apply_theme(ctx, t);
                                    ui.close_menu();
                                }
                            }
                        });
                        if ui.button(tr("Raccourcis clavier…")).clicked() {
                            self.show_shortcuts = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .button(tr("Exporter mes réglages (raccourcis, préréglages, langue, thème, palette)…"))
                            .clicked()
                        {
                            ui.close_menu();
                            self.export_settings();
                        }
                        if ui.button(tr("Importer des réglages…")).clicked() {
                            ui.close_menu();
                            self.import_settings(ctx);
                        }
                    });
                });
                ui.menu_button(tr("Projet"), |ui| {
                    if ui.button(tr("Storyboard (plans)…")).clicked() {
                        self.show_storyboard = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label(trf("Taille : {} × {}", &[&self.doc.w, &self.doc.h]));
                    ui.menu_button(tr("Papier"), |ui| {
                        for p in crate::doc::ALL_PAPERS {
                            if ui.selectable_value(&mut self.doc.paper_texture, p, p.label()).changed() {
                                self.needs_full = true;
                            }
                        }
                    });
                });
                ui.menu_button(tr("Clip"), |ui| {
                    if ui.button(if self.playing { tr("Arrêter") } else { tr("Lire") }).clicked() {
                        self.toggle_play();
                        ui.close_menu();
                    }
                    ui.checkbox(&mut self.looping, tr("Lecture en boucle"));
                    ui.separator();
                    let f = self.doc.frame;
                    if ui.button(tr("Signet ici (ajouter / retirer)")).clicked() {
                        self.doc.toggle_bookmark(f);
                    }
                    if ui.button(tr("Signet précédent")).clicked() {
                        if let Some(b) = self.doc.prev_bookmark(f) {
                            self.doc.frame = b;
                        }
                    }
                    if ui.button(tr("Signet suivant")).clicked() {
                        if let Some(b) = self.doc.next_bookmark(f) {
                            self.doc.frame = b;
                        }
                    }
                    if ui.button(tr("Début de lecture ici")).clicked() {
                        self.doc.mark_in = Some(f);
                    }
                    if ui.button(tr("Fin de lecture ici")).clicked() {
                        self.doc.mark_out = Some(f);
                    }
                    if ui.button(tr("Effacer début et fin de lecture")).clicked() {
                        self.doc.mark_in = None;
                        self.doc.mark_out = None;
                    }
                    ui.separator();
                    if ui.button(tr("Importer un son (WAV, MP3)…")).clicked() {
                        ui.close_menu();
                        self.import_sound(None);
                    }
                    if ui.checkbox(&mut self.cam_view, tr("Voir à travers la caméra")).changed() {
                        self.needs_full = true;
                        self.fit_pending = true;
                        self.tex = None;
                    }
                });
                ui.menu_button(tr("Calque"), |ui| {
                    if ui.button(tr("Nouveau calque")).clicked() {
                        self.doc.add_layer();
                        self.needs_full = true;
                        ui.close_menu();
                    }
                    if ui.button(tr("Dupliquer le calque")).clicked() {
                        self.doc.duplicate_layer();
                        self.needs_full = true;
                        ui.close_menu();
                    }
                    if ui.button(tr("Supprimer le calque")).clicked() {
                        self.delete_layers();
                        ui.close_menu();
                    }
                    if ui.button(tr("Monter le calque")).clicked() {
                        self.doc.move_layer(true);
                        self.needs_full = true;
                    }
                    if ui.button(tr("Descendre le calque")).clicked() {
                        self.doc.move_layer(false);
                        self.needs_full = true;
                    }
                    ui.separator();
                    if ui.button(tr("Copier le calque")).clicked() {
                        self.layer_clip = Some(self.doc.copy_layer(self.doc.current_layer));
                        self.status = tr("Calque copié.").into();
                        ui.close_menu();
                    }
                    if ui.button(tr("Couper le calque")).clicked() {
                        self.layer_clip = Some(self.doc.copy_layer(self.doc.current_layer));
                        self.doc.delete_layer();
                        self.needs_full = true;
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(self.layer_clip.is_some(), egui::Button::new(tr("Coller le calque")))
                        .clicked()
                    {
                        if let Some(c) = self.layer_clip.clone() {
                            self.doc.paste_layer(&c);
                            self.needs_full = true;
                        }
                        ui.close_menu();
                    }
                    ui.horizontal(|ui| {
                        ui.label(tr("Étirer à"));
                        ui.add(egui::DragValue::new(&mut self.stretch_len).range(1..=100000));
                        ui.label(tr("images"));
                        if ui.button(tr("Étirer le calque")).clicked() {
                            let l = self.doc.current_layer;
                            if self.doc.stretch_layer(l, self.stretch_len) {
                                self.needs_full = true;
                            } else {
                                self.status = tr("Rien à étirer sur ce calque.").into();
                            }
                            ui.close_menu();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Ratio de temps"));
                        ui.add(
                            egui::DragValue::new(&mut self.stretch_pct)
                                .range(1.0..=10000.0)
                                .speed(1.0)
                                .max_decimals(0)
                                .suffix(" %"),
                        );
                        if ui
                            .button(tr("Appliquer le ratio"))
                            .on_hover_text(tr(
                                "Étire le calque (et les calques choisis avec lui) dans cette proportion : 200 % = deux fois plus lent, 50 % = deux fois plus rapide.",
                            ))
                            .clicked()
                        {
                            self.stretch_by_ratio();
                            ui.close_menu();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Exposition de chaque dessin"));
                        ui.add(egui::DragValue::new(&mut self.retime_n).range(1..=999));
                        if ui
                            .button(tr("Recaler le calque"))
                            .on_hover_text(tr(
                                "Chaque dessin du calque dure ce nombre d'images : 1 = une image par dessin, 2 = animation « par deux »…",
                            ))
                            .clicked()
                        {
                            self.retime_layer();
                            ui.close_menu();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.shift_n).range(-100000..=100000))
                            .on_hover_text(tr("Nombre d'images : positif = plus tard, négatif = plus tôt"));
                        if ui
                            .button(tr("Décaler le calque"))
                            .on_hover_text(tr(
                                "Déplace toute l'animation du calque (et des calques choisis avec lui) dans le temps.",
                            ))
                            .clicked()
                        {
                            self.shift_layers(self.shift_n);
                            ui.close_menu();
                        }
                        if ui
                            .button(tr("Faire glisser le contenu"))
                            .on_hover_text(tr(
                                "Les dessins se déplacent à l'intérieur du calque, qui garde son début et sa fin.",
                            ))
                            .clicked()
                        {
                            self.slide_layer(self.shift_n);
                            ui.close_menu();
                        }
                    });
                    if ui
                        .button(tr("Scinder le calque ici"))
                        .on_hover_text(tr(
                            "Coupe le calque en deux à l'image courante : la suite va dans un nouveau calque au-dessus.",
                        ))
                        .clicked()
                    {
                        self.split_layer(self.doc.frame);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(tr("Fusionner avec le calque du dessous")).clicked() {
                        self.merge(false);
                        ui.close_menu();
                    }
                    if ui.button(tr("Fusionner les calques visibles")).clicked() {
                        self.merge(true);
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(
                            self.doc.target_layers().len() > 1,
                            egui::Button::new(tr("Fusionner les calques choisis")),
                        )
                        .clicked()
                    {
                        self.merge_selected();
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(
                            self.doc.target_layers().len() > 1,
                            egui::Button::new(tr("Joindre les calques choisis")),
                        )
                        .on_hover_text(tr(
                            "Met les calques choisis bout à bout dans un seul calque, du bas vers le haut (l'inverse de scinder).",
                        ))
                        .clicked()
                    {
                        self.join_selected();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .button(tr("Choisir tous les calques"))
                        .on_hover_text(tr(
                            "Ctrl + clic sur le nom d'un calque l'ajoute au choix, Maj + clic choisit jusqu'à lui.",
                        ))
                        .clicked()
                    {
                        self.doc.select_layers(true);
                        ui.close_menu();
                    }
                    if ui.button(tr("Ne garder que le calque courant")).clicked() {
                        self.doc.select_layers(false);
                        ui.close_menu();
                    }
                    ui.separator();
                    let cl = self.doc.current_layer;
                    ui.checkbox(&mut self.doc.layers[cl].stencil, tr("Pochoir"));
                    let mut folded = self.doc.layers[cl].collapsed;
                    if ui
                        .checkbox(&mut folded, tr("Calque replié"))
                        .on_hover_text(tr(
                            "Un calque replié ne prend qu'une ligne fine dans la timeline (aussi : le petit triangle à droite de son nom).",
                        ))
                        .changed()
                    {
                        self.collapse_layers(false, Some(folded));
                    }
                    if ui.button(tr("Début du calque ici")).clicked() {
                        self.trim_layer_start(self.doc.frame);
                        ui.close_menu();
                    }
                    if ui.button(tr("Fin du calque ici")).clicked() {
                        self.doc.push_structure_undo();
                        let f = self.doc.frame;
                        self.doc.layers[cl].end = Some(f);
                        self.needs_full = true;
                    }
                    if ui.button(tr("Jusqu'au bout")).clicked() {
                        self.doc.layers[cl].end = None;
                        self.needs_full = true;
                    }
                    ui.separator();
                    ui.checkbox(&mut self.doc.auto_break, tr("Peindre sur une tenue crée un dessin"))
                        .on_hover_text(tr(
                            "Dessiner sur une image tenue la transforme en un dessin à elle (copie du dessin tenu).",
                        ));
                    ui.checkbox(&mut self.doc.auto_stretch, tr("Allonger le calque tout seul"))
                        .on_hover_text(tr(
                            "Peindre après la fin d'un calque l'allonge jusque-là (sinon, la question est posée).",
                        ));
                    ui.separator();
                    if ui.button(tr("Importer une suite d'images (nouveau calque)…")).clicked() {
                        ui.close_menu();
                        self.import_sequence();
                    }
                });
                ui.menu_button(tr("Image§menu"), |ui| {
                    if ui.button(tr("Nouvelle image après")).clicked() {
                        self.doc.add_blank_after();
                        self.needs_full = true;
                        ui.close_menu();
                    }
                    if ui.button(tr("Dupliquer l'image")).clicked() {
                        self.doc.duplicate_after();
                        self.needs_full = true;
                        ui.close_menu();
                    }
                    if ui.button(tr("Exposition +")).clicked() {
                        self.doc.expose_more_n(self.expo_n);
                        self.needs_full = true;
                    }
                    if ui.button(tr("Exposition −")).clicked() {
                        self.doc.expose_less_n(self.expo_n);
                        self.needs_full = true;
                    }
                    if ui.button(tr("Supprimer la cellule")).clicked() {
                        let (l, a, b) = self.cell_range();
                        self.doc.delete_cells(l, a, b);
                        self.cell_sel = None;
                        self.needs_full = true;
                        ui.close_menu();
                    }
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.tween_n).range(1..=99));
                        if ui
                            .button(tr("Insérer des intervalles"))
                            .on_hover_text(tr(
                                "Ajoute ce nombre de dessins entre ce dessin et le suivant du calque.",
                            ))
                            .clicked()
                        {
                            self.insert_tweens();
                            ui.close_menu();
                        }
                        ui.checkbox(&mut self.tween_mix, tr("en fondu"))
                            .on_hover_text(tr(
                                "Chaque intervalle est un mélange des deux dessins qui l'entourent, pour dessiner par-dessus ; sinon il est vide.",
                            ));
                    });
                    ui.separator();
                    ui.menu_button(tr("Marque de l'image"), |ui| {
                        let (l, f) = (self.doc.current_layer, self.doc.frame);
                        self.mark_menu(ui, l, f);
                    });
                    if ui.button(tr("Image marquée précédente")).clicked() {
                        self.step_mark(false);
                    }
                    if ui.button(tr("Image marquée suivante")).clicked() {
                        self.step_mark(true);
                    }
                    ui.separator();
                    if ui.button(tr("Effacer le dessin")).clicked() {
                        self.clear_drawing();
                        ui.close_menu();
                    }
                    if ui.button(tr("Retourner horizontalement")).clicked() {
                        self.flip_image(true);
                        ui.close_menu();
                    }
                    if ui.button(tr("Retourner verticalement")).clicked() {
                        self.flip_image(false);
                        ui.close_menu();
                    }
                    if ui.button(tr("Importer une image (PNG, JPEG)…")).clicked() {
                        ui.close_menu();
                        self.import_picture();
                    }
                });
                ui.menu_button(tr("Effets"), |ui| {
                    if ui.button(tr("Effets du calque (pile)…")).clicked() {
                        self.show_fx_stack = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    for fx in ALL_EFFECTS {
                        if ui.button(format!("{}…", fx.label())).clicked() {
                            ui.close_menu();
                            self.open_fx(fx);
                        }
                    }
                });
                ui.menu_button(tr("Affichage"), |ui| {
                    if ui.button(tr("Ajuster à la fenêtre")).clicked() {
                        self.fit_pending = true;
                        ui.close_menu();
                    }
                    if ui.button(tr("Taille réelle (100 %)")).clicked() {
                        self.zoom = 1.0;
                        self.pan = Vec2::ZERO;
                        ui.close_menu();
                    }
                    if ui.button(tr("Tourner la vue de +15°")).clicked() {
                        self.view_rot = (self.view_rot + 15.0) % 360.0;
                    }
                    if ui.button(tr("Tourner la vue de −15°")).clicked() {
                        self.view_rot = (self.view_rot - 15.0) % 360.0;
                    }
                    if ui.button(tr("Remettre la vue droite")).clicked() {
                        self.view_rot = 0.0;
                    }
                    ui.checkbox(&mut self.view_flip, tr("Vue en miroir"));
                    ui.separator();
                    if ui.checkbox(&mut self.doc.light.active, tr("Table lumineuse")).changed() {
                        self.needs_full = true;
                    }
                    if ui.checkbox(&mut self.cam_view, tr("Voir à travers la caméra")).changed() {
                        self.needs_full = true;
                        self.fit_pending = true;
                        self.tex = None;
                    }
                    ui.separator();
                    ui.checkbox(&mut self.show_grid, tr("Grille"));
                    ui.horizontal(|ui| {
                        ui.label(tr("Pas de la grille"));
                        ui.add(egui::DragValue::new(&mut self.grid_size).range(4.0..=1000.0).suffix(" px"));
                    });
                    ui.checkbox(&mut self.show_thirds, tr("Règle des tiers"));
                    ui.checkbox(&mut self.show_center, tr("Centre et zone de sécurité"));
                });
                ui.menu_button(tr("Fenêtres"), |ui| {
                    ui.menu_button(tr("Espaces de travail"), |ui| {
                        for i in 0..self.rooms.len() {
                            if ui.radio(self.room == i, self.rooms[i].name.clone()).clicked() {
                                self.switch_room(i);
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        ui.label(tr("Nom de l'espace de travail"));
                        if self.room_name_edit.is_empty() {
                            self.room_name_edit = self.rooms[self.room].name.clone();
                        }
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.room_name_edit).desired_width(140.0));
                            if ui.button(tr("Renommer")).clicked() && !self.room_name_edit.trim().is_empty() {
                                self.rooms[self.room].name = self.room_name_edit.trim().to_string();
                                self.layout_dirty = true;
                            }
                        });
                        ui.separator();
                        if ui.button(tr("Exporter l'espace de travail…")).clicked() {
                            ui.close_menu();
                            if let Some(p) = rfd::FileDialog::new()
                                .add_filter(tr("Espace de travail"), &["json"])
                                .set_file_name("room.json")
                                .save_file()
                            {
                                if let Err(e) = crate::layout::export_room(&p, &self.rooms[self.room]) {
                                    self.status = trf("Échec de l'enregistrement : {}", &[&e]);
                                }
                            }
                        }
                        if ui.button(tr("Importer l'espace de travail…")).clicked() {
                            ui.close_menu();
                            if let Some(p) = rfd::FileDialog::new().add_filter(tr("Espace de travail"), &["json"]).pick_file() {
                                match crate::layout::import_room(&p) {
                                    Ok(r) => {
                                        self.room_name_edit = r.name.clone();
                                        self.rooms[self.room] = r;
                                        self.tl_cell = self.rooms[self.room].timeline_cell.clamp(TL_CELL_MIN, TL_CELL_MAX);
                                        self.layout_dirty = true;
                                    }
                                    Err(e) => self.status = trf("Échec de l'ouverture : {}", &[&e]),
                                }
                            }
                        }
                        if ui.button(tr("Remettre cet espace par défaut")).clicked() {
                            self.rooms[self.room] = crate::layout::Room::default_room(self.room);
                            self.tl_cell = TL_CELL;
                            self.room_name_edit.clear();
                            self.layout_dirty = true;
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    for p in ALL_PANELS {
                        let mut on = self.rooms[self.room].is_shown(p);
                        if ui.checkbox(&mut on, p.title()).changed() {
                            if on {
                                self.rooms[self.room].show(p);
                            } else {
                                self.rooms[self.room].remove(p);
                            }
                            self.layout_dirty = true;
                        }
                    }
                    let r = &mut self.rooms[self.room];
                    if ui.checkbox(&mut r.timeline, tr("Timeline (en bas)")).changed() {
                        self.layout_dirty = true;
                    }
                    if ui.checkbox(&mut r.left_pinned, tr("Colonne de gauche épinglée")).changed() {
                        self.layout_dirty = true;
                    }
                    if ui.checkbox(&mut r.right_pinned, tr("Colonne de droite épinglée")).changed() {
                        self.layout_dirty = true;
                    }
                    if ui.checkbox(&mut r.timeline_pinned, tr("Timeline épinglée")).changed() {
                        self.layout_dirty = true;
                    }
                    ui.separator();
                    if ui.button(tr("Feuille d'exposition")).clicked() {
                        self.show_xsheet = true;
                        ui.close_menu();
                    }
                    if ui.button(tr("Storyboard (plans)…")).clicked() {
                        self.show_storyboard = true;
                        ui.close_menu();
                    }
                    if ui.button(tr("Raccourcis clavier…")).clicked() {
                        self.show_shortcuts = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button(tr("Aide"), |ui| {
                    if ui.button(tr("Connecter une IA (MCP)…")).clicked() {
                        self.show_ai = true;
                        ui.close_menu();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for i in (0..self.rooms.len()).rev() {
                        if ui
                            .selectable_label(self.room == i, format!("{}", i + 1))
                            .on_hover_text(trf("Espace de travail : {}", &[&self.rooms[i].name]))
                            .clicked()
                        {
                            self.switch_room(i);
                            self.room_name_edit.clear();
                        }
                    }
                    ui.separator();
                    ui.label(format!("{:.0} %", self.zoom * 100.0));
                    if let Some((x, y)) = self.cursor_px {
                        ui.label(format!("x {:.0}  y {:.0}", x, y));
                    }
                    ui.label(format!(
                        "{} × {} · {} {}{}",
                        self.doc.w,
                        self.doc.h,
                        self.doc.fps,
                        tr("i/s"),
                        if self.doc.dirty { tr(" · modifié") } else { "" }
                    ));
                });
            });
        });
    }

    /// The reference's "Stretch Layer ?" question: painting after a layer's end with auto stretch off.
    fn stretch_window(&mut self, ctx: &egui::Context) {
        let Some((l, f)) = self.doc.stretch_asked else { return };
        if l >= self.doc.layers.len() {
            self.doc.stretch_asked = None;
            return;
        }
        let mut answer = None;
        egui::Window::new(tr("Allonger le calque ?"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(trf(
                    "Cette image est après la fin du calque « {} ». L'allonger jusqu'à l'image {} ?",
                    &[&self.doc.layers[l].name, &(f + 1)],
                ));
                ui.horizontal(|ui| {
                    if ui.button(tr("Allonger")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(tr("Fermer")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        if let Some(yes) = answer {
            if yes {
                self.doc.push_structure_undo();
                self.doc.layers[l].end = Some(f);
                self.needs_full = true;
            }
            self.doc.stretch_asked = None;
        }
    }

    fn sound_panel(&mut self, ui: &mut egui::Ui) {
        self.sync_audio();
        let mut remove = None;
        let mut replace = None;
        let mut changed = false;
        let fps = self.doc.fps.max(1.0);
        let many = self.doc.sounds.len() > 1;
        for (k, t) in self.doc.sounds.iter_mut().enumerate() {
            ui.push_id(("sound-track", k), |ui| {
                if k > 0 {
                    ui.separator();
                }
                let name = t.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let title = if many { format!("{} · {name}", k + 1) } else { name };
                ui.label(egui::RichText::new(title).monospace())
                    .on_hover_text(t.path.display().to_string());
                if self.audio.get(k).is_none_or(|c| c.is_none()) {
                    ui.colored_label(Color32::from_rgb(255, 190, 80), tr("Fichier introuvable"));
                }
                let colors = &self.doc.groups;
                ui.horizontal(|ui| {
                    changed |= ui
                        .checkbox(&mut t.locked, tr("Verrouillée"))
                        .on_hover_text(tr("Une piste verrouillée ne peut être ni modifiée ni retirée"))
                        .changed();
                    // the track's colour in the timeline: the usual blue, or a colour group's
                    let now = sound_color(t.color, colors);
                    ui.menu_button(egui::RichText::new("■").color(now), |ui| {
                        ui.horizontal(|ui| {
                            for c in 0..=12u8 {
                                let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::click());
                                ui.painter().rect_filled(r, 3.0, sound_color(c, colors));
                                if t.color == c {
                                    ui.painter()
                                        .rect_stroke(r, 3.0, Line::new(2.0_f32, Color32::WHITE), StrokeKind::Inside);
                                }
                                if resp.clicked() && !t.locked {
                                    t.color = c;
                                    changed = true;
                                    ui.close_menu();
                                }
                            }
                        });
                    })
                    .response
                    .on_hover_text(tr("Couleur de la piste dans la timeline"));
                });
                let locked = t.locked;
                ui.add_enabled_ui(!locked, |ui| {
                    changed |= ui.checkbox(&mut t.mute, tr("Muet")).changed();
                    let mut vol = t.volume * 100.0;
                    if ui
                        .add(egui::Slider::new(&mut vol, 0.0..=200.0).text(tr("Volume")).suffix(" %"))
                        .changed()
                    {
                        t.volume = vol / 100.0;
                        changed = true;
                    }
                    let mut off_frames = (t.offset * fps).round() as i32;
                    ui.horizontal(|ui| {
                        ui.label(tr("Décalage"));
                        if ui
                            .add(
                                egui::DragValue::new(&mut off_frames)
                                    .range(-100000..=100000)
                                    .suffix(format!(" {}", tr("images"))),
                            )
                            .changed()
                        {
                            t.offset = off_frames as f32 / fps;
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Fondu"));
                        changed |= ui
                            .add(egui::DragValue::new(&mut t.fade_in).range(0.0..=60.0).speed(0.05).suffix(" s"))
                            .on_hover_text(tr("Fondu d'entrée"))
                            .changed();
                        changed |= ui
                            .add(egui::DragValue::new(&mut t.fade_out).range(0.0..=60.0).speed(0.05).suffix(" s"))
                            .on_hover_text(tr("Fondu de sortie"))
                            .changed();
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button(tr("Remplacer…")).clicked() {
                            replace = Some(k);
                        }
                        if ui.small_button(tr("Retirer le son")).clicked() {
                            remove = Some(k);
                        }
                    });
                });
            });
        }
        if !self.doc.sounds.is_empty() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(tr("Son en glissant"));
                egui::ComboBox::from_id_salt("scrub")
                    .selected_text([tr("Jamais"), tr("En glissant"), tr("Toujours")][self.scrub_mode.min(2) as usize])
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.scrub_mode, 0, tr("Jamais"));
                        ui.selectable_value(&mut self.scrub_mode, 1, tr("En glissant"));
                        ui.selectable_value(&mut self.scrub_mode, 2, tr("Toujours"));
                    });
                ui.add(
                    egui::DragValue::new(&mut self.scrub_secs)
                        .range(0.02..=2.0)
                        .speed(0.01)
                        .suffix(" s"),
                );
            });
        }
        let add = if self.doc.sounds.is_empty() {
            tr("Importer un son (WAV, MP3)…")
        } else {
            tr("Ajouter une piste son…")
        };
        if ui.button(add).clicked() {
            self.import_sound(None);
        }
        if changed {
            self.doc.dirty = true;
            if self.playing {
                self.start_sound();
            }
        }
        if replace.is_some() {
            self.import_sound(replace);
        }
        if let Some(k) = remove {
            self.doc.sounds.remove(k);
            if k < self.audio.len() {
                self.audio.remove(k);
                self.audio_paths.remove(k);
            }
            self.doc.dirty = true;
            self.player.stop();
            self.peaks_key.clear();
            if self.playing {
                self.start_sound();
            }
        }
    }

    fn toggle_play(&mut self) {
        self.playing = !self.playing;
        self.last_tick = Instant::now();
        self.needs_full = true;
        if self.playing {
            let (lo, hi) = self.doc.play_range();
            if self.doc.frame < lo || self.doc.frame > hi {
                self.doc.frame = lo;
            }
            self.start_sound();
        } else {
            self.player.stop();
        }
    }

    // ---------- panels (movable: columns, floating windows, rooms) ----------

    fn panel_tools(&mut self, ui: &mut egui::Ui) {
        let bw = ((ui.available_width() - 4.0) / 2.0).max(56.0);
        egui::Grid::new("toolgrid").num_columns(2).spacing([4.0, 4.0]).show(ui, |ui| {
            for (i, t) in ALL_TOOLS.iter().enumerate() {
                let b = ui
                    .add_sized(
                        [bw, 22.0],
                        egui::SelectableLabel::new(self.tool == *t, egui::RichText::new(t.label()).size(12.5)),
                    )
                    .on_hover_text(trf("Raccourci : {}", &[&self.keymap.first_text(crate::keymap::Action::Tool(*t))]));
                if b.clicked() {
                    self.tool = *t;
                }
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    }

    fn panel_colours(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (r1, _) = ui.allocate_exact_size(vec2(40.0, 28.0), Sense::hover());
            ui.painter().rect_filled(r1, 3.0, c32(self.color));
            ui.painter()
                .rect_stroke(r1, 3.0, Line::new(1.0_f32, Color32::BLACK), StrokeKind::Inside);
            let (r2, _) = ui.allocate_exact_size(vec2(28.0, 20.0), Sense::hover());
            ui.painter().rect_filled(r2, 3.0, c32(self.color2));
            ui.painter()
                .rect_stroke(r2, 3.0, Line::new(1.0_f32, Color32::BLACK), StrokeKind::Inside);
            if ui.small_button("A/B").on_hover_text(tr("Échanger les couleurs (X)")).clicked() {
                std::mem::swap(&mut self.color, &mut self.color2);
            }
            ui.color_edit_button_srgb(&mut self.color2).on_hover_text(tr("Couleur B"));
        });
        let mut c = c32(self.color);
        if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
            self.color = [c.r(), c.g(), c.b()];
        }
        ui.add_space(4.0);
        egui::Grid::new("palette").spacing([2.0, 2.0]).show(ui, |ui| {
            let mut remove: Option<usize> = None;
            for (i, p) in self.doc.palette.clone().iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(28.0, 18.0), Sense::click());
                ui.painter().rect_filled(r, 2.0, c32(*p));
                if *p == self.color {
                    ui.painter()
                        .rect_stroke(r, 2.0, Line::new(2.0_f32, Color32::WHITE), StrokeKind::Inside);
                }
                if resp.clicked() {
                    self.color = *p;
                }
                if resp.secondary_clicked() {
                    if ui.input(|i| i.modifiers.alt) {
                        remove = Some(i);
                    } else {
                        self.color2 = *p;
                    }
                }
                if i % 6 == 5 {
                    ui.end_row();
                }
            }
            if let Some(i) = remove {
                self.doc.palette.remove(i);
                self.doc.dirty = true;
            }
        });
        ui.small(tr("Clic : couleur A · clic droit : couleur B"));
        ui.small(tr("Alt + clic droit : retirer de la palette"));
        ui.horizontal_wrapped(|ui| {
            if ui.small_button(tr("+ Ajouter la couleur")).clicked() && !self.doc.palette.contains(&self.color) {
                self.doc.palette.push(self.color);
                self.doc.dirty = true;
            }
            if ui.small_button(tr("Charger…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter(tr("Palette"), &["gpl"]).pick_file() {
                    match io::load_palette(&p) {
                        Ok(c) => {
                            self.doc.palette = c;
                            self.doc.dirty = true;
                        }
                        Err(e) => self.status = trf("Échec de l'ouverture : {}", &[&e]),
                    }
                }
            }
            if ui.small_button(tr("Enregistrer…")).clicked() {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter(tr("Palette"), &["gpl"])
                    .set_file_name("palette.gpl")
                    .save_file()
                {
                    if let Err(e) = io::save_palette(&p, &self.doc.palette) {
                        self.status = trf("Échec de l'enregistrement : {}", &[&e]);
                    }
                }
            }
        });
    }

    fn panel_settings(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().slider_width = (ui.available_width() - 95.0).max(60.0);
        let tool = self.tool;
        if tool.has_size() {
            let s = self.set_mut();
            ui.add(egui::Slider::new(&mut s.radius, 0.5..=300.0).logarithmic(true).text(tr("Taille")));
            ui.horizontal_wrapped(|ui| {
                for p in tool.presets() {
                    if ui.small_button(format!("{p:.0}")).clicked() {
                        s.radius = *p;
                    }
                }
            });
        }
        if tool == Tool::Fill {
            let s = self.set_mut();
            ui.add(egui::Slider::new(&mut s.power, 0.0..=100.0).text(tr("Tolérance")));
        } else if tool.paints() || tool.is_shape() {
            let s = self.set_mut();
            ui.add(egui::Slider::new(&mut s.power, 1.0..=100.0).text(tr("Puissance")));
        }
        if tool != Tool::Picker {
            let s = self.set_mut();
            ui.add(egui::Slider::new(&mut s.opacity, 1.0..=100.0).text(tr("Opacité")));
        }
        if tool.paints() {
            let s = self.set_mut();
            ui.add(egui::Slider::new(&mut s.aspect, 5.0..=100.0).text(tr("Aspect")));
            ui.add(egui::Slider::new(&mut s.angle, -180.0..=180.0).text(tr("Angle")).suffix("°"));
            ui.small(trf(
                "Espacement : {} px · force par touche : {} %",
                &[
                    &format!("{:.0}", brush::dab_spacing(tool, s.radius.max(brush::MIN_RADIUS))),
                    &format!("{:.0}", brush::dab_strength(s.power) * 100.0),
                ],
            ));
        }
        if tool.paints() && tool != Tool::Eraser && tool != Tool::Warp {
            let s = self.set_mut();
            egui::ComboBox::from_label(tr("Mode"))
                .selected_text(s.mode.label())
                .show_ui(ui, |ui| {
                    for m in crate::brush::ALL_DRAW_MODES {
                        ui.selectable_value(&mut s.mode, m, m.label());
                    }
                });
        }
        if tool.paints() || tool == Tool::Custom {
            ui.checkbox(&mut self.preserve, tr("Préserver la transparence"))
                .on_hover_text(tr("Peindre seulement sur ce qui est déjà dessiné."));
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.smooth_on, tr("Lissage du trait"));
                ui.add(egui::DragValue::new(&mut self.smooth_value).range(0.0..=200.0).speed(0.5));
            });
            if self.smooth_on {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.smooth_type, 2, tr("Corde"));
                    ui.radio_value(&mut self.smooth_type, 1, tr("Moyenne"));
                    ui.radio_value(&mut self.smooth_type, 3, tr("Points"));
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.smooth_catchup, tr("Rattraper à la fin"));
                    ui.checkbox(&mut self.smooth_hud, tr("Afficher la corde"));
                });
            }
            if self.doc.layers.iter().any(|l| l.stencil) {
                ui.checkbox(&mut self.stencil_invert, tr("Peindre dans le pochoir (sinon autour)"));
            }
        }
        if tool.paints() {
            let s = self.set_mut();
            ui.label(tr("Connexions (ce qui fait varier le trait) :"));
            for (k, label) in [(0, tr("Taille")), (1, tr("Puissance")), (2, tr("Aspect")), (3, tr("Angle"))] {
                let c = match k {
                    0 => &mut s.con_size,
                    1 => &mut s.con_power,
                    2 => &mut s.con_aspect,
                    _ => &mut s.con_angle,
                };
                // the older "pressure" switches become a pressure connection
                if k == 0 && c.source == crate::brush::ConSource::Off && s.size_pressure {
                    c.source = crate::brush::ConSource::Pressure;
                }
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(("con", k))
                        .width(86.0)
                        .selected_text(c.source.label())
                        .show_ui(ui, |ui| {
                            for src in crate::brush::ALL_SOURCES {
                                ui.selectable_value(&mut c.source, src, src.label());
                            }
                        });
                    ui.label(label);
                    if c.source != crate::brush::ConSource::Off {
                        ui.checkbox(&mut c.invert, tr("Inv."))
                            .on_hover_text(tr("Inverser : plus la pression est forte, plus la valeur baisse"));
                        ui.add(egui::DragValue::new(&mut c.curve).range(-1.0..=1.0).speed(0.02))
                            .on_hover_text(tr("Courbe : négative = doux, positive = dur"));
                    }
                });
            }
            s.size_pressure = s.con_size.source == crate::brush::ConSource::Pressure;
            s.power_pressure = s.con_power.source == crate::brush::ConSource::Pressure;
            ui.checkbox(&mut self.sim_pressure, tr("Simuler la pression avec la vitesse (souris)"))
                .on_hover_text(tr("Sans tablette : plus le trait est rapide, plus il est fin."));
        }
        if tool == Tool::Text {
            let t = &mut self.text;
            ui.add(egui::TextEdit::multiline(&mut t.text).desired_rows(2).desired_width(f32::INFINITY));
            ui.add(egui::Slider::new(&mut t.size, 6.0..=600.0).logarithmic(true).text(tr("Taille")));
            ui.horizontal(|ui| {
                ui.checkbox(&mut t.border_on, tr("Bordure"));
                ui.add(egui::DragValue::new(&mut t.border).range(0.0..=40.0).speed(0.2));
                ui.color_edit_button_srgb(&mut t.border_color);
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut t.italic_on, tr("Italique"));
                ui.add(egui::DragValue::new(&mut t.italic).range(-100.0..=100.0).suffix(" %"));
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut t.rotate_on, tr("Rotation"));
                ui.add(egui::DragValue::new(&mut t.rotate).range(-180.0..=180.0).suffix("°"));
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut t.track_on, tr("Espacement des lettres"));
                ui.add(egui::DragValue::new(&mut t.track).range(20.0..=400.0).suffix(" %"));
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut t.glow_on, tr("Lueur"));
                ui.add(egui::DragValue::new(&mut t.glow).range(0.0..=60.0).speed(0.2));
                ui.color_edit_button_srgb(&mut t.glow_color);
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut t.outline_on, tr("Lettres creuses"));
                ui.add(egui::DragValue::new(&mut t.outline).range(0.5..=30.0).speed(0.1));
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut t.opacity_on, tr("Opacité"));
                ui.add(egui::DragValue::new(&mut t.opacity).range(1.0..=100.0).suffix(" %"));
            });
            ui.small(tr("Cliquez sur la toile pour poser le texte (coin haut-gauche)."));
        }
        if tool == Tool::Select || self.floating.is_some() {
            self.selection_panel(ui);
        }
        if tool == Tool::Custom {
            self.custom_panel(ui);
        }
        if tool == Tool::Camera {
            self.camera_panel(ui);
        }
        if tool == Tool::Gradient {
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.grad_kind, GradientKind::Linear, tr("Linéaire"));
                ui.radio_value(&mut self.grad_kind, GradientKind::Radial, tr("Radial"));
            });
            ui.add(egui::Slider::new(&mut self.grad_mid, 0.05..=0.95).text(tr("Milieu")));
            ui.small(tr(
                "Glissez sur la toile : de la couleur A à la couleur B, dans la sélection s'il y en a une.",
            ));
        }
        if ui.small_button(tr("Valeurs par défaut")).clicked() {
            *self.set_mut() = tool.defaults();
            if tool == Tool::Text {
                self.text = TextSettings {
                    text: self.text.text.clone(),
                    ..TextSettings::default()
                };
            }
        }
    }

    fn panel_light(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().slider_width = (ui.available_width() - 95.0).max(60.0);
        let mut changed = false;
        let lt = &mut self.doc.light;
        changed |= ui.checkbox(&mut lt.active, tr("Activée (T)")).changed();
        ui.horizontal(|ui| {
            ui.label(tr("Avant"));
            changed |= ui.add(egui::DragValue::new(&mut lt.prev_count).range(0..=10)).changed();
            ui.label(tr("Après"));
            changed |= ui.add(egui::DragValue::new(&mut lt.next_count).range(0..=10)).changed();
        });
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut lt.use_prev_color, tr("Teinte avant")).changed();
            changed |= ui.color_edit_button_srgb(&mut lt.prev_color).changed();
        });
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut lt.use_next_color, tr("Teinte après")).changed();
            changed |= ui.color_edit_button_srgb(&mut lt.next_color).changed();
        });
        changed |= ui.checkbox(&mut lt.front, tr("Devant le dessin")).changed();
        changed |= ui
            .add(egui::Slider::new(&mut lt.opacity, 0.0..=100.0).text(tr("Opacité")))
            .changed();
        ui.label(tr("Opacité par image :"));
        let n = lt.prev_count.max(lt.next_count).max(1);
        for (k, r) in lt.ranges.iter_mut().take(n).enumerate() {
            changed |= ui.add(egui::Slider::new(r, 0.0..=100.0).text(format!("±{}", k + 1))).changed();
        }
        if changed {
            self.needs_full = true;
        }
    }

    fn panel_project(&mut self, ui: &mut egui::Ui) {
        ui.label(trf("Taille : {} × {}", &[&self.doc.w, &self.doc.h]));
        ui.horizontal(|ui| {
            ui.label(tr("Papier"));
            if ui.color_edit_button_srgb(&mut self.doc.paper).changed() {
                self.needs_full = true;
            }
            egui::ComboBox::from_id_salt("paper_tex")
                .selected_text(self.doc.paper_texture.label())
                .show_ui(ui, |ui| {
                    for p in crate::doc::ALL_PAPERS {
                        if ui.selectable_value(&mut self.doc.paper_texture, p, p.label()).changed() {
                            self.needs_full = true;
                        }
                    }
                });
        });
        ui.label(trf("Calques : {}", &[&self.doc.layers.len()]));
        ui.label(trf("Durée : {} s", &[&format!("{:.2}", self.doc.frames as f32 / self.doc.fps)]));
    }

    fn panel_shortcuts(&mut self, ui: &mut egui::Ui) {
        use crate::keymap::Action as KA;
        let pair = |a: KA, b: KA| {
            let (x, y) = (
                self.keymap.first_text(a),
                if a == b { String::new() } else { self.keymap.first_text(b) },
            );
            match (x.is_empty(), y.is_empty()) {
                (true, true) => "—".to_string(),
                (false, true) => x,
                (true, false) => y,
                _ => format!("{x} / {y}"),
            }
        };
        for (k, v) in [
            (pair(KA::PlayStop, KA::PlayStop), tr("lire / arrêter").to_string()),
            (pair(KA::PrevFrame, KA::NextFrame), tr("image précédente / suivante").to_string()),
            (pair(KA::LayerUp, KA::LayerDown), tr("calque au-dessus / dessous").to_string()),
            (pair(KA::BrushSmaller, KA::BrushBigger), tr("taille du pinceau").to_string()),
            (tr("Espace + glisser").to_string(), tr("déplacer la vue").to_string()),
            (tr("Molette").to_string(), tr("zoom").to_string()),
            (pair(KA::Undo, KA::Redo), tr("annuler / rétablir").to_string()),
            (pair(KA::Delete, KA::Delete), tr("effacer le dessin").to_string()),
            (pair(KA::FlipPrev, KA::FlipNext), tr("feuilleter (maintenir)").to_string()),
        ] {
            ui.horizontal(|ui| {
                ui.monospace(k);
                ui.small(v);
            });
        }
    }

    fn panel_correction(&mut self, ui: &mut egui::Ui) {
        ui.small(tr("Corrige les couleurs du dessin courant."));
        for fx in ALL_EFFECTS {
            if matches!(
                fx,
                Effect::BrightnessContrast { .. } | Effect::HueSaturation { .. } | Effect::Invert
            ) && ui.button(format!("{}…", fx.label())).clicked()
            {
                self.open_fx(fx);
            }
        }
    }

    /// Go to the previous (-1) or next (+1) drawing change of the current layer.
    fn step_instance(&mut self, d: i32) {
        let keys = self.doc.keys(self.doc.current_layer);
        let f = self.doc.frame;
        let to = if d < 0 {
            keys.iter().rev().map(|k| k.0).find(|&k| k < f)
        } else {
            keys.iter().map(|k| k.0).find(|&k| k > f)
        };
        if let Some(k) = to {
            self.doc.frame = k.min(self.doc.frames.saturating_sub(1));
            self.needs_full = true;
        }
    }

    /// The playback remote, with its buttons in the usual order: sound, loop and camera toggles, then previous image, play, next image, stop, first image,
    /// previous instance, next instance, last image.
    fn panel_remote(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            // one switch for every sound track
            let muted = !self.doc.sounds.is_empty() && self.doc.sounds.iter().all(|t| t.mute);
            if ui.selectable_label(!muted, "🔊").on_hover_text(tr("Son activé / coupé")).clicked() && !self.doc.sounds.is_empty() {
                for t in &mut self.doc.sounds {
                    t.mute = !muted;
                }
                if self.playing {
                    self.player.stop();
                    self.start_sound();
                }
            }
            if ui
                .selectable_label(self.looping, "🔁")
                .on_hover_text(tr("Lecture en boucle"))
                .clicked()
            {
                self.looping = !self.looping;
            }
            if ui
                .selectable_label(self.cam_view, "🎥")
                .on_hover_text(tr("Voir à travers la caméra"))
                .clicked()
            {
                self.cam_view = !self.cam_view;
                self.needs_full = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("◀").on_hover_text(tr("Image précédente")).clicked() {
                self.step_frame(-1);
            }
            if ui
                .button(if self.playing { "⏸" } else { "▶" })
                .on_hover_text(tr("lire / arrêter"))
                .clicked()
            {
                self.toggle_play();
            }
            if ui.button("▶|").on_hover_text(tr("Image suivante")).clicked() {
                self.step_frame(1);
            }
            if ui.button("⏹").on_hover_text(tr("Arrêter et revenir au début")).clicked() {
                if self.playing {
                    self.toggle_play();
                }
                self.doc.frame = self.doc.play_range().0;
                self.needs_full = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("⏮").on_hover_text(tr("Première image")).clicked() {
                self.doc.frame = 0;
                self.needs_full = true;
            }
            if ui.button("⇤").on_hover_text(tr("Dessin précédent")).clicked() {
                self.step_instance(-1);
            }
            if ui.button("⇥").on_hover_text(tr("Dessin suivant")).clicked() {
                self.step_instance(1);
            }
            if ui.button("⏭").on_hover_text(tr("Dernière image")).clicked() {
                self.doc.frame = self.doc.frames - 1;
                self.needs_full = true;
            }
        });
        ui.small(trf("Image {} / {}", &[&(self.doc.frame + 1), &self.doc.frames]));
    }

    /// The colour group panel, laid out like the reference's: one row per listed group
    /// with "n: name", its colour, Apply (puts the current layer in the group), for groups 1-12 a
    /// protect toggle (a protected group keeps its colour on reset and its layers stay locked when
    /// the group is unlocked), and an eye showing whether its layers are
    /// all shown, all hidden or mixed. The ☰ menu picks the groups listed; a right
    /// click on a row opens the group's own menu.
    /// Whether the timeline shows layer `li`: its group is shown there, or it is the current layer.
    fn layer_in_timeline(&self, li: usize) -> bool {
        if li == self.doc.current_layer {
            return true;
        }
        match self.doc.group_of(self.doc.layers[li].group) {
            Some(g) => self.group_in_timeline[g],
            None => true,
        }
    }

    fn panel_groups(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("☰", |ui| {
            for (g, on) in self.group_listed.iter_mut().enumerate() {
                ui.checkbox(on, format!("{g}: {}", self.doc.group_name(g)));
            }
            ui.separator();
            if ui.button(tr("Tout afficher")).clicked() {
                self.group_listed = [true; 13];
            }
            if ui.button(tr("Tout masquer")).clicked() {
                self.group_listed = [false; 13];
            }
            ui.separator();
            if ui.button(tr("Modifier les groupes…")).clicked() {
                self.group_edit = Some(self.doc.groups.clone());
                ui.close_menu();
            }
        })
        .response
        .on_hover_text(tr("Groupes affichés dans ce panneau"));
        let cl = self.doc.current_layer;
        for g in 0..self.doc.groups.len().min(13) {
            if !self.group_listed[g] {
                continue;
            }
            let col = self.doc.groups[g].color;
            let members: Vec<usize> = (0..self.doc.layers.len()).filter(|&l| self.doc.layers[l].group == col).collect();
            let row = ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                ui.painter().rect_filled(r, 2.0, c32(col));
                ui.painter()
                    .rect_stroke(r, 2.0, Line::new(1.0_f32, Color32::BLACK), StrokeKind::Inside);
                let current = self.doc.layers.get(cl).map(|l| l.group == col).unwrap_or(false);
                let label = format!("{g}: {}", self.doc.group_name(g));
                if ui
                    .selectable_label(current, label)
                    .on_hover_text(tr("Mettre le calque courant dans ce groupe"))
                    .clicked()
                    && cl < self.doc.layers.len()
                {
                    self.doc.layers[cl].group = col;
                }
                if g > 0 {
                    let p = self.group_protect[g];
                    if ui
                        .selectable_label(p, if p { "🔒" } else { "🔓" })
                        .on_hover_text(tr("Protéger le groupe (sa couleur et ses calques verrouillés)"))
                        .clicked()
                    {
                        self.group_protect[g] = !p;
                    }
                }
                let shown = members.iter().filter(|&&l| self.doc.layers[l].visible).count();
                let eye = if members.is_empty() || shown == members.len() {
                    "👁"
                } else if shown == 0 {
                    "—"
                } else {
                    "◐"
                };
                if ui.small_button(eye).on_hover_text(trf("{} calques", &[&members.len()])).clicked() {
                    let all = shown == members.len();
                    for &l in &members {
                        self.doc.layers[l].visible = !all;
                    }
                    self.needs_full = true;
                }
            });
            row.response.context_menu(|ui| {
                // action 5: every other group off in the timeline
                if ui.button(tr("Seulement ce groupe dans la timeline")).clicked() {
                    self.group_in_timeline = [false; 13];
                    self.group_in_timeline[g] = true;
                    ui.close_menu();
                }
                if ui.button(tr("Tous les groupes dans la timeline")).clicked() {
                    self.group_in_timeline = [true; 13];
                    ui.close_menu();
                }
                ui.checkbox(&mut self.group_in_timeline[g], tr("Ce groupe dans la timeline"));
                ui.separator();
                // actions 10 and 11: choose the group's layers, or add them to the chosen ones
                if ui
                    .add_enabled(!members.is_empty(), egui::Button::new(tr("Choisir les calques du groupe")))
                    .clicked()
                {
                    self.doc.select_layers(false);
                    for &l in &members {
                        self.doc.layers[l].selected = true;
                    }
                    self.doc.current_layer = *members.last().unwrap_or(&cl);
                    ui.close_menu();
                }
                if ui
                    .add_enabled(!members.is_empty(), egui::Button::new(tr("Ajouter ses calques au choix")))
                    .clicked()
                {
                    for &l in &members {
                        self.doc.layers[l].selected = true;
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(tr("Verrouiller les calques du groupe")).clicked() {
                    for &l in &members {
                        self.doc.layers[l].locked = true;
                    }
                    ui.close_menu();
                }
                if ui.button(tr("Déverrouiller les calques du groupe")).clicked() {
                    // a protected group's layers stay locked
                    if !self.group_protect[g] {
                        for &l in &members {
                            self.doc.layers[l].locked = false;
                        }
                    }
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(tr("Masquer les autres groupes")).clicked() {
                    for l in &mut self.doc.layers {
                        if l.group != col {
                            l.visible = false;
                        }
                    }
                    self.needs_full = true;
                    ui.close_menu();
                }
                if ui.button(tr("Afficher les autres groupes")).clicked() {
                    for l in &mut self.doc.layers {
                        if l.group != col {
                            l.visible = true;
                        }
                    }
                    self.needs_full = true;
                    ui.close_menu();
                }
                ui.separator();
                if ui.button(tr("Modifier les groupes…")).clicked() {
                    self.group_edit = Some(self.doc.groups.clone());
                    ui.close_menu();
                }
            });
        }
    }

    /// The colour group editor: colours and names of groups 1-12, with the usual four actions
    /// (back to the defaults, make these the defaults, save to a file, load from a file).
    /// Protected groups keep their colour and name. Nothing changes until "Apply".
    fn group_editor(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.group_edit.take() else { return };
        let mut open = true;
        let mut apply = false;
        let mut close = false;
        let keep = self.group_protect;
        // a whole table replaces the groups that aren't protected
        let put = |draft: &mut Vec<crate::doc::Group>, table: Vec<crate::doc::Group>| {
            for g in 1..13 {
                if !keep[g] {
                    draft[g] = table[g].clone();
                }
            }
        };
        egui::Window::new(tr("Modifier les groupes de couleur"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("group-edit").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
                    for (g, group) in draft.iter_mut().enumerate().skip(1) {
                        ui.label(format!("{g}"));
                        ui.add_enabled_ui(!keep[g], |ui| {
                            ui.color_edit_button_srgb(&mut group.color);
                        });
                        ui.add_enabled_ui(!keep[g], |ui| {
                            ui.add_sized(
                                [190.0, 20.0],
                                egui::TextEdit::singleline(&mut group.name).hint_text(tr(crate::doc::GROUP_NAMES[g])),
                            );
                        });
                        if keep[g] {
                            ui.label("🔒").on_hover_text(tr("Groupe protégé : il garde sa couleur et son nom"));
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button(tr("Valeurs par défaut"))
                        .on_hover_text(tr(
                            "Reprend vos groupes par défaut (ou ceux d'origine si vous n'en avez pas défini)",
                        ))
                        .clicked()
                    {
                        put(&mut draft, crate::doc::default_groups());
                    }
                    if ui.button(tr("Groupes d'origine")).clicked() {
                        put(&mut draft, crate::doc::builtin_groups());
                    }
                    if ui
                        .button(tr("Définir comme défaut"))
                        .on_hover_text(tr("Les nouveaux projets commenceront avec ces groupes"))
                        .clicked()
                    {
                        let table = crate::doc::clean_groups(draft.clone());
                        let same = table == crate::doc::builtin_groups();
                        if same {
                            crate::prefs::clear_default_groups();
                        }
                        match if same { Ok(()) } else { crate::prefs::save_default_groups(&table) } {
                            Ok(()) => self.status = tr("Groupes par défaut enregistrés.").into(),
                            Err(e) => self.status = trf("Échec de l'enregistrement : {}", &[&e]),
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(tr("Enregistrer dans un fichier…")).clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter(tr("Groupes de couleur"), &["txt"])
                            .set_file_name("groups.txt")
                            .save_file()
                        {
                            let text = crate::doc::groups_to_text(&crate::doc::clean_groups(draft.clone()));
                            if let Err(e) = std::fs::write(&p, text) {
                                self.status = trf("Échec de l'enregistrement : {}", &[&e]);
                            }
                        }
                    }
                    if ui.button(tr("Charger un fichier…")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter(tr("Groupes de couleur"), &["txt"]).pick_file() {
                            match std::fs::read_to_string(&p)
                                .map_err(|e| e.to_string())
                                .and_then(|t| crate::doc::groups_from_text(&t))
                            {
                                Ok(table) => put(&mut draft, table),
                                Err(e) => self.status = trf("Échec de l'ouverture : {}", &[&e]),
                            }
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(tr("Appliquer")).clicked() {
                        apply = true;
                    }
                    if ui.button(tr("Fermer")).clicked() {
                        close = true;
                    }
                });
            });
        if apply {
            let n = self.doc.set_groups(&draft, &keep);
            self.status = trf("{} groupes modifiés.", &[&n]);
            self.needs_full = true;
        } else if open && !close {
            self.group_edit = Some(draft);
        }
    }

    fn panel_body(&mut self, ui: &mut egui::Ui, p: PanelId) {
        match p {
            PanelId::Tools => self.panel_tools(ui),
            PanelId::Colours => self.panel_colours(ui),
            PanelId::Settings => self.panel_settings(ui),
            PanelId::Presets => self.presets_panel(ui),
            PanelId::Light => self.panel_light(ui),
            PanelId::Sound => self.sound_panel(ui),
            PanelId::Project => self.panel_project(ui),
            PanelId::Shortcuts => self.panel_shortcuts(ui),
            PanelId::Correction => self.panel_correction(ui),
            PanelId::Remote => self.panel_remote(ui),
            PanelId::Groups => self.panel_groups(ui),
        }
    }

    fn panel_title(&self, p: PanelId) -> String {
        match p {
            PanelId::Settings => trf("Réglages : {}", &[&self.tool.label()]),
            _ => p.title().to_string(),
        }
    }

    /// The title bar of a panel: drag it to move the panel; detach / close buttons.
    fn panel_header(&mut self, ui: &mut egui::Ui, p: PanelId, floating: bool) {
        let title = self.panel_title(p);
        ui.horizontal(|ui| {
            let h = ui
                .dnd_drag_source(egui::Id::new(("panel-handle", p)), p, |ui| {
                    ui.label(egui::RichText::new(format!("☰ {title}")).strong());
                })
                .response;
            h.on_hover_text(tr(
                "Glissez pour déplacer le panneau : dans une colonne, ou sur la toile pour le détacher.",
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("✕")
                    .on_hover_text(tr("Fermer le panneau (Fenêtres pour le rouvrir)"))
                    .clicked()
                {
                    self.layout_ops.push(LayoutOp::Hide(p));
                }
                if floating {
                    if ui.small_button("⇥").on_hover_text(tr("Ancrer à droite")).clicked() {
                        self.layout_ops.push(LayoutOp::Place(p, Side::Right, usize::MAX));
                    }
                    if ui.small_button("⇤").on_hover_text(tr("Ancrer à gauche")).clicked() {
                        self.layout_ops.push(LayoutOp::Place(p, Side::Left, usize::MAX));
                    }
                } else if ui.small_button("↗").on_hover_text(tr("Détacher (fenêtre flottante)")).clicked() {
                    let at = ui.min_rect().left_top() + vec2(30.0, 20.0);
                    self.layout_ops.push(LayoutOp::Float(p, [at.x, at.y]));
                }
                if p == PanelId::Shortcuts && ui.small_button(tr("Changer…")).clicked() {
                    self.show_shortcuts = true;
                }
            });
        });
    }

    /// The panels of one column, each in a frame that is also a drop place.
    fn column_contents(&mut self, ui: &mut egui::Ui, side: Side) {
        let room = self.room;
        ui.horizontal(|ui| {
            let pinned = if side == Side::Left {
                self.rooms[room].left_pinned
            } else {
                self.rooms[room].right_pinned
            };
            let lab = if pinned { tr("📌 Épinglée") } else { tr("📌 Épingler") };
            if ui
                .selectable_label(pinned, egui::RichText::new(lab).small())
                .on_hover_text(tr("Une colonne non épinglée se replie et ressort quand la souris touche le bord."))
                .clicked()
            {
                let r = &mut self.rooms[room];
                if side == Side::Left {
                    r.left_pinned = !r.left_pinned;
                } else {
                    r.right_pinned = !r.right_pinned;
                }
                self.layout_dirty = true;
            }
        });
        let list = if side == Side::Left {
            self.rooms[room].left.clone()
        } else {
            self.rooms[room].right.clone()
        };
        // content as wide as the space the column gives (never wider, so the column can't grow)
        let inner_w = (ui.available_width() - 24.0).max(110.0);
        let dragging = egui::DragAndDrop::has_payload_of_type::<PanelId>(ui.ctx());
        egui::ScrollArea::both()
            .id_salt(("column", side as u8, room))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (i, p) in list.iter().copied().enumerate() {
                    let fr = egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
                        ui.set_width(inner_w);
                        self.panel_header(ui, p, false);
                        ui.separator();
                        self.panel_body(ui, p);
                    });
                    let rect = fr.response.rect;
                    let zone = ui.interact(rect, egui::Id::new(("panel-drop", p)), Sense::hover());
                    if dragging {
                        if let Some(ptr) = ui.ctx().pointer_hover_pos().filter(|q| rect.contains(*q)) {
                            let y = if ptr.y < rect.center().y {
                                rect.top() - 2.0
                            } else {
                                rect.bottom() + 2.0
                            };
                            ui.painter().line_segment(
                                [pos2(rect.left(), y), pos2(rect.right(), y)],
                                Line::new(3.0_f32, Color32::from_rgb(60, 140, 255)),
                            );
                        }
                    }
                    if let Some(d) = zone.dnd_release_payload::<PanelId>() {
                        let below = ui.ctx().pointer_interact_pos().is_some_and(|q| q.y >= rect.center().y);
                        self.layout_ops.push(LayoutOp::Place(*d, side, if below { i + 1 } else { i }));
                    }
                }
                // the rest of the column: drop here to put a panel at the end
                let h = ui.available_height().max(48.0);
                let (rect, zone) = ui.allocate_exact_size(vec2(inner_w, h), Sense::hover());
                if dragging && zone.contains_pointer() {
                    ui.painter().rect_stroke(
                        rect.shrink(2.0),
                        4.0,
                        Line::new(2.0_f32, Color32::from_rgb(60, 140, 255)),
                        StrokeKind::Inside,
                    );
                }
                if list.is_empty() {
                    ui.painter().text(
                        rect.center_top() + vec2(0.0, 20.0),
                        egui::Align2::CENTER_CENTER,
                        tr("Déposez un panneau ici"),
                        egui::FontId::proportional(12.0),
                        ui.visuals().weak_text_color(),
                    );
                }
                if let Some(d) = zone.dnd_release_payload::<PanelId>() {
                    self.layout_ops.push(LayoutOp::Place(*d, side, list.len()));
                }
            });
    }

    /// A column: docked (pinned) or folded into a strip that slides out (unpinned).
    fn column(&mut self, ctx: &egui::Context, side: Side) {
        let room = self.room;
        let r = &self.rooms[room];
        let (empty, pinned, width) = match side {
            Side::Left => (r.left.is_empty(), r.left_pinned, r.left_width),
            Side::Right => (r.right.is_empty(), r.right_pinned, r.right_width),
        };
        let dragging = egui::DragAndDrop::has_payload_of_type::<PanelId>(ctx);
        if empty && !dragging {
            return;
        }
        let name = if side == Side::Left { "left" } else { "right" };
        if pinned {
            let panel = match side {
                Side::Left => egui::SidePanel::left(egui::Id::new(("col", name, room))),
                Side::Right => egui::SidePanel::right(egui::Id::new(("col", name, room))),
            };
            let shown = panel
                .resizable(true)
                .default_width(width)
                .width_range(120.0..=520.0)
                .show(ctx, |ui| self.column_contents(ui, side));
            let w = shown.response.rect.width();
            let r = &mut self.rooms[room];
            if side == Side::Left {
                r.left_width = w;
            } else {
                r.right_width = w;
            }
            return;
        }
        // unpinned: a thin strip; the column slides out over the canvas while the pointer is on it
        let strip = match side {
            Side::Left => egui::SidePanel::left(egui::Id::new(("strip", name, room))),
            Side::Right => egui::SidePanel::right(egui::Id::new(("strip", name, room))),
        };
        let shown = strip.exact_width(14.0).resizable(false).show(ctx, |ui| {
            let r = ui.max_rect();
            ui.painter().text(
                r.center_top() + vec2(0.0, 30.0),
                egui::Align2::CENTER_CENTER,
                if side == Side::Left { "▸" } else { "◂" },
                egui::FontId::proportional(14.0),
                ui.visuals().text_color(),
            );
        });
        let srect = shown.response.rect;
        let ptr = ctx.pointer_hover_pos();
        let last = if side == Side::Left {
            self.left_overlay
        } else {
            self.right_overlay
        };
        let busy = ctx.input(|i| i.pointer.any_down()) || ctx.memory(|m| m.any_popup_open());
        let was_open = last.is_some();
        let open =
            ptr.is_some_and(|p| srect.contains(p)) || (was_open && (busy || ptr.is_some_and(|p| last.unwrap().expand(12.0).contains(p))));
        if !open {
            if side == Side::Left {
                self.left_overlay = None;
            } else {
                self.right_overlay = None;
            }
            return;
        }
        let x = if side == Side::Left { srect.right() } else { srect.left() - width };
        let area = egui::Area::new(egui::Id::new(("slide", name, room)))
            .order(egui::Order::Foreground)
            .fixed_pos(pos2(x, srect.top()))
            .show(ctx, |ui| {
                egui::Frame::window(ui.style()).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                    ui.set_width(width - 12.0);
                    ui.set_height(srect.height() - 12.0);
                    self.column_contents(ui, side);
                });
            });
        let rect = area.response.rect;
        if side == Side::Left {
            self.left_overlay = Some(rect);
        } else {
            self.right_overlay = Some(rect);
        }
    }

    /// Floating panels, each in its own window.
    fn floating_panels(&mut self, ctx: &egui::Context) {
        let room = self.room;
        let list = self.rooms[room].floating.clone();
        for (k, (p, pos)) in list.iter().enumerate() {
            let mut open = true;
            let title = self.panel_title(*p);
            let w = egui::Window::new(title)
                .id(egui::Id::new(("float", *p as u8, room)))
                .default_pos(pos2(pos[0], pos[1]))
                .default_width(220.0)
                .resizable(true)
                .open(&mut open)
                .show(ctx, |ui| {
                    self.panel_header(ui, *p, true);
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .max_height(560.0)
                        .show(ui, |ui| self.panel_body(ui, *p));
                });
            if let Some(w) = w {
                let at = w.response.rect.left_top();
                if let Some(f) = self.rooms[room].floating.get_mut(k) {
                    f.1 = [at.x, at.y];
                }
            }
            if !open {
                self.layout_ops.push(LayoutOp::Hide(*p));
            }
        }
    }

    /// Apply the moves asked for this frame; a panel dropped outside the columns floats there.
    fn apply_layout_ops(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.any_released()) {
            if let Some(p) = egui::DragAndDrop::take_payload::<PanelId>(ctx) {
                if let Some(at) = ctx.pointer_interact_pos() {
                    self.layout_ops.push(LayoutOp::Float(*p, [at.x - 20.0, at.y - 10.0]));
                }
            }
        }
        let ops = std::mem::take(&mut self.layout_ops);
        let room = &mut self.rooms[self.room];
        for op in &ops {
            match *op {
                LayoutOp::Place(p, side, i) => {
                    let n = if side == Side::Left { room.left.len() } else { room.right.len() };
                    room.place(p, side, i.min(n));
                }
                LayoutOp::Float(p, pos) => room.float(p, pos),
                LayoutOp::Hide(p) => room.remove(p),
            }
        }
        if !ops.is_empty() || self.layout_dirty {
            crate::layout::save(&self.rooms, self.room);
            self.layout_dirty = false;
        }
    }

    /// Change room like the reference: keep the current room's shortcuts with it,
    /// then take the new room's layout and shortcuts.
    fn switch_room(&mut self, n: usize) {
        if n < self.rooms.len() && n != self.room {
            self.rooms[self.room].keys = Some(self.keymap.to_cfg());
            self.room = n;
            // each room keeps its own timeline zoom
            self.tl_cell = self.rooms[n].timeline_cell.clamp(TL_CELL_MIN, TL_CELL_MAX);
            self.tl_cell_shown = self.tl_cell;
            if let Some(cfg) = self.rooms[n].keys.clone() {
                self.keymap = crate::keymap::KeyMap::from_cfg(&cfg, true).0;
                crate::keymap::save(&self.keymap);
            }
            self.left_overlay = None;
            self.right_overlay = None;
            crate::layout::save(&self.rooms, self.room);
        }
    }

    fn timeline(&mut self, ctx: &egui::Context) {
        let th = THEMES[self.theme];
        let room = self.room;
        let pinned = self.rooms[room].timeline_pinned;
        let folded_unpinned = !pinned && !self.tl_open;
        let compact = folded_unpinned || self.rooms[room].timeline_height < 60.0;
        let panel = if folded_unpinned {
            egui::TopBottomPanel::bottom(egui::Id::new(("timeline-folded", room))).exact_height(32.0)
        } else {
            egui::TopBottomPanel::bottom(egui::Id::new(("timeline", room)))
                .resizable(true)
                .default_height(self.rooms[room].timeline_height.max(28.0))
                .min_height(28.0)
        };
        let shown = panel.show(ctx, |ui| {
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                let lab = if self.rooms[room].timeline_pinned { "📌" } else { "📍" };
                if ui
                    .selectable_label(self.rooms[room].timeline_pinned, lab)
                    .on_hover_text(tr(
                        "Épingler la timeline (sinon elle se replie et ressort quand la souris la touche)",
                    ))
                    .clicked()
                {
                    self.rooms[room].timeline_pinned = !self.rooms[room].timeline_pinned;
                    self.layout_dirty = true;
                }
                if ui.button("⏮").on_hover_text(tr("Première image")).clicked() {
                    self.doc.frame = 0;
                }
                if ui.button("◀").on_hover_text(tr("Image précédente")).clicked() {
                    self.step_frame(-1);
                }
                if ui.button(if self.playing { tr("⏸ Arrêter") } else { tr("▶ Lire") }).clicked() {
                    self.toggle_play();
                }
                if ui.button("▶|").on_hover_text(tr("Image suivante")).clicked() {
                    self.step_frame(1);
                }
                if ui.button("⏭").on_hover_text(tr("Dernière image")).clicked() {
                    self.doc.frame = self.doc.frames - 1;
                }
                ui.checkbox(&mut self.looping, tr("Boucle"));
                ui.separator();
                ui.label(tr("Image"));
                let mut f = self.doc.frame + 1;
                if ui.add(egui::DragValue::new(&mut f).range(1..=self.doc.frames)).changed() {
                    self.doc.frame = f - 1;
                }
                ui.label(format!("/ {}", self.doc.frames));
                ui.separator();
                ui.label(tr("Cadence"));
                ui.add(
                    egui::DragValue::new(&mut self.doc.fps)
                        .range(1.0..=120.0)
                        .speed(0.1)
                        .suffix(format!(" {}", tr("i/s"))),
                );
                ui.label(tr("Durée"));
                let mut n = self.doc.frames;
                if ui
                    .add(egui::DragValue::new(&mut n).range(1..=10000))
                    .on_hover_text(tr("Nombre d'images du projet"))
                    .changed()
                {
                    self.doc.set_length(n);
                    self.needs_full = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .selectable_label(self.tl_timecode, tr("Timecode"))
                        .on_hover_text(tr("La règle affiche le temps (secondes:images) au lieu des numéros d'image"))
                        .clicked()
                    {
                        self.tl_timecode = !self.tl_timecode;
                    }
                    ui.menu_button(tr("Lignes"), |ui| {
                        let sizes = [tr("Petites"), tr("Moyennes"), tr("Grandes")];
                        for (h, label) in crate::layout::TIMELINE_ROWS.into_iter().zip(sizes) {
                            if ui.radio((self.rooms[room].timeline_row - h).abs() < 0.5, label).clicked() {
                                self.rooms[room].timeline_row = h;
                                self.layout_dirty = true;
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui
                            .checkbox(&mut self.rooms[room].timeline_thumbs, tr("Vignettes des calques"))
                            .on_hover_text(tr("Une petite image du dessin courant de chaque calque, à côté de son nom"))
                            .changed()
                        {
                            self.layout_dirty = true;
                        }
                        ui.separator();
                        if ui.button(tr("Replier tous les calques")).clicked() {
                            self.collapse_layers(true, Some(true));
                            ui.close_menu();
                        }
                        if ui.button(tr("Déplier tous les calques")).clicked() {
                            self.collapse_layers(true, Some(false));
                            ui.close_menu();
                        }
                    })
                    .response
                    .on_hover_text(tr("Hauteur des lignes de la timeline, vignettes et calques repliés"));
                    ui.menu_button(tr("Ajuster"), |ui| {
                        if ui.button(tr("Tout le plan")).clicked() {
                            self.tl_fit = Some(TlFit::Clip);
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(self.cell_sel.is_some(), egui::Button::new(tr("Les cellules choisies")))
                            .clicked()
                        {
                            self.tl_fit = Some(TlFit::Selection);
                            ui.close_menu();
                        }
                        if ui
                            .add_enabled(self.audio.iter().any(|c| c.is_some()), egui::Button::new(tr("Le son")))
                            .clicked()
                        {
                            self.tl_fit = Some(TlFit::Sound);
                            ui.close_menu();
                        }
                        if ui.button(tr("Taille normale")).clicked() {
                            self.tl_cell = TL_CELL;
                            ui.close_menu();
                        }
                    })
                    .response
                    .on_hover_text(tr("Ajuster le zoom de la timeline"));
                    if ui.button("+").on_hover_text(tr("Zoom avant de la timeline")).clicked() {
                        self.tl_zoom(1.25);
                    }
                    ui.spacing_mut().slider_width = 80.0;
                    ui.add(
                        egui::Slider::new(&mut self.tl_cell, TL_CELL_MIN..=TL_CELL_MAX)
                            .show_value(false)
                            .logarithmic(true),
                    )
                    .on_hover_text(tr("Zoom de la timeline (aussi : Ctrl + molette)"));
                    if ui.button("−").on_hover_text(tr("Zoom arrière de la timeline")).clicked() {
                        self.tl_zoom(0.8);
                    }
                });
            });
            if compact {
                return;
            }
            ui.horizontal_wrapped(|ui| {
                let act = |ui: &mut egui::Ui, label: &str, tip: &str| ui.button(label).on_hover_text(tip).clicked();
                if act(ui, tr("+ Image"), tr("Nouvelle image vierge après l'image courante")) {
                    self.doc.add_blank_after();
                    self.needs_full = true;
                }
                if act(ui, tr("Dupliquer"), tr("Copie du dessin courant après lui")) {
                    self.doc.duplicate_after();
                    self.needs_full = true;
                }
                if act(ui, tr("Exposition +"), tr("Garder le dessin plus longtemps (du nombre choisi)")) {
                    self.doc.expose_more_n(self.expo_n);
                    self.needs_full = true;
                }
                ui.add(egui::DragValue::new(&mut self.expo_n).range(1..=999))
                    .on_hover_text(tr("Nombre d'images"));
                if act(ui, tr("Exposition −"), tr("Garder le dessin moins longtemps (du nombre choisi)")) {
                    self.doc.expose_less_n(self.expo_n);
                    self.needs_full = true;
                }
                if act(
                    ui,
                    tr("Régler l'exposition"),
                    tr("Chaque dessin des cellules choisies dure exactement ce nombre d'images"),
                ) {
                    let (l, a, b) = self.cell_range();
                    if let Some(end) = self.doc.set_exposure(l, a, b, self.expo_n) {
                        if self.cell_sel.is_some() {
                            self.cell_sel = Some((l, a.min(end), end));
                        }
                    }
                    self.needs_full = true;
                }
                if act(ui, tr("Supprimer la cellule"), tr("Retire les cellules choisies du calque")) {
                    let (l, a, b) = self.cell_range();
                    self.doc.delete_cells(l, a, b);
                    self.cell_sel = None;
                    self.needs_full = true;
                }
                ui.separator();
                if act(ui, tr("+ Calque"), tr("Nouveau calque au-dessus")) {
                    self.doc.add_layer();
                    self.needs_full = true;
                }
                if act(ui, tr("Dupliquer le calque"), tr("Copie du calque courant")) {
                    self.doc.duplicate_layer();
                    self.needs_full = true;
                }
                if act(ui, tr("Supprimer le calque"), tr("Retire le calque courant")) {
                    self.delete_layers();
                }
                if act(ui, tr("Monter"), tr("Monter le calque")) {
                    self.doc.move_layer(true);
                    self.needs_full = true;
                }
                if act(ui, tr("Descendre"), tr("Descendre le calque")) {
                    self.doc.move_layer(false);
                    self.needs_full = true;
                }
                ui.separator();
                self.timing_bar(ui);
                ui.separator();
                let cl = self.doc.current_layer;
                ui.add(egui::TextEdit::singleline(&mut self.doc.layers[cl].name).desired_width(110.0));
                let mut op = self.doc.layers[cl].opacity * 100.0;
                if ui
                    .add(egui::Slider::new(&mut op, 0.0..=100.0).text(tr("Opacité")).suffix(" %"))
                    .changed()
                {
                    self.doc.layers[cl].opacity = op / 100.0;
                    self.needs_full = true;
                }
                let cur_blend = self.doc.layers[cl].blend;
                egui::ComboBox::from_id_salt("blend")
                    .selected_text(cur_blend.label())
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for b in crate::doc::ALL_BLENDS {
                            if ui.selectable_value(&mut self.doc.layers[cl].blend, b, b.label()).changed() {
                                self.needs_full = true;
                            }
                        }
                    })
                    .response
                    .on_hover_text(tr("Mode de mélange du calque"));
                ui.menu_button(tr("Couleur"), |ui| {
                    ui.horizontal(|ui| {
                        for gi in 0..self.doc.groups.len().min(13) {
                            let g = self.doc.groups[gi].color;
                            let (r, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                            let resp = resp.on_hover_text(format!("{gi}: {}", self.doc.group_name(gi)));
                            ui.painter().rect_filled(r, 3.0, c32(g));
                            if self.doc.layers[cl].group == g {
                                ui.painter()
                                    .rect_stroke(r, 3.0, Line::new(2.0_f32, Color32::WHITE), StrokeKind::Inside);
                            }
                            if resp.clicked() {
                                // the current layer and the layers chosen with it
                                for l in self.doc.target_layers() {
                                    self.doc.layers[l].group = g;
                                }
                                ui.close_menu();
                            }
                        }
                    });
                })
                .response
                .on_hover_text(tr("Groupe de couleur du calque"));
            });
            ui.add_space(2.0);
            self.timeline_grid(ui, &th);
        });
        let rect = shown.response.rect;
        if !folded_unpinned {
            self.rooms[room].timeline_height = rect.height();
        }
        if room == self.room {
            self.rooms[room].timeline_cell = self.tl_cell;
        }
        if !self.rooms[room].timeline_pinned {
            let busy = ctx.input(|i| i.pointer.any_down()) || ctx.memory(|m| m.any_popup_open());
            let over = ctx.pointer_hover_pos().is_some_and(|p| rect.expand(10.0).contains(p));
            self.tl_open = over || (self.tl_open && busy);
        }
    }

    fn timeline_grid(&mut self, ui: &mut egui::Ui, th: &Theme) {
        self.update_peaks();
        const HEAD: f32 = 20.0;
        const NOTES: f32 = 18.0;
        /// Height of a folded layer's row.
        const FOLDED: f32 = 12.0;
        // the rows' height in this room, and room for the layers' small pictures
        let row_h = self.rooms[self.room].timeline_row.clamp(20.0, 64.0);
        let thumbs = self.rooms[self.room].timeline_thumbs;
        let thumb_h = (row_h - 6.0).floor();
        let thumb_w = if thumbs {
            (thumb_h * self.doc.w as f32 / self.doc.h.max(1) as f32).clamp(8.0, 80.0).floor()
        } else {
            0.0
        };
        let col_w = 250.0 + if thumbs { thumb_w + 6.0 } else { 0.0 };
        let area = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(area, Sense::click_and_drag());
        let painter = ui.painter_at(area);
        let nl_all = self.doc.layers.len();
        let rows: Vec<usize> = (0..nl_all).rev().filter(|&li| self.layer_in_timeline(li)).collect();
        let nl = rows.len();
        let nf = self.doc.frames;
        let cells_w = (area.width() - col_w).max(1.0);
        // the ruler, then the notes row when it is shown, then the layers
        let top = HEAD + if self.tl_notes { NOTES } else { 0.0 };
        let hover = resp.hover_pos();
        if resp.hovered() {
            // Ctrl + wheel (or a pinch) zooms; the wheel alone scrolls
            let z = ui.input(|i| i.zoom_delta());
            if (z - 1.0).abs() > 1e-4 {
                self.tl_zoom(z);
            }
            let d = ui.input(|i| i.smooth_scroll_delta);
            let shift = ui.input(|i| i.modifiers.shift);
            if shift {
                self.tl_scroll.x -= d.y + d.x;
            } else {
                self.tl_scroll.x -= d.x;
                self.tl_scroll.y -= d.y;
            }
        }
        // "Fit": the whole shot, the chosen cells or the sound across the width
        if let Some(fit) = self.tl_fit.take() {
            let (first, count) = match fit {
                TlFit::Clip => (0, nf),
                TlFit::Selection => match self.cell_sel {
                    Some((_, a, b)) => (a.min(nf - 1), b.saturating_sub(a) + 1),
                    None => (0, nf),
                },
                TlFit::Sound => {
                    // from where the first sound starts to where the last one ends
                    let fps = self.doc.fps.max(1.0);
                    let spans: Vec<(f32, f32)> = self
                        .audio
                        .iter()
                        .zip(&self.doc.sounds)
                        .filter_map(|(c, t)| c.as_ref().map(|c| (t.offset.max(0.0), t.offset + c.duration())))
                        .collect();
                    if spans.is_empty() {
                        (0, nf)
                    } else {
                        let a = spans.iter().fold(f32::MAX, |m, s| m.min(s.0));
                        let b = spans.iter().fold(0.0f32, |m, s| m.max(s.1)).max(a);
                        let first = ((a * fps).floor() as usize).min(nf - 1);
                        (first, (((b - a) * fps).ceil() as usize).max(1))
                    }
                }
            };
            self.tl_cell = (cells_w / count.max(1) as f32).clamp(TL_CELL_MIN, TL_CELL_MAX);
            self.tl_cell_shown = self.tl_cell;
            self.tl_scroll.x = first as f32 * self.tl_cell;
        }
        let cell = self.tl_cell.clamp(TL_CELL_MIN, TL_CELL_MAX);
        if (cell - self.tl_cell_shown).abs() > 1e-4 {
            // the zoom changed: the frame under the pointer (else the current frame) stays put
            let old = self.tl_cell_shown.max(0.1);
            let anchor = match hover {
                Some(p) if p.x >= area.left() + col_w => p.x - area.left() - col_w,
                _ => (self.doc.frame as f32 + 0.5) * old - self.tl_scroll.x,
            }
            .clamp(0.0, cells_w);
            let at = (anchor + self.tl_scroll.x) / old;
            self.tl_scroll.x = at * cell - anchor;
            self.tl_cell_shown = cell;
        }
        let max_x = (nf as f32 * cell - cells_w + cell).max(0.0);
        let sound_rows = self.doc.sounds.len();
        // each row's height and its top (from the first row): the layers, then the sound tracks
        let heights: Vec<f32> = rows
            .iter()
            .map(|&li| if self.doc.layers[li].collapsed { FOLDED } else { row_h })
            .chain(std::iter::repeat_n(row_h, sound_rows))
            .collect();
        let mut tops = Vec::with_capacity(heights.len());
        let mut rows_h = 0.0;
        for h in &heights {
            tops.push(rows_h);
            rows_h += h;
        }
        let max_y = (rows_h - (area.height() - top)).max(0.0);
        // bring the current frame into view when it changes (scrolling away from it is allowed)
        if self.doc.frame != self.tl_frame_seen {
            self.tl_frame_seen = self.doc.frame;
            let fx = self.doc.frame as f32 * cell;
            if fx < self.tl_scroll.x {
                self.tl_scroll.x = fx;
            } else if fx + cell > self.tl_scroll.x + cells_w {
                self.tl_scroll.x = fx + cell - cells_w;
            }
        }
        self.tl_scroll.x = self.tl_scroll.x.clamp(0.0, max_x);
        self.tl_scroll.y = self.tl_scroll.y.clamp(0.0, max_y);
        let sx = self.tl_scroll.x;
        let sy = self.tl_scroll.y;

        let dark = th.doc_bg;
        painter.rect_filled(area, 0.0, dark);
        let text = th.text;
        let font = egui::FontId::proportional(12.0);
        let small = egui::FontId::proportional(10.0);
        let cell_x = |f: usize| area.left() + col_w + f as f32 * cell - sx;
        let row_y = |r: usize| area.top() + top + tops[r] - sy;
        // the row at a height of the panel
        let row_at = |y: f32| {
            let rel = y - area.top() - top + sy;
            tops.iter()
                .rposition(|t| *t <= rel)
                .filter(|&r| rel >= 0.0 && rel < tops[r] + heights[r])
        };
        let tiny = egui::FontId::proportional(9.0);
        // the layers' small pictures, for the rows in view
        let mut thumb_tex: std::collections::HashMap<usize, egui::TextureId> = Default::default();
        self.thumbs.retain(|l, _| thumbs && *l < nl_all);
        if thumbs {
            let mut budget = 2;
            for (r, &li) in rows.iter().enumerate() {
                let y = row_y(r);
                if self.doc.layers[li].collapsed || y + row_h < area.top() || y > area.bottom() {
                    continue;
                }
                if let Some(t) = self.layer_thumb(ui.ctx(), li, thumb_w as usize, thumb_h as usize, &mut budget) {
                    thumb_tex.insert(li, t);
                }
            }
        }
        // frames between two numbers of the ruler and two lines of the grid
        let label_w = if self.tl_timecode { 34.0 } else { 22.0 };
        let step = [1usize, 2, 5, 10, 25, 50, 100, 250, 500, 1000]
            .into_iter()
            .find(|n| *n as f32 * cell >= label_w)
            .unwrap_or(1000);
        let line_step = if cell >= 8.0 { 1 } else { step };

        // rows
        let grid_clip = egui::Rect::from_min_max(pos2(area.left() + col_w, area.top() + top), area.max);
        let col_clip = egui::Rect::from_min_max(pos2(area.left(), area.top() + top), pos2(area.left() + col_w, area.bottom()));
        for (r, &li) in rows.iter().enumerate() {
            let layer = &self.doc.layers[li];
            let y = row_y(r);
            let h = heights[r];
            let folded = layer.collapsed;
            let cur = li == self.doc.current_layer;
            let rowrect = egui::Rect::from_min_size(pos2(area.left(), y), vec2(col_w, h - 1.0));
            let p = painter.with_clip_rect(col_clip);
            let row_col = if cur {
                th.selected
            } else if layer.selected {
                lerp_color(th.panel, th.selected, 0.5)
            } else {
                th.panel
            };
            p.rect_filled(rowrect, 0.0, row_col);
            p.rect_filled(
                egui::Rect::from_min_size(rowrect.min, vec2(3.0, rowrect.height())),
                0.0,
                c32(layer.group),
            );
            let cell_col = lerp_color(th.cell, c32(layer.group), 0.55);
            let icon =
                |i: usize| egui::Rect::from_min_size(pos2(area.left() + 4.0 + i as f32 * 24.0, y + (h - 20.0) / 2.0), vec2(20.0, 20.0));
            // a folded layer shows no icons
            let icons = [(0, layer.visible), (1, layer.locked), (2, layer.onion), (3, layer.stencil)];
            for (i, on) in icons.into_iter().filter(|_| !folded) {
                let ir = icon(i);
                p.rect_filled(ir, 3.0, if on { th.widget } else { th.panel.gamma_multiply(0.8) });
                let col = if on { text } else { text.gamma_multiply(0.3) };
                let c = ir.center();
                match i {
                    0 => {
                        // eye
                        let pts: Vec<Pos2> = (0..=16)
                            .map(|k| {
                                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                                pos2(c.x + 7.0 * a.cos(), c.y + 4.0 * a.sin())
                            })
                            .collect();
                        p.add(egui::Shape::line(pts, Line::new(1.3_f32, col)));
                        p.circle_filled(c, 2.2, col);
                    }
                    1 => {
                        // padlock
                        let body = egui::Rect::from_center_size(c + vec2(0.0, 2.5), vec2(10.0, 7.0));
                        p.rect_filled(body, 1.0, col);
                        let pts: Vec<Pos2> = (0..=10)
                            .map(|k| {
                                let a = std::f32::consts::PI * (1.0 + k as f32 / 10.0);
                                pos2(c.x + 3.5 * a.cos(), c.y - 1.0 + 4.0 * a.sin())
                            })
                            .collect();
                        p.add(egui::Shape::line(pts, Line::new(1.5_f32, col)));
                    }
                    3 => {
                        // stencil: a cut-out square
                        p.rect_stroke(
                            egui::Rect::from_center_size(c, vec2(12.0, 12.0)),
                            1.0,
                            Line::new(1.5_f32, col),
                            StrokeKind::Inside,
                        );
                        p.circle_filled(c, 3.0, col);
                    }
                    _ => {
                        // light table: two overlapping discs
                        p.circle_filled(
                            c + vec2(-2.5, 0.0),
                            5.0,
                            Color32::from_rgb(0, 192, 0).gamma_multiply(if on { 0.9 } else { 0.3 }),
                        );
                        p.circle_filled(
                            c + vec2(2.5, 0.0),
                            5.0,
                            Color32::from_rgb(255, 128, 0).gamma_multiply(if on { 0.9 } else { 0.3 }),
                        );
                    }
                }
            }
            // the triangle that folds or unfolds the layer, at the right of the name column
            let tc = pos2(area.left() + col_w - 9.0, y + h / 2.0);
            let triangle = if folded {
                vec![tc + vec2(-2.5, -4.0), tc + vec2(3.0, 0.0), tc + vec2(-2.5, 4.0)]
            } else {
                vec![tc + vec2(-4.0, -2.5), tc + vec2(4.0, -2.5), tc + vec2(0.0, 3.0)]
            };
            p.add(egui::Shape::convex_polygon(triangle, text.gamma_multiply(0.6), Line::NONE));
            if folded {
                p.text(
                    pos2(area.left() + 10.0, y + h / 2.0),
                    egui::Align2::LEFT_CENTER,
                    &layer.name,
                    tiny.clone(),
                    text.gamma_multiply(0.8),
                );
            } else {
                let mut name_x = area.left() + 104.0;
                if thumbs {
                    let tr = egui::Rect::from_min_size(pos2(name_x, y + 3.0), vec2(thumb_w, thumb_h));
                    p.rect_filled(tr, 1.0, c32(self.doc.paper));
                    if let Some(t) = thumb_tex.get(&li) {
                        let uv = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                        p.image(*t, tr, uv, Color32::WHITE);
                    }
                    p.rect_stroke(tr, 1.0, Line::new(1.0_f32, th.border), StrokeKind::Inside);
                    name_x += thumb_w + 6.0;
                }
                p.text(
                    pos2(name_x, y + h / 2.0),
                    egui::Align2::LEFT_CENTER,
                    &layer.name,
                    font.clone(),
                    text,
                );
                p.text(
                    pos2(area.left() + col_w - 20.0, y + h / 2.0),
                    egui::Align2::RIGHT_CENTER,
                    format!("{:.0}%", layer.opacity * 100.0),
                    small.clone(),
                    text.gamma_multiply(0.7),
                );
            }
            // cells
            let g = painter.with_clip_rect(grid_clip);
            g.rect_filled(
                egui::Rect::from_min_max(pos2(area.left() + col_w, y), pos2(area.right(), y + h - 1.0)),
                0.0,
                th.panel.gamma_multiply(0.85),
            );
            let first = (sx / cell).floor() as usize;
            let last = (((sx + cells_w) / cell).ceil() as usize + 1).min(nf);
            let mut has_prev = self.doc.key_index(li, first).is_some();
            let numbers = self.doc.drawing_numbers(li);
            let lend = layer.end.filter(|e| *e + 1 < nf);
            // zoomed out, the cells touch each other and lose their details
            let gap = if cell >= 8.0 { 1.0 } else { 0.0 };
            for f in first..last {
                let x = cell_x(f);
                let c = egui::Rect::from_min_size(pos2(x + gap, y + 3.0), vec2(cell - 2.0 * gap, h - 7.0));
                let outside = lend.is_some_and(|e| f > e);
                match layer.cels.get(f).copied().flatten() {
                    Some(id) => {
                        has_prev = true;
                        g.rect_filled(c, 2.0 * gap, if outside { cell_col.gamma_multiply(0.35) } else { cell_col });
                        g.rect_filled(
                            egui::Rect::from_min_size(c.min, vec2((cell * 0.2).clamp(1.0, 3.0), c.height())),
                            0.0,
                            th.text.gamma_multiply(0.6),
                        );
                        if cell >= 16.0 && !folded {
                            let label: String = match layer.names.get(&id) {
                                Some(n) => n.chars().take(3).collect(),
                                None => numbers.get(&id).map(|n| n.to_string()).unwrap_or_default(),
                            };
                            g.text(
                                c.center() + vec2(1.5, 0.0),
                                egui::Align2::CENTER_CENTER,
                                label,
                                tiny.clone(),
                                th.text.gamma_multiply(0.85),
                            );
                        }
                        // image mark: a corner in the mark's colour
                        if let Some(m) = layer.marks.get(&id).filter(|_| !folded) {
                            let k = (cell * 0.45).clamp(3.0, 9.0);
                            let col = c32(crate::doc::GROUPS[(*m as usize).min(12)]);
                            g.add(egui::Shape::convex_polygon(
                                vec![c.right_top() - vec2(k, 0.0), c.right_top(), c.right_top() + vec2(0.0, k)],
                                col,
                                Line::new(1.0_f32, Color32::from_black_alpha(160)),
                            ));
                            g.rect_filled(
                                egui::Rect::from_min_max(pos2(c.left(), c.bottom() - 3.0), c.right_bottom()),
                                0.0,
                                col,
                            );
                        }
                    }
                    None if has_prev && !outside => {
                        g.rect_filled(c.shrink2(vec2(-gap, (h - 7.0) * 0.3)), 0.0, cell_col.gamma_multiply(0.7));
                    }
                    None => {}
                }
                // what the post behaviour shows after the layer's end
                if outside && layer.post != crate::doc::Behavior::None {
                    g.line_segment(
                        [pos2(x + 2.0 * gap, y + h / 2.0), pos2(x + cell - 2.0 * gap, y + h / 2.0)],
                        Line::new(1.0_f32, cell_col.gamma_multiply(0.8)),
                    );
                }
                if (f + 1) % line_step == 0 {
                    g.line_segment(
                        [pos2(x + cell, y), pos2(x + cell, y + h - 1.0)],
                        Line::new(1.0_f32, th.border.gamma_multiply(0.6)),
                    );
                }
            }
            if let Some(e) = lend {
                let x = cell_x(e + 1);
                g.line_segment(
                    [pos2(x, y + 1.0), pos2(x, y + h - 2.0)],
                    Line::new(2.0_f32, Color32::from_rgb(240, 200, 60)),
                );
            }
            if let Some((sl, a, b)) = self.cell_sel {
                if sl == li {
                    let r = egui::Rect::from_min_max(pos2(cell_x(a), y + 1.0), pos2(cell_x(b) + cell, y + h - 2.0));
                    g.rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(60, 140, 255, 50));
                    g.rect_stroke(r, 2.0, Line::new(1.5_f32, Color32::from_rgb(60, 140, 255)), StrokeKind::Inside);
                }
            }
        }
        // sound track rows, under the layers
        for (k, track) in self.doc.sounds.iter().enumerate() {
            let y = row_y(nl + k);
            let h = heights[nl + k];
            let p = painter.with_clip_rect(col_clip);
            let rowrect = egui::Rect::from_min_size(pos2(area.left(), y), vec2(col_w, h - 1.0));
            p.rect_filled(rowrect, 0.0, th.panel);
            let track_col = sound_color(track.color, &self.doc.groups);
            p.rect_filled(egui::Rect::from_min_size(rowrect.min, vec2(3.0, rowrect.height())), 0.0, track_col);
            let name = track.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let muted = track.mute;
            let missing = self.audio.get(k).is_none_or(|c| c.is_none());
            p.text(
                pos2(area.left() + 10.0, y + h / 2.0),
                egui::Align2::LEFT_CENTER,
                format!("{} {name}", if track.locked { "🔒" } else { "♪" }),
                font.clone(),
                if missing {
                    Color32::from_rgb(255, 190, 80)
                } else if muted {
                    text.gamma_multiply(0.4)
                } else {
                    text
                },
            );
            let g = painter.with_clip_rect(grid_clip);
            g.rect_filled(
                egui::Rect::from_min_max(pos2(area.left() + col_w, y), pos2(area.right(), y + h - 1.0)),
                0.0,
                th.panel.gamma_multiply(0.7),
            );
            let first = (sx / cell).floor() as usize;
            let last = (((sx + cells_w) / cell).ceil() as usize + 1).min(nf);
            let wave = if muted { Color32::from_gray(110) } else { track_col };
            for f in first..last {
                let v = self.peaks.get(k).and_then(|p| p.get(f)).copied().unwrap_or(0.0);
                if v <= 0.0 {
                    continue;
                }
                let x = cell_x(f);
                let hh = (h - 6.0) * v;
                g.rect_filled(
                    egui::Rect::from_center_size(pos2(x + cell / 2.0, y + h / 2.0), vec2((cell - 2.0).max(1.0), hh.max(1.0))),
                    1.0,
                    wave,
                );
            }
        }
        // header
        let head = egui::Rect::from_min_size(area.min, vec2(area.width(), HEAD));
        painter.rect_filled(head, 0.0, th.widget);
        let hp = painter.with_clip_rect(egui::Rect::from_min_max(
            pos2(area.left() + col_w, area.top()),
            pos2(area.right(), area.top() + HEAD),
        ));
        let first = (sx / cell).floor() as usize;
        let last = (((sx + cells_w) / cell).ceil() as usize + 1).min(nf);
        for f in first..last {
            let x = cell_x(f);
            // frame numbers count from 1 (5, 10, 15…), times from 0
            let shown = if self.tl_timecode {
                f % step == 0
            } else {
                (f + 1) % step == 0 || f == 0
            };
            if shown {
                hp.text(
                    pos2(x + cell / 2.0, area.top() + HEAD / 2.0),
                    egui::Align2::CENTER_CENTER,
                    self.frame_label(f),
                    small.clone(),
                    text,
                );
            }
        }
        // notes row: each note is written from its frame up to the next note
        if self.tl_notes {
            let y = area.top() + HEAD;
            painter.rect_filled(
                egui::Rect::from_min_size(pos2(area.left(), y), vec2(area.width(), NOTES)),
                0.0,
                th.panel,
            );
            painter.text(
                pos2(area.left() + 6.0, y + NOTES / 2.0),
                egui::Align2::LEFT_CENTER,
                tr("Notes"),
                small.clone(),
                text.gamma_multiply(0.7),
            );
            let note_col = Color32::from_rgb(250, 220, 120);
            let mut notes = self.doc.notes.range(..nf).peekable();
            while let Some((f, note)) = notes.next() {
                let x0 = cell_x(*f);
                let x1 = notes.peek().map(|n| cell_x(*n.0) - 2.0).unwrap_or(area.right());
                if x1 < area.left() + col_w || x0 > area.right() {
                    continue;
                }
                let clip = egui::Rect::from_min_max(pos2(x0.max(area.left() + col_w), y), pos2(x1.max(x0 + 4.0), y + NOTES));
                let np = painter.with_clip_rect(clip.intersect(area));
                np.rect_filled(egui::Rect::from_min_size(pos2(x0, y + 1.0), vec2(3.0, NOTES - 2.0)), 0.0, note_col);
                np.text(
                    pos2(x0 + 6.0, y + NOTES / 2.0),
                    egui::Align2::LEFT_CENTER,
                    note.lines().next().unwrap_or(""),
                    small.clone(),
                    note_col,
                );
            }
        }
        painter.text(
            pos2(area.left() + 6.0, area.top() + HEAD / 2.0),
            egui::Align2::LEFT_CENTER,
            tr("Calques"),
            font.clone(),
            text,
        );
        // play range (mark in / out): frames outside it are darkened
        if self.doc.mark_in.is_some() || self.doc.mark_out.is_some() {
            let (lo, hi) = self.doc.play_range();
            let shade = Color32::from_black_alpha(110);
            let left = cell_x(lo);
            let right = cell_x(hi) + cell;
            hp.rect_filled(
                egui::Rect::from_min_max(pos2(area.left() + col_w, area.top()), pos2(left, area.top() + HEAD)),
                0.0,
                shade,
            );
            hp.rect_filled(
                egui::Rect::from_min_max(pos2(right, area.top()), pos2(area.right(), area.top() + HEAD)),
                0.0,
                shade,
            );
            let mk = Color32::from_rgb(80, 200, 120);
            if self.doc.mark_in.is_some() {
                hp.line_segment(
                    [pos2(left + 1.0, area.top()), pos2(left + 1.0, area.top() + HEAD)],
                    Line::new(3.0_f32, mk),
                );
            }
            if self.doc.mark_out.is_some() {
                hp.line_segment(
                    [pos2(right - 1.0, area.top()), pos2(right - 1.0, area.top() + HEAD)],
                    Line::new(3.0_f32, mk),
                );
            }
        }
        for &b in &self.doc.bookmarks {
            let x = cell_x(b) + cell / 2.0;
            let t = area.top() + 1.0;
            hp.add(egui::Shape::convex_polygon(
                vec![pos2(x - 5.0, t), pos2(x + 5.0, t), pos2(x, t + 7.0)],
                Color32::from_rgb(255, 200, 40),
                Line::NONE,
            ));
        }
        // current frame
        let x = cell_x(self.doc.frame);
        if x + cell > area.left() + col_w {
            let full = painter.with_clip_rect(egui::Rect::from_min_max(pos2(area.left() + col_w, area.top()), area.max));
            full.rect_stroke(
                egui::Rect::from_min_max(pos2(x, area.top()), pos2(x + cell, area.bottom())),
                0.0,
                Line::new(2.0_f32, Color32::from_rgb(230, 70, 60)),
                StrokeKind::Inside,
            );
            full.rect_filled(
                egui::Rect::from_min_max(pos2(x, area.top()), pos2(x + cell, area.top() + HEAD)),
                0.0,
                Color32::from_rgb(230, 70, 60),
            );
            full.text(
                pos2(x + cell / 2.0, area.top() + HEAD / 2.0),
                egui::Align2::CENTER_CENTER,
                self.frame_label(self.doc.frame),
                small,
                Color32::WHITE,
            );
        }

        // interaction
        self.tl_drag = resp.dragged();
        let frame_at = |x: f32| (((x - area.left() - col_w + sx) / cell).floor().max(0.0) as usize).min(nf - 1);
        if resp.secondary_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.tl_menu_frame = frame_at(p.x);
                // a right click on a layer's row makes it the current layer
                if p.y >= area.top() + top {
                    if let Some(r) = row_at(p.y).filter(|r| *r < nl) {
                        self.doc.focus_layer(rows[r]);
                    }
                }
            }
        }
        // a double click in the notes row writes a note there; a note shows whole under the pointer
        if self.tl_notes {
            let in_notes = |p: Pos2| p.x >= area.left() + col_w && p.y >= area.top() + HEAD && p.y < area.top() + top;
            if let Some(p) = hover.filter(|p| in_notes(*p)) {
                let f = frame_at(p.x);
                if resp.double_clicked() {
                    self.edit_note(f);
                } else if let Some((_, note)) = self.doc.notes.range(..=f).next_back() {
                    let note = note.clone();
                    resp.clone().on_hover_text_at_pointer(note);
                }
            }
        }
        resp.context_menu(|ui| {
            let f = self.tl_menu_frame;
            ui.label(trf("Image {}", &[&(f + 1)]));
            ui.menu_button(tr("Marque de l'image"), |ui| {
                let l = self.doc.current_layer;
                self.mark_menu(ui, l, f);
            });
            if ui.button(tr("Note ici…")).clicked() {
                self.edit_note(f);
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Signet ici (ajouter / retirer)")).clicked() {
                self.doc.toggle_bookmark(f);
                ui.close_menu();
            }
            if ui.button(tr("Début de lecture ici")).clicked() {
                self.doc.mark_in = Some(f);
                ui.close_menu();
            }
            if ui.button(tr("Fin de lecture ici")).clicked() {
                self.doc.mark_out = Some(f);
                ui.close_menu();
            }
            if ui.button(tr("Effacer début et fin de lecture")).clicked() {
                self.doc.mark_in = None;
                self.doc.mark_out = None;
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Fin du calque ici")).clicked() {
                self.doc.push_structure_undo();
                let l = self.doc.current_layer;
                self.doc.layers[l].end = Some(f);
                self.needs_full = true;
                ui.close_menu();
            }
            if ui.button(tr("Début du calque ici")).clicked() {
                self.trim_layer_start(f);
                ui.close_menu();
            }
            if ui.button(tr("Scinder le calque ici")).clicked() {
                self.split_layer(f);
                ui.close_menu();
            }
        });
        if let Some(p) = resp.interact_pointer_pos() {
            let pressed = (resp.clicked() || resp.drag_started() || resp.dragged()) && !resp.secondary_clicked();
            let shift = ui.input(|i| i.modifiers.shift);
            if pressed && shift && p.y >= area.top() + top && p.x >= area.left() + col_w {
                // shift + click / drag: select cells from the current frame
                let frame = (((p.x - area.left() - col_w + sx) / cell).floor().max(0.0) as usize).min(nf - 1);
                if let Some(r) = row_at(p.y).filter(|r| *r < nl) {
                    let li = rows[r];
                    if self.cell_sel.map(|s| s.0) != Some(li) {
                        self.cell_anchor = if li == self.doc.current_layer { self.doc.frame } else { frame };
                    }
                    self.doc.focus_layer(li);
                    let a = self.cell_anchor.min(frame);
                    let b = self.cell_anchor.max(frame);
                    self.cell_sel = Some((li, a, b));
                }
            } else if pressed {
                if resp.clicked() || resp.drag_started() {
                    self.cell_sel = None;
                }
                let in_cells = p.x >= area.left() + col_w;
                let frame = (((p.x - area.left() - col_w + sx) / cell).floor().max(0.0) as usize).min(nf - 1);
                if p.y < area.top() + top {
                    if in_cells {
                        self.doc.frame = frame;
                    }
                } else {
                    if let Some(r) = row_at(p.y).filter(|r| *r < nl) {
                        let li = rows[r];
                        if in_cells {
                            self.doc.focus_layer(li);
                            self.doc.frame = frame;
                        } else if resp.clicked() {
                            let ix = ((p.x - area.left() - 4.0) / 24.0).floor() as i32;
                            let iy = p.y - row_y(r) - (heights[r] - 20.0) / 2.0;
                            let folded = self.doc.layers[li].collapsed;
                            let (cmd, shift) = ui.input(|i| (i.modifiers.command || i.modifiers.ctrl, i.modifiers.shift));
                            if p.x >= area.left() + col_w - 18.0 {
                                // the triangle folds or unfolds the layer, and the layers chosen with it
                                let targets = self.doc.target_layers();
                                let group = if targets.contains(&li) { targets } else { vec![li] };
                                for i in group {
                                    self.doc.layers[i].collapsed = !folded;
                                }
                                self.doc.dirty = true;
                            } else if !folded && (0..4).contains(&ix) && (0.0..20.0).contains(&iy) {
                                // an icon of one of the chosen layers changes them all
                                let targets = self.doc.target_layers();
                                let group = if targets.contains(&li) { targets } else { vec![li] };
                                let l = &self.doc.layers[li];
                                let on = match ix {
                                    0 => !l.visible,
                                    1 => !l.locked,
                                    2 => !l.onion,
                                    _ => !l.stencil,
                                };
                                for i in group {
                                    let l = &mut self.doc.layers[i];
                                    match ix {
                                        0 => l.visible = on,
                                        1 => l.locked = on,
                                        2 => l.onion = on,
                                        _ => l.stencil = on,
                                    }
                                }
                                self.needs_full = true;
                            } else if cmd {
                                // Ctrl / Cmd + click: add the layer to the chosen ones, or take it out
                                if li != self.doc.current_layer {
                                    let l = &mut self.doc.layers[li];
                                    l.selected = !l.selected;
                                }
                            } else if shift {
                                // Shift + click: every layer from the current one to this one
                                let cur = self.doc.current_layer;
                                for l in &mut self.doc.layers[li.min(cur)..=li.max(cur)] {
                                    l.selected = true;
                                }
                            } else {
                                self.doc.select_layers(false);
                                self.doc.current_layer = li;
                            }
                        }
                    }
                }
            }
        }
    }

    /// The small picture of a layer's current image for the timeline. It is made again only when
    /// the image changed: at most `budget` pictures per screen refresh, and none while playing.
    fn layer_thumb(&mut self, ctx: &egui::Context, li: usize, tw: usize, th: usize, budget: &mut usize) -> Option<egui::TextureId> {
        let id = self.doc.shown_drawing(li, self.doc.frame);
        let paint = if li == self.doc.current_layer { self.paint_stamp } else { 0 };
        let key: ThumbKey = (id, self.full_stamp, paint, tw, th);
        let made = self.thumbs.get(&li).map(|t| t.0 == key);
        if made.is_none() || (made == Some(false) && *budget > 0 && !self.playing) {
            *budget = budget.saturating_sub(1);
            let px = match id {
                Some(id) => thumb_pixels(&self.doc.drawings[id].px, self.doc.w, self.doc.h, tw, th, self.doc.paper),
                None => [self.doc.paper[0], self.doc.paper[1], self.doc.paper[2], 255].repeat(tw * th),
            };
            let img = egui::ColorImage::from_rgba_unmultiplied([tw, th], &px);
            match self.thumbs.get_mut(&li) {
                Some(t) => {
                    t.1.set(img, egui::TextureOptions::LINEAR);
                    t.0 = key;
                }
                None => {
                    let t = ctx.load_texture(format!("layer-thumb-{li}"), img, egui::TextureOptions::LINEAR);
                    self.thumbs.insert(li, (key, t));
                }
            }
        } else if made == Some(false) {
            // made on a next refresh
            ctx.request_repaint();
        }
        self.thumbs.get(&li).map(|t| t.1.id())
    }

    fn canvas(&mut self, ctx: &egui::Context) {
        let th = THEMES[self.theme];
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(th.doc_bg))
            .show(ctx, |ui| {
                let view = ui.max_rect();
                let resp = ui.allocate_rect(view, Sense::click_and_drag());
                let (w, h) = if self.viewing_camera() {
                    (self.doc.camera.width.max(1) as f32, self.doc.camera.height.max(1) as f32)
                } else {
                    (self.doc.w as f32, self.doc.h as f32)
                };
                if self.fit_pending && view.width() > 10.0 {
                    self.zoom = ((view.width() - 40.0) / w).min((view.height() - 40.0) / h).max(0.02);
                    self.pan = Vec2::ZERO;
                    self.fit_pending = false;
                }
                // zoom with the wheel around the pointer
                if resp.hovered() {
                    let (scroll, pinch, hover) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta(), i.pointer.hover_pos()));
                    let factor = pinch * (scroll * 0.0025).exp();
                    if (factor - 1.0).abs() > 1e-4 {
                        if let Some(hp) = hover {
                            let old = self.zoom;
                            self.zoom = (self.zoom * factor).clamp(0.02, 32.0);
                            let c = view.center() + self.pan;
                            let rel = hp - c;
                            self.pan += rel - rel * (self.zoom / old);
                        }
                    }
                }
                let space = ui.input(|i| i.key_down(Key::Space));
                let middle = ui.input(|i| i.pointer.middle_down());
                if resp.dragged() && (space || middle || resp.dragged_by(egui::PointerButton::Middle)) {
                    self.pan += resp.drag_delta();
                }
                let center = view.center() + self.pan;
                let zoom = self.zoom;
                // view rotation and mirror around the view centre
                let (vc, vs) = (self.view_rot.to_radians().cos(), self.view_rot.to_radians().sin());
                let flip = if self.view_flip { -1.0 } else { 1.0 };
                let (cw, ch) = (w, h);
                let to_screen = move |p: (f32, f32)| {
                    let dx = (p.0 - cw / 2.0) * zoom * flip;
                    let dy = (p.1 - ch / 2.0) * zoom;
                    pos2(center.x + dx * vc - dy * vs, center.y + dx * vs + dy * vc)
                };
                let to_px = move |p: Pos2| {
                    let (dx, dy) = (p.x - center.x, p.y - center.y);
                    let ux = dx * vc + dy * vs;
                    let uy = -dx * vs + dy * vc;
                    (ux / zoom * flip + cw / 2.0, uy / zoom + ch / 2.0)
                };
                let corners = [to_screen((0.0, 0.0)), to_screen((w, 0.0)), to_screen((w, h)), to_screen((0.0, h))];
                let quad = |painter: &egui::Painter, tex: egui::TextureId| {
                    let mut mesh = egui::Mesh::with_texture(tex);
                    let uv = [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)];
                    for k in 0..4 {
                        mesh.vertices.push(egui::epaint::Vertex {
                            pos: corners[k],
                            uv: uv[k],
                            color: Color32::WHITE,
                        });
                    }
                    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
                    painter.add(egui::Shape::mesh(mesh));
                };

                // paint the picture
                self.refresh(ctx);
                let painter = ui.painter_at(view);
                painter.add(egui::Shape::convex_polygon(
                    corners.iter().map(|p| *p + vec2(4.0, 5.0)).collect(),
                    Color32::from_black_alpha(80),
                    Line::NONE,
                ));
                if let Some(t) = &self.tex {
                    quad(&painter, t.id());
                }
                let mut border = corners.to_vec();
                border.push(corners[0]);
                painter.add(egui::Shape::line(border, Line::new(1.0_f32, Color32::from_black_alpha(160))));
                // guides: grid, thirds, centre
                let guide = Line::new(1.0_f32, Color32::from_rgba_unmultiplied(40, 140, 255, 140));
                let gp = painter.with_clip_rect(view);
                if self.show_grid && self.grid_size * self.zoom >= 4.0 {
                    let step = self.grid_size;
                    let mut gx = step;
                    while gx < w {
                        gp.line_segment([to_screen((gx, 0.0)), to_screen((gx, h))], guide);
                        gx += step;
                    }
                    let mut gy = step;
                    while gy < h {
                        gp.line_segment([to_screen((0.0, gy)), to_screen((w, gy))], guide);
                        gy += step;
                    }
                }
                if self.show_thirds {
                    let t = Line::new(1.5_f32, Color32::from_rgba_unmultiplied(255, 90, 160, 170));
                    for k in 1..3 {
                        let (x, y) = (w * k as f32 / 3.0, h * k as f32 / 3.0);
                        gp.line_segment([to_screen((x, 0.0)), to_screen((x, h))], t);
                        gp.line_segment([to_screen((0.0, y)), to_screen((w, y))], t);
                    }
                }
                if self.show_center {
                    let t = Line::new(1.5_f32, Color32::from_rgba_unmultiplied(255, 200, 40, 200));
                    let c = (w / 2.0, h / 2.0);
                    let d = 30.0 / zoom;
                    gp.line_segment([to_screen((c.0 - d, c.1)), to_screen((c.0 + d, c.1))], t);
                    gp.line_segment([to_screen((c.0, c.1 - d)), to_screen((c.0, c.1 + d))], t);
                    // title-safe area (90 %)
                    let (x0, y0, x1, y1) = (w * 0.05, h * 0.05, w * 0.95, h * 0.95);
                    let safe = vec![
                        to_screen((x0, y0)),
                        to_screen((x1, y0)),
                        to_screen((x1, y1)),
                        to_screen((x0, y1)),
                        to_screen((x0, y0)),
                    ];
                    gp.add(egui::Shape::line(
                        safe,
                        Line::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 200, 40, 120)),
                    ));
                }
                if self.view_rot != 0.0 || self.view_flip {
                    let txt = format!("{}{:.0}°{}", tr("Vue : "), self.view_rot, if self.view_flip { " ↔" } else { "" });
                    painter.text(
                        view.right_top() + vec2(-10.0, 10.0),
                        egui::Align2::RIGHT_TOP,
                        txt,
                        egui::FontId::proportional(13.0),
                        Color32::from_rgb(255, 210, 60),
                    );
                }

                let hover = resp.hover_pos();
                self.cursor_px = hover.map(to_px);
                let panning = space || middle;
                let primary = ui.input(|i| i.pointer.primary_down());
                let pos = ui.input(|i| i.pointer.interact_pos());
                // pen pressure: from a tablet or touch screen when the system reports it,
                // otherwise simulated from the speed (if asked), otherwise full
                let (force, speed) = ui.input(|i| {
                    let mut f = None;
                    for e in &i.events {
                        if let egui::Event::Touch { force: Some(fc), .. } = e {
                            f = Some(*fc);
                        }
                    }
                    (f, i.pointer.velocity().length() / self.zoom.max(0.01))
                });
                // a pen the system reports itself (macOS) comes first
                self.pen = crate::tablet::sample();
                self.pressure = match (self.pen, force) {
                    (Some(p), _) => p.pressure,
                    (None, Some(f)) => f.clamp(0.0, 1.0),
                    (None, None) if self.sim_pressure => (1.0 - speed / 4000.0).clamp(0.25, 1.0) * 0.4 + self.pressure * 0.6,
                    (None, None) => 1.0,
                };

                // ---- selection, gradient and floating piece ----
                let special = matches!(self.tool, Tool::Select | Tool::Gradient | Tool::Camera | Tool::Position) || self.viewing_camera();
                // position tool: drag the whole drawing of the current layer
                if self.tool == Tool::Position && !self.viewing_camera() && !panning && !self.playing {
                    if let Some(p) = pos {
                        let px = to_px(p);
                        let just = ui.input(|i| i.pointer.primary_pressed()) && resp.is_pointer_button_down_on();
                        if just {
                            self.lift_all();
                            if self.floating.is_some() {
                                self.float_drag = Some(px);
                            }
                        } else if primary {
                            if let (Some(last), Some(f)) = (self.float_drag, self.floating.as_mut()) {
                                f.cx += px.0 - last.0;
                                f.cy += px.1 - last.1;
                                self.float_drag = Some(px);
                                self.redraw_floating();
                            }
                        } else if self.float_drag.take().is_some() {
                            self.commit_floating();
                        }
                    }
                }
                // camera tool: drag to move the camera at this frame
                if self.tool == Tool::Camera && !self.viewing_camera() && !panning && !self.playing {
                    if let Some(p) = pos {
                        let px = to_px(p);
                        let just = ui.input(|i| i.pointer.primary_pressed()) && resp.is_pointer_button_down_on();
                        if just {
                            self.cam_drag = Some(px);
                            self.doc.camera.enabled = true;
                        } else if primary {
                            if let Some(last) = self.cam_drag {
                                let (cw, ch) = (self.doc.w, self.doc.h);
                                let mut k = self.doc.camera.at(self.doc.frame, cw, ch);
                                k.frame = self.doc.frame;
                                if !self.doc.camera.lock_x {
                                    k.x += px.0 - last.0;
                                }
                                if !self.doc.camera.lock_y {
                                    k.y += px.1 - last.1;
                                }
                                self.doc.camera.set_key(k);
                                self.doc.dirty = true;
                                self.cam_drag = Some(px);
                            }
                        } else {
                            self.cam_drag = None;
                        }
                    }
                }
                if (self.doc.camera.enabled || self.tool == Tool::Camera) && !self.viewing_camera() {
                    let (cw, ch) = (self.doc.w, self.doc.h);
                    let mut pts: Vec<Pos2> = self
                        .doc
                        .camera
                        .corners(self.doc.frame, cw, ch)
                        .iter()
                        .map(|q| to_screen(*q))
                        .collect();
                    pts.push(pts[0]);
                    let col = Color32::from_rgb(255, 210, 60);
                    painter.add(egui::Shape::line(pts.clone(), Line::new(2.0_f32, col)));
                    painter.line_segment([pts[0], pts[1]], Line::new(4.0_f32, col));
                    // the path through the keys
                    let path: Vec<Pos2> = (0..self.doc.frames)
                        .map(|f| {
                            let k = self.doc.camera.at(f, cw, ch);
                            to_screen((k.x, k.y))
                        })
                        .collect();
                    if path.len() > 1 {
                        painter.add(egui::Shape::line(path, Line::new(1.0_f32, col.gamma_multiply(0.6))));
                    }
                    for k in &self.doc.camera.keys {
                        painter.circle_filled(to_screen((k.x, k.y)), if k.frame == self.doc.frame { 6.0 } else { 4.0 }, col);
                    }
                }
                let pressed_now = ui.input(|i| i.pointer.primary_pressed()) && resp.is_pointer_button_down_on();
                let mods = ui.input(|i| i.modifiers);
                if special && !panning && !self.playing {
                    if let Some(p) = pos {
                        let px = to_px(p);
                        if self.tool == Tool::Select && self.floating.is_some() {
                            if pressed_now {
                                self.float_drag = Some(px);
                            } else if primary {
                                if let Some(last) = self.float_drag {
                                    if let Some(f) = self.floating.as_mut() {
                                        f.cx += px.0 - last.0;
                                        f.cy += px.1 - last.1;
                                    }
                                    self.float_drag = Some(px);
                                    self.redraw_floating();
                                }
                            }
                        } else if self.tool == Tool::Select {
                            if pressed_now {
                                self.sel_combine = if mods.shift {
                                    Combine::Add
                                } else if mods.alt {
                                    Combine::Subtract
                                } else {
                                    Combine::Replace
                                };
                                if self.sel_mode == SelectMode::Wand {
                                    self.wand_at(px);
                                } else {
                                    self.sel_drag = Some(vec![px]);
                                }
                            } else if primary {
                                if let Some(d) = self.sel_drag.as_mut() {
                                    match self.sel_mode {
                                        SelectMode::Rect => {
                                            if d.len() < 2 {
                                                d.push(px);
                                            } else {
                                                d[1] = px;
                                            }
                                        }
                                        SelectMode::Lasso => d.push(px),
                                        SelectMode::Wand => {}
                                    }
                                }
                            }
                        } else if self.tool == Tool::Gradient && pressed_now {
                            self.commit_floating();
                            self.grad_start = Some(px);
                        }
                        if !primary {
                            if let Some(d) = self.sel_drag.take() {
                                self.finish_selection(d);
                            }
                            if let Some(a) = self.grad_start.take() {
                                self.apply_gradient(a, px);
                            }
                            self.float_drag = None;
                        }
                    }
                }
                // selection overlay
                if let Some(sel) = &self.selection {
                    if self.sel_dirty || self.sel_tex.is_none() {
                        let mut ci = egui::ColorImage::new([sel.w, sel.h], Color32::TRANSPARENT);
                        let tint = Color32::from_rgba_unmultiplied(60, 140, 255, 70);
                        for (i, m) in sel.mask.iter().enumerate() {
                            if *m != 0 {
                                ci.pixels[i] = tint;
                            }
                        }
                        match &mut self.sel_tex {
                            Some(t) => t.set(ci, egui::TextureOptions::NEAREST),
                            None => self.sel_tex = Some(ctx.load_texture("selection", ci, egui::TextureOptions::NEAREST)),
                        }
                        self.sel_dirty = false;
                    }
                    if self.floating.is_none() {
                        if let Some(t) = &self.sel_tex {
                            quad(&painter, t.id());
                        }
                        let b = sel.bounds;
                        let (x0, y0, x1, y1) = (b.x0 as f32, b.y0 as f32, b.x1 as f32, b.y1 as f32);
                        let r = vec![
                            to_screen((x0, y0)),
                            to_screen((x1, y0)),
                            to_screen((x1, y1)),
                            to_screen((x0, y1)),
                            to_screen((x0, y0)),
                        ];
                        painter.add(egui::Shape::line(r, Line::new(1.0_f32, Color32::from_rgb(60, 140, 255))));
                    }
                }
                if let Some(d) = &self.sel_drag {
                    let pts: Vec<Pos2> = match self.sel_mode {
                        SelectMode::Rect if d.len() == 2 => {
                            let (a, b) = (d[0], d[1]);
                            vec![
                                to_screen(a),
                                to_screen((b.0, a.1)),
                                to_screen(b),
                                to_screen((a.0, b.1)),
                                to_screen(a),
                            ]
                        }
                        _ => d.iter().map(|q| to_screen(*q)).collect(),
                    };
                    painter.add(egui::Shape::line(pts, Line::new(1.5_f32, Color32::from_rgb(60, 140, 255))));
                }
                if let Some(f) = &self.floating {
                    let mut pts: Vec<Pos2> = f.corners().iter().map(|q| to_screen(*q)).collect();
                    pts.push(pts[0]);
                    painter.add(egui::Shape::line(pts, Line::new(1.5_f32, Color32::from_rgb(255, 160, 40))));
                }
                if let (Some(a), Some(p)) = (self.grad_start, pos) {
                    painter.line_segment([to_screen(a), p], Line::new(2.0_f32, Color32::from_rgb(255, 160, 40)));
                    painter.circle_filled(to_screen(a), 4.0, c32(self.color));
                    painter.circle_filled(p, 4.0, c32(self.color2));
                }

                // ---- tools ----
                if !panning && !self.playing && !special {
                    // one press starts a tool once: the press itself, or (when the press began
                    // elsewhere) the start of a drag — never both, or a stroke would start twice and
                    // its first dabs would lose their undo step
                    let started = !self.tool_pressed
                        && self.stroke.is_none()
                        && self.custom_stroke.is_none()
                        && self.shape_start.is_none()
                        && (resp.drag_started_by(egui::PointerButton::Primary)
                            || (resp.is_pointer_button_down_on() && primary && ui.input(|i| i.pointer.primary_pressed())));
                    if !primary {
                        self.tool_pressed = false;
                    }
                    if started {
                        self.tool_pressed = true;
                    }
                    if started {
                        if let Some(p) = pos {
                            let px = to_px(p);
                            // the pen's eraser end erases, whatever the drawing tool, for this stroke
                            if self.pen.is_some_and(|p| p.eraser) && self.tool.paints() && self.tool != Tool::Eraser {
                                self.eraser_back = Some(self.tool);
                                self.tool = Tool::Eraser;
                            }
                            self.begin_tool(px);
                        }
                    }
                }
                if let Some(p) = pos {
                    if primary {
                        let px = to_px(p);
                        if self.stroke.is_some() {
                            let s = self.stroke.as_mut().unwrap();
                            let id = s.id;
                            let q = self.smooth_point(px);
                            let s = self.stroke.as_mut().unwrap();
                            if let Some(p) = self.pen {
                                s.set_pen(p.tilt(), p.azimuth(), p.twist());
                            }
                            let r = s.to_p(q.0, q.1, self.pressure, &mut self.doc.drawings[id].px);
                            self.refresh_rect(r);
                        } else if self.custom_stroke.is_some() {
                            let q = self.smooth_point(px);
                            let cs = self.custom_stroke.as_mut().unwrap();
                            let id = cs.id;
                            let r = cs.to(q.0, q.1, &mut self.doc.drawings[id].px);
                            self.refresh_rect(r);
                        } else if self.tool == Tool::Picker && resp.is_pointer_button_down_on() && !panning {
                            self.pick(px);
                        }
                    }
                }
                if !primary {
                    // back to the drawing tool after a stroke with the pen's eraser end
                    if let Some(t) = self.eraser_back.take() {
                        if self.tool == Tool::Eraser {
                            self.tool = t;
                        }
                    }
                    if let Some(mut s) = self.stroke.take() {
                        // catch up: the stroke ends where the pen was lifted
                        if self.smooth_on && self.smooth_catchup {
                            if let Some(r) = self.smooth_raw {
                                let id = s.id;
                                s.to_p(r.0, r.1, self.pressure, &mut self.doc.drawings[id].px);
                            }
                        }
                        self.smooth_pos = None;
                        self.doc.push_pixel_undo(s.id, s.dirty, &s.before);
                        self.needs_full = true;
                    }
                    if let Some(s) = self.custom_stroke.take() {
                        self.doc.push_pixel_undo(s.id, s.dirty, &s.before);
                    }
                    if let Some(a) = self.shape_start.take() {
                        if let Some(p) = pos {
                            self.finish_shape(a, to_px(p));
                        }
                    }
                }
                // shape preview
                if let (Some(a), Some(p)) = (self.shape_start, pos) {
                    let b = to_px(p);
                    let width = (self.set().radius * 2.0 * self.zoom).max(1.0);
                    for poly in brush::shape_points(self.tool, a, b) {
                        let pts: Vec<Pos2> = poly.iter().map(|q| to_screen(*q)).collect();
                        painter.add(egui::Shape::line(pts, Line::new(width, c32(self.color))));
                    }
                }
                // line smoothing HUD: the string between the pen and the brush
                if self.smooth_on && self.smooth_hud && self.stroke.is_some() {
                    if let (Some(a), Some(b)) = (self.smooth_pos, self.smooth_raw) {
                        painter.line_segment([to_screen(a), to_screen(b)], Line::new(1.5_f32, Color32::from_rgb(255, 160, 40)));
                        painter.circle_filled(to_screen(a), 3.0, Color32::from_rgb(255, 160, 40));
                    }
                }
                // custom brush outline
                if let (Some(hp), Some(b), Tool::Custom) = (hover, &self.custom_brush, self.tool) {
                    let sc = self.custom_set.scale / 100.0 * self.zoom;
                    let r = egui::Rect::from_center_size(hp, vec2(b.w as f32 * sc, b.h as f32 * sc));
                    painter.rect_stroke(r, 0.0, Line::new(1.0_f32, Color32::from_white_alpha(160)), StrokeKind::Outside);
                }
                // brush outline
                if let Some(hp) = hover {
                    if self.tool.has_size() && !panning {
                        let r = self.set().radius * self.zoom;
                        painter.circle_stroke(hp, r.max(1.0), Line::new(1.5_f32, Color32::from_black_alpha(180)));
                        painter.circle_stroke(hp, (r - 1.5).max(0.5), Line::new(1.0_f32, Color32::from_white_alpha(200)));
                        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                    } else if panning {
                        ctx.set_cursor_icon(egui::CursorIcon::Grab);
                    } else {
                        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                    }
                }
                // layer locked notice
                let l = &self.doc.layers[self.doc.current_layer];
                if l.locked {
                    painter.text(
                        view.left_top() + vec2(10.0, 10.0),
                        egui::Align2::LEFT_TOP,
                        trf("« {} » est verrouillé", &[&l.name]),
                        egui::FontId::proportional(14.0),
                        Color32::from_rgb(255, 200, 80),
                    );
                }
                if !self.status.is_empty() {
                    painter.text(
                        view.left_bottom() + vec2(10.0, -8.0),
                        egui::Align2::LEFT_BOTTOM,
                        &self.status,
                        egui::FontId::proportional(12.0),
                        th.text.gamma_multiply(0.8),
                    );
                }
            });
    }

    /// Merge the current layer down, or every visible layer.
    fn merge(&mut self, visible: bool) {
        self.commit_floating();
        let opts = crate::doc::MergeOptions::default();
        let ok = if visible {
            self.doc.merge_visible(opts)
        } else {
            self.doc.merge_down(opts)
        };
        if !ok {
            self.status = if visible {
                tr("Il faut au moins deux calques visibles et non verrouillés.").into()
            } else {
                tr("Il n'y a pas de calque dessous, ou l'un des deux est verrouillé.").into()
            };
        }
        self.needs_full = true;
    }

    fn begin_tool(&mut self, px: (f32, f32)) {
        // a stroke still open is finished first, so its undo step is kept
        if let Some(s) = self.stroke.take() {
            self.doc.push_pixel_undo(s.id, s.dirty, &s.before);
        }
        if let Some(s) = self.custom_stroke.take() {
            self.doc.push_pixel_undo(s.id, s.dirty, &s.before);
        }
        self.commit_floating();
        let s = self.set();
        match self.tool {
            t if t.paints() => {
                if let Some(id) = self.doc.target_drawing() {
                    self.needs_full = true;
                    let (w, h) = (self.doc.w, self.doc.h);
                    let mut st = Stroke::begin(id, &self.doc.drawings[id].px, w, h, t, self.color, s);
                    if let Some(m) = self.paint_mask() {
                        st.set_clip(m);
                    }
                    st.set_preserve(self.preserve);
                    self.stroke = Some(st);
                    self.smooth_pos = Some(px);
                    self.smooth_raw = Some(px);
                    self.smooth_hist.clear();
                    self.smooth_hist.push_back(px);
                } else {
                    self.status = tr("Ce calque est verrouillé.").into();
                }
            }
            t if t.is_shape() => {
                if self.doc.layers[self.doc.current_layer].locked {
                    self.status = tr("Ce calque est verrouillé.").into();
                } else {
                    self.shape_start = Some(px);
                }
            }
            Tool::Fill => {
                if px.0 < 0.0 || px.1 < 0.0 {
                    return;
                }
                let (x, y) = (px.0 as usize, px.1 as usize);
                if x >= self.doc.w || y >= self.doc.h {
                    return;
                }
                if let Some(id) = self.doc.target_drawing() {
                    let before = self.doc.drawings[id].px.clone();
                    let (w, h) = (self.doc.w, self.doc.h);
                    let r = brush::flood_fill(&mut self.doc.drawings[id].px, w, h, x, y, self.color, s.power, s.opacity);
                    self.clip_to_selection(id, &before);
                    self.doc.push_pixel_undo(id, r, &before);
                    self.needs_full = true;
                }
            }
            Tool::Picker => self.pick(px),
            Tool::Text => self.place_text(px),
            Tool::Custom => {
                let Some(b) = self.custom_brush.clone() else {
                    self.status = tr(
                        "Pas encore de pinceau : sélectionnez une zone puis « Créer un pinceau avec la sélection », ou chargez une image.",
                    )
                    .into();
                    return;
                };
                if let Some(id) = self.doc.target_drawing() {
                    let (w, h) = (self.doc.w, self.doc.h);
                    let clip = self.paint_mask();
                    self.smooth_pos = Some(px);
                    self.smooth_raw = Some(px);
                    self.smooth_hist.clear();
                    let mut st = CustomStroke::begin(id, &self.doc.drawings[id].px, w, h, b, self.custom_set, self.color, clip);
                    st.to(px.0, px.1, &mut self.doc.drawings[id].px);
                    self.custom_stroke = Some(st);
                    self.needs_full = true;
                } else {
                    self.status = tr("Ce calque est verrouillé.").into();
                }
            }
            _ => {}
        }
    }

    fn place_text(&mut self, px: (f32, f32)) {
        if self.text.text.trim().is_empty() {
            return;
        }
        if self.font.is_none() {
            // a font with Chinese / Japanese letters when the text needs one
            let needs_cjk = self.text.text.chars().any(|c| (c as u32) >= 0x2E80);
            self.font = if needs_cjk {
                i18n::find_cjk_font(i18n::lang() == i18n::Lang::Ja)
                    .and_then(|(p, i)| textmod::font_from_file(&p, i))
                    .or_else(textmod::default_font)
            } else {
                textmod::default_font()
            };
        }
        let Some(font) = &self.font else { return };
        let Some(id) = self.doc.target_drawing() else {
            self.status = tr("Ce calque est verrouillé.").into();
            return;
        };
        let (w, h) = (self.doc.w, self.doc.h);
        let before = self.doc.drawings[id].px.clone();
        let r = textmod::stamp(&mut self.doc.drawings[id].px, w, h, px.0, px.1, self.color, &self.text, font);
        self.doc.push_pixel_undo(id, r, &before);
        self.needs_full = true;
        // the next text may need another font
        self.font = None;
    }

    /// Put back the pixels outside the selection (after a fill or a text).
    fn clip_to_selection(&mut self, id: usize, before: &[u8]) {
        if let Some(mask) = self.paint_mask() {
            let px = &mut self.doc.drawings[id].px;
            for (i, m) in mask.iter().enumerate() {
                if *m == 0 {
                    px[i * 4..i * 4 + 4].copy_from_slice(&before[i * 4..i * 4 + 4]);
                }
            }
        }
    }

    /// Where paint may go: the selection, and outside (or inside, inverted) the drawings of the
    /// layers marked as stencils. None = everywhere.
    fn paint_mask(&self) -> Option<std::sync::Arc<Vec<u8>>> {
        let sel = self.selection.as_ref().map(|s| &s.mask);
        let mask = match (sel, self.doc.stencil_mask(self.stencil_invert)) {
            (None, None) => return None,
            (Some(s), None) => s.clone(),
            (None, Some(m)) => m,
            (Some(s), Some(mut m)) => {
                for (a, b) in m.iter_mut().zip(s.iter()) {
                    *a = (*a).min(*b);
                }
                m
            }
        };
        Some(std::sync::Arc::new(mask))
    }

    /// Smooth a pen point (line smoothing), remembering the raw one for "catch up".
    fn smooth_point(&mut self, raw: (f32, f32)) -> (f32, f32) {
        self.smooth_raw = Some(raw);
        if !self.smooth_on {
            return raw;
        }
        let v = self.smooth_value.max(1.0);
        match self.smooth_type {
            // pulled string: the brush follows the pen at the end of a string `value` pixels long
            2 => {
                let p = self.smooth_pos.unwrap_or(raw);
                let (dx, dy) = (raw.0 - p.0, raw.1 - p.1);
                let d = (dx * dx + dy * dy).sqrt();
                let q = if d > v {
                    (p.0 + dx * (1.0 - v / d), p.1 + dy * (1.0 - v / d))
                } else {
                    p
                };
                self.smooth_pos = Some(q);
                q
            }
            // average of the last `value` pen positions (1), or of the points spaced at least
            // `value` / 4 pixels apart (3, "average points")
            t => {
                let min_gap = if t == 3 { v / 4.0 } else { 0.0 };
                let keep = match self.smooth_hist.back() {
                    Some(b) => ((raw.0 - b.0).powi(2) + (raw.1 - b.1).powi(2)).sqrt() >= min_gap,
                    None => true,
                };
                if keep {
                    self.smooth_hist.push_back(raw);
                }
                while self.smooth_hist.len() > v.round() as usize {
                    self.smooth_hist.pop_front();
                }
                let n = self.smooth_hist.len().max(1) as f32;
                let q = self.smooth_hist.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0 / n, a.1 + p.1 / n));
                self.smooth_pos = Some(q);
                q
            }
        }
    }

    /// Lift the whole drawing of the current layer to move it (position tool).
    fn lift_all(&mut self) {
        self.commit_floating();
        let Some(id) = self.doc.target_drawing() else {
            self.status = tr("Ce calque est verrouillé.").into();
            return;
        };
        let (w, h) = (self.doc.w, self.doc.h);
        let before = self.doc.drawings[id].px.clone();
        let base = vec![0u8; w * h * 4];
        self.floating = Some(Floating::new(id, (before.clone(), w, h, 0, 0), before, base));
    }

    fn selection_panel(&mut self, ui: &mut egui::Ui) {
        if let Some(f) = &mut self.floating {
            ui.label(tr("Transformation en cours"));
            let mut changed = false;
            let mut sc = f.scale * 100.0;
            if ui
                .add(
                    egui::Slider::new(&mut sc, 1.0..=800.0)
                        .logarithmic(true)
                        .text(tr("Échelle"))
                        .suffix(" %"),
                )
                .changed()
            {
                f.scale = sc / 100.0;
                changed = true;
            }
            changed |= ui
                .add(egui::Slider::new(&mut f.angle, -180.0..=180.0).text(tr("Angle")).suffix("°"))
                .changed();
            ui.horizontal(|ui| {
                changed |= ui.checkbox(&mut f.flip_x, tr("Miroir ↔")).changed();
                changed |= ui.checkbox(&mut f.flip_y, tr("Miroir ↕")).changed();
            });
            ui.small(tr("Glissez sur la toile pour déplacer."));
            let (mut ok, mut cancel) = (false, false);
            ui.horizontal(|ui| {
                ok = ui.button(tr("Valider (Entrée)")).clicked();
                cancel = ui.button(tr("Annuler (Échap)")).clicked();
            });
            if changed {
                self.redraw_floating();
            }
            if ok {
                self.commit_floating();
            }
            if cancel {
                self.cancel_floating();
            }
            return;
        }
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.sel_mode, SelectMode::Rect, tr("Rectangle"));
            ui.radio_value(&mut self.sel_mode, SelectMode::Lasso, tr("Lasso"));
            ui.radio_value(&mut self.sel_mode, SelectMode::Wand, tr("Baguette"));
        });
        if self.sel_mode == SelectMode::Wand {
            ui.add(egui::Slider::new(&mut self.wand_tol, 0.0..=100.0).text(tr("Tolérance")));
        }
        ui.small(tr("Maj : ajouter · Alt : retirer"));
        ui.horizontal_wrapped(|ui| {
            if ui.small_button(tr("Tout")).clicked() {
                self.select_all();
            }
            if ui.small_button(tr("Inverser")).clicked() {
                if let Some(s) = &mut self.selection {
                    s.invert();
                    self.sel_dirty = true;
                }
            }
            if ui.small_button(tr("Aucune")).clicked() {
                self.deselect();
            }
        });
        let has = self.selection.is_some();
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(has, egui::Button::new(tr("Copier")).small()).clicked() {
                self.copy_selection();
            }
            if ui.add_enabled(has, egui::Button::new(tr("Couper")).small()).clicked() {
                self.cut_selection();
            }
            if ui
                .add_enabled(self.clipboard.is_some(), egui::Button::new(tr("Coller")).small())
                .clicked()
            {
                self.paste();
            }
            if ui.add_enabled(has, egui::Button::new(tr("Effacer")).small()).clicked() {
                self.delete_selection();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(has, egui::Button::new(tr("Transformer")).small()).clicked() {
                self.lift();
            }
            if ui.add_enabled(has, egui::Button::new(tr("Miroir ↔")).small()).clicked() {
                self.flip_selection(true);
            }
            if ui.add_enabled(has, egui::Button::new(tr("Miroir ↕")).small()).clicked() {
                self.flip_selection(false);
            }
        });
        if ui
            .add_enabled(has, egui::Button::new(tr("Créer un pinceau avec la sélection")))
            .clicked()
        {
            self.brush_from_selection();
        }
    }

    fn set_custom_brush(&mut self, b: CustomBrush, ctx: Option<&egui::Context>) {
        let _ = ctx;
        self.custom_tex = None;
        self.custom_brush = Some(std::sync::Arc::new(b));
        self.tool = Tool::Custom;
    }

    fn brush_from_selection(&mut self) {
        let (Some(sel), Some(id)) = (&self.selection, self.current_drawing()) else {
            return;
        };
        if let Some((px, w, h, _, _)) = sel.copy(&self.doc.drawings[id].px) {
            match CustomBrush::trimmed(px, w, h) {
                Some(b) => {
                    self.status = trf("Pinceau créé : {} × {} px", &[&b.w, &b.h]);
                    self.set_custom_brush(b, None);
                }
                None => self.status = tr("La sélection est vide.").into(),
            }
        }
    }

    fn custom_panel(&mut self, ui: &mut egui::Ui) {
        if let Some(b) = self.custom_brush.clone() {
            if self.custom_tex.is_none() {
                let img = egui::ColorImage::from_rgba_unmultiplied([b.w, b.h], &b.px);
                self.custom_tex = Some(ui.ctx().load_texture("custom-brush", img, egui::TextureOptions::LINEAR));
            }
            if let Some(t) = &self.custom_tex {
                let sz = vec2(b.w as f32, b.h as f32);
                let k = (96.0 / sz.x.max(sz.y)).min(1.0);
                ui.add(egui::Image::new((t.id(), sz * k)));
            }
            ui.small(format!("{} × {} px", b.w, b.h));
        } else {
            ui.small(tr(
                "Pas encore de pinceau : sélectionnez une zone puis « Créer un pinceau avec la sélection », ou chargez une image.",
            ));
        }
        let has_sel = self.selection.is_some();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(has_sel, egui::Button::new(tr("Depuis la sélection")).small())
                .clicked()
            {
                self.brush_from_selection();
            }
            if ui.small_button(tr("Charger une image…")).clicked() {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter(tr("Image"), &["png", "jpg", "jpeg", "tif", "tiff", "tga", "bmp"])
                    .pick_file()
                {
                    match io::load_picture(&p) {
                        Ok((px, w, h)) => match CustomBrush::trimmed(px, w, h) {
                            Some(b) => self.set_custom_brush(b, None),
                            None => self.status = tr("La sélection est vide.").into(),
                        },
                        Err(e) => self.status = trf("Échec de l'ouverture : {}", &[&e]),
                    }
                }
            }
        });
        let c = &mut self.custom_set;
        ui.add(
            egui::Slider::new(&mut c.scale, 1.0..=400.0)
                .logarithmic(true)
                .text(tr("Taille"))
                .suffix(" %"),
        );
        ui.add(egui::Slider::new(&mut c.angle, -180.0..=180.0).text(tr("Angle")).suffix("°"));
        ui.add(egui::Slider::new(&mut c.opacity, 1.0..=100.0).text(tr("Opacité")).suffix(" %"));
        ui.add(egui::Slider::new(&mut c.step, 1.0..=200.0).text(tr("Espacement")).suffix(" %"));
        ui.horizontal(|ui| {
            ui.checkbox(&mut c.flip_x, tr("Miroir ↔"));
            ui.checkbox(&mut c.flip_y, tr("Miroir ↕"));
        });
        ui.checkbox(&mut c.tint, tr("Peindre avec la couleur A (garder la forme)"));
    }

    fn presets_panel(&mut self, ui: &mut egui::Ui) {
        let mut apply: Option<usize> = None;
        let mut remove: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, p) in self.presets.iter().enumerate() {
                let b = ui.add(
                    egui::Button::new(egui::RichText::new(&p.name).small())
                        .selected(p.tool == self.tool && p.settings.radius == self.set().radius && p.custom.is_none()),
                );
                if b.clicked() {
                    apply = Some(i);
                }
                if b.secondary_clicked() && ui.input(|i| i.modifiers.alt) {
                    remove = Some(i);
                }
            }
        });
        ui.small(tr("Alt + clic droit : supprimer le préréglage"));
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.preset_name)
                    .hint_text(tr("Nom"))
                    .desired_width(110.0),
            );
            if ui.small_button(tr("Enregistrer")).clicked() {
                let name = if self.preset_name.trim().is_empty() {
                    format!("{} {:.0}", self.tool.label(), self.set().radius)
                } else {
                    self.preset_name.trim().to_string()
                };
                let (custom, image) = if self.tool == Tool::Custom {
                    (
                        Some(self.custom_set),
                        self.custom_brush.as_ref().and_then(|b| crate::custom::save_brush_image(b)),
                    )
                } else {
                    (None, None)
                };
                self.presets.push(Preset {
                    name,
                    tool: self.tool,
                    settings: self.set(),
                    custom,
                    image,
                });
                crate::custom::save_presets(&self.presets);
                self.preset_name.clear();
            }
        });
        if let Some(i) = apply {
            let p = self.presets[i].clone();
            self.tool = p.tool;
            *self.set_mut() = p.settings;
            if let Some(c) = p.custom {
                self.custom_set = c;
            }
            if let Some(img) = &p.image {
                match crate::custom::load_brush_image(img) {
                    Some(b) => self.set_custom_brush(b, None),
                    None => self.status = tr("Fichier introuvable").into(),
                }
            }
        }
        if let Some(i) = remove {
            self.presets.remove(i);
            crate::custom::save_presets(&self.presets);
        }
    }

    /// Make another clip the one being edited.
    fn switch_clip(&mut self, i: usize) {
        if i == self.active_clip || i >= self.clips.len() {
            return;
        }
        self.commit_floating();
        self.close_fx(false);
        self.player.stop();
        let Some(next) = self.clips[i].doc.take() else { return };
        let dirty = self.doc.dirty;
        let cur = std::mem::replace(&mut self.doc, next);
        self.clips[self.active_clip].doc = Some(cur);
        self.clips[self.active_clip].thumb = None;
        self.active_clip = i;
        self.replace_doc_keep_clips();
        self.doc.dirty |= dirty;
    }

    /// Reset the view for the animation now in `self.doc` (another clip).
    fn replace_doc_keep_clips(&mut self) {
        self.tex = None;
        self.comp.clear();
        self.needs_full = true;
        self.fit_pending = true;
        self.stroke = None;
        self.tl_scroll = Vec2::ZERO;
        self.selection = None;
        self.sel_dirty = true;
        self.load_clip();
    }

    fn clip_doc(&self, i: usize) -> &Doc {
        if i == self.active_clip {
            &self.doc
        } else {
            self.clips[i].doc.as_ref().expect("inactive clip")
        }
    }

    fn add_clip(&mut self, duplicate: bool) {
        let doc = if duplicate {
            match io::duplicate(&self.doc) {
                Ok(d) => d,
                Err(e) => {
                    self.status = e;
                    return;
                }
            }
        } else {
            Doc::new(self.doc.w, self.doc.h, self.doc.fps)
        };
        let name = if duplicate {
            trf("{} copie", &[&self.clips[self.active_clip].name])
        } else {
            format!("{} {}", tr("Plan"), self.clips.len() + 1)
        };
        let at = self.active_clip + 1;
        self.clips.insert(at, ClipSlot::new(name, Some(doc)));
        self.switch_clip(at);
        self.doc.dirty = true;
    }

    fn delete_clip(&mut self, i: usize) {
        if self.clips.len() <= 1 {
            return;
        }
        if i == self.active_clip {
            let other = if i == 0 { 1 } else { i - 1 };
            self.switch_clip(other);
        }
        self.clips.remove(i);
        if self.active_clip > i {
            self.active_clip -= 1;
        }
        self.doc.dirty = true;
    }

    fn move_clip(&mut self, i: usize, right: bool) {
        let j = if right { i + 1 } else { i.wrapping_sub(1) };
        if j >= self.clips.len() {
            return;
        }
        self.clips.swap(i, j);
        if self.active_clip == i {
            self.active_clip = j;
        } else if self.active_clip == j {
            self.active_clip = i;
        }
        self.doc.dirty = true;
    }

    fn export_film(&mut self, video: bool) {
        let docs: Vec<&Doc> = (0..self.clips.len()).map(|i| self.clip_doc(i)).collect();
        if video {
            let Some(ff) = io::find_ffmpeg() else {
                self.status = tr("L'export vidéo a besoin de FFmpeg (gratuit, ffmpeg.org) installé sur l'ordinateur.").into();
                return;
            };
            if let Some(mut p) = rfd::FileDialog::new()
                .add_filter(tr("Vidéo MP4"), &["mp4"])
                .set_file_name("film.mp4")
                .save_file()
            {
                if p.extension().is_none() {
                    p.set_extension("mp4");
                }
                let r = io::export_film_video(&docs, &p, &ff);
                self.status = match r {
                    Ok(()) => trf("Vidéo exportée : {}", &[&p.display()]),
                    Err(e) => trf("Échec de l'export vidéo : {}", &[&e]),
                };
            }
        } else if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            let r = io::export_film_frames(&docs, &dir);
            self.status = match r {
                Ok(n) => trf("{} images exportées dans {}", &[&n, &dir.display()]),
                Err(e) => trf("Échec de l'export : {}", &[&e]),
            };
        }
    }

    fn storyboard_window(&mut self, ctx: &egui::Context) {
        if !self.show_storyboard {
            return;
        }
        // thumbnails: the first frame of each clip
        for i in 0..self.clips.len() {
            if self.clips[i].thumb.is_none() || i == self.active_clip {
                let d = self.clip_doc(i);
                if d.frames == 0 {
                    continue;
                }
                if i == self.active_clip && self.clips[i].thumb.is_some() && !self.doc.dirty {
                    continue;
                }
                let frame = if i == self.active_clip { self.doc.frame } else { 0 };
                let (ow, oh) = d.output_size();
                let px = d.render(frame);
                let img = image::RgbaImage::from_raw(ow as u32, oh as u32, px).unwrap_or_default();
                let k = (192.0 / ow.max(oh) as f32).min(1.0);
                let (tw, th) = (((ow as f32 * k) as u32).max(1), ((oh as f32 * k) as u32).max(1));
                let small = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
                let ci = egui::ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], small.as_raw());
                self.clips[i].thumb = Some(ctx.load_texture(format!("clip-{i}"), ci, egui::TextureOptions::LINEAR));
            }
        }
        let mut open = true;
        let (mut go, mut del, mut mv): (Option<usize>, Option<usize>, Option<(usize, bool)>) = (None, None, None);
        let (mut add, mut dup) = (false, false);
        let total: usize = (0..self.clips.len()).map(|i| self.clip_doc(i).frames).sum();
        let fps = self.doc.fps.max(1.0);
        egui::Window::new(tr("Storyboard"))
            .open(&mut open)
            .default_size([760.0, 420.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    add = ui.button(tr("+ Nouveau plan")).clicked();
                    dup = ui.button(tr("Dupliquer le plan")).clicked();
                    if ui
                        .button(tr("Exporter en page web (HTML)…"))
                        .on_hover_text(tr(
                            "Une page avec une vignette, la durée et les notes de chaque plan ; imprimable en PDF depuis le navigateur.",
                        ))
                        .clicked()
                    {
                        self.export_board_pending = true;
                    }
                    ui.checkbox(&mut self.play_film, tr("Lire tous les plans à la suite"));
                    ui.label(trf(
                        "{} plans · {} images · {} s",
                        &[&self.clips.len(), &total, &format!("{:.1}", total as f32 / fps)],
                    ));
                });
                ui.separator();
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let n = self.clips.len();
                        for i in 0..n {
                            let frames = self.clip_doc(i).frames;
                            let cur = i == self.active_clip;
                            let clip = &mut self.clips[i];
                            egui::Frame::group(ui.style())
                                .fill(if cur {
                                    ui.visuals().selection.bg_fill.gamma_multiply(0.5)
                                } else {
                                    ui.visuals().faint_bg_color
                                })
                                .show(ui, |ui| {
                                    ui.set_width(200.0);
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.strong(format!("{}", i + 1));
                                            ui.add(egui::TextEdit::singleline(&mut clip.name).desired_width(150.0));
                                        });
                                        if let Some(t) = &clip.thumb {
                                            let r = ui.add(egui::Image::new((t.id(), t.size_vec2())).sense(Sense::click()));
                                            if r.clicked() {
                                                go = Some(i);
                                            }
                                        }
                                        ui.small(format!("{} {} · {:.1} s", frames, tr("images"), frames as f32 / fps));
                                        ui.add(
                                            egui::TextEdit::multiline(&mut clip.notes)
                                                .hint_text(tr("Notes (action, dialogue…)"))
                                                .desired_rows(2)
                                                .desired_width(190.0),
                                        );
                                        ui.horizontal(|ui| {
                                            if ui.add_enabled(!cur, egui::Button::new(tr("Ouvrir")).small()).clicked() {
                                                go = Some(i);
                                            }
                                            if ui.add_enabled(i > 0, egui::Button::new("◀").small()).clicked() {
                                                mv = Some((i, false));
                                            }
                                            if ui.add_enabled(i + 1 < n, egui::Button::new("▶").small()).clicked() {
                                                mv = Some((i, true));
                                            }
                                            if ui.add_enabled(n > 1, egui::Button::new(tr("Supprimer")).small()).clicked() {
                                                del = Some(i);
                                            }
                                        });
                                    });
                                });
                        }
                    });
                });
            });
        self.show_storyboard = open;
        if let Some(i) = go {
            self.switch_clip(i);
        }
        if let Some((i, r)) = mv {
            self.move_clip(i, r);
        }
        if let Some(i) = del {
            self.delete_clip(i);
        }
        if add {
            self.add_clip(false);
        }
        if dup {
            self.add_clip(true);
        }
    }

    fn camera_panel(&mut self, ui: &mut egui::Ui) {
        let (w, h, f) = (self.doc.w, self.doc.h, self.doc.frame);
        let mut changed = false;
        let cam = &mut self.doc.camera;
        changed |= ui
            .checkbox(&mut cam.enabled, tr("Caméra active (exports à travers la caméra)"))
            .changed();
        ui.horizontal(|ui| {
            ui.label(tr("Taille"));
            changed |= ui.add(egui::DragValue::new(&mut cam.width).range(16..=8192)).changed();
            ui.label("×");
            changed |= ui.add(egui::DragValue::new(&mut cam.height).range(16..=8192)).changed();
        });
        ui.small(tr(
            "Glissez sur la toile pour déplacer la caméra à cette image (une clé est créée).",
        ));
        let mut k = cam.at(f, w, h);
        k.frame = f;
        let orig = k;
        let mut key_changed = false;
        let mut zoom = k.zoom * 100.0;
        if ui
            .add(
                egui::Slider::new(&mut zoom, 10.0..=1000.0)
                    .logarithmic(true)
                    .text(tr("Zoom"))
                    .suffix(" %"),
            )
            .changed()
        {
            k.zoom = zoom / 100.0;
            key_changed = true;
        }
        key_changed |= ui
            .add(egui::Slider::new(&mut k.rotation, -360.0..=360.0).text(tr("Rotation")).suffix("°"))
            .changed();
        ui.horizontal(|ui| {
            ui.label("X");
            key_changed |= ui.add(egui::DragValue::new(&mut k.x).speed(1.0)).changed();
            ui.label("Y");
            key_changed |= ui.add(egui::DragValue::new(&mut k.y).speed(1.0)).changed();
        });
        let is_key = cam.keys.iter().any(|e| e.frame == f);
        ui.horizontal_wrapped(|ui| {
            if ui.small_button(tr("Clé à cette image")).clicked() {
                key_changed = true;
            }
            if ui.add_enabled(is_key, egui::Button::new(tr("Supprimer la clé")).small()).clicked() {
                cam.keys.retain(|e| e.frame != f);
                changed = true;
            }
            if ui.small_button(tr("Recentrer")).clicked() {
                k = crate::camera::Camera::rest(w, h, f);
                key_changed = true;
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("Verrous :"));
            changed |= ui.checkbox(&mut cam.lock_x, "X").changed();
            changed |= ui.checkbox(&mut cam.lock_y, "Y").changed();
            changed |= ui.checkbox(&mut cam.lock_rotation, tr("Rotation")).changed();
            changed |= ui.checkbox(&mut cam.lock_zoom, tr("Zoom")).changed();
        });
        // locked values stay as they were
        if cam.lock_x {
            k.x = orig.x;
        }
        if cam.lock_y {
            k.y = orig.y;
        }
        if cam.lock_rotation {
            k.rotation = orig.rotation;
        }
        if cam.lock_zoom {
            k.zoom = orig.zoom;
        }
        if key_changed {
            cam.enabled = true;
            cam.set_key(k);
            changed = true;
        }
        ui.horizontal(|ui| {
            changed |= ui
                .radio_value(&mut cam.interpolation, crate::camera::Interpolation::Smooth, tr("Courbe"))
                .changed();
            changed |= ui
                .radio_value(&mut cam.interpolation, crate::camera::Interpolation::Linear, tr("Linéaire"))
                .changed();
        });
        changed |= ui
            .checkbox(&mut cam.ease, tr("Ralentir au départ et à l'arrivée"))
            .on_hover_text(tr(
                "Entre chaque paire de clés. Le profil de vitesse, lui, règle tout le mouvement.",
            ))
            .changed();
        let mut blur = cam.motion_blur * 100.0;
        let blur_tip = tr(
            "Part du temps de chaque image pendant laquelle la caméra continue de bouger : son mouvement laisse un flou dans la vue caméra et les exports. 0 = image nette.",
        );
        ui.label(tr("Flou de mouvement")).on_hover_text(blur_tip);
        if ui
            .add(egui::Slider::new(&mut blur, 0.0..=100.0).suffix(" %").max_decimals(0))
            .on_hover_text(blur_tip)
            .changed()
        {
            cam.motion_blur = blur / 100.0;
            changed = true;
        }
        ui.label(tr("Profil de vitesse")).on_hover_text(tr(
            "La vitesse de la caméra sur tout son mouvement, de la première à la dernière clé : le temps va de gauche à droite, le chemin parcouru de bas en haut.",
        ));
        ui.horizontal_wrapped(|ui| {
            use crate::camera::ProfilePreset as P;
            for p in crate::camera::ALL_PROFILES {
                let label = match p {
                    P::Even => tr("Régulier"),
                    P::SlowIn => tr("Départ lent"),
                    P::SlowOut => tr("Arrivée lente"),
                    P::SlowInOut => tr("Départ et arrivée lents"),
                };
                if ui.selectable_label(cam.profile == p.points(), label).clicked() {
                    cam.profile = p.points();
                    changed = true;
                }
            }
        });
        changed |= profile_editor(ui, cam);
        ui.small(tr("Glissez un point, cliquez pour en ajouter un, clic droit pour l'enlever."));
        ui.small(trf("Clés : {}", &[&cam.keys.len()]));
        if ui.checkbox(&mut self.cam_view, tr("Voir à travers la caméra")).changed() {
            self.needs_full = true;
            self.fit_pending = true;
            self.tex = None;
        }
        if changed {
            self.doc.dirty = true;
            if self.viewing_camera() {
                self.needs_full = true;
            }
        }
    }

    fn open_fx(&mut self, fx: Effect) {
        self.commit_floating();
        self.close_fx(false);
        let Some(id) = self.current_drawing() else {
            self.status = tr("Rien à modifier ici : dessinez d'abord.").into();
            return;
        };
        if self.doc.layers[self.doc.current_layer].locked {
            self.status = tr("Ce calque est verrouillé.").into();
            return;
        }
        let before = self.doc.drawings[id].px.clone();
        self.fx_dialog = Some(FxDialog {
            fx,
            all_layer: false,
            id,
            before,
        });
        self.preview_fx();
    }

    fn preview_fx(&mut self) {
        if let Some(d) = &self.fx_dialog {
            let (w, h) = (self.doc.w, self.doc.h);
            let mut px = d.before.clone();
            crate::fx::apply(&d.fx, &d.before, &mut px, w, h, self.selection.as_ref());
            self.doc.drawings[d.id].px = px;
            self.needs_full = true;
        }
    }

    /// Close the effect window: keep the effect (`apply`) or put the drawing back.
    fn close_fx(&mut self, apply: bool) {
        let Some(d) = self.fx_dialog.take() else { return };
        let (w, h) = (self.doc.w, self.doc.h);
        if !apply {
            self.doc.drawings[d.id].px = d.before;
            self.needs_full = true;
            return;
        }
        self.doc.push_pixel_undo(d.id, Rect::full(w, h), &d.before);
        if d.all_layer {
            let l = self.doc.current_layer;
            for (_, id) in self.doc.keys(l) {
                if id == d.id {
                    continue;
                }
                let before = self.doc.drawings[id].px.clone();
                let mut px = before.clone();
                crate::fx::apply(&d.fx, &before, &mut px, w, h, self.selection.as_ref());
                self.doc.drawings[id].px = px;
                self.doc.push_pixel_undo(id, Rect::full(w, h), &before);
            }
        }
        self.needs_full = true;
    }

    fn fx_window(&mut self, ctx: &egui::Context) {
        let Some(d) = self.fx_dialog.as_mut() else { return };
        let mut open = true;
        let (mut changed, mut apply, mut cancel) = (false, false, false);
        let title = d.fx.label();
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let color_a = self.color;
                changed |= fx_params_ui(ui, &mut d.fx, color_a);
                ui.separator();
                ui.checkbox(&mut d.all_layer, tr("Toutes les images du calque"));
                if self.selection.is_some() {
                    ui.small(tr("Seulement dans la sélection."));
                }
                ui.horizontal(|ui| {
                    apply = ui.button(tr("Appliquer")).clicked();
                    cancel = ui.button(tr("Fermer")).clicked();
                });
            });
        if changed {
            self.preview_fx();
        }
        if apply {
            self.close_fx(true);
        } else if cancel || !open {
            self.close_fx(false);
        }
    }

    fn finish_selection(&mut self, d: Vec<(f32, f32)>) {
        let (w, h) = (self.doc.w, self.doc.h);
        let new = match self.sel_mode {
            SelectMode::Rect if d.len() == 2 && ((d[0].0 - d[1].0).abs() >= 2.0 || (d[0].1 - d[1].1).abs() >= 2.0) => {
                Selection::rect(w, h, d[0], d[1])
            }
            SelectMode::Lasso if d.len() > 2 => Selection::lasso(w, h, &d),
            _ => {
                if self.sel_combine == Combine::Replace {
                    self.deselect();
                }
                return;
            }
        };
        self.merge_selection(new);
    }

    fn merge_selection(&mut self, new: Selection) {
        let mut cur = self.selection.take().unwrap_or_else(|| Selection::empty(new.w, new.h));
        cur.combine(&new, self.sel_combine);
        self.selection = if cur.is_empty() { None } else { Some(cur) };
        self.sel_dirty = true;
    }

    fn wand_at(&mut self, px: (f32, f32)) {
        if px.0 < 0.0 || px.1 < 0.0 || px.0 as usize >= self.doc.w || px.1 as usize >= self.doc.h {
            return;
        }
        let (w, h) = (self.doc.w, self.doc.h);
        let src = match self.doc.drawing_at(self.doc.current_layer, self.doc.frame) {
            Some(id) => self.doc.drawings[id].px.clone(),
            None => self.comp.clone(),
        };
        if src.len() != w * h * 4 {
            return;
        }
        let s = Selection::wand(&src, w, h, px.0 as usize, px.1 as usize, self.wand_tol);
        self.merge_selection(s);
    }

    fn select_all(&mut self) {
        self.selection = Some(Selection::all(self.doc.w, self.doc.h));
        self.sel_dirty = true;
    }

    fn deselect(&mut self) {
        self.selection = None;
        self.sel_dirty = true;
    }

    fn current_drawing(&self) -> Option<usize> {
        self.doc.drawing_at(self.doc.current_layer, self.doc.frame)
    }

    /// Copy the selection (or, without one, the whole drawing) here and to the system clipboard,
    /// so other programs can paste it.
    fn copy_selection(&mut self) {
        let Some(id) = self.current_drawing() else { return };
        let (w, h) = (self.doc.w, self.doc.h);
        self.clipboard = match &self.selection {
            Some(sel) => sel.copy(&self.doc.drawings[id].px),
            None => Some((self.doc.drawings[id].px.clone(), w, h, 0, 0)),
        };
        if let Some((px, pw, ph, _, _)) = &self.clipboard {
            let img = arboard::ImageData {
                width: *pw,
                height: *ph,
                bytes: std::borrow::Cow::Borrowed(px.as_slice()),
            };
            match arboard::Clipboard::new().and_then(|mut c| c.set_image(img)) {
                Ok(()) => self.sys_clip_mark = Some(clip_mark(px, *pw, *ph)),
                Err(_) => self.sys_clip_mark = None,
            }
            self.status = tr("Copié.").into();
        }
    }

    fn delete_selection(&mut self) {
        self.commit_floating();
        if self.doc.layers[self.doc.current_layer].locked {
            self.status = tr("Ce calque est verrouillé.").into();
            return;
        }
        if let (Some(sel), Some(id)) = (self.selection.clone(), self.current_drawing()) {
            let before = self.doc.drawings[id].px.clone();
            sel.clear(&mut self.doc.drawings[id].px);
            self.doc.push_pixel_undo(id, sel.bounds, &before);
            self.needs_full = true;
        }
    }

    fn cut_selection(&mut self) {
        self.copy_selection();
        self.delete_selection();
    }

    /// Lift the selected pixels into a floating piece that can be moved and transformed.
    fn lift(&mut self) {
        self.commit_floating();
        let Some(sel) = self.selection.clone() else { return };
        let Some(id) = self.doc.target_drawing() else {
            self.status = tr("Ce calque est verrouillé.").into();
            return;
        };
        let before = self.doc.drawings[id].px.clone();
        let Some(piece) = sel.copy(&before) else { return };
        let mut base = before.clone();
        sel.clear(&mut base);
        self.floating = Some(Floating::new(id, piece, before, base));
        self.tool = Tool::Select;
        self.redraw_floating();
    }

    fn paste(&mut self) {
        // a picture copied in another program comes first; our own copy keeps its place
        let mut piece = self.clipboard.clone();
        if let Ok(img) = arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
            let (iw, ih) = (img.width, img.height);
            if iw > 0 && ih > 0 && img.bytes.len() >= iw * ih * 4 {
                let px = img.bytes[..iw * ih * 4].to_vec();
                if Some(clip_mark(&px, iw, ih)) != self.sys_clip_mark || piece.is_none() {
                    let (w, h) = (self.doc.w, self.doc.h);
                    let x = (w.saturating_sub(iw)) / 2;
                    let y = (h.saturating_sub(ih)) / 2;
                    piece = Some((px, iw, ih, x, y));
                    self.status = tr("Image collée depuis le presse-papiers.").into();
                }
            }
        }
        let Some(piece) = piece else { return };
        self.commit_floating();
        let Some(id) = self.doc.target_drawing() else {
            self.status = tr("Ce calque est verrouillé.").into();
            return;
        };
        let before = self.doc.drawings[id].px.clone();
        self.floating = Some(Floating::new(id, piece, before.clone(), before));
        self.tool = Tool::Select;
        self.redraw_floating();
    }

    fn flip_selection(&mut self, horizontal: bool) {
        self.lift();
        if let Some(f) = &mut self.floating {
            if horizontal {
                f.flip_x = true;
            } else {
                f.flip_y = true;
            }
        }
        self.redraw_floating();
        self.commit_floating();
    }

    fn redraw_floating(&mut self) {
        if let Some(f) = &self.floating {
            let (w, h) = (self.doc.w, self.doc.h);
            let mut px = f.base.clone();
            f.render(&mut px, w, h);
            self.doc.drawings[f.id].px = px;
            self.needs_full = true;
        }
    }

    /// Put the floating piece down (one undo step).
    fn commit_floating(&mut self) {
        if let Some(f) = self.floating.take() {
            let (w, h) = (self.doc.w, self.doc.h);
            let mut px = f.base.clone();
            f.render(&mut px, w, h);
            self.doc.drawings[f.id].px = px;
            self.doc.push_pixel_undo(f.id, Rect::full(w, h), &f.before);
            self.selection = None;
            self.sel_dirty = true;
            self.needs_full = true;
        }
    }

    fn cancel_floating(&mut self) {
        if let Some(f) = self.floating.take() {
            self.doc.drawings[f.id].px = f.before;
            self.needs_full = true;
        }
    }

    fn apply_gradient(&mut self, a: (f32, f32), b: (f32, f32)) {
        let Some(id) = self.doc.target_drawing() else {
            self.status = tr("Ce calque est verrouillé.").into();
            return;
        };
        let (w, h) = (self.doc.w, self.doc.h);
        let before = self.doc.drawings[id].px.clone();
        let op = self.set().opacity;
        let r = crate::select::gradient(
            &mut self.doc.drawings[id].px,
            w,
            h,
            a,
            b,
            self.color,
            self.color2,
            self.grad_kind,
            self.grad_mid,
            op,
            self.selection.as_ref(),
        );
        self.doc.push_pixel_undo(id, r, &before);
        self.needs_full = true;
    }

    fn import_picture(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .add_filter(tr("Image"), &["png", "jpg", "jpeg", "tif", "tiff", "tga", "bmp"])
            .pick_file()
        else {
            return;
        };
        match io::load_picture(&p) {
            Ok((px, pw, ph)) => {
                self.commit_floating();
                let Some(id) = self.doc.target_drawing() else {
                    self.status = tr("Ce calque est verrouillé.").into();
                    return;
                };
                let before = self.doc.drawings[id].px.clone();
                let (w, h) = (self.doc.w, self.doc.h);
                let mut f = Floating::new(id, (px, pw, ph, 0, 0), before.clone(), before);
                f.cx = w as f32 / 2.0;
                f.cy = h as f32 / 2.0;
                f.scale = (w as f32 / pw as f32).min(h as f32 / ph as f32).min(1.0);
                self.floating = Some(f);
                self.tool = Tool::Select;
                self.redraw_floating();
                self.status = tr("Image importée : déplacez-la, puis Entrée pour valider.").into();
            }
            Err(e) => self.status = trf("Échec de l'ouverture : {}", &[&e]),
        }
    }

    fn import_sequence(&mut self) {
        let Some(mut files) = rfd::FileDialog::new()
            .add_filter(tr("Image"), &["png", "jpg", "jpeg", "tif", "tiff", "tga", "bmp"])
            .pick_files()
        else {
            return;
        };
        files.sort();
        let name = files.first().and_then(|f| f.file_stem()).map(|n| n.to_string_lossy().to_string());
        self.import_sequence_files(&files, name);
    }

    /// Import a video's frames into a new layer, at the project's frame rate (with FFmpeg), to draw
    /// over them (rotoscoping).
    fn import_video(&mut self) {
        let Some(ff) = io::find_ffmpeg() else {
            self.status = tr("Importer une vidéo nécessite FFmpeg (ffmpeg.org).").into();
            return;
        };
        let Some(video) = rfd::FileDialog::new()
            .add_filter(
                tr("Vidéo"),
                &["mp4", "mov", "m4v", "avi", "mkv", "webm", "gif", "wmv", "mpg", "mpeg"],
            )
            .pick_file()
        else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("toonlight-video-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.status = trf("Échec de l'import : {}", &[&e]);
            return;
        }
        let fps = format!("fps={}", self.doc.fps);
        let out = std::process::Command::new(ff)
            .arg("-loglevel")
            .arg("error")
            .arg("-i")
            .arg(&video)
            .arg("-vf")
            .arg(&fps)
            .arg("-frames:v")
            .arg(MAX_VIDEO_FRAMES.to_string())
            .arg(dir.join("f%05d.png"))
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                    .map(|r| {
                        r.flatten()
                            .map(|e| e.path())
                            .filter(|p| p.extension().is_some_and(|e| e == "png"))
                            .collect()
                    })
                    .unwrap_or_default();
                files.sort();
                let name = video.file_stem().map(|n| n.to_string_lossy().to_string());
                self.import_sequence_files(&files, name);
            }
            Ok(o) => self.status = trf("Échec de l'import : {}", &[&String::from_utf8_lossy(&o.stderr).trim().to_string()]),
            Err(e) => self.status = trf("Échec de l'import : {}", &[&e]),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn export_psd(&mut self) {
        let name = format!("{}-{}.psd", self.project_name(), self.doc.frame + 1);
        if let Some(p) = rfd::FileDialog::new().add_filter("PSD", &["psd"]).set_file_name(name).save_file() {
            match crate::psd::export(&self.doc, self.doc.frame, &p) {
                Ok(()) => self.status = trf("Exporté : {}", &[&p.display()]),
                Err(e) => self.status = trf("Échec de l'export : {}", &[&e]),
            }
        }
    }

    /// Each layer of a PSD becomes a layer (on the current frame), above the current one.
    fn import_psd(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("PSD", &["psd"]).pick_file() else {
            return;
        };
        match crate::psd::read(&p) {
            Ok((layers, _, _)) if !layers.is_empty() => {
                self.commit_floating();
                let (w, h) = (self.doc.w, self.doc.h);
                let f = self.doc.frame;
                let n = layers.len();
                for l in &layers {
                    self.doc.add_layer();
                    let cur = self.doc.current_layer;
                    let id = self.doc.new_drawing();
                    self.doc.drawings[id].px = crate::psd::place(l, w, h);
                    let layer = &mut self.doc.layers[cur];
                    layer.cels.iter_mut().for_each(|c| *c = None);
                    layer.cels[f] = Some(id);
                    layer.name = l.name.clone();
                    layer.opacity = l.opacity as f32 / 255.0;
                    layer.visible = l.visible;
                    layer.blend = l.blend;
                }
                self.needs_full = true;
                self.status = trf("{} calques importés.", &[&n]);
            }
            Ok(_) => self.status = tr("Ce PSD n'a pas de calques.").into(),
            Err(e) => self.status = trf("Échec de l'import : {}", &[&e]),
        }
    }

    /// Write the storyboard as a web page (one card per shot) in a chosen folder.
    fn export_storyboard(&mut self) {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
        let dir = dir.join(format!("{}-storyboard", self.project_name()));
        let shots: Vec<io::BoardShot> = (0..self.clips.len())
            .map(|i| {
                let d = self.clip_doc(i);
                let (w, h) = d.output_size();
                io::BoardShot {
                    name: self.clips[i].name.clone(),
                    notes: self.clips[i].notes.clone(),
                    frames: d.frames,
                    fps: d.fps,
                    picture: (d.render(0), w, h),
                }
            })
            .collect();
        match io::export_storyboard_html(&self.project_name(), &shots, &dir) {
            Ok(index) => {
                self.status = trf("Storyboard exporté : {}", &[&index.display()]);
                let _ = open_in_browser(&index);
            }
            Err(e) => self.status = trf("Échec de l'export : {}", &[&e]),
        }
    }

    /// Flip the selection, or the whole drawing when nothing is selected ("Image: Flip").
    fn flip_image(&mut self, horizontal: bool) {
        let had = self.selection.is_some();
        if !had {
            self.select_all();
        }
        self.flip_selection(horizontal);
        if !had {
            self.deselect();
        }
        self.needs_full = true;
    }

    /// Put pictures (in order) into a new layer, one per frame.
    fn import_sequence_files(&mut self, files: &[PathBuf], name: Option<String>) {
        if files.is_empty() {
            self.status = tr("Aucune image à importer.").into();
            return;
        }
        self.commit_floating();
        self.doc.add_layer();
        let l = self.doc.current_layer;
        if let Some(name) = name {
            self.doc.layers[l].name = name;
        }
        if self.doc.frames < files.len() {
            let n = files.len();
            for layer in &mut self.doc.layers {
                layer.cels.resize(n, None);
            }
            self.doc.frames = n;
        }
        let (w, h) = (self.doc.w, self.doc.h);
        let mut count = 0;
        for (i, f) in files.iter().enumerate() {
            if let Ok((px, pw, ph)) = io::load_picture(f) {
                let id = if i == 0 {
                    self.doc.layers[l].cels[0].unwrap_or_else(|| self.doc.new_drawing())
                } else {
                    self.doc.new_drawing()
                };
                self.doc.drawings[id].px = io::fit_picture(px, pw, ph, w, h);
                self.doc.layers[l].cels[i] = Some(id);
                count += 1;
            }
        }
        self.needs_full = true;
        self.status = trf("{} images importées dans un nouveau calque.", &[&count]);
    }

    fn finish_shape(&mut self, a: (f32, f32), b: (f32, f32)) {
        let Some(id) = self.doc.target_drawing() else { return };
        let (w, h) = (self.doc.w, self.doc.h);
        let mut st = Stroke::begin(id, &self.doc.drawings[id].px, w, h, self.tool, self.color, self.set());
        for poly in brush::shape_points(self.tool, a, b) {
            for p in poly {
                st.to(p.0, p.1, &mut self.doc.drawings[id].px);
            }
        }
        self.doc.push_pixel_undo(id, st.dirty, &st.before);
        self.needs_full = true;
    }

    fn pick(&mut self, px: (f32, f32)) {
        if px.0 < 0.0 || px.1 < 0.0 {
            return;
        }
        let (x, y) = (px.0 as usize, px.1 as usize);
        if x < self.doc.w && y < self.doc.h && !self.comp.is_empty() {
            let i = (y * self.doc.w + x) * 4;
            self.color = [self.comp[i], self.comp[i + 1], self.comp[i + 2]];
        }
    }

    fn keys(&mut self, ctx: &egui::Context) {
        use crate::keymap::Action as A;
        // waiting for a new shortcut in the shortcuts window
        if let Some((action, add)) = self.capture {
            let got = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, m)) = got {
                if key == Key::Escape && !m.any() {
                    self.capture = None;
                } else {
                    let b = crate::keymap::Binding {
                        key,
                        command: m.command,
                        shift: m.shift,
                        alt: m.alt,
                    };
                    if let Some(prev) = self.keymap.assign(action, b, add) {
                        self.status = trf("{} était utilisé par « {} » : il ne l'est plus.", &[&b.display(), &prev.label()]);
                    }
                    crate::keymap::save(&self.keymap);
                    self.capture = None;
                }
            }
            return;
        }
        let typing = ctx.wants_keyboard_input();
        let km = self.keymap.clone();
        let hit = |a: A| ctx.input(|i| km.pressed(i, a));
        // shortcuts with Ctrl / Cmd work even while typing in a field
        let with_cmd = |a: A| km.get(a).iter().any(|b| b.command);
        let go = |a: A| (!typing || with_cmd(a)) && hit(a);
        if go(A::Undo) && self.stroke.is_none() {
            self.cancel_floating();
            self.doc.undo();
            self.needs_full = true;
        }
        if go(A::Redo) && self.stroke.is_none() {
            self.doc.redo();
            self.needs_full = true;
        }
        if go(A::Save) {
            self.save(false);
        }
        if go(A::Open) {
            self.open_dialog();
        }
        if typing {
            return;
        }
        let (mut copy, mut cut, mut paste) = (false, false, false);
        ctx.input(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Copy => copy = true,
                    egui::Event::Cut => cut = true,
                    egui::Event::Paste(_) => paste = true,
                    // a picture alone on the clipboard gives no paste event, only the key
                    egui::Event::Key {
                        key: Key::V,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.command && !modifiers.shift => paste = true,
                    _ => {}
                }
            }
        });
        if copy {
            self.copy_selection();
        }
        if cut {
            self.cut_selection();
        }
        if paste {
            self.paste();
        }
        if go(A::SelectAll) {
            self.select_all();
        }
        if go(A::Deselect) {
            self.deselect();
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.floating.is_some() {
                self.cancel_floating();
            } else {
                self.deselect();
            }
        }
        if go(A::PlayStop) {
            if self.floating.is_some() {
                self.commit_floating();
            } else {
                self.toggle_play();
            }
        } else if self.floating.is_some() && ctx.input(|i| i.key_pressed(Key::Enter)) {
            self.commit_floating();
        }
        for (a, n) in [(A::Room1, 0), (A::Room2, 1), (A::Room3, 2), (A::Room4, 3)] {
            if go(a) {
                self.switch_room(n);
            }
        }
        if go(A::PinLeft) || go(A::PinAll) {
            let r = &mut self.rooms[self.room];
            r.left_pinned = !r.left_pinned;
            self.layout_dirty = true;
        }
        if go(A::PinRight) || go(A::PinAll) {
            let r = &mut self.rooms[self.room];
            r.right_pinned = !r.right_pinned;
            self.layout_dirty = true;
        }
        if go(A::TimelineZoomIn) {
            self.tl_zoom(1.25);
        }
        if go(A::TimelineZoomOut) {
            self.tl_zoom(0.8);
        }
        if go(A::TimelineFit) {
            self.tl_fit = Some(TlFit::Clip);
        }
        if go(A::TimelineTimecode) {
            self.tl_timecode = !self.tl_timecode;
        }
        if go(A::MarkImage) {
            let (l, f) = (self.doc.current_layer, self.doc.frame);
            let m = if self.doc.mark_at(l, f).is_some() { 0 } else { self.mark_pick };
            self.set_mark(l, f, m);
        }
        if go(A::NextMark) {
            self.step_mark(true);
        }
        if go(A::PrevMark) {
            self.step_mark(false);
        }
        if go(A::InsertTweens) {
            self.insert_tweens();
        }
        if go(A::MoveCellsLeft) {
            self.move_cells(false);
        }
        if go(A::MoveCellsRight) {
            self.move_cells(true);
        }
        if go(A::ToggleBookmark) {
            let f = self.doc.frame;
            self.doc.toggle_bookmark(f);
        }
        if go(A::NextBookmark) {
            if let Some(b) = self.doc.next_bookmark(self.doc.frame) {
                self.doc.frame = b;
            }
        }
        if go(A::PrevBookmark) {
            if let Some(b) = self.doc.prev_bookmark(self.doc.frame) {
                self.doc.frame = b;
            }
        }
        if go(A::MarkIn) {
            self.doc.mark_in = Some(self.doc.frame);
        }
        if go(A::MarkOut) {
            self.doc.mark_out = Some(self.doc.frame);
        }
        if go(A::ClearMarks) {
            self.doc.mark_in = None;
            self.doc.mark_out = None;
        }
        if go(A::PrevFrame) {
            self.step_frame(-1);
        }
        if go(A::NextFrame) {
            self.step_frame(1);
        }
        if go(A::LayerUp) && self.doc.current_layer + 1 < self.doc.layers.len() {
            self.doc.focus_layer(self.doc.current_layer + 1);
        }
        if go(A::LayerDown) && self.doc.current_layer > 0 {
            self.doc.focus_layer(self.doc.current_layer - 1);
        }
        if go(A::ShiftLayerLater) {
            self.shift_layers(1);
        }
        if go(A::ShiftLayerEarlier) {
            self.shift_layers(-1);
        }
        if go(A::SplitLayer) {
            self.split_layer(self.doc.frame);
        }
        if go(A::CollapseLayer) {
            self.collapse_layers(false, None);
        }
        if go(A::UndoMark) {
            self.toggle_undo_mark();
        }
        if go(A::UndoPrevMark) {
            self.go_undo_mark(false);
        }
        if go(A::UndoNextMark) {
            self.go_undo_mark(true);
        }
        let (flip_prev, flip_next) = ctx.input(|i| (km.down(i, A::FlipPrev), km.down(i, A::FlipNext)));
        if flip_prev || flip_next {
            let origin = *self.flip_origin.get_or_insert(self.doc.frame);
            let l = self.doc.current_layer;
            let keys = self.doc.keys(l);
            let here = self.doc.key_index(l, origin).unwrap_or(0);
            let pos = keys.iter().position(|k| k.0 == here).unwrap_or(0);
            let target = if flip_prev {
                pos.checked_sub(1)
            } else {
                (pos + 1 < keys.len()).then_some(pos + 1)
            };
            if let Some(t) = target {
                self.doc.frame = keys[t].0;
            }
        } else if let Some(o) = self.flip_origin.take() {
            self.doc.frame = o.min(self.doc.frames - 1);
        }
        if go(A::FirstFrame) {
            self.doc.frame = 0;
        }
        if go(A::LastFrame) {
            self.doc.frame = self.doc.frames - 1;
        }
        if go(A::Delete) {
            if self.selection.is_some() {
                self.delete_selection();
            } else {
                self.clear_drawing();
            }
        }
        if go(A::SwapColors) {
            std::mem::swap(&mut self.color, &mut self.color2);
        }
        if go(A::ToggleOnion) {
            self.doc.light.active = !self.doc.light.active;
            self.needs_full = true;
        }
        if go(A::BrushSmaller) {
            let s = self.set_mut();
            s.radius = (s.radius / 1.2).max(0.5);
        }
        if go(A::BrushBigger) {
            let s = self.set_mut();
            s.radius = (s.radius * 1.2).min(300.0);
        }
        if go(A::NewFrame) {
            self.doc.add_blank_after();
            self.needs_full = true;
        }
        if go(A::DuplicateFrame) {
            self.doc.duplicate_after();
            self.needs_full = true;
        }
        if go(A::HoldMore) {
            self.doc.expose_more();
            self.needs_full = true;
        }
        if go(A::HoldLess) {
            self.doc.expose_less();
            self.needs_full = true;
        }
        if go(A::NewLayer) {
            self.doc.add_layer();
            self.needs_full = true;
        }
        if go(A::CameraView) {
            self.cam_view = !self.cam_view;
            self.needs_full = true;
            self.fit_pending = true;
            self.tex = None;
        }
        if go(A::RotateViewLeft) {
            self.view_rot = (self.view_rot + 15.0) % 360.0;
        }
        if go(A::RotateViewRight) {
            self.view_rot = (self.view_rot - 15.0) % 360.0;
        }
        if go(A::RotateViewReset) {
            self.view_rot = 0.0;
        }
        if go(A::MirrorView) {
            self.view_flip = !self.view_flip;
        }
        if go(A::PreserveTransparency) {
            self.preserve = !self.preserve;
        }
        if go(A::LineSmoothing) {
            self.smooth_on = !self.smooth_on;
        }
        if go(A::ZoomFit) {
            self.fit_pending = true;
        }
        if go(A::Zoom100) {
            self.zoom = 1.0;
            self.pan = Vec2::ZERO;
        }
        for t in ALL_TOOLS {
            if go(A::Tool(t)) {
                self.tool = t;
            }
        }
    }

    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts {
            return;
        }
        let mut open = true;
        let mut reset_all = false;
        let (mut export_cfg, mut import_cfg) = (false, false);
        let mut changed = false;
        egui::Window::new(tr("Raccourcis clavier"))
            .open(&mut open)
            .default_size([560.0, 560.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.shortcut_filter)
                            .hint_text(tr("Chercher…"))
                            .desired_width(180.0),
                    );
                    if ui.button(tr("Tout par défaut")).clicked() {
                        reset_all = true;
                    }
                    if ui.button(tr("Exporter (.cfg)…")).clicked() {
                        export_cfg = true;
                    }
                    if ui.button(tr("Importer (.cfg)…")).clicked() {
                        import_cfg = true;
                    }
                });
                if let Some((a, _)) = self.capture {
                    ui.colored_label(
                        Color32::from_rgb(255, 200, 80),
                        trf("Appuyez sur la nouvelle touche pour « {} » (Échap : annuler)", &[&a.label()]),
                    );
                }
                ui.separator();
                let filter = self.shortcut_filter.to_lowercase();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("shortcuts").striped(true).num_columns(3).show(ui, |ui| {
                        for a in crate::keymap::all_actions() {
                            let label = a.label();
                            let keys: Vec<String> = self.keymap.get(a).iter().map(|b| b.display()).collect();
                            let keys_text = if keys.is_empty() { "—".to_string() } else { keys.join(", ") };
                            if !filter.is_empty() && !label.to_lowercase().contains(&filter) && !keys_text.to_lowercase().contains(&filter)
                            {
                                continue;
                            }
                            ui.label(label);
                            ui.monospace(keys_text);
                            ui.horizontal(|ui| {
                                if ui.small_button(tr("Changer")).clicked() {
                                    self.capture = Some((a, false));
                                }
                                if ui.small_button("+").on_hover_text(tr("Ajouter un autre raccourci")).clicked() {
                                    self.capture = Some((a, true));
                                }
                                if ui.small_button(tr("Aucun")).clicked() {
                                    self.keymap.map.insert(a, vec![]);
                                    changed = true;
                                }
                                if ui.small_button(tr("Par défaut")).clicked() {
                                    let d = crate::keymap::KeyMap::default().get(a).to_vec();
                                    for b in d {
                                        self.keymap.assign(a, b, false);
                                    }
                                    changed = true;
                                }
                            });
                            ui.end_row();
                        }
                    });
                });
                ui.separator();
                ui.small(tr(
                    "Copier, couper et coller suivent les raccourcis du système. Les raccourcis sont enregistrés tout de suite.",
                ));
            });
        self.show_shortcuts = open;
        if !open {
            self.capture = None;
        }
        if reset_all {
            self.keymap = crate::keymap::KeyMap::default();
            changed = true;
        }
        if changed {
            crate::keymap::save(&self.keymap);
        }
        if export_cfg {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter(tr("Raccourcis"), &["cfg"])
                .set_file_name("shortcuts.cfg")
                .save_file()
            {
                match std::fs::write(&p, self.keymap.to_cfg()) {
                    Ok(()) => self.status = trf("Enregistré : {}", &[&p.display()]),
                    Err(e) => self.status = trf("Échec de l'enregistrement : {}", &[&e]),
                }
            }
        }
        if import_cfg {
            if let Some(p) = rfd::FileDialog::new().add_filter(tr("Raccourcis"), &["cfg"]).pick_file() {
                match std::fs::read_to_string(&p) {
                    Ok(t) => {
                        let (km, skipped) = crate::keymap::KeyMap::from_cfg(&t, false);
                        self.keymap = km;
                        crate::keymap::save(&self.keymap);
                        self.status = trf("Raccourcis importés ({} lignes ignorées).", &[&skipped]);
                    }
                    Err(e) => self.status = trf("Échec de l'ouverture : {}", &[&e]),
                }
            }
        }
    }

    fn export_settings(&mut self) {
        if let Some(mut p) = rfd::FileDialog::new()
            .add_filter(tr("Réglages"), &["json"])
            .set_file_name("toonlight-settings.json")
            .save_file()
        {
            if p.extension().is_none() {
                p.set_extension("json");
            }
            match crate::settings::export(&p, &self.keymap, &self.presets, self.theme, &self.doc.palette) {
                Ok(()) => self.status = trf("Réglages exportés : {}", &[&p.display()]),
                Err(e) => self.status = trf("Échec de l'enregistrement : {}", &[&e]),
            }
        }
    }

    fn import_settings(&mut self, ctx: &egui::Context) {
        let Some(p) = rfd::FileDialog::new().add_filter(tr("Réglages"), &["json"]).pick_file() else {
            return;
        };
        match crate::settings::import(&p) {
            Ok(b) => {
                if let Some(l) = b.language.as_deref().and_then(i18n::Lang::from_code) {
                    i18n::set_lang(l);
                    i18n::save_lang(l);
                }
                if let Some(t) = b.theme.filter(|t| *t < THEMES.len()) {
                    self.theme = t;
                    apply_theme(ctx, &THEMES[t]);
                }
                if let Some(cfg) = &b.shortcuts {
                    self.keymap = crate::keymap::KeyMap::from_cfg(cfg, true).0;
                    crate::keymap::save(&self.keymap);
                }
                if let Some(pr) = b.presets {
                    self.presets = pr;
                    crate::custom::save_presets(&self.presets);
                }
                if let Some(pal) = b.palette {
                    self.doc.palette = pal;
                }
                self.status = trf("Réglages importés : {}", &[&p.display()]);
            }
            Err(e) => self.status = trf("Échec de l'ouverture : {}", &[&e]),
        }
    }

    fn new_dialog(&mut self, ctx: &egui::Context) {
        let mut close = false;
        let mut create = None;
        if let Some(d) = &mut self.new_dialog {
            egui::Window::new(tr("Nouveau projet"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    egui::ComboBox::from_label(tr("Modèle"))
                        .selected_text(
                            d.template
                                .map(|t| tr(crate::doc::TEMPLATES[t].name))
                                .unwrap_or(tr("Aucun (un calque)")),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut d.template, None, tr("Aucun (un calque)"));
                            for (i, t) in crate::doc::TEMPLATES.iter().enumerate() {
                                if ui
                                    .selectable_value(
                                        &mut d.template,
                                        Some(i),
                                        format!("{} ({}×{}, {} {})", tr(t.name), t.w, t.h, t.fps, tr("i/s")),
                                    )
                                    .clicked()
                                {
                                    d.w = t.w;
                                    d.h = t.h;
                                    d.fps = t.fps;
                                }
                            }
                        });
                    egui::ComboBox::from_label(tr("Format"))
                        .selected_text(PAGE_SIZES[d.preset].0)
                        .show_ui(ui, |ui| {
                            for (i, p) in PAGE_SIZES.iter().enumerate() {
                                if ui
                                    .selectable_value(&mut d.preset, i, format!("{} ({}×{}, {} {})", p.0, p.1, p.2, p.3, tr("i/s")))
                                    .clicked()
                                {
                                    d.w = p.1;
                                    d.h = p.2;
                                    d.fps = p.3;
                                }
                            }
                        });
                    ui.horizontal(|ui| {
                        ui.label(tr("Largeur"));
                        ui.add(egui::DragValue::new(&mut d.w).range(16..=4096));
                        ui.label(tr("Hauteur"));
                        ui.add(egui::DragValue::new(&mut d.h).range(16..=4096));
                    });
                    ui.horizontal(|ui| {
                        ui.label(tr("Cadence"));
                        ui.add(
                            egui::DragValue::new(&mut d.fps)
                                .range(1.0..=120.0)
                                .speed(0.1)
                                .suffix(format!(" {}", tr("i/s"))),
                        );
                    });
                    ui.small(tr("Le projet en cours sera fermé (pensez à l'enregistrer)."));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Créer")).clicked() {
                            create = Some((d.w, d.h, d.fps, d.template));
                        }
                        if ui.button(tr("Fermer")).clicked() {
                            close = true;
                        }
                    });
                });
        }
        if let Some((w, h, fps, template)) = create {
            let doc = match template {
                Some(t) => {
                    let t = &crate::doc::TEMPLATES[t];
                    // the size and rate typed in the window win over the template's
                    let custom = crate::doc::Template {
                        w,
                        h,
                        fps,
                        ..crate::doc::Template { ..*t }
                    };
                    Doc::from_template(&custom)
                }
                None => Doc::new(w, h, fps),
            };
            self.replace_doc(doc);
            self.clips = vec![ClipSlot::new(format!("{} 1", tr("Plan")), None)];
            self.active_clip = 0;
            self.path = None;
            self.status = trf("Nouveau projet {} × {}", &[&w, &h]);
            close = true;
        }
        if close {
            self.new_dialog = None;
        }
    }

    fn playback(&mut self, ctx: &egui::Context) {
        if !self.playing {
            self.scrub();
            return;
        }
        let (lo, hi) = self.doc.play_range();
        let dt = 1.0 / self.doc.fps.max(1.0);
        let el = self.last_tick.elapsed().as_secs_f32();
        if el >= dt {
            let steps = (el / dt).floor() as i32;
            self.last_tick += std::time::Duration::from_secs_f32(steps as f32 * dt);
            let next = self.doc.frame + steps as usize;
            if !self.looping && next > hi {
                self.doc.frame = hi;
                self.playing = false;
                self.player.stop();
                self.needs_full = true;
            } else {
                let before = self.doc.frame;
                if self.play_film && self.clips.len() > 1 && self.doc.frame + steps as usize >= self.doc.frames {
                    let next = (self.active_clip + 1) % self.clips.len();
                    self.switch_clip(next);
                    self.doc.frame = 0;
                    self.playing = true;
                    self.last_tick = Instant::now();
                    self.start_sound();
                    self.needs_full = true;
                } else if next > hi {
                    // loop inside the play range and start the sound again
                    self.doc.frame = lo + (next - lo) % (hi - lo + 1);
                    let _ = before;
                    self.start_sound();
                } else {
                    self.doc.frame = next;
                }
            }
            // get the effect pictures of the coming frames ready, several at a time
            if self.playing {
                let ahead: Vec<usize> = (1..=16)
                    .map(|k| {
                        let f = self.doc.frame + k;
                        if f > hi {
                            lo + (f - hi - 1) % (hi - lo + 1)
                        } else {
                            f
                        }
                    })
                    .collect();
                self.doc.prewarm_fx(&ahead);
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f32(
            (dt - self.last_tick.elapsed().as_secs_f32()).max(0.001),
        ));
    }

    fn screenshot(&mut self, ctx: &egui::Context) {
        let mut saved = None;
        ctx.input(|i| {
            for e in &i.events {
                if let egui::Event::Screenshot { image, .. } = e {
                    saved = Some(image.clone());
                }
            }
        });
        match &mut self.shot {
            Shot::None => return,
            Shot::Counting(p, n) => {
                *n += 1;
                if *n >= 120 {
                    let p = p.clone();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                    self.shot = Shot::Waiting(p);
                }
            }
            Shot::Waiting(p) => {
                if let Some(img) = saved {
                    let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                    if let Err(e) = io::save_png(p, &bytes, img.size[0], img.size[1]) {
                        eprintln!("screenshot failed: {e}");
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    self.shot = Shot::None;
                }
            }
        }
        ctx.request_repaint();
    }
}

impl eframe::App for AnimApp {
    /// A normal close: the crash-recovery copy isn't needed any more.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.bridge.is_some() {
            crate::mcp::remove_bridge_file();
        }
        crate::layout::save(&self.rooms, self.room);
        if !self.doc.dirty {
            self.remove_recovery();
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_audio();
        self.keys(ctx);
        if let Some(f) = &self.floating {
            if self.doc.drawing_at(self.doc.current_layer, self.doc.frame) != Some(f.id) {
                self.commit_floating();
            }
        }
        self.playback(ctx);
        self.menu(ctx);
        if self.rooms[self.room].timeline {
            self.timeline(ctx);
        }
        self.column(ctx, Side::Left);
        self.column(ctx, Side::Right);
        self.floating_panels(ctx);
        self.stretch_window(ctx);
        self.note_window(ctx);
        self.group_editor(ctx);
        self.recovery_tick();
        self.bridge_tick();
        self.prefs_window(ctx);
        self.history_window(ctx);
        self.fx_stack_window(ctx);
        self.recovery_window(ctx);
        self.canvas(ctx);
        self.apply_layout_ops(ctx);
        self.new_dialog(ctx);
        self.ai_window(ctx);
        self.xsheet_window(ctx);
        self.fx_window(ctx);
        self.storyboard_window(ctx);
        if std::mem::take(&mut self.export_board_pending) {
            self.export_storyboard();
        }
        self.shortcuts_window(ctx);
        self.watch_file(ctx);
        self.screenshot(ctx);
    }
}

/// A sound track's colour in the timeline: 0 = the usual blue, 1-12 = that colour group's colour.
/// The camera's speed profile as a curve to edit: the time from left to right, the part of the
/// move done from bottom to top. Returns true when the profile changed.
fn profile_editor(ui: &mut egui::Ui, cam: &mut crate::camera::Camera) -> bool {
    let width = ui.available_width().clamp(80.0, 180.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, (width * 0.7).round()), Sense::click_and_drag());
    let inner = rect.shrink(6.0);
    let to_screen = |p: [f32; 2]| pos2(inner.left() + p[0] * inner.width(), inner.bottom() - p[1] * inner.height());
    let from_screen = |p: Pos2| {
        [
            ((p.x - inner.left()) / inner.width()).clamp(0.0, 1.0),
            ((inner.bottom() - p.y) / inner.height()).clamp(0.0, 1.0),
        ]
    };
    let near = |pts: &[[f32; 2]], at: Pos2| pts.iter().position(|p| to_screen(*p).distance(at) <= 9.0);
    // the point being dragged, kept from one screen refresh to the next
    let drag_id = resp.id.with("point");
    let mut changed = false;
    if let Some(at) = resp.interact_pointer_pos() {
        if resp.secondary_clicked() {
            if let Some(i) = near(&cam.profile, at) {
                cam.profile.remove(i);
                changed = true;
            }
        } else if resp.drag_started() || resp.clicked() {
            // take the point under the pointer, or make one there
            let mut taken = near(&cam.profile, at);
            if taken.is_none() {
                let p = from_screen(at);
                let room = (0.02..=0.98).contains(&p[0]) && cam.profile.iter().all(|q| (q[0] - p[0]).abs() >= 0.03);
                if room {
                    let i = cam.profile.iter().position(|q| q[0] > p[0]).unwrap_or(cam.profile.len());
                    cam.profile.insert(i, p);
                    taken = Some(i);
                    changed = true;
                }
            }
            ui.data_mut(|d| d.insert_temp(drag_id, taken));
        }
        if resp.dragged() {
            let taken: Option<usize> = ui.data(|d| d.get_temp::<Option<usize>>(drag_id)).flatten();
            if let Some(i) = taken.filter(|i| *i < cam.profile.len()) {
                // a point stays between its neighbours in time
                let lo = if i == 0 { 0.02 } else { cam.profile[i - 1][0] + 0.03 };
                let hi = if i + 1 == cam.profile.len() {
                    0.98
                } else {
                    cam.profile[i + 1][0] - 0.03
                };
                if lo <= hi {
                    let p = from_screen(at);
                    cam.profile[i] = [p[0].clamp(lo, hi), p[1]];
                    changed = true;
                }
            }
        }
    }
    let p = ui.painter_at(rect);
    let v = ui.visuals();
    p.rect_filled(rect, 3.0, v.extreme_bg_color);
    let faint = Line::new(1.0_f32, v.weak_text_color().gamma_multiply(0.25));
    for k in 0..=4 {
        let t = k as f32 / 4.0;
        p.line_segment([to_screen([t, 0.0]), to_screen([t, 1.0])], faint);
        p.line_segment([to_screen([0.0, t]), to_screen([1.0, t])], faint);
    }
    // the even speed, for comparison, then the profile
    p.line_segment([to_screen([0.0, 0.0]), to_screen([1.0, 1.0])], faint);
    let curve: Vec<Pos2> = (0..=48)
        .map(|k| {
            let t = k as f32 / 48.0;
            to_screen([t, cam.profile_at(t)])
        })
        .collect();
    let accent = v.selection.bg_fill;
    p.add(egui::Shape::line(curve, Line::new(2.0_f32, accent)));
    for q in [[0.0, 0.0], [1.0, 1.0]] {
        p.circle_filled(to_screen(q), 3.0, v.weak_text_color());
    }
    for q in &cam.profile {
        p.circle_filled(to_screen(*q), 4.5, v.strong_text_color());
        p.circle_stroke(to_screen(*q), 4.5, Line::new(1.0_f32, accent));
    }
    changed
}

/// What a layer's small timeline picture was made from: the drawing, the two picture counters
/// and its size.
type ThumbKey = (Option<usize>, u64, u64, usize, usize);

/// A drawing (straight RGBA, `w` × `h`) reduced to `tw` × `th` over the paper colour. Every pixel
/// counts (a box average), and faint coverage is strengthened so thin lines stay readable.
fn thumb_pixels(px: &[u8], w: usize, h: usize, tw: usize, th: usize, paper: [u8; 3]) -> Vec<u8> {
    let mut out = vec![255u8; tw * th * 4];
    for ty in 0..th {
        let y0 = ty * h / th;
        let y1 = ((ty + 1) * h / th).clamp(y0 + 1, h.max(y0 + 1));
        for tx in 0..tw {
            let x0 = tx * w / tw;
            let x1 = ((tx + 1) * w / tw).clamp(x0 + 1, w.max(x0 + 1));
            let (mut r, mut g, mut b, mut a) = (0u64, 0u64, 0u64, 0u64);
            for y in y0..y1.min(h) {
                for p in px[(y * w + x0) * 4..(y * w + x1.min(w)) * 4].chunks_exact(4) {
                    let al = p[3] as u64;
                    r += p[0] as u64 * al;
                    g += p[1] as u64 * al;
                    b += p[2] as u64 * al;
                    a += al;
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as f32 * 255.0;
            let cover = (a as f32 / n).clamp(0.0, 1.0).sqrt();
            let o = &mut out[(ty * tw + tx) * 4..(ty * tw + tx) * 4 + 3];
            for (k, sum) in [r, g, b].into_iter().enumerate() {
                let c = sum.checked_div(a).unwrap_or(0) as f32;
                o[k] = (paper[k] as f32 * (1.0 - cover) + c * cover).round() as u8;
            }
        }
    }
    out
}

fn sound_color(c: u8, groups: &[crate::doc::Group]) -> Color32 {
    match groups.get(c as usize) {
        Some(g) if c > 0 => c32(g.color),
        _ => Color32::from_rgb(90, 160, 220),
    }
}

/// Default fonts plus, when the computer has one, a font with Chinese and Japanese characters
/// (used as a fallback, so language names and CJK interfaces show properly).
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some((path, index)) = i18n::find_cjk_font(i18n::lang() == i18n::Lang::Ja) {
        if let Ok(bytes) = std::fs::read(&path) {
            let mut data = egui::FontData::from_owned(bytes);
            data.index = index;
            fonts.font_data.insert("cjk".into(), std::sync::Arc::new(data));
            for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(fam).or_default().push("cjk".into());
            }
        }
    }
    ctx.set_fonts(fonts);
}

fn modified(p: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn apply_theme(ctx: &egui::Context, t: &Theme) {
    let mut v = if t.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = t.panel;
    v.window_fill = t.panel;
    v.extreme_bg_color = t.text_bg;
    v.faint_bg_color = t.widget;
    v.override_text_color = Some(t.text);
    v.selection.bg_fill = t.selected;
    v.widgets.noninteractive.bg_fill = t.panel;
    v.widgets.noninteractive.bg_stroke = Line::new(1.0_f32, t.border);
    v.widgets.inactive.bg_fill = t.widget;
    v.widgets.inactive.weak_bg_fill = t.widget;
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.active.bg_fill = t.selected;
    v.widgets.active.weak_bg_fill = t.selected;
    v.widgets.open.weak_bg_fill = t.selected;
    ctx.set_visuals(v);
}

/// The project shown at start: a ball bouncing over a ground line, to show the light table at work.
fn sample_doc() -> Doc {
    let mut d = Doc::new(1920, 1080, 24.0);
    let (w, h) = (d.w, d.h);
    d.layers[0].name = tr("Décor").into();
    d.layers[0].onion = false;
    let ground = 860.0;
    {
        let id = d.layers[0].cels[0].unwrap();
        let pencil = Settings::new(4.0, 80.0, 100.0);
        let mut st = Stroke::begin(id, &d.drawings[id].px, w, h, Tool::Pencil, [70, 60, 50], pencil);
        let mut x = 120.0;
        while x <= 1800.0 {
            st.to(x, ground + (x * 0.01).sin() * 3.0, &mut d.drawings[id].px);
            x += 8.0;
        }
        let sun = Settings::new(90.0, 40.0, 60.0);
        let mut st = Stroke::begin(id, &d.drawings[id].px, w, h, Tool::Airbrush, [250, 200, 90], sun);
        st.to(1580.0, 230.0, &mut d.drawings[id].px);
        st.to(1584.0, 232.0, &mut d.drawings[id].px);
    }
    let n = 12;
    d.set_length(n);
    d.add_layer();
    d.layers[1].name = tr("Balle").into();
    let ball = d.layers[1].cels[0].unwrap();
    for f in 0..n {
        let id = if f == 0 { ball } else { d.new_drawing() };
        d.layers[1].cels[f] = Some(id);
        let t = f as f32 / (n - 1) as f32;
        let cx = 300.0 + t * 1300.0;
        let bounce = ((t * 2.0 * std::f32::consts::PI).sin()).abs();
        let r = 70.0;
        let squash: f32 = if bounce < 0.15 { 0.78 } else { 1.0 };
        let (rx, ry) = (r / squash.sqrt(), r * squash);
        let cy = ground - ry - 4.0 - bounce * 520.0;
        let pen = Settings::new(5.0, 100.0, 100.0);
        let mut st = Stroke::begin(id, &d.drawings[id].px, w, h, Tool::Pen, [25, 25, 35], pen);
        for i in 0..=120 {
            let a = i as f32 / 120.0 * std::f32::consts::TAU;
            st.to(cx + rx * a.cos(), cy + ry * a.sin(), &mut d.drawings[id].px);
        }
        brush::flood_fill(&mut d.drawings[id].px, w, h, cx as usize, cy as usize, [220, 60, 60], 15.0, 100.0);
    }
    d.frame = 4;
    d.current_layer = 1;
    d.clear_history();
    d.dirty = false;
    d
}

/// The settings of an effect, shared by the effect window and the layer's effect stack.
fn fx_params_ui(ui: &mut egui::Ui, fx: &mut Effect, color_a: [u8; 3]) -> bool {
    let mut changed = false;
    match fx {
        Effect::Blur { rx, ry } => {
            changed |= ui.add(egui::Slider::new(rx, 0.0..=200.0).text(tr("Rayon horizontal"))).changed();
            changed |= ui
                .add(egui::Slider::new(ry, 0.0..=200.0).text(tr("Rayon vertical (0 = pareil)")))
                .changed();
        }
        Effect::Mosaic { sx, sy } => {
            changed |= ui.add(egui::Slider::new(sx, 1..=200).text(tr("Largeur des carreaux"))).changed();
            changed |= ui.add(egui::Slider::new(sy, 1..=200).text(tr("Hauteur des carreaux"))).changed();
        }
        Effect::Noise { amount, mono } => {
            changed |= ui.add(egui::Slider::new(amount, 0.0..=100.0).text(tr("Quantité"))).changed();
            changed |= ui.checkbox(mono, tr("Monochrome")).changed();
        }
        Effect::Wave {
            amp_x,
            len_x,
            amp_y,
            len_y,
            phase,
        } => {
            changed |= ui
                .add(egui::Slider::new(amp_x, 0.0..=200.0).text(tr("Amplitude horizontale")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(len_x, 2.0..=1000.0).text(tr("Longueur d'onde horizontale")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(amp_y, 0.0..=200.0).text(tr("Amplitude verticale")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(len_y, 2.0..=1000.0).text(tr("Longueur d'onde verticale")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(phase, 0.0..=360.0).text(tr("Phase")).suffix("°"))
                .changed();
        }
        Effect::BrightnessContrast { brightness, contrast } => {
            changed |= ui
                .add(egui::Slider::new(brightness, -100.0..=100.0).text(tr("Luminosité")))
                .changed();
            changed |= ui.add(egui::Slider::new(contrast, -100.0..=100.0).text(tr("Contraste"))).changed();
        }
        Effect::HueSaturation {
            hue,
            saturation,
            lightness,
        } => {
            changed |= ui
                .add(egui::Slider::new(hue, -180.0..=180.0).text(tr("Teinte")).suffix("°"))
                .changed();
            changed |= ui
                .add(egui::Slider::new(saturation, -100.0..=100.0).text(tr("Saturation")))
                .changed();
            changed |= ui.add(egui::Slider::new(lightness, -100.0..=100.0).text(tr("Clarté"))).changed();
        }
        Effect::Invert => {
            ui.label(tr("Inverse les couleurs."));
        }
        Effect::ColorKey { color, tolerance } => {
            ui.horizontal(|ui| {
                ui.label(tr("Couleur à rendre transparente"));
                changed |= ui.color_edit_button_srgb(color).changed();
                if ui.small_button(tr("Couleur A")).clicked() {
                    *color = color_a;
                    changed = true;
                }
            });
            changed |= ui.add(egui::Slider::new(tolerance, 0.0..=100.0).text(tr("Tolérance"))).changed();
        }
        Effect::LumaKey { threshold, dark } => {
            changed |= ui.add(egui::Slider::new(threshold, 0.0..=100.0).text(tr("Seuil"))).changed();
            changed |= ui.checkbox(dark, tr("Rendre transparent le sombre (sinon le clair)")).changed();
        }
    }
    changed
}

/// Open a file in the system's browser / default program.
fn open_in_browser(p: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut c = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut c = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut c = std::process::Command::new("xdg-open");
    c.arg(p).spawn().map(|_| ())
}

/// Most frames a video import takes (ours: about 80 s at 24 fps, to keep memory reasonable).
const MAX_VIDEO_FRAMES: usize = 2000;

/// A quick fingerprint of a picture (size and a sample of its bytes).
fn clip_mark(px: &[u8], w: usize, h: usize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hs = std::collections::hash_map::DefaultHasher::new();
    (w, h).hash(&mut hs);
    let step = (px.len() / 4096).max(1);
    for b in px.iter().step_by(step) {
        b.hash(&mut hs);
    }
    hs.finish()
}
