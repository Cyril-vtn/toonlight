//! MCP server (Model Context Protocol) so an AI assistant — Claude, or any other MCP client — can
//! create and edit animations: `toonlight --mcp` speaks JSON-RPC 2.0 over stdin/stdout, one
//! message per line, and works on one project in memory, without a window.
//!
//! Saving the project to the file the window has open makes the window reload it (when it has no
//! unsaved changes), so an assistant's work shows up live.

use crate::brush::{self, Settings, Stroke, Tool};
use crate::doc::{Blend, Doc, Paper, Rect, ALL_BLENDS, ALL_PAPERS};
use crate::io;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;

pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub(crate) struct Server {
    pub(crate) doc: Doc,
    pub(crate) path: Option<PathBuf>,
    pub(crate) brush: Option<std::sync::Arc<crate::custom::CustomBrush>>,
    /// Shots of the project: name, notes, and the animation (None for the active one, in `doc`).
    pub(crate) clips: Vec<(String, String, Option<Doc>)>,
    pub(crate) active: usize,
    /// A layer copied with layer_copy (copy / cut), for paste.
    pub(crate) layer_clip: Option<crate::doc::LayerCopy>,
}

impl Server {
    pub(crate) fn new(doc: Doc) -> Server {
        Server {
            doc,
            path: None,
            brush: None,
            clips: vec![("Shot 1".into(), String::new(), None)],
            active: 0,
            layer_clip: None,
        }
    }

    fn select_clip(&mut self, i: usize) -> Result<(), String> {
        if i >= self.clips.len() {
            return Err(format!("shot {i} does not exist (0..{})", self.clips.len() - 1));
        }
        if i != self.active {
            let next = self.clips[i].2.take().ok_or("shot missing")?;
            let cur = std::mem::replace(&mut self.doc, next);
            self.clips[self.active].2 = Some(cur);
            self.active = i;
        }
        Ok(())
    }

    fn clip_list(&self) -> Value {
        let list: Vec<Value> = self
            .clips
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let d = if i == self.active { &self.doc } else { c.2.as_ref().unwrap() };
                json!({"index": i, "name": c.0, "notes": c.1, "frames": d.frames, "active": i == self.active})
            })
            .collect();
        json!({"shots": list})
    }
}

pub fn run() {
    crate::prefs::load_default_groups();
    let standalone = std::env::args().any(|a| a == "--standalone");
    // with a window open, every command goes to its project (the "bridge"); otherwise this
    // process keeps its own project in memory
    let mut bridge = if standalone { None } else { bridge_connect() };
    let mut reader = bridge.as_ref().and_then(|b| b.try_clone().ok()).map(std::io::BufReader::new);
    let mut server: Option<Server> = None;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let (Some(b), Some(r)) = (bridge.as_mut(), reader.as_mut()) {
            let mut answer = String::new();
            if writeln!(b, "{line}").is_ok() && r.read_line(&mut answer).map(|n| n > 0).unwrap_or(false) {
                let answer = answer.trim_end();
                if !answer.is_empty() {
                    let _ = writeln!(stdout, "{answer}");
                    let _ = stdout.flush();
                }
                continue;
            }
            // the window was closed: carry on alone
            bridge = None;
            reader = None;
        }
        let srv = server.get_or_insert_with(|| Server::new(Doc::new(1920, 1080, 24.0)));
        if let Some(r) = srv.handle_line(&line) {
            let _ = writeln!(stdout, "{r}");
            let _ = stdout.flush();
        }
    }
}

impl Server {
    /// Answer one JSON-RPC line (None for notifications).
    pub(crate) fn handle_line(&mut self, line: &str) -> Option<String> {
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(msg) => self.handle(&msg),
            Err(e) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}})),
        };
        reply.map(|r| r.to_string())
    }
}

/// Where a running window says how to reach it: `{"port": …, "token": …}` in the settings folder.
fn bridge_file() -> Option<PathBuf> {
    crate::i18n::config_dir().map(|d| d.join("bridge.json"))
}

pub fn remove_bridge_file() {
    if let Some(p) = bridge_file() {
        let _ = std::fs::remove_file(p);
    }
}

/// Requests from AI clients to the open window: a JSON-RPC line and where to send the answer.
pub type BridgeRequests = std::sync::mpsc::Receiver<(String, std::sync::mpsc::Sender<String>)>;

/// The window listens on this computer only (127.0.0.1, a free port) so `toonlight --mcp` can pass
/// AI commands to the open project. A random code in the bridge file must start each connection.
pub fn start_bridge(wake: impl Fn() + Send + Sync + 'static) -> Option<BridgeRequests> {
    use std::hash::{Hash, Hasher};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    let mut hs = std::collections::hash_map::DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hs);
    std::process::id().hash(&mut hs);
    port.hash(&mut hs);
    let token = format!("{:016x}{:08x}", hs.finish(), std::process::id());
    let file = bridge_file()?;
    if let Some(d) = file.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    std::fs::write(&file, json!({"port": port, "token": token, "pid": std::process::id()}).to_string()).ok()?;
    let (tx, rx) = std::sync::mpsc::channel::<(String, std::sync::mpsc::Sender<String>)>();
    let wake = std::sync::Arc::new(wake);
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let tx = tx.clone();
            let token = token.clone();
            let wake = wake.clone();
            std::thread::spawn(move || {
                let Ok(read) = conn.try_clone() else { return };
                let mut lines = std::io::BufReader::new(read).lines();
                let mut out = conn;
                if lines.next().and_then(|l| l.ok()).map(|l| l.trim().to_string()) != Some(token) {
                    return;
                }
                for line in lines {
                    let Ok(line) = line else { break };
                    let (rtx, rrx) = std::sync::mpsc::channel();
                    if tx.send((line, rtx)).is_err() {
                        break;
                    }
                    wake();
                    let Ok(answer) = rrx.recv() else { break };
                    if writeln!(out, "{answer}").is_err() {
                        break;
                    }
                }
            });
        }
    });
    Some(rx)
}

/// Connect to an open window, if there is one.
fn bridge_connect() -> Option<std::net::TcpStream> {
    let text = std::fs::read_to_string(bridge_file()?).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let port = v.get("port")?.as_u64()? as u16;
    let token = v.get("token")?.as_str()?.to_string();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(400)).ok()?;
    writeln!(s, "{token}").ok()?;
    Some(s)
}

fn ok(id: &Value, result: Value) -> Option<Value> {
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn err(id: &Value, code: i64, message: &str) -> Option<Value> {
    Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}))
}

fn text(s: impl Into<String>) -> Value {
    json!({"content": [{"type": "text", "text": s.into()}]})
}

