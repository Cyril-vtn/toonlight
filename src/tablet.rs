//! Pen tablet data: pressure, tilt, barrel rotation and the eraser end.
//!
//! The window library passes on where the pointer is, but not what the pen itself says. This
//! module listens to each system's own events and keeps what the pen last said, for the canvas
//! to read:
//! - macOS: the app's events (mouse events sent by a pen, tablet points, proximity);
//! - Windows: the pointer messages of the window's queue (pen reports);
//! - Linux and the BSDs: the X server's raw device events, read on a connection of its own (the
//!   pressure too, which the window library does not give there). Under Wayland the app runs
//!   through XWayland for that reason (see `main.rs`).

use std::sync::Mutex;

/// What the pen is doing: pressure 0..=1, tilt -1..=1 on each axis (0 = upright; x to the right,
/// y towards the bottom of the screen, that is towards the person), barrel rotation in degrees,
/// and whether its eraser end is the one in use.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pen {
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub rotation: f32,
    pub eraser: bool,
}

impl Pen {
    /// How far the pen leans: 0 upright ..= 1 flat.
    pub fn tilt(&self) -> f32 {
        (self.tilt_x * self.tilt_x + self.tilt_y * self.tilt_y).sqrt().min(1.0)
    }

    /// Which way the pen leans, as 0..1 of a turn (0 = to the right, a quarter = down the screen).
    pub fn azimuth(&self) -> f32 {
        if self.tilt() < 0.01 {
            return 0.0;
        }
        (self.tilt_y.atan2(self.tilt_x) / std::f32::consts::TAU).rem_euclid(1.0)
    }

    /// The barrel rotation as 0..1 of a turn.
    pub fn twist(&self) -> f32 {
        (self.rotation / 360.0).rem_euclid(1.0)
    }
}

/// A system event, reduced to what matters here. `tablet` is the pen's data when the event came
/// from a pen, None when it came from a mouse or a trackpad.
#[derive(Clone, Copy, Debug, PartialEq)]
enum PenEvent {
    Down {
        tablet: Option<Pen>,
    },
    Move {
        tablet: Option<Pen>,
    },
    /// The X server's raw events don't say when a press ends; nothing is lost, the release
    /// keeps the last values anyway.
    #[cfg_attr(not(any(target_os = "macos", windows, test)), allow(dead_code))]
    Up,
    /// macOS says which end of the pen comes near the tablet apart from the pen's reports.
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    Proximity {
        entering: bool,
        eraser: bool,
    },
}

#[derive(Default)]
struct State {
    /// The press going on (or the last one) was made with a pen.
    pen_press: bool,
    pen: Pen,
    eraser: bool,
}

impl State {
    fn apply(&mut self, e: PenEvent) {
        match e {
            PenEvent::Down { tablet: Some(p) } => {
                self.pen_press = true;
                self.pen = p;
            }
            // a press with a mouse: the pen's values no longer apply
            PenEvent::Down { tablet: None } => self.pen_press = false,
            PenEvent::Move { tablet: Some(p) } if self.pen_press => self.pen = p,
            PenEvent::Move { .. } => {}
            // the release keeps the last values, so the stroke ends as it was going
            PenEvent::Up => {}
            PenEvent::Proximity { entering, eraser } => self.eraser = entering && eraser,
        }
    }

    fn sample(&self) -> Option<Pen> {
        self.pen_press.then_some(Pen {
            // the eraser end: said with each report, or (macOS) when the pen came near
            eraser: self.eraser || self.pen.eraser,
            ..self.pen
        })
    }
}

static STATE: Mutex<State> = Mutex::new(State {
    pen_press: false,
    pen: Pen {
        pressure: 0.0,
        tilt_x: 0.0,
        tilt_y: 0.0,
        rotation: 0.0,
        eraser: false,
    },
    eraser: false,
});

fn apply(e: PenEvent) {
    if let PenEvent::Down { tablet: Some(p) } = e {
        log(|| format!("a pen takes over: {p:?}"));
    }
    if let Ok(mut s) = STATE.lock() {
        s.apply(e);
    }
}

