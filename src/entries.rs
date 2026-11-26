//! The plugin's entries. Each one is split in two: a plan reads and checks the inputs and works out
//! the pair without touching the design, and a build makes the features from the plan. The dialog
//! hook (`dialog.rs`) runs the same plans while the user types, so every problem a plan finds is
//! shown before Run, and a build fails only when the engine does.
//!
//! Every entry defines a pair. Gear 1 (or the worm wheel) sits on the Z axis, except in a ring gear
//! pair, where the ring does; the other part is always placed where it meshes, so gears built one
//! at a time (Create gear 1 / gear 2) still fit.
//!
//! A build's features go in a timeline folder of their own, and the design keeps a record of the
//! build, so `edit-gears` can open its dialog again and the build can replace it (`edit.rs`).

use crate::bevel::{self, BevelGear};
use crate::cad::{self, Cad, Host};
use crate::cyl::{self, CylSpec};
use crate::edit::{self, Slot, Store};
use crate::geom::r9;
use crate::involute::Tooth;
use crate::pair::{self, Mesh, PairType};
use crate::pulley::{self, Profile, PulleySpec};
use crate::rack::{self, RackSpec};
use crate::schema::{self, bad, Args, Bad};
use crate::worm::{self, WormSpec};
use serde_json::{json, Value};
use std::f64::consts::PI;

/// Why an entry failed: a bad input (the field and why), or the engine refused a command.
#[derive(Debug)]
pub enum Error {
    Input(Bad),
    Failed(String),
}

impl From<Bad> for Error {
    fn from(b: Bad) -> Error {
        Error::Input(b)
    }
}

impl From<String> for Error {
    fn from(s: String) -> Error {
        Error::Failed(s)
    }
}

pub type R<T> = Result<T, Error>;

/// Run `entry` with its parameters: plan, then build into a timeline folder, in place of the
/// gears `replace` names if it names any, and record the build in the design. The reply holds the
/// pair's numbers, the names of the bodies made and the folder.
pub fn execute(host: &mut dyn Host, entry: &str, params: Value) -> R<Value> {
    if entry == schema::EDIT {
        return edit::open(host, params);
    }
    let e = schema::entry(entry).ok_or_else(|| bad("entry", format!("unknown entry {entry}")))?;
    let a = Args::new(e, params)?;
    let plan = Plan::new(entry, &a)?;
    let replace = a.replace()?;
    let mut cad = Cad::new(host);
    let store = Store::load(&mut cad)?;
    let slot = match replace {
        Some(id) => Slot::open(&mut cad, id, store.get(id))?,
        None => None,
    };
    let built = plan.build(&mut cad);
    cad.hide_sketches();
    let mut out = built?;
    let made = std::mem::take(&mut cad.features);
    let auto_name = plan.folder_name();
    let name = slot.as_ref().and_then(Slot::kept_name).unwrap_or_else(|| auto_name.clone());
    let (folder, made) = edit::group(&mut cad, &made, &name, slot.as_ref().and_then(|s| s.parent))?;
    let mut notes: Vec<String> = Vec::new();
    match slot {
        Some(s) => s.close(&mut cad, made.len())?,
        None if replace.is_some() => notes.push("the gears to replace are gone (deleted or undone), so these are new ones".into()),
        None => {}
    }
    if store.writable {
        let record = json!({
            "entry": entry,
            "params": a.resolved(),
            "folder": name,
            "auto_name": auto_name,
            "bodies": out["bodies"],
            "features": made,
            "version": env!("CARGO_PKG_VERSION"),
        });
        edit::record(&mut cad, folder, record, replace)?;
    } else {
        notes.push("this design's Gear Kit data comes from a newer Gear Kit, so these gears can't be edited later".into());
    }
    if !notes.is_empty() {
        let mut w = out.get("warnings").and_then(Value::as_array).cloned().unwrap_or_default();
        w.extend(notes.into_iter().map(Value::from));
        out["warnings"] = Value::Array(w);
    }
    out["folder"] = json!({"id": folder, "name": name});
    Ok(out)
}

/// An entry's checked plan: everything about the gears, nothing built yet.
pub enum Plan {
    Spur(Box<SpurPlan>),
    Bevel(Box<BevelPlan>),
    Worm(Box<WormPlan>),
    Pulley(Box<PulleyPlan>),
}

