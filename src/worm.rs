//! Cylindrical worms (ZA: straight flanks in the axial section) and worm wheels.
//!
//! The worm: one or two starts are cut as thread spaces, a coil of the axial space profile per
//! start, out of a cylinder at the tip diameter; more starts are added as threads to a core. The
//! wheel (built by `cyl`): a helical gear whose helix angle is the worm's lead angle and whose
//! transverse module and pressure angle are the worm's axial ones, with a throat (a torus around
//! the wheel axis) where the worm wraps it. That wheel is the usual approximation of a hobbed
//! wheel: good for printing, models and light duty.

use crate::cad::{created_body, Cad};
use crate::curve::Curve;
use crate::geom::{self, r9, P};
use crate::involute::Tooth;
use crate::schema::{bad, Bad};
use serde_json::{json, Value};
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct WormSpec {
    /// Axial module (mm).
    pub m: f64,
    pub starts: u32,
    /// Pitch diameter (mm).
    pub d1: f64,
    pub length: f64,
    /// Axial pressure angle (rad).
    pub alpha: f64,
    /// +1 right hand, -1 left hand.
    pub hand: f64,
    /// Backlash added to the thread spaces (axial, on the pitch line, mm).
    pub backlash: f64,
    /// Body name; the features are named after it.
    pub name: String,
}

impl WormSpec {
    pub fn axial_pitch(&self) -> f64 {
        PI * self.m
    }
    /// Axial advance per turn: the axial pitch times the starts.
    pub fn lead(&self) -> f64 {
        self.axial_pitch() * self.starts as f64
    }
    /// Lead angle γ on the pitch cylinder.
    pub fn lead_angle(&self) -> f64 {
        (self.lead() / (PI * self.d1)).atan()
    }
    pub fn r1(&self) -> f64 {
        self.d1 / 2.0
    }
    /// Tip radius (addendum 1 m).
    pub fn ra(&self) -> f64 {
        self.r1() + self.m
    }
    /// Root radius (dedendum 1.25 m).
    pub fn rf(&self) -> f64 {
        self.r1() - 1.25 * self.m
    }
}

/// Rough build time of the worm in seconds: every start beyond the first adds a coil.
pub fn est_seconds(w: &WormSpec) -> f64 {
    0.7 + 1.6 * (w.starts as f64 - 1.0)
}

/// Everything that can be checked without the engine. Fields: `worm_diameter`, `pressure_angle`.
pub fn check(w: &WormSpec) -> Result<(), Bad> {
    if w.d1 < 4.0 * w.m {
        return Err(
            bad("worm_diameter", format!("must be at least 4 modules ({} mm)", 4.0 * w.m)).short(format!("worm Ø ≥ {} mm", 4.0 * w.m))
        );
    }
    space_profile(w).map(|_| ()).map_err(|e| bad("pressure_angle", e))
}

/// The worm wheel's tooth in the terms of a helical gear: normal values from the worm's axial
/// ones, addendum and dedendum scaled so they stay 1 m and 1.25 m.
pub fn wheel_tooth(w: &WormSpec, z2: u32, backlash: f64) -> Tooth {
    let g = w.lead_angle();
    Tooth {
        z: z2,
        mn: w.m * g.cos(),
        alpha_n: (w.alpha.tan() * g.cos()).atan(),
        beta: g,
        x: 0.0,
        ha: 1.0 / g.cos(),
        hf: 1.25 / g.cos(),
        rho: 0.3,
        backlash,
    }
}