/// With the `TOONLIGHT_PEN_LOG` variable set, what the pen listener finds is written to the
/// error output: the way to check a tablet on a computer where nothing else can be looked at.
fn log(text: impl FnOnce() -> String) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("TOONLIGHT_PEN_LOG").is_some_and(|v| !v.is_empty())) {
        eprintln!("pen: {}", text());
    }
}

/// The pen's values for the press going on, when it is made with a pen the system reports.
/// None with a mouse, and when no pen was seen.
pub fn sample() -> Option<Pen> {
    STATE.lock().ok().and_then(|s| s.sample())
}

/// Start listening to the system's pen events. Call once, from the main thread, when the window
/// exists.
pub fn install() {
    #[cfg(target_os = "macos")]
    mac::install();
    #[cfg(windows)]
    win::install();
    #[cfg(all(unix, not(target_os = "macos")))]
    x11::install();
}

/// A Windows pen report as a `Pen`: pressure 0..=1024, rotation in degrees, tilt in degrees
/// (-90..=90: x to the right, y towards the person), and its flags (2 = the pen is turned over,
/// 4 = the eraser touches).
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn pen_from_windows(pressure: u32, rotation: u32, tilt_x: i32, tilt_y: i32, flags: u32) -> Pen {
    Pen {
        pressure: (pressure as f32 / 1024.0).clamp(0.0, 1.0),
        tilt_x: (tilt_x as f32 / 90.0).clamp(-1.0, 1.0),
        tilt_y: (tilt_y as f32 / 90.0).clamp(-1.0, 1.0),
        rotation: rotation as f32,
        eraser: flags & (2 | 4) != 0,
    }
}

/// One axis of an X11 device: its number among the device's axes, and its range.
#[cfg_attr(not(any(all(unix, not(target_os = "macos")), test)), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
struct XAxis {
    number: u16,
    min: f64,
    max: f64,
}

