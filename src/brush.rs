//! Drawing tools: stamped dabs building a coverage mask over the drawing as it was before the stroke.

use crate::doc::Rect;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Tool {
    Pencil,
    Pen,
    Airbrush,
    Oil,
    Wet,
    Eraser,
    Fill,
    Line,
    Rectangle,
    Ellipse,
    Text,
    Select,
    Gradient,
    Custom,
    Warp,
    Camera,
    Position,
    Picker,
}

pub const ALL_TOOLS: [Tool; 18] = [
    Tool::Pencil,
    Tool::Pen,
    Tool::Airbrush,
    Tool::Oil,
    Tool::Wet,
    Tool::Eraser,
    Tool::Fill,
    Tool::Line,
    Tool::Rectangle,
    Tool::Ellipse,
    Tool::Text,
    Tool::Select,
    Tool::Gradient,
    Tool::Custom,
    Tool::Warp,
    Tool::Camera,
    Tool::Position,
    Tool::Picker,
];

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    pub radius: f32,
    /// 0..=100
    pub power: f32,
    /// 0..=100
    pub opacity: f32,
    /// 0..=100, 100 = round dab ("aspect", Data/defaulttools.conf)
    pub aspect: f32,
    /// degrees ("angle")
    pub angle: f32,
    /// Pen pressure changes the size ("consize"; off by default, Data/defaulttools.conf consize=0).
    pub size_pressure: bool,
    /// Pen pressure changes the power ("conpower"; off by default, conpower=0).
    pub power_pressure: bool,
    /// How the paint mixes with what is already drawn ("mode=", 0 = color).
    #[serde(default)]
    pub mode: DrawMode,
    /// What drives the size, power, aspect and angle while drawing ("ConSize2", "ConPower2",
    /// "ConAspect2", "ConAngle2": a source, inverted or not, and a curve).
    #[serde(default)]
    pub con_size: Connection,
    #[serde(default)]
    pub con_power: Connection,
    #[serde(default)]
    pub con_aspect: Connection,
    #[serde(default)]
    pub con_angle: Connection,
}

/// What a brush setting follows while drawing, numbered like the reference's connection sources
/// (random 5, pressure 6, speed 7, direction 8, azimuth 9, tilt 10, twist 11; 1000
/// is "invert"). Tilt, azimuth and twist need a pen that reports them (see `tablet`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum ConSource {
    #[default]
    Off,
    Pressure,
    Speed,
    Random,
    Direction,
    /// How far the pen leans (0 upright, 1 flat).
    Tilt,
    /// Which way the pen leans.
    Azimuth,
    /// The pen's rotation around its own axis.
    Twist,
}

pub const ALL_SOURCES: [ConSource; 8] = [
    ConSource::Off,
    ConSource::Pressure,
    ConSource::Speed,
    ConSource::Random,
    ConSource::Direction,
    ConSource::Tilt,
    ConSource::Azimuth,
    ConSource::Twist,
];

/// What a connection can follow at one point of a stroke, each 0..=1 (direction, azimuth and
/// twist as parts of a turn).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Inputs {
    pub pressure: f32,
    pub speed: f32,
    pub random: f32,
    pub direction: f32,
    pub tilt: f32,
    pub azimuth: f32,
    pub twist: f32,
}

impl ConSource {
    #[allow(dead_code)] // the number other programs use for it, kept for file exchange
    pub fn id(self) -> u32 {
        match self {
            ConSource::Off => 0,
            ConSource::Random => 5,
            ConSource::Pressure => 6,
            ConSource::Speed => 7,
            ConSource::Direction => 8,
            ConSource::Azimuth => 9,
            ConSource::Tilt => 10,
            ConSource::Twist => 11,
        }
    }

    /// A source that is an angle: connected to the dab's angle, it turns the dab by that much.
    pub fn is_turn(self) -> bool {
        matches!(self, ConSource::Direction | ConSource::Azimuth | ConSource::Twist)
    }
    pub fn label(self) -> &'static str {
        use crate::i18n::tr;
        match self {
            ConSource::Off => tr("Rien"),
            ConSource::Pressure => tr("Pression"),
            ConSource::Speed => tr("Vitesse"),
            ConSource::Random => tr("Hasard"),
            ConSource::Direction => tr("Direction"),
            ConSource::Tilt => tr("Inclinaison du stylet"),
            ConSource::Azimuth => tr("Orientation du stylet"),
            ConSource::Twist => tr("Rotation du stylet"),
        }
    }
}

/// One connection: a source, inverted or not, through a curve (-1 soft … 0 straight … 1 hard).
#[derive(Clone, Copy, PartialEq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Connection {
    pub source: ConSource,
    pub invert: bool,
    pub curve: f32,
}

impl Connection {
    pub fn pressure() -> Connection {
        Connection {
            source: ConSource::Pressure,
            invert: false,
            curve: 0.0,
        }
    }

