//! One settings file to carry this program's set-up to another computer: shortcuts, tool presets
//! (with their custom brush pictures), interface language and theme, and the palette.

use crate::custom::Preset;
use crate::keymap::KeyMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const FORMAT: &str = "toonlight-settings";

#[derive(Serialize, Deserialize)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    pub app_version: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub theme: Option<usize>,
    /// The shortcuts in the `.cfg` text form.
    #[serde(default)]
    pub shortcuts: Option<String>,
    #[serde(default)]
    pub presets: Option<Vec<Preset>>,
    /// Custom brush pictures used by the presets: file name → PNG in base64.
    #[serde(default)]
    pub brush_images: BTreeMap<String, String>,
    #[serde(default)]
    pub palette: Option<Vec<[u8; 3]>>,
}

pub fn export(path: &Path, keymap: &KeyMap, presets: &[Preset], theme: usize, palette: &[[u8; 3]]) -> Result<(), String> {
    let mut images = BTreeMap::new();
    if let Some(dir) = crate::custom::presets_dir() {
        for p in presets {
            if let Some(img) = &p.image {
                if let Ok(bytes) = std::fs::read(dir.join(img)) {
                    images.insert(img.clone(), crate::mcp::base64(&bytes));
                }
            }
        }
    }
    let b = Bundle {
        format: FORMAT.into(),
        version: 1,
        app_version: env!("CARGO_PKG_VERSION").into(),
        language: Some(crate::i18n::lang().setting_name().into()),
        theme: Some(theme),
        shortcuts: Some(keymap.to_cfg()),
        presets: Some(presets.to_vec()),
        brush_images: images,
        palette: Some(palette.to_vec()),
    };
    let text = serde_json::to_string_pretty(&b).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Read a settings file. Brush pictures are written into the presets folder right away.
pub fn import(path: &Path) -> Result<Bundle, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let b: Bundle = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if b.format != FORMAT && b.format != "animcraft-settings" {
        return Err(crate::i18n::tr("Ce fichier ne contient pas de réglages.").into());
    }
    if let Some(dir) = crate::custom::presets_dir() {
        let _ = std::fs::create_dir_all(&dir);
        for (name, data) in &b.brush_images {
            // only plain file names
            if name.contains('/') || name.contains('\\') || name.contains("..") {
                continue;
            }
            if let Some(bytes) = base64_decode(data) {
                let _ = std::fs::write(dir.join(name), bytes);
            }
        }
    }
    Ok(b)
}

pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let bytes: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let pad = chunk.iter().filter(|c| **c == b'=').count();
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            let v = if *c == b'=' { 0 } else { val(*c)? };
            n |= v << (18 - 6 * i as u32);
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trip() {
        for s in [&b"M"[..], b"Ma", b"Man", b"hello world!"] {
            assert_eq!(base64_decode(&crate::mcp::base64(s)).unwrap(), s);
        }
    }

    #[test]
    fn settings_round_trip() {
        let dir = std::env::temp_dir().join(format!("toonlight-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("s.json");
        let mut km = KeyMap::default();
        km.assign(
            crate::keymap::Action::PlayStop,
            crate::keymap::Binding::plain(eframe::egui::Key::Space),
            false,
        );
        export(&p, &km, &crate::custom::default_presets(), 1, &[[1, 2, 3]]).unwrap();
        let b = import(&p).unwrap();
        assert_eq!(b.theme, Some(1));
        assert_eq!(b.palette.unwrap(), vec![[1, 2, 3]]);
        let (km2, _) = KeyMap::from_cfg(&b.shortcuts.unwrap(), true);
        assert_eq!(km2, km);
    }
}