#[cfg_attr(not(any(all(unix, not(target_os = "macos")), test)), allow(dead_code))]
impl XAxis {
    /// A value of the axis as 0..=1 of its range.
    fn part(&self, value: f64) -> f32 {
        if self.max > self.min {
            ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }
}

/// An X11 tablet tool (a pen's tip, or its eraser end, each a device of its own): where its
/// values are among the device's axes, and what it last said.
#[cfg_attr(not(any(all(unix, not(target_os = "macos")), test)), allow(dead_code))]
#[derive(Clone, Debug, Default, PartialEq)]
struct XTool {
    name: String,
    eraser: bool,
    pressure: Option<XAxis>,
    tilt_x: Option<XAxis>,
    tilt_y: Option<XAxis>,
    rotation: Option<XAxis>,
    pen: Pen,
}

#[cfg_attr(not(any(all(unix, not(target_os = "macos")), test)), allow(dead_code))]
impl XTool {
    /// Take in the axis values of one event (only the axes that changed are sent).
    fn update(&mut self, values: &[(u16, f64)]) {
        let of = |axis: Option<XAxis>, n: u16| axis.filter(|a| a.number == n);
        for &(n, v) in values {
            if let Some(a) = of(self.pressure, n) {
                self.pen.pressure = a.part(v);
            }
            // tilt axes run from full left / away to full right / towards the person
            if let Some(a) = of(self.tilt_x, n) {
                self.pen.tilt_x = a.part(v) * 2.0 - 1.0;
            }
            if let Some(a) = of(self.tilt_y, n) {
                self.pen.tilt_y = a.part(v) * 2.0 - 1.0;
            }
            if let Some(a) = of(self.rotation, n) {
                self.pen.rotation = a.part(v) * 360.0;
            }
        }
        self.pen.eraser = self.eraser;
    }
}

/// The axis numbers and values of an X11 raw event: one value for each bit set in the mask.
#[cfg_attr(not(any(all(unix, not(target_os = "macos")), test)), allow(dead_code))]
fn x_valuators(mask: &[u32], values: &[f64]) -> Vec<(u16, f64)> {
    let mut out = vec![];
    let mut k = 0;
    for (word, bits) in mask.iter().enumerate() {
        for bit in 0..32 {
            if bits & (1 << bit) != 0 {
                if let Some(v) = values.get(k) {
                    out.push(((word * 32 + bit) as u16, *v));
                }
                k += 1;
            }
        }
    }
    out
}

/// What an X11 raw event means for the pen. `tools` are the tablet tools by device number and
/// `active` the tool that spoke last. A tool counts from the moment it moves near the tablet
/// (its pressure is then 0), so a stroke never starts with another device's values; any other
/// device (a mouse) moving or pressing takes over.
#[cfg_attr(not(any(all(unix, not(target_os = "macos")), test)), allow(dead_code))]
fn x_event(tools: &mut std::collections::HashMap<u16, XTool>, active: &mut Option<u16>, device: u16, values: &[(u16, f64)]) -> PenEvent {
    match tools.get_mut(&device) {
        Some(tool) => {
            tool.update(values);
            let first = *active != Some(device);
            *active = Some(device);
            if first {
                PenEvent::Down { tablet: Some(tool.pen) }
            } else {
                PenEvent::Move { tablet: Some(tool.pen) }
            }
        }
        None => {
            *active = None;
            PenEvent::Down { tablet: None }
        }
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{Pen, PenEvent};
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventSubtype, NSEventType, NSPointingDeviceType};
    use std::ptr::NonNull;

    /// The pen values of a tablet event (a tablet point, or a mouse event a pen sent).
    fn pen(e: &NSEvent) -> Pen {
        let tilt = e.tilt();
        Pen {
            pressure: e.pressure().clamp(0.0, 1.0),
            tilt_x: tilt.x as f32,
            // the system counts a tilt away from the person as positive: ours is the other way
            tilt_y: -(tilt.y as f32),
            rotation: e.rotation(),
            eraser: false,
        }
    }

    fn proximity(e: &NSEvent) -> PenEvent {
        PenEvent::Proximity {
            entering: e.isEnteringProximity(),
            eraser: e.pointingDeviceType() == NSPointingDeviceType::Eraser,
        }
    }

    /// Only the properties valid for each kind of event are read (the system refuses the others).
    pub(super) fn read(e: &NSEvent) -> Option<PenEvent> {
        let kind = e.r#type();
        if kind == NSEventType::TabletPoint {
            return Some(PenEvent::Move { tablet: Some(pen(e)) });
        }
        if kind == NSEventType::TabletProximity {
            return Some(proximity(e));
        }
        let mouse = [NSEventType::LeftMouseDown, NSEventType::LeftMouseDragged, NSEventType::LeftMouseUp];
        if !mouse.contains(&kind) {
            return None;
        }
        // a pen sends mouse events marked as coming from a tablet
        let sub = e.subtype();
        if sub == NSEventSubtype::TabletProximity {
            return Some(proximity(e));
        }
        let tablet = (sub == NSEventSubtype::TabletPoint).then(|| pen(e));
        Some(if kind == NSEventType::LeftMouseDown {
            PenEvent::Down { tablet }
        } else if kind == NSEventType::LeftMouseDragged {
            PenEvent::Move { tablet }
        } else {
            PenEvent::Up
        })
    }

    pub fn install() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let mask = NSEventMask::LeftMouseDown
                | NSEventMask::LeftMouseDragged
                | NSEventMask::LeftMouseUp
                | NSEventMask::TabletPoint
                | NSEventMask::TabletProximity;
            let block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
                // SAFETY: the system hands a valid event for the length of this call.
                let e = unsafe { event.as_ref() };
                if let Some(pe) = read(e) {
                    super::apply(pe);
                }
                // the event goes on to the window untouched
                event.as_ptr()
            });
            // SAFETY: the block returns the very event it was given, which is valid.
            let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
            super::log(|| format!("macOS: listening to the app's events: {}", monitor.is_some()));
            // listening lasts as long as the app
            std::mem::forget(monitor);
        });
    }
}