impl Plan {
    pub fn new(entry: &str, a: &Args) -> R<Plan> {
        Ok(match entry {
            "spur-gears" => Plan::Spur(Box::new(spur_plan(a)?)),
            "bevel-gears" => Plan::Bevel(Box::new(bevel_plan(a)?)),
            "worm-gears" => Plan::Worm(Box::new(worm_plan(a)?)),
            "timing-pulleys" => Plan::Pulley(Box::new(pulley_plan(a)?)),
            _ => unreachable!("every building entry in schema::ENTRIES has a plan"),
        })
    }

    fn build(&self, cad: &mut Cad) -> R<Value> {
        match self {
            Plan::Spur(p) => spur_build(cad, p),
            Plan::Bevel(p) => bevel_build(cad, p),
            Plan::Worm(p) => worm_build(cad, p),
            Plan::Pulley(p) => pulley_build(cad, p),
        }
    }

    /// The timeline folder's name: the body name prefix, the tooth counts, then what was built:
    /// "Gear: 18:36 helical gears", or with one side turned off "Gear: 18 helical gear". The
    /// counts come first because the timeline's folder chip shows only about 19 characters.
    pub fn folder_name(&self) -> String {
        let what = match self {
            Plan::Spur(p) => {
                let teeth = if p.t1.beta == 0.0 {
                    "spur"
                } else if p.herringbone {
                    "herringbone"
                } else {
                    "helical"
                };
                let (z1, z2) = (p.t1.z, p.t2.z);
                match (p.kind, p.g1.is_some(), p.mate.is_some()) {
                    (PairType::External, true, true) => format!("{z1}:{z2} {teeth} gears"),
                    (PairType::Internal, true, true) => format!("{z1}:{z2} {teeth} ring gear pair"),
                    (PairType::Rack, true, true) => format!("{z1}:{z2} {teeth} gear and rack"),
                    (_, true, false) => format!("{z1} {teeth} gear"),
                    (PairType::External, _, _) => format!("{z2} {teeth} gear"),
                    (PairType::Internal, _, _) => format!("{z2} {teeth} ring gear"),
                    (PairType::Rack, _, _) => format!("{z2} {teeth} rack"),
                }
            }
            Plan::Bevel(p) => match p.create {
                (true, true) => format!("{}:{} bevel gears", p.g1.z, p.g2.z),
                (true, false) => format!("{} bevel gear", p.g1.z),
                _ => format!("{} bevel gear", p.g2.z),
            },
            Plan::Worm(p) => match (p.worm, p.wheel.is_some()) {
                (true, true) => format!("{}:{} worm drive", p.w.starts, p.z2),
                (true, false) => format!("{}-start worm", p.w.starts),
                _ => format!("{} worm wheel", p.z2),
            },
            Plan::Pulley(p) => {
                let t = p.profile.title;
                match (&p.p1, &p.p2) {
                    (Some(_), Some(_)) => format!("{}:{} {t} pulleys", p.z1, p.z2),
                    (Some(_), None) => format!("{} {t} pulley", p.z1),
                    _ => format!("{} {t} pulley", p.z2),
                }
            }
        };
        let name = match self {
            Plan::Spur(p) => &p.name,
            Plan::Bevel(p) => &p.name,
            Plan::Worm(p) => &p.w.name,
            Plan::Pulley(p) => &p.name,
        };
        format!("{name}: {what}")
    }
}

/// An input problem in the user's words: "Gear 2 teeth: a ring gear needs ...". SolidCraft
/// shows only the reason of a failed run, so the field's title goes in front.
pub fn describe(entry: &str, b: &Bad) -> String {
    match schema::entry(entry).and_then(|e| e.field(&b.field)) {
        // without a note in brackets, as in "Ring Ø (0=auto)"
        Some(f) => format!("{}: {}", f.title.split(" (").next().unwrap_or(f.title), b.reason),
        None => b.reason.clone(),
    }
}

/// A failed module check in the entry's terms: `names` maps the module's generic field names
/// (`teeth`, ...) to the entry's (`teeth_1`, ...); a name not listed is the entry's already.
fn renamed(b: Bad, names: &[(&str, &str)]) -> Error {
    let field = names.iter().find(|(from, _)| *from == b.field).map_or(b.field.clone(), |(_, to)| to.to_string());
    Error::Input(Bad { field, ..b })
}

/// Refuse a pair whose two parts together would take too long, reported on `field`.
fn check_time(est: f64, field: &str, what: &str) -> R<()> {
    if est > cad::TIME_BUDGET_S {
        return Err(bad(
            field,
            cad::too_slow(what, est, "turn off Create for one side and build the other in a second run (it is placed in mesh)"),
        )
        .short(cad::too_slow_short(est))
        .into());
    }
    Ok(())
}

