//! The plugin's entries and their inputs, in one place: the manifest's input schemas are generated
//! from these definitions (`cargo run --bin manifest`), and the entries read their parameters
//! through them, so the dialog and the code cannot disagree about names, defaults and limits.
//!
//! SolidCraft draws one flat list of fields, in the order they are declared here
//! (`x-solidcraft-field-order: declared`, which SolidCraft 0.50 honours; the manifest asks for
//! 0.50.0 at least). No field is required, so the dialog marks none with "*": every field has a
//! default, which fills a key a program leaves out, so programs pass only what they change. Fields show, hide or grey out through declarative
//! conditions on their siblings (`x-solidcraft-visible-when` / `-enabled-when`), and the
//! entries' dialog hook (`dialog.rs`) fills the read-only result line and the tooltips.

use serde_json::{json, Map, Value};

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Num {
        min: f64,
        max: f64,
        unit: &'static str,
    },
    Int {
        min: i64,
        max: i64,
    },
    Bool,
    /// A dropdown: (value, shown title, picture or "") per option. A picture is a package path
    /// (`x-solidcraft-icon`); SolidCraft shows pictures only when every option has one.
    Choice(&'static [(&'static str, &'static str, &'static str)]),
    Text,
    /// Read-only text the dialog hook fills (the result line). Programs may leave it out.
    Info,
    /// One thing picked in the viewport, of these kinds (`body`, `face`, ...); the plugin gets
    /// the app's selection reference, such as `{"type": "body", "name": "Gear 1"}`.
    Pick(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug)]
pub enum Def {
    N(f64),
    I(i64),
    B(bool),
    S(&'static str),
    /// No default (a pick).
    None,
}

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub key: &'static str,
    pub title: &'static str,
    pub desc: &'static str,
    pub kind: Kind,
    pub def: Def,
    /// Conditions as JSON text (SolidCraft's declarative form): shown only when, enabled only when.
    pub show: Option<&'static str>,
    pub enable: Option<&'static str>,
}

