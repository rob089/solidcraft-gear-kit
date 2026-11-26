//! Gear racks, straight and helical. The rack lies along +X from x = 0 with its pitch line on
//! y = 0 and the teeth pointing to +Y; the faces are z = 0 and z = face width. It starts and ends
//! in the middle of a tooth space.
//!
//! The build: a block, and one sketch with every tooth space (a seed and sketch patterns) cut out
//! of it by an extrude, or for helical teeth by a sweep along the slanted tooth direction.

use crate::cad::{self, created_body, Cad};
use crate::curve::Curve;
use crate::geom::{self, r9, P};
use crate::schema::{bad, Bad};
use serde_json::{json, Value};
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct RackSpec {
    pub teeth: u32,
    /// Normal module (mm), normal pressure angle and helix angle (rad).
    pub mn: f64,
    pub alpha_n: f64,
    pub beta: f64,
    /// +1: the teeth lean so the rack meshes with a right-hand pinion.
    pub hand: f64,
    pub width: f64,
    /// Back face to pitch line.
    pub height: f64,
    /// Addendum, dedendum and root fillet radius, as multiples of the normal module.
    pub ha: f64,
    pub hf: f64,
    pub rho: f64,
    /// Backlash added to the rack's tooth spaces (on the pitch line, along the rack, mm).
    pub backlash: f64,
    /// Body name; the features are named after it.
    pub name: String,
}

impl RackSpec {
    /// Pitch along the rack (the transverse pitch of a helical rack).
    pub fn pitch(&self) -> f64 {
        PI * self.mn / self.beta.cos()
    }
    /// Pressure angle in the section square to the faces.
    pub fn alpha_t(&self) -> f64 {
        (self.alpha_n.tan() / self.beta.cos()).atan()
    }
    pub fn length(&self) -> f64 {
        self.pitch() * self.teeth as f64
    }
}

/// Rough build time in seconds (the sketch grows with the square of the tooth count).
pub fn est_seconds(s: &RackSpec) -> f64 {
    let n = s.teeth as f64;
    0.5 + 0.0002 * n * n * if s.beta > 0.0 { 2.0 } else { 1.0 }
}

/// Everything that can be checked without the engine. Fields: `height`, `teeth`, `addendum`,
/// `dedendum` (see `cyl::check`).
pub fn check(s: &RackSpec) -> Result<(), Bad> {
    if s.height <= (s.hf + 0.2) * s.mn {
        let min = (s.hf + 0.2) * s.mn;
        return Err(
            bad("height", format!("must be more than the dedendum and a little: {min:.3} mm")).short(format!("height > {min:.2} mm"))
        );
    }
    let est = est_seconds(s);
    if est > cad::TIME_BUDGET_S {
        return Err(bad("teeth", cad::too_slow("this rack", est, "make it in shorter pieces")).short(cad::too_slow_short(est)));
    }
    gap(s, s.ha * s.mn).map(|_| ())
}

/// One tooth space centred on x = 0, from the root line up to `y_top`, as a closed loop, and the
/// root fillet radius it got (smaller than asked if the space is narrow).
pub fn gap(s: &RackSpec, y_top: f64) -> Result<(Vec<Curve>, f64), Bad> {
    let a = s.alpha_t();
    let e = s.pitch() / 2.0 + s.backlash; // space width on the pitch line
    let yf = -s.hf * s.mn;
    let hw = |y: f64| e / 2.0 + y * a.tan();
    let hw_f = hw(yf);
    if hw_f <= 0.0 {
        return Err(bad("dedendum", "the rack's tooth spaces close up at the root: lower the dedendum"));
    }
    if hw(s.ha * s.mn) * 2.0 >= s.pitch() - 0.05 * s.mn {
        return Err(bad("addendum", "the rack teeth come to a point: lower the addendum or the backlash"));
    }
    let rho_max = hw_f * a.cos() / (1.0 - a.sin());
    let rho = (s.rho * s.mn).min(0.95 * rho_max);
    // a rounding too small to draw is left out: the root corners stay sharp
    let rho = if rho < 1e-3 * s.mn { 0.0 } else { rho };
    let y1 = yf + rho * (1.0 - a.sin());
    let pf = [hw_f + rho * (1.0 - a.sin()) * a.tan(), y1];
    let c = [pf[0] - rho * a.cos(), yf + rho];
    let root = [c[0], yf];
    let (u, v) = (geom::sub(pf, c), geom::sub(root, c));
    let bis = geom::add(geom::scale(u, 1.0 / geom::len(u)), geom::scale(v, 1.0 / geom::len(v)));
    let mid = geom::add(c, geom::scale(bis, rho / geom::len(bis)));
    let m = |p: P| [-p[0], p[1]];
    let top = [hw(y_top), y_top];
    let mut cv = Vec::new();
    if rho > 0.0 {
        cv.push(Curve::Line(top, m(top)));
        cv.push(Curve::Line(m(top), m(pf)));
        cv.push(Curve::Arc(m(pf), m(root), m(mid)));
        if root[0] > 1e-9 {
            cv.push(Curve::Line(m(root), root));
        }
        cv.push(Curve::Arc(root, pf, mid));
        cv.push(Curve::Line(pf, top));
    } else {
        let bot = [hw_f, yf];
        cv.push(Curve::Line(top, m(top)));
        cv.push(Curve::Line(m(top), m(bot)));
        cv.push(Curve::Line(m(bot), bot));
        cv.push(Curve::Line(bot, top));
    }
    Ok((cv, rho))
}