fn nothing_to_build(one: &str, two: &str) -> Error {
    bad("create_1", format!("nothing to build: turn on {one}, {two} or both")).short("nothing to build").into()
}

/// Something worth knowing about a pair that still builds: a few words for the result line, the
/// full sentence for the reply and the field's tooltip, and the field it is about.
pub struct Warning {
    pub short: &'static str,
    pub text: String,
    pub field: &'static str,
}

fn warnings_json(w: &[Warning]) -> Value {
    json!(w.iter().map(|w| w.text.as_str()).collect::<Vec<_>>())
}

/// Turn the mate of an external pair so a tooth space faces gear 1's tooth 0 (on +X): the mate
/// meets gear 1 at its own azimuth π, where an even tooth count has a tooth.
fn mate_phase(z2: u32) -> f64 {
    if z2.is_multiple_of(2) {
        PI / z2 as f64
    } else {
        0.0
    }
}

// ---------------------------------------------------------------- spur and helical gears

pub enum Mate {
    Gear(CylSpec),
    Ring(CylSpec),
    Rack(RackSpec),
}

pub struct SpurPlan {
    pub kind: PairType,
    pub t1: Tooth,
    /// Gear 2's tooth (for a rack: its tooth count, otherwise unused).
    pub t2: Tooth,
    pub mesh: Mesh,
    pub width: f64,
    /// What gets built: gear 1 and its mate, each when its Create box is on.
    pub g1: Option<CylSpec>,
    pub mate: Option<Mate>,
    /// Ring gear outer diameter or rack height as built (after "0 = auto"); 0 if neither is built.
    pub size: f64,
    pub name: String,
    pub herringbone: bool,
    /// Estimated build time (s).
    pub est: f64,
}

/// A cylindrical gear's tooth from the spur entry's fields.
fn tooth(a: &Args, z: u32, x: f64, backlash: f64) -> R<Tooth> {
    Ok(Tooth {
        z,
        mn: a.num("module")?,
        alpha_n: a.num("pressure_angle")?.to_radians(),
        beta: a.num("helix_angle")?.to_radians(),
        x,
        ha: a.num("addendum")?,
        hf: a.num("dedendum")?,
        rho: a.num("root_fillet")?,
        backlash,
    })
}

/// A ring gear's outer diameter when the field is 0.
pub fn rim_default(t: &Tooth) -> f64 {
    2.0 * t.rf_int() + 6.0 * t.mn
}

/// A rack's height (back face to pitch line) when the field is 0.
pub fn rack_height_default(t: &Tooth) -> f64 {
    6.0 * t.mn
}