const fn base(key: &'static str, title: &'static str, desc: &'static str, kind: Kind, def: Def) -> Field {
    Field { key, title, desc, kind, def, show: None, enable: None }
}
const fn num(key: &'static str, title: &'static str, desc: &'static str, def: f64, min: f64, max: f64, unit: &'static str) -> Field {
    base(key, title, desc, Kind::Num { min, max, unit }, Def::N(def))
}
const fn int(key: &'static str, title: &'static str, desc: &'static str, def: i64, min: i64, max: i64) -> Field {
    base(key, title, desc, Kind::Int { min, max }, Def::I(def))
}
const fn boolean(key: &'static str, title: &'static str, desc: &'static str, def: bool) -> Field {
    base(key, title, desc, Kind::Bool, Def::B(def))
}
const fn choice(
    key: &'static str,
    title: &'static str,
    desc: &'static str,
    options: &'static [(&'static str, &'static str, &'static str)],
    def: &'static str,
) -> Field {
    base(key, title, desc, Kind::Choice(options), Def::S(def))
}
const fn text(key: &'static str, title: &'static str, desc: &'static str, def: &'static str) -> Field {
    base(key, title, desc, Kind::Text, Def::S(def))
}
const fn pick(key: &'static str, title: &'static str, desc: &'static str, accept: &'static [&'static str]) -> Field {
    base(key, title, desc, Kind::Pick(accept), Def::None)
}
const fn info(title: &'static str) -> Field {
    note(title, RESULT_HELP)
}
const fn note(title: &'static str, desc: &'static str) -> Field {
    base(SUMMARY, title, desc, Kind::Info, Def::S("-"))
}
const fn read_only(f: Field) -> Field {
    Field { enable: Some(NEVER), ..f }
}
const fn when(f: Field, cond: &'static str) -> Field {
    Field { show: Some(cond), ..f }
}

pub struct Entry {
    /// The entry's id, as programs call it.
    pub name: &'static str,
    /// Its name in the menus.
    pub label: &'static str,
    /// What it does to the design ("edit": it adds features).
    pub effect: &'static str,
    /// The entry's icon in the ribbon, the menus and the dialog header: a package path.
    pub icon: &'static str,
    pub fields: &'static [Field],
}

/// The read-only result line every entry has.
pub const SUMMARY: &str = "summary";

/// The parameter that makes a build replace gears made before: the id of their timeline folder,
/// 0 for new gears. SolidCraft takes only declared inputs, so it is a field: read-only, last, and
/// hidden by the dialog hook while it is 0; `edit-gears` fills it in the populated form.
pub const REPLACE: &str = "replace";

/// The entry that opens a gear's own dialog again, with the values it was built with.
pub const EDIT: &str = "edit-gears";

/// Longest result line, in bytes.
pub const INFO_MAX: usize = 200;

/// The result line's description.
pub const RESULT_HELP: &str = "Hover the fields for the diameters.";

pub const HANDS: &[(&str, &str, &str)] = &[("right", "Right", ""), ("left", "Left", "")];
pub const MATES: &[(&str, &str, &str)] = &[
    ("gear", "External gear", "icons/mate_gear.svg"),
    ("ring", "Ring gear (internal)", "icons/mate_ring.svg"),
    ("rack", "Rack", "icons/mate_rack.svg"),
];

/// The belt profiles of `timing-pulleys`, in `pulley::PROFILES` order.
pub const BELTS: &[(&str, &str, &str)] = &[
    ("gt2-2", "GT2 2 mm", ""),
    ("gt2-3", "GT2 3 mm", ""),
    ("htd-3m", "HTD 3M", ""),
    ("htd-5m", "HTD 5M", ""),
    ("htd-8m", "HTD 8M", ""),
    ("t2.5", "T2.5", ""),
    ("t5", "T5", ""),
    ("t10", "T10", ""),
    ("mxl", "MXL", ""),
    ("xl", "XL", ""),
];
pub const FLANGES: &[(&str, &str, &str)] = &[("none", "None", ""), ("one", "One, below the teeth", ""), ("both", "Both sides", "")];

// Conditions, as SolidCraft's declarative JSON.
const NEVER: &str = r#"{"not":{"all":[]}}"#;
const FLANGED: &str = r#"{"not":{"field":"flanges","eq":"none"}}"#;
const HELICAL: &str = r#"{"not":{"field":"helix_angle","eq":0}}"#;
const NOT_RACK_HELICAL: &str = r#"{"all":[{"not":{"field":"helix_angle","eq":0}},{"not":{"field":"mate","eq":"rack"}}]}"#;
const NOT_RACK: &str = r#"{"not":{"field":"mate","eq":"rack"}}"#;
const C2_RING: &str = r#"{"all":[{"field":"create_2","eq":true},{"field":"mate","eq":"ring"}]}"#;
const C2_RACK: &str = r#"{"all":[{"field":"create_2","eq":true},{"field":"mate","eq":"rack"}]}"#;

const MODULE: Field = num("module", "Module", "Normal module m.", 2.0, 0.2, 50.0, "mm");
const PRESSURE: Field = num("pressure_angle", "Pressure angle", "Normal pressure angle.", 20.0, 10.0, 35.0, "°");
const BACKLASH: Field = num("backlash", "Backlash", "Of the pair, taken half from each gear.", 0.0, 0.0, 2.0, "mm");
const NAME: Field = text("name", "Name", "Prefix of the body names.", "Gear");
const ADDENDUM: Field = num("addendum", "Addendum coefficient", "× module (standard 1).", 1.0, 0.5, 1.5, "");
const DEDENDUM: Field = num("dedendum", "Dedendum coefficient", "× module (standard 1.25).", 1.25, 0.8, 2.0, "");
const REPLACING: Field = read_only(int(
    REPLACE,
    "Replaces folder",
    "Set by Edit Gears: the timeline folder of the gears this run replaces (0: new gears).",
    0,
    0,
    2_000_000_000,
));
const FILLET: Field =
    num("root_fillet", "Root fillet coefficient", "Cutter tip radius × module (DIN 867: 0.38); ring gears use 0.25.", 0.38, 0.0, 0.6, "");

/// Spur and helical gears in pairs: gear 1 (on the Z axis) with an external gear, a ring gear or a
/// rack. Either side can be left out; the other is still placed where it meshes. Only the toothed
/// gear is made: bores, hubs and keyways are ordinary features modelled afterwards.
pub const SPUR: &[Field] = &[
    info("Result"),
    choice("mate", "Gear 2 type", "Gear 1 meshes with an external gear, a ring gear or a rack.", MATES, "gear"),
    MODULE,
    PRESSURE,
    num("helix_angle", "Helix angle", "0 = spur (straight) teeth.", 0.0, 0.0, 45.0, "°"),
    when(choice("helix_hand", "Gear 1 helix hand", "Gear 2 gets the hand that meshes.", HANDS, "right"), HELICAL),
    when(boolean("herringbone", "Herringbone", "Double helical teeth, no axial thrust.", false), NOT_RACK_HELICAL),
    num("face_width", "Face width", "Both gears.", 10.0, 0.5, 2000.0, "mm"),
    BACKLASH,
    boolean("create_1", "Create gear 1", "On the Z axis; with a ring gear, the ring is.", true),
    int("teeth_1", "Gear 1 teeth", "Below 17 (20°) the root is undercut as hobbed.", 18, 4, 200),
    num("profile_shift_1", "Gear 1 profile shift", "x1; positive: thicker teeth, larger tip.", 0.0, -1.0, 1.5, ""),
    boolean("create_2", "Create gear 2", "Placed in mesh with gear 1.", true),
    int("teeth_2", "Gear 2 teeth", "Teeth of gear 2, the ring gear or the rack (its length in pitches).", 36, 1, 400),
    when(
        num("profile_shift_2", "Gear 2 profile shift", "x2; the centre distance follows. Ring gear: positive outward.", 0.0, -1.0, 1.5, ""),
        NOT_RACK,
    ),
    when(
        num("rim_diameter", "Ring Ø (0=auto)", "Outer diameter of the ring; 0 = root diameter + 6 modules.", 0.0, 0.0, 5000.0, "mm"),
        C2_RING,
    ),
    when(num("rack_height", "Rack height (0=auto)", "Back face to pitch line; 0 = 6 modules.", 0.0, 0.0, 2000.0, "mm"), C2_RACK),
    NAME,
    ADDENDUM,
    DEDENDUM,
    FILLET,
    REPLACING,
];

pub const BEVEL: &[Field] = &[
    info("Result"),
    MODULE,
    PRESSURE,
    num("shaft_angle", "Shaft angle", "Pitch cones up to 72°.", 90.0, 20.0, 120.0, "°"),
    num("face_width", "Face width", "Along the cone; usually 1/3 of the cone distance, at most 0.42.", 8.0, 0.5, 1000.0, "mm"),
    num("profile_shift", "Profile shift", "x for gear 1, -x for gear 2.", 0.0, -0.5, 0.8, ""),
    BACKLASH,
    boolean("create_1", "Create gear 1", "Gear 1 (the pinion) sits on the Z axis.", true),
    int("teeth_1", "Gear 1 teeth", "", 15, 6, 140),
    boolean("create_2", "Create gear 2", "Placed in mesh with gear 1.", true),
    int("teeth_2", "Gear 2 teeth", "", 30, 6, 140),
    NAME,
    REPLACING,
];

pub const WORM: &[Field] = &[
    info("Result"),
    num("module", "Module", "Axial module of the worm.", 2.0, 0.3, 30.0, "mm"),
    num("pressure_angle", "Pressure angle", "Axial pressure angle of the worm.", 20.0, 10.0, 35.0, "°"),
    choice("helix_hand", "Hand", "The wheel gets the same hand.", HANDS, "right"),
    BACKLASH,
    boolean("create_1", "Create worm", "Along Y beside the wheel, in mesh.", true),
    int("starts", "Worm starts", "Ratio = wheel teeth / starts.", 1, 1, 8),
    num("worm_diameter", "Worm pitch diameter", "d1, usually 8 to 16 modules.", 20.0, 2.0, 1000.0, "mm"),
    num("worm_length", "Worm length", "", 40.0, 2.0, 3000.0, "mm"),
    boolean("create_2", "Create worm wheel", "The wheel sits on the Z axis.", true),
    int("wheel_teeth", "Wheel teeth", "", 30, 10, 80),
    num("wheel_face_width", "Wheel face width", "", 12.0, 0.5, 1000.0, "mm"),
    text("name", "Name", "Prefix of the body names.", "Worm"),
    REPLACING,
];

/// Timing belt pulleys in pairs: pulley 1 on the Z axis, pulley 2 on +X where the belt puts it.
/// Either can be left out. As with the gears, bores and hubs are modelled afterwards.
pub const PULLEY: &[Field] = &[
    info("Result"),
    choice("profile", "Belt profile", "GT2: Gates' belt, in no standard. HTD: ISO 13050. T: DIN 7721. MXL, XL: ISO 5294.", BELTS, "gt2-2"),
    num("face_width", "Width", "Of the toothed part; the standard's minimum for each belt width is in the tooltip.", 7.0, 1.0, 500.0, "mm"),
    num(
        "groove_clearance",
        "Groove clearance",
        "Widens every groove by this much, half on each side; 0 = as the standard. Printed pulleys: try 0.1 to 0.2.",
        0.0,
        0.0,
        0.5,
        "mm",
    ),
    choice("flanges", "Flanges", "Discs on the pulley that keep the belt on.", FLANGES, "both"),
    when(
        num("flange_height", "Flange height (0=auto)", "Above the outside diameter; 0 = the standard's minimum.", 0.0, 0.0, 50.0, "mm"),
        FLANGED,
    ),
    when(num("flange_thickness", "Flange thickness", "", 1.0, 0.2, 20.0, "mm"), FLANGED),
    boolean("create_1", "Create pulley 1", "On the Z axis.", true),
    int("teeth_1", "Pulley 1 teeth", "", 20, 8, 250),
    boolean("create_2", "Create pulley 2", "On +X, at the centre distance the belt sets.", true),
    int("teeth_2", "Pulley 2 teeth", "", 40, 8, 250),
    int("belt_teeth", "Belt teeth", "Belt length in teeth (pitch length ÷ pitch); sets the centre distance.", 100, 20, 2000),
    text("name", "Name", "Prefix of the body names.", "Pulley"),
    REPLACING,
];

/// Picks a gear and opens the dialog it was made with (see `edit.rs`).
pub const EDIT_FIELDS: &[Field] = &[
    note("Gears", "The gears the pick belongs to."),
    pick("gear", "Gear", "A gear or pulley made by Gear Kit; Run opens its dialog with its values.", &["body"]),
];

pub const ENTRIES: &[Entry] = &[
    Entry { name: "spur-gears", label: "Spur / Helical Gears", effect: "edit", icon: "icons/spur_gears.svg", fields: SPUR },
    Entry { name: "bevel-gears", label: "Bevel Gears", effect: "edit", icon: "icons/bevel_gears.svg", fields: BEVEL },
    Entry { name: "worm-gears", label: "Worm Gears", effect: "edit", icon: "icons/worm_gears.svg", fields: WORM },
    Entry { name: "timing-pulleys", label: "Timing Pulleys", effect: "edit", icon: "icons/timing_pulleys.svg", fields: PULLEY },
    Entry { name: EDIT, label: "Edit Gears", effect: "read", icon: "icons/edit_gears.svg", fields: EDIT_FIELDS },
];

/// The engine commands the plugin calls (see `cad.rs`), listed in the manifest: SolidCraft lets
/// a plugin run only these.
pub const COMMANDS: &[&str] = &[
    "sketch.create",
    "sketch.finish",
    "sketch.line",
    "sketch.arc.center_point",
    "sketch.spline.fit_point",
    "sketch.circle.center",
    "sketch.rectangle.two_point",
    "sketch.pattern.circular",
    "sketch.pattern.rectangular",
    "solid.cylinder",
    "solid.extrude",
    "solid.sweep",
    "solid.revolve",
    "solid.coil",
    "solid.move",
    "solid.mirror",
    "browser.visibility",
    // each build in a timeline folder, recorded in the design, replaceable in place (edit.rs)
    "timeline.group",
    "timeline.inspect",
    "timeline.delete",
    "timeline.roll_to",
    "document.inspect",
    "storage.get",
    "storage.patch",
];

impl Field {
    fn schema(&self) -> Value {
        // whole numbers without ".0", as a person would write them
        let n = |x: f64| if x.fract() == 0.0 && x.abs() < 1e15 { json!(x as i64) } else { json!(x) };
        let cond = |s: &str| serde_json::from_str::<Value>(s).expect("condition JSON");
        let mut m = Map::new();
        m.insert("title".into(), json!(self.title));
        if !self.desc.is_empty() {
            m.insert("description".into(), json!(self.desc));
        }
        match self.kind {
            Kind::Num { min, max, unit } => {
                m.insert("type".into(), json!("number"));
                m.insert("minimum".into(), n(min));
                m.insert("maximum".into(), n(max));
                if !unit.is_empty() {
                    m.insert("unit".into(), json!(unit));
                }
            }
            Kind::Int { min, max } => {
                m.insert("type".into(), json!("integer"));
                m.insert("minimum".into(), json!(min));
                m.insert("maximum".into(), json!(max));
            }
            Kind::Bool => {
                m.insert("type".into(), json!("boolean"));
            }
            Kind::Choice(opts) => {
                m.insert("type".into(), json!("string"));
                let opts: Vec<Value> = opts
                    .iter()
                    .map(|(v, t, icon)| {
                        let mut o = json!({"const": v, "title": t});
                        if !icon.is_empty() {
                            o["x-solidcraft-icon"] = json!(icon);
                        }
                        o
                    })
                    .collect();
                m.insert("oneOf".into(), Value::Array(opts));
            }
            Kind::Text => {
                m.insert("type".into(), json!("string"));
                m.insert("minLength".into(), json!(1));
                m.insert("maxLength".into(), json!(60));
            }
            Kind::Info => {
                m.insert("type".into(), json!("string"));
                m.insert("maxLength".into(), json!(INFO_MAX));
                // never editable: the hook writes it
                m.insert("x-solidcraft-enabled-when".into(), json!({"not": {"all": []}}));
            }
            Kind::Pick(accept) => {
                m.insert("type".into(), json!("array"));
                m.insert("minItems".into(), json!(1));
                m.insert("maxItems".into(), json!(1));
                m.insert("x-solidcraft-selection".into(), json!({"version": 1, "accept": accept}));
            }
        }
        if let Some(c) = self.show {
            m.insert("x-solidcraft-visible-when".into(), cond(c));
        }
        if let Some(c) = self.enable {
            m.insert("x-solidcraft-enabled-when".into(), cond(c));
        }
        let default = match self.def {
            Def::N(v) => n(v),
            Def::I(v) => json!(v),
            Def::B(v) => json!(v),
            Def::S(v) => json!(v),
            Def::None => Value::Null,
        };
        if !default.is_null() {
            m.insert("default".into(), default);
        }
        Value::Object(m)
    }
}

impl Entry {
    pub fn input_schema(&self) -> Value {
        let mut props = Map::new();
        for f in self.fields {
            props.insert(f.key.into(), f.schema());
        }
        json!({
            "type": "object",
            "x-solidcraft-icon": self.icon,
            "x-solidcraft-field-order": "declared",
            "properties": props,
        })
    }

    /// The field called `key`.
    pub fn field(&self, key: &str) -> Option<Field> {
        self.fields.iter().find(|f| f.key == key).copied()
    }
}

/// The entry called `name`.
pub fn entry(name: &str) -> Option<&'static Entry> {
    ENTRIES.iter().find(|e| e.name == name)
}