    /// The factor 0..=1 for these inputs (all 0..=1), or None when off.
    pub fn value(&self, i: &Inputs) -> Option<f32> {
        let x = match self.source {
            ConSource::Off => return None,
            ConSource::Pressure => i.pressure,
            ConSource::Speed => i.speed,
            ConSource::Random => i.random,
            ConSource::Direction => i.direction,
            ConSource::Tilt => i.tilt,
            ConSource::Azimuth => i.azimuth,
            ConSource::Twist => i.twist,
        }
        .clamp(0.0, 1.0);
        // the curve as an exponent: soft lifts light touches, hard holds them back
        let v = x.powf(4f32.powf(self.curve.clamp(-1.0, 1.0)));
        Some(if self.invert { 1.0 - v } else { v })
    }
}

/// Brush draw modes, numbered like the reference's mode table (color 0, merge 1,
/// behind 2, light 3, shade 4, noise 5, erase 6, tint 7, colorize 8, blur 9, negative 11, sharp 12,
/// grain 13, emboss 14, solarize 17, panto 18, saturate 19, unsaturate 20, multiply 24, screen 25,
/// healing 36, diff 38, dodge 41, burn 42, lighten 43, darken 44, bpentoapen 46). The ones below are
/// built; the formulas are ours.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub enum DrawMode {
    #[default]
    Color,
    Behind,
    Light,
    Shade,
    Erase,
    Tint,
    Colorize,
    Negative,
    Saturate,
    Unsaturate,
    Multiply,
    Screen,
    Diff,
    Dodge,
    Burn,
    Lighten,
    Darken,
}

pub const ALL_DRAW_MODES: [DrawMode; 17] = [
    DrawMode::Color,
    DrawMode::Behind,
    DrawMode::Light,
    DrawMode::Shade,
    DrawMode::Erase,
    DrawMode::Tint,
    DrawMode::Colorize,
    DrawMode::Negative,
    DrawMode::Saturate,
    DrawMode::Unsaturate,
    DrawMode::Multiply,
    DrawMode::Screen,
    DrawMode::Diff,
    DrawMode::Dodge,
    DrawMode::Burn,
    DrawMode::Lighten,
    DrawMode::Darken,
];

impl DrawMode {
    /// The reference's number for this mode.
    #[allow(dead_code)] // the number other programs use for it, kept for file exchange
    pub fn id(self) -> u32 {
        match self {
            DrawMode::Color => 0,
            DrawMode::Behind => 2,
            DrawMode::Light => 3,
            DrawMode::Shade => 4,
            DrawMode::Erase => 6,
            DrawMode::Tint => 7,
            DrawMode::Colorize => 8,
            DrawMode::Negative => 11,
            DrawMode::Saturate => 19,
            DrawMode::Unsaturate => 20,
            DrawMode::Multiply => 24,
            DrawMode::Screen => 25,
            DrawMode::Diff => 38,
            DrawMode::Dodge => 41,
            DrawMode::Burn => 42,
            DrawMode::Lighten => 43,
            DrawMode::Darken => 44,
        }
    }

    /// The reference's name ("color", "behind"…).
    pub fn name(self) -> &'static str {
        match self {
            DrawMode::Color => "color",
            DrawMode::Behind => "behind",
            DrawMode::Light => "light",
            DrawMode::Shade => "shade",
            DrawMode::Erase => "erase",
            DrawMode::Tint => "tint",
            DrawMode::Colorize => "colorize",
            DrawMode::Negative => "negative",
            DrawMode::Saturate => "saturate",
            DrawMode::Unsaturate => "unsaturate",
            DrawMode::Multiply => "multiply",
            DrawMode::Screen => "screen",
            DrawMode::Diff => "diff",
            DrawMode::Dodge => "dodge",
            DrawMode::Burn => "burn",
            DrawMode::Lighten => "lighten",
            DrawMode::Darken => "darken",
        }
    }

    pub fn from_name(n: &str) -> Option<DrawMode> {
        ALL_DRAW_MODES.iter().copied().find(|m| m.name().eq_ignore_ascii_case(n))
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::tr;
        match self {
            DrawMode::Color => tr("Couleur"),
            DrawMode::Behind => tr("Derrière"),
            DrawMode::Light => tr("Éclaircir"),
            DrawMode::Shade => tr("Ombrer"),
            DrawMode::Erase => tr("Gommer"),
            DrawMode::Tint => tr("Teinter"),
            DrawMode::Colorize => tr("Coloriser"),
            DrawMode::Negative => tr("Négatif"),
            DrawMode::Saturate => tr("Saturer"),
            DrawMode::Unsaturate => tr("Désaturer"),
            DrawMode::Multiply => tr("Produit"),
            DrawMode::Screen => tr("Superposition claire"),
            DrawMode::Diff => tr("Différence"),
            DrawMode::Dodge => tr("Densité −"),
            DrawMode::Burn => tr("Densité +"),
            DrawMode::Lighten => tr("Éclaircir (max)"),
            DrawMode::Darken => tr("Assombrir"),
        }
    }

    /// New colour of an existing pixel `b` painted with `c`, for the modes that change what is
    /// already drawn (they keep its transparency).
    fn recolor(self, c: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        let lum = |p: [f32; 3]| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
        match self {
            DrawMode::Light => b.map(|v| v + (255.0 - v) * 0.5),
            DrawMode::Shade => b.map(|v| v * 0.5),
            DrawMode::Tint => c,
            DrawMode::Colorize => {
                // keep the pixel's lightness, take the brush colour's hue and saturation
                let (lb, lc) = (lum(b), lum(c).max(1.0));
                c.map(|v| (v * lb / lc).clamp(0.0, 255.0))
            }
            DrawMode::Negative => b.map(|v| 255.0 - v),
            DrawMode::Saturate => {
                let l = lum(b);
                b.map(|v| (l + (v - l) * 1.5).clamp(0.0, 255.0))
            }
            DrawMode::Unsaturate => {
                let l = lum(b);
                [l, l, l]
            }
            DrawMode::Multiply => [c[0] * b[0] / 255.0, c[1] * b[1] / 255.0, c[2] * b[2] / 255.0],
            DrawMode::Screen => [0, 1, 2].map(|k| 255.0 - (255.0 - c[k]) * (255.0 - b[k]) / 255.0),
            DrawMode::Diff => [0, 1, 2].map(|k| (c[k] - b[k]).abs()),
            DrawMode::Dodge => [0, 1, 2].map(|k| (b[k] * 255.0 / (255.0 - c[k]).max(1.0)).min(255.0)),
            DrawMode::Burn => [0, 1, 2].map(|k| (255.0 - (255.0 - b[k]) * 255.0 / c[k].max(1.0)).max(0.0)),
            DrawMode::Lighten => [0, 1, 2].map(|k| c[k].max(b[k])),
            DrawMode::Darken => [0, 1, 2].map(|k| c[k].min(b[k])),
            DrawMode::Color | DrawMode::Behind | DrawMode::Erase => c,
        }
    }
}

