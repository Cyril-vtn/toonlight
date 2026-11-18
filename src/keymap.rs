//! Keyboard shortcuts the user can change, save, export and import.
//!
//! Exported like the reference's "Export Shortcuts": a `.cfg` text file with a
//! `[Keyboard]` section and one `key=action` line per shortcut, where the `=` key is written
//! "equal" and the `[` key "braquet".

use crate::brush::{Tool, ALL_TOOLS};
use eframe::egui::{self, Key, Modifiers};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Action {
    PlayStop,
    PrevFrame,
    NextFrame,
    FirstFrame,
    LastFrame,
    LayerUp,
    LayerDown,
    Delete,
    SwapColors,
    ToggleOnion,
    BrushSmaller,
    BrushBigger,
    FlipPrev,
    FlipNext,
    Undo,
    Redo,
    Save,
    Open,
    SelectAll,
    Deselect,
    NewFrame,
    DuplicateFrame,
    HoldMore,
    HoldLess,
    NewLayer,
    CameraView,
    ZoomFit,
    Zoom100,
    RotateViewLeft,
    RotateViewRight,
    RotateViewReset,
    MirrorView,
    PreserveTransparency,
    LineSmoothing,
    ToggleBookmark,
    NextBookmark,
    PrevBookmark,
    MarkIn,
    MarkOut,
    ClearMarks,
    MoveCellsLeft,
    MoveCellsRight,
    Room1,
    Room2,
    Room3,
    Room4,
    PinLeft,
    PinRight,
    PinAll,
    TimelineZoomIn,
    TimelineZoomOut,
    TimelineFit,
    TimelineTimecode,
    MarkImage,
    NextMark,
    PrevMark,
    InsertTweens,
    ShiftLayerLater,
    ShiftLayerEarlier,
    SplitLayer,
    CollapseLayer,
    UndoMark,
    UndoPrevMark,
    UndoNextMark,
    Tool(Tool),
}

pub fn all_actions() -> Vec<Action> {
    use Action::*;
    let mut v = vec![
        PlayStop,
        PrevFrame,
        NextFrame,
        FirstFrame,
        LastFrame,
        LayerUp,
        LayerDown,
        Delete,
        SwapColors,
        ToggleOnion,
        BrushSmaller,
        BrushBigger,
        FlipPrev,
        FlipNext,
        Undo,
        Redo,
        Save,
        Open,
        SelectAll,
        Deselect,
        NewFrame,
        DuplicateFrame,
        HoldMore,
        HoldLess,
        NewLayer,
        CameraView,
        ZoomFit,
        Zoom100,
        RotateViewLeft,
        RotateViewRight,
        RotateViewReset,
        MirrorView,
        PreserveTransparency,
        LineSmoothing,
        ToggleBookmark,
        NextBookmark,
        PrevBookmark,
        MarkIn,
        MarkOut,
        ClearMarks,
        MoveCellsLeft,
        MoveCellsRight,
        Room1,
        Room2,
        Room3,
        Room4,
        PinLeft,
        PinRight,
        PinAll,
        TimelineZoomIn,
        TimelineZoomOut,
        TimelineFit,
        TimelineTimecode,
        MarkImage,
        NextMark,
        PrevMark,
        InsertTweens,
        ShiftLayerLater,
        ShiftLayerEarlier,
        SplitLayer,
        CollapseLayer,
        UndoMark,
        UndoPrevMark,
        UndoNextMark,
    ];
    v.extend(ALL_TOOLS.iter().map(|t| Tool(*t)));
    v
}