/// A bad input: which field and why.
#[derive(Debug, Clone)]
pub struct Bad {
    pub field: String,
    pub reason: String,
    /// The reason in a few words for the dialog's result line, which SolidCraft draws as a narrow
    /// single-line field (about 24 characters show): "cone 80.5° > 72°". None: the line shows
    /// the field's title and the whole reason, cut where the field ends.
    pub short: Option<String>,
}

/// A bad input: `field` and why, in words that follow the field's title ("must be ..."). The
/// geometry modules' own messages get their short form from `short_for`.
pub fn bad(field: &str, reason: impl Into<String>) -> Bad {
    let reason = reason.into();
    let short = short_for(&reason).map(String::from);
    Bad { field: field.into(), reason, short }
}

impl Bad {
    /// With `s` as its short form (see `short`).
    pub fn short(mut self, s: impl Into<String>) -> Bad {
        self.short = Some(s.into());
        self
    }
}

/// The short form of a message the geometry modules give (`involute`, `pair`, `bevel`, `worm`,
/// `rack`, `pulley`), by what it says.
fn short_for(reason: &str) -> Option<&'static str> {
    const SHORT: &[(&str, &str)] = &[
        ("shifts are too negative", "shift too negative"),
        ("has no thickness", "backlash too large"),
        ("cutter tip would be pointed", "cutter tip pointed"),
        ("undercut all the way", "undercut to tip"),
        ("root fillet", "no room for fillet"),
        ("comes to a point", "thread pointed"),
        ("come to a point", "teeth pointed"),
        ("no height left", "no tooth height"),
        ("close up at the root", "root spaces close"),
        ("throat reaches the axis", "wheel too wide"),
        ("leave no land", "no land left"),
        ("roundings overlap", "roundings overlap"),
        ("too narrow for its rounding", "groove too narrow"),
        ("misses the outside", "groove does not fit"),
        ("does not fit", "groove does not fit"),
    ];
    SHORT.iter().find(|(says, _)| reason.contains(says)).map(|(_, short)| *short)
}