impl Settings {
    pub fn new(radius: f32, power: f32, opacity: f32) -> Settings {
        Settings {
            radius,
            power,
            opacity,
            aspect: 100.0,
            angle: 0.0,
            size_pressure: false,
            power_pressure: false,
            mode: DrawMode::Color,
            con_size: Connection::default(),
            con_power: Connection::default(),
            con_aspect: Connection::default(),
            con_angle: Connection::default(),
        }
    }
}

/// Dab spacing in percent of the radius ("StepVal" = 12.50, Data/defaulttools.conf).
pub const STEP_VAL: f32 = 12.5;
/// Factor applied to StepVal × Radius, and to Power and Aspect (0.01).
pub const PERCENT: f32 = 0.01;
/// Smallest dab radius (1.2).
pub const MIN_RADIUS: f32 = 1.2;

/// Distance between two dabs, in whole pixels: trunc(StepVal × Radius × 0.01 × StepOn).
/// Tools without spacing (StepOn = 0: pen, eraser, shapes) and spacings that round to 0 stamp every pixel.
pub fn dab_spacing(tool: Tool, radius: f32) -> f32 {
    let step_on = matches!(tool, Tool::Pencil | Tool::Airbrush | Tool::Oil | Tool::Wet | Tool::Warp);
    let s = if step_on { (STEP_VAL * radius * PERCENT).trunc() } else { 0.0 };
    s.max(1.0)
}

/// Dab strength: (Power × 0.01)².
pub fn dab_strength(power: f32) -> f32 {
    let p = (power * PERCENT).clamp(0.0, 1.0);
    p * p
}

impl Tool {
    pub fn index(self) -> usize {
        ALL_TOOLS.iter().position(|t| *t == self).unwrap()
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Pencil => crate::i18n::tr("Crayon"),
            Tool::Pen => crate::i18n::tr("Stylo"),
            Tool::Airbrush => crate::i18n::tr("Aérographe"),
            Tool::Oil => crate::i18n::tr("Pinceau"),
            Tool::Wet => crate::i18n::tr("Pinceau humide"),
            Tool::Eraser => crate::i18n::tr("Gomme"),
            Tool::Fill => crate::i18n::tr("Remplissage"),
            Tool::Line => crate::i18n::tr("Ligne"),
            Tool::Rectangle => crate::i18n::tr("Rectangle"),
            Tool::Ellipse => crate::i18n::tr("Ellipse"),
            Tool::Text => crate::i18n::tr("Texte"),
            Tool::Select => crate::i18n::tr("Sélection"),
            Tool::Gradient => crate::i18n::tr("Dégradé"),
            Tool::Custom => crate::i18n::tr("Pinceau personnalisé"),
            Tool::Warp => crate::i18n::tr("Déformation"),
            Tool::Camera => crate::i18n::tr("Caméra"),
            Tool::Position => crate::i18n::tr("Position"),
            Tool::Picker => crate::i18n::tr("Pipette"),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Tool::Pencil => "N",
            Tool::Pen => "B",
            Tool::Airbrush => "A",
            Tool::Oil => "H",
            Tool::Wet => "W",
            Tool::Eraser => "E",
            Tool::Fill => "G",
            Tool::Line => "L",
            Tool::Rectangle => "R",
            Tool::Ellipse => "O",
            Tool::Text => "K",
            Tool::Select => "S",
            Tool::Gradient => "D",
            Tool::Custom => "C",
            Tool::Warp => "Q",
            Tool::Camera => "M",
            Tool::Position => "P",
            Tool::Picker => "I",
        }
    }

    /// Defaults from Data/defaulttools.conf; shapes and fill estimated.
    pub fn defaults(self) -> Settings {
        let (radius, power, opacity) = match self {
            Tool::Airbrush => (10.0, 30.0, 100.0),
            Tool::Oil => (30.0, 30.0, 100.0),
            Tool::Pen => (20.0, 100.0, 100.0),
            Tool::Wet => (20.0, 50.0, 100.0),
            Tool::Pencil => (30.0, 30.0, 100.0),
            Tool::Warp => (30.0, 5.0, 100.0), // tooltype 10-1: radius 30, power 5
            Tool::Eraser => (20.0, 100.0, 100.0),
            Tool::Fill => (0.0, 15.0, 100.0), // power = colour tolerance (estimated)
            Tool::Line | Tool::Rectangle | Tool::Ellipse => (4.0, 100.0, 100.0), // estimated
            Tool::Picker | Tool::Text | Tool::Select | Tool::Gradient | Tool::Custom | Tool::Camera | Tool::Position => (0.0, 100.0, 100.0),
        };
        Settings::new(radius, power, opacity)
    }

    pub fn presets(self) -> &'static [f32] {
        match self {
            Tool::Airbrush => &[10.0, 20.0, 30.0, 40.0, 50.0, 75.0, 100.0, 150.0],
            Tool::Pen | Tool::Wet => &[20.0, 40.0, 60.0, 80.0],
            Tool::Pencil | Tool::Oil => &[5.0, 10.0, 20.0, 30.0],
            Tool::Eraser => &[10.0, 20.0, 40.0, 80.0],
            Tool::Line | Tool::Rectangle | Tool::Ellipse => &[2.0, 4.0, 8.0, 16.0],
            _ => &[],
        }
    }

    pub fn paints(self) -> bool {
        matches!(
            self,
            Tool::Pencil | Tool::Pen | Tool::Airbrush | Tool::Oil | Tool::Wet | Tool::Eraser | Tool::Warp
        )
    }

    pub fn is_shape(self) -> bool {
        matches!(self, Tool::Line | Tool::Rectangle | Tool::Ellipse)
    }

    pub fn has_size(self) -> bool {
        self.paints() || self.is_shape()
    }
}

