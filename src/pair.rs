//! Meshing pairs of cylindrical gears: working centre distance with profile shift, contact and
//! overlap ratio, tip clearance, tooth span (base tangent length), undercut limit, and a pair's
//! numbers as the entries report them.
//!
//! Gear 1 is the pinion. Gear 2 is an external gear, a ring gear or a rack; a ring gear's numbers
//! use positive tooth counts, and its profile shift counts positive outward, away from its axis
//! (thinner teeth, wider spaces), as in KHK's tables; ISO 21771, with negative tooth counts for
//! internal gears, uses the opposite sign.

use crate::geom::{inv, r9};
use crate::involute::Tooth;
use serde_json::{json, Value};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PairType {
    External,
    Internal,
    Rack,
}

/// Solve inv(a) = v for a in (0, π/2); `v` must be positive.
pub fn inv_inverse(v: f64) -> f64 {
    let mut a = (3.0 * v).cbrt().min(1.5); // inv(a) ≈ a³/3 near 0
    for _ in 0..60 {
        let f = inv(a) - v;
        let df = a.tan().powi(2);
        if df == 0.0 {
            break;
        }
        let next = (a - f / df).clamp(1e-9, PI / 2.0 - 1e-9);
        if (next - a).abs() < 1e-15 {
            a = next;
            break;
        }
        a = next;
    }
    a
}

#[derive(Clone, Copy, Debug)]
pub struct Mesh {
    /// Reference centre distance (no shift). For a rack: gear 1's axis to the rack's pitch line.
    pub a: f64,
    /// Working centre distance and transverse working pressure angle.
    pub aw: f64,
    pub alpha_w: f64,
    /// Transverse contact ratio εα.
    pub contact_ratio: f64,
}

/// How gear 1 and gear 2 mesh. For a rack, `t2` is not used. Fails when the profile shifts are
/// so negative that no working pressure angle closes the backlash-free mesh.
pub fn mesh(t1: &Tooth, t2: &Tooth, kind: PairType) -> Result<Mesh, String> {
    let at = t1.alpha_t();
    let pbt = PI * t1.mt() * at.cos();
    let sq = |ra: f64, rb: f64| (ra * ra - rb * rb).max(0.0).sqrt();
    let working = |v: f64, fix: &str| {
        if v > 0.0 {
            Ok(inv_inverse(v))
        } else {
            Err(format!("the profile shifts are too negative for these gears to mesh: {fix}"))
        }
    };
    Ok(match kind {
        PairType::External => {
            let a = t1.rp() + t2.rp();
            let v = inv(at) + 2.0 * (t1.x + t2.x) * t1.alpha_n.tan() / (t1.zf() + t2.zf());
            let alpha_w = working(v, "raise x1 + x2")?;
            let aw = a * at.cos() / alpha_w.cos();
            let eps = (sq(t1.ra(), t1.rb()) + sq(t2.ra(), t2.rb()) - aw * alpha_w.sin()) / pbt;
            Mesh { a, aw, alpha_w, contact_ratio: eps }
        }
        PairType::Internal => {
            let a = t2.rp() - t1.rp();
            let v = inv(at) + 2.0 * (t2.x - t1.x) * t1.alpha_n.tan() / (t2.zf() - t1.zf());
            let alpha_w = working(v, "raise x2 or lower x1")?;
            let aw = a * at.cos() / alpha_w.cos();
            let ra2 = t2.ra_int().max(t2.rb());
            let eps = (sq(t1.ra(), t1.rb()) - sq(ra2, t2.rb()) + aw * alpha_w.sin()) / pbt;
            Mesh { a, aw, alpha_w, contact_ratio: eps }
        }
        PairType::Rack => {
            let a = t1.rp() + t1.x * t1.mn;
            let ga1 = sq(t1.ra(), t1.rb()) - t1.rp() * at.sin();
            let ga2 = (t1.ha - t1.x) * t1.mn / at.sin();
            Mesh { a, aw: a, alpha_w: at, contact_ratio: (ga1 + ga2) / pbt }
        }
    })
}

/// Overlap ratio εβ of helical teeth over the face width.
pub fn overlap_ratio(t: &Tooth, width: f64) -> f64 {
    width * t.beta.sin() / (PI * t.mn)
}