impl Server {
    fn handle(&mut self, msg: &Value) -> Option<Value> {
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = msg.get("id").cloned();
        // notifications (no id) get no answer
        let id = id?;
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        match method {
            "initialize" => {
                let version = params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or(PROTOCOL_VERSION)
                    .to_string();
                ok(
                    &id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": {"tools": {"listChanged": false}},
                        "serverInfo": {"name": "toonlight", "title": "Toonlight", "version": env!("CARGO_PKG_VERSION")},
                        "instructions": "Toonlight is a 2D frame-by-frame animation program. When the Toonlight window is open, these tools work on the project open in it, live (each change can be undone there with Ctrl/Cmd+Z); otherwise on a project in memory. One project is open at a time \
                            (1920x1080 at 24 fps to start). Coordinates are canvas pixels from the top-left. Layers are numbered \
                            from 0 at the bottom; frames from 0. A frame without its own drawing holds the previous one. \
                            Use render_frame to see the result, project_save to write an .toonlight file the window can open."
                    }),
                )
            }
            "ping" => ok(&id, json!({})),
            "tools/list" => ok(&id, json!({"tools": tools()})),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match self.call(name, &args) {
                    Ok(v) => ok(&id, v),
                    Err(e) => ok(&id, json!({"content": [{"type": "text", "text": e}], "isError": true})),
                }
            }
            "resources/list" => ok(&id, json!({"resources": []})),
            "prompts/list" => ok(&id, json!({"prompts": []})),
            _ => err(&id, -32601, &format!("unknown method {method}")),
        }
    }

    fn layer_arg(&self, a: &Value) -> Result<usize, String> {
        match a.get("layer").and_then(Value::as_u64) {
            Some(l) if (l as usize) < self.doc.layers.len() => Ok(l as usize),
            Some(l) => Err(format!("layer {l} does not exist (0..{})", self.doc.layers.len() - 1)),
            None => Ok(self.doc.current_layer),
        }
    }

    /// The layers named by `layers` (a list of indexes), else the one named by `layer`.
    fn layers_arg(&self, a: &Value) -> Result<Vec<usize>, String> {
        let Some(list) = a.get("layers").and_then(Value::as_array) else {
            return Ok(vec![self.layer_arg(a)?]);
        };
        let mut out = vec![];
        for v in list {
            let l = v.as_u64().ok_or("layers is a list of layer indexes")? as usize;
            if l >= self.doc.layers.len() {
                return Err(format!("layer {l} does not exist (0..{})", self.doc.layers.len() - 1));
            }
            out.push(l);
        }
        Ok(out)
    }

    fn frame_arg(&self, a: &Value) -> Result<usize, String> {
        match a.get("frame").and_then(Value::as_u64) {
            Some(f) if (f as usize) < self.doc.frames => Ok(f as usize),
            Some(f) => Err(format!("frame {f} does not exist (0..{})", self.doc.frames - 1)),
            None => Ok(self.doc.frame),
        }
    }

    /// Select the layer and frame given (or keep the current ones).
    fn place(&mut self, a: &Value) -> Result<(), String> {
        self.doc.current_layer = self.layer_arg(a)?;
        self.doc.frame = self.frame_arg(a)?;
        Ok(())
    }

    /// The undo history: how many steps are done, how many there are, and the marked states.
    fn history_json(&self) -> Value {
        let (done, undone) = self.doc.history_marks();
        let marks: Vec<usize> = done
            .iter()
            .chain(&undone)
            .enumerate()
            .filter(|(_, m)| **m)
            .map(|(i, _)| i + 1)
            .collect();
        json!({"step": done.len(), "steps": done.len() + undone.len(), "marks": marks})
    }

    fn info(&self) -> Value {
        let layers: Vec<Value> = self
            .doc
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| {
                json!({
                    "index": i,
                    "name": l.name,
                    "visible": l.visible,
                    "locked": l.locked,
                    "stencil": l.stencil,
                    "opacity": (l.opacity * 100.0).round(),
                    "blend": format!("{:?}", l.blend).to_lowercase(),
                    "color_group": self.doc.group_of(l.group).map(|i| json!(i)).unwrap_or_else(|| json!(hex(l.group))),
                    "light_table": l.onion,
                    "selected": l.selected,
                    "collapsed": l.collapsed,
                    "drawings_start_at_frames": self.doc.keys(i).iter().map(|k| k.0).collect::<Vec<_>>(),
                    "drawing_names": l.names.iter().filter_map(|(id, n)| l.cels.iter().position(|c| *c == Some(*id)).map(|f| (f.to_string(), json!(n)))).collect::<serde_json::Map<_, _>>(),
                    "image_marks": self.doc.keys(i).iter().filter_map(|(f, id)| l.marks.get(id).map(|m| (f.to_string(), json!(m)))).collect::<serde_json::Map<_, _>>(),
                    "end": l.end,
                    "before": l.pre.name(),
                    "after": l.post.name(),
                })
            })
            .collect();
        json!({
            "file": self.path.as_ref().map(|p| p.display().to_string()),
            "width": self.doc.w,
            "height": self.doc.h,
            "fps": self.doc.fps,
            "frames": self.doc.frames,
            "current_frame": self.doc.frame,
            "current_layer": self.doc.current_layer,
            "bookmarks": self.doc.bookmarks,
            "mark_in": self.doc.mark_in,
            "mark_out": self.doc.mark_out,
            "notes": self.doc.notes.iter().map(|(f, t)| (f.to_string(), json!(t))).collect::<serde_json::Map<_, _>>(),
            "history": self.history_json(),
            "layers_bottom_to_top": layers,
            "paper": hex(self.doc.paper),
            "sounds": self.doc.sounds.iter().enumerate().map(|(i, t)| json!({"track": i, "file": t.path.display().to_string(), "volume": (t.volume * 100.0).round(), "offset_frames": (t.offset * self.doc.fps).round(), "mute": t.mute, "fade_in": t.fade_in, "fade_out": t.fade_out})).collect::<Vec<_>>(),
            "paper_texture": format!("{:?}", self.doc.paper_texture).to_lowercase(),
            "light_table": {
                "active": self.doc.light.active,
                "before": self.doc.light.prev_count,
                "after": self.doc.light.next_count,
                "opacity": self.doc.light.opacity,
            },
        })
    }

    fn call(&mut self, name: &str, a: &Value) -> Result<Value, String> {
        match name {
            "project_new" => {
                let w = a.get("width").and_then(Value::as_u64).unwrap_or(1920).clamp(16, 4096) as usize;
                let h = a.get("height").and_then(Value::as_u64).unwrap_or(1080).clamp(16, 4096) as usize;
                let fps = a.get("fps").and_then(Value::as_f64).unwrap_or(24.0).clamp(1.0, 120.0) as f32;
                self.doc = match a.get("template").and_then(Value::as_u64) {
                    Some(t) => {
                        let t = crate::doc::TEMPLATES
                            .get(t as usize)
                            .ok_or("template is 0..18 (see the description)")?;
                        let (w, h, fps) = (
                            a.get("width").map(|_| w).unwrap_or(t.w),
                            a.get("height").map(|_| h).unwrap_or(t.h),
                            a.get("fps").map(|_| fps).unwrap_or(t.fps),
                        );
                        Doc::from_template(&crate::doc::Template {
                            w,
                            h,
                            fps,
                            ..crate::doc::Template { ..*t }
                        })
                    }
                    None => Doc::new(w, h, fps),
                };
                let (w, h, fps) = (self.doc.w, self.doc.h, self.doc.fps);
                self.clips = vec![("Shot 1".into(), String::new(), None)];
                self.active = 0;
                self.path = None;
                Ok(text(format!("New project {w}x{h} at {fps} fps.")))
            }
            "project_open" => {
                let p = PathBuf::from(str_arg(a, "path")?);
                let mut list = io::open_project(&p)?;
                let first = list.remove(0);
                self.doc = first.doc;
                self.clips = vec![(if first.name.is_empty() { "Shot 1".into() } else { first.name }, first.notes, None)];
                for c in list {
                    self.clips.push((c.name, c.notes, Some(c.doc)));
                }
                self.active = 0;
                self.path = Some(p);
                Ok(json!({"content": [{"type": "text", "text": self.info().to_string()}]}))
            }
            "project_save" => {
                let p = match a.get("path").and_then(Value::as_str) {
                    Some(p) => {
                        let mut p = PathBuf::from(p);
                        if p.extension().is_none() {
                            p.set_extension("toonlight");
                        }
                        p
                    }
                    None => self.path.clone().ok_or("give a path to save to (….toonlight)")?,
                };
                let list: Vec<(&Doc, &str, &str)> = self
                    .clips
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let d = if i == self.active { &self.doc } else { c.2.as_ref().unwrap() };
                        let name = if self.clips.len() == 1 && c.1.is_empty() {
                            ""
                        } else {
                            c.0.as_str()
                        };
                        (d, name, c.1.as_str())
                    })
                    .collect();
                io::save_project(&list, &p)?;
                self.doc.dirty = false;
                let msg = format!("Saved {}", p.display());
                self.path = Some(p);
                Ok(text(msg))
            }
            "project_info" => Ok(text(self.info().to_string())),
            "project_settings" => {
                if let Some(f) = a.get("fps").and_then(Value::as_f64) {
                    self.doc.fps = (f as f32).clamp(1.0, 120.0);
                }
                if let Some(n) = a.get("frames").and_then(Value::as_u64) {
                    self.doc.set_length(n as usize);
                }
                if let Some(c) = a.get("paper") {
                    self.doc.paper = color_arg(c)?;
                }
                if let Some(t) = a.get("paper_texture").and_then(Value::as_str) {
                    self.doc.paper_texture = ALL_PAPERS
                        .iter()
                        .copied()
                        .find(|p| format!("{p:?}").eq_ignore_ascii_case(t))
                        .ok_or("paper_texture is smooth, grain, canvas or watercolor")?;
                }
                if let Some(l) = a.get("light_table") {
                    let lt = &mut self.doc.light;
                    if let Some(v) = l.get("active").and_then(Value::as_bool) {
                        lt.active = v;
                    }
                    if let Some(v) = l.get("before").and_then(Value::as_u64) {
                        lt.prev_count = (v as usize).min(10);
                    }
                    if let Some(v) = l.get("after").and_then(Value::as_u64) {
                        lt.next_count = (v as usize).min(10);
                    }
                    if let Some(v) = l.get("opacity").and_then(Value::as_f64) {
                        lt.opacity = (v as f32).clamp(0.0, 100.0);
                    }
                }
                Ok(text(self.info().to_string()))
            }
            "go_to" => {
                self.place(a)?;
                let f = self.doc.frame;
                if let Some(d) = a.get("drawing").and_then(Value::as_str) {
                    let keys = self.doc.keys(self.doc.current_layer);
                    let to = match d {
                        "next" => keys.iter().map(|k| k.0).find(|&k| k > f),
                        "previous" | "prev" => keys.iter().rev().map(|k| k.0).find(|&k| k < f),
                        other => return Err(format!("drawing {other}: use next or previous")),
                    };
                    self.doc.frame = to.ok_or("no drawing change that way on this layer")?.min(self.doc.frames - 1);
                }
                if let Some(d) = a.get("mark").and_then(Value::as_str) {
                    let l = self.doc.current_layer;
                    let to = match d {
                        "next" => self.doc.next_mark(l, f),
                        "previous" | "prev" => self.doc.prev_mark(l, f),
                        other => return Err(format!("mark {other}: use next or previous")),
                    };
                    self.doc.frame = to.ok_or("no marked image that way on this layer")?;
                }
                if let Some(d) = a.get("bookmark").and_then(Value::as_str) {
                    let to = match d {
                        "next" => self.doc.next_bookmark(f),
                        "previous" | "prev" => self.doc.prev_bookmark(f),
                        other => return Err(format!("bookmark {other}: use next or previous")),
                    };
                    self.doc.frame = to.ok_or("no bookmark that way")?;
                }
                Ok(text(format!("Frame {}, layer {}.", self.doc.frame, self.doc.current_layer)))
            }
            "layer_add" => {
                self.doc.add_layer();
                let l = self.doc.current_layer;
                if let Some(n) = a.get("name").and_then(Value::as_str) {
                    self.doc.layers[l].name = n.to_string();
                }
                Ok(text(format!("Layer {l} added (now current).")))
            }
            "layer_set" => {
                let l = self.layer_arg(a)?;
                let layer = &mut self.doc.layers[l];
                if let Some(v) = a.get("name").and_then(Value::as_str) {
                    layer.name = v.to_string();
                }
                if let Some(v) = a.get("visible").and_then(Value::as_bool) {
                    layer.visible = v;
                }
                if let Some(v) = a.get("locked").and_then(Value::as_bool) {
                    layer.locked = v;
                }
                if let Some(v) = a.get("light_table").and_then(Value::as_bool) {
                    layer.onion = v;
                }
                if let Some(v) = a.get("stencil").and_then(Value::as_bool) {
                    layer.stencil = v;
                }
                if let Some(v) = a.get("collapsed").and_then(Value::as_bool) {
                    layer.collapsed = v;
                }
                for (key, slot) in [("before", &mut layer.pre), ("after", &mut layer.post)] {
                    if let Some(v) = a.get(key).and_then(Value::as_str) {
                        *slot = crate::doc::Behavior::from_name(v).ok_or("before / after is none, repeat, pingpong or hold")?;
                    }
                }
                if let Some(v) = a.get("auto_break").and_then(Value::as_bool) {
                    self.doc.auto_break = v;
                }
                if let Some(v) = a.get("auto_stretch").and_then(Value::as_bool) {
                    self.doc.auto_stretch = v;
                }
                match a.get("end") {
                    Some(Value::Null) => layer.end = None,
                    Some(v) => layer.end = Some(v.as_u64().ok_or("end is a frame index or null")? as usize),
                    None => {}
                }
                if let Some(v) = a.get("opacity").and_then(Value::as_f64) {
                    layer.opacity = (v as f32 / 100.0).clamp(0.0, 1.0);
                }
                if let Some(v) = a.get("blend").and_then(Value::as_str) {
                    layer.blend = ALL_BLENDS
                        .iter()
                        .copied()
                        .find(|b| format!("{b:?}").eq_ignore_ascii_case(v) || b.json_name().eq_ignore_ascii_case(v))
                        .ok_or("blend is color, multiply, screen, add, overlay, darken, lighten, difference or erase")?;
                }
                if let Some(c) = a.get("color_group") {
                    // a number picks one of the 13 groups (clamped to 0..12); a colour sets it
                    // directly
                    layer.group = match c.as_u64() {
                        Some(i) => self.doc.groups[(i as usize).min(12)].color,
                        None => color_arg(c)?,
                    };
                }
                Ok(text(format!("Layer {l} updated.")))
            }
            "layer_delete" => {
                let layers = self.layers_arg(a)?;
                if self.doc.layers.len() <= 1 {
                    return Err("the last layer can't be deleted".into());
                }
                let n = self.doc.delete_layers(&layers);
                Ok(text(format!("{n} layer(s) deleted; {} left.", self.doc.layers.len())))
            }
            "layer_merge" => {
                self.doc.current_layer = self.layer_arg(a)?;
                let opts = crate::doc::MergeOptions {
                    keep_color_group: a.get("keep_color_group").and_then(Value::as_bool).unwrap_or(true),
                    keep_instance_names: a.get("keep_instance_names").and_then(Value::as_bool).unwrap_or(true),
                    keep_image_marks: a.get("keep_image_marks").and_then(Value::as_bool).unwrap_or(true),
                };
                let ok = match a.get("what").and_then(Value::as_str).unwrap_or("down") {
                    "down" => self.doc.merge_down(opts),
                    "visible" => self.doc.merge_visible(opts),
                    "selected" => {
                        let layers = if a.get("layers").is_some() {
                            self.layers_arg(a)?
                        } else {
                            self.doc.target_layers()
                        };
                        self.doc.merge_layers(&layers, opts)
                    }
                    "join" => {
                        let layers = if a.get("layers").is_some() {
                            self.layers_arg(a)?
                        } else {
                            self.doc.target_layers()
                        };
                        self.doc.join_layers(&layers)
                    }
                    other => return Err(format!("what {other}: use down, visible, selected or join")),
                };
                if !ok {
                    return Err("nothing to merge: no layer below, fewer than two layers, or a locked layer".into());
                }
                Ok(text(format!(
                    "Merged; {} layers now, current layer {}.",
                    self.doc.layers.len(),
                    self.doc.current_layer
                )))
            }
            "layer_move" => {
                self.doc.current_layer = self.layer_arg(a)?;
                let up = str_arg(a, "direction")? == "up";
                self.doc.move_layer(up);
                Ok(text(format!("Layer is now {}.", self.doc.current_layer)))
            }
            "frame_add" => {
                self.place(a)?;
                match a.get("kind").and_then(Value::as_str).unwrap_or("blank") {
                    "blank" => self.doc.add_blank_after(),
                    "duplicate" => self.doc.duplicate_after(),
                    "hold" => self.doc.expose_more(),
                    other => return Err(format!("kind {other}: use blank, duplicate or hold")),
                }
                Ok(text(format!("Now at frame {} of {}.", self.doc.frame, self.doc.frames)))
            }
            "timing" => {
                let l = self.layer_arg(a)?;
                let from = a.get("from").and_then(Value::as_u64).map(|v| v as usize).unwrap_or(self.doc.frame);
                let to = a.get("to").and_then(Value::as_u64).map(|v| v as usize).unwrap_or(from).max(from);
                let last = self.doc.frames - 1;
                if from > last {
                    return Err(format!("frame {from} does not exist (0..{last})"));
                }
                let to = to.min(last);
                match str_arg(a, "action")? {
                    "set_exposure" => {
                        let n = a.get("frames").and_then(Value::as_u64).ok_or("frames is missing")? as usize;
                        let end = self.doc.set_exposure(l, from, to, n).ok_or("no drawing there, or the layer is locked")?;
                        Ok(text(format!("Each drawing now lasts {n} frames; the run ends at frame {end}.")))
                    }
                    "add_exposures" | "remove_exposures" => {
                        let n = a.get("frames").and_then(Value::as_u64).unwrap_or(1) as usize;
                        self.doc.current_layer = l;
                        self.doc.frame = from;
                        if str_arg(a, "action")? == "add_exposures" {
                            self.doc.expose_more_n(n);
                        } else {
                            self.doc.expose_less_n(n);
                        }
                        Ok(text(format!("{} frames.", self.doc.frames)))
                    }
                    "move_cells" => {
                        let by = a.get("by").and_then(Value::as_i64).ok_or("by is missing")?;
                        let (mut a0, mut b0) = (from, to);
                        for _ in 0..by.unsigned_abs() {
                            if !self.doc.move_cells(l, a0, b0, by > 0) {
                                return Err("the cells can't move further".into());
                            }
                            if by > 0 {
                                a0 += 1;
                                b0 += 1;
                            } else {
                                a0 -= 1;
                                b0 -= 1;
                            }
                        }
                        Ok(text(format!("Cells are now at frames {a0}..{b0}.")))
                    }
                    "stretch" => {
                        if let Some(ratio) = a.get("ratio").and_then(Value::as_f64) {
                            // a time ratio in percent, for one layer or several together
                            let layers = self.layers_arg(a)?;
                            let n = self.doc.stretch_layers_by(&layers, ratio as f32 / 100.0);
                            if n == 0 {
                                return Err("nothing to stretch: the layers are locked or empty, or the ratio changes nothing".into());
                            }
                            return Ok(text(format!("{n} layer(s) stretched to {ratio} %; {} frames now.", self.doc.frames)));
                        }
                        let n = a.get("frames").and_then(Value::as_u64).ok_or("frames (or ratio) is missing")? as usize;
                        if !self.doc.stretch_layer(l, n) {
                            return Err("nothing to stretch on this layer, or it is locked".into());
                        }
                        Ok(text(format!("Layer {l} now lasts {n} frames.")))
                    }
                    "retime" => {
                        let n = a.get("frames").and_then(Value::as_u64).ok_or("frames is missing")? as usize;
                        let last = self.doc.retime_layer(l, n).ok_or("nothing to retime on this layer, or it is locked")?;
                        Ok(text(format!("Every drawing of layer {l} now lasts {} frame(s); the layer ends at frame {last}.", n.max(1))))
                    }
                    "trim_start" => {
                        if !self.doc.trim_layer_start(l, from) {
                            return Err("the layer can't start there: pick a frame after its first drawing and not after its end (and unlock it)".into());
                        }
                        Ok(text(format!("Layer {l} now starts at frame {from}.")))
                    }
                    "repeat" => {
                        let times = a.get("frames").and_then(Value::as_u64).unwrap_or(1) as usize;
                        let mode = match a.get("mode").and_then(Value::as_str).unwrap_or("loop") {
                            "loop" => crate::doc::RepeatMode::Loop,
                            "pingpong" => crate::doc::RepeatMode::PingPong,
                            "random" => crate::doc::RepeatMode::Random,
                            other => return Err(format!("mode {other}: use loop, pingpong or random")),
                        };
                        let end = self.doc.repeat_cells(l, from, to, times, mode).ok_or("nothing to repeat there, or the layer is locked")?;
                        Ok(text(format!("Repeated; the copies end at frame {end}.")))
                    }
                    "delete_cells" => {
                        self.doc.delete_cells(l, from, to);
                        Ok(text("Cells removed."))
                    }
                    "name_drawing" => {
                        self.doc.set_instance_name(l, from, a.get("name").and_then(Value::as_str).unwrap_or(""));
                        Ok(text("Drawing named."))
                    }
                    "set_number" => {
                        let t = match a.get("number") {
                            Some(Value::String(s)) => s.clone(),
                            Some(Value::Number(n)) => n.to_string(),
                            Some(Value::Null) | None => String::new(),
                            _ => return Err("number is a drawing number, or empty / \"-\" for no drawing".into()),
                        };
                        for f in from..=to {
                            if !self.doc.set_cell_number(l, f, &t) {
                                return Err("the layer is locked, or the number is not valid".into());
                            }
                        }
                        Ok(text(format!("Frames {from}..{to} now show drawing {}.", if t.is_empty() { "none" } else { &t })))
                    }
                    "bookmark" => {
                        self.doc.toggle_bookmark(from);
                        Ok(text(format!("Bookmarks: {:?}", self.doc.bookmarks)))
                    }
                    "shift" => {
                        let by = a.get("by").and_then(Value::as_i64).ok_or("by is missing")?;
                        let layers = self.layers_arg(a)?;
                        let moved = self.doc.shift_layers(&layers, by);
                        if moved == 0 {
                            return Err("the layer can't move: it is locked, has no drawing, or already starts at the first frame".into());
                        }
                        Ok(text(format!("Shifted by {moved} frames; {} frames now.", self.doc.frames)))
                    }
                    "slide" => {
                        let by = a.get("by").and_then(Value::as_i64).ok_or("by is missing")?;
                        if !self.doc.slide_layer(l, by) {
                            return Err("nothing to slide on this layer, or it is locked".into());
                        }
                        Ok(text(format!("The drawings of layer {l} slid by {by} frames inside the layer.")))
                    }
                    "split" => {
                        if !self.doc.split_layer(l, from) {
                            return Err("the layer can't be split there: pick a frame after its first drawing and not after its end (and unlock it)".into());
                        }
                        Ok(text(format!("Layer {l} now ends at frame {}; the rest is the new layer {} (now current).", from - 1, l + 1)))
                    }
                    "mark_image" => {
                        let m = a.get("mark").and_then(Value::as_u64).ok_or("mark is missing (1-12, 0 removes it)")?.min(12) as u8;
                        for f in from..=to {
                            if !self.doc.set_mark(l, f, m) {
                                return Err("no drawing there, or the layer is locked".into());
                            }
                        }
                        Ok(text(if m == 0 { "Mark removed.".to_string() } else { format!("Marked with colour {m}.") }))
                    }
                    "note" => {
                        let t = a.get("text").and_then(Value::as_str).unwrap_or("");
                        self.doc.set_note(from, t);
                        Ok(text(format!("{} notes on the timeline.", self.doc.notes.len())))
                    }
                    "insert_tweens" => {
                        let n = a.get("frames").and_then(Value::as_u64).unwrap_or(1).clamp(1, 99) as usize;
                        let mix = a.get("mix").and_then(Value::as_bool).unwrap_or(true);
                        let at = self.doc.insert_tweens(l, from, n, mix).ok_or("no drawing after this one on the layer, or the layer is locked")?;
                        Ok(text(format!("{n} in-between drawings inserted at frames {at}..{}; {} frames now.", at + n - 1, self.doc.frames)))
                    }
                    "mark_in" => {
                        self.doc.mark_in = if a.get("clear").and_then(Value::as_bool).unwrap_or(false) { None } else { Some(from) };
                        Ok(text(format!("Play range {:?}.", self.doc.play_range())))
                    }
                    "mark_out" => {
                        self.doc.mark_out = if a.get("clear").and_then(Value::as_bool).unwrap_or(false) { None } else { Some(from) };
                        Ok(text(format!("Play range {:?}.", self.doc.play_range())))
                    }
                    other => Err(format!("action {other}: use set_exposure, add_exposures, remove_exposures, move_cells, delete_cells, repeat, stretch, set_number, name_drawing, bookmark, mark_in, mark_out, mark_image, note, insert_tweens, shift, slide or split")),
                }
            }
            "frame_remove" => {
                self.place(a)?;
                match a.get("kind").and_then(Value::as_str).unwrap_or("cell") {
                    "cell" => self.doc.delete_cell(),
                    "hold" => self.doc.expose_less(),
                    other => return Err(format!("kind {other}: use cell or hold")),
                }
                Ok(text(format!("{} frames.", self.doc.frames)))
            }
            "draw_stroke" => {
                self.place(a)?;
                if a.get("tool").and_then(Value::as_str) == Some("custom") {
                    let b = self.brush.clone().ok_or("no custom brush yet: call custom_brush first")?;
                    let pts = points_arg(a)?;
                    let mut cs = crate::custom::CustomSettings::default();
                    if let Some(v) = a.get("size").and_then(Value::as_f64) {
                        cs.scale = v as f32;
                    }
                    if let Some(v) = a.get("opacity").and_then(Value::as_f64) {
                        cs.opacity = v as f32;
                    }
                    if let Some(v) = a.get("spacing").and_then(Value::as_f64) {
                        cs.step = v as f32;
                    }
                    cs.tint = a.get("color").is_some();
                    let color = a.get("color").map(color_arg).transpose()?.unwrap_or([20, 20, 30]);
                    let id = self.doc.target_drawing().ok_or("this layer is locked")?;
                    let (w, h) = (self.doc.w, self.doc.h);
                    let mut st = crate::custom::CustomStroke::begin(id, &self.doc.drawings[id].px, w, h, b, cs, color, None);
                    for (x, y, _) in &pts {
                        st.to(*x, *y, &mut self.doc.drawings[id].px);
                    }
                    self.doc.push_pixel_undo(id, st.dirty, &st.before);
                    return Ok(text("Custom brush stroke drawn."));
                }
                let tool = tool_arg(a.get("tool").and_then(Value::as_str).unwrap_or("pen"))?;
                if !tool.paints() {
                    return Err("tool is pencil, pen, airbrush, brush, wet_brush, eraser, warp or custom".into());
                }
                let pts = points_arg(a)?;
                let mut s = settings_arg(a, tool);
                if let Some(v) = a.get("aspect").and_then(Value::as_f64) {
                    s.aspect = (v as f32).clamp(5.0, 100.0);
                }
                if let Some(v) = a.get("angle").and_then(Value::as_f64) {
                    s.angle = v as f32;
                }
                if let Some(c) = a.get("connections") {
                    for (key, slot) in [
                        ("size", &mut s.con_size),
                        ("power", &mut s.con_power),
                        ("aspect", &mut s.con_aspect),
                        ("angle", &mut s.con_angle),
                    ] {
                        if let Some(v) = c.get(key) {
                            *slot = connection_arg(v)?;
                        }
                    }
                }
                if pts.iter().any(|p| p.2 < 1.0) {
                    s.size_pressure = a.get("pressure_size").and_then(Value::as_bool).unwrap_or(true);
                    s.power_pressure = a.get("pressure_power").and_then(Value::as_bool).unwrap_or(false);
                }
                if let Some(m) = a.get("mode").and_then(Value::as_str) {
                    s.mode = brush::DrawMode::from_name(m).ok_or_else(|| {
                        format!(
                            "mode {m}: use {}",
                            brush::ALL_DRAW_MODES.iter().map(|d| d.name()).collect::<Vec<_>>().join(", ")
                        )
                    })?;
                }
                let color = a.get("color").map(color_arg).transpose()?.unwrap_or([20, 20, 30]);
                let id = self.doc.target_drawing().ok_or("this layer is locked")?;
                let (w, h) = (self.doc.w, self.doc.h);
                let mut st = Stroke::begin(id, &self.doc.drawings[id].px, w, h, tool, color, s);
                st.set_preserve(a.get("preserve_transparency").and_then(Value::as_bool).unwrap_or(false));
                if let Some(m) = self
                    .doc
                    .stencil_mask(a.get("stencil_inside").and_then(Value::as_bool).unwrap_or(false))
                {
                    st.set_clip(std::sync::Arc::new(m));
                }
                for (x, y, p) in &pts {
                    st.to_p(*x, *y, *p, &mut self.doc.drawings[id].px);
                }
                self.doc.push_pixel_undo(id, st.dirty, &st.before);
                Ok(text(format!(
                    "Stroke of {} points drawn on layer {} frame {}.",
                    pts.len(),
                    self.doc.current_layer,
                    self.doc.frame
                )))
            }
            "draw_shape" => {
                self.place(a)?;
                let tool = match str_arg(a, "shape")? {
                    "line" => Tool::Line,
                    "rectangle" => Tool::Rectangle,
                    "ellipse" => Tool::Ellipse,
                    other => return Err(format!("shape {other}: use line, rectangle or ellipse")),
                };
                let from = pair(a.get("from").ok_or("from is missing")?)?;
                let to = pair(a.get("to").ok_or("to is missing")?)?;
                let s = settings_arg(a, tool);
                let color = a.get("color").map(color_arg).transpose()?.unwrap_or([20, 20, 30]);
                let id = self.doc.target_drawing().ok_or("this layer is locked")?;
                let (w, h) = (self.doc.w, self.doc.h);
                let mut st = Stroke::begin(id, &self.doc.drawings[id].px, w, h, tool, color, s);
                for poly in brush::shape_points(tool, from, to) {
                    for (x, y) in poly {
                        st.to(x, y, &mut self.doc.drawings[id].px);
                    }
                }
                if a.get("filled").and_then(Value::as_bool).unwrap_or(false) && tool != Tool::Line {
                    let c = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
                    if c.0 >= 0.0 && c.1 >= 0.0 && (c.0 as usize) < w && (c.1 as usize) < h {
                        brush::flood_fill(&mut self.doc.drawings[id].px, w, h, c.0 as usize, c.1 as usize, color, 15.0, 100.0);
                        st.dirty = st.dirty.union(&Rect::full(w, h));
                    }
                }
                self.doc.push_pixel_undo(id, st.dirty, &st.before);
                Ok(text("Shape drawn."))
            }
            "fill" => {
                self.place(a)?;
                let (x, y) = pair(a.get("at").ok_or("at is missing ([x, y])")?)?;
                let color = a.get("color").map(color_arg).transpose()?.unwrap_or([20, 20, 30]);
                let tol = a.get("tolerance").and_then(Value::as_f64).unwrap_or(15.0) as f32;
                let (w, h) = (self.doc.w, self.doc.h);
                if x < 0.0 || y < 0.0 || x as usize >= w || y as usize >= h {
                    return Err("at is outside the picture".into());
                }
                let id = self.doc.target_drawing().ok_or("this layer is locked")?;
                let before = self.doc.drawings[id].px.clone();
                let r = brush::flood_fill(&mut self.doc.drawings[id].px, w, h, x as usize, y as usize, color, tol, 100.0);
                self.doc.push_pixel_undo(id, r, &before);
                Ok(text("Filled."))
            }
            "clear_drawing" => {
                self.place(a)?;
                let l = self.doc.current_layer;
                if let Some(id) = self.doc.drawing_at(l, self.doc.frame) {
                    let before = self.doc.drawings[id].px.clone();
                    self.doc.drawings[id].px.fill(0);
                    self.doc.push_pixel_undo(id, Rect::full(self.doc.w, self.doc.h), &before);
                }
                Ok(text("Cleared."))
            }
            "draw_text" => {
                self.place(a)?;
                let (x, y) = pair(a.get("at").ok_or("at is missing ([x, y], top-left corner)")?)?;
                let mut t = crate::text::TextSettings {
                    text: str_arg(a, "text")?.to_string(),
                    ..Default::default()
                };
                if let Some(v) = a.get("size").and_then(Value::as_f64) {
                    t.size = (v as f32).clamp(4.0, 1000.0);
                }
                match a.get("border").and_then(Value::as_f64) {
                    Some(b) if b > 0.0 => t.border = b as f32,
                    Some(_) => t.border_on = false,
                    None => {}
                }
                if let Some(c) = a.get("border_color") {
                    t.border_color = color_arg(c)?;
                }
                if let Some(v) = a.get("italic").and_then(Value::as_f64) {
                    t.italic_on = true;
                    t.italic = v as f32;
                }
                if let Some(v) = a.get("angle").and_then(Value::as_f64) {
                    t.rotate_on = true;
                    t.rotate = v as f32;
                }
                let color = a.get("color").map(color_arg).transpose()?.unwrap_or([20, 20, 30]);
                let needs_cjk = t.text.chars().any(|c| (c as u32) >= 0x2E80);
                let font = if needs_cjk {
                    crate::i18n::find_cjk_font(false).and_then(|(p, i)| crate::text::font_from_file(&p, i))
                } else {
                    None
                }
                .or_else(crate::text::default_font)
                .ok_or("no font available")?;
                let id = self.doc.target_drawing().ok_or("this layer is locked")?;
                let (w, h) = (self.doc.w, self.doc.h);
                let before = self.doc.drawings[id].px.clone();
                let r = crate::text::stamp(&mut self.doc.drawings[id].px, w, h, x, y, color, &t, &font);
                self.doc.push_pixel_undo(id, r, &before);
                Ok(text("Text written."))
            }
            "sound_set" => {
                let n = self.doc.sounds.len();
                let track = match a.get("track").and_then(Value::as_u64) {
                    Some(t) if (t as usize) < n => Some(t as usize),
                    Some(t) => return Err(format!("sound track {t} does not exist ({n} tracks)")),
                    None => None,
                };
                if a.get("remove").and_then(Value::as_bool).unwrap_or(false) {
                    // one track, or every track when none is named; locked tracks stay
                    match track {
                        Some(t) if self.doc.sounds[t].locked => {
                            return Err(format!("sound track {t} is locked: unlock it first (locked: false)"));
                        }
                        Some(t) => {
                            self.doc.sounds.remove(t);
                        }
                        None => self.doc.sounds.retain(|t| t.locked),
                    }
                    self.doc.dirty = true;
                    return Ok(text(format!("Sound removed; {} tracks left.", self.doc.sounds.len())));
                }
                // which track the settings go to: the one named, a new one (add, or the first
                // sound of the shot), else the first
                let mut track = track.unwrap_or(0);
                if let Some(p) = a.get("path").and_then(Value::as_str) {
                    let clip = crate::sound::Clip::load(std::path::Path::new(p))?;
                    let add = a.get("add").and_then(Value::as_bool).unwrap_or(false);
                    if add || n == 0 {
                        self.doc.sounds.push(crate::sound::SoundTrack::new(p.into()));
                        track = self.doc.sounds.len() - 1;
                    } else if self.doc.sounds[track].locked {
                        return Err(format!("sound track {track} is locked: unlock it first (locked: false)"));
                    } else {
                        self.doc.sounds[track] = crate::sound::SoundTrack::new(p.into());
                    }
                    let secs = clip.duration();
                    if a.get("fit_length").and_then(Value::as_bool).unwrap_or(false) {
                        let n = (secs * self.doc.fps).ceil() as usize;
                        self.doc.set_length(n.max(1));
                    }
                }
                self.doc.dirty = true;
                let t = self.doc.sounds.get_mut(track).ok_or("no sound yet: give a path (WAV or MP3)")?;
                // a track is unlocked before the other changes of the same call, and locked after them
                if a.get("locked").and_then(Value::as_bool) == Some(false) {
                    t.locked = false;
                }
                let changes = ["volume", "offset_frames", "mute", "fade_in", "fade_out", "color"];
                if t.locked && changes.iter().any(|k| a.get(k).is_some()) {
                    return Err(format!("sound track {track} is locked: unlock it first (locked: false)"));
                }
                if let Some(v) = a.get("color").and_then(Value::as_u64) {
                    t.color = v.min(12) as u8;
                }
                if let Some(v) = a.get("volume").and_then(Value::as_f64) {
                    t.volume = (v as f32 / 100.0).clamp(0.0, 2.0);
                }
                if let Some(v) = a.get("offset_frames").and_then(Value::as_f64) {
                    t.offset = v as f32 / self.doc.fps.max(1.0);
                }
                if let Some(v) = a.get("mute").and_then(Value::as_bool) {
                    t.mute = v;
                }
                if let Some(v) = a.get("fade_in").and_then(Value::as_f64) {
                    t.fade_in = (v as f32).max(0.0);
                }
                if let Some(v) = a.get("fade_out").and_then(Value::as_f64) {
                    t.fade_out = (v as f32).max(0.0);
                }
                if a.get("locked").and_then(Value::as_bool) == Some(true) {
                    self.doc.sounds[track].locked = true;
                }
                Ok(text(format!(
                    "Sound track {track} of {}: {:?}",
                    self.doc.sounds.len(),
                    self.doc.sounds[track]
                )))
            }
            "import_image" => {
                self.place(a)?;
                let (px, pw, ph) = io::load_picture(std::path::Path::new(str_arg(a, "path")?))?;
                let id = self.doc.target_drawing().ok_or("this layer is locked")?;
                let (w, h) = (self.doc.w, self.doc.h);
                let before = self.doc.drawings[id].px.clone();
                let mut f = crate::select::Floating::new(id, (px, pw, ph, 0, 0), before.clone(), before.clone());
                let fit = (w as f32 / pw as f32).min(h as f32 / ph as f32).min(1.0);
                f.scale = a.get("scale").and_then(Value::as_f64).map(|v| v as f32 / 100.0).unwrap_or(fit);
                let (cx, cy) = match a.get("center") {
                    Some(c) => pair(c)?,
                    None => (w as f32 / 2.0, h as f32 / 2.0),
                };
                f.cx = cx;
                f.cy = cy;
                let mut out = before.clone();
                f.render(&mut out, w, h);
                self.doc.drawings[id].px = out;
                self.doc.push_pixel_undo(id, Rect::full(w, h), &before);
                Ok(text(format!("Picture {pw}x{ph} placed at scale {:.0} %.", f.scale * 100.0)))
            }
            "gradient" => {
                self.place(a)?;
                let from = pair(a.get("from").ok_or("from is missing")?)?;
                let to = pair(a.get("to").ok_or("to is missing")?)?;
                let ca = a.get("color_from").map(color_arg).transpose()?.unwrap_or([0, 0, 0]);
                let cb = a.get("color_to").map(color_arg).transpose()?.unwrap_or([255, 255, 255]);
                let kind = if a.get("radial").and_then(Value::as_bool).unwrap_or(false) {
                    crate::select::GradientKind::Radial
                } else {
                    crate::select::GradientKind::Linear
                };
                let opacity = a.get("opacity").and_then(Value::as_f64).unwrap_or(100.0) as f32;
                let id = self.doc.target_drawing().ok_or("this layer is locked")?;
                let (w, h) = (self.doc.w, self.doc.h);
                let sel = match a.get("rect") {
                    Some(r) => {
                        let v: Vec<f32> = r
                            .as_array()
                            .ok_or("rect is [x0, y0, x1, y1]")?
                            .iter()
                            .filter_map(Value::as_f64)
                            .map(|x| x as f32)
                            .collect();
                        if v.len() != 4 {
                            return Err("rect is [x0, y0, x1, y1]".into());
                        }
                        Some(crate::select::Selection::rect(w, h, (v[0], v[1]), (v[2], v[3])))
                    }
                    None => None,
                };
                let before = self.doc.drawings[id].px.clone();
                let r = crate::select::gradient(
                    &mut self.doc.drawings[id].px,
                    w,
                    h,
                    from,
                    to,
                    ca,
                    cb,
                    kind,
                    0.5,
                    opacity,
                    sel.as_ref(),
                );
                self.doc.push_pixel_undo(id, r, &before);
                Ok(text("Gradient painted."))
            }
            "transform" => {
                self.place(a)?;
                let id = self.doc.target_drawing().ok_or("this layer is locked")?;
                let (w, h) = (self.doc.w, self.doc.h);
                let sel = match a.get("rect") {
                    Some(r) => {
                        let v: Vec<f32> = r
                            .as_array()
                            .ok_or("rect is [x0, y0, x1, y1]")?
                            .iter()
                            .filter_map(Value::as_f64)
                            .map(|x| x as f32)
                            .collect();
                        if v.len() != 4 {
                            return Err("rect is [x0, y0, x1, y1]".into());
                        }
                        crate::select::Selection::rect(w, h, (v[0], v[1]), (v[2], v[3]))
                    }
                    None => crate::select::Selection::all(w, h),
                };
                let before = self.doc.drawings[id].px.clone();
                let piece = sel.copy(&before).ok_or("nothing to transform")?;
                let mut base = before.clone();
                sel.clear(&mut base);
                let mut f = crate::select::Floating::new(id, piece, before.clone(), base.clone());
                f.cx += a.get("dx").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                f.cy += a.get("dy").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                f.scale = a.get("scale").and_then(Value::as_f64).map(|v| v as f32 / 100.0).unwrap_or(1.0);
                f.angle = a.get("angle").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                f.flip_x = a.get("flip_x").and_then(Value::as_bool).unwrap_or(false);
                f.flip_y = a.get("flip_y").and_then(Value::as_bool).unwrap_or(false);
                let mut out = base;
                f.render(&mut out, w, h);
                self.doc.drawings[id].px = out;
                self.doc.push_pixel_undo(id, Rect::full(w, h), &before);
                Ok(text("Transformed."))
            }
            "custom_brush" => {
                self.place(a)?;
                let b = if let Some(p) = a.get("path").and_then(Value::as_str) {
                    let (px, w, h) = io::load_picture(std::path::Path::new(p))?;
                    crate::custom::CustomBrush::trimmed(px, w, h)
                } else {
                    let r: Vec<f32> = a
                        .get("rect")
                        .and_then(Value::as_array)
                        .ok_or("give rect [x0, y0, x1, y1] or path")?
                        .iter()
                        .filter_map(Value::as_f64)
                        .map(|v| v as f32)
                        .collect();
                    if r.len() != 4 {
                        return Err("rect is [x0, y0, x1, y1]".into());
                    }
                    let id = self
                        .doc
                        .drawing_at(self.doc.current_layer, self.doc.frame)
                        .ok_or("nothing drawn here")?;
                    let sel = crate::select::Selection::rect(self.doc.w, self.doc.h, (r[0], r[1]), (r[2], r[3]));
                    sel.copy(&self.doc.drawings[id].px)
                        .and_then(|(px, w, h, _, _)| crate::custom::CustomBrush::trimmed(px, w, h))
                }
                .ok_or("the brush picture is empty")?;
                let msg = format!("Custom brush {}x{} ready: use draw_stroke with tool \"custom\".", b.w, b.h);
                self.brush = Some(std::sync::Arc::new(b));
                Ok(text(msg))
            }
            "layer_fx" => {
                self.doc.current_layer = self.layer_arg(a)?;
                let l = self.doc.current_layer;
                let frame = a.get("frame").and_then(Value::as_u64).map(|f| f as usize);
                match str_arg(a, "action")? {
                    "add" => {
                        let fx = effect_arg(a)?;
                        let mut e = crate::fx::FxEntry::new(fx);
                        if let Some(f) = frame {
                            e.keys.push((f, fx));
                        }
                        self.doc.layers[l].fx.push(e);
                        self.doc.dirty = true;
                        Ok(text(format!(
                            "Effect {} added to layer {l} (stack of {}).",
                            self.doc.layers[l].fx.len() - 1,
                            self.doc.layers[l].fx.len()
                        )))
                    }
                    "key" => {
                        let i = a.get("index").and_then(Value::as_u64).ok_or("index is missing")? as usize;
                        let f = frame.ok_or("frame is missing")?;
                        let fx = effect_arg(a)?;
                        let e = self.doc.layers[l].fx.get_mut(i).ok_or("no effect with this index")?;
                        if !e.effect.same_kind(&fx) {
                            return Err("the key must be the same effect as the one in the stack".into());
                        }
                        e.keys.retain(|k| k.0 != f);
                        e.keys.push((f, fx));
                        e.keys.sort_by_key(|k| k.0);
                        self.doc.dirty = true;
                        Ok(text(format!("Key at frame {f} for effect {i}.")))
                    }
                    "remove" => {
                        let i = a.get("index").and_then(Value::as_u64).ok_or("index is missing")? as usize;
                        if i >= self.doc.layers[l].fx.len() {
                            return Err("no effect with this index".into());
                        }
                        self.doc.layers[l].fx.remove(i);
                        self.doc.dirty = true;
                        Ok(text("Effect removed."))
                    }
                    "clear" => {
                        self.doc.layers[l].fx.clear();
                        self.doc.dirty = true;
                        Ok(text("Stack emptied."))
                    }
                    "apply" => {
                        if !self.doc.apply_fx_stack(l) {
                            return Err("no effect on, or the layer is locked".into());
                        }
                        Ok(text("Effects painted into the drawings; stack emptied."))
                    }
                    "list" => {
                        let list: Vec<Value> = self.doc.layers[l]
                            .fx
                            .iter()
                            .enumerate()
                            .map(|(i, e)| json!({"index": i, "effect": format!("{:?}", e.effect), "on": e.on, "key_frames": e.keys.iter().map(|k| k.0).collect::<Vec<_>>()}))
                            .collect();
                        Ok(text(json!({"layer": l, "stack": list}).to_string()))
                    }
                    "save" => {
                        let path = std::path::Path::new(str_arg(a, "path")?);
                        let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let json = crate::fx::stack_to_json(&name, &self.doc.layers[l].fx)?;
                        std::fs::write(path, json).map_err(|e| e.to_string())?;
                        Ok(text(format!(
                            "{} effects saved to {}.",
                            self.doc.layers[l].fx.len(),
                            path.display()
                        )))
                    }
                    "load" => {
                        let t = std::fs::read_to_string(str_arg(a, "path")?).map_err(|e| e.to_string())?;
                        let preset = crate::fx::stack_from_json(&t)?;
                        self.doc.push_structure_undo();
                        self.doc.layers[l].fx = preset.effects;
                        Ok(text(format!(
                            "Layer {l} now has the {} effects of the file.",
                            self.doc.layers[l].fx.len()
                        )))
                    }
                    other => Err(format!("action {other}: use add, key, remove, clear, apply, list, save or load")),
                }
            }
            "effect" => {
                self.place(a)?;
                let fx = effect_arg(a)?;
                let (w, h) = (self.doc.w, self.doc.h);
                let l = self.doc.current_layer;
                let ids: Vec<usize> = if a.get("all_frames").and_then(Value::as_bool).unwrap_or(false) {
                    self.doc.keys(l).iter().map(|k| k.1).collect()
                } else {
                    vec![self.doc.drawing_at(l, self.doc.frame).ok_or("nothing drawn here")?]
                };
                for id in &ids {
                    let before = self.doc.drawings[*id].px.clone();
                    let mut px = before.clone();
                    crate::fx::apply(&fx, &before, &mut px, w, h, None);
                    self.doc.drawings[*id].px = px;
                    self.doc.push_pixel_undo(*id, Rect::full(w, h), &before);
                }
                Ok(text(format!("Effect applied to {} drawing(s).", ids.len())))
            }
            "camera" => {
                let (w, h) = (self.doc.w, self.doc.h);
                let cam = &mut self.doc.camera;
                if let Some(v) = a.get("enabled").and_then(Value::as_bool) {
                    cam.enabled = v;
                }
                if let Some(v) = a.get("width").and_then(Value::as_u64) {
                    cam.width = (v as usize).clamp(16, 8192);
                }
                if let Some(v) = a.get("height").and_then(Value::as_u64) {
                    cam.height = (v as usize).clamp(16, 8192);
                }
                if let Some(v) = a.get("smooth").and_then(Value::as_bool) {
                    cam.interpolation = if v {
                        crate::camera::Interpolation::Smooth
                    } else {
                        crate::camera::Interpolation::Linear
                    };
                }
                if let Some(v) = a.get("ease").and_then(Value::as_bool) {
                    cam.ease = v;
                }
                for (key, slot) in [
                    ("lock_x", &mut cam.lock_x),
                    ("lock_y", &mut cam.lock_y),
                    ("lock_rotation", &mut cam.lock_rotation),
                    ("lock_zoom", &mut cam.lock_zoom),
                ] {
                    if let Some(v) = a.get(key).and_then(Value::as_bool) {
                        *slot = v;
                    }
                }
                if a.get("clear_keys").and_then(Value::as_bool).unwrap_or(false) {
                    cam.keys.clear();
                }
                if let Some(keys) = a.get("keys").and_then(Value::as_array) {
                    for k in keys {
                        let f = k.get("frame").and_then(Value::as_u64).ok_or("each key needs a frame")? as usize;
                        let mut ck = cam.at(f, w, h);
                        ck.frame = f;
                        // locked values keep what the camera already has there
                        if let Some(v) = k.get("x").and_then(Value::as_f64).filter(|_| !cam.lock_x) {
                            ck.x = v as f32;
                        }
                        if let Some(v) = k.get("y").and_then(Value::as_f64).filter(|_| !cam.lock_y) {
                            ck.y = v as f32;
                        }
                        if let Some(v) = k.get("zoom").and_then(Value::as_f64).filter(|_| !cam.lock_zoom) {
                            ck.zoom = (v as f32 / 100.0).max(0.01);
                        }
                        if let Some(v) = k.get("rotation").and_then(Value::as_f64).filter(|_| !cam.lock_rotation) {
                            ck.rotation = v as f32;
                        }
                        cam.set_key(ck);
                        cam.enabled = true;
                    }
                }
                let keys: Vec<Value> = cam
                    .keys
                    .iter()
                    .map(|k| json!({"frame": k.frame, "x": k.x, "y": k.y, "zoom": (k.zoom * 100.0).round(), "rotation": k.rotation}))
                    .collect();
                Ok(text(
                    json!({"enabled": cam.enabled, "width": cam.width, "height": cam.height, "keys": keys,
                        "lock_x": cam.lock_x, "lock_y": cam.lock_y, "lock_rotation": cam.lock_rotation, "lock_zoom": cam.lock_zoom})
                    .to_string(),
                ))
            }
            "shot" => {
                let idx = a.get("index").and_then(Value::as_u64).map(|v| v as usize);
                match a.get("action").and_then(Value::as_str).unwrap_or("list") {
                    "list" => {}
                    "add" | "duplicate" => {
                        let doc = if a.get("action").and_then(Value::as_str) == Some("duplicate") {
                            io::duplicate(&self.doc)?
                        } else {
                            Doc::new(self.doc.w, self.doc.h, self.doc.fps)
                        };
                        let name = a
                            .get("name")
                            .and_then(Value::as_str)
                            .map(String::from)
                            .unwrap_or_else(|| format!("Shot {}", self.clips.len() + 1));
                        let at = self.active + 1;
                        self.clips.insert(at, (name, String::new(), Some(doc)));
                        // the active shot moved one place right if it was after the insertion
                        self.select_clip(at)?;
                    }
                    "select" => self.select_clip(idx.ok_or("index is missing")?)?,
                    "delete" => {
                        let i = idx.unwrap_or(self.active);
                        if self.clips.len() <= 1 {
                            return Err("the last shot can't be deleted".into());
                        }
                        if i == self.active {
                            self.select_clip(if i == 0 { 1 } else { i - 1 })?;
                        }
                        self.clips.remove(i);
                        if self.active > i {
                            self.active -= 1;
                        }
                    }
                    "rename" => {
                        let i = idx.unwrap_or(self.active);
                        let c = self.clips.get_mut(i).ok_or("no such shot")?;
                        if let Some(n) = a.get("name").and_then(Value::as_str) {
                            c.0 = n.to_string();
                        }
                        if let Some(n) = a.get("notes").and_then(Value::as_str) {
                            c.1 = n.to_string();
                        }
                    }
                    other => return Err(format!("action {other}: use list, add, duplicate, select, delete or rename")),
                }
                Ok(text(self.clip_list().to_string()))
            }
            "layer_copy" => {
                let l = self.layer_arg(a)?;
                self.doc.current_layer = l;
                match a.get("action").and_then(Value::as_str).unwrap_or("duplicate") {
                    "duplicate" => {
                        self.doc.duplicate_layer();
                        Ok(text(format!("Layer {l} duplicated; the copy is layer {}.", self.doc.current_layer)))
                    }
                    "copy" => {
                        self.layer_clip = Some(self.doc.copy_layer(l));
                        Ok(text(format!("Layer {l} copied (paste it in any shot).")))
                    }
                    "cut" => {
                        if self.doc.layers.len() <= 1 {
                            return Err("the last layer can't be cut".into());
                        }
                        self.layer_clip = Some(self.doc.copy_layer(l));
                        self.doc.delete_layer();
                        Ok(text(format!("Layer {l} cut.")))
                    }
                    "paste" => {
                        let c = self.layer_clip.clone().ok_or("nothing copied yet: use action copy or cut first")?;
                        self.doc.paste_layer(&c);
                        Ok(text(format!("Layer pasted as layer {}.", self.doc.current_layer)))
                    }
                    other => Err(format!("action {other}: use duplicate, copy, cut or paste")),
                }
            }
            "import_psd" => {
                let p = PathBuf::from(str_arg(a, "path")?);
                self.place(a)?;
                let (layers, _, _) = crate::psd::read(&p)?;
                if layers.is_empty() {
                    return Err("this PSD has no layers".into());
                }
                let (w, h) = (self.doc.w, self.doc.h);
                let f = self.doc.frame;
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
                Ok(text(format!("{} layers imported at frame {f}.", layers.len())))
            }
            "import_sequence" | "import_video" => {
                let (files, name, tmp) = if name == "import_video" {
                    let video = PathBuf::from(str_arg(a, "path")?);
                    let ff = io::find_ffmpeg().ok_or("importing a video needs FFmpeg installed (ffmpeg.org)")?;
                    let max = a.get("max_frames").and_then(Value::as_u64).unwrap_or(2000).clamp(1, 2000);
                    let dir = std::env::temp_dir().join(format!("toonlight-mcp-video-{}", std::process::id()));
                    let _ = std::fs::remove_dir_all(&dir);
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let out = std::process::Command::new(ff)
                        .args(["-loglevel", "error", "-i"])
                        .arg(&video)
                        .arg("-vf")
                        .arg(format!("fps={}", self.doc.fps))
                        .arg("-frames:v")
                        .arg(max.to_string())
                        .arg(dir.join("f%05d.png"))
                        .output()
                        .map_err(|e| e.to_string())?;
                    if !out.status.success() {
                        let _ = std::fs::remove_dir_all(&dir);
                        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
                    }
                    let name = video.file_stem().map(|n| n.to_string_lossy().to_string());
                    (picture_files(&dir), name, Some(dir))
                } else {
                    let files = match (a.get("paths").and_then(Value::as_array), a.get("folder").and_then(Value::as_str)) {
                        (Some(list), _) => list.iter().filter_map(Value::as_str).map(PathBuf::from).collect(),
                        (None, Some(dir)) => picture_files(std::path::Path::new(dir)),
                        _ => return Err("give paths (a list of pictures) or folder".into()),
                    };
                    (files, a.get("name").and_then(Value::as_str).map(String::from), None)
                };
                if files.is_empty() {
                    return Err("no pictures to import".into());
                }
                self.doc.push_structure_undo();
                self.doc.add_layer();
                let l = self.doc.current_layer;
                if let Some(n) = name {
                    self.doc.layers[l].name = n;
                }
                if self.doc.frames < files.len() {
                    self.doc.set_length(files.len());
                }
                let (w, h) = (self.doc.w, self.doc.h);
                let mut count = 0;
                for (i, f) in files.iter().enumerate() {
                    if let Ok((px, pw, ph)) = io::load_picture(f) {
                        let id = self.doc.layers[l].cels[i].unwrap_or_else(|| self.doc.new_drawing());
                        self.doc.drawings[id].px = io::fit_picture(px, pw, ph, w, h);
                        self.doc.layers[l].cels[i] = Some(id);
                        count += 1;
                    }
                }
                if let Some(d) = tmp {
                    let _ = std::fs::remove_dir_all(d);
                }
                Ok(text(format!("{count} pictures imported into layer {l}, one per frame.")))
            }
            "color_group" => {
                // act on every layer of a group and answer how many
                let gi = a.get("group").and_then(Value::as_u64).unwrap_or(0).min(12) as usize;
                let action = a.get("action").and_then(Value::as_str).unwrap_or("list");
                // the group editor: one group's colour and name, or every group back to the defaults
                if action == "set" || action == "reset" {
                    let mut table = if action == "reset" {
                        crate::doc::default_groups()
                    } else {
                        self.doc.groups.clone()
                    };
                    if action == "set" {
                        if gi == 0 {
                            return Err("group 0 (no colour) can't be changed: use 1-12".into());
                        }
                        if let Some(c) = a.get("color") {
                            table[gi].color = color_arg(c)?;
                        }
                        if let Some(n) = a.get("name").and_then(Value::as_str) {
                            table[gi].name = n.to_string();
                        }
                    }
                    let n = self.doc.set_groups(&table, &[false; 13]);
                    return Ok(text(format!("{n} groups changed.")));
                }
                let g = self.doc.groups[gi].color;
                if action == "list" {
                    let groups: Vec<Value> = (0..self.doc.groups.len())
                        .map(|i| {
                            let c = self.doc.groups[i].color;
                            let layers: Vec<usize> = (0..self.doc.layers.len()).filter(|&l| self.doc.layers[l].group == c).collect();
                            json!({"group": i, "color": hex(c), "name": self.doc.group_name(i), "layers": layers})
                        })
                        .collect();
                    return Ok(text(json!(groups).to_string()));
                }
                let mut n = 0;
                for layer in self.doc.layers.iter_mut().filter(|l| l.group == g) {
                    match action {
                        "lock" => layer.locked = true,
                        "unlock" => layer.locked = false,
                        "show" => layer.visible = true,
                        "hide" => layer.visible = false,
                        "select" => layer.selected = true,
                        "deselect" => layer.selected = false,
                        other => {
                            return Err(format!(
                                "action {other}: use list, set, reset, lock, unlock, show, hide, select or deselect"
                            ))
                        }
                    }
                    n += 1;
                }
                Ok(text(format!("{n} layers.")))
            }
            "pick_color" => {
                let f = self.frame_arg(a)?;
                let (x, y) = pair(a.get("at").ok_or("at is missing")?)?;
                let (w, h) = (self.doc.w, self.doc.h);
                if x < 0.0 || y < 0.0 || x as usize >= w || y as usize >= h {
                    return Err(format!("the point is outside the canvas ({w}x{h})"));
                }
                let (x, y) = (x as usize, y as usize);
                let i = (y * w + x) * 4;
                let mine = a.get("layer_only").and_then(Value::as_bool).unwrap_or(false);
                let px = if mine {
                    let l = self.layer_arg(a)?;
                    let id = self.doc.shown_drawing(l, f).ok_or("no drawing on this layer there")?;
                    self.doc.drawings[id].px[i..i + 4].to_vec()
                } else {
                    let mut buf = vec![0u8; w * h * 4];
                    self.doc.composite(
                        f,
                        false,
                        Rect {
                            x0: x,
                            y0: y,
                            x1: x + 1,
                            y1: y + 1,
                        },
                        &mut buf,
                    );
                    buf[i..i + 4].to_vec()
                };
                Ok(text(json!({"color": hex([px[0], px[1], px[2]]), "alpha": px[3]}).to_string()))
            }
            "palette" => {
                match a.get("action").and_then(Value::as_str).unwrap_or("list") {
                    "list" => {}
                    "set" => {
                        let list = a.get("colors").and_then(Value::as_array).ok_or("colors is missing")?;
                        self.doc.palette = list.iter().map(color_arg).collect::<Result<_, _>>()?;
                    }
                    "add" => {
                        let c = color_arg(a.get("color").ok_or("color is missing")?)?;
                        self.doc.palette.push(c);
                    }
                    "remove" => {
                        let i = a.get("index").and_then(Value::as_u64).ok_or("index is missing")? as usize;
                        if i >= self.doc.palette.len() {
                            return Err("no colour at that index".into());
                        }
                        self.doc.palette.remove(i);
                    }
                    "load" => self.doc.palette = io::load_palette(std::path::Path::new(str_arg(a, "path")?))?,
                    "save" => io::save_palette(std::path::Path::new(str_arg(a, "path")?), &self.doc.palette)?,
                    other => return Err(format!("action {other}: use list, set, add, remove, load or save")),
                }
                Ok(text(
                    json!({"palette": self.doc.palette.iter().map(|c| hex(*c)).collect::<Vec<_>>()}).to_string(),
                ))
            }
            "undo" => {
                let flag = |k: &str| a.get(k).and_then(Value::as_bool).unwrap_or(false);
                if flag("mark") {
                    // put a mark on the current state, or remove it
                    let step = self.doc.history_step();
                    if step == 0 {
                        return Err("nothing to mark: the history is empty".into());
                    }
                    let on = self.doc.toggle_undo_mark(step);
                    return Ok(text(format!(
                        "Step {step} is {} marked. {}",
                        if on { "now" } else { "no longer" },
                        self.history_json()
                    )));
                }
                if flag("to_mark") {
                    let step = self.doc.undo_mark_from_here(false).ok_or("no undo mark before this state")?;
                    self.doc.go_to_step(step);
                    return Ok(text(format!("Back at the mark of step {step}. {}", self.history_json())));
                }
                if let Some(step) = a.get("to_step").and_then(Value::as_u64) {
                    self.doc.go_to_step(step as usize);
                    return Ok(text(format!("Now at step {}. {}", self.doc.history_step(), self.history_json())));
                }
                Ok(text(if self.doc.undo() { "Undone." } else { "Nothing to undo." }))
            }
            "redo" => {
                if a.get("to_mark").and_then(Value::as_bool).unwrap_or(false) {
                    let step = self.doc.undo_mark_from_here(true).ok_or("no undo mark after this state")?;
                    self.doc.go_to_step(step);
                    return Ok(text(format!("Forward at the mark of step {step}. {}", self.history_json())));
                }
                Ok(text(if self.doc.redo() { "Redone." } else { "Nothing to redo." }))
            }
            "render_frame" => {
                let f = self.frame_arg(a)?;
                let onion = a.get("light_table").and_then(Value::as_bool).unwrap_or(false);
                let max = a.get("max_size").and_then(Value::as_u64).unwrap_or(768).clamp(64, 4096) as u32;
                let (mut w, mut h) = (self.doc.w, self.doc.h);
                let mut buf = vec![0u8; w * h * 4];
                let keep = self.doc.frame;
                self.doc.frame = f;
                self.doc.composite(f, onion, Rect::full(w, h), &mut buf);
                self.doc.frame = keep;
                // through the camera when it's on (and the light table isn't asked for)
                if self.doc.camera.enabled && !onion && !a.get("whole_canvas").and_then(Value::as_bool).unwrap_or(false) {
                    buf = self.doc.camera.shoot(f, &buf, w, h, self.doc.paper);
                    (w, h) = self.doc.output_size();
                }
                let img = image::RgbaImage::from_raw(w as u32, h as u32, buf).ok_or("image")?;
                let scale = (max as f32 / w.max(h) as f32).min(1.0);
                let img = if scale < 1.0 {
                    image::imageops::resize(
                        &img,
                        ((w as f32 * scale) as u32).max(1),
                        ((h as f32 * scale) as u32).max(1),
                        image::imageops::FilterType::Triangle,
                    )
                } else {
                    img
                };
                let png = io::png_bytes(img.as_raw(), img.width() as usize, img.height() as usize)?;
                Ok(json!({"content": [
                    {"type": "image", "data": base64(&png), "mimeType": "image/png"},
                    {"type": "text", "text": format!("Frame {f} of {} ({}x{} shown at {}x{}).", self.doc.frames, w, h, img.width(), img.height())}
                ]}))
            }
            "export" => {
                let p = PathBuf::from(str_arg(a, "path")?);
                match str_arg(a, "format")? {
                    "png" => {
                        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
                        let n = io::export_frames(&self.doc, &p)?;
                        Ok(text(format!("{n} PNG pictures written in {}", p.display())))
                    }
                    "gif" => {
                        io::export_gif(&self.doc, &p, 960)?;
                        Ok(text(format!("GIF written: {}", p.display())))
                    }
                    "mp4" => {
                        let ff = io::find_ffmpeg().ok_or("MP4 export needs FFmpeg installed (ffmpeg.org)")?;
                        io::export_video(&self.doc, &p, &ff)?;
                        Ok(text(format!("Video written: {}", p.display())))
                    }
                    "tiff" | "tga" | "jpeg" | "bmp" => {
                        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
                        let n = io::export_frames_as(&self.doc, &p, str_arg(a, "format")?)?;
                        Ok(text(format!("{n} pictures written in {}", p.display())))
                    }
                    "psd" => {
                        crate::psd::export(&self.doc, self.doc.frame, &p)?;
                        Ok(text(format!(
                            "PSD of frame {} with {} layers written: {}",
                            self.doc.frame,
                            self.doc.layers.len() + 1,
                            p.display()
                        )))
                    }
                    "storyboard" | "film_png" | "film_mp4" => {
                        let docs: Vec<&Doc> = (0..self.clips.len())
                            .map(|i| {
                                if i == self.active {
                                    &self.doc
                                } else {
                                    self.clips[i].2.as_ref().unwrap()
                                }
                            })
                            .collect();
                        match str_arg(a, "format")? {
                            "storyboard" => {
                                let shots: Vec<io::BoardShot> = docs
                                    .iter()
                                    .enumerate()
                                    .map(|(i, d)| {
                                        let (w, h) = d.output_size();
                                        io::BoardShot {
                                            name: self.clips[i].0.clone(),
                                            notes: self.clips[i].1.clone(),
                                            frames: d.frames,
                                            fps: d.fps,
                                            picture: (d.render(0), w, h),
                                        }
                                    })
                                    .collect();
                                let title = self
                                    .path
                                    .as_ref()
                                    .and_then(|p| p.file_stem())
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Storyboard".into());
                                let index = io::export_storyboard_html(&title, &shots, &p)?;
                                Ok(text(format!("Storyboard page written: {}", index.display())))
                            }
                            "film_png" => {
                                std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
                                let n = io::export_film_frames(&docs, &p)?;
                                Ok(text(format!("{n} pictures of every shot written in {}", p.display())))
                            }
                            _ => {
                                let ff = io::find_ffmpeg().ok_or("MP4 export needs FFmpeg installed (ffmpeg.org)")?;
                                io::export_film_video(&docs, &p, &ff)?;
                                Ok(text(format!("Video of every shot written: {}", p.display())))
                            }
                        }
                    }
                    "after_effects" => {
                        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
                        let name = self
                            .path
                            .as_ref()
                            .and_then(|p| p.file_stem())
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "animation".into());
                        let j = io::export_after_effects(&self.doc, &p, &name)?;
                        Ok(text(format!("After Effects JSON written: {}", j.display())))
                    }
                    other => Err(format!(
                        "format {other}: use png, gif, mp4, tiff, tga, jpeg, bmp, psd, after_effects, storyboard, film_png or film_mp4"
                    )),
                }
            }
            _ => Err(format!("unknown tool {name}")),
        }
    }
}