pub struct Stroke {
    pub id: usize,
    pub before: Vec<u8>,
    mask: Vec<f32>,
    pub dirty: Rect,
    last: Option<(f32, f32)>,
    last_p: f32,
    /// Previous dab centre (the warp tool pushes pixels along the movement).
    last_dab: Option<(f32, f32)>,
    /// Power as given (0..=100), for the warp strength.
    raw_power: f32,
    carry: f32,
    base_radius: f32,
    /// Only pixels of the selection are painted (255 = selected).
    clip: Option<std::sync::Arc<Vec<u8>>>,
    base_power: f32,
    /// size, power, aspect, angle
    con: [Connection; 4],
    base_inv_aspect: f32,
    base_angle: f32,
    /// Movement direction (0..=1 of a turn) and speed (0..=1) of the last segment.
    dir: f32,
    speed: f32,
    /// The pen's tilt, azimuth and twist (each 0..=1), when a tablet reports them.
    pen: (f32, f32, f32),
    dabs: u32,
    mode: DrawMode,
    /// Only change pixels that are already drawn, keeping their transparency.
    preserve: bool,
    tool: Tool,
    color: [u8; 3],
    radius: f32,
    power: f32,
    opacity: f32,
    step: f32,
    /// cos, sin of the angle and 1 / aspect
    rot: (f32, f32),
    inv_aspect: f32,
    w: usize,
    h: usize,
}

fn hash(x: i32, y: i32) -> f32 {
    let mut n = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
}

impl Stroke {
    pub fn begin(id: usize, before: &[u8], w: usize, h: usize, tool: Tool, color: [u8; 3], s: Settings) -> Stroke {
        Stroke {
            id,
            before: before.to_vec(),
            mask: vec![0.0; w * h],
            dirty: Rect::empty(),
            last: None,
            last_p: 1.0,
            last_dab: None,
            raw_power: s.power,
            carry: 0.0,
            base_radius: s.radius.max(MIN_RADIUS),
            clip: None,
            base_power: dab_strength(s.power),
            con: [
                if s.con_size.source != ConSource::Off {
                    s.con_size
                } else if s.size_pressure {
                    Connection::pressure()
                } else {
                    Connection::default()
                },
                if s.con_power.source != ConSource::Off {
                    s.con_power
                } else if s.power_pressure {
                    Connection::pressure()
                } else {
                    Connection::default()
                },
                s.con_aspect,
                s.con_angle,
            ],
            base_inv_aspect: 1.0 / (s.aspect * PERCENT).clamp(0.05, 1.0),
            base_angle: s.angle.to_radians(),
            dir: 0.0,
            speed: 0.0,
            pen: (0.0, 0.0, 0.0),
            dabs: 0,
            mode: s.mode,
            preserve: false,
            tool,
            color,
            radius: s.radius.max(MIN_RADIUS),
            power: dab_strength(s.power),
            opacity: (s.opacity * PERCENT).clamp(0.0, 1.0),
            step: dab_spacing(tool, s.radius.max(MIN_RADIUS)),
            rot: (s.angle.to_radians().cos(), s.angle.to_radians().sin()),
            inv_aspect: 1.0 / (s.aspect * PERCENT).clamp(0.05, 1.0),
            w,
            h,
        }
    }