/// Tip clearance of an external pair at the working centre distance: the smaller of the gaps
/// between one gear's tip circle and the other's root circle.
pub fn tip_clearance(t1: &Tooth, t2: &Tooth, m: &Mesh) -> f64 {
    (m.aw - t1.ra() - t2.rf()).min(m.aw - t2.ra() - t1.rf())
}

/// Number of teeth spanned and the base tangent length W_k of an external gear (normal section,
/// DIN 3960): k puts the anvils' contact near the circle d + 2·x·mn.
pub fn tooth_span(t: &Tooth) -> (u32, f64) {
    let z = t.zf();
    let at = t.alpha_t();
    let beta_b = (t.beta.tan() * at.cos()).atan();
    let cos_ax = (z * at.cos() / (z + 2.0 * t.x * t.beta.cos())).clamp(-1.0, 1.0);
    let ax = cos_ax.acos();
    let k = ((z / PI) * (ax.tan() / beta_b.cos().powi(2) - 2.0 * t.x * t.alpha_n.tan() / z - inv(at)) + 0.5).round().max(1.0);
    let w = t.mn * t.alpha_n.cos() * (PI * (k - 0.5) + z * inv(at)) + 2.0 * t.x * t.mn * t.alpha_n.sin();
    (k as u32, w)
}

/// Fewest teeth an external gear can have without undercut when its basic rack cuts it (17.1 for
/// a standard 20° gear). What counts is how deep the rack's straight flank reaches: its tip
/// rounding starts hf − ρ(1 − sin α) below the reference line (1.0 module for DIN 867, with ρ as
/// `Tooth::cutter_tip_radius`), so the limit follows the dedendum and the root fillet, not the
/// addendum.
pub fn min_teeth(t: &Tooth) -> f64 {
    let at = t.alpha_t();
    let rho = t.cutter_tip_radius().unwrap_or(0.0) / t.mn;
    let depth = t.hf - rho * (1.0 - at.sin());
    2.0 * (depth - t.x) * t.beta.cos() / at.sin().powi(2)
}

/// One gear's numbers for a reply.
pub fn gear_numbers(t: &Tooth, internal: bool) -> Value {
    let mut v = json!({
        "teeth": t.z,
        "pitch_diameter": r9(2.0 * t.rp()),
        "base_diameter": r9(2.0 * t.rb()),
        "profile_shift": r9(t.x),
    });
    if internal {
        v["tip_diameter"] = json!(r9(2.0 * t.ra_int().max(t.rb())));
        v["root_diameter"] = json!(r9(2.0 * t.rf_int()));
        v["space_width_on_pitch_circle"] = json!(r9(t.et_int()));
    } else {
        let (k, w) = tooth_span(t);
        v["tip_diameter"] = json!(r9(2.0 * t.ra()));
        v["root_diameter"] = json!(r9(2.0 * t.rf()));
        v["tooth_thickness_on_pitch_circle"] = json!(r9(t.st()));
        v["span_teeth_k"] = json!(k);
        v["span_measurement_wk"] = json!(r9(w));
        v["undercut_free_from_teeth"] = json!(r9(min_teeth(t).max(0.0)));
    }
    v
}