pub fn spur_plan(a: &Args) -> R<SpurPlan> {
    let kind = match a.choice("mate")? {
        "ring" => PairType::Internal,
        "rack" => PairType::Rack,
        _ => PairType::External,
    };
    let (c1, c2) = (a.flag("create_1")?, a.flag("create_2")?);
    if !c1 && !c2 {
        return Err(nothing_to_build("Create gear 1", "Create gear 2"));
    }
    let (z1, z2) = (a.int("teeth_1")?, a.int("teeth_2")?);
    let half = a.num("backlash")? / 2.0;
    let t1 = tooth(a, z1, a.num("profile_shift_1")?, half)?;
    let x2 = if kind == PairType::Rack { 0.0 } else { a.num("profile_shift_2")? };
    // the ring gear's own root fillet (its tooth spaces are not generated by a rack cutter)
    let rho2 = if kind == PairType::Internal { 0.25 } else { t1.rho };
    let t2 = Tooth { rho: rho2, ..tooth(a, z2, x2, half)? };
    let width = a.num("face_width")?;
    let hand = if a.choice("helix_hand")? == "left" { -1.0 } else { 1.0 };
    // the dialog hides Herringbone without a helix angle and for a rack: then it does not apply
    let herringbone = a.flag("herringbone")? && t1.beta > 0.0 && kind != PairType::Rack;
    let name = a.text("name")?;
    match kind {
        PairType::External if z2 < 4 => return Err(bad("teeth_2", "a gear needs at least 4 teeth").short("≥ 4 teeth").into()),
        PairType::Internal if z2 < 12 => {
            return Err(bad("teeth_2", "a ring gear needs at least 12 teeth").short("ring: ≥ 12 teeth").into())
        }
        PairType::Internal if z2 < z1 + 6 => {
            let short = format!("ring: ≥ {} teeth", z1 + 6);
            return Err(bad("teeth_2", "a ring gear needs at least 6 teeth more than gear 1").short(short).into());
        }
        _ => {}
    }
    let mesh = pair::mesh(&t1, &t2, kind).map_err(|e| bad("profile_shift_1", e))?;
    if kind == PairType::External {
        let c = pair::tip_clearance(&t1, &t2, &mesh);
        if c < 0.0 {
            let reason = format!("the tips would reach {:.3} mm into the other gear's root: lower the addendum or raise the dedendum", -c);
            return Err(bad("profile_shift_1", reason).short("tips hit the roots").into());
        }
    }
    let gear = |t: Tooth, name: String, n: u32| -> R<CylSpec> {
        let mut s = CylSpec::new(t, width, &name);
        s.hand = hand;
        s.herringbone = herringbone;
        let teeth = format!("teeth_{n}");
        cyl::check(&s).map_err(|b| renamed(b, &[("teeth", &teeth)]))?;
        Ok(s)
    };
    let mut est = 0.0;
    let mut size = 0.0;
    let g1 = if c1 {
        let mut s = gear(t1, format!("{name} 1"), 1)?;
        if kind == PairType::Rack {
            s.phase = -PI / 2.0; // tooth 0 points down, into the rack's middle space
        }
        est += cyl::est_seconds(&s);
        Some(s)
    } else {
        None
    };
    let mate = if c2 {
        Some(match kind {
            PairType::External => {
                // parallel axes: the opposite hand
                let mut s = CylSpec { hand: -hand, ..gear(t2, format!("{name} 2"), 2)? };
                s.phase = mate_phase(z2);
                est += cyl::est_seconds(&s);
                Mate::Gear(s)
            }
            PairType::Internal => {
                let rim = a.num("rim_diameter")?;
                let mut s = CylSpec::new(t2, width, &format!("{name} ring"));
                s.internal = true;
                s.rim_d = if rim > 0.0 { rim } else { rim_default(&t2) };
                s.hand = hand; // inside a ring gear: the same hand
                s.herringbone = herringbone;
                s.phase = PI / z2 as f64; // a tooth space on +X, where gear 1's tooth 0 points
                cyl::check(&s).map_err(|b| renamed(b, &[("teeth", "teeth_2")]))?;
                size = s.rim_d;
                est += cyl::est_seconds(&s);
                Mate::Ring(s)
            }
            PairType::Rack => {
                let h = a.num("rack_height")?;
                size = if h > 0.0 { h } else { rack_height_default(&t1) };
                let s = RackSpec {
                    teeth: z2,
                    mn: t1.mn,
                    alpha_n: t1.alpha_n,
                    beta: t1.beta,
                    hand,
                    width,
                    height: size,
                    ha: t1.ha,
                    hf: t1.hf,
                    rho: t1.rho,
                    backlash: half,
                    name: format!("{name} rack"),
                };
                rack::check(&s).map_err(|b| renamed(b, &[("height", "rack_height"), ("teeth", "teeth_2")]))?;
                est += rack::est_seconds(&s);
                Mate::Rack(s)
            }
        })
    } else {
        None
    };
    check_time(est, "teeth_2", "both gears together")?;
    Ok(SpurPlan { kind, t1, t2, mesh, width, g1, mate, size, name, herringbone, est })
}

/// What the result line warns about for a spur, helical or herringbone pair.
pub fn spur_warnings(p: &SpurPlan) -> Vec<Warning> {
    let (t1, t2, m) = (&p.t1, &p.t2, &p.mesh);
    let mut w = Vec::new();
    if m.contact_ratio < 1.1 {
        let text = "contact ratio below 1.1: the teeth may lose contact; use more teeth or less profile shift".into();
        w.push(Warning { short: "εα < 1.1", text, field: "teeth_1" });
    }
    let undercut = |t: &Tooth| t.zf() < pair::min_teeth(t) - 1e-9;
    let undercut_text = |t: &Tooth, n: u32| format!("gear {n} undercut: below {:.1} teeth at this profile shift", pair::min_teeth(t));
    if undercut(t1) {
        w.push(Warning { short: "undercut 1", text: undercut_text(t1, 1), field: "teeth_1" });
    }
    if p.kind == PairType::External && undercut(t2) {
        w.push(Warning { short: "undercut 2", text: undercut_text(t2, 2), field: "teeth_2" });
    }
    if p.kind == PairType::External {
        // a negative clearance is refused by the plan
        let c = pair::tip_clearance(t1, t2, m);
        if c < 0.1 * t1.mn {
            let text = format!("tip clearance only {c:.3} mm: lower the addendum or raise the dedendum");
            w.push(Warning { short: "tip clearance", text, field: "profile_shift_1" });
        }
    }
    if p.kind == PairType::Internal && t2.z < t1.z + 8 {
        let text = "fewer than 8 teeth more than gear 1: check the ring gear for tip interference".into();
        w.push(Warning { short: "interference?", text, field: "teeth_2" });
    }
    w
}