/// Parameters of one call, read through the entry's field definitions: a missing or null value
/// is the field's default, anything outside the field's limits is a `Bad`. Asking for a field
/// the entry does not have, or with the wrong type, is a bug and panics.
pub struct Args<'a> {
    pub entry: &'a Entry,
    pub v: Value,
}

impl<'a> Args<'a> {
    pub fn new(entry: &'a Entry, v: Value) -> Result<Args<'a>, Bad> {
        let v = if v.is_null() { json!({}) } else { v };
        let obj = v.as_object().ok_or_else(|| bad("params", "must be an object"))?;
        for k in obj.keys() {
            if entry.field(k).is_none() {
                return Err(bad(k, format!("unknown parameter for {}", entry.name)));
            }
        }
        Ok(Args { entry, v })
    }

    fn def(&self, key: &str) -> Field {
        self.entry.field(key).unwrap_or_else(|| panic!("no field {key} in {}", self.entry.name))
    }

    pub fn num(&self, key: &str) -> Result<f64, Bad> {
        let f = self.def(key);
        let Kind::Num { min, max, .. } = f.kind else { panic!("{key} is not a number") };
        let Def::N(d) = f.def else { unreachable!() };
        let x = match self.v.get(key) {
            None | Some(Value::Null) => d,
            Some(v) => v.as_f64().ok_or_else(|| bad(key, "must be a number"))?,
        };
        if !x.is_finite() || x < min || x > max {
            return Err(bad(key, format!("must be between {min} and {max}")));
        }
        Ok(x)
    }

    pub fn int(&self, key: &str) -> Result<u32, Bad> {
        let f = self.def(key);
        let Kind::Int { min, max } = f.kind else { panic!("{key} is not an integer") };
        let Def::I(d) = f.def else { unreachable!() };
        let x = match self.v.get(key) {
            None | Some(Value::Null) => d,
            Some(v) => match v.as_i64() {
                Some(i) => i,
                None => match v.as_f64() {
                    Some(f) if f.fract() == 0.0 => f as i64,
                    _ => return Err(bad(key, "must be a whole number")),
                },
            },
        };
        if x < min || x > max {
            return Err(bad(key, format!("must be between {min} and {max}")));
        }
        Ok(x as u32)
    }

    pub fn flag(&self, key: &str) -> Result<bool, Bad> {
        let f = self.def(key);
        let Def::B(d) = f.def else { panic!("{key} is not a boolean") };
        match self.v.get(key) {
            None | Some(Value::Null) => Ok(d),
            Some(v) => v.as_bool().ok_or_else(|| bad(key, "must be true or false")),
        }
    }

    pub fn choice(&self, key: &str) -> Result<&'static str, Bad> {
        let f = self.def(key);
        let (Kind::Choice(opts), Def::S(d)) = (f.kind, f.def) else { panic!("{key} is not a choice") };
        let s = match self.v.get(key) {
            None | Some(Value::Null) => d,
            Some(v) => v.as_str().ok_or_else(|| bad(key, "must be text"))?,
        };
        opts.iter()
            .map(|(v, _, _)| *v)
            .find(|o| o.eq_ignore_ascii_case(s.trim()))
            .ok_or_else(|| bad(key, format!("must be one of {}", opts.iter().map(|(v, _, _)| *v).collect::<Vec<_>>().join(", "))))
    }