/// An effect from tool arguments (name and its settings).
fn effect_arg(a: &Value) -> Result<crate::fx::Effect, String> {
    let num = |k: &str, d: f64| a.get(k).and_then(Value::as_f64).unwrap_or(d) as f32;
    Ok(match str_arg(a, "name")? {
        "blur" => crate::fx::Effect::Blur {
            rx: num("radius", 8.0),
            ry: num("radius_y", 0.0),
        },
        "mosaic" => crate::fx::Effect::Mosaic {
            sx: num("size", 16.0) as u32,
            sy: num("size_y", num("size", 16.0) as f64) as u32,
        },
        "noise" => crate::fx::Effect::Noise {
            amount: num("amount", 20.0),
            mono: a.get("mono").and_then(Value::as_bool).unwrap_or(true),
        },
        "wave" => crate::fx::Effect::Wave {
            amp_x: num("amplitude_x", 10.0),
            len_x: num("wavelength_x", 80.0),
            amp_y: num("amplitude_y", 0.0),
            len_y: num("wavelength_y", 80.0),
            phase: num("phase", 0.0),
        },
        "brightness_contrast" => crate::fx::Effect::BrightnessContrast {
            brightness: num("brightness", 0.0),
            contrast: num("contrast", 0.0),
        },
        "hue_saturation" => crate::fx::Effect::HueSaturation {
            hue: num("hue", 0.0),
            saturation: num("saturation", 0.0),
            lightness: num("lightness", 0.0),
        },
        "invert" => crate::fx::Effect::Invert,
        "color_key" => crate::fx::Effect::ColorKey {
            color: a.get("color").map(color_arg).transpose()?.unwrap_or([0, 255, 0]),
            tolerance: num("tolerance", 20.0),
        },
        "luma_key" => crate::fx::Effect::LumaKey {
            threshold: num("threshold", 50.0),
            dark: a.get("dark").and_then(Value::as_bool).unwrap_or(false),
        },
        other => return Err(format!("unknown effect {other}")),
    })
}

