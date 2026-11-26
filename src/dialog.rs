//! The dialog hook (`dialog@0.1`): while the user edits a dialog, SolidCraft sends the current
//! values here and draws the reply. The reply fills the read-only result line (centre distance,
//! ratio, contact ratio, warnings, or what is wrong with the inputs) and puts each gear's
//! dimensions into the tooltips of its fields. It runs the same plan as the entry, so what the
//! dialog says is what Run builds, and it never touches the design.
//!
//! The result line is a narrow single-line field, so it is kept short: warnings first, then the key
//! numbers without trailing zeros (`a 54 mm · i 2 · εα 1.61`). SolidCraft shows no tooltip on a
//! read-only field, so each warning is spelled out in the tooltip of the field it is about (an
//! undercut on the teeth of that gear), ahead of that field's dimensions.
//!
//! The host allows at most 1024 bytes and no line breaks per hint; the result line is a text
//! field of at most `INFO_MAX` bytes. The hook runs on a budget of its own, at most 10⁷ fuel,
//! 16 MiB and 250 ms, and SolidCraft also calls it once before every run: a plan whose checks grow
//! too expensive stops the entry itself (see the README).

use crate::bevel::BevelGear;
use crate::entries::{self, BevelPlan, Error, Mate, PulleyPlan, SpurPlan, Warning, WormPlan};
use crate::involute::Tooth;
use crate::pair::{self, PairType};
use crate::schema::{self, Args, INFO_MAX, RESULT_HELP, SUMMARY};
use serde_json::{json, Map, Value};
use std::f64::consts::PI;

/// Longest hint the host takes, in bytes.
const HINT_MAX: usize = 1024;

/// Characters the result row shows at SolidCraft's default dialog width (measured on 0.49): the
/// line takes whole parts up to this, so no number is cut in half. Everything is also in a
/// field's tooltip.
const LINE_CHARS: usize = 26;

/// The reply for `entry` with the dialog's current `values`: `{field: {hint | value | visible}}`.
pub fn update_fields(entry: &str, values: Value) -> Value {
    // `replace` shows only in a dialog Edit Gears filled in: hidden, it is left out of the run
    let replacing = values.get(schema::REPLACE).and_then(Value::as_u64).filter(|&id| id > 0);
    let mut reply = fields(entry, values);
    if let (Some(e), Value::Object(m)) = (schema::entry(entry), &mut reply) {
        if e.field(schema::REPLACE).is_some() {
            let patch = match replacing {
                Some(id) => json!({"hint": format!("These gears replace the ones in timeline folder {id}; set by Edit Gears.")}),
                None => json!({"visible": false}),
            };
            m.insert(schema::REPLACE.into(), patch);
        }
    }
    reply
}

fn fields(entry: &str, values: Value) -> Value {
    let Some(e) = schema::entry(entry) else { return json!({}) };
    // keep only keys the entry knows (the host sends just those, but be lenient)
    let values = match values {
        Value::Object(m) => Value::Object(m.into_iter().filter(|(k, _)| e.field(k).is_some()).collect()),
        _ => json!({}),
    };
    let a = match Args::new(e, values) {
        Ok(a) => a,
        Err(b) => return problem(entry, &b),
    };
    let reply = match entry {
        "spur-gears" => entries::spur_plan(&a).map(|p| spur(&p)),
        "bevel-gears" => entries::bevel_plan(&a).map(|p| bevel(&p)),
        "worm-gears" => entries::worm_plan(&a).map(|p| worm(&p)),
        "timing-pulleys" => entries::pulley_plan(&a).map(|p| pulley(&p)),
        _ => return json!({}),
    };
    match reply {
        Ok(r) => r,
        Err(Error::Input(b)) => problem(entry, &b),
        Err(Error::Failed(msg)) => summary_only(&format!("⚠ {msg}")),
    }
}

