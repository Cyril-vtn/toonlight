# Roadmap

What Toonlight does today and what is planned, compared with what professional 2D frame-by-frame
animation software usually offers. Contributions are welcome on any ❌ or 🟡 line: open an issue
first so we can agree on the design.

Legend: ✅ done · 🟡 partial · ❌ missing. Priority: **P1** core for animators, **P2** important,
**P3** nice to have.

Today: **58** done, **21** partial, **71** missing.

## Next (P1)

1. Drawing: try the macOS pen pressure and tilt with real tablets; pen tilt on Windows and Linux.
2. Layers: time ratio / retime; layer marks, collapse.
3. Sound: tracks for the whole project.
4. Undo marks; camera motion blur and speed profile; storyboard PDF styles.

---

## 1. Drawing tools and brushes

| Feature | Status | Priority |
|---|---|---|
| Airbrush, oil brush, pen, wet brush, pencil, eraser | ✅ | — |
| Chalk, ink pen, "point" brush, propelling pencil | ❌ | P2 |
| Fill-by-scribbling brush and eraser | ❌ | P2 |
| Special brush (blending modes on the brush: smudge, lighten…) | ❌ | P2 |
| Brush draw modes (Color, Behind, Erase, Shade, Light, Colorize, Tint…) | ✅ | — |
| Brush profile editor (curve), opacity maps | ❌ | P2 |
| Pressure curves (size / power / opacity / angle / aspect connections) | ✅ | — |
| Pen tilt, direction and rotation as brush connections, eraser end (macOS; done); other systems, stylus only | 🟡 | P2 |
| Tablet pressure on macOS (to be confirmed on real tablets) | ✅ | — |
| Line smoothing (stabiliser), live and HUD | ✅ | — |
| Preserve transparency (paint only on existing pixels) | ✅ | — |
| Stencil (mask from a layer) | ✅ | — |
| Grain / paper texture on the brush | 🟡 | P2 |
| Colour mixer palettes (pick colours from painted mixes) | ❌ | P3 |
| Jitter (size, opacity, angle randomness) | ❌ | P2 |
| Custom brush from selection, size, angle, mirror | ✅ | — |
| Custom brush cut tools (rectangle, poly, freehand, magic wand), recut, restore, outline | 🟡 | P2 |
| Brush resize shortcuts (double/halve size, width, height, rotate ±90°) | ❌ | P3 |
| Animated custom brushes (animbrush, next/previous, loop) | ❌ | P2 |
| Stamp mode / brush handle corner | ❌ | P3 |
| Reading brush files of other animation programs | ❌ | P3 |
| Tool presets panel | ✅ | — |
| Text tool (size, border, glow, hollow, italic, rotation, spacing) | ✅ | — |
| Warp brush | ✅ | — |
| Healing / clone source | ❌ | P3 |
| Particle brushes | ❌ | P3 |

## 2. Shapes and selections

| Feature | Status | Priority |
|---|---|---|
| Line, rectangle, ellipse (outline / filled) | ✅ | — |
| Freehand line, freehand fill, dot | 🟡 | P2 |
| Spline, B-spline, Bézier lines and fills | ❌ | P2 |
| Circles from 2 or 3 points | ❌ | P3 |
| Flood fill | ✅ | — |
| Selections: rectangle, freehand (lasso), magic wand | ✅ | — |
| Selections: ellipse, polygon, spline, Bézier, B-spline, circles | ❌ | P2 |
| Selection feather / grow / shrink, selection mask display toggle | ❌ | P2 |
| Transform: move, scale, rotate, mirror | ✅ | — |
| Perspective transform (4 points), perspective flip / rotate | ❌ | P2 |
| Crop | ❌ | P2 |
| Panning / position tool (move the whole layer), wrap | 🟡 | P3 |
| Image flip horizontally / vertically / both | ✅ | — |

## 3. Layers

| Feature | Status | Priority |
|---|---|---|
| Add, duplicate, delete, move, rename, visibility, lock, opacity | ✅ | — |
| Blending modes | 🟡 | P2 |
| Colour groups (13, panel, protect, show / hide / lock a group, timeline filter) | ✅ | — |
| Colour group editor (rename, recolour, reset, defaults, save / load) | ✅ | — |
| Merge below / merge visible / merge selected / join selected | ✅ | — |
| Selecting several layers (Ctrl / Shift + click, by colour group) and acting on them together | ✅ | — |
| Cut / copy / paste layers (with their frames) | ✅ | — |
| Position lock | ❌ | P3 |
| Pre / post behaviours (none, repeat, ping-pong, hold) | ✅ | — |
| Layer types: image (still) layer, video layer, CTG (colour) layer | ❌ | P2 |
| New-layer presets (Anim, Background, Colo & Texture, Duplicate structure) | ❌ | P2 |
| Background mode (none, colour, checker) per layer | ❌ | P3 |
| Layer marks, collapse, thumbnails, icon size | ❌ | P3 |
| Stretch / shift / slide / split a layer (done); resize, time ratio, retime | 🟡 | P2 |
| Pick layer under the cursor | ❌ | P3 |