#[cfg(windows)]
mod win {
    use super::{pen_from_windows, PenEvent};
    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::Pointer::{GetPointerPenInfo, GetPointerType, POINTER_PEN_INFO};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageExtraInfo, SetWindowsHookExW, HC_ACTION, MSG, PM_REMOVE, PT_PEN, WH_GETMESSAGE, WM_LBUTTONDOWN,
        WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE,
    };

    /// What a message of the window's queue says about the pen, if anything.
    ///
    /// # Safety
    /// Only calls system functions that fill plain values.
    unsafe fn read(msg: &MSG) -> Option<PenEvent> {
        match msg.message {
            WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP => {
                // the pointer's number is the low half of the message's first value
                let id = (msg.wParam & 0xFFFF) as u32;
                let mut kind = 0;
                if GetPointerType(id, &mut kind) == 0 {
                    return None;
                }
                let tablet = if kind == PT_PEN {
                    let mut info: POINTER_PEN_INFO = std::mem::zeroed();
                    if GetPointerPenInfo(id, &mut info) == 0 {
                        return None;
                    }
                    Some(pen_from_windows(
                        info.pressure,
                        info.rotation,
                        info.tiltX,
                        info.tiltY,
                        info.penFlags,
                    ))
                } else {
                    None
                };
                Some(match msg.message {
                    WM_POINTERDOWN => PenEvent::Down { tablet },
                    WM_POINTERUP => PenEvent::Up,
                    _ => PenEvent::Move { tablet },
                })
            }
            WM_LBUTTONDOWN => {
                // a press of a real mouse; the presses the system makes up for a pen or a finger
                // carry its mark and are left out
                let made_up = (GetMessageExtraInfo() as usize & 0xFFFF_FF00) == 0xFF51_5700;
                (!made_up).then_some(PenEvent::Down { tablet: None })
            }
            _ => None,
        }
    }

    /// Called by the system for every message the window's thread takes from its queue.
    unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // only messages being taken out, so each is read once
        if code == HC_ACTION as i32 && wparam == PM_REMOVE as WPARAM && lparam != 0 {
            // SAFETY: for this hook the system passes a valid message.
            let msg = &*(lparam as *const MSG);
            if let Some(e) = read(msg) {
                super::apply(e);
            }
        }
        // the message goes on untouched
        CallNextHookEx(0, code, wparam, lparam)
    }

    pub fn install() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            // SAFETY: a hook on this thread's own message queue, kept as long as the app runs.
            let hooked = unsafe { SetWindowsHookExW(WH_GETMESSAGE, Some(hook), 0, GetCurrentThreadId()) };
            super::log(|| format!("Windows: listening to the window's messages: {}", hooked != 0));
        });
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod x11 {
    use super::{x_event, x_valuators, XAxis, XTool};
    use std::collections::HashMap;
    use x11rb::connection::Connection;
    use x11rb::protocol::xinput::{self, ConnectionExt as _, DeviceClassData, Fp3232, XIEventMask};
    use x11rb::protocol::xproto::{Atom, ConnectionExt as _};
    use x11rb::protocol::Event;
    use x11rb::rust_connection::RustConnection;

    type Failure = Box<dyn std::error::Error>;

    fn number(v: Fp3232) -> f64 {
        v.integral as f64 + v.frac as f64 / 4_294_967_296.0
    }

    /// The names the X server gives the axes that matter.
    struct Labels {
        pressure: Atom,
        tilt_x: Atom,
        tilt_y: Atom,
        rotation: Atom,
    }

    /// The tablet tools among the X server's devices: those with a pressure axis.
    fn tools(conn: &RustConnection, labels: &Labels) -> Result<HashMap<u16, XTool>, Failure> {
        let mut out = HashMap::new();
        for info in conn.xinput_xi_query_device(xinput::Device::ALL)?.reply()?.infos {
            let name = String::from_utf8_lossy(&info.name).to_string();
            let mut tool = XTool {
                eraser: name.to_lowercase().contains("eraser"),
                name,
                ..XTool::default()
            };
            for class in &info.classes {
                if let DeviceClassData::Valuator(v) = &class.data {
                    let axis = Some(XAxis {
                        number: v.number,
                        min: number(v.min),
                        max: number(v.max),
                    });
                    if v.label == labels.pressure {
                        tool.pressure = axis;
                    } else if v.label == labels.tilt_x {
                        tool.tilt_x = axis;
                    } else if v.label == labels.tilt_y {
                        tool.tilt_y = axis;
                    } else if v.label == labels.rotation {
                        tool.rotation = axis;
                    }
                }
            }
            if tool.pressure.is_some_and(|a| a.max > a.min) {
                out.insert(info.deviceid, tool);
            }
        }
        Ok(out)
    }

    /// Read the X server's raw device events for as long as the app runs.
    fn listen() -> Result<(), Failure> {
        let (conn, screen) = x11rb::connect(None)?;
        let root = conn.setup().roots.get(screen).ok_or("no screen")?.root;
        conn.xinput_xi_query_version(2, 2)?.reply()?;
        let atom = |name: &[u8]| -> Result<Atom, Failure> { Ok(conn.intern_atom(false, name)?.reply()?.atom) };
        let labels = Labels {
            pressure: atom(b"Abs Pressure")?,
            tilt_x: atom(b"Abs Tilt X")?,
            tilt_y: atom(b"Abs Tilt Y")?,
            rotation: atom(b"Abs Rotary Z")?,
        };
        let mut known = tools(&conn, &labels)?;
        let names = |known: &HashMap<u16, XTool>| known.values().map(|t| t.name.clone()).collect::<Vec<_>>();
        let mut active = None;
        // devices coming and going are told for "all devices"; what they do, through the
        // pointer they drive
        let masks = [
            xinput::EventMask {
                deviceid: xinput::Device::ALL.into(),
                mask: vec![XIEventMask::HIERARCHY],
            },
            xinput::EventMask {
                deviceid: xinput::Device::ALL_MASTER.into(),
                mask: vec![XIEventMask::RAW_MOTION | XIEventMask::RAW_BUTTON_PRESS],
            },
        ];
        conn.xinput_xi_select_events(root, &masks)?.check()?;
        super::log(|| format!("X11: listening; tablet tools: {:?}", names(&known)));
        loop {
            match conn.wait_for_event()? {
                Event::XinputHierarchy(_) => {
                    known = tools(&conn, &labels)?;
                    active = None;
                    super::log(|| format!("X11: devices changed; tablet tools: {:?}", names(&known)));
                }
                Event::XinputRawMotion(e) | Event::XinputRawButtonPress(e) => {
                    // the device that really moved, when the server says it
                    let device = if e.sourceid != 0 { e.sourceid } else { e.deviceid };
                    let values: Vec<f64> = e.axisvalues.iter().map(|v| number(*v)).collect();
                    super::apply(x_event(&mut known, &mut active, device, &x_valuators(&e.valuator_mask, &values)));
                }
                _ => {}
            }
        }
    }

    pub fn install() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            // without an X server (or its input extension) the pen simply gives no values
            let _ = std::thread::Builder::new().name("pen-x11".into()).spawn(|| {
                if let Err(e) = listen() {
                    super::log(|| format!("X11: not listening: {e}"));
                }
            });
        });
    }
}