/// The reply for an input that cannot be built. The result line is narrow (about 24 characters
/// show), so it says the problem in a few words, naming the gear where a pair has two
/// ("⚠ z2: cone 80.5° > 72°"); the field it is about gets the whole sentence as its tooltip,
/// which SolidCraft shows on that field's label (it shows none on the read-only result line).
pub fn problem(entry: &str, b: &schema::Bad) -> Value {
    let full = format!("⚠ {}", entries::describe(entry, b));
    let line = match &b.short {
        Some(short) => format!("⚠ {}{short}", gear_tag(&b.field)),
        None => full.clone(),
    };
    let mut reply = json!({ SUMMARY: {"value": clip(&line, INFO_MAX), "hint": clip(&full, HINT_MAX)} });
    if b.field != SUMMARY && schema::entry(entry).is_some_and(|e| e.field(&b.field).is_some()) {
        reply[b.field.as_str()] = json!({"hint": clip(&format!("⚠ {}", b.reason), HINT_MAX)});
    }
    reply
}

/// Which gear of a pair a field is about, for a short problem: "z2: ", "x1: ".
fn gear_tag(field: &str) -> &'static str {
    match field {
        "teeth_1" => "z1: ",
        "teeth_2" | "wheel_teeth" => "z2: ",
        "profile_shift_1" => "x1: ",
        "profile_shift_2" => "x2: ",
        _ => "",
    }
}

/// A reply that only sets the result line, for inputs that cannot be built.
pub fn summary_only(text: &str) -> Value {
    json!({ SUMMARY: {"value": clip(text, INFO_MAX), "hint": clip(text, HINT_MAX)} })
}