fn spur_build(cad: &mut Cad, p: &SpurPlan) -> R<Value> {
    let mut out = pair::pair_numbers(&p.t1, &p.t2, p.kind, &p.mesh, p.width);
    let warnings = spur_warnings(p);
    if !warnings.is_empty() {
        out["warnings"] = warnings_json(&warnings);
    }
    let mut bodies = Vec::new();
    let aw = p.mesh.aw;
    let up = [0.0, 0.0, 1.0];
    // the ring gear is the one on the Z axis of an internal pair; gear 1 sits inside it at +X
    if let Some(Mate::Ring(s)) = &p.mate {
        bodies.push(cyl::build(cad, s)?.body);
    }
    if let Some(s) = &p.g1 {
        let b = cyl::build(cad, s)?;
        if p.kind == PairType::Internal {
            cad.mov(&b.body, up, 0.0, [0.0; 3], [aw, 0.0, 0.0], &format!("{} placement", s.name))?;
        }
        bodies.insert(0, b.body);
    }
    match &p.mate {
        Some(Mate::Gear(s)) => {
            let b = cyl::build(cad, s)?;
            cad.mov(&b.body, up, 0.0, [0.0; 3], [aw, 0.0, 0.0], &format!("{} placement", s.name))?;
            bodies.push(b.body);
        }
        Some(Mate::Rack(s)) => {
            let b = rack::build(cad, s)?;
            // below gear 1, a tooth space centred under its axis
            let shift = -((s.teeth / 2) as f64) * s.pitch();
            cad.mov(&b.body, up, 0.0, [0.0; 3], [shift, -aw, 0.0], &format!("{} placement", s.name))?;
            bodies.push(b.body);
            out["rack"] = b.info;
        }
        _ => {}
    }
    match p.kind {
        PairType::External => out["gear_2_axis"] = json!([r9(aw), 0.0, 0.0]),
        PairType::Internal => out["gear_1_axis"] = json!([r9(aw), 0.0, 0.0]),
        PairType::Rack => out["rack_pitch_line_y"] = json!(r9(-aw)),
    }
    if p.size > 0.0 {
        out[if p.kind == PairType::Rack { "rack_height" } else { "rim_diameter" }] = json!(r9(p.size));
    }
    out["bodies"] = json!(bodies);
    Ok(out)
}

// ---------------------------------------------------------------- bevel gears

pub struct BevelPlan {
    pub g1: BevelGear,
    pub g2: BevelGear,
    pub shaft: f64,
    /// Create gear 1, create gear 2.
    pub create: (bool, bool),
    pub name: String,
}

pub fn bevel_plan(a: &Args) -> R<BevelPlan> {
    let create = (a.flag("create_1")?, a.flag("create_2")?);
    if !create.0 && !create.1 {
        return Err(nothing_to_build("Create gear 1", "Create gear 2"));
    }
    let (z1, z2) = (a.int("teeth_1")?, a.int("teeth_2")?);
    let shaft = a.num("shaft_angle")?.to_radians();
    let m = a.num("module")?;
    let (d1, d2) = bevel::cone_angles(z1, z2, shaft);
    let re = m * z1 as f64 / (2.0 * d1.sin());
    let x = a.num("profile_shift")?;
    let backlash = a.num("backlash")? / 2.0;
    let (alpha, width) = (a.num("pressure_angle")?.to_radians(), a.num("face_width")?);
    let gear = |z: u32, delta: f64, x: f64| BevelGear { z, delta, re, m, alpha, x, ha: 1.0, hf: 1.25, backlash, width };
    let (g1, g2) = (gear(z1, d1, x), gear(z2, d2, -x));
    let mut est = 0.0;
    for (g, n, on) in [(&g1, 1, create.0), (&g2, 2, create.1)] {
        if !on {
            continue;
        }
        bevel::check(g).map_err(|b| {
            let Bad { field, reason, short } = b;
            Error::Input(match field.as_str() {
                "teeth" => Bad { field: format!("teeth_{n}"), reason, short },
                // one field for both gears: say which
                "profile_shift" => Bad { field, reason: format!("gear {n}: {reason}"), short: short.map(|s| format!("x{n}: {s}")) },
                _ => Bad { field, reason, short },
            })
        })?;
        est += bevel::est_seconds(g);
    }
    check_time(est, "teeth_2", "both gears together")?;
    Ok(BevelPlan { g1, g2, shaft, create, name: a.text("name")? })
}