    /// Add the stroke up to point (x, y), writing into `px`. Returns the changed area.
    pub fn to(&mut self, x: f32, y: f32, px: &mut [u8]) -> Rect {
        self.to_p(x, y, 1.0, px)
    }

    /// The pen's tilt (0 upright ..= 1 flat), azimuth and twist (0..=1 of a turn) for the points
    /// that follow, when a tablet reports them.
    pub fn set_pen(&mut self, tilt: f32, azimuth: f32, twist: f32) {
        self.pen = (tilt, azimuth, twist);
    }

    fn apply_pressure(&mut self, p: f32) {
        self.dabs = self.dabs.wrapping_add(1);
        let i = Inputs {
            pressure: p.clamp(0.0, 1.0),
            speed: self.speed,
            random: hash(self.dabs as i32, 7919),
            direction: self.dir,
            tilt: self.pen.0,
            azimuth: self.pen.1,
            twist: self.pen.2,
        };
        let [cs, cp, ca, cg] = self.con;
        self.radius = match cs.value(&i) {
            Some(v) => (self.base_radius * v).max(MIN_RADIUS),
            None => self.base_radius,
        };
        self.power = match cp.value(&i) {
            Some(v) => self.base_power * v,
            None => self.base_power,
        };
        // a lighter touch makes a flatter dab
        self.inv_aspect = match ca.value(&i) {
            Some(v) => (1.0 / self.base_inv_aspect * v).clamp(0.05, 1.0).recip(),
            None => self.base_inv_aspect,
        };
        let angle = match (cg.source, cg.value(&i)) {
            // the dab turns with the stroke, or with the pen
            (src, Some(v)) if src.is_turn() => self.base_angle + v * std::f32::consts::TAU,
            (_, Some(v)) => self.base_angle * v,
            _ => self.base_angle,
        };
        self.rot = (angle.cos(), angle.sin());
    }