    /// The folder of the gears this build replaces, if it replaces any (`REPLACE` is not 0).
    pub fn replace(&self) -> Result<Option<u64>, Bad> {
        Ok(Some(self.int(REPLACE)? as u64).filter(|&id| id > 0))
    }

    /// The one thing picked for `key`, as the app's selection reference.
    pub fn pick(&self, key: &str) -> Result<Value, Bad> {
        let f = self.def(key);
        let Kind::Pick(accept) = f.kind else { panic!("{key} is not a pick") };
        let item = match self.v.get(key) {
            Some(Value::Array(a)) if a.len() == 1 => a[0].clone(),
            Some(Value::Array(a)) if !a.is_empty() => return Err(bad(key, "takes one pick")),
            None | Some(Value::Null) | Some(Value::Array(_)) => return Err(bad(key, "pick one")),
            Some(_) => return Err(bad(key, "must be a list of picks")),
        };
        let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
        if !accept.contains(&kind) {
            return Err(bad(key, format!("must be a {}", accept.join(" or "))));
        }
        Ok(item)
    }

    /// Every field's value as this call reads it (defaults filled in), without the result line
    /// and picks: the parameters a build is recorded with, so editing it later starts from
    /// exactly what was built, whatever later versions make the defaults.
    pub fn resolved(&self) -> Value {
        let mut m = Map::new();
        for f in self.entry.fields {
            let v = match f.kind {
                Kind::Info | Kind::Pick(_) => continue,
                _ if f.key == REPLACE => continue,
                Kind::Num { .. } => self.num(f.key).map(|x| json!(x)),
                Kind::Int { .. } => self.int(f.key).map(|x| json!(x)),
                Kind::Bool => self.flag(f.key).map(|x| json!(x)),
                Kind::Choice(_) => self.choice(f.key).map(|x| json!(x)),
                Kind::Text => self.text(f.key).map(|x| json!(x)),
            };
            if let Ok(v) = v {
                m.insert(f.key.into(), v);
            }
        }
        Value::Object(m)
    }

