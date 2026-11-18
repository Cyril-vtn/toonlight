//! Driving the interface without a person, for checks: `--script file` plays a list of clicks,
//! keys and named actions on the real window, one step per screen refresh, and can save pictures
//! of the window along the way. The same script runs on every system (in CI too), which is how
//! "click everywhere and see that nothing breaks" is checked on Windows, macOS and Linux.
//!
//! One command per line (`#` starts a comment):
//! - `wait N`: let N refreshes pass; `sleep S`: let S seconds pass;
//! - `move X Y`, `click X Y`, `rclick X Y`, `dclick X Y`, `drag X0 Y0 X1 Y1`: the pointer, in
//!   points from the window's top-left corner;
//! - `key NAME [cmd] [shift] [alt]`: press and release a key (`Enter`, `Z`, `F5`…);
//! - `text SOME WORDS`: type them;
//! - `float PANEL`, `dock PANEL left|right`, `hide PANEL`, `show PANEL`: what a panel's buttons
//!   do, without needing to know where they are (panels: tools, colours, settings, presets,
//!   light, sound, project, shortcuts, correction, remote, groups);
//! - `room N`: go to room N (1-4);
//! - `tool NAME`: select a tool;
//! - `open WINDOW` / `close WINDOW`: history, shortcuts, storyboard, groups, preferences,
//!   xsheet, effects, ai;
//! - `shot FILE`: save a picture of the window;
//! - `crash`: stop on an internal error on purpose (to check that the report is written);
//! - `quit`: close the app.

use eframe::egui::{pos2, Event, Key, Modifiers, PointerButton, Pos2};
use std::collections::VecDeque;
use std::path::PathBuf;

/// Something a script asks the app itself to do (not a pointer or key event).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Float(String),
    Dock(String, bool),
    Hide(String),
    Show(String),
    Room(usize),
    Tool(String),
    Window(String, bool),
    Shot(PathBuf),
    Crash,
    Quit,
}

/// What happens during one screen refresh.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Idle,
    /// Nothing for this many seconds.
    Sleep(f32),
    Events(Vec<Event>),
    Do(Action),
}

/// A script being played.
#[derive(Debug, Default)]
pub struct Script {
    steps: VecDeque<Step>,
}

fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// A press and a release at one place, each in its own refresh as a hand would do.
fn click(pos: Pos2, b: PointerButton) -> Vec<Step> {
    vec![
        Step::Events(vec![Event::PointerMoved(pos)]),
        Step::Events(vec![button(pos, b, true)]),
        Step::Events(vec![button(pos, b, false)]),
    ]
}

/// The steps of one line of a script, or what is wrong with it.
pub fn parse_line(line: &str) -> Result<Vec<Step>, String> {
    let line = line.split('#').next().unwrap_or("").trim();
    let mut words = line.split_whitespace();
    let Some(command) = words.next() else { return Ok(vec![]) };
    let rest: Vec<&str> = words.collect();
    let number = |i: usize| -> Result<f32, String> {
        rest.get(i)
            .and_then(|w| w.parse::<f32>().ok())
            .ok_or_else(|| format!("`{line}`: a number is missing"))
    };
    let word = |i: usize| -> Result<String, String> {
        rest.get(i)
            .map(|w| w.to_lowercase())
            .ok_or_else(|| format!("`{line}`: a name is missing"))
    };
    let at = |i: usize| -> Result<Pos2, String> { Ok(pos2(number(i)?, number(i + 1)?)) };
    Ok(match command {
        "wait" => vec![Step::Idle; number(0)?.max(0.0) as usize],
        "sleep" => vec![Step::Sleep(number(0)?.clamp(0.0, 600.0))],
        "move" => vec![Step::Events(vec![Event::PointerMoved(at(0)?)])],
        "click" => click(at(0)?, PointerButton::Primary),
        "rclick" => click(at(0)?, PointerButton::Secondary),
        "dclick" => [click(at(0)?, PointerButton::Primary), click(at(0)?, PointerButton::Primary)].concat(),
        "drag" => {
            let (a, b) = (at(0)?, at(2)?);
            let mut steps = vec![
                Step::Events(vec![Event::PointerMoved(a)]),
                Step::Events(vec![button(a, PointerButton::Primary, true)]),
            ];
            for k in 1..=8 {
                steps.push(Step::Events(vec![Event::PointerMoved(a + (b - a) * (k as f32 / 8.0))]));
            }
            steps.push(Step::Events(vec![button(b, PointerButton::Primary, false)]));
            steps
        }
        "key" => {
            let name = rest.first().ok_or_else(|| format!("`{line}`: a key is missing"))?;
            let key = Key::from_name(name).ok_or_else(|| format!("`{line}`: no key named {name}"))?;
            let has = |m: &str| rest.iter().skip(1).any(|w| w.eq_ignore_ascii_case(m));
            let modifiers = Modifiers {
                alt: has("alt"),
                shift: has("shift"),
                command: has("cmd"),
                ctrl: has("cmd") && !cfg!(target_os = "macos"),
                mac_cmd: has("cmd") && cfg!(target_os = "macos"),
            };
            let event = |pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            };
            vec![Step::Events(vec![event(true)]), Step::Events(vec![event(false)])]
        }
        "text" => vec![Step::Events(vec![Event::Text(rest.join(" "))])],
        "float" => vec![Step::Do(Action::Float(word(0)?))],
        "dock" => vec![Step::Do(Action::Dock(word(0)?, word(1)? == "left"))],
        "hide" => vec![Step::Do(Action::Hide(word(0)?))],
        "show" => vec![Step::Do(Action::Show(word(0)?))],
        "room" => vec![Step::Do(Action::Room(number(0)?.max(1.0) as usize))],
        "tool" => vec![Step::Do(Action::Tool(word(0)?))],
        "open" => vec![Step::Do(Action::Window(word(0)?, true))],
        "close" => vec![Step::Do(Action::Window(word(0)?, false))],
        "shot" => {
            let file = rest.join(" ");
            if file.is_empty() {
                return Err(format!("`{line}`: a file name is missing"));
            }
            vec![Step::Do(Action::Shot(file.into()))]
        }
        "crash" => vec![Step::Do(Action::Crash)],
        "quit" => vec![Step::Do(Action::Quit)],
        other => return Err(format!("`{line}`: no command named {other}")),
    })
}