pub struct BuiltRack {
    pub body: String,
    pub info: Value,
}

/// Build a rack that has passed `check`.
pub fn build(cad: &mut Cad, s: &RackSpec) -> Result<BuiltRack, String> {
    let mn = s.mn;
    let e = (0.2 * mn).max(0.3); // the cut reaches this far past both faces
    let p = s.pitch();
    let length = s.length();
    let y_tip = s.ha * mn;
    let (seed, rho) = gap(s, y_tip + (0.1 * mn).max(0.05)).map_err(|b| b.reason)?;
    let name = &s.name;
    let k = s.hand * s.beta.tan(); // x shift of the teeth per mm of height
                                   // blank
    let sk = cad.sketch_xy(0.0, &format!("{name} blank"))?;
    cad.rectangle([0.0, -s.height], [length, y_tip])?;
    cad.finish()?;
    let v = cad.extrude_new(sk, s.width, &format!("{name} blank"), name)?;
    let body = created_body(&v, name);
    // tooth spaces, one at each end and one between every two teeth
    let gaps = cad.sketch_xy(-e, &format!("{name} tooth spaces"))?;
    let seed: Vec<Curve> = seed.iter().map(|c| c.shifted([-k * e, 0.0])).collect();
    cad.loops_linear(&seed, s.teeth as usize + 1, p)?;
    cad.finish()?;
    let length_z = s.width + 2.0 * e;
    if k == 0.0 {
        cad.extrude_cut(gaps, length_z, &body, &format!("{name} teeth"))?;
    } else {
        let (path, line) = cad.path_z(&format!("{name} helix"), -k * e, -e, k * (s.width + e), s.width + e)?;
        cad.run(
            "solid.sweep",
            json!({"sketch": gaps, "path_sketch": path, "path": [line], "orientation": "parallel",
                   "operation": "cut", "targets": [body], "name": format!("{name} teeth")}),
        )?;
    }
    let info = json!({
        "body": body,
        "teeth": s.teeth,
        "normal_module": r9(mn),
        "pitch": r9(p),
        "length": r9(length),
        "pitch_line": "y = 0",
        "tip_line_y": r9(y_tip),
        "root_line_y": r9(-s.hf * mn),
        "root_fillet_radius": r9(rho),
        "helix_angle": r9(s.beta.to_degrees()),
    });
    Ok(BuiltRack { body, info })
}

/// Cross-section area of a rack (any helix angle: the shear keeps the area of every slice).
pub fn section_area(s: &RackSpec) -> Result<f64, String> {
    let y_tip = s.ha * s.mn;
    let (c, _) = gap(s, y_tip).map_err(|b| b.reason)?;
    let gap_area = geom::polygon_area(&crate::curve::loop_polygon(&c)).abs();
    Ok(s.length() * (s.height + y_tip) - s.teeth as f64 * gap_area)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::loop_gap;

    #[test]
    fn rack_gap_closes_and_has_the_right_width() {
        let s = RackSpec {
            teeth: 10,
            mn: 2.0,
            alpha_n: 20f64.to_radians(),
            beta: 0.0,
            hand: 1.0,
            width: 10.0,
            height: 10.0,
            ha: 1.0,
            hf: 1.25,
            rho: 0.38,
            backlash: 0.0,
            name: "R".into(),
        };
        let (c, _) = gap(&s, 2.2).unwrap();
        assert!(loop_gap(&c) < 1e-12);
        // the flanks cross the pitch line half a pitch apart
        if let Curve::Line(a, b) = c.last().unwrap() {
            let t = (0.0 - a[1]) / (b[1] - a[1]);
            let x = a[0] + t * (b[0] - a[0]);
            assert!((2.0 * x - PI).abs() < 1e-12, "{x}");
        } else {
            panic!("last curve is a flank line");
        }
    }
}