impl Action {
    /// Stable name used in the saved files.
    pub fn id(self) -> String {
        use Action::*;
        match self {
            PlayStop => "play_stop".into(),
            PrevFrame => "previous_frame".into(),
            NextFrame => "next_frame".into(),
            FirstFrame => "first_frame".into(),
            LastFrame => "last_frame".into(),
            LayerUp => "layer_above".into(),
            LayerDown => "layer_below".into(),
            Delete => "delete".into(),
            SwapColors => "swap_colours".into(),
            ToggleOnion => "light_table".into(),
            BrushSmaller => "brush_smaller".into(),
            BrushBigger => "brush_bigger".into(),
            FlipPrev => "flip_previous".into(),
            FlipNext => "flip_next".into(),
            Undo => "undo".into(),
            Redo => "redo".into(),
            Save => "save".into(),
            Open => "open".into(),
            SelectAll => "select_all".into(),
            Deselect => "deselect".into(),
            NewFrame => "new_frame".into(),
            DuplicateFrame => "duplicate_frame".into(),
            HoldMore => "hold_more".into(),
            HoldLess => "hold_less".into(),
            NewLayer => "new_layer".into(),
            CameraView => "camera_view".into(),
            ZoomFit => "zoom_fit".into(),
            Zoom100 => "zoom_100".into(),
            RotateViewLeft => "view_rotate_left".into(),
            RotateViewRight => "view_rotate_right".into(),
            RotateViewReset => "view_rotate_reset".into(),
            MirrorView => "view_mirror".into(),
            PreserveTransparency => "preserve_transparency".into(),
            LineSmoothing => "line_smoothing".into(),
            ToggleBookmark => "bookmark_toggle".into(),
            NextBookmark => "bookmark_next".into(),
            PrevBookmark => "bookmark_previous".into(),
            MarkIn => "mark_in".into(),
            MarkOut => "mark_out".into(),
            ClearMarks => "marks_clear".into(),
            MoveCellsLeft => "cells_move_left".into(),
            MoveCellsRight => "cells_move_right".into(),
            Room1 => "room_1".into(),
            Room2 => "room_2".into(),
            Room3 => "room_3".into(),
            Room4 => "room_4".into(),
            PinLeft => "bin_left_toggle".into(),
            PinRight => "bin_right_toggle".into(),
            PinAll => "bins_toggle_all".into(),
            TimelineZoomIn => "timeline_zoom_in".into(),
            TimelineZoomOut => "timeline_zoom_out".into(),
            TimelineFit => "timeline_fit".into(),
            TimelineTimecode => "timeline_timecode".into(),
            MarkImage => "image_mark_toggle".into(),
            NextMark => "image_mark_next".into(),
            PrevMark => "image_mark_previous".into(),
            InsertTweens => "insert_tweens".into(),
            ShiftLayerLater => "layer_shift_later".into(),
            ShiftLayerEarlier => "layer_shift_earlier".into(),
            SplitLayer => "layer_split".into(),
            CollapseLayer => "layer_collapse_toggle".into(),
            UndoMark => "undo_mark_toggle".into(),
            UndoPrevMark => "undo_mark_previous".into(),
            UndoNextMark => "undo_mark_next".into(),
            Tool(t) => format!("tool_{}", format!("{t:?}").to_lowercase()),
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        all_actions().into_iter().find(|a| a.id() == id)
    }

    pub fn label(self) -> String {
        use crate::i18n::tr;
        use Action::*;
        match self {
            PlayStop => tr("Lire / arrêter").into(),
            PrevFrame => tr("Image précédente").into(),
            NextFrame => tr("Image suivante").into(),
            FirstFrame => tr("Première image").into(),
            LastFrame => tr("Dernière image").into(),
            LayerUp => tr("Calque au-dessus").into(),
            LayerDown => tr("Calque en dessous").into(),
            Delete => tr("Effacer la sélection ou le dessin").into(),
            SwapColors => tr("Échanger les couleurs").into(),
            ToggleOnion => tr("Table lumineuse").into(),
            BrushSmaller => tr("Pinceau plus petit").into(),
            BrushBigger => tr("Pinceau plus grand").into(),
            FlipPrev => tr("Feuilleter : dessin précédent (maintenir)").into(),
            FlipNext => tr("Feuilleter : dessin suivant (maintenir)").into(),
            Undo => tr("Annuler").into(),
            Redo => tr("Rétablir").into(),
            Save => tr("Enregistrer").into(),
            Open => tr("Ouvrir…").into(),
            SelectAll => tr("Tout sélectionner").into(),
            Deselect => tr("Désélectionner").into(),
            NewFrame => tr("Nouvelle image après").into(),
            DuplicateFrame => tr("Dupliquer l'image").into(),
            HoldMore => tr("Exposition +").into(),
            HoldLess => tr("Exposition −").into(),
            NewLayer => tr("Nouveau calque").into(),
            CameraView => tr("Voir à travers la caméra").into(),
            ZoomFit => tr("Ajuster à la fenêtre").into(),
            Zoom100 => tr("Taille réelle (100 %)").into(),
            RotateViewLeft => tr("Tourner la vue de +15°").into(),
            RotateViewRight => tr("Tourner la vue de −15°").into(),
            RotateViewReset => tr("Remettre la vue droite").into(),
            MirrorView => tr("Vue en miroir").into(),
            PreserveTransparency => tr("Préserver la transparence").into(),
            LineSmoothing => tr("Lissage du trait").into(),
            ToggleBookmark => tr("Signet ici (ajouter / retirer)").into(),
            NextBookmark => tr("Signet suivant").into(),
            PrevBookmark => tr("Signet précédent").into(),
            MarkIn => tr("Début de lecture ici").into(),
            MarkOut => tr("Fin de lecture ici").into(),
            ClearMarks => tr("Effacer début et fin de lecture").into(),
            MoveCellsLeft => tr("Décaler les cellules à gauche").into(),
            MoveCellsRight => tr("Décaler les cellules à droite").into(),
            Room1 => crate::i18n::trf("Espace de travail {}", &[&1]),
            Room2 => crate::i18n::trf("Espace de travail {}", &[&2]),
            Room3 => crate::i18n::trf("Espace de travail {}", &[&3]),
            Room4 => crate::i18n::trf("Espace de travail {}", &[&4]),
            PinLeft => tr("Épingler / replier la colonne de gauche").into(),
            PinRight => tr("Épingler / replier la colonne de droite").into(),
            PinAll => tr("Épingler / replier les deux colonnes").into(),
            TimelineZoomIn => tr("Zoom avant de la timeline").into(),
            TimelineZoomOut => tr("Zoom arrière de la timeline").into(),
            TimelineFit => tr("Timeline : voir tout le plan").into(),
            TimelineTimecode => tr("Timeline : images ou timecode").into(),
            MarkImage => tr("Marquer l'image (ajouter / retirer)").into(),
            NextMark => tr("Image marquée suivante").into(),
            PrevMark => tr("Image marquée précédente").into(),
            InsertTweens => tr("Insérer des intervalles").into(),
            ShiftLayerLater => tr("Décaler le calque d'une image plus tard").into(),
            ShiftLayerEarlier => tr("Décaler le calque d'une image plus tôt").into(),
            SplitLayer => tr("Scinder le calque ici").into(),
            CollapseLayer => tr("Replier / déplier le calque").into(),
            UndoMark => tr("Marque d'annulation (poser / retirer)").into(),
            UndoPrevMark => tr("Revenir à la marque d'annulation précédente").into(),
            UndoNextMark => tr("Aller à la marque d'annulation suivante").into(),
            Tool(t) => crate::i18n::trf("Outil : {}", &[&t.label()]),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Binding {
    pub key: Key,
    /// Ctrl on Windows / Linux, Cmd on macOS.
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Binding {
    pub const fn plain(key: Key) -> Binding {
        Binding {
            key,
            command: false,
            shift: false,
            alt: false,
        }
    }
    pub const fn cmd(key: Key) -> Binding {
        Binding {
            key,
            command: true,
            shift: false,
            alt: false,
        }
    }

    pub fn modifiers(&self) -> Modifiers {
        let mut m = Modifiers::NONE;
        if self.command {
            m |= Modifiers::COMMAND;
        }
        if self.shift {
            m |= Modifiers::SHIFT;
        }
        if self.alt {
            m |= Modifiers::ALT;
        }
        m
    }

    /// "Ctrl+Shift+Z" (the saved form; "Ctrl" means Cmd on macOS).
    pub fn text(&self) -> String {
        let mut s = String::new();
        if self.command {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        s.push_str(self.key.name());
        s
    }

    /// For the screen: the key's symbol ("[" rather than "OpenBracket"), "Cmd" on macOS.
    pub fn display(&self) -> String {
        let t = self.text();
        let t = match t.rfind(self.key.name()) {
            Some(i) => format!("{}{}", &t[..i], self.key.symbol_or_name()),
            None => t,
        };
        if cfg!(target_os = "macos") {
            t.replace("Ctrl+", "Cmd+").replace("Alt+", "Option+")
        } else {
            t
        }
    }

    pub fn parse(s: &str) -> Option<Binding> {
        let mut b = Binding {
            key: Key::A,
            command: false,
            shift: false,
            alt: false,
        };
        let s = s.trim();
        // the "+" key itself: "+" or "Ctrl++"
        let (s, plus) = if s == "+" {
            ("", true)
        } else if let Some(r) = s.strip_suffix("++") {
            (r, true)
        } else {
            (s, false)
        };
        if plus {
            for m in s.split('+').filter(|m| !m.is_empty()) {
                match m.trim().to_lowercase().as_str() {
                    "ctrl" | "cmd" | "command" => b.command = true,
                    "shift" | "maj" => b.shift = true,
                    "alt" | "option" => b.alt = true,
                    _ => return None,
                }
            }
            b.key = Key::Plus;
            return Some(b);
        }
        let parts: Vec<&str> = s.split('+').collect();
        let (last, mods) = parts.split_last()?;
        for m in mods {
            match m.trim().to_lowercase().as_str() {
                "ctrl" | "cmd" | "command" => b.command = true,
                "shift" | "maj" => b.shift = true,
                "alt" | "option" => b.alt = true,
                _ => return None,
            }
        }
        // the "+" key itself ("Ctrl++") leaves an empty last part
        let name = if last.is_empty() { "Plus" } else { last.trim() };
        b.key = Key::from_name(name).or_else(|| match name.to_lowercase().as_str() {
            "equal" => Some(Key::Equals),
            "braquet" => Some(Key::OpenBracket),
            _ => None,
        })?;
        Some(b)
    }

    /// Pressed this frame (exactly these modifiers).
    pub fn pressed(&self, i: &egui::InputState) -> bool {
        i.key_pressed(self.key) && i.modifiers.matches_exact(self.modifiers())
    }

    /// Held down now.
    pub fn down(&self, i: &egui::InputState) -> bool {
        i.key_down(self.key) && i.modifiers.command == self.command && i.modifiers.alt == self.alt
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct KeyMap {
    pub map: BTreeMap<Action, Vec<Binding>>,
}

impl Default for KeyMap {
    /// The shortcuts this build started with.
    fn default() -> Self {
        use Action::*;
        let mut map: BTreeMap<Action, Vec<Binding>> = BTreeMap::new();
        let mut set = |a: Action, b: Vec<Binding>| {
            map.insert(a, b);
        };
        set(PlayStop, vec![Binding::plain(Key::Enter)]);
        set(PrevFrame, vec![Binding::plain(Key::ArrowLeft)]);
        set(NextFrame, vec![Binding::plain(Key::ArrowRight)]);
        set(FirstFrame, vec![Binding::plain(Key::Home)]);
        set(LastFrame, vec![Binding::plain(Key::End)]);
        set(LayerUp, vec![Binding::plain(Key::ArrowUp)]);
        set(LayerDown, vec![Binding::plain(Key::ArrowDown)]);
        set(Delete, vec![Binding::plain(Key::Delete), Binding::plain(Key::Backspace)]);
        set(SwapColors, vec![Binding::plain(Key::X)]);
        set(ToggleOnion, vec![Binding::plain(Key::T)]);
        set(BrushSmaller, vec![Binding::plain(Key::OpenBracket)]);
        set(BrushBigger, vec![Binding::plain(Key::CloseBracket)]);
        set(FlipPrev, vec![Binding::plain(Key::Comma)]);
        set(FlipNext, vec![Binding::plain(Key::Period)]);
        set(Undo, vec![Binding::cmd(Key::Z)]);
        set(
            Redo,
            vec![
                Binding {
                    key: Key::Z,
                    command: true,
                    shift: true,
                    alt: false,
                },
                Binding::cmd(Key::Y),
            ],
        );
        set(Save, vec![Binding::cmd(Key::S)]);
        set(Open, vec![Binding::cmd(Key::O)]);
        set(SelectAll, vec![Binding::cmd(Key::A)]);
        set(Deselect, vec![Binding::cmd(Key::D)]);
        set(NewFrame, vec![]);
        set(DuplicateFrame, vec![]);
        set(HoldMore, vec![]);
        set(HoldLess, vec![]);
        set(NewLayer, vec![]);
        set(CameraView, vec![]);
        set(ZoomFit, vec![]);
        set(Zoom100, vec![]);
        set(
            RotateViewLeft,
            vec![Binding {
                key: Key::ArrowLeft,
                command: false,
                shift: true,
                alt: false,
            }],
        );
        set(
            RotateViewRight,
            vec![Binding {
                key: Key::ArrowRight,
                command: false,
                shift: true,
                alt: false,
            }],
        );
        set(
            RotateViewReset,
            vec![Binding {
                key: Key::ArrowDown,
                command: false,
                shift: true,
                alt: false,
            }],
        );
        set(
            MirrorView,
            vec![Binding {
                key: Key::H,
                command: false,
                shift: true,
                alt: false,
            }],
        );
        set(
            PreserveTransparency,
            vec![Binding {
                key: Key::Slash,
                command: false,
                shift: false,
                alt: false,
            }],
        );
        set(
            LineSmoothing,
            vec![Binding {
                key: Key::L,
                command: false,
                shift: true,
                alt: false,
            }],
        );
        set(
            ToggleBookmark,
            vec![Binding {
                key: Key::B,
                command: false,
                shift: true,
                alt: false,
            }],
        );
        set(
            NextBookmark,
            vec![Binding {
                key: Key::Period,
                command: false,
                shift: true,
                alt: false,
            }],
        );
        set(
            PrevBookmark,
            vec![Binding {
                key: Key::Comma,
                command: false,
                shift: true,
                alt: false,
            }],
        );
        set(
            MarkIn,
            vec![Binding {
                key: Key::I,
                command: false,
                shift: true,
                alt: false,
            }],
        );
        set(
            MarkOut,
            vec![Binding {
                key: Key::O,
                command: false,
                shift: true,
                alt: false,
            }],
        );
        set(ClearMarks, vec![]);
        set(
            MoveCellsLeft,
            vec![Binding {
                key: Key::ArrowLeft,
                command: false,
                shift: false,
                alt: true,
            }],
        );
        set(
            MoveCellsRight,
            vec![Binding {
                key: Key::ArrowRight,
                command: false,
                shift: false,
                alt: true,
            }],
        );
        for a in [Room1, Room2, Room3, Room4, PinLeft, PinRight, PinAll] {
            set(a, vec![]);
        }
        for a in [
            TimelineZoomIn,
            TimelineZoomOut,
            TimelineFit,
            TimelineTimecode,
            MarkImage,
            NextMark,
            PrevMark,
            InsertTweens,
            ShiftLayerLater,
            ShiftLayerEarlier,
            SplitLayer,
            CollapseLayer,
            UndoMark,
            UndoPrevMark,
            UndoNextMark,
        ] {
            set(a, vec![]);
        }
        for t in ALL_TOOLS {
            let b = Key::from_name(t.key()).map(Binding::plain).into_iter().collect();
            set(Tool(t), b);
        }
        KeyMap { map }
    }
}

impl KeyMap {
    pub fn get(&self, a: Action) -> &[Binding] {
        self.map.get(&a).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn first_text(&self, a: Action) -> String {
        self.get(a).first().map(|b| b.display()).unwrap_or_default()
    }

    /// Which action already uses this binding.
    pub fn owner(&self, b: &Binding) -> Option<Action> {
        self.map.iter().find(|(_, v)| v.contains(b)).map(|(a, _)| *a)
    }

    /// Give `b` to `a` (taking it from any other action). `add` keeps a's other shortcuts.
    pub fn assign(&mut self, a: Action, b: Binding, add: bool) -> Option<Action> {
        let taken = self.owner(&b).filter(|o| *o != a);
        for v in self.map.values_mut() {
            v.retain(|x| x != &b);
        }
        let e = self.map.entry(a).or_default();
        if !add {
            e.clear();
        }
        e.push(b);
        taken
    }

    pub fn pressed(&self, i: &egui::InputState, a: Action) -> bool {
        self.get(a).iter().any(|b| b.pressed(i))
    }

    pub fn down(&self, i: &egui::InputState, a: Action) -> bool {
        self.get(a).iter().any(|b| b.down(i))
    }

    /// The reference-style text: `[Keyboard]` then `key=action` lines.
    pub fn to_cfg(&self) -> String {
        let mut s = String::from("[Keyboard]\n");
        for (a, bs) in &self.map {
            if bs.is_empty() {
                continue;
            }
            for b in bs {
                // like the reference: the "=" key is written "equal" and "[" "braquet"
                let name = match b.key {
                    Key::Equals => "equal".to_string(),
                    Key::OpenBracket => "braquet".to_string(),
                    k => k.name().to_string(),
                };
                let mut key = String::new();
                if b.command {
                    key.push_str("Ctrl+");
                }
                if b.alt {
                    key.push_str("Alt+");
                }
                if b.shift {
                    key.push_str("Shift+");
                }
                key.push_str(&name);
                s.push_str(&format!("{key}={}\n", a.id()));
            }
        }
        s
    }

    /// Read a `.cfg` file written by `to_cfg`. Actions it doesn't mention keep their default
    /// shortcut unless `replace_all`; unknown lines are skipped and counted.
    pub fn from_cfg(text: &str, replace_all: bool) -> (KeyMap, usize) {
        let mut km = KeyMap::default();
        if replace_all {
            for v in km.map.values_mut() {
                v.clear();
            }
        }
        let mut seen: Vec<Action> = vec![];
        let mut skipped = 0;
        for line in text.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('[') && l.ends_with(']') || l.starts_with('#') || l.starts_with(';') {
                continue;
            }
            // the key part may itself be "=" ("equal"), so split at the last "="
            let Some(pos) = l.rfind('=') else {
                skipped += 1;
                continue;
            };
            let (k, a) = (&l[..pos], &l[pos + 1..]);
            match (Binding::parse(k), Action::from_id(a.trim())) {
                (Some(b), Some(act)) => {
                    if !seen.contains(&act) {
                        km.map.entry(act).or_default().clear();
                        seen.push(act);
                    }
                    km.assign(act, b, true);
                }
                _ => skipped += 1,
            }
        }
        (km, skipped)
    }
}

pub fn shortcuts_file() -> Option<std::path::PathBuf> {
    crate::i18n::config_dir().map(|d| d.join("shortcuts.cfg"))
}

pub fn load() -> KeyMap {
    shortcuts_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| KeyMap::from_cfg(&t, true).0)
        .unwrap_or_default()
}

pub fn save(km: &KeyMap) {
    if let Some(p) = shortcuts_file() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, km.to_cfg());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_round_trip() {
        let mut km = KeyMap::default();
        km.assign(Action::PlayStop, Binding::plain(Key::Space), false);
        km.assign(Action::BrushBigger, Binding::plain(Key::Equals), false);
        let text = km.to_cfg();
        assert!(text.starts_with("[Keyboard]\n"));
        assert!(text.contains("Space=play_stop"));
        assert!(text.contains("equal=brush_bigger"));
        let (back, skipped) = KeyMap::from_cfg(&text, true);
        assert_eq!(skipped, 0);
        assert_eq!(back, km);
    }

    #[test]
    fn assigning_takes_the_key_from_another_action() {
        let mut km = KeyMap::default();
        let taken = km.assign(Action::Save, Binding::plain(Key::X), false);
        assert_eq!(taken, Some(Action::SwapColors));
        assert!(km.get(Action::SwapColors).is_empty());
        assert_eq!(
            Binding::parse("Ctrl+Shift+Z").unwrap(),
            Binding {
                key: Key::Z,
                command: true,
                shift: true,
                alt: false
            }
        );
    }
}