/// On macOS, the reading itself is checked on events built the way a tablet driver builds them.
#[cfg(all(test, target_os = "macos"))]
mod mac_tests {
    use super::mac::read;
    use super::PenEvent;
    use objc2::encode::{Encoding, RefEncode};
    use objc2::rc::Retained;
    use objc2::{msg_send, ClassType};
    use objc2_app_kit::NSEvent;
    use std::ffi::c_void;

    #[repr(C)]
    struct CGEvent {
        _private: [u8; 0],
    }
    // SAFETY: the system's name for this opaque type.
    unsafe impl RefEncode for CGEvent {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("__CGEvent", &[]));
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateMouseEvent(source: *const c_void, kind: u32, at: CGPoint, button: u32) -> *mut CGEvent;
        fn CGEventSetIntegerValueField(event: *mut CGEvent, field: u32, value: i64);
        fn CGEventSetDoubleValueField(event: *mut CGEvent, field: u32, value: f64);
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    const LEFT_DOWN: u32 = 1;
    const LEFT_UP: u32 = 2;
    const LEFT_DRAGGED: u32 = 6;

    /// A mouse event, marked as coming from a pen when `pen` gives (pressure, tilt x, tilt y,
    /// rotation). None when this computer can't make events (no screen session).
    fn event(kind: u32, pen: Option<(f64, f64, f64, f64)>) -> Option<Retained<NSEvent>> {
        // SAFETY: plain system calls on an event this function owns until it is released.
        unsafe {
            let cg = CGEventCreateMouseEvent(std::ptr::null(), kind, CGPoint { x: 10.0, y: 10.0 }, 0);
            if cg.is_null() {
                return None;
            }
            if let Some((pressure, tilt_x, tilt_y, rotation)) = pen {
                CGEventSetIntegerValueField(cg, 7, 1); // mouse event subtype: tablet point
                CGEventSetDoubleValueField(cg, 2, pressure); // mouse event pressure
                CGEventSetDoubleValueField(cg, 19, pressure); // tablet point pressure
                CGEventSetDoubleValueField(cg, 20, tilt_x);
                CGEventSetDoubleValueField(cg, 21, tilt_y);
                CGEventSetDoubleValueField(cg, 22, rotation);
            }
            let ns: Option<Retained<NSEvent>> = msg_send![NSEvent::class(), eventWithCGEvent: cg];
            CFRelease(cg.cast());
            ns
        }
    }

