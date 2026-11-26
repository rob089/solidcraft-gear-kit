//! Cylindrical gears: spur, helical and herringbone, external and internal, and worm wheels.
//!
//! The build: one sketch holding the whole toothed outline (a seed period drawn curve by curve and
//! sketch patterns for the rest), made into a body by one extrude for straight teeth or one sweep
//! with twist for helical teeth. A herringbone gear is a helical half mirrored onto itself. A ring
//! gear's outline includes its outer circle; with many teeth its toothed bore is cut out of a ring
//! cylinder instead. A worm wheel's teeth are intersected with a blank of revolution that has the
//! throat. Bores, hubs and keyways are not part of the gear: model them afterwards like any other
//! feature.

use crate::cad::{self, created_body, Cad};
use crate::curve::Curve;
use crate::geom::r9;
use crate::involute::{self, Detail, Tooth};
use crate::schema::{bad, Bad};
use serde_json::{json, Value};
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct CylSpec {
    pub tooth: Tooth,
    pub internal: bool,
    /// Internal gears: outside diameter of the ring.
    pub rim_d: f64,
    pub width: f64,
    /// +1 right hand, -1 left hand.
    pub hand: f64,
    pub herringbone: bool,
    /// Body name; the features are named after it.
    pub name: String,
    /// Angle of tooth 0 about Z (rad) at height `phase_z`.
    pub phase: f64,
    pub phase_z: f64,
    /// Worm wheels: the blank's outside radius (instead of the tip circle), and the throat: the
    /// worm axis's distance from the wheel axis and the throat radius around it.
    pub blank_r: Option<f64>,
    pub throat: Option<(f64, f64)>,
}

impl CylSpec {
    pub fn new(tooth: Tooth, width: f64, name: &str) -> CylSpec {
        CylSpec {
            tooth,
            internal: false,
            rim_d: 0.0,
            width,
            hand: 1.0,
            herringbone: false,
            name: name.into(),
            phase: 0.0,
            phase_z: 0.0,
            blank_r: None,
            throat: None,
        }
    }

    /// Twist rate of the teeth (rad per mm of height, CCW positive).
    pub fn twist_rate(&self) -> f64 {
        if self.tooth.beta == 0.0 {
            0.0
        } else {
            self.hand * self.tooth.beta.tan() / self.tooth.rp()
        }
    }

    /// The worm wheel's throat and blank radius, if this is one.
    fn wheel(&self) -> Option<((f64, f64), f64)> {
        self.throat.zip(self.blank_r)
    }

    /// Radius the outline reaches out to: the tip circle, or a worm wheel's blank (which trims
    /// the tips) plus a little.
    fn r_tip(&self) -> f64 {
        match self.wheel() {
            Some((_, r_e)) => r_e + (0.1 * self.tooth.mn).max(0.05),
            None => self.tooth.ra(),
        }
    }
}

pub struct Built {
    pub body: String,
    pub info: Value,
}

/// Rough build time in seconds, fitted to measurements on an Apple M-series machine. The sketch
/// of a straight gear grows about with the square of the tooth count, a twisted sweep about
/// linearly; a herringbone gear mirrors its half; a worm wheel's intersection with its blank
/// takes the longest.
pub fn est_seconds(s: &CylSpec) -> f64 {
    let z = s.tooth.zf();
    if s.throat.is_some() {
        return 0.27 * z;
    }
    let t = if s.tooth.beta == 0.0 { 0.4 + 0.0004 * z * z } else { 0.5 + 0.155 * z };
    if s.herringbone {
        t * 1.7
    } else {
        t
    }
}