## 4. Timeline, exposures and instances

| Feature | Status | Priority |
|---|---|---|
| Cells, holds, add / remove exposure, insert / duplicate / delete images | ✅ | — |
| Add / remove multiple exposures, set exposure value, break, remove all | ✅ | — |
| Repeat images (loop, ping-pong, random) | ✅ | — |
| Instance names (rename, smart rename, reset), go to head / tail / next / previous instance | 🟡 | P3 |
| Auto break instance / auto create frame when drawing on a hold | ✅ | — |
| Insert in-between (tween) images, cross-faded or blank | ✅ | — |
| Recompute exposures (merge identical drawings, with threshold) | ❌ | P3 |
| Selecting several cells / images / keys, then copy / move them | ✅ | — |
| Image marks (12 colours) on drawings, next / previous marked image | ✅ | — |
| Bookmarks, mark in / mark out (play range) | ✅ | — |
| Timeline notes, zoom in / out, fit to clip / sound / selection, frames ↔ timecode | ✅ | — |
| Exposure sheet (X-sheet) | ✅ | — |
| Timesheet plugin (paper timesheet export / print) | ❌ | P2 |
| Field chart, animator field, safe area | 🟡 | P3 |

## 5. Light table, flipping, registration

| Feature | Status | Priority |
|---|---|---|
| Onion skin before / after, colours, opacity per image, in front / behind | ✅ | — |
| Up to 10 images each side, by image or by frame, per-layer light table | 🟡 | P2 |
| "Out of pegs" editing (edit the next/previous key in the light table) | ❌ | P2 |
| Peg holes registration (align scanned drawings) | ❌ | P3 |
| Flips: manual (hold key) | ✅ | — |
| Flips: configurable sets, rolling, back and forth, loop, by marks | ❌ | P2 |

## 6. View and workspace

| Feature | Status | Priority |
|---|---|---|
| Zoom, pan, fit, 100 % | ✅ | — |
| Rotate the view (±15°), flip the view (mirror check) | ✅ | — |
| Several zoom windows | ❌ | P3 |
| Black preview, shake, toggle aspect / video | ❌ | P3 |
| Grid with snap, guides (lines, rulers, perspective, circles) with snap and editing | 🟡 | P2 |
| Colour checker display | ❌ | P3 |
| Rooms (workspaces) and movable / dockable panels | ✅ | — |
| Playback remote panel | ✅ | — |
| Fast playback (every core, effects prepared ahead) | ✅ | — |
| Custom panels (buttons running commands / scripts) | ❌ | P3 |
| Colour schemes, save your own | 🟡 | P3 |
| Undo history panel, multi-undo, undo marks | 🟡 | P2 |
| Stopwatch (time an action) | ❌ | P3 |

## 7. Colour

| Feature | Status | Priority |
|---|---|---|
| A / B colours, picker, palette, load / save `.gpl` | ✅ | — |
| Colour shortcuts (hue / saturation / luminosity / RGB ±) | ❌ | P3 |
| Floating colour picker, colour range | ❌ | P3 |
| More palette formats and several palettes | ❌ | P3 |
| Colour-and-texture (CTG) workflow, squiggles, apply colorization | ❌ | P2 |

## 8. Effects (FX)

Each layer has an **effect stack** (several effects in a row, shown live, animated with keys);
9 effects so far. A stack can be saved as a preset or a file and put on another layer.