/// A pair's numbers for a reply.
pub fn pair_numbers(t1: &Tooth, t2: &Tooth, kind: PairType, m: &Mesh, width: f64) -> Value {
    let mut v = json!({
        "pair": match kind { PairType::External => "external", PairType::Internal => "internal", PairType::Rack => "rack" },
        "normal_module": r9(t1.mn),
        "transverse_module": r9(t1.mt()),
        "transverse_pressure_angle": r9(t1.alpha_t().to_degrees()),
        "reference_centre_distance": r9(m.a),
        "centre_distance": r9(m.aw),
        "working_pressure_angle": r9(m.alpha_w.to_degrees()),
        "contact_ratio": r9(m.contact_ratio),
        "gear_1": gear_numbers(t1, false),
    });
    if kind == PairType::Rack {
        v["rack_pitch"] = json!(r9(PI * t1.mt()));
        v["rack_travel_per_pinion_turn"] = json!(r9(2.0 * PI * t1.rp()));
        v["note"] = json!("centre distance = gear 1 axis to rack pitch line");
    } else {
        v["ratio"] = json!(r9(t2.zf() / t1.zf()));
        v["gear_2"] = gear_numbers(t2, kind == PairType::Internal);
    }
    if t1.beta > 0.0 {
        let eb = overlap_ratio(t1, width);
        v["helix_angle"] = json!(r9(t1.beta.to_degrees()));
        v["overlap_ratio"] = json!(r9(eb));
        v["total_contact_ratio"] = json!(r9(m.contact_ratio + eb));
    }
    if kind == PairType::External {
        v["tip_clearance"] = json!(r9(tip_clearance(t1, t2, m)));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_involute() {
        for a in [0.1f64, 0.35, 0.6, 1.0] {
            assert!((inv_inverse(inv(a)) - a).abs() < 1e-12);
        }
    }

    #[test]
    fn standard_pair() {
        let t1 = Tooth::standard(20, 2.0, 20.0);
        let t2 = Tooth::standard(40, 2.0, 20.0);
        let m = mesh(&t1, &t2, PairType::External).unwrap();
        assert!((m.a - 60.0).abs() < 1e-12);
        assert!((m.aw - 60.0).abs() < 1e-9);
        // textbook: about 1.64 for 20/40 teeth at 20°
        assert!((m.contact_ratio - 1.64).abs() < 0.02, "{}", m.contact_ratio);
    }

    #[test]
    fn shifted_pair_moves_apart() {
        let mut t1 = Tooth::standard(12, 2.0, 20.0);
        t1.x = 0.4;
        let t2 = Tooth::standard(30, 2.0, 20.0);
        let m = mesh(&t1, &t2, PairType::External).unwrap();
        assert!(m.aw > m.a && m.aw < m.a + 0.4 * 2.0);
        // so negative that nothing closes the mesh: refused, not clamped to αw = 0
        let (mut t1, mut t2) = (Tooth::standard(24, 2.0, 20.0), Tooth::standard(24, 2.0, 20.0));
        (t1.x, t2.x) = (-0.5, -0.5);
        assert!(mesh(&t1, &t2, PairType::External).is_err());
    }

    #[test]
    fn span_of_a_standard_gear() {
        // z = 30, m = 2, 20°: k = 4, W = 2 × 10.7526 mm (tables: 10.7526 for m = 1)
        let (k, w) = tooth_span(&Tooth::standard(30, 2.0, 20.0));
        assert_eq!(k, 4);
        assert!((w - 21.5052).abs() < 0.001, "{w}");
    }

    #[test]
    fn span_of_a_helical_gear() {
        // z = 30, mn = 2, αn = 20°, β = 30°: k = 5 puts the contact near the pitch circle (without
        // the 1/cos²βb of DIN 3960 it would be k = 4, touching 1.3 mm below it)
        let mut t = Tooth::standard(30, 2.0, 20.0);
        t.beta = 30f64.to_radians();
        let (k, w) = tooth_span(&t);
        assert_eq!(k, 5);
        // the anvils touch where the transverse roll is W·cos βb / 2 from the base tangent point
        let beta_b = (t.beta.tan() * t.alpha_t().cos()).atan();
        let r = (t.rb().powi(2) + (w * beta_b.cos() / 2.0).powi(2)).sqrt();
        assert!((r - t.rp()).abs() < 0.02 * t.rp(), "contact at r = {r}, pitch radius {}", t.rp());
    }

    #[test]
    fn min_teeth_follows_the_generator() {
        let t = Tooth::standard(20, 1.0, 20.0);
        assert!((min_teeth(&t) - 17.097).abs() < 0.01);
        // the limit agrees with what the rack cutter actually generates, for a deeper dedendum
        // and a taller addendum alike
        for (hf, ha) in [(1.25, 1.0), (1.6, 1.0), (1.25, 1.5), (1.4, 0.8)] {
            for z in 8..40 {
                let t = Tooth { hf, ha, ..Tooth::standard(z, 2.0, 20.0) };
                let Ok(f) = crate::involute::external_flank(&t) else { continue };
                let limit = min_teeth(&t);
                if (t.zf() - limit).abs() > 0.5 {
                    assert_eq!(f.undercut, t.zf() < limit, "hf {hf}, ha {ha}, z {z}: limit {limit}");
                }
            }
        }
    }
}