/// Everything that can be checked without the engine, so the dialog can say it before Run.
/// The field names are generic (`teeth`, `face_width`, `helix_angle`, `herringbone`,
/// `rim_diameter`); the entry renames them to its own.
pub fn check(s: &CylSpec) -> Result<(), Bad> {
    let t = &s.tooth;
    if s.herringbone && t.beta == 0.0 {
        return Err(bad("herringbone", "a herringbone gear needs a helix angle above 0").short("herringbone: no helix"));
    }
    if s.internal && t.beta != 0.0 && t.z > 80 {
        return Err(bad("teeth", "helical ring gears are limited to 80 teeth in this version (straight ones go to 200)")
            .short("helical ring: ≤ 80"));
    }
    let est = est_seconds(s);
    if est > cad::TIME_BUDGET_S {
        return Err(bad("teeth", cad::too_slow("this gear", est, "use fewer teeth (or straight teeth)")).short(cad::too_slow_short(est)));
    }
    let turns = (s.twist_rate() * s.width).abs() / (2.0 * PI);
    if turns > 3.5 {
        return Err(bad("helix_angle", "the teeth would turn more than 3.5 times over the face width").short("helix > 3.5 turns"));
    }
    if s.internal {
        let rim = s.rim_d / 2.0;
        if rim <= t.rf_int() + 0.5 * t.mn {
            let min = 2.0 * (t.rf_int() + 0.5 * t.mn);
            return Err(bad("rim_diameter", format!("must be larger than {min:.3} mm (root diameter + 1 module)"))
                .short(format!("Ring Ø > {min:.1} mm")));
        }
        involute::internal_period(t, Detail::for_teeth(t.z)).map_err(|e| bad("teeth", e))?;
        return Ok(());
    }
    involute::external_flank(t).map_err(|e| bad("teeth", e))?;
    if let Some(((a, r_t), r_e)) = s.wheel() {
        let r_tip = s.r_tip();
        if t.theta_inv(r_tip) * r_tip * 2.0 < 0.02 * t.mn {
            return Err(bad("teeth", "the teeth come to a point below the outside diameter"));
        }
        blank_outline(a, r_t, r_e, s.width, t.mn).map_err(|e| bad("face_width", e))?;
    }
    Ok(())
}

/// The outline of a worm wheel's blank in (r, z): a disc of radius `r_e` and width `b` with the
/// throat (a circle of radius `r_t` around the worm axis, at distance `a` from Z in the mid
/// plane) taken out of its rim, from the axis at the bottom face round to the axis at the top.
fn blank_outline(a: f64, r_t: f64, r_e: f64, b: f64, mn: f64) -> Result<Vec<Curve>, String> {
    let zm = b / 2.0;
    let deep = [a - r_t, zm];
    let lines = |pts: &[[f64; 2]]| -> Vec<Curve> { pts.windows(2).map(|w| Curve::Line(w[0], w[1])).collect() };
    if r_t <= a - r_e {
        return Ok(lines(&[[0.0, 0.0], [r_e, 0.0], [r_e, b], [0.0, b], [0.0, 0.0]]));
    }
    let dz = (r_t * r_t - (a - r_e).powi(2)).sqrt();
    let mut c;
    let (p1, p2);
    if dz < zm {
        // the throat cuts into the rim between the faces
        (p1, p2) = ([r_e, zm - dz], [r_e, zm + dz]);
        c = lines(&[[0.0, 0.0], [r_e, 0.0], p1]);
        c.push(Curve::Arc(p1, p2, deep));
        c.extend(lines(&[p2, [r_e, b], [0.0, b], [0.0, 0.0]]));
    } else {
        // the throat is wider than the wheel: it is the whole rim
        let r0 = a - (r_t * r_t - zm * zm).sqrt();
        if r0 <= 0.1 * mn {
            return Err("the throat reaches the axis: use a narrower wheel".into());
        }
        (p1, p2) = ([r0, 0.0], [r0, b]);
        c = lines(&[[0.0, 0.0], p1]);
        c.push(Curve::Arc(p1, p2, deep));
        c.extend(lines(&[p2, [0.0, b], [0.0, 0.0]]));
    }
    Ok(c)
}

/// The worm wheel's blank, revolved about Z from `blank_outline`.
fn wheel_blank(cad: &mut Cad, s: &CylSpec, a: f64, r_t: f64, r_e: f64) -> Result<String, String> {
    let curves = blank_outline(a, r_t, r_e, s.width, s.tooth.mn)?;
    let name = &s.name;
    let sk = cad.sketch_xz(&format!("{name} blank"))?;
    // sketch y is world -Z
    let drawn: Vec<Curve> = curves.iter().map(|c| c.map(&|p| [p[0], -p[1]])).collect();
    cad.draw(&drawn)?;
    cad.finish()?;
    let v = cad
        .run("solid.revolve", json!({"sketch": sk, "axis": "Z", "operation": "new", "name": format!("{name} blank"), "body_name": name}))?;
    Ok(created_body(&v, name))
}

