//! Preferences kept between sessions (Edit > Preferences…), saved as `preferences.json` in the
//! settings folder.
//!
//! Backup copies follow the reference's save: with `[Main] AutoBackup` on, the file being replaced
//! is first copied to `<file name>.%Y%m%d.%H%M%S`, in `[Main] AutoBackupFolder`, or next to the
//! project when that is empty. Crash recovery is ours (the reference has an emergency save when it
//! crashes): while there are unsaved changes, the project is written to a recovery file every few
//! minutes, deleted when the app closes normally, and offered back at the next start.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Copy the previous version before saving over it (`[Main] AutoBackup`, zero-filled = off).
    pub auto_backup: bool,
    /// Where backup copies go; None = next to the project (`[Main] AutoBackupFolder` empty).
    pub backup_folder: Option<PathBuf>,
    /// Minutes between recovery copies while there are unsaved changes (0 = never). Ours.
    pub recovery_minutes: u32,
    /// Memory for undo, in MB (`[Undo] MaxSize`, 256 MB).
    pub undo_mb: u32,
    /// New projects: painting on a hold makes a drawing (`[LayerDefault] AutoBreakInstance`, on).
    pub auto_break: bool,
    /// New projects: painting after a layer's end stretches it (`[LayerDefault] AutoStretchLayer`, on).
    pub auto_stretch: bool,
    /// Sound while moving the playhead: 0 never, 1 while dragging, 2 always (`[Audio] ScrubMode`, 1).
    pub scrub_mode: u8,
    /// Length of each scrub sound, seconds (`[Audio] ScrubDuration`, 0.1).
    pub scrub_secs: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            auto_backup: false,
            backup_folder: None,
            recovery_minutes: 2,
            undo_mb: 256,
            auto_break: true,
            auto_stretch: true,
            scrub_mode: 1,
            scrub_secs: 0.1,
        }
    }
}

/// Undo memory in bytes, read by `Doc` (set from the preferences).
pub static UNDO_LIMIT: AtomicUsize = AtomicUsize::new(268_435_456);
/// Defaults for new documents (set from the preferences).
pub static AUTO_BREAK: AtomicBool = AtomicBool::new(true);
pub static AUTO_STRETCH: AtomicBool = AtomicBool::new(true);

impl Prefs {
    /// Make the values that documents read take effect.
    pub fn apply_globals(&self) {
        UNDO_LIMIT.store(self.undo_mb.max(16) as usize * 1_048_576, Ordering::Relaxed);
        AUTO_BREAK.store(self.auto_break, Ordering::Relaxed);
        AUTO_STRETCH.store(self.auto_stretch, Ordering::Relaxed);
    }
}

fn file() -> Option<PathBuf> {
    crate::i18n::config_dir().map(|d| d.join("preferences.json"))
}

pub fn load() -> Prefs {
    file()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(p: &Prefs) {
    if let Some(f) = file() {
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(b) = serde_json::to_vec_pretty(p) {
            let _ = std::fs::write(f, b);
        }
    }
}

/// Where the backup copy of `project` goes when it is saved over at `stamp`
/// ("walk.toonlight" → "walk.toonlight.20261115.143005").
pub fn backup_path(project: &Path, folder: Option<&Path>, stamp: &str) -> PathBuf {
    let name = project
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "animation.toonlight".into());
    let dir = match folder {
        Some(f) if !f.as_os_str().is_empty() => f.to_path_buf(),
        _ => project.parent().map(|d| d.to_path_buf()).unwrap_or_default(),
    };
    dir.join(format!("{name}.{stamp}"))
}

/// The local date and time as the reference writes it: "%Y%m%d.%H%M%S".
pub fn stamp_now() -> String {
    chrono::Local::now().format("%Y%m%d.%H%M%S").to_string()
}

/// Copy `project` to its backup place before it is saved over. Returns the copy's path.
pub fn back_up(project: &Path, folder: Option<&Path>) -> Result<PathBuf, String> {
    let to = backup_path(project, folder, &stamp_now());
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::copy(project, &to).map_err(|e| e.to_string())?;
    Ok(to)
}

/// The crash-recovery copy of the open project.
pub fn recovery_path() -> Option<PathBuf> {
    crate::i18n::config_dir().map(|d| d.join("recovery").join("recovery.toonlight"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names_follow_the_reference() {
        let p = Path::new("/films/walk.toonlight");
        assert_eq!(
            backup_path(p, None, "20261115.143005"),
            PathBuf::from("/films/walk.toonlight.20261115.143005")
        );
        assert_eq!(
            backup_path(p, Some(Path::new("/backups")), "20261115.143005"),
            PathBuf::from("/backups/walk.toonlight.20261115.143005")
        );
        assert_eq!(stamp_now().len(), 15);
    }

    #[test]
    fn missing_fields_take_the_defaults() {
        let p: Prefs = serde_json::from_str(r#"{"auto_backup": true}"#).unwrap();
        assert!(p.auto_backup);
        assert_eq!(p.undo_mb, 256);
        assert_eq!(p.scrub_mode, 1);
    }
}