    #[test]
    fn mouse_and_pen_events_are_read() {
        let Some(e) = event(LEFT_DOWN, None) else { return };
        assert!(matches!(read(&e), Some(PenEvent::Down { tablet: None })), "a mouse press");
        let e = event(LEFT_UP, None).unwrap();
        assert!(matches!(read(&e), Some(PenEvent::Up)));
        let e = event(LEFT_DOWN, Some((0.25, 0.0, 0.0, 0.0))).unwrap();
        match read(&e) {
            Some(PenEvent::Down { tablet: Some(p) }) => assert!((p.pressure - 0.25).abs() < 0.01, "{p:?}"),
            other => panic!("a pen press was read as {other:?}"),
        }
        let e = event(LEFT_DRAGGED, Some((0.6, 0.5, -0.25, 30.0))).unwrap();
        match read(&e) {
            Some(PenEvent::Move { tablet: Some(p) }) => {
                assert!((p.pressure - 0.6).abs() < 0.01, "{p:?}");
                // the event says 0.25 away from the person: for us that is -0.25... turned over
                assert!((p.tilt_x - 0.5).abs() < 0.01 && (p.tilt_y - 0.25).abs() < 0.01, "{p:?}");
                assert!((p.rotation - 30.0).abs() < 0.5, "{p:?}");
            }
            other => panic!("a pen move was read as {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pen(pressure: f32) -> Pen {
        Pen {
            pressure,
            ..Pen::default()
        }
    }

    #[test]
    fn a_pen_press_gives_its_values_and_a_mouse_press_none() {
        let mut s = State::default();
        assert_eq!(s.sample(), None, "nothing before any press");
        s.apply(PenEvent::Down { tablet: Some(pen(0.2)) });
        s.apply(PenEvent::Move { tablet: Some(pen(0.7)) });
        assert_eq!(s.sample().map(|p| p.pressure), Some(0.7));
        s.apply(PenEvent::Up);
        assert_eq!(s.sample().map(|p| p.pressure), Some(0.7), "the release keeps the last pressure");
        s.apply(PenEvent::Down { tablet: None });
        assert_eq!(s.sample(), None, "a mouse press has no pen values");
        s.apply(PenEvent::Move { tablet: Some(pen(0.9)) });
        assert_eq!(s.sample(), None, "pen moves don't count during a mouse press");
        // the eraser end
        s.apply(PenEvent::Proximity {
            entering: true,
            eraser: true,
        });
        s.apply(PenEvent::Down { tablet: Some(pen(0.5)) });
        assert!(s.sample().unwrap().eraser);
        s.apply(PenEvent::Proximity {
            entering: false,
            eraser: true,
        });
        s.apply(PenEvent::Down { tablet: Some(pen(0.5)) });
        assert!(!s.sample().unwrap().eraser);
    }

    #[test]
    fn a_windows_pen_report_is_read() {
        let p = pen_from_windows(512, 90, 45, -90, 0);
        assert_eq!(
            (p.pressure, p.tilt_x, p.tilt_y, p.rotation, p.eraser),
            (0.5, 0.5, -1.0, 90.0, false)
        );
        assert!((p.twist() - 0.25).abs() < 1e-6);
        assert!(pen_from_windows(0, 0, 0, 0, 2).eraser, "the pen turned over");
        assert!(pen_from_windows(100, 0, 0, 0, 4).eraser, "the eraser touching");
        assert!(!pen_from_windows(100, 0, 0, 0, 1).eraser, "the side button is not the eraser");
        assert_eq!(pen_from_windows(5000, 0, 200, 0, 0).pressure, 1.0);
        // through the state: the report carries the eraser itself
        let mut s = State::default();
        s.apply(PenEvent::Down {
            tablet: Some(pen_from_windows(300, 0, 0, 0, 4)),
        });
        assert!(s.sample().unwrap().eraser);
        s.apply(PenEvent::Down {
            tablet: Some(pen_from_windows(300, 0, 0, 0, 0)),
        });
        assert!(!s.sample().unwrap().eraser);
    }

    #[test]
    fn x11_tablet_events_are_read() {
        use std::collections::HashMap;
        // a raw event lists one value for each bit of its mask
        assert_eq!(x_valuators(&[0b100101], &[1.0, 2.0, 3.0]), vec![(0, 1.0), (2, 2.0), (5, 3.0)]);
        assert_eq!(x_valuators(&[0, 0b10], &[7.0]), vec![(33, 7.0)]);
        assert_eq!(
            x_valuators(&[0b11], &[1.0]),
            vec![(0, 1.0)],
            "a short event is not read past its end"
        );
        let axis = |number, min, max| Some(XAxis { number, min, max });
        let pen = XTool {
            pressure: axis(2, 0.0, 65535.0),
            tilt_x: axis(3, -64.0, 63.0),
            tilt_y: axis(4, -64.0, 63.0),
            ..XTool::default()
        };
        let eraser = XTool {
            eraser: true,
            pressure: axis(2, 0.0, 2047.0),
            ..XTool::default()
        };
        let mut tools = HashMap::from([(9, pen), (10, eraser)]);
        let mut active = None;
        let mut s = State::default();
        // the pen comes near (no pressure yet): it counts from then on
        s.apply(x_event(&mut tools, &mut active, 9, &[(0, 100.0), (1, 50.0)]));
        assert_eq!(s.sample().map(|p| p.pressure), Some(0.0));
        // it touches and leans to the right and towards the person; only changed axes are sent
        s.apply(x_event(&mut tools, &mut active, 9, &[(2, 32767.5), (3, 63.0)]));
        s.apply(x_event(&mut tools, &mut active, 9, &[(4, 31.25)]));
        let p = s.sample().unwrap();
        assert!(
            (p.pressure - 0.5).abs() < 1e-4 && p.tilt_x == 1.0 && (p.tilt_y - 0.5).abs() < 0.01,
            "{p:?}"
        );
        assert!(!p.eraser);
        // the mouse moves: the pen's values no longer apply
        s.apply(x_event(&mut tools, &mut active, 4, &[(0, 1.0)]));
        assert_eq!(s.sample(), None);
        // the pen turned over is another device: its eraser end
        s.apply(x_event(&mut tools, &mut active, 10, &[(2, 2047.0)]));
        let p = s.sample().unwrap();
        assert!(p.eraser && p.pressure == 1.0 && p.tilt() == 0.0, "{p:?}");
        // back to the tip: its last values are still known
        s.apply(x_event(&mut tools, &mut active, 9, &[]));
        assert!(!s.sample().unwrap().eraser);
        // an axis without a range gives nothing
        assert_eq!(
            XAxis {
                number: 0,
                min: 5.0,
                max: 5.0
            }
            .part(5.0),
            0.0
        );
    }

    #[test]
    fn tilt_azimuth_and_twist() {
        let p = Pen {
            tilt_x: 0.0,
            tilt_y: 0.5,
            rotation: 90.0,
            ..Pen::default()
        };
        assert!((p.tilt() - 0.5).abs() < 1e-6);
        assert!((p.azimuth() - 0.25).abs() < 1e-6, "leaning down the screen is a quarter turn");
        assert!((p.twist() - 0.25).abs() < 1e-6);
        assert_eq!(Pen::default().azimuth(), 0.0, "an upright pen points nowhere");
        let left = Pen {
            tilt_x: -1.0,
            ..Pen::default()
        };
        assert!((left.azimuth() - 0.5).abs() < 1e-6);
    }
}