fn str_arg<'a>(a: &'a Value, k: &str) -> Result<&'a str, String> {
    a.get(k).and_then(Value::as_str).ok_or_else(|| format!("{k} is missing"))
}

fn pair(v: &Value) -> Result<(f32, f32), String> {
    let arr = v.as_array().ok_or("a point is [x, y]")?;
    let x = arr.first().and_then(Value::as_f64).ok_or("a point is [x, y]")?;
    let y = arr.get(1).and_then(Value::as_f64).ok_or("a point is [x, y]")?;
    Ok((x as f32, y as f32))
}

/// Points as [x, y] or [x, y, pressure 0..1].
fn points_arg(a: &Value) -> Result<Vec<(f32, f32, f32)>, String> {
    let arr = a
        .get("points")
        .and_then(Value::as_array)
        .ok_or("points is missing ([[x, y], ...])")?;
    if arr.is_empty() {
        return Err("points is empty".into());
    }
    arr.iter()
        .map(|v| {
            let (x, y) = pair(v)?;
            let p = v.as_array().and_then(|a| a.get(2)).and_then(Value::as_f64).unwrap_or(1.0) as f32;
            Ok((x, y, p.clamp(0.0, 1.0)))
        })
        .collect()
}

/// The pictures of a folder, sorted by name.
fn picture_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                        ["png", "jpg", "jpeg", "tif", "tiff", "tga", "bmp", "gif"].contains(&e.to_ascii_lowercase().as_str())
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