/// Build a gear that has passed `check`.
pub fn build(cad: &mut Cad, s: &CylSpec) -> Result<Built, String> {
    let t = s.tooth;
    let mn = t.mn;
    let k = s.twist_rate();
    let hb = if s.herringbone { s.width / 2.0 } else { s.width };
    let name = &s.name;
    let mut info = json!({});

    // One period of the outline: a tooth and the space after it.
    let period = if s.internal {
        let (period, ii) = involute::internal_period(&t, Detail::for_teeth(t.z))?;
        info["tip_diameter"] = json!(r9(2.0 * ii.r_tip));
        info["root_diameter"] = json!(r9(2.0 * ii.r_root));
        info["root_fillet_radius"] = json!(r9(ii.fillet));
        info["outer_diameter"] = json!(r9(s.rim_d));
        if ii.tip_raised {
            info["note"] = json!("the tip circle was raised to the base circle; check that the pinion clears it");
        }
        period
    } else {
        let flank = involute::external_flank(&t)?;
        info["tip_diameter"] = json!(r9(2.0 * t.ra()));
        info["root_diameter"] = json!(r9(2.0 * t.rf()));
        info["root_fillet_radius"] = json!(r9(flank.rho));
        info["undercut"] = json!(flank.undercut);
        involute::external_period(&t, &flank, s.r_tip(), Detail::for_teeth(t.z))
    };

    let blank = match s.wheel() {
        Some(((a, r_t), r_e)) => Some(wheel_blank(cad, s, a, r_t, r_e)?),
        None => None,
    };
    // The whole outline in one sketch at z = 0, turned so the teeth sit at `phase` at height
    // `phase_z`, made into a body by one extrude (straight teeth) or one twisted sweep (helical).
    // Ring gears with many teeth: a region with a hole of more than about 500 edges is beyond
    // what the kernel's naming takes, so the toothed bore is cut out of a ring cylinder instead.
    let ring_cut = s.internal && t.z > 80;
    let ring = if ring_cut {
        let v = cad.cylinder(s.rim_d / 2.0, hb, "new", None, &format!("{name} ring"), Some(name))?;
        Some(created_body(&v, name))
    } else {
        None
    };
    let z_sk = if ring_cut { -(0.2 * mn).max(0.3) } else { 0.0 }; // a cut reaches past both faces
    let len = hb - 2.0 * z_sk;
    let start = s.phase - k * (s.phase_z - z_sk);
    let sk = cad.sketch_xy(z_sk, &format!("{name} profile"))?;
    if s.internal && !ring_cut {
        cad.circle([0.0, 0.0], s.rim_d)?;
    }
    cad.loops_circular(&period, t.z as usize, 2.0 * PI / t.zf(), start)?;
    cad.finish()?;
    cad.host.progress(0.5, "making the teeth");
    let mut args = match (&blank, &ring) {
        (Some(b), _) => json!({"operation": "intersect", "targets": [b]}),
        (None, Some(r)) => json!({"operation": "cut", "targets": [r]}),
        (None, None) => json!({"operation": "new", "body_name": name}),
    };
    args["sketch"] = json!(sk);
    args["name"] = json!(format!("{name} teeth"));
    let v = if k == 0.0 {
        args["distance"] = json!(r9(len));
        cad.run("solid.extrude", args)?
    } else {
        let (path, line) = cad.path_z(&format!("{name} helix"), 0.0, z_sk, 0.0, z_sk + len)?;
        args["path_sketch"] = json!(path);
        args["path"] = json!([line]);
        args["twist"] = json!(r9((k * len).to_degrees()));
        cad.run("solid.sweep", args)?
    };
    let blank = blank.or(ring);
    // the new body (the engine may number the name if it is taken), or the blank
    let body = match &blank {
        Some(b) => b.clone(),
        None => created_body(&v, name),
    };
    if s.herringbone {
        cad.run(
            "solid.mirror",
            json!({"bodies": [body], "plane": {"origin": [0.0, 0.0, r9(hb)], "normal": [0.0, 0.0, 1.0]},
                   "combine": true, "name": format!("{name} second half")}),
        )?;
    }
    let twist = (k * s.width).to_degrees();
    info["body"] = json!(body);
    info["teeth"] = json!(t.z);
    info["normal_module"] = json!(r9(t.mn));
    info["transverse_module"] = json!(r9(t.mt()));
    info["pitch_diameter"] = json!(r9(2.0 * t.rp()));
    info["base_diameter"] = json!(r9(2.0 * t.rb()));
    info["face_width"] = json!(r9(s.width));
    if t.beta != 0.0 {
        info["helix_angle"] = json!(r9(t.beta.to_degrees()));
        info["hand"] = json!(if s.hand > 0.0 { "right" } else { "left" });
        info["lead"] = json!(r9(2.0 * PI * t.rp() / t.beta.tan()));
        info["twist_over_face_width_deg"] = json!(r9(if s.herringbone { twist / 2.0 } else { twist }));
    }
    Ok(Built { body, info })
}