| Effect | Status | Priority |
|---|---|---|
| Gaussian blur | ✅ | — |
| Mosaic, noise, wave, negative, brightness / contrast, hue / saturation, colour key, luma key | ✅ | — |
| FX stack (chain, save, load, presets, preview) and keyframed FX | ✅ | — |
| Curves, colour adjust, histogram, posterize, black-and-white converter, colour replacer, colour eraser, colour range | ❌ | P2 |
| Advanced keyer (matte, blur matte, gain / offset) | ❌ | P2 |
| Blurs: cubic, median, radial, directional, centre, chroma, motion | ❌ | P2 |
| Glow, bloom, drop shadow, bevel, bump, lighting, volumetric light, lens flare | ❌ | P2 |
| Grain, half-tone, Perlin noise, plasma, background generator, kaleidoscope, mirror, tornado / twirl, warp grid, blind | ❌ | P3 |
| Erode, convolution (3×3, 5×5), fix motion, speed, spare mixer, blender, lumix | ❌ | P3 |
| Scan cleaner (black-and-white and extended) | ❌ | P2 |
| Optical flow, stabilisation (1 point), roto-tracking | ❌ | P3 |
| Particles generator (emitters, world, life, bounce…) | ❌ | P3 |
| Multi-plane camera, position, simple / multiline text, time code generator, sliders | ❌ | P3 |
| Transitions | ❌ | P3 |

## 9. Camera

| Feature | Status | Priority |
|---|---|---|
| Camera keys (position, zoom, rotation), smooth / linear, size, view through it, exports | ✅ | — |
| Camera locks (X, Y, rotation, zoom, all), motion blur, speed profile curve | 🟡 | P2 |
| Follow camera display, camera handles on canvas | ❌ | P2 |
| Render camera to a new project / clip | ❌ | P3 |
| Export camera to LightWave | ❌ | P3 |

## 10. Project, clips, scenes, storyboard

| Feature | Status | Priority |
|---|---|---|
| Several shots, storyboard cards, notes, reorder, play in a row, export the film | ✅ | — |
| Scenes (groups of clips): add, split, join, duplicate, navigate | ❌ | P2 |
| Clip operations: split, insert, hide / show, copy, new project from selected clips | ❌ | P2 |
| Storyboard views (thumbnails, rows, columns, timeline), drawing panels per shot, sound per shot | 🟡 | P2 |
| Storyboard export: PDF / HTML / EDL with styles, publish | 🟡 | P2 |
| Project templates (cartoon, film, TV PAL, web banner…) | ✅ | — |
| Page sizes, pixel aspect, interlaced fields | 🟡 | P3 |
| Resize page, modify project, project header (author, notes) | ❌ | P2 |
| Image library (shared pictures across the project) | ❌ | P2 |
| Spare page (second picture to copy from / swap) | ❌ | P3 |
| Project notes / sticky notes on the canvas | ❌ | P3 |
| Alternative paths for missing files | ❌ | P3 |

## 11. Sound

| Feature | Status | Priority |
|---|---|---|
| Sound per shot: waveform, volume, offset, fades, mute, export with MP4 | ✅ | — |
| Several sound tracks per shot, lock, colour (done); tracks for the whole project | 🟡 | P2 |
| Sound scrubbing (hear while moving the playhead) | ✅ | — |
| Metronome, beeps | ❌ | P3 |
| Sound across shots in the film export | ✅ | — |
| Audio sequence export | ❌ | P3 |

## 12. Import and export

| Feature | Status | Priority |
|---|---|---|
| PNG sequence, GIF, MP4 (FFmpeg), After Effects JSON + pictures | ✅ | — |
| Import PNG / JPEG picture and sequence | ✅ | — |
| Video import (FFmpeg) — rotoscoping | 🟡 | P3 |
| Export / import PSD with layers | ✅ | — |
| TGA, TIFF, BMP, DPX, Cineon, PCX, SGI, SUN, ILBM, Softimage, PostScript | 🟡 | P2 |
| AVI, FLI/FLC, MOV / other codecs through FFmpeg settings | ❌ | P2 |
| Sprite sheet generator | ❌ | P2 |
| CSV / XML timesheet export | ❌ | P2 |
| Package file to share brushes / panels / tools | ❌ | P3 |
| Print | ❌ | P3 |
| Clipboard image copy / paste with other programs | ✅ | — |
| Open project files of other animation programs | ❌ | P3 |

## 13. Capture, scripting, plugins

| Feature | Status | Priority |
|---|---|---|
| Scripting language | 🟡 | P2 |
| AI control over MCP (39 tools, live on the open project) | ✅ | — |
| Plugins / extensions | ❌ | P3 |
| Screen recorder, video grab (camera / capture card) | ❌ | P3 |
| Changelog / what's new panel | ❌ | P3 |

## 14. Settings and accessibility

| Feature | Status | Priority |
|---|---|---|
| Languages (6), shortcuts editable, export / import settings | ✅ | — |
| Shortcuts for mouse / pen buttons, right mouse button tool | ❌ | P2 |
| Preferences window (undo depth, memory, autosave, paths) | ✅ | — |
| Autosave and crash recovery | ✅ | — |

---