    pub fn text(&self, key: &str) -> Result<String, Bad> {
        let f = self.def(key);
        let Def::S(d) = f.def else { panic!("{key} is not text") };
        let s = match self.v.get(key) {
            None | Some(Value::Null) => d.to_string(),
            Some(v) => v.as_str().ok_or_else(|| bad(key, "must be text"))?.trim().to_string(),
        };
        if s.is_empty() || s.chars().count() > 60 {
            return Err(bad(key, "must be 1 to 60 characters"));
        }
        Ok(s)
    }
}

/// The GitHub repository Gear Kit is released from (`tools/release.sh`). The manifest names it
/// (`updates`), so SolidCraft offers each new release as an update on the plugin's card.
pub const GITHUB_REPO: &str = "rob089/solidcraft-gear-kit";

/// The oldest SolidCraft Gear Kit runs on: 0.50.0 draws the fields in declared order without
/// marking them all required.
pub const MIN_HOST: &str = "0.50.0";

/// The manifest, as SolidCraft reads it. It has no `description`: SolidCraft writes the
/// description into every dialog's header line, after the name and version, and the dialogs are
/// better without it (the README has the long form).
pub fn manifest(version: &str) -> Value {
    let entries: Vec<Value> = ENTRIES
        .iter()
        .map(|e| {
            json!({
                "name": e.name,
                "effect": e.effect,
                "input_schema": e.input_schema(),
                "dialog_hook": true,
                "slots": [{"kind": "menu", "name": e.label}],
            })
        })
        .collect();
    json!({
        "id": crate::edit::PLUGIN_ID,
        "name": "Gear Kit",
        "version": version,
        "min_host_version": MIN_HOST,
        "updates": {"github": GITHUB_REPO},
        "capabilities": ["commands@0.2", "dialog@0.1", "storage@0.1", "forms@0.1"],
        "runtime": {
            "module": "plugin.wasm",
            "entries": entries,
            "commands": COMMANDS,
            "data_format": crate::edit::DATA_FORMAT,
            "limits": {"fuel": 20_000_000_000u64, "memory_mib": 64, "deadline_ms": crate::cad::DEADLINE_MS},
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_are_drawable() {
        // every entry's schema uses only what the SolidCraft dialog draws
        for e in ENTRIES {
            let s = e.input_schema();
            for (k, p) in s["properties"].as_object().unwrap() {
                assert!(p.get("type").is_some(), "{}.{k}", e.name);
                for bad in ["$ref", "anyOf", "allOf", "const", "items", "properties", "enum"] {
                    assert!(p.get(bad).is_none(), "{}.{k}: {bad}", e.name);
                }
                // conditions name existing siblings only
                for c in ["x-solidcraft-visible-when", "x-solidcraft-enabled-when"] {
                    if let Some(c) = p.get(c) {
                        for name in c.to_string().split("\"field\":\"").skip(1) {
                            let name = name.split('"').next().unwrap();
                            assert!(e.field(name).is_some(), "{}.{k} names {name}", e.name);
                        }
                    }
                }
            }
            assert!(e.name.len() <= 32);
            assert_eq!(e.fields[0].key, SUMMARY);
            let mut keys: Vec<_> = e.fields.iter().map(|f| f.key).collect();
            keys.sort();
            keys.dedup();
            assert_eq!(keys.len(), e.fields.len(), "{}: duplicate keys", e.name);
        }
        // every image the manifest names is in the package (icons/, copied by build.sh)
        let root = env!("CARGO_MANIFEST_DIR");
        let mut images: Vec<&str> = ENTRIES.iter().map(|e| e.icon).collect();
        for e in ENTRIES {
            for f in e.fields {
                if let Kind::Choice(opts) = f.kind {
                    let named: Vec<&str> = opts.iter().map(|(_, _, i)| *i).filter(|i| !i.is_empty()).collect();
                    assert!(
                        named.is_empty() || named.len() == opts.len(),
                        "{}.{}: pictures show only when every option has one",
                        e.name,
                        f.key
                    );
                    images.extend(named);
                }
            }
        }
        for i in images {
            assert!(i.starts_with("icons/") && i.ends_with(".svg"), "{i}");
            assert!(std::path::Path::new(root).join(i).is_file(), "{i} missing");
        }
        // no description: SolidCraft would put it into every dialog's header
        assert!(manifest("0.0.0").get("description").is_none());
        // the plugin.json in the repository (for plugin lists that read it) is the current one
        let committed: Value =
            serde_json::from_str(&std::fs::read_to_string(std::path::Path::new(root).join("plugin.json")).unwrap()).unwrap();
        assert_eq!(committed, manifest(env!("CARGO_PKG_VERSION")), "plugin.json is out of date: run ./build.sh");
        // SolidCraft refuses a plugin.json over 64 KiB
        let size = manifest("0.0.0").to_string().len();
        assert!(size < 64 * 1024, "manifest {size} bytes, over 64 KiB");
    }

    #[test]
    fn defaults_and_limits() {
        let e = entry("spur-gears").unwrap();
        let a = Args::new(e, json!({"teeth_1": 31})).unwrap();
        assert_eq!(a.int("teeth_1").unwrap(), 31);
        assert_eq!(a.num("module").unwrap(), 2.0);
        assert_eq!(a.choice("helix_hand").unwrap(), "right");
        assert_eq!(a.choice("mate").unwrap(), "gear");
        let a = Args::new(e, json!({"teeth_1": 2, "mate": "Ring"})).unwrap();
        assert!(a.int("teeth_1").is_err());
        assert_eq!(a.choice("mate").unwrap(), "ring");
        assert!(Args::new(e, json!({"teeths": 2})).is_err());
        // bores and hubs are modelled afterwards, not by the plugin
        assert!(Args::new(e, json!({"bore_1": 10})).is_err());
    }
}