    /// Like `to`, with the pen pressure (0..=1) at this point; pressure is interpolated between points.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_p(&mut self, x: f32, y: f32, pressure: f32, px: &mut [u8]) -> Rect {
        let step = self.step;
        let mut changed = Rect::empty();
        let p0 = self.last_p;
        self.last_p = pressure;
        match self.last {
            None => {
                self.apply_pressure(pressure);
                changed = self.dab(x, y, px);
            }
            Some((lx, ly)) => {
                let dx = x - lx;
                let dy = y - ly;
                let len = (dx * dx + dy * dy).sqrt();
                if len > 0.0 {
                    self.dir = (dy.atan2(dx) / std::f32::consts::TAU).rem_euclid(1.0);
                    // pixels per pointer event; 40 and more counts as full speed (ours)
                    self.speed = (len / 40.0).min(1.0);
                }
                let mut t = step - self.carry;
                if len > 0.0 {
                    while t <= len {
                        self.apply_pressure(p0 + (pressure - p0) * (t / len));
                        let r = self.dab(lx + dx * t / len, ly + dy * t / len, px);
                        changed = changed.union(&r);
                        t += step;
                    }
                    self.carry = len - (t - step);
                }
            }
        }
        self.last = Some((x, y));
        self.dirty = self.dirty.union(&changed);
        changed
    }

    fn dab(&mut self, cx: f32, cy: f32, px: &mut [u8]) -> Rect {
        if self.tool == Tool::Warp {
            let r = self.warp_dab(cx, cy, px);
            self.last_dab = Some((cx, cy));
            return r;
        }
        let r = self.radius;
        let rr = r * self.inv_aspect.max(1.0);
        let x0 = ((cx - rr - 1.0).floor().max(0.0)) as usize;
        let y0 = ((cy - rr - 1.0).floor().max(0.0)) as usize;
        let x1 = ((cx + rr + 2.0).ceil().max(0.0) as usize).min(self.w);
        let y1 = ((cy + rr + 2.0).ceil().max(0.0) as usize).min(self.h);
        if x0 >= x1 || y0 >= y1 {
            return Rect::empty();
        }
        for y in y0..y1 {
            for x in x0..x1 {
                let ox = x as f32 + 0.5 - cx;
                let oy = y as f32 + 0.5 - cy;
                // rotate into the dab's frame and stretch the short axis by the aspect
                let dx = ox * self.rot.0 + oy * self.rot.1;
                let dy = (-ox * self.rot.1 + oy * self.rot.0) * self.inv_aspect;
                let d = (dx * dx + dy * dy).sqrt();
                // one-pixel smooth edge (the original box-filters its profile over 8 sub-samples)
                let edge = (r - d + 0.5).clamp(0.0, 1.0);
                if edge <= 0.0 {
                    continue;
                }
                // linear profile (points 0,0 and 1,1): full at the centre, nothing at the rim
                let profile = (1.0 - d / r).clamp(0.0, 1.0);
                let i = y * self.w + x;
                let m = &mut self.mask[i];
                match self.tool {
                    Tool::Pen | Tool::Line | Tool::Rectangle | Tool::Ellipse => {
                        // pen: no profile, anti-aliased edge ("aliasing=1")
                        *m = m.max(edge * self.power);
                    }
                    Tool::Pencil => {
                        let grain = 0.45 + 0.55 * hash(x as i32, y as i32);
                        *m += edge * grain * self.power * (1.0 - *m);
                    }
                    Tool::Airbrush | Tool::Oil | Tool::Wet | Tool::Eraser => {
                        *m += edge * profile * self.power * (1.0 - *m);
                    }
                    _ => {}
                }
                self.shade(i, px);
            }
        }
        Rect { x0, y0, x1, y1 }
    }

    /// Push the pixels under the brush along the movement since the last dab.
    /// Strength: power / 5, so the reference's default power 5 moves them fully (our mapping).
    fn warp_dab(&mut self, cx: f32, cy: f32, px: &mut [u8]) -> Rect {
        let Some((lx, ly)) = self.last_dab else { return Rect::empty() };
        let (mx, my) = (cx - lx, cy - ly);
        let r = self.radius;
        let x0 = (cx - r - 1.0).floor().max(0.0) as usize;
        let y0 = (cy - r - 1.0).floor().max(0.0) as usize;
        let x1 = ((cx + r + 2.0).ceil().max(0.0) as usize).min(self.w);
        let y1 = ((cy + r + 2.0).ceil().max(0.0) as usize).min(self.h);
        if x0 >= x1 || y0 >= y1 {
            return Rect::empty();
        }
        let strength = (self.raw_power / 5.0).clamp(0.0, 1.0);
        // copy of the area, with a margin as large as the movement, to sample from
        let m = (mx.abs().max(my.abs()) + 2.0).ceil() as usize;
        let (ox, oy) = (x0.saturating_sub(m), y0.saturating_sub(m));
        let (ex, ey) = ((x1 + m).min(self.w), (y1 + m).min(self.h));
        let (sw, sh) = (ex - ox, ey - oy);
        let mut src = vec![0u8; sw * sh * 4];
        for yy in 0..sh {
            let row = ((oy + yy) * self.w + ox) * 4;
            src[yy * sw * 4..(yy + 1) * sw * 4].copy_from_slice(&px[row..row + sw * 4]);
        }
        for y in y0..y1 {
            for x in x0..x1 {
                let i = y * self.w + x;
                if let Some(c) = &self.clip {
                    if c[i] == 0 {
                        continue;
                    }
                }
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                if d >= r {
                    continue;
                }
                let t = 1.0 - d / r;
                let wgt = t * t * (3.0 - 2.0 * t) * strength;
                let sx = x as f32 - mx * wgt;
                let sy = y as f32 - my * wgt;
                let sp = bilinear(&src, sw, sh, sx - ox as f32, sy - oy as f32);
                px[i * 4..i * 4 + 4].copy_from_slice(&sp);
            }
        }
        Rect { x0, y0, x1, y1 }
    }

    /// Keep the transparency of what is drawn: paint only on existing pixels.
    pub fn set_preserve(&mut self, on: bool) {
        self.preserve = on;
    }

    /// Paint only inside a selection mask.
    pub fn set_clip(&mut self, mask: std::sync::Arc<Vec<u8>>) {
        self.clip = Some(mask);
    }

    fn shade(&self, i: usize, px: &mut [u8]) {
        if let Some(c) = &self.clip {
            if c[i] == 0 {
                return;
            }
        }
        let m = (self.mask[i] * self.opacity).clamp(0.0, 1.0);
        let b = &self.before[i * 4..i * 4 + 4];
        let o = &mut px[i * 4..i * 4 + 4];
        let ba = b[3] as f32 / 255.0;
        if self.tool == Tool::Eraser || self.mode == DrawMode::Erase {
            o.copy_from_slice(b);
            o[3] = (ba * (1.0 - m) * 255.0).round() as u8;
            return;
        }
        let bc = [b[0] as f32, b[1] as f32, b[2] as f32];
        let brush = [self.color[0] as f32, self.color[1] as f32, self.color[2] as f32];
        if self.mode == DrawMode::Behind {
            // the paint goes under what is already there
            let pa = m;
            let oa = ba + pa * (1.0 - ba);
            if oa <= 0.0 {
                o.copy_from_slice(&[0, 0, 0, 0]);
                return;
            }
            for k in 0..3 {
                o[k] = ((bc[k] * ba + brush[k] * pa * (1.0 - ba)) / oa).round().clamp(0.0, 255.0) as u8;
            }
            o[3] = (oa * 255.0).round() as u8;
            return;
        }
        if self.mode != DrawMode::Color || self.preserve {
            // change only what is drawn; transparency stays
            if ba <= 0.0 {
                o.copy_from_slice(b);
                return;
            }
            let target = if self.mode == DrawMode::Color {
                brush
            } else {
                self.mode.recolor(brush, bc)
            };
            for k in 0..3 {
                o[k] = (bc[k] + (target[k] - bc[k]) * m).round().clamp(0.0, 255.0) as u8;
            }
            o[3] = b[3];
            return;
        }
        let mut c = [self.color[0] as f32, self.color[1] as f32, self.color[2] as f32];
        if self.tool == Tool::Wet && ba > 0.0 {
            for k in 0..3 {
                c[k] = c[k] * (1.0 - 0.5 * ba) + b[k] as f32 * 0.5 * ba;
            }
        }
        let oa = m + ba * (1.0 - m);
        if oa <= 0.0 {
            o.copy_from_slice(&[0, 0, 0, 0]);
            return;
        }
        for k in 0..3 {
            o[k] = ((c[k] * m + b[k] as f32 * ba * (1.0 - m)) / oa).round().clamp(0.0, 255.0) as u8;
        }
        o[3] = (oa * 255.0).round() as u8;
    }
}

