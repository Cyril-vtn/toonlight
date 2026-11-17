# Changelog

All notable changes to Toonlight. The format follows [Keep a Changelog](https://keepachangelog.com)
and versions follow [Semantic Versioning](https://semver.org). What is planned is in
[`docs/ROADMAP.md`](docs/ROADMAP.md).

## [Unreleased]

- CI: clippy now passes with no warnings on Rust 1.100 and is blocking (`-D warnings`); workflow
  actions updated (checkout v7, upload-artifact v7, download-artifact v8, action-gh-release v3);
  rfd updated to 0.18. Dependabot groups action and patch updates and leaves eframe / egui /
  rodio minor versions to be ported by hand.
- **New name: Toonlight** (was AnimCraft). Projects are now `.toonlight` files; `.animcraft` and
  `.atanim` files still open, and the old settings folder is moved to the new name.
- Faster playback: frames are composed on every core and layer effects are prepared ahead.
- Colour groups: the 13 groups with their colours, a Colour groups panel, timeline filter.
- **Live AI bridge**: with the window open, the AI (MCP) works directly on the project on screen;
  every change appears at once and can be undone. Without the window it works alone as before.
- AI (MCP): new `layer_fx` tool (layer effect stack with keys); `project_new` takes a `template`.
- AI (MCP): new tools `layer_copy` (duplicate, copy, cut, paste), `import_psd`, `import_video`,
  `import_sequence`, `pick_color` and `palette`; `draw_stroke` takes the dab aspect, angle and
  connections (pressure, speed, random, direction with invert and curve); `camera` takes the locks;
  `timing` takes `set_number` (exposure sheet); `go_to` jumps to the next / previous drawing or
  bookmark; `export` writes a storyboard page and every shot as one film (pictures or MP4).
- **Remote panel** (Windows > Remote): sound, loop and camera toggles, playback and jump buttons.

- **Layer effect stack** (Effects > Layer effects): several effects in a row on a layer, shown live
  without changing the drawings, switched on / off, reordered, and animated with keys (the settings
  change smoothly between two keys). "Apply to the drawings" makes them permanent.

- **Copy, cut and paste whole layers** with their drawings, also between projects (Layer menu).
- **Stretch a layer** to a new length: its drawings are spread proportionally (Layer menu; AI: `timing`
  action `stretch`).
- **Camera locks**: X, Y, rotation and zoom can be locked while moving the camera.

- **Brush connections**: size, power, aspect and angle can each follow the pen pressure, the speed,
  chance or the stroke's direction, inverted or not, through a soft-to-hard curve (tool settings).

- **Editable exposure sheet**: double-click a cell and type a drawing number (a new number makes a
  new drawing) or "-" to hold. A drawing used twice keeps the same number everywhere.

- **Project templates** in New project: basic cartoon, film on twos, TV PAL 16:9 and 4:3, web
  banner — size, frame rate, length and named layers.
- **Repeat cells** (Cells menu): repeat the selection N times in a loop, back and forth, or at random.
  AI (MCP): `timing` action `repeat`.
- **Undo history** (Edit menu): see every step and jump back or forward to any of them.

- **Preferences window** (Edit > Preferences…): backup copies, recovery copies, undo memory, new
  projects' auto break / auto stretch, sound scrubbing; kept between sessions.
- **Backup copy at each save** (off by default): the previous version is kept as
  "name.toonlight.YYYYMMDD.HHMMSS", next to the project or in a chosen folder.
- **Crash recovery**: unsaved work is copied every 2 minutes and offered back after a crash.
- **Copy / paste pictures with other programs** (system clipboard).
- **PSD with layers**: export the current frame (File > Export), import a PSD's layers (File > Import).
  AI (MCP): export format `psd`.
- **Video import** (File > Import, with FFmpeg): the frames go into a new layer to draw over.
- **Storyboard as a web page** (Storyboard window): one card per shot, printable to PDF.
- **Flip** the selection or the whole drawing (Image menu).
- **TIFF, TGA, JPEG, BMP** picture sequences (File > Export) and imports. AI (MCP): export formats
  `tiff`, `tga`, `jpeg`, `bmp`.

- **Merge layers** (Layer menu): with the layer below, or every visible layer; the upper layer's
  opacity and blending are painted in, holds and before / after behaviours are followed; the merged
  layer takes the upper layer's colour group and drawing names are joined ("upper-lower"). AI (MCP): `layer_merge`.

- **Fixed**: undoing a stroke left its first dab behind (a press could start the stroke twice).
- **Fixed**: the left column could not be made narrower; its panels now scroll sideways when the
  column is narrow.

- **Save as** offers a free name next to the open project: "walk(2)", "walk(3)"… (an existing
  number in brackets is replaced, not added to).
- **Save** asks where to save when the project came from an older `.atanim` file or its file was
  moved or deleted, instead of writing to a path that no longer fits.

## [0.15.0] — 2026-11-12 — Toonlight

- **New name: Toonlight**, with its own icon, and a public repository with continuous integration,
  installers for Windows (setup .exe), macOS (.dmg) and Linux (.deb, .AppImage) built for every
  version, a contributing guide, a code of conduct and a security policy.
- Project files are now `.toonlight`; `.atanim` files from earlier versions still open.
- Release builds are smaller and faster (link-time optimisation, stripped); no console window behind
  the app on Windows.
- Code formatted with `rustfmt`.

## 0.14 — movable panels and rooms

- **Menu names of the reference** in every language: the Clip menu and the Image menu (English
  "Clip" / "Image", French "Clip" / "Image"), while the timeline keeps saying "frame".
- **Panels move with the mouse**: every panel (Tools, Colours, Tool settings, Presets, Light table,
  Sound, Project, Shortcuts, Colour correction) has a title bar; drag it into the other column,
  between two panels, or onto the canvas to make it a floating window; ↗ detaches it, ⇤ / ⇥ dock it
  back, ✕ closes it (Windows menu to open it again). Columns are resizable.
- **Pinned or sliding columns and timeline**, as in the reference: an unpinned column folds into a
  thin strip and slides out over the canvas when the mouse reaches it; an unpinned timeline folds to
  its play controls and opens when the mouse touches it.
- **The reference's four rooms** (workspaces), switched with 1 2 3 4 at the right of the menu bar or
  in Windows > Rooms: Room 1 and 3 (left column pinned, presets column sliding, timeline 340 px),
  Room 2 (everything sliding, for drawing), Room 4 (timeline folded to 29 px, colour-correction
  panel floating). Each room remembers its own layout; rename, export, import, reset.
- New **Colour correction** panel (brightness / contrast, hue / saturation, invert).
- **Each room keeps its own keyboard shortcuts** (saved when you leave it);
  Room 1…4 and "pin / fold the left / right / both columns" can be given shortcuts (none by default,
  like the reference).

## 0.13 — the reference's layout

- **Same places as the reference**: the menu bar
  is File, Edit, Project, Shot, Layer, Frame, Effects, View, Windows, Help in that order; the left
  column holds the tools, then the colours, then the tool settings; the right column starts with
  the tool presets; the timeline is docked at the bottom, 340 px tall.
- Preferences (language, theme, shortcuts, settings export / import) moved to **Edit > Preferences**;
  imports are grouped under **File > Import**; layer, frame and shot commands have their own menus.
- **Windows menu**: show or hide the left column, the right column and the timeline, open the
  exposure sheet, storyboard and shortcuts, and go back to the default layout.
- **Auto stretch layer** (Layer menu, on by default like the reference): painting after a layer's
  end makes it longer; turned off, the app asks "Stretch layer?" first.
- AI (MCP): layer_set takes `auto_stretch`.

## 0.12 — timing work

- **Several cells at once**: Shift + click (or drag) in the timeline selects a run of cells. Move them
  one frame left / right (◀ ▶, Alt+← / Alt+→), copy and paste them (Cells menu), delete them.
- **Exposures by a number**: Hold + / Hold − add or remove the number of frames chosen beside them;
  **Set exposure** makes every drawing of the selection last exactly that many frames.
- **Drawing (instance) names**: each drawing shows its number in the timeline; give it a name in the
  field of the timeline bar (shown on its cells, kept in the file and the AI connection).
- **Bookmarks** (Shift+B, next / previous with Shift+. / Shift+,) and **mark in / mark out**
  (Shift+I / Shift+O): playback loops between the marks; frames outside are darkened. Also in the
  Marks menu and by right-clicking the timeline.
- **Layer before / after behaviours**: a layer can end at a frame ("Layer ends here"); after its end
  it shows nothing, repeats, plays back and forth, or holds its last drawing — and before its first
  drawing the same choices. Exports and the light table follow them.
- **Sound scrubbing**: drag in the timeline to hear a tenth of a second of the sound at each frame
  (or always, or never; in the Sound panel).
- New layers hold their last drawing after their end, and painting on
  a held cell makes it a drawing of its own (a copy), as the reference does by default; this can be
  turned off in the Animation menu.
- **AI (MCP)**: new `timing` tool (set exposure, add / remove exposures, move / delete cells, name a
  drawing, bookmarks, mark in / out); layer_set takes `end`, `before`, `after` (30 tools).

## 0.11 — drawing comfort

- **Draw modes** for every brush (Mode list in the tool settings): colour, behind, light, shade,
  erase, tint, colorize, negative, saturate, unsaturate, multiply, screen, difference, dodge, burn,
  lighten, darken — 17 of the reference's modes, with its names and numbers.
- **Preserve transparency** (/ key): paint only over what is already drawn.
- **Stencil layers** (4th icon in the timeline): a layer's drawing protects the other layers from
  paint; "paint inside the stencil" inverts it. Works with selections, fills and gradients.
- **Line smoothing** (Shift+L): pulled string, average or average points, with a strength, catch up
  at the end of the stroke and the string drawn on the canvas.
- **Rotate the view** by 15° (Shift+← / Shift+→, Shift+↓ to straighten) and **mirror the view**
  (Shift+H) to check a drawing; also in the View menu. The picture itself does not change.
- **Position tool** (P): drag the whole drawing of the layer.
- **Undo** now keeps as many steps as fit in 256 MB, like the reference, instead of 50.
- **AI (MCP)**: draw_stroke takes `mode`, `preserve_transparency` and `stencil_inside`; layer_set
  takes `stencil`.

## 0.10 — custom shortcuts, settings to carry to another computer

- **Keyboard shortcuts you can change** (Edit > Keyboard shortcuts…): every action and tool, with
  search, change (press the new key), add a second shortcut, none, default, all to defaults. A key
  given to one action is taken from the one that had it (with a message). Saved right away; the
  Shortcuts panel and tool tips show your keys.
- **Export / import the shortcuts** as a `.cfg` file in the reference's style (`[Keyboard]`, one
  `key=action` line each, "equal" and "braquet" for `=` and `[`).
- **Export / import all my settings** (File > Settings): one JSON file with the shortcuts, tool
  presets (custom brush pictures included), language, theme and palette, to set up another
  computer in one step.
- `--shortcuts` start option opens the shortcuts window (for checks).

## 0.9 — animated camera, shots and storyboard

- **Animated camera** (Camera tool, M): its own picture size (the canvas size by default), keys
  with position, zoom and rotation on any frame, smooth (curve) or linear moves, optional slow in /
  slow out. Drag on the canvas to move it at the current frame; its frame and path are drawn on the
  canvas; "View through the camera" shows what it sees. When it's on, every export (PNG, GIF, MP4)
  goes through it, and the After Effects export writes one camera point per frame.
- **Shots and storyboard** (Animation > Storyboard): a project can hold several shots, each with
  its own layers, frames, sound and camera. Cards with a thumbnail, name, length and notes; add,
  duplicate, reorder, delete, open; "play every shot in a row". **Export the whole film** (every
  shot one after the other) as PNG or MP4. Projects with one shot are still saved in the old format.
- **AI (MCP)**: `camera` and `shot` tools; `render_frame` shows what the camera sees (29 tools).
- Fixed: the New project window's cancel button said "Undo".

## 0.8 — effects, warp tool, text glow and outline

- **Effects menu** with a live preview: blur (gaussian, the reference's weights), mosaic, noise,
  wave, brightness / contrast, hue / saturation / lightness, invert, colour key and luma key.
  Apply to the current drawing or to every drawing of the layer; only inside the selection when
  there is one; one undo step per drawing.
- **Warp tool** (Q): pushes the pixels under the brush along the stroke (radius 30, power 5 by
  default, like the reference's warp tool settings).
- **Text**: glow (8 px, green by default) and hollow letters (2 px outline), off by default as in
  the reference's text settings.
- **AI (MCP)**: `effect` tool; `draw_stroke` accepts `tool: "warp"` (27 tools).

## 0.7 — custom brushes and tool presets

- **Custom brush tool** (C): turn the selection (or a loaded PNG / JPEG) into a brush that is
  stamped along the stroke, like the reference's custom brush (tooltype 5). Size in percent,
  angle, opacity, mirror ↔ / ↕, spacing in percent of the brush's largest side (12.5 % by default,
  the reference's `stepval`), and an option to paint its shape with colour A. Stays inside the
  selection. "Make a brush from the selection" button in the Selection tool.
- **Tool presets**: a Presets panel of one-click buttons; the first start offers the reference's
  size bins (pen 20-80, airbrush 10-150, wet brush 20-80, pencil 30, brush 30). Save the current
  tool (custom brushes keep their picture), Alt + right-click to delete. Stored in the user's
  settings folder.
- **AI (MCP)**: `custom_brush` tool, and `draw_stroke` with `tool: "custom"` (26 tools).

## 0.6 — selections, transform, gradients, import, palettes

- **Selection tool** (S): rectangle, lasso and magic wand (tolerance 15 % by default, estimated);
  Shift adds, Alt subtracts; select all (Ctrl/Cmd+A), invert, deselect (Ctrl/Cmd+D, Esc).
  Brush strokes, fills and gradients only change the selected pixels.
- **Copy / cut / paste / erase** the selection (Ctrl/Cmd+C, X, V, Del).
- **Transform**: the selection becomes a floating piece you drag, scale, rotate and mirror, then
  apply with Enter (one undo step) or cancel with Esc. Quick mirror ↔ / ↕ buttons.
- **Gradient tool** (D): linear or radial, from colour A to colour B with a midpoint (the
  reference's default gradient: stops at 0 and 1, midpoint 0.5, full opacity).
- **Import a picture** (PNG, JPEG) as a floating piece, scaled to fit; **import a picture sequence**
  as a new layer, one drawing per frame.
- **Palettes**: add the current colour, Alt + right-click to remove, load / save GIMP palettes
  (`.gpl`); the palette is saved with the project.
- **AI (MCP)**: new tools `import_image`, `gradient` and `transform` (25 tools).
- Documentation is now in English.

## 0.5 — sound, text, pressure, exposure sheet, flipping, guides

- **Pen pressure**: "pressure → size" and "pressure → power" for each painting tool (off by
  default, like the reference's `consize=0`, `conpower=0`), from tablets and touch screens the
  system reports, or simulated from the speed with a mouse.
- **Sound**: WAV or MP3 import, playback in sync with the animation (restarted on each loop),
  waveform in the timeline, volume, offset in frames, mute, fade in / out, replace / remove. The
  project keeps the file's path, like the reference. MP4 export includes the sound.
- **Exposure sheet** (Animation > Exposure sheet): one row per frame, one column per layer, the
  drawing's number where it starts and a line while it is held, plus the sound; click to go there.
- **Text tool** (K): multi-line text, size, border (on, 2 px, red, as in the reference's
  settings), italic, rotation, letter spacing, opacity; Chinese and Japanese with a system font.
- **Flipping**: hold `,` or `.` to see the previous or next drawing, back to the starting frame on
  release (`comebacktoorigin=1` in the reference's settings).
- **Guides** (View menu): grid with adjustable spacing, rule of thirds, centre and safe area.
- **AI (MCP)**: `draw_text`, `sound_set`; `draw_stroke` accepts `[x, y, pressure]` points.

## 0.4 — languages and AI
- Six languages (French, English, Spanish, German, Simplified Chinese, Japanese), with English as
  the base for missing texts (the reference's rule).
- MCP server to drive the app from Claude or any compatible AI assistant; Help > Connect an AI
  window; automatic reload of a project changed by the AI.

## 0.3 — reference brushes, exports, open source
- Brush engine following the reference's rules (spacing, strength, profile, aspect, angle).
- Layer blending modes and colour groups; paper textures.
- GIF, MP4 (FFmpeg) and After Effects (JSON + pictures) export.
- MIT licence, README.

## 0.1 — base
- Drawing (pencil, pen, airbrush, brushes, eraser, fill, shapes, picker), layers, timeline with
  holds, loop playback, onion skin, undo / redo, `.toonlight` projects, PNG export, themes.

## Still missing

The full, detailed list (145 features of the reference software in 14 areas, each marked done,
partial or missing, with a priority and where it was found) is in
[`docs/ROADMAP.md`](docs/ROADMAP.md). Today: 34 done, 18 partial, 93 missing.

Next, in order (the P1 items of that list):

1. Layers: merge below / visible / selected.
2. Exchange: PSD with layers, video import, image clipboard with other programs, TIFF / TGA.
3. Safety: preferences window and autosave.
4. Production: FX stack with keyframes, storyboard PDF / HTML export, project templates.