/// A brush connection: {source: off | pressure | speed | random | direction, invert, curve -1..1}.
fn connection_arg(v: &Value) -> Result<brush::Connection, String> {
    let source = match v.get("source").and_then(Value::as_str).unwrap_or("pressure") {
        "off" | "none" => brush::ConSource::Off,
        "pressure" => brush::ConSource::Pressure,
        "speed" => brush::ConSource::Speed,
        "random" => brush::ConSource::Random,
        "direction" => brush::ConSource::Direction,
        "tilt" => brush::ConSource::Tilt,
        "azimuth" => brush::ConSource::Azimuth,
        "twist" => brush::ConSource::Twist,
        other => return Err(format!("source {other}: use off, pressure, speed, random or direction")),
    };
    Ok(brush::Connection {
        source,
        invert: v.get("invert").and_then(Value::as_bool).unwrap_or(false),
        curve: v.get("curve").and_then(Value::as_f64).unwrap_or(0.0).clamp(-1.0, 1.0) as f32,
    })
}

fn tool_arg(s: &str) -> Result<Tool, String> {
    Ok(match s {
        "pencil" => Tool::Pencil,
        "pen" => Tool::Pen,
        "airbrush" => Tool::Airbrush,
        "brush" | "oil" => Tool::Oil,
        "wet_brush" | "wet" => Tool::Wet,
        "eraser" => Tool::Eraser,
        "warp" => Tool::Warp,
        other => return Err(format!("unknown tool {other}")),
    })
}