/// Points along a shape outline, in canvas pixels.
pub fn shape_points(tool: Tool, a: (f32, f32), b: (f32, f32)) -> Vec<Vec<(f32, f32)>> {
    match tool {
        Tool::Line => vec![vec![a, b]],
        Tool::Rectangle => vec![vec![a, (b.0, a.1), b, (a.0, b.1), a]],
        Tool::Ellipse => {
            let cx = (a.0 + b.0) / 2.0;
            let cy = (a.1 + b.1) / 2.0;
            let rx = (b.0 - a.0).abs() / 2.0;
            let ry = (b.1 - a.1).abs() / 2.0;
            let n = ((rx + ry) * 0.5).clamp(24.0, 720.0) as usize;
            vec![(0..=n)
                .map(|i| {
                    let t = i as f32 / n as f32 * std::f32::consts::TAU;
                    (cx + rx * t.cos(), cy + ry * t.sin())
                })
                .collect()]
        }
        _ => vec![],
    }
}

/// Flood fill from (x, y) on `px`, matching colours within `tolerance` (0..=100).
#[allow(clippy::too_many_arguments)]
pub fn flood_fill(px: &mut [u8], w: usize, h: usize, x: usize, y: usize, color: [u8; 3], tolerance: f32, opacity: f32) -> Rect {
    if x >= w || y >= h {
        return Rect::empty();
    }
    let seed: [u8; 4] = px[(y * w + x) * 4..(y * w + x) * 4 + 4].try_into().unwrap();
    let tol = (tolerance / 100.0 * 255.0) as i32;
    let matches = |p: &[u8]| -> bool { (0..4).all(|k| (p[k] as i32 - seed[k] as i32).abs() <= tol) };
    let mut seen = vec![false; w * h];
    let mut stack = vec![(x, y)];
    let mut r = Rect::empty();
    let a = (opacity / 100.0).clamp(0.0, 1.0);
    let mut filled = vec![];
    while let Some((sx, sy)) = stack.pop() {
        let i = sy * w + sx;
        if seen[i] || !matches(&px[i * 4..i * 4 + 4]) {
            continue;
        }
        // scan left/right
        let mut lx = sx;
        while lx > 0 && !seen[sy * w + lx - 1] && matches(&px[(sy * w + lx - 1) * 4..(sy * w + lx) * 4]) {
            lx -= 1;
        }
        let mut rx = sx;
        while rx + 1 < w && !seen[sy * w + rx + 1] && matches(&px[(sy * w + rx + 1) * 4..(sy * w + rx + 2) * 4]) {
            rx += 1;
        }
        for xx in lx..=rx {
            seen[sy * w + xx] = true;
            filled.push(sy * w + xx);
            if sy > 0 {
                stack.push((xx, sy - 1));
            }
            if sy + 1 < h {
                stack.push((xx, sy + 1));
            }
        }
        r = r.union(&Rect {
            x0: lx,
            y0: sy,
            x1: rx + 1,
            y1: sy + 1,
        });
    }
    // the pixels just outside (anti-aliased line edges) get the colour behind them
    let mut edge = vec![];
    for &i in &filled {
        let (fx, fy) = (i % w, i / w);
        let n = [
            (fx > 0).then(|| i - 1),
            (fx + 1 < w).then(|| i + 1),
            (fy > 0).then(|| i - w),
            (fy + 1 < h).then(|| i + w),
        ];
        for j in n.into_iter().flatten() {
            if !seen[j] {
                seen[j] = true;
                edge.push(j);
            }
        }
    }
    for j in edge {
        let o = &mut px[j * 4..j * 4 + 4];
        let ea = o[3] as f32 / 255.0;
        let oa = ea + a * (1.0 - ea);
        if oa > 0.0 {
            for k in 0..3 {
                o[k] = ((o[k] as f32 * ea + color[k] as f32 * a * (1.0 - ea)) / oa).round() as u8;
            }
            o[3] = (oa * 255.0).round() as u8;
        }
    }
    if !r.is_empty() {
        r = Rect {
            x0: r.x0.saturating_sub(1),
            y0: r.y0.saturating_sub(1),
            x1: (r.x1 + 1).min(w),
            y1: (r.y1 + 1).min(h),
        };
    }
    for i in filled {
        let o = &mut px[i * 4..i * 4 + 4];
        let ba = o[3] as f32 / 255.0;
        let oa = a + ba * (1.0 - a);
        for k in 0..3 {
            o[k] = ((color[k] as f32 * a + o[k] as f32 * ba * (1.0 - a)) / oa.max(1e-6)).round() as u8;
        }
        o[3] = (oa * 255.0).round() as u8;
    }
    // grow by one pixel so anti-aliased line edges are covered
    r
}