/// The worm wheel's outside radius: 0.4 module above its throat tip circle (an outside diameter
/// of da2 + 0.8 m), less where the teeth of a small wheel would come to a point before that. The
/// teeth are drawn a little past it (see `cyl`), so they must stay a tenth of a module wide there.
pub fn wheel_blank_radius(t: &Tooth, m: f64) -> f64 {
    let wide = |r: f64| {
        let drawn = r + (0.1 * t.mn).max(0.05);
        2.0 * drawn * t.theta_inv(drawn) >= 0.1 * t.mn
    };
    let want = t.ra() + 0.4 * m;
    if wide(want) {
        return want;
    }
    if !wide(t.ra()) {
        return t.ra(); // pointed even at the throat tip: `cyl::check` refuses the wheel
    }
    let (mut lo, mut hi) = (t.ra(), want);
    for _ in 0..50 {
        let mid = 0.5 * (lo + hi);
        if wide(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// The thread space in the axial section (x = radius, y = axial position, centred on y = 0),
/// from the root out past the tip.
pub fn space_profile(w: &WormSpec) -> Result<Vec<Curve>, String> {
    let px = w.axial_pitch();
    let e1 = px / 2.0 + w.backlash; // space width on the pitch line
    let a = w.alpha;
    let hw = |r: f64| e1 / 2.0 + (r - w.r1()) * a.tan();
    let rf = w.rf();
    let r_out = w.ra() + 0.2 * w.m;
    if hw(rf) <= 0.05 * w.m {
        return Err("the thread spaces close up at the root: lower the pressure angle".into());
    }
    if 2.0 * hw(w.ra()) >= px - 0.1 * w.m {
        return Err("the worm thread comes to a point: lower the pressure angle or the backlash".into());
    }
    // root fillets as on a rack, turned so that the radius plays the rack's height
    let rho = (0.2 * w.m).min(0.9 * hw(rf) * a.cos() / (1.0 - a.sin()));
    let r1 = rf + rho * (1.0 - a.sin());
    let pf = [r1, hw(rf) + rho * (1.0 - a.sin()) * a.tan()];
    let c = [rf + rho, pf[1] - rho * a.cos()];
    let root = [rf, c[1]];
    let (u, v) = (geom::sub(pf, c), geom::sub(root, c));
    let bis = geom::add(geom::scale(u, 1.0 / geom::len(u)), geom::scale(v, 1.0 / geom::len(v)));
    let mid = geom::add(c, geom::scale(bis, rho / geom::len(bis)));
    let m = |p: P| [p[0], -p[1]];
    let top = [r_out, hw(r_out)];
    let mut cv = vec![Curve::Line(m(top), top), Curve::Line(top, pf), Curve::Arc(pf, root, mid)];
    if root[1] > 1e-9 {
        cv.push(Curve::Line(root, m(root)));
    }
    cv.push(Curve::Arc(m(root), m(pf), m(mid)));
    cv.push(Curve::Line(m(pf), m(top)));
    Ok(cv)
}

/// A thread without root fillets: its flanks run straight into the core, so adding it to the
/// core cylinder is a clean crossing (fillets ending tangent to the core make the kernel slow).
pub fn thread_profile_plain(w: &WormSpec) -> Result<Vec<Curve>, String> {
    space_profile(w)?; // the checks
    let px = w.axial_pitch();
    let e1 = px / 2.0 + w.backlash;
    let half = |r: f64| px / 2.0 - (e1 / 2.0 + (r - w.r1()) * w.alpha.tan());
    let inner = w.rf() - (0.3 * w.m).min(0.5 * w.rf());
    let ra = w.ra();
    Ok(vec![
        Curve::Line([inner, -half(inner)], [ra, -half(ra)]),
        Curve::Line([ra, -half(ra)], [ra, half(ra)]),
        Curve::Line([ra, half(ra)], [inner, half(inner)]),
        Curve::Line([inner, half(inner)], [inner, -half(inner)]),
    ])
}

pub struct WormBuilt {
    pub body: String,
    pub info: Value,
}

/// One coil per start of `prof` (axial section, centred on y = 0) from height `z0` on.
#[allow(clippy::too_many_arguments)]
fn coils(cad: &mut Cad, w: &WormSpec, prof: &[Curve], z0: f64, turns: f64, op: &str, body: &str, what: &str) -> Result<(), String> {
    let name = &w.name;
    for k in 0..w.starts {
        let zk = z0 + w.axial_pitch() * k as f64;
        let label = if w.starts == 1 { format!("{name} {what}") } else { format!("{name} {what} {}", k + 1) };
        let sk = cad.sketch_xz(&label)?;
        // sketch y is world -Z
        let curves: Vec<Curve> = prof.iter().map(|c| c.map(&|p| [p[0], -(p[1] + zk)])).collect();
        cad.draw(&curves)?;
        cad.finish()?;
        cad.host.progress(0.2 + 0.6 * k as f32 / w.starts as f32, "the thread");
        let mut args = json!({"sketch": sk, "pitch": r9(w.lead()), "revolutions": turns, "operation": op,
                              "targets": [body], "name": label});
        if w.hand < 0.0 {
            args["clockwise"] = json!(true);
        }
        cad.run("solid.coil", args)?;
    }
    Ok(())
}

/// Build a worm that has passed `check`, on the Z axis from z = 0 to its length, with a thread
/// space centred on azimuth π at half its length (where it meets the wheel once placed).
pub fn build_worm(cad: &mut Cad, w: &WormSpec) -> Result<WormBuilt, String> {
    let name = &w.name;
    let (px, pz) = (w.axial_pitch(), w.lead());
    // a space at azimuth 0 sits at z_s, half a turn on (azimuth π) at z_s + pz/2: put one at
    // half the length there, where the worm meets the wheel
    let mut z_s = w.length / 2.0 - pz / 2.0;
    while z_s > -pz {
        z_s -= px;
    }
    let body;
    if w.starts <= 2 {
        // cut the spaces (with root fillets) out of a cylinder, one coil per start
        let prof = space_profile(w)?;
        let v = cad.cylinder(w.ra(), w.length, "new", None, &format!("{name} blank"), Some(name))?;
        body = created_body(&v, name);
        let turns = ((w.length - z_s) / pz + 1.0).ceil();
        coils(cad, w, &prof, z_s, turns, "cut", &body, "thread space")?;
    } else {
        // more starts: add the threads to a core, which the kernel does much faster than
        // cutting intertwined spaces; these threads have sharp root corners
        let prof = thread_profile_plain(w)?;
        let v = cad.cylinder(w.rf(), w.length, "new", None, &format!("{name} core"), Some(name))?;
        body = created_body(&v, name);
        let z_t = z_s + px / 2.0;
        let turns = ((w.length - z_t) / pz + 1.0).ceil();
        coils(cad, w, &prof, z_t, turns, "join", &body, "thread")?;
        cad.cylinder(w.ra() + w.m, w.length, "intersect", Some(&body), &format!("{name} ends"), None)?;
    }
    let info = json!({
        "body": body,
        "starts": w.starts,
        "axial_module": r9(w.m),
        "pitch_diameter": r9(w.d1),
        "tip_diameter": r9(2.0 * w.ra()),
        "root_diameter": r9(2.0 * w.rf()),
        "diameter_factor_q": r9(w.d1 / w.m),
        "axial_pitch": r9(px),
        "lead": r9(pz),
        "lead_angle": r9(w.lead_angle().to_degrees()),
        "hand": if w.hand > 0.0 { "right" } else { "left" },
        "length": r9(w.length),
    });
    Ok(WormBuilt { body, info })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::loop_gap;

    fn w() -> WormSpec {
        WormSpec { m: 2.0, starts: 1, d1: 20.0, length: 40.0, alpha: 20f64.to_radians(), hand: 1.0, backlash: 0.0, name: "W".into() }
    }

    #[test]
    fn worm_numbers() {
        let w = w();
        assert!((w.lead_angle().tan() - 0.1).abs() < 1e-12);
        assert!((w.ra() - 12.0).abs() < 1e-12);
    }

    #[test]
    fn space_and_thread_close() {
        let c = space_profile(&w()).unwrap();
        assert!(loop_gap(&c) < 1e-12);
        let t = thread_profile_plain(&w()).unwrap();
        assert!(loop_gap(&t) < 1e-12);
    }

    #[test]
    fn wheel_transverse_module_is_axial_module() {
        let t = wheel_tooth(&w(), 30, 0.0);
        assert!((t.mt() - 2.0).abs() < 1e-12);
        assert!((t.alpha_t() - 20f64.to_radians()).abs() < 1e-12);
        assert!((t.ra() - 32.0).abs() < 1e-9);
        assert!((t.rf() - 27.5).abs() < 1e-9);
    }
}
