//! Rooms (workspaces) and movable panels.
//!
//! The four default rooms follow the reference's `Default Config/config.ini` ([Room-1] … [Room-4]):
//! a pinned left column ("bin 1": tools, colour picker, tool settings, 185 px wide), a second column
//! ("bin 2": tool presets first), the layer timeline docked at the bottom (340 px), and, in Room 4,
//! a floating colour-correction panel with the timeline folded to 29 px. Rooms 2 and 4 have their
//! columns unpinned (`reqbin1-pinned=0`, `reqbin2-pinned=0`): they slide out when the pointer reaches
//! the edge. Room names are the reference's ("Room 1"…, interface texts 75010-75013).

use crate::i18n::{tr, trf};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A panel of the window that can be docked in a column or float.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum PanelId {
    Tools,
    Colours,
    Settings,
    Presets,
    Light,
    Sound,
    Project,
    Shortcuts,
    Correction,
    Remote,
    Groups,
}

pub const ALL_PANELS: [PanelId; 11] = [
    PanelId::Tools,
    PanelId::Colours,
    PanelId::Settings,
    PanelId::Presets,
    PanelId::Light,
    PanelId::Sound,
    PanelId::Project,
    PanelId::Shortcuts,
    PanelId::Correction,
    PanelId::Remote,
    PanelId::Groups,
];

impl PanelId {
    pub fn title(self) -> &'static str {
        match self {
            PanelId::Tools => tr("Outils"),
            PanelId::Colours => tr("Couleurs"),
            PanelId::Settings => tr("Réglages de l'outil"),
            PanelId::Presets => tr("Préréglages"),
            PanelId::Light => tr("Table lumineuse"),
            PanelId::Sound => tr("Son"),
            PanelId::Project => tr("Projet"),
            PanelId::Shortcuts => tr("Raccourcis"),
            PanelId::Correction => tr("Correction des couleurs"),
            PanelId::Remote => tr("Télécommande"),
            PanelId::Groups => tr("Groupes de couleur"),
        }
    }

    /// The column a panel goes back to when it is shown again.
    pub fn home(self) -> Side {
        match self {
            PanelId::Tools | PanelId::Colours | PanelId::Settings => Side::Left,
            _ => Side::Right,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Side {
    Left,
    Right,
}

/// One workspace: where each panel is and how the columns and the timeline behave.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub name: String,
    pub left: Vec<PanelId>,
    pub right: Vec<PanelId>,
    /// Floating panels and the top-left corner of their window.
    pub floating: Vec<(PanelId, [f32; 2])>,
    pub left_pinned: bool,
    pub right_pinned: bool,
    pub left_width: f32,
    pub right_width: f32,
    pub timeline: bool,
    pub timeline_pinned: bool,
    pub timeline_height: f32,
    /// This room's keyboard shortcuts (`.cfg` text); the reference keeps one "Keyboard-" section
    /// per room and swaps it when the room changes. None = the shortcuts in use.
    #[serde(default)]
    pub keys: Option<String>,
}

/// Width of the reference's columns (`*-winw=185`).
pub const COLUMN_WIDTH: f32 = 185.0;
/// Height of the reference's docked timeline in Room 1 (`layerpanel-dockheight=340`).
pub const TIMELINE_HEIGHT: f32 = 340.0;

impl Room {
    /// The reference's default room `n` (0-based).
    pub fn default_room(n: usize) -> Room {
        let mut r = Room {
            name: trf("Espace {}", &[&(n + 1)]),
            left: vec![PanelId::Tools, PanelId::Colours, PanelId::Settings],
            right: vec![
                PanelId::Presets,
                PanelId::Light,
                PanelId::Sound,
                PanelId::Project,
                PanelId::Shortcuts,
            ],
            floating: vec![],
            left_pinned: true,
            right_pinned: false,
            left_width: COLUMN_WIDTH,
            right_width: COLUMN_WIDTH,
            timeline: true,
            timeline_pinned: true,
            timeline_height: TIMELINE_HEIGHT,
            keys: None,
        };
        match n {
            1 => {
                // Room-2: both columns and the timeline unpinned (they slide out)
                r.left_pinned = false;
                r.timeline_pinned = false;
            }
            3 => {
                // Room-4: columns unpinned, timeline folded to 29 px, colour correction floating at 38, 79
                r.left_pinned = false;
                r.timeline_height = 29.0;
                r.floating.push((PanelId::Correction, [38.0, 79.0]));
            }
            _ => {}
        }
        r
    }

    pub fn defaults() -> Vec<Room> {
        (0..4).map(Room::default_room).collect()
    }

    pub fn is_shown(&self, p: PanelId) -> bool {
        self.left.contains(&p) || self.right.contains(&p) || self.floating.iter().any(|f| f.0 == p)
    }

    /// Take a panel out of the room (hide it).
    pub fn remove(&mut self, p: PanelId) {
        self.left.retain(|x| *x != p);
        self.right.retain(|x| *x != p);
        self.floating.retain(|f| f.0 != p);
    }

    /// Dock a panel in a column at `index` (moved from wherever it was).
    pub fn place(&mut self, p: PanelId, side: Side, index: usize) {
        let mut index = index;
        let list = if side == Side::Left { &self.left } else { &self.right };
        if let Some(old) = list.iter().position(|x| *x == p) {
            if old < index {
                index -= 1;
            }
        }
        self.remove(p);
        let list = if side == Side::Left { &mut self.left } else { &mut self.right };
        list.insert(index.min(list.len()), p);
    }

    /// Make a panel float with its window's corner at `pos`.
    pub fn float(&mut self, p: PanelId, pos: [f32; 2]) {
        self.remove(p);
        self.floating.push((p, pos));
    }

    /// Show a hidden panel again, at the end of its usual column.
    pub fn show(&mut self, p: PanelId) {
        if self.is_shown(p) {
            return;
        }
        let side = p.home();
        let n = if side == Side::Left { self.left.len() } else { self.right.len() };
        self.place(p, side, n);
    }
}

/// Version of the saved rooms file; files of other versions are ignored (the defaults are used).
const ROOMS_VERSION: u32 = 3;

#[derive(Serialize, Deserialize)]
struct RoomsFile {
    format: String,
    version: u32,
    current: usize,
    rooms: Vec<Room>,
}

#[derive(Serialize, Deserialize)]
struct RoomFile {
    format: String,
    version: u32,
    room: Room,
}

fn rooms_path() -> Option<std::path::PathBuf> {
    crate::i18n::config_dir().map(|d| d.join("rooms.json"))
}

/// The rooms saved last time, or the reference's four.
pub fn load() -> (Vec<Room>, usize) {
    let read = rooms_path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<RoomsFile>(&b).ok());
    match read {
        Some(f) if f.rooms.len() == 4 && f.version == ROOMS_VERSION => {
            let cur = f.current.min(3);
            (f.rooms, cur)
        }
        _ => (Room::defaults(), 0),
    }
}