pub fn bevel_numbers(p: &BevelPlan) -> Value {
    json!({
        "ratio": r9(p.g2.z as f64 / p.g1.z as f64),
        "shaft_angle": r9(p.shaft.to_degrees()),
        "cone_distance_outer": r9(p.g1.re),
        "pitch_cone_angle_1": r9(p.g1.delta.to_degrees()),
        "pitch_cone_angle_2": r9(p.g2.delta.to_degrees()),
    })
}

fn bevel_build(cad: &mut Cad, p: &BevelPlan) -> R<Value> {
    let mut out = bevel_numbers(p);
    let mut bodies = Vec::new();
    let name = &p.name;
    if p.create.0 {
        let b = bevel::build(cad, &p.g1, &format!("{name} 1"))?;
        bodies.push(b.body);
        out["gear_1"] = b.info;
    }
    if p.create.1 {
        let b = bevel::build(cad, &p.g2, &format!("{name} 2"))?;
        let ph = mate_phase(p.g2.z);
        if ph != 0.0 {
            cad.mov(&b.body, [0.0, 0.0, 1.0], ph.to_degrees(), [0.0; 3], [0.0; 3], &format!("{name} 2 phase"))?;
        }
        // tilt by the shaft angle about its apex, then bring the apexes together
        let (a1, a2) = (p.g1.apex(), p.g2.apex());
        cad.mov(&b.body, [0.0, 1.0, 0.0], -p.shaft.to_degrees(), [0.0, 0.0, a2], [0.0, 0.0, a1 - a2], &format!("{name} 2 placement"))?;
        bodies.push(b.body);
        out["gear_2"] = b.info;
    }
    out["apex"] = json!([0.0, 0.0, r9(p.g1.apex())]);
    out["bodies"] = json!(bodies);
    Ok(out)
}

// ---------------------------------------------------------------- worm gears

pub struct WormPlan {
    pub w: WormSpec,
    pub z2: u32,
    /// Centre distance.
    pub a: f64,
    /// Create the worm.
    pub worm: bool,
    /// The wheel, when it is created.
    pub wheel: Option<CylSpec>,
    pub wheel_tooth: Tooth,
    pub face: f64,
}

pub fn worm_plan(a: &Args) -> R<WormPlan> {
    let (worm, wheel) = (a.flag("create_1")?, a.flag("create_2")?);
    if !worm && !wheel {
        return Err(nothing_to_build("Create worm", "Create worm wheel"));
    }
    let half = a.num("backlash")? / 2.0;
    let w = WormSpec {
        m: a.num("module")?,
        starts: a.int("starts")?,
        d1: a.num("worm_diameter")?,
        length: a.num("worm_length")?,
        alpha: a.num("pressure_angle")?.to_radians(),
        hand: if a.choice("helix_hand")? == "left" { -1.0 } else { 1.0 },
        backlash: half,
        name: a.text("name")?,
    };
    worm::check(&w)?;
    let z2 = a.int("wheel_teeth")?;
    let face = a.num("wheel_face_width")?;
    let t = worm::wheel_tooth(&w, z2, half);
    let centre = (w.d1 + w.m * z2 as f64) / 2.0;
    let mut est = if worm { worm::est_seconds(&w) } else { 0.0 };
    let wheel = if wheel {
        let mut s = CylSpec::new(t, face, &format!("{} wheel", w.name));
        s.hand = w.hand;
        s.phase_z = face / 2.0; // tooth 0 on +X in the worm's plane
        s.blank_r = Some(worm::wheel_blank_radius(&t, w.m));
        s.throat = Some((centre, centre - t.ra())); // throat radius: the worm's root plus clearance
        let names = [("teeth", "wheel_teeth"), ("face_width", "wheel_face_width"), ("helix_angle", "wheel_face_width")];
        cyl::check(&s).map_err(|b| renamed(b, &names))?;
        est += cyl::est_seconds(&s);
        Some(s)
    } else {
        None
    };
    check_time(est, "wheel_teeth", "worm and wheel together")?;
    Ok(WormPlan { w, z2, a: centre, worm, wheel, wheel_tooth: t, face })
}

/// What the result line warns about for a worm drive.
pub fn worm_warnings(p: &WormPlan) -> Vec<Warning> {
    let mut w = Vec::new();
    if p.w.lead_angle().to_degrees() < 5.0 {
        let text = "likely self-locking: with a lead angle below 5° the wheel can hardly drive the worm (it depends on friction)".into();
        w.push(Warning { short: "self-locking", text, field: "worm_diameter" });
    }
    w
}

