#![recursion_limit = "512"]
// No console window behind the app on Windows release builds (stdin / stdout pipes, used by
// `--mcp`, still work).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod app;
mod board;
mod brush;
mod camera;
mod custom;
mod doc;
mod fx;
mod i18n;
mod io;
mod keymap;
mod layout;
mod mcp;
mod prefs;
mod psd;
mod select;
mod settings;
mod sound;
mod tablet;
mod text;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--mcp") {
        // MCP server for AI assistants: JSON-RPC over stdin/stdout, no window.
        mcp::run();
        return Ok(());
    }
    let mut screenshot: Option<std::path::PathBuf> = None;
    let mut open: Option<std::path::PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" && i + 1 < args.len() {
            screenshot = Some(args[i + 1].clone().into());
            i += 2;
            continue;
        }
        if args[i] == "--shortcuts" {
            app::OPEN_SHORTCUTS.store(true, std::sync::atomic::Ordering::Relaxed);
            i += 1;
            continue;
        }
        if args[i] == "--storyboard" {
            app::OPEN_STORYBOARD.store(true, std::sync::atomic::Ordering::Relaxed);
            i += 1;
            continue;
        }
        if args[i] == "--groups" {
            app::OPEN_GROUPS.store(true, std::sync::atomic::Ordering::Relaxed);
            i += 1;
            continue;
        }
        if args[i] == "--history" {
            app::OPEN_HISTORY.store(true, std::sync::atomic::Ordering::Relaxed);
            i += 1;
            continue;
        }
        if args[i] == "--tool" && i + 1 < args.len() {
            *app::START_TOOL.lock().unwrap() = Some(args[i + 1].clone());
            i += 2;
            continue;
        }
        if args[i] == "--commands" && i + 1 < args.len() {
            *app::START_COMMANDS.lock().unwrap() = Some(args[i + 1].clone().into());
            i += 2;
            continue;
        }
        if args[i] == "--lang" && i + 1 < args.len() {
            if let Some(l) = i18n::Lang::from_code(&args[i + 1]) {
                i18n::set_override(l);
            }
            i += 2;
            continue;
        }
        if !args[i].starts_with("--") {
            open = Some(args[i].clone().into());
        }
        i += 1;
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Toonlight")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([960.0, 600.0]);
    if let Ok(img) = image::load_from_memory(include_bytes!("../assets/icon-256.png")) {
        let img = img.into_rgba8();
        let (w, h) = img.dimensions();
        viewport = viewport.with_icon(egui::IconData {
            rgba: img.into_raw(),
            width: w,
            height: h,
        });
    }
    let mut options = eframe::NativeOptions::default();
    if screenshot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
        #[cfg(target_os = "macos")]
        {
            options.event_loop_builder = Some(Box::new(|b| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                b.with_activate_ignoring_other_apps(false);
            }));
        }
    }
    options.viewport = viewport;
    options.run_and_return = true;

    eframe::run_native(
        "Toonlight",
        options,
        Box::new(move |cc| Ok(Box::new(app::AnimApp::new(cc, screenshot, open)))),
    )
}

use eframe::egui;