pub fn save(rooms: &[Room], current: usize) {
    if let Some(p) = rooms_path() {
        let f = RoomsFile {
            format: "toonlight-rooms".into(),
            version: ROOMS_VERSION,
            current,
            rooms: rooms.to_vec(),
        };
        if let Ok(b) = serde_json::to_vec_pretty(&f) {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, b);
        }
    }
}

/// Write one room to a file ("Export Room").
pub fn export_room(path: &Path, room: &Room) -> Result<(), String> {
    let f = RoomFile {
        format: "toonlight-room".into(),
        version: 1,
        room: room.clone(),
    };
    let b = serde_json::to_vec_pretty(&f).map_err(|e| e.to_string())?;
    std::fs::write(path, b).map_err(|e| e.to_string())
}

/// Read a room written by `export_room` ("Import Room").
pub fn import_room(path: &Path) -> Result<Room, String> {
    let b = std::fs::read(path).map_err(|e| e.to_string())?;
    let f: RoomFile = serde_json::from_slice(&b).map_err(|e| e.to_string())?;
    if f.format != "toonlight-room" && f.format != "animcraft-room" {
        return Err(tr("Ce fichier n'est pas un espace de travail.").into());
    }
    Ok(f.room)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rooms_follow_the_reference() {
        let r = Room::defaults();
        assert_eq!(r[0].left, vec![PanelId::Tools, PanelId::Colours, PanelId::Settings]);
        assert_eq!(r[0].right[0], PanelId::Presets);
        assert!(r[0].left_pinned && !r[0].right_pinned && r[0].timeline_pinned);
        assert!(!r[1].left_pinned && !r[1].timeline_pinned);
        assert_eq!(
            Room {
                name: String::new(),
                ..r[0].clone()
            },
            Room {
                name: String::new(),
                ..r[2].clone()
            }
        );
        assert_eq!(r[3].timeline_height, 29.0);
        assert_eq!(r[3].floating, vec![(PanelId::Correction, [38.0, 79.0])]);
    }

    #[test]
    fn panels_move_between_columns() {
        let mut r = Room::default_room(0);
        r.place(PanelId::Presets, Side::Left, 1);
        assert_eq!(r.left, vec![PanelId::Tools, PanelId::Presets, PanelId::Colours, PanelId::Settings]);
        assert!(!r.right.contains(&PanelId::Presets));
        // moving down inside the same column
        r.place(PanelId::Tools, Side::Left, 3);
        assert_eq!(r.left, vec![PanelId::Presets, PanelId::Colours, PanelId::Tools, PanelId::Settings]);
        r.float(PanelId::Tools, [10.0, 20.0]);
        assert!(!r.left.contains(&PanelId::Tools) && r.is_shown(PanelId::Tools));
        r.remove(PanelId::Tools);
        assert!(!r.is_shown(PanelId::Tools));
        r.show(PanelId::Tools);
        assert_eq!(r.left.last(), Some(&PanelId::Tools));
    }

    #[test]
    fn a_room_survives_export_and_import() {
        let dir = std::env::temp_dir().join(format!("toonlight-room-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("room.json");
        let r = Room::default_room(3);
        export_room(&p, &r).unwrap();
        assert_eq!(import_room(&p).unwrap(), r);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