pub fn worm_numbers(p: &WormPlan) -> Value {
    json!({
        "ratio": r9(p.z2 as f64 / p.w.starts as f64),
        "centre_distance": r9(p.a),
        "lead_angle": r9(p.w.lead_angle().to_degrees()),
        "wheel_pitch_diameter": r9(p.w.m * p.z2 as f64),
    })
}

/// The wheel's outside diameter (of its blank).
pub fn wheel_outside_diameter(p: &WormPlan) -> f64 {
    2.0 * worm::wheel_blank_radius(&p.wheel_tooth, p.w.m)
}

fn worm_build(cad: &mut Cad, p: &WormPlan) -> R<Value> {
    let mut out = worm_numbers(p);
    let warnings = worm_warnings(p);
    if !warnings.is_empty() {
        out["warnings"] = warnings_json(&warnings);
    }
    let mut bodies = Vec::new();
    if let Some(s) = &p.wheel {
        let b = cyl::build(cad, s)?;
        bodies.push(b.body);
        let mut info = b.info;
        info["throat_radius"] = json!(r9(p.a - p.wheel_tooth.ra()));
        info["outside_diameter"] = json!(r9(wheel_outside_diameter(p)));
        out["wheel"] = info;
    }
    if p.worm {
        let b = worm::build_worm(cad, &p.w)?;
        // from the Z axis to along Y, centred on y = 0, in the wheel's mid plane at +X
        let mid = [p.a, 0.0, p.face / 2.0];
        cad.mov(
            &b.body,
            [1.0, 0.0, 0.0],
            -90.0,
            [0.0; 3],
            [p.a, -p.w.length / 2.0, p.face / 2.0],
            &format!("{} worm placement", p.w.name),
        )?;
        out["worm_axis"] = json!({"through": mid.map(r9), "direction": [0.0, 1.0, 0.0]});
        bodies.push(b.body);
        out["worm"] = b.info;
    }
    out["bodies"] = json!(bodies);
    Ok(out)
}

// ---------------------------------------------------------------- timing belt pulleys

pub struct PulleyPlan {
    pub profile: &'static Profile,
    pub z1: u32,
    pub z2: u32,
    pub belt: u32,
    /// Centre distance; none when the belt is too short and only pulley 1 is built (it does not
    /// need one).
    pub c: Option<f64>,
    /// What gets built: each pulley when its Create box is on.
    pub p1: Option<PulleySpec>,
    pub p2: Option<PulleySpec>,
    /// Flange height as built (after "0 = auto"), 0 without flanges.
    pub flange_h: f64,
    pub name: String,
    pub est: f64,
}

impl PulleyPlan {
    pub fn pitch_radius(&self, n: u32) -> f64 {
        self.profile.pitch_diameter(if n == 1 { self.z1 } else { self.z2 }) / 2.0
    }
    /// The angle the belt wraps round pulley `n` (radians).
    pub fn wrap(&self, n: u32, c: f64) -> f64 {
        let (r1, r2) = (self.pitch_radius(1), self.pitch_radius(2));
        if n == 1 {
            pulley::wrap(r1, r2, c)
        } else {
            pulley::wrap(r2, r1, c)
        }
    }
    /// Belt teeth in the grooves of pulley `n`.
    pub fn in_mesh(&self, n: u32, c: f64) -> f64 {
        let z = if n == 1 { self.z1 } else { self.z2 };
        z as f64 * self.wrap(n, c) / (2.0 * PI)
    }
    /// The belt's pitch length.
    pub fn belt_length(&self) -> f64 {
        self.belt as f64 * self.profile.pitch
    }
    /// Pulley `n`'s outermost radius, flanges included.
    pub fn r_max(&self, n: u32) -> f64 {
        self.profile.outside_diameter(if n == 1 { self.z1 } else { self.z2 }) / 2.0 + self.flange_h
    }
    /// The fewest belt teeth that keep the pulleys apart.
    pub fn min_belt_teeth(&self) -> u32 {
        let c_min = self.r_max(1) + self.r_max(2);
        (pulley::belt_length(self.pitch_radius(1), self.pitch_radius(2), c_min) / self.profile.pitch).floor() as u32 + 1
    }
}