fn settings_arg(a: &Value, tool: Tool) -> Settings {
    let mut s = tool.defaults();
    if let Some(v) = a.get("size").and_then(Value::as_f64) {
        s.radius = (v as f32).clamp(0.5, 300.0);
    }
    if let Some(v) = a.get("power").and_then(Value::as_f64) {
        s.power = (v as f32).clamp(1.0, 100.0);
    }
    if let Some(v) = a.get("opacity").and_then(Value::as_f64) {
        s.opacity = (v as f32).clamp(1.0, 100.0);
    }
    s
}

fn color_arg(v: &Value) -> Result<[u8; 3], String> {
    if let Some(s) = v.as_str() {
        let h = s.trim_start_matches('#');
        if h.len() == 6 {
            let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| "colour is #rrggbb".to_string());
            return Ok([p(0)?, p(2)?, p(4)?]);
        }
    }
    if let Some(arr) = v.as_array() {
        if arr.len() >= 3 {
            let c = |i: usize| arr[i].as_u64().map(|x| x.min(255) as u8).ok_or("colour is [r, g, b]");
            return Ok([c(0)?, c(1)?, c(2)?]);
        }
    }
    Err("colour is \"#rrggbb\" or [r, g, b]".into())
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

fn tools() -> Value {
    let layer = json!({"type": "integer", "description": "Layer index, 0 = bottom (default: current layer)"});
    let frame = json!({"type": "integer", "description": "Frame index from 0 (default: current frame)"});
    let color = json!({"description": "Colour as \"#rrggbb\" or [r, g, b]", "oneOf": [{"type": "string"}, {"type": "array", "items": {"type": "integer"}}]});
    let point =
        json!({"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2, "description": "[x, y] in canvas pixels"});
    let connection = json!({"type": "object", "properties": {"source": {"type": "string", "enum": ["off", "pressure", "speed", "random", "direction", "tilt", "azimuth", "twist"]}, "invert": {"type": "boolean"}, "curve": {"type": "number"}}});
    let blends: Vec<String> = ALL_BLENDS.iter().map(|b: &Blend| format!("{b:?}").to_lowercase()).collect();
    let papers: Vec<String> = ALL_PAPERS.iter().map(|p: &Paper| format!("{p:?}").to_lowercase()).collect();
    json!([
        {"name": "project_new", "title": "New project", "description": "Start a new empty project, optionally from a template (closes the one in memory; with the window open, the window's).",
         "inputSchema": {"type": "object", "properties": {"width": {"type": "integer"}, "height": {"type": "integer"}, "fps": {"type": "number"}, "template": {"type": "integer", "description": "Optional template (layers named Foreground / Line / Shadow / Color / Background or Animation / Background): 0 basic cartoon 2 s 800x600 30 fps 60 frames, 1 basic cartoon 5 s 800x600 30 fps 150 frames, 2 basic cartoon 10 s 800x600 30 fps 300 frames, 3 film on twos 2 s 2 layers 1280x720 12 fps 24 frames, 4 film on twos 5 s 2 layers 1280x720 12 fps 60 frames, 5 film on twos 10 s 2 layers 1280x720 12 fps 120 frames, 6 film on twos 2 s 5 layers 1280x720 12 fps 24 frames, 7 film on twos 5 s 5 layers 1280x720 12 fps 60 frames, 8 film on twos 10 s 5 layers 1280x720 12 fps 120 frames, 9 TV PAL 16:9 2 s 2 layers 720x405 12.5 fps 25 frames, 10 TV PAL 16:9 10 s 2 layers 720x405 12.5 fps 125 frames, 11 TV PAL 16:9 2 s 5 layers 720x405 12.5 fps 25 frames, 12 TV PAL 16:9 10 s 5 layers 720x405 12.5 fps 125 frames, 13 TV PAL 4:3 2 s 2 layers 768x576 12.5 fps 25 frames, 14 TV PAL 4:3 10 s 2 layers 768x576 12.5 fps 125 frames, 15 TV PAL 4:3 2 s 5 layers 768x576 12.5 fps 25 frames, 16 TV PAL 4:3 10 s 5 layers 768x576 12.5 fps 125 frames, 17 web banner 10 fps 480x60 10 fps 50 frames, 18 web banner 15 fps 480x60 15 fps 75 frames"}}}},
        {"name": "project_open", "title": "Open project", "description": "Open an .toonlight project file.",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
        {"name": "project_save", "title": "Save project", "description": "Save the project as .toonlight (to path, or where it was opened from). If the Toonlight window has this file open without unsaved changes, it reloads it.",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}}},
        {"name": "project_info", "title": "Project info", "description": "Size, frame rate, frames, layers (with the frames where each drawing starts), light table and paper.",
         "inputSchema": {"type": "object", "properties": {}}, "annotations": {"readOnlyHint": true}},
        {"name": "project_settings", "title": "Project settings", "description": "Change frame rate, length in frames, paper colour and texture, and the light table (onion skin).",
         "inputSchema": {"type": "object", "properties": {
            "fps": {"type": "number"}, "frames": {"type": "integer"}, "paper": color, "paper_texture": {"type": "string", "enum": papers},
            "light_table": {"type": "object", "properties": {"active": {"type": "boolean"}, "before": {"type": "integer"}, "after": {"type": "integer"}, "opacity": {"type": "number"}}}}}},
        {"name": "go_to", "title": "Go to", "description": "Make a layer and/or frame current; drawing next / previous jumps to the next drawing change of the layer, mark next / previous to the layer's next marked image, bookmark next / previous to a bookmark.",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "drawing": {"type": "string", "enum": ["next", "previous"]}, "mark": {"type": "string", "enum": ["next", "previous"]}, "bookmark": {"type": "string", "enum": ["next", "previous"]}}}},
        {"name": "layer_copy", "title": "Copy layer", "description": "duplicate (a copy above, default), copy / cut (keep the layer with its drawings to paste, also into another shot) or paste (above the current layer).",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "action": {"type": "string", "enum": ["duplicate", "copy", "cut", "paste"]}}}},
        {"name": "import_psd", "title": "Import a PSD", "description": "Each layer of a Photoshop file (8-bit RGB) becomes a layer, with its name, opacity, visibility and blending, drawn at the frame.",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "frame": frame}, "required": ["path"]}},
        {"name": "import_video", "title": "Import a video", "description": "Put a video's pictures into a new layer, one per frame at the project's frame rate, to draw over them (rotoscoping). Needs FFmpeg. The project gets longer if needed.",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "max_frames": {"type": "integer", "description": "At most this many pictures (default and maximum 2000)"}}, "required": ["path"]}},
        {"name": "import_sequence", "title": "Import a picture sequence", "description": "Put pictures into a new layer, one per frame: paths (in order) or every picture of folder (sorted by name).",
         "inputSchema": {"type": "object", "properties": {"paths": {"type": "array", "items": {"type": "string"}}, "folder": {"type": "string"}, "name": {"type": "string"}}}},
        {"name": "pick_color", "title": "Pick a colour", "description": "The colour at a point of the finished frame (or, with layer_only, of the layer's drawing, with its alpha).",
         "inputSchema": {"type": "object", "properties": {"at": point, "frame": frame, "layer": layer, "layer_only": {"type": "boolean"}}, "required": ["at"]}, "annotations": {"readOnlyHint": true}},
        {"name": "palette", "title": "Colour palette", "description": "The project's colour swatches: list, set (colors), add (color), remove (index), load or save (path, a .gpl palette).",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "set", "add", "remove", "load", "save"]}, "colors": {"type": "array", "items": color}, "color": color, "index": {"type": "integer"}, "path": {"type": "string"}}}},
        {"name": "layer_add", "title": "Add layer", "description": "Add a layer above the current one; it becomes current.",
         "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}}},
        {"name": "layer_set", "title": "Change layer", "description": "Rename a layer or change its visibility, lock, light table, stencil (its drawing protects the other layers from paint), opacity (0-100), blending mode or colour group; collapsed (a thin row in the timeline); end (last frame of the layer's own animation, null = to the end) with what it shows before its first drawing and after its end (before / after: none, repeat, pingpong, hold).",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "name": {"type": "string"}, "visible": {"type": "boolean"}, "locked": {"type": "boolean"}, "stencil": {"type": "boolean"}, "collapsed": {"type": "boolean"},
            "end": {"type": ["integer", "null"]}, "before": {"type": "string", "enum": ["none", "repeat", "pingpong", "hold"]}, "after": {"type": "string", "enum": ["none", "repeat", "pingpong", "hold"]}, "auto_break": {"type": "boolean", "description": "Painting on a held cell makes a new drawing there (project-wide, default true)"}, "auto_stretch": {"type": "boolean", "description": "Painting after a layer's end makes it longer (project-wide, default true); off, painting there is refused"},
            "light_table": {"type": "boolean"}, "opacity": {"type": "number"}, "blend": {"type": "string", "enum": blends},
            "color_group": {"description": "A group number 0-12 (0 = no colour; 1 red, 2 orange, 3 yellow, 4 lime, 5 green, 6 teal, 7 blue, 8 indigo, 9 violet, 10 pink, 11 plum, 12 white) or a colour", "oneOf": [{"type": "integer"}, {"type": "string"}, {"type": "array", "items": {"type": "integer"}}]}}}},
        {"name": "color_group", "title": "Colour group", "description": "Layers by colour group: list (each group with its colour, name and layers), or lock, unlock, show, hide, select or deselect every layer of group (0-12); answers how many layers. Selected layers are the ones actions on several layers use (layer_merge what = selected). set changes the colour and / or name of group 1-12 (its layers follow); reset puts every group back to the defaults.",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "set", "reset", "lock", "unlock", "show", "hide", "select", "deselect"]}, "group": {"type": "integer"}, "color": color, "name": {"type": "string"}}}},
        {"name": "layer_delete", "title": "Delete layer", "description": "Delete a layer, or several at once with layers (one layer always stays).",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "layers": {"type": "array", "items": {"type": "integer"}}}}, "annotations": {"destructiveHint": true}},
        {"name": "layer_merge", "title": "Merge layers", "description": "Merge a layer down onto the layer under it (what = down, default), every visible layer into the lowest visible one (what = visible), or the layers listed in `layers` (else the selected ones) into the lowest of them (what = selected), or those layers put end to end in time, from the bottom up, into one layer (what = join; the reverse of the timing action split). For the merges, the upper layer's opacity and blending are painted in; holds and before / after behaviours are followed. keep_color_group, keep_instance_names and keep_image_marks (default true) keep the colour group, the drawing names and the image marks.",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "what": {"type": "string", "enum": ["down", "visible", "selected", "join"]}, "layers": {"type": "array", "items": {"type": "integer"}}, "keep_color_group": {"type": "boolean"}, "keep_instance_names": {"type": "boolean"}, "keep_image_marks": {"type": "boolean"}}}},
        {"name": "layer_move", "title": "Move layer", "description": "Move a layer up or down in the stack.",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "direction": {"type": "string", "enum": ["up", "down"]}}, "required": ["direction"]}},
        {"name": "frame_add", "title": "Add frame", "description": "After the given frame on a layer: a new blank drawing, a duplicate of the drawing, or hold the drawing one more frame. Moves to the new frame.",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "kind": {"type": "string", "enum": ["blank", "duplicate", "hold"]}}}},
        {"name": "timing", "title": "Timing", "description": "Timing of a layer's cells from frame `from` to `to` (default: the current frame): set_exposure (every drawing lasts `frames` frames), add_exposures / remove_exposures (hold the drawing at `from` `frames` more or less), move_cells (`by` frames, + = later), delete_cells, repeat (`frames` times, mode loop / pingpong / random), stretch (the layer to `frames` frames, or by `ratio` percent: 200 = twice slower, 50 = twice faster; with `layers`, several layers by the same ratio), retime (every drawing of the layer lasts `frames` frames: 1 = on ones, 2 = on twos), trim_start (the layer starts at `from`: what came before is removed), set_number (exposure sheet: frames from..to show drawing `number`), name_drawing (`name`), bookmark (toggle at `from`), mark_in / mark_out (play range start / end at `from`; `clear` removes it), mark_image (give the drawings shown at from..to the colour `mark` 1-12, as keys / breakdowns / in-betweens; 0 removes it), note (write `text` on the timeline at `from`; empty removes it), insert_tweens (`frames` in-between drawings after the drawing at `from`, before the layer's next drawing; each is a cross-fade of the two unless mix is false), shift (move the layer's whole animation `by` frames, + = later; `layers` moves several together; earlier stops at the first frame), slide (move the drawings `by` frames inside the layer, which keeps its first and last frame), split (cut the layer in two at `from`: the rest goes to a new layer above).",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "action": {"type": "string", "enum": ["set_exposure", "add_exposures", "remove_exposures", "move_cells", "delete_cells", "repeat", "stretch", "retime", "trim_start", "set_number", "name_drawing", "bookmark", "mark_in", "mark_out", "mark_image", "note", "insert_tweens", "shift", "slide", "split"]}, "layers": {"type": "array", "items": {"type": "integer"}}, "mark": {"type": "integer", "description": "For mark_image: colour 1-12 (1 red, 2 orange, 3 yellow, 4 lime, 5 green, 6 teal, 7 blue, 8 indigo, 9 violet, 10 pink, 11 plum, 12 white), 0 = no mark"}, "text": {"type": "string"}, "mix": {"type": "boolean"}, "number": {"description": "For set_number: the drawing number shown (as typed in the exposure sheet; a new number makes a new drawing; empty or \"-\" = none)", "oneOf": [{"type": "integer"}, {"type": "string"}]}, "mode": {"type": "string", "enum": ["loop", "pingpong", "random"]},
            "from": frame, "to": frame, "frames": {"type": "integer"}, "ratio": {"type": "number", "description": "For stretch: the new length in percent of the old one"}, "by": {"type": "integer"}, "name": {"type": "string"}, "clear": {"type": "boolean"}}, "required": ["action"]}},
        {"name": "frame_remove", "title": "Remove frame", "description": "Remove a layer's cell at a frame (later cells move left), or hold its drawing one frame less.",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "kind": {"type": "string", "enum": ["cell", "hold"]}}}, "annotations": {"destructiveHint": true}},
        {"name": "draw_stroke", "title": "Draw a stroke", "description": "Paint a stroke through points with a brush tool (aspect, angle and connections shape the dab like the tool settings). Points are [x, y] or [x, y, pressure 0-1] (pressure changes the size, and the power with pressure_power). Size is the brush radius in pixels; power and opacity are 0-100 (defaults per tool, e.g. pen 20/100/100, airbrush 10/30/100). mode mixes the paint with the drawing (color, behind, light, shade, erase, tint, colorize, negative, saturate, unsaturate, multiply, screen, diff, dodge, burn, lighten, darken); preserve_transparency paints only over what is drawn; stencil layers protect their shapes (stencil_inside paints only inside them).",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "points": {"type": "array", "items": point},
            "tool": {"type": "string", "enum": ["pencil", "pen", "airbrush", "brush", "wet_brush", "eraser", "warp", "custom"]}, "color": color, "spacing": {"type": "number"},
            "size": {"type": "number"}, "power": {"type": "number"}, "opacity": {"type": "number"},
            "pressure_size": {"type": "boolean"}, "pressure_power": {"type": "boolean"},
            "aspect": {"type": "number", "description": "Dab roundness 5-100 (100 = round)"}, "angle": {"type": "number", "description": "Dab angle in degrees"},
            "connections": {"type": "object", "description": "What drives the size, power, aspect and angle along the stroke: {size|power|aspect|angle: {source: off, pressure, speed, random or direction, invert, curve -1 (soft) .. 1 (hard)}}. Direction on angle turns the dab with the stroke.",
                "properties": {"size": connection.clone(), "power": connection.clone(), "aspect": connection.clone(), "angle": connection.clone()}},
            "mode": {"type": "string", "enum": brush::ALL_DRAW_MODES.iter().map(|d| d.name()).collect::<Vec<_>>()},
            "preserve_transparency": {"type": "boolean"}, "stencil_inside": {"type": "boolean"}}, "required": ["points"]}},
        {"name": "draw_shape", "title": "Draw a shape", "description": "Draw a line, rectangle or ellipse outline between two corners, optionally filled.",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "shape": {"type": "string", "enum": ["line", "rectangle", "ellipse"]},
            "from": point, "to": point, "color": color, "size": {"type": "number"}, "filled": {"type": "boolean"}}, "required": ["shape", "from", "to"]}},
        {"name": "fill", "title": "Fill", "description": "Flood-fill the area of the drawing at a point with a colour.",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "at": point, "color": color, "tolerance": {"type": "number"}}, "required": ["at"]}},
        {"name": "clear_drawing", "title": "Clear drawing", "description": "Erase the drawing shown on a layer at a frame.",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame}}, "annotations": {"destructiveHint": true}},
        {"name": "draw_text", "title": "Write text", "description": "Write text (several lines allowed) with its top-left corner at a point. Size is the letter height in pixels (default 100); border in pixels (default 2, red, 0 = none); italic in percent; angle in degrees.",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "text": {"type": "string"}, "at": point, "color": color,
            "size": {"type": "number"}, "border": {"type": "number"}, "border_color": color, "italic": {"type": "number"}, "angle": {"type": "number"}}, "required": ["text", "at"]}},
        {"name": "sound_set", "title": "Sound tracks", "description": "The shot's sound tracks, played together. Give a WAV or MP3 path to set a track's file (add=true makes a new track; otherwise track, default 0, is replaced) and/or change that track's volume (0-200 %), offset in frames, mute and fades (seconds); remove=true removes the track (every track when none is named). fit_length makes the shot as long as the sound. Video exports include every track; the film export places each shot's sounds where the shot starts.",
         "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}, "track": {"type": "integer", "description": "Sound track index from 0 (default 0)"}, "add": {"type": "boolean"}, "locked": {"type": "boolean", "description": "A locked track can't be changed, replaced or removed"}, "color": {"type": "integer", "description": "Timeline colour: 0 the usual blue, 1-12 a colour group's colour"}, "volume": {"type": "number"}, "offset_frames": {"type": "number"}, "mute": {"type": "boolean"},
            "fade_in": {"type": "number"}, "fade_out": {"type": "number"}, "fit_length": {"type": "boolean"}, "remove": {"type": "boolean"}}}},
        {"name": "import_image", "title": "Import a picture", "description": "Place a PNG or JPEG picture into the drawing (scaled to fit unless scale, in percent, is given; centred unless center [x, y] is given).",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "path": {"type": "string"}, "scale": {"type": "number"}, "center": point}, "required": ["path"]}},
        {"name": "gradient", "title": "Gradient", "description": "Paint a linear (or radial) gradient from color_from at `from` to color_to at `to`, over the whole drawing or inside rect [x0, y0, x1, y1].",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "from": point, "to": point, "color_from": color, "color_to": color,
            "radial": {"type": "boolean"}, "opacity": {"type": "number"}, "rect": {"type": "array", "items": {"type": "number"}}}, "required": ["from", "to"]}},
        {"name": "transform", "title": "Transform", "description": "Move (dx, dy), scale (percent), rotate (degrees) or mirror the drawing, or only the part inside rect [x0, y0, x1, y1].",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "rect": {"type": "array", "items": {"type": "number"}},
            "dx": {"type": "number"}, "dy": {"type": "number"}, "scale": {"type": "number"}, "angle": {"type": "number"}, "flip_x": {"type": "boolean"}, "flip_y": {"type": "boolean"}}}},
        {"name": "custom_brush", "title": "Custom brush", "description": "Make a custom brush from a picture file (path) or from the drawing inside rect [x0, y0, x1, y1]; then draw_stroke with tool \"custom\" stamps it along the points (size = percent of the picture, spacing = percent of its largest side, default 12.5; giving a color paints its shape in that colour).",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "path": {"type": "string"}, "rect": {"type": "array", "items": {"type": "number"}}}}},
        {"name": "layer_fx", "title": "Layer effect stack", "description": "The layer's effect stack, applied live without changing the drawings: add (an effect with name and settings as in the effect tool; with frame, its first key), key (index, frame and the same effect's settings: the settings move in a straight line between keys), remove (index), clear, apply (paint them into the drawings for good), list, save (write the stack with its keys to the file `path`) and load (replace the layer's stack with the one of the file `path`).",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "action": {"type": "string", "enum": ["add", "key", "remove", "clear", "apply", "list", "save", "load"]}, "index": {"type": "integer"}, "frame": frame, "path": {"type": "string"},
            "name": {"type": "string", "enum": ["blur", "mosaic", "noise", "wave", "brightness_contrast", "hue_saturation", "invert", "color_key", "luma_key"]},
            "radius": {"type": "number"}, "radius_y": {"type": "number"}, "size": {"type": "number"}, "size_y": {"type": "number"}, "amount": {"type": "number"}, "mono": {"type": "boolean"},
            "amplitude_x": {"type": "number"}, "wavelength_x": {"type": "number"}, "amplitude_y": {"type": "number"}, "wavelength_y": {"type": "number"}, "phase": {"type": "number"},
            "brightness": {"type": "number"}, "contrast": {"type": "number"}, "hue": {"type": "number"}, "saturation": {"type": "number"}, "lightness": {"type": "number"},
            "color": color, "tolerance": {"type": "number"}, "threshold": {"type": "number"}, "dark": {"type": "boolean"}}, "required": ["action"]}},
        {"name": "effect", "title": "Effect", "description": "Apply an effect to the drawing at the frame (or every drawing of the layer with all_frames): blur (radius, radius_y), mosaic (size, size_y), noise (amount 0-100, mono), wave (amplitude_x, wavelength_x, amplitude_y, wavelength_y, phase), brightness_contrast (brightness, contrast -100..100), hue_saturation (hue degrees, saturation, lightness -100..100), invert, color_key (color, tolerance), luma_key (threshold 0-100, dark).",
         "inputSchema": {"type": "object", "properties": {"layer": layer, "frame": frame, "name": {"type": "string", "enum": ["blur", "mosaic", "noise", "wave", "brightness_contrast", "hue_saturation", "invert", "color_key", "luma_key"]},
            "all_frames": {"type": "boolean"}, "radius": {"type": "number"}, "radius_y": {"type": "number"}, "size": {"type": "number"}, "size_y": {"type": "number"}, "amount": {"type": "number"}, "mono": {"type": "boolean"},
            "amplitude_x": {"type": "number"}, "wavelength_x": {"type": "number"}, "amplitude_y": {"type": "number"}, "wavelength_y": {"type": "number"}, "phase": {"type": "number"},
            "brightness": {"type": "number"}, "contrast": {"type": "number"}, "hue": {"type": "number"}, "saturation": {"type": "number"}, "lightness": {"type": "number"},
            "color": color, "tolerance": {"type": "number"}, "threshold": {"type": "number"}, "dark": {"type": "boolean"}}, "required": ["name"]}},
        {"name": "camera", "title": "Camera", "description": "Animated camera: turn it on, set its picture size, and set keys {frame, x, y (centre on the canvas), zoom (percent, 100 = whole camera size), rotation (degrees)}; between keys it moves along a smooth curve (or linearly with smooth=false; ease slows in and out). When on, every export and render_frame goes through it. lock_x / lock_y / lock_rotation / lock_zoom keep that value as it is when keys are set.",
         "inputSchema": {"type": "object", "properties": {"enabled": {"type": "boolean"}, "width": {"type": "integer"}, "height": {"type": "integer"}, "smooth": {"type": "boolean"}, "ease": {"type": "boolean"}, "clear_keys": {"type": "boolean"},
            "lock_x": {"type": "boolean"}, "lock_y": {"type": "boolean"}, "lock_rotation": {"type": "boolean"}, "lock_zoom": {"type": "boolean"},
            "keys": {"type": "array", "items": {"type": "object", "properties": {"frame": {"type": "integer"}, "x": {"type": "number"}, "y": {"type": "number"}, "zoom": {"type": "number"}, "rotation": {"type": "number"}}, "required": ["frame"]}}}}},
        {"name": "shot", "title": "Shots (storyboard)", "description": "A project can hold several shots (clips) played one after the other. action: list, add (new empty shot after the active one, becomes active), duplicate (copy of the active shot), select (index), delete (index), rename (index, name, notes). Every other tool works on the active shot; project_save keeps them all.",
         "inputSchema": {"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "add", "duplicate", "select", "delete", "rename"]}, "index": {"type": "integer"}, "name": {"type": "string"}, "notes": {"type": "string"}}}},
        {"name": "undo", "title": "Undo", "description": "Undo the last change. Undo marks: mark=true puts a mark on the current state (or removes it), a place to come back to in one go; to_mark=true goes back to the nearest mark before the current state; to_step goes to the state after that many steps of the history (0 = the start). project_info lists the history and its marks.",
         "inputSchema": {"type": "object", "properties": {"mark": {"type": "boolean"}, "to_mark": {"type": "boolean"}, "to_step": {"type": "integer"}}}},
        {"name": "redo", "title": "Redo", "description": "Redo the last undone change; to_mark=true goes forward to the nearest undo mark after the current state.",
         "inputSchema": {"type": "object", "properties": {"to_mark": {"type": "boolean"}}}},
        {"name": "render_frame", "title": "See a frame", "description": "Render a frame as a PNG picture to look at (optionally with the light table). With the camera on, it shows what the camera sees unless whole_canvas is true.",
         "inputSchema": {"type": "object", "properties": {"frame": frame, "light_table": {"type": "boolean"}, "whole_canvas": {"type": "boolean"}, "max_size": {"type": "integer"}}}, "annotations": {"readOnlyHint": true}},
        {"name": "export", "title": "Export", "description": "Export the animation: png (a folder of numbered pictures), gif, mp4 (needs FFmpeg), after_effects (a folder with JSON + pictures), psd (the current frame, one Photoshop layer per layer plus the paper), tiff / tga / jpeg / bmp (a folder of pictures), storyboard (a folder with a web page, one card per shot with its first picture, name, notes and length; print it to PDF from a browser), film_png / film_mp4 (every shot one after the other).",
         "inputSchema": {"type": "object", "properties": {"format": {"type": "string", "enum": ["png", "gif", "mp4", "after_effects", "psd", "tiff", "tga", "jpeg", "bmp", "storyboard", "film_png", "film_mp4"]}, "path": {"type": "string"}}, "required": ["format", "path"]}}
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(s: &mut Server, id: i64, method: &str, params: Value) -> Value {
        s.handle(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .unwrap()
    }

    #[test]
    fn the_bridge_passes_commands_to_the_window() {
        let dir = std::env::temp_dir().join(format!("toonlight-bridge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("TOONLIGHT_CONFIG_DIR", &dir);
        let rx = start_bridge(|| {}).expect("bridge");
        // the "window": answers with its own project
        std::thread::spawn(move || {
            let mut srv = Server::new(Doc::new(64, 32, 12.0));
            for (line, reply) in rx {
                let _ = reply.send(srv.handle_line(&line).unwrap_or_default());
            }
        });
        let mut s = bridge_connect().expect("connect");
        let mut r = std::io::BufReader::new(s.try_clone().unwrap());
        let mut line = String::new();
        writeln!(
            s,
            "{}",
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "project_info", "arguments": {}}})
        )
        .unwrap();
        r.read_line(&mut line).unwrap();
        assert!(line.contains("64") && line.contains("32"), "the window's project answers: {line}");
        // a notification gets an empty line back, which the client skips
        line.clear();
        writeln!(s, "{}", json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).unwrap();
        r.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "");
        remove_bridge_file();
        std::env::remove_var("TOONLIGHT_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn protocol_round_trip() {
        let mut s = Server::new(Doc::new(200, 100, 12.0));
        let init = call(
            &mut s,
            1,
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "toonlight");
        assert!(s
            .handle(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .is_none());
        let list = call(&mut s, 2, "tools/list", json!({}));
        assert!(list["result"]["tools"].as_array().unwrap().len() >= 27);
        let r = call(
            &mut s,
            40,
            "tools/call",
            json!({"name": "effect", "arguments": {"name": "blur", "radius": 3}}),
        );
        assert!(r["result"]["isError"].is_null(), "{r}");
        let r = call(
            &mut s,
            30,
            "tools/call",
            json!({"name": "gradient", "arguments": {"from": [0, 0], "to": [200, 0], "rect": [0, 80, 200, 100]}}),
        );
        assert!(r["result"]["isError"].is_null(), "{r}");
        let r = call(
            &mut s,
            31,
            "tools/call",
            json!({"name": "transform", "arguments": {"dx": 10, "flip_x": true}}),
        );
        assert!(r["result"]["isError"].is_null(), "{r}");
        let r = call(
            &mut s,
            20,
            "tools/call",
            json!({"name": "draw_text", "arguments": {"text": "Hi", "at": [5, 5], "size": 30}}),
        );
        assert!(r["result"]["isError"].is_null(), "{r}");
        let r = call(
            &mut s,
            21,
            "tools/call",
            json!({"name": "draw_stroke", "arguments": {"points": [[10, 50, 0.2], [100, 50, 1.0]], "size": 10}}),
        );
        assert!(r["result"]["isError"].is_null(), "{r}");
        let r = call(
            &mut s,
            3,
            "tools/call",
            json!({"name": "draw_stroke", "arguments": {"points": [[10, 10], [150, 80]], "color": "#ff0000", "size": 6}}),
        );
        assert!(r["result"]["isError"].is_null(), "{r}");
        let r = call(
            &mut s,
            22,
            "tools/call",
            json!({"name": "draw_stroke", "arguments": {"points": [[10, 10], [150, 80]], "mode": "shade", "preserve_transparency": true}}),
        );
        assert!(r["result"]["isError"].is_null(), "{r}");
        let r = call(
            &mut s,
            23,
            "tools/call",
            json!({"name": "draw_stroke", "arguments": {"points": [[10, 10]], "mode": "sparkle"}}),
        );
        assert_eq!(r["result"]["isError"], true);
        let r = call(
            &mut s,
            24,
            "tools/call",
            json!({"name": "timing", "arguments": {"action": "bookmark", "from": 0}}),
        );
        assert!(r["result"]["isError"].is_null(), "{r}");
        assert_eq!(s.doc.bookmarks, vec![0]);
        let r = call(
            &mut s,
            4,
            "tools/call",
            json!({"name": "frame_add", "arguments": {"kind": "duplicate"}}),
        );
        assert!(r["result"]["isError"].is_null(), "{r}");
        assert_eq!(s.doc.frames, 2);
        let r = call(&mut s, 5, "tools/call", json!({"name": "render_frame", "arguments": {"frame": 1}}));
        assert_eq!(r["result"]["content"][0]["type"], "image");
        let r = call(&mut s, 6, "tools/call", json!({"name": "draw_stroke", "arguments": {"points": []}}));
        assert_eq!(r["result"]["isError"], true);
        let r = call(&mut s, 7, "nope", json!({}));
        assert_eq!(r["error"]["code"], -32601);
    }

    #[test]
    fn the_new_tools_work() {
        let mut s = Server::new(Doc::new(64, 48, 24.0));
        s.doc.set_length(6);
        let ok = |s: &mut Server, name: &str, args: Value| {
            let r = call(s, 9, "tools/call", json!({"name": name, "arguments": args}));
            assert!(r["result"]["isError"].is_null(), "{name}: {r}");
            r["result"]["content"][0]["text"].as_str().unwrap_or("").to_string()
        };
        ok(
            &mut s,
            "draw_stroke",
            json!({"points": [[5, 5, 0.3], [40, 30, 1.0]], "size": 4, "aspect": 40, "angle": 30,
            "connections": {"size": {"source": "speed", "curve": 0.5}, "angle": {"source": "direction"}}}),
        );
        let picked = ok(&mut s, "pick_color", json!({"at": [1, 40]}));
        assert!(picked.contains("#ffffff"), "{picked}");
        ok(&mut s, "layer_copy", json!({"action": "copy", "layer": 0}));
        ok(&mut s, "layer_copy", json!({"action": "paste"}));
        ok(&mut s, "layer_copy", json!({"action": "duplicate"}));
        assert_eq!(s.doc.layers.len(), 3);
        ok(
            &mut s,
            "timing",
            json!({"action": "set_number", "layer": 0, "from": 2, "to": 3, "number": 1}),
        );
        assert_eq!(s.doc.layers[0].cels[2], s.doc.layers[0].cels[0]);
        ok(&mut s, "go_to", json!({"layer": 0, "frame": 0, "drawing": "next"}));
        assert_eq!(s.doc.frame, 2);
        // image marks, timeline notes and in-betweens
        ok(&mut s, "timing", json!({"action": "mark_image", "layer": 0, "from": 2, "mark": 4}));
        assert_eq!(s.doc.mark_at(0, 3), Some(4), "the same drawing shown again has the mark too");
        ok(&mut s, "go_to", json!({"layer": 0, "frame": 0, "mark": "next"}));
        assert_eq!(s.doc.frame, 2);
        ok(&mut s, "timing", json!({"action": "note", "from": 1, "text": "anticipation"}));
        assert!(ok(&mut s, "project_info", json!({})).contains("anticipation"));
        let frames = s.doc.frames;
        ok(
            &mut s,
            "timing",
            json!({"action": "insert_tweens", "layer": 0, "from": 0, "frames": 2}),
        );
        assert_eq!(s.doc.frames, frames + 2);
        ok(&mut s, "undo", json!({}));
        assert_eq!(s.doc.frames, frames);
        // layers in time: shift, slide, split; several layers at once
        let n = s.doc.layers.len();
        ok(&mut s, "timing", json!({"action": "shift", "layer": 0, "by": 2}));
        assert_eq!(s.doc.keys(0)[0].0, 2);
        ok(&mut s, "timing", json!({"action": "shift", "layers": [0], "by": -2}));
        ok(&mut s, "timing", json!({"action": "slide", "layer": 0, "by": 1}));
        assert_eq!(s.doc.keys(0)[1].0, 3);
        ok(&mut s, "timing", json!({"action": "split", "layer": 0, "from": 3}));
        assert_eq!((s.doc.layers.len(), s.doc.layers[0].end), (n + 1, Some(2)));
        ok(&mut s, "layer_merge", json!({"what": "selected", "layers": [0, 1]}));
        assert_eq!(s.doc.layers.len(), n);
        assert_eq!(s.doc.keys(0)[1].0, 3, "the two halves are one layer again");
        // time ratio, retime, trim the start, collapse
        let before: Vec<usize> = s.doc.keys(0).iter().map(|k| k.0).collect();
        ok(&mut s, "timing", json!({"action": "stretch", "layer": 0, "ratio": 200}));
        assert_eq!(s.doc.keys(0)[1].0, before[1] * 2);
        ok(&mut s, "undo", json!({}));
        ok(&mut s, "timing", json!({"action": "retime", "layer": 0, "frames": 3}));
        assert_eq!(s.doc.keys(0)[1].0 - s.doc.keys(0)[0].0, 3);
        ok(&mut s, "timing", json!({"action": "trim_start", "layer": 0, "from": 4}));
        assert_eq!(s.doc.keys(0)[0].0, 4);
        ok(&mut s, "undo", json!({}));
        ok(&mut s, "undo", json!({}));
        assert_eq!(s.doc.keys(0).iter().map(|k| k.0).collect::<Vec<_>>(), before);
        ok(&mut s, "layer_set", json!({"layer": 0, "collapsed": true}));
        assert!(s.doc.layers[0].collapsed);
        ok(&mut s, "layer_set", json!({"layer": 0, "collapsed": false}));
        // undo marks: mark a state, change things, come back to it in one go
        ok(&mut s, "undo", json!({"mark": true}));
        let marked = s.doc.history_step();
        let layers = s.doc.layers.len();
        ok(&mut s, "layer_add", json!({}));
        ok(&mut s, "layer_add", json!({}));
        ok(&mut s, "undo", json!({"to_mark": true}));
        assert_eq!((s.doc.history_step(), s.doc.layers.len()), (marked, layers));
        ok(&mut s, "redo", json!({}));
        ok(&mut s, "undo", json!({"mark": true}));
        ok(&mut s, "undo", json!({"to_step": marked}));
        ok(&mut s, "redo", json!({"to_mark": true}));
        assert_eq!(s.doc.layers.len(), layers + 1);
        assert_eq!(s.info()["history"]["marks"], json!([marked, marked + 1]));
        ok(&mut s, "undo", json!({"to_step": marked}));
        ok(&mut s, "camera", json!({"lock_x": true, "keys": [{"frame": 0, "x": 5, "y": 6}]}));
        assert!(s.doc.camera.lock_x && (s.doc.camera.keys[0].x - 32.0).abs() < 0.01);
        ok(&mut s, "palette", json!({"action": "set", "colors": ["#102030", [1, 2, 3]]}));
        // colour groups: the reference's 12 colours, group 0 for new layers
        assert_eq!(s.doc.layers[1].group, crate::doc::default_group());
        ok(&mut s, "layer_set", json!({"layer": 0, "color_group": 1}));
        assert_eq!(s.doc.layers[0].group, [0xa7, 0x37, 0x37]);
        assert_eq!(ok(&mut s, "color_group", json!({"action": "hide", "group": 1})), "1 layers.");
        assert!(!s.doc.layers[0].visible);
        ok(&mut s, "color_group", json!({"action": "show", "group": 1}));
        // the group editor: a new colour and a name; the group's layers follow
        ok(
            &mut s,
            "color_group",
            json!({"action": "set", "group": 1, "color": "#112233", "name": "Line"}),
        );
        assert_eq!(s.doc.layers[0].group, [0x11, 0x22, 0x33]);
        assert!(ok(&mut s, "color_group", json!({})).contains("\"name\":\"Line\""));
        assert!(ok(&mut s, "project_info", json!({})).contains("\"color_group\":1"));
        ok(&mut s, "color_group", json!({"action": "reset"}));
        assert_eq!(s.doc.layers[0].group, [0xa7, 0x37, 0x37]);
        assert_eq!(s.doc.palette, vec![[16, 32, 48], [1, 2, 3]]);
        let dir = std::env::temp_dir().join(format!("toonlight-mcp-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        io::save_png(&dir.join("a.png"), &vec![255u8; 8 * 8 * 4], 8, 8).unwrap();
        io::save_png(&dir.join("b.png"), &vec![128u8; 8 * 8 * 4], 8, 8).unwrap();
        ok(&mut s, "import_sequence", json!({"folder": dir.display().to_string()}));
        assert_eq!(s.doc.layers.len(), 4);
        // several sound tracks: a tenth of a second of 8-bit mono WAV
        let wav = dir.join("beep.wav");
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(836u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend([1, 0, 1, 0]);
        bytes.extend(8000u32.to_le_bytes());
        bytes.extend(8000u32.to_le_bytes());
        bytes.extend([1, 0, 8, 0]);
        bytes.extend(b"data");
        bytes.extend(800u32.to_le_bytes());
        bytes.extend((0..800).map(|i| if i % 16 < 8 { 200u8 } else { 56 }));
        std::fs::write(&wav, bytes).unwrap();
        let wav = wav.display().to_string();
        ok(&mut s, "sound_set", json!({"path": wav, "volume": 50}));
        ok(
            &mut s,
            "sound_set",
            json!({"path": wav, "add": true, "offset_frames": 12, "fade_out": 0.05}),
        );
        assert_eq!(s.doc.sounds.len(), 2);
        assert_eq!((s.doc.sounds[0].volume, s.doc.sounds[1].offset), (0.5, 0.5));
        ok(&mut s, "sound_set", json!({"track": 1, "mute": true}));
        assert!(s.doc.sounds[1].mute && !s.doc.sounds[0].mute);
        // a locked track refuses changes until it is unlocked
        ok(&mut s, "sound_set", json!({"track": 0, "locked": true, "color": 3}));
        assert_eq!((s.doc.sounds[0].locked, s.doc.sounds[0].color), (true, 3), "changed, then locked");
        let r = call(
            &mut s,
            9,
            "tools/call",
            json!({"name": "sound_set", "arguments": {"track": 0, "volume": 80}}),
        );
        assert_eq!(r["result"]["isError"], true, "a locked track can't be changed");
        let r = call(
            &mut s,
            9,
            "tools/call",
            json!({"name": "sound_set", "arguments": {"track": 0, "remove": true}}),
        );
        assert_eq!(r["result"]["isError"], true, "nor removed");
        ok(&mut s, "sound_set", json!({"track": 0, "locked": false, "volume": 80}));
        assert_eq!((s.doc.sounds[0].locked, s.doc.sounds[0].volume), (false, 0.8));
        // layers joined end to end
        let n = s.doc.layers.len();
        ok(&mut s, "timing", json!({"action": "split", "layer": 0, "from": 3}));
        ok(&mut s, "layer_merge", json!({"what": "join", "layers": [0, 1]}));
        assert_eq!(s.doc.layers.len(), n);
        ok(&mut s, "sound_set", json!({"track": 0, "remove": true}));
        assert_eq!(s.doc.sounds.len(), 1);
        ok(&mut s, "sound_set", json!({"remove": true}));
        assert!(s.doc.sounds.is_empty());
        // an effect stack saved to a file and put on another layer
        let fxfile = dir.join("stack.json").display().to_string();
        ok(
            &mut s,
            "layer_fx",
            json!({"action": "add", "layer": 0, "name": "blur", "radius": 4}),
        );
        ok(&mut s, "layer_fx", json!({"action": "save", "layer": 0, "path": fxfile}));
        ok(&mut s, "layer_fx", json!({"action": "load", "layer": 1, "path": fxfile}));
        assert_eq!(s.doc.layers[1].fx, s.doc.layers[0].fx);
        ok(&mut s, "layer_fx", json!({"action": "clear", "layer": 0}));
        ok(&mut s, "layer_fx", json!({"action": "clear", "layer": 1}));
        let psd = dir.join("t.psd");
        ok(&mut s, "export", json!({"format": "psd", "path": psd.display().to_string()}));
        ok(&mut s, "import_psd", json!({"path": psd.display().to_string()}));
        assert_eq!(s.doc.layers.len(), 9);
        ok(
            &mut s,
            "export",
            json!({"format": "storyboard", "path": dir.join("board").display().to_string()}),
        );
        assert!(dir.join("board").join("index.html").exists());
        ok(
            &mut s,
            "export",
            json!({"format": "film_png", "path": dir.join("film").display().to_string()}),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }
}