/// Bilinear sample (straight RGBA, alpha-weighted colours).
fn bilinear(px: &[u8], w: usize, h: usize, x: f32, y: f32) -> [u8; 4] {
    let x = x.clamp(0.0, w as f32 - 1.0);
    let y = y.clamp(0.0, h as f32 - 1.0);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let mut acc = [0f32; 4];
    for (xi, yi, wt) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x1, y0, fx * (1.0 - fy)),
        (x0, y1, (1.0 - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        let i = (yi * w + xi) * 4;
        let a = px[i + 3] as f32;
        for k in 0..3 {
            acc[k] += px[i + k] as f32 * a * wt;
        }
        acc[3] += a * wt;
    }
    let mut out = [0u8; 4];
    if acc[3] > 0.0 {
        for k in 0..3 {
            out[k] = (acc[k] / acc[3]).round().clamp(0.0, 255.0) as u8;
        }
    }
    out[3] = acc[3].round().clamp(0.0, 255.0) as u8;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warp_pushes_pixels_along_the_stroke() {
        let (w, h) = (60, 20);
        let mut px = vec![0u8; w * h * 4];
        // left half red
        for y in 0..h {
            for x in 0..30 {
                px[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let mut st = Stroke::begin(0, &px.clone(), w, h, Tool::Warp, [0, 0, 0], Tool::Warp.defaults());
        st.to(25.0, 10.0, &mut px);
        st.to(40.0, 10.0, &mut px);
        assert!(px[(10 * w + 34) * 4 + 3] > 128, "red pushed to the right");
    }

    fn half_red(w: usize, h: usize) -> Vec<u8> {
        let mut px = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w / 2 {
                px[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        px
    }

    #[test]
    fn preserve_transparency_keeps_empty_pixels_empty() {
        let (w, h) = (60, 20);
        let mut px = half_red(w, h);
        let mut st = Stroke::begin(0, &px.clone(), w, h, Tool::Pen, [0, 0, 255], Tool::Pen.defaults());
        st.set_preserve(true);
        st.to(5.0, 10.0, &mut px);
        st.to(55.0, 10.0, &mut px);
        assert_eq!(px[(10 * w + 45) * 4 + 3], 0, "empty side stays empty");
        assert!(px[(10 * w + 15) * 4 + 2] > 200, "drawn side turns blue");
        assert_eq!(px[(10 * w + 15) * 4 + 3], 255);
    }

    #[test]
    fn behind_mode_paints_under_the_drawing() {
        let (w, h) = (60, 20);
        let mut px = half_red(w, h);
        let mut s = Tool::Pen.defaults();
        s.mode = DrawMode::Behind;
        let mut st = Stroke::begin(0, &px.clone(), w, h, Tool::Pen, [0, 0, 255], s);
        st.to(5.0, 10.0, &mut px);
        st.to(55.0, 10.0, &mut px);
        assert_eq!(&px[(10 * w + 15) * 4..(10 * w + 15) * 4 + 3], &[255, 0, 0], "red stays on top");
        assert!(px[(10 * w + 45) * 4 + 2] > 200, "blue under the empty side");
    }

    #[test]
    fn connections_follow_their_source_and_curve() {
        let c = Connection::pressure();
        let at = |pressure: f32| Inputs {
            pressure,
            ..Inputs::default()
        };
        assert_eq!(c.value(&at(0.5)), Some(0.5));
        let soft = Connection { curve: -1.0, ..c };
        assert!(soft.value(&at(0.5)).unwrap() > 0.8, "soft lifts light touches");
        let hard = Connection { curve: 1.0, ..c };
        assert!(hard.value(&at(0.5)).unwrap() < 0.1);
        let inv = Connection { invert: true, ..c };
        assert_eq!(inv.value(&at(1.0)), Some(0.0));
        assert_eq!(Connection::default().value(&at(1.0)), None);
        // a pen's tilt drives a setting like the pressure does
        let tilt = Connection {
            source: ConSource::Tilt,
            ..Connection::default()
        };
        let leaning = Inputs {
            tilt: 0.25,
            ..Inputs::default()
        };
        assert_eq!(tilt.value(&leaning), Some(0.25));
        assert_eq!((ConSource::Azimuth.id(), ConSource::Tilt.id(), ConSource::Twist.id()), (9, 10, 11));
        assert!(ConSource::Twist.is_turn() && !ConSource::Tilt.is_turn());
        assert_eq!(ConSource::Pressure.id(), 6);
        // a stroke whose size follows the pressure gets thinner when lighter
        let mut s = Tool::Pen.defaults();
        s.con_size = Connection::pressure();
        let (w, h) = (60, 20);
        let mut px = vec![0u8; w * h * 4];
        let mut st = Stroke::begin(0, &px.clone(), w, h, Tool::Pen, [0, 0, 0], s);
        let r = st.to_p(30.0, 10.0, 0.1, &mut px);
        assert!(r.x1 - r.x0 < 12, "a light touch makes a small dab");
    }

    #[test]
    fn draw_mode_names_round_trip() {
        for m in ALL_DRAW_MODES {
            assert_eq!(DrawMode::from_name(m.name()), Some(m));
        }
        assert_eq!(DrawMode::Darken.id(), 44);
    }
}