pub fn pulley_plan(a: &Args) -> R<PulleyPlan> {
    let profile = pulley::profile(a.choice("profile")?).expect("every BELTS value is a profile");
    let (c1, c2) = (a.flag("create_1")?, a.flag("create_2")?);
    if !c1 && !c2 {
        return Err(nothing_to_build("Create pulley 1", "Create pulley 2"));
    }
    let (z1, z2, belt) = (a.int("teeth_1")?, a.int("teeth_2")?, a.int("belt_teeth")?);
    let flanges = match a.choice("flanges")? {
        "none" => 0,
        "one" => 1,
        _ => 2,
    };
    let h = a.num("flange_height")?;
    let flange_h = match flanges {
        0 => 0.0,
        _ if h > 0.0 => h,
        _ => profile.flange_h,
    };
    let name = a.text("name")?;
    let spec = |z: u32, n: u32| -> R<PulleySpec> {
        let s = PulleySpec {
            profile,
            z,
            width: a.num("face_width")?,
            clearance: a.num("groove_clearance")?,
            flanges,
            flange_d: profile.outside_diameter(z) + 2.0 * flange_h,
            flange_t: a.num("flange_thickness")?,
            name: format!("{name} {n}"),
        };
        let teeth = format!("teeth_{n}");
        pulley::check(&s).map_err(|b| renamed(b, &[("teeth", &teeth)]))?;
        Ok(s)
    };
    let p1 = if c1 { Some(spec(z1, 1)?) } else { None };
    let p2 = if c2 { Some(spec(z2, 2)?) } else { None };
    let mut plan = PulleyPlan { profile, z1, z2, belt, c: None, p1, p2, flange_h, name, est: 0.0 };
    // the centre distance places pulley 2; the pulleys, flanges included, must not touch
    let (r1, r2) = (plan.pitch_radius(1), plan.pitch_radius(2));
    let c_min = plan.r_max(1) + plan.r_max(2);
    plan.c = pulley::centre_distance(r1, r2, plan.belt_length()).filter(|&c| c > c_min);
    if c2 && plan.c.is_none() {
        let need = plan.min_belt_teeth();
        let reason = format!("the belt is too short for these pulleys: it needs at least {need} teeth");
        return Err(bad("belt_teeth", reason).short(format!("belt: ≥ {need} teeth")).into());
    }
    plan.est = plan.p1.iter().chain(plan.p2.iter()).map(pulley::est_seconds).sum();
    check_time(plan.est, "teeth_2", "both pulleys together")?;
    Ok(plan)
}

/// What the result line warns about for a belt drive.
pub fn pulley_warnings(p: &PulleyPlan) -> Vec<Warning> {
    let mut w = Vec::new();
    let Some(c) = p.c else { return w };
    let small = if p.z1 <= p.z2 { 1 } else { 2 };
    let m = p.in_mesh(small, c);
    if m < 6.0 {
        let text = format!("only {m:.1} belt teeth in pulley {small}'s grooves: belt makers ask for 6 or more; use a larger pulley {small} or a longer belt");
        w.push(Warning { short: "< 6 in mesh", text, field: if small == 1 { "teeth_1" } else { "teeth_2" } });
    }
    w
}

pub fn pulley_numbers(p: &PulleyPlan) -> Value {
    let mut out = json!({
        "profile": p.profile.title,
        "ratio": r9(p.z2 as f64 / p.z1 as f64),
        "belt_pitch_length": r9(p.belt_length()),
        "belt_teeth": p.belt,
    });
    if let Some(c) = p.c {
        out["centre_distance"] = json!(r9(c));
        out["wrap_angle_1"] = json!(r9(p.wrap(1, c).to_degrees()));
        out["wrap_angle_2"] = json!(r9(p.wrap(2, c).to_degrees()));
    }
    out
}

fn pulley_build(cad: &mut Cad, p: &PulleyPlan) -> R<Value> {
    let mut out = pulley_numbers(p);
    let warnings = pulley_warnings(p);
    if !warnings.is_empty() {
        out["warnings"] = warnings_json(&warnings);
    }
    let mut bodies = Vec::new();
    if let Some(s) = &p.p1 {
        let b = pulley::build(cad, s)?;
        bodies.push(b.body);
        out["pulley_1"] = b.info;
    }
    if let (Some(s), Some(c)) = (&p.p2, p.c) {
        let b = pulley::build(cad, s)?;
        cad.mov(&b.body, [0.0, 0.0, 1.0], 0.0, [0.0; 3], [c, 0.0, 0.0], &format!("{} placement", s.name))?;
        bodies.push(b.body);
        out["pulley_2"] = b.info;
    }
    if let Some(c) = p.c {
        out["pulley_2_axis"] = json!([r9(c), 0.0, 0.0]);
    }
    out["bodies"] = json!(bodies);
    Ok(out)
}