/// A number for the result line: at most `dec` decimals, no trailing zeros.
fn n(x: f64, dec: usize) -> String {
    let s = format!("{x:.dec$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

/// A length for a tooltip.
fn mm(x: f64) -> String {
    format!("{x:.3}")
}

/// At most `max` bytes, cut at a character boundary, with an ellipsis when cut; no control
/// characters.
fn clip(s: &str, max: usize) -> String {
    let s: String = s.chars().filter(|c| !c.is_control()).collect();
    if s.len() <= max {
        return s;
    }
    let mut end = max - '…'.len_utf8();
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

/// A reply being put together.
struct Reply(Map<String, Value>);

impl Reply {
    /// The result line: warnings first, then `short`. Its own tooltip (`long`, the warnings and
    /// a pointer to the other tooltips) is there should SolidCraft show tooltips on read-only
    /// fields one day.
    fn new(warnings: &[Warning], short: &[String], long: &[String]) -> Reply {
        let parts: Vec<String> = warnings.iter().map(|w| format!("⚠ {}", w.short)).chain(short.iter().cloned()).collect();
        let mut line: Vec<String> = Vec::new();
        for p in parts {
            let len = line.iter().chain(std::iter::once(&p)).map(|s| s.chars().count()).sum::<usize>() + 3 * line.len();
            if line.is_empty() || len <= LINE_CHARS {
                line.push(p);
            } else {
                break;
            }
        }
        let mut full: Vec<String> = long.to_vec();
        full.extend(warnings.iter().map(|w| format!("⚠ {}", w.text)));
        full.push(RESULT_HELP.into());
        let mut m = Map::new();
        m.insert(SUMMARY.into(), json!({"value": clip(&line.join(" · "), INFO_MAX), "hint": clip(&full.join(" · "), HINT_MAX)}));
        Reply(m)
    }

    fn hint(&mut self, key: &str, text: String) {
        self.0.insert(key.into(), json!({"hint": clip(&text, HINT_MAX)}));
    }

    /// Put each warning ahead of the tooltip of the field it is about. Call after the hints.
    fn warnings(mut self, warnings: &[Warning]) -> Value {
        for w in warnings {
            let old = self.0.get(w.field).and_then(|f| f["hint"].as_str()).map(str::to_owned);
            let text = match old {
                Some(old) => format!("⚠ {} · {old}", w.text),
                None => format!("⚠ {}", w.text),
            };
            self.hint(w.field, text);
        }
        Value::Object(self.0)
    }
}

/// Where undercut starts, for a tooltip.
fn undercut_limit(t: &Tooth) -> String {
    let z = pair::min_teeth(t);
    if z < 4.0 {
        "no undercut at any tooth count".into()
    } else {
        format!("no undercut from {z:.1} teeth up")
    }
}

/// The dimensions of an external gear, for a tooltip.
fn external(t: &Tooth) -> String {
    let (k, w) = pair::tooth_span(t);
    format!(
        "d = {} · da = {} · df = {} · db = {} mm · tooth thickness s = {} mm · span over k = {k} teeth Wk = {} mm",
        mm(2.0 * t.rp()),
        mm(2.0 * t.ra()),
        mm(2.0 * t.rf()),
        mm(2.0 * t.rb()),
        mm(t.st()),
        mm(w)
    )
}

/// The dimensions of a ring gear, for a tooltip.
fn internal(t: &Tooth) -> String {
    format!(
        "d = {} · da = {} · df = {} mm (internal) · space width on d = {} mm",
        mm(2.0 * t.rp()),
        mm(2.0 * t.ra_int().max(t.rb())),
        mm(2.0 * t.rf_int()),
        mm(t.et_int())
    )
}

fn spur(p: &SpurPlan) -> Value {
    let (t1, t2, m) = (&p.t1, &p.t2, &p.mesh);
    let (mut short, mut long) = (Vec::new(), Vec::new());
    match p.kind {
        PairType::Rack => {
            let turn = 2.0 * PI * t1.rp();
            short.push(format!("{} mm/turn", n(turn, 2)));
            short.push(format!("pitch line {} mm", n(m.aw, 2)));
            long.push(format!("pitch line {} mm from the gear axis", mm(m.aw)));
            long.push(format!("one turn of gear 1 moves the rack {} mm", mm(turn)));
        }
        _ => {
            let i = t2.zf() / t1.zf();
            short.push(format!("a {} mm", n(m.aw, 2)));
            short.push(format!("i {}", n(i, 3)));
            long.push(format!("centre distance a = {} mm", mm(m.aw)));
            long.push(format!("ratio i = {i:.3}"));
        }
    }
    short.push(format!("εα {:.2}", m.contact_ratio));
    long.push(format!("transverse contact ratio εα = {:.3}", m.contact_ratio));
    if t1.beta > 0.0 {
        let eb = pair::overlap_ratio(t1, p.width);
        short.push(format!("εβ {eb:.2}"));
        long.push(format!("overlap ratio εβ = {eb:.3}"));
    }
    if p.est > 8.0 {
        short.push(format!("~{:.0} s", p.est));
        long.push(format!("builds in about {:.0} s", p.est));
    }
    let warnings = entries::spur_warnings(p);
    let mut r = Reply::new(&warnings, &short, &long);
    r.hint("teeth_1", format!("Gear 1: {}", external(t1)));
    let gear2 = match p.kind {
        PairType::External => format!("Gear 2: {}", external(t2)),
        PairType::Internal => format!("Ring gear: {}", internal(t2)),
        PairType::Rack => format!("Rack: pitch p = {} mm · length {} mm", mm(PI * t1.mt()), mm(t2.zf() * PI * t1.mt())),
    };
    // the result row's own tooltip never shows: the pair's numbers go with gear 2
    r.hint("teeth_2", format!("{gear2} · Pair: {}", long.join(" · ")));
    let follows =
        if p.kind == PairType::Rack { "the pitch line follows the shift" } else { "the centre distance follows the sum of both shifts" };
    r.hint("profile_shift_1", format!("x1 = {:.3}: {} · {follows}", t1.x, undercut_limit(t1)));
    match p.kind {
        PairType::External => r.hint(
            "profile_shift_2",
            format!(
                "x2 = {:.3}: {} · reference centre distance a0 = {} mm, working a = {} mm, αw = {:.3}°",
                t2.x,
                undercut_limit(t2),
                mm(m.a),
                mm(m.aw),
                m.alpha_w.to_degrees()
            ),
        ),
        PairType::Internal => r.hint(
            "profile_shift_2",
            format!(
                "x2 = {:.3}, positive outward (wider spaces) · reference centre distance a0 = {} mm, working a = {} mm, αw = {:.3}°",
                t2.x,
                mm(m.a),
                mm(m.aw),
                m.alpha_w.to_degrees()
            ),
        ),
        PairType::Rack => {}
    }
    if t1.beta > 0.0 {
        r.hint(
            "helix_angle",
            format!(
                "Transverse module mt = {} mm · transverse pressure angle αt = {:.3}° · lead of gear 1 = {} mm",
                mm(t1.mt()),
                t1.alpha_t().to_degrees(),
                mm(2.0 * PI * t1.rp() / t1.beta.tan())
            ),
        );
    }
    match &p.mate {
        Some(Mate::Ring(_)) => r.hint(
            "rim_diameter",
            format!("Outer diameter of the ring; 0 = auto: {} mm (root diameter + 6 modules)", mm(entries::rim_default(t2))),
        ),
        Some(Mate::Rack(s)) => r.hint(
            "rack_height",
            format!("Back face to pitch line; 0 = auto: {} mm (6 modules) · now {} mm", mm(entries::rack_height_default(t1)), mm(s.height)),
        ),
        _ => {}
    }
    r.warnings(&warnings)
}

fn bevel(p: &BevelPlan) -> Value {
    let (g1, g2) = (&p.g1, &p.g2);
    let i = g2.z as f64 / g1.z as f64;
    let (d1, d2) = (g1.delta.to_degrees(), g2.delta.to_degrees());
    let short = [format!("i {}", n(i, 3)), format!("δ {}°/{}°", n(d1, 1), n(d2, 1)), format!("Re {} mm", n(g1.re, 1))];
    let long = [
        format!("ratio i = {i:.3}"),
        format!("pitch cone angles δ1 = {d1:.3}°, δ2 = {d2:.3}°"),
        format!("outer cone distance Re = {} mm", mm(g1.re)),
    ];
    let gear = |g: &BevelGear, n: u32| {
        format!(
            "Gear {n} (large end): d = {} · da = {} mm · pitch cone δ = {:.3}° · tip cone {:.3}° · root cone {:.3}°",
            mm(2.0 * g.rp()),
            mm(g.tip_diameter()),
            g.delta.to_degrees(),
            g.delta_a().to_degrees(),
            g.delta_f().to_degrees()
        )
    };
    let mut r = Reply::new(&[], &short, &long);
    r.hint("teeth_1", gear(g1, 1));
    r.hint("teeth_2", format!("{} · Pair: {}", gear(g2, 2), long.join(" · ")));
    r.hint("face_width", format!("At most 0.42 × the outer cone distance: {} mm (Re = {} mm)", mm(0.42 * g1.re), mm(g1.re)));
    r.warnings(&[])
}

fn worm(p: &WormPlan) -> Value {
    let w = &p.w;
    let lead = w.lead_angle().to_degrees();
    let i = p.z2 as f64 / w.starts as f64;
    let short = [format!("a {} mm", n(p.a, 2)), format!("i {}", n(i, 2)), format!("γ {}°", n(lead, 2))];
    let long = [format!("centre distance a = {} mm", mm(p.a)), format!("ratio i = {i:.2}"), format!("lead angle γ = {lead:.3}°")];
    let warnings = entries::worm_warnings(p);
    let mut r = Reply::new(&warnings, &short, &long);
    r.hint(
        "worm_diameter",
        format!(
            "Worm: d1 = {} · da1 = {} · df1 = {} mm · axial pitch {} mm · lead {} mm",
            mm(w.d1),
            mm(2.0 * w.ra()),
            mm(2.0 * w.rf()),
            mm(w.axial_pitch()),
            mm(w.lead())
        ),
    );
    r.hint(
        "wheel_teeth",
        format!(
            "Wheel: d2 = {} · da2 = {} mm (throat) · outside {} mm · Pair: {}",
            mm(w.m * p.z2 as f64),
            mm(2.0 * p.wheel_tooth.ra()),
            mm(entries::wheel_outside_diameter(p)),
            long.join(" · ")
        ),
    );
    r.warnings(&warnings)
}

fn pulley(p: &PulleyPlan) -> Value {
    let pr = p.profile;
    let (mut short, mut long) = (Vec::new(), Vec::new());
    let l = p.belt_length();
    match p.c {
        Some(c) if p.p2.is_some() => {
            let i = p.z2 as f64 / p.z1 as f64;
            short.push(format!("a {} mm", n(c, 2)));
            short.push(format!("i {}", n(i, 3)));
            short.push(format!("L {} mm", n(l, 1)));
            long.push(format!("centre distance a = {} mm", mm(c)));
            long.push(format!("ratio i = {i:.3}"));
        }
        _ => {
            short.push(format!("d {} mm", n(pr.pitch_diameter(p.z1), 2)));
            short.push(format!("do {} mm", n(pr.outside_diameter(p.z1), 2)));
        }
    }
    if let Some(c) = p.c {
        long.push(format!("belt pitch length L = {} mm", mm(l)));
        long.push(format!(
            "belt teeth in the grooves: {:.1} on pulley 1, {:.1} on pulley 2 (wrap {:.1}° / {:.1}°)",
            p.in_mesh(1, c),
            p.in_mesh(2, c),
            p.wrap(1, c).to_degrees(),
            p.wrap(2, c).to_degrees()
        ));
    }
    if p.est > 8.0 {
        short.push(format!("~{:.0} s", p.est));
        long.push(format!("builds in about {:.0} s", p.est));
    }
    let warnings = entries::pulley_warnings(p);
    let mut r = Reply::new(&warnings, &short, &long);
    let unstandard = if pr.key.starts_with("gt2") { " · not standardised: check the fit with a test print and a real belt" } else { "" };
    r.hint("profile", format!("{}: pitch {} mm · 2a = {} mm · from {}{unstandard}", pr.title, pr.pitch, pr.two_a, pr.source));
    let one = |z: u32, n: u32| {
        let mut t = format!(
            "Pulley {n}: d = {} · do = {} mm · groove depth {} mm · {}",
            mm(pr.pitch_diameter(z)),
            mm(pr.outside_diameter(z)),
            mm(pr.depth(z)),
            pr.groove_name(z)
        );
        if p.flange_h > 0.0 {
            t += &format!(" · flange Ø {} mm", mm(pr.outside_diameter(z) + 2.0 * p.flange_h));
        }
        t
    };
    r.hint("teeth_1", one(p.z1, 1));
    r.hint("teeth_2", one(p.z2, 2));
    let widths: Vec<String> = pr.widths.iter().map(|(b, f, u)| format!("belt {} mm: {} / {} mm", n(*b, 1), n(*f, 1), n(*u, 1))).collect();
    let usual = if pr.key.starts_with("gt2") { "Usual pulley widths" } else { "The standard's minimum pulley widths" };
    r.hint("face_width", format!("{usual} for {} (with flanges / without): {}", pr.title, widths.join(" · ")));
    let rule = match pr.key {
        "mxl" | "xl" => "ISO 5294 Annex A",
        "t2.5" | "t5" | "t10" => "DIN 7721-2 Table 8",
        "htd-3m" | "htd-5m" | "htd-8m" => "ISO 13050 Annex D: belt tooth height + a",
        _ => "belt tooth height + a, as ISO 13050 has it for HTD",
    };
    r.hint("flange_height", format!("Above the outside diameter; 0 = auto: {} mm ({rule})", mm(pr.flange_h)));
    // the result line's tooltip does not show (it is read-only), so the belt's numbers go here
    let mut belt = format!("Belt pitch length L = {} mm ({} × {} mm)", mm(l), p.belt, pr.pitch);
    if let Some(c) = p.c {
        belt += &format!(
            " · centre distance {} mm · belt teeth in the grooves: {:.1} on pulley 1, {:.1} on pulley 2 (wrap {:.1}° / {:.1}°)",
            mm(c),
            p.in_mesh(1, c),
            p.in_mesh(2, c),
            p.wrap(1, c).to_degrees(),
            p.wrap(2, c).to_degrees()
        );
    }
    belt += &format!(" · these pulleys need at least {} teeth", p.min_belt_teeth());
    r.hint("belt_teeth", belt);
    r.warnings(&warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(v: &Value) -> &str {
        v[SUMMARY]["value"].as_str().unwrap()
    }

    fn hint<'a>(v: &'a Value, key: &str) -> &'a str {
        v[key]["hint"].as_str().unwrap()
    }

    #[test]
    fn spur_defaults() {
        let v = update_fields("spur-gears", json!({}));
        let s = summary(&v);
        assert_eq!(s, "a 54 mm · i 2 · εα 1.61");
        assert!(hint(&v, SUMMARY).contains("centre distance a = 54.000 mm"));
        assert!(hint(&v, "teeth_1").contains("d = 36.000"));
    }

    /// A refused input: the result line says it in a few words that fit the narrow field, and
    /// the field it is about has the whole sentence as its tooltip.
    fn refused(v: &Value, line: &str, field: &str, says: &str) {
        let s = summary(v);
        assert!(s.starts_with(line), "{s}");
        assert!(s.chars().count() <= 24, "too long for the result line: {s}");
        let tip = hint(v, field);
        assert!(tip.starts_with("⚠ ") && tip.contains(says), "{field}: {tip}");
        assert!(hint(v, SUMMARY).contains(says));
    }

    #[test]
    fn spur_problems_show_in_the_result_line() {
        let v = update_fields("spur-gears", json!({"mate": "ring", "teeth_1": 20, "teeth_2": 22}));
        refused(&v, "⚠ z2: ring: ≥ 26 teeth", "teeth_2", "a ring gear needs at least 6 teeth more");
        let v = update_fields("spur-gears", json!({"mate": "ring", "teeth_2": 60, "rim_diameter": 100}));
        refused(&v, "⚠ Ring Ø > 127", "rim_diameter", "must be larger than");
        let v = update_fields("spur-gears", json!({"teeth_1": 12}));
        assert!(summary(&v).starts_with("⚠ undercut 1 · a "), "{}", summary(&v));
        let tip = hint(&v, "teeth_1");
        assert!(tip.starts_with("⚠ gear 1 undercut: below ") && tip.contains("Gear 1: d = "), "{tip}");
        let v = update_fields("spur-gears", json!({"create_1": false, "create_2": false}));
        refused(&v, "⚠ nothing to build", "create_1", "turn on Create gear 1");
        let v = update_fields("spur-gears", json!({"teeth_1": 150, "teeth_2": 150, "helix_angle": 20}));
        refused(&v, "⚠ z2: ~", "teeth_2", "SolidCraft stops a plugin after 30 s");
        assert!(summary(&v).ends_with("s: too slow"));
        // a bad value is named by its field's title, as the entry reports it
        let v = update_fields("spur-gears", json!({"module": 0}));
        assert_eq!(summary(&v), "⚠ Module: must be between 0.2 and 50");
    }

    #[test]
    fn profile_problems_show_before_run() {
        // pointed teeth were only found by the build; the dialog says so now
        let v = update_fields("spur-gears", json!({"teeth_1": 8, "profile_shift_1": 1.5}));
        refused(&v, "⚠ z1: teeth pointed", "teeth_1", "the teeth come to a point");
        let v = update_fields("spur-gears", json!({"mate": "rack", "teeth_2": 10, "rack_height": 2}));
        refused(&v, "⚠ height > 2.90 mm", "rack_height", "must be more than");
        let v = update_fields("bevel-gears", json!({"teeth_1": 6, "teeth_2": 6, "profile_shift": 0.8, "face_width": 2}));
        refused(&v, "⚠ x1: teeth pointed", "profile_shift", "gear 1: the teeth come to a point");
        // the 15/90 pair that could not be read in the dialog
        let v = update_fields("bevel-gears", json!({"teeth_1": 15, "teeth_2": 90}));
        refused(&v, "⚠ z2: cone 80.5° > 72°", "teeth_2", "a pitch cone angle of 80.5°");
        let v = update_fields("bevel-gears", json!({"teeth_1": 60, "teeth_2": 15}));
        refused(&v, "⚠ z1: cone ", "teeth_1", "a pitch cone angle");
    }

    #[test]
    fn pairs_that_would_collide_are_refused() {
        // no working pressure angle: was clamped to 0° and built overlapping
        let v = update_fields("spur-gears", json!({"teeth_1": 24, "teeth_2": 24, "profile_shift_1": -0.5, "profile_shift_2": -0.5}));
        refused(&v, "⚠ x1: shift too negative", "profile_shift_1", "the profile shifts are too negative");
        // a tiny working pressure angle: the tips run into the other gear's root
        let v = update_fields("spur-gears", json!({"teeth_1": 30, "teeth_2": 30, "profile_shift_1": -0.6, "profile_shift_2": -0.6}));
        refused(&v, "⚠ x1: tips hit the roots", "profile_shift_1", "the tips would reach");
    }

    #[test]
    fn hidden_fields_do_not_get_in_the_way() {
        // Herringbone is hidden without a helix angle and for a rack: a value left there is ignored
        let v = update_fields("spur-gears", json!({"helix_angle": 0, "herringbone": true}));
        assert!(summary(&v).starts_with("a 54 mm"), "{}", summary(&v));
        let v = update_fields("spur-gears", json!({"mate": "rack", "helix_angle": 10, "herringbone": true}));
        assert!(summary(&v).ends_with("mm/turn"), "{}", summary(&v));
    }

    #[test]
    fn small_worm_wheels_build() {
        // the wheel's outside diameter shrinks before its teeth would come to a point
        for z in [10, 14, 18] {
            let v = update_fields("worm-gears", json!({"wheel_teeth": z}));
            assert!(summary(&v).starts_with("a "), "z = {z}: {}", summary(&v));
        }
        // a 30-tooth wheel keeps the full da2 + 0.8 m
        let v = update_fields("worm-gears", json!({"wheel_teeth": 30}));
        assert!(hint(&v, "wheel_teeth").contains("outside 65.600 mm · Pair: centre distance a = 40.000 mm"), "{}", hint(&v, "wheel_teeth"));
    }

    #[test]
    fn rack_and_ring() {
        let v = update_fields("spur-gears", json!({"mate": "rack", "teeth_2": 12}));
        assert_eq!(summary(&v), "113.1 mm/turn");
        assert!(hint(&v, "teeth_2").contains("Pair: pitch line 18.000 mm from the gear axis"), "{}", hint(&v, "teeth_2"));
        let v = update_fields("spur-gears", json!({"mate": "ring", "teeth_2": 60}));
        assert!(summary(&v).starts_with("a 42 mm"), "{}", summary(&v));
        assert!(hint(&v, "rim_diameter").contains("auto"));
    }

    #[test]
    fn bevel_and_worm() {
        let v = update_fields("bevel-gears", json!({}));
        assert_eq!(summary(&v), "i 2 · δ 26.6°/63.4°");
        assert!(hint(&v, "face_width").contains("Re = 33.541 mm"), "{}", hint(&v, "face_width"));
        let v = update_fields("worm-gears", json!({}));
        assert_eq!(summary(&v), "a 40 mm · i 30 · γ 5.71°", "γ = 5.71° by default: not self-locking");
        let v = update_fields("worm-gears", json!({"worm_diameter": 30}));
        assert!(summary(&v).starts_with("⚠ self-locking · a "), "{}", summary(&v));
        assert!(hint(&v, "worm_diameter").starts_with("⚠ likely self-locking"));
    }

    #[test]
    fn pulleys() {
        let v = update_fields("timing-pulleys", json!({}));
        assert_eq!(summary(&v), "a 69.71 mm · i 2", "GT2 20/40 on a 100-tooth belt; L is in the belt's tooltip");
        assert!(hint(&v, "teeth_1").starts_with("Pulley 1: d = 12.732 · do = 12.224 mm"), "{}", hint(&v, "teeth_1"));
        assert!(hint(&v, "profile").contains("not standardised"));
        let belt = hint(&v, "belt_teeth");
        assert!(belt.contains("L = 200.000 mm") && belt.contains("on pulley 1") && belt.contains("wrap 169.5°"), "{belt}");
        // DIN 7721's own diameters
        let v = update_fields("timing-pulleys", json!({"profile": "t5", "teeth_1": 10, "teeth_2": 20}));
        assert!(hint(&v, "teeth_1").contains("do = 15.050 mm") && hint(&v, "teeth_1").contains("form SE"), "{}", hint(&v, "teeth_1"));
        assert!(hint(&v, "teeth_2").contains("form SE"));
        // a short belt is refused while pulley 2 is built, and says how long it must be
        let v = update_fields("timing-pulleys", json!({"belt_teeth": 40}));
        refused(&v, "⚠ belt: ≥ ", "belt_teeth", "the belt is too short for these pulleys: it needs at least ");
        let v = update_fields("timing-pulleys", json!({"belt_teeth": 40, "create_2": false}));
        assert_eq!(summary(&v), "d 12.73 mm · do 12.22 mm");
        // few teeth in mesh
        let v = update_fields("timing-pulleys", json!({"teeth_1": 12, "teeth_2": 80, "belt_teeth": 120}));
        assert!(summary(&v).starts_with("⚠ < 6 in mesh"), "{}", summary(&v));
        assert!(hint(&v, "teeth_1").starts_with("⚠ only "), "{}", hint(&v, "teeth_1"));
        // below the standard's smallest pulley
        let v = update_fields("timing-pulleys", json!({"profile": "htd-8m", "teeth_1": 20}));
        refused(&v, "⚠ z1: ≥ 22 grooves", "teeth_1", "HTD 8M pulleys need at least 22 grooves");
        let v = update_fields("timing-pulleys", json!({"profile": "t2.5", "teeth_1": 10, "groove_clearance": 0.5, "create_2": false}));
        refused(&v, "⚠ no land left", "groove_clearance", "the grooves leave no land");
    }

    #[test]
    fn result_lines_fit_the_row() {
        // whole parts only, up to what the row shows; a helical pair drops εβ (it is in the tooltip)
        for (e, v) in [
            ("spur-gears", json!({"teeth_1": 16, "teeth_2": 24, "helix_angle": 30, "herringbone": true})),
            ("spur-gears", json!({"teeth_1": 12})),
            ("spur-gears", json!({"mate": "rack", "teeth_1": 16})),
            ("spur-gears", json!({"teeth_1": 150, "teeth_2": 150})),
            ("bevel-gears", json!({})),
            ("worm-gears", json!({"worm_diameter": 30})),
            ("timing-pulleys", json!({})),
            ("timing-pulleys", json!({"teeth_1": 12, "teeth_2": 80, "belt_teeth": 120})),
        ] {
            let r = update_fields(e, v.clone());
            let s = summary(&r);
            assert!(s.chars().count() <= LINE_CHARS || !s.contains(" · "), "{e} {v}: {s}");
        }
        let v = update_fields("spur-gears", json!({"teeth_1": 16, "teeth_2": 24, "helix_angle": 30}));
        assert!(hint(&v, "teeth_2").contains("overlap ratio εβ"), "{}", hint(&v, "teeth_2"));
    }

    #[test]
    fn hints_are_single_lines_within_limits() {
        for (e, v) in [
            ("spur-gears", json!({"helix_angle": 20})),
            ("spur-gears", json!({"mate": "rack"})),
            ("bevel-gears", json!({})),
            ("worm-gears", json!({})),
            ("timing-pulleys", json!({})),
            ("timing-pulleys", json!({"profile": "htd-5m", "flanges": "none"})),
        ] {
            let r = update_fields(e, v);
            for (k, f) in r.as_object().unwrap() {
                assert!(schema::entry(e).unwrap().field(k).is_some());
                for s in [f.get("hint"), f.get("value")].into_iter().flatten() {
                    let s = s.as_str().unwrap();
                    assert!(s.len() <= HINT_MAX && !s.chars().any(char::is_control), "{e}.{k}: {s}");
                }
            }
            assert!(summary(&r).len() <= INFO_MAX);
        }
    }

    #[test]
    fn clipping_keeps_characters_whole() {
        let s = "εα".repeat(200);
        let c = clip(&s, 51);
        assert!(c.len() <= 51 && c.ends_with('…'));
    }
}
