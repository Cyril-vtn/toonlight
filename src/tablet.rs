//! Pen tablet data on macOS.
//!
//! There, the window library passes on where the pointer is, but not the pen's pressure, tilt or
//! barrel rotation. This module listens to the system's own events as they reach the app and
//! keeps what the pen last said, for the canvas to read. On other systems the pressure comes with
//! the touch events and this module has nothing to say.

use std::sync::Mutex;

/// What the pen is doing: pressure 0..=1, tilt -1..=1 on each axis (0 = upright; x to the right,
/// y towards the bottom of the screen), barrel rotation in degrees, and whether its eraser end is
/// the one in use.
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
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
enum PenEvent {
    Down { tablet: Option<Pen> },
    Move { tablet: Option<Pen> },
    Up,
    Proximity { entering: bool, eraser: bool },
}

#[derive(Default)]
struct State {
    /// The press going on (or the last one) was made with a pen.
    pen_press: bool,
    pen: Pen,
    eraser: bool,
}

impl State {
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
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
            eraser: self.eraser,
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

/// The pen's values for the press going on, when it is made with a pen the system reports
/// (macOS). None with a mouse, and on systems where this module doesn't listen.
pub fn sample() -> Option<Pen> {
    STATE.lock().ok().and_then(|s| s.sample())
}

/// Start listening to the system's pen events. Call once, from the main thread, when the window
/// exists. Does nothing on systems other than macOS.
pub fn install() {
    #[cfg(target_os = "macos")]
    mac::install();
}

#[cfg(target_os = "macos")]
mod mac {
    use super::{Pen, PenEvent, STATE};
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventSubtype, NSEventType, NSPointingDeviceType};
    use std::ptr::NonNull;

    /// The pen values of a tablet event (a tablet point, or a mouse event a pen sent).
    fn pen(e: &NSEvent) -> Pen {
        let tilt = e.tilt();
        Pen {
            pressure: e.pressure().clamp(0.0, 1.0),
            tilt_x: tilt.x as f32,
            tilt_y: tilt.y as f32,
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
                    if let Ok(mut s) = STATE.lock() {
                        s.apply(pe);
                    }
                }
                // the event goes on to the window untouched
                event.as_ptr()
            });
            // SAFETY: the block returns the very event it was given, which is valid.
            let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
            // listening lasts as long as the app
            std::mem::forget(monitor);
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
                assert!((p.tilt_x - 0.5).abs() < 0.01 && (p.tilt_y + 0.25).abs() < 0.01, "{p:?}");
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