impl Script {
    /// Read a script; a line that can't be read stops everything, with its number.
    pub fn parse(text: &str) -> Result<Script, String> {
        let mut steps = VecDeque::new();
        for (n, line) in text.lines().enumerate() {
            steps.extend(parse_line(line).map_err(|e| format!("line {}: {e}", n + 1))?);
        }
        Ok(Script { steps })
    }

    pub fn is_done(&self) -> bool {
        self.steps.is_empty()
    }

    /// The next refresh's step.
    pub fn next(&mut self) -> Option<Step> {
        self.steps.pop_front()
    }
}

/// While a script plays, the person's own pointer must not get in its way: what the real mouse
/// and keyboard send is dropped.
pub fn drop_real_input(events: &mut Vec<Event>) {
    events.retain(|e| {
        !matches!(
            e,
            Event::PointerMoved(_)
                | Event::PointerButton { .. }
                | Event::PointerGone
                | Event::MouseMoved(_)
                | Event::MouseWheel { .. }
                | Event::Key { .. }
                | Event::Text(_)
                | Event::Touch { .. }
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_are_read() {
        let s = Script::parse("# a comment\n\nwait 3\nclick 10 20  # the arrow\nfloat Colours\nquit\n").unwrap();
        let steps: Vec<Step> = s.steps.into_iter().collect();
        assert_eq!(steps.len(), 3 + 3 + 1 + 1);
        assert_eq!(steps[0], Step::Idle);
        assert_eq!(steps[3], Step::Events(vec![Event::PointerMoved(pos2(10.0, 20.0))]));
        assert!(matches!(&steps[4], Step::Events(e) if matches!(e[0], Event::PointerButton { pressed: true, .. })));
        assert!(matches!(&steps[5], Step::Events(e) if matches!(e[0], Event::PointerButton { pressed: false, .. })));
        assert_eq!(steps[6], Step::Do(Action::Float("colours".into())));
        assert_eq!(steps[7], Step::Do(Action::Quit));
        // a drag: press, eight moves, release
        assert_eq!(parse_line("drag 0 0 80 0").unwrap().len(), 2 + 8 + 1);
        assert_eq!(parse_line("sleep 1.5").unwrap(), vec![Step::Sleep(1.5)]);
        assert_eq!(
            parse_line("dock sound LEFT").unwrap(),
            vec![Step::Do(Action::Dock("sound".into(), true))]
        );
        assert_eq!(
            parse_line("shot out/a b.png").unwrap(),
            vec![Step::Do(Action::Shot("out/a b.png".into()))]
        );
        assert!(
            matches!(&parse_line("key Z cmd").unwrap()[0], Step::Events(e) if matches!(e[0], Event::Key { key: Key::Z, pressed: true, modifiers, .. } if modifiers.command))
        );
        // mistakes are told with the line
        assert!(Script::parse("wait 1\nclick 10\n").unwrap_err().starts_with("line 2:"));
        assert!(parse_line("jump 1").is_err() && parse_line("key Nope").is_err() && parse_line("shot").is_err());
        // the person's own mouse is kept out while a script plays
        let mut events = vec![Event::PointerMoved(pos2(1.0, 1.0)), Event::WindowFocused(true), Event::PointerGone];
        drop_real_input(&mut events);
        assert_eq!(events, vec![Event::WindowFocused(true)]);
    }
}
