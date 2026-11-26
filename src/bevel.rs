//! Straight bevel gears with spherical involute teeth.
//!
//! Every surface of a straight bevel gear is a cone through the apex, so its section by a plane
//! square to the axis is the same shape at every height, only scaled. The plugin draws the tooth
//! spaces once, in a plane below the gear (the spherical involute projected from the apex onto
//! that plane), sweeps them up the axis while scaling them toward the apex, and cuts them out of a
//! blank of revolution bounded by the back cone, the tip cone and the front cone.
//!
//! Each gear is built with its axis on Z, the pitch circle of its outer (large) end on z = 0 and
//! its apex above, on the Z axis.

use crate::cad::{created_body, Cad};
use crate::curve::Curve;
use crate::geom::{self, polar, r9, P};
use crate::involute::corner_fillet;
use crate::schema::{bad, Bad};
use serde_json::{json, Value};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug)]
pub struct BevelGear {
    pub z: u32,
    /// Pitch cone angle (rad).
    pub delta: f64,
    /// Outer cone distance (apex to the pitch circle of the large end).
    pub re: f64,
    /// Module and pressure angle at the large end.
    pub m: f64,
    pub alpha: f64,
    /// Profile shift coefficient (gear 2 of a pair gets -x).
    pub x: f64,
    /// Addendum and dedendum as multiples of the module.
    pub ha: f64,
    pub hf: f64,
    /// Circular backlash taken off this gear's tooth at the large end (mm).
    pub backlash: f64,
    /// Face width along the cone.
    pub width: f64,
}

/// Pitch cone angles of a pair for a shaft angle (rad).
pub fn cone_angles(z1: u32, z2: u32, shaft: f64) -> (f64, f64) {
    let (z1, z2) = (z1 as f64, z2 as f64);
    let d1 = shaft.sin().atan2(z2 / z1 + shaft.cos());
    (d1, shaft - d1)
}

/// Azimuth of the spherical involute of the base cone (polar angle `db`) at polar angle `th`.
pub fn sph_inv(th: f64, db: f64) -> f64 {
    if th <= db {
        return 0.0;
    }
    let lam = (th.cos() / db.cos()).clamp(-1.0, 1.0).acos();
    let s = lam / db.sin();
    s - (lam.tan() / db.sin()).atan()
}

impl BevelGear {
    pub fn zf(&self) -> f64 {
        self.z as f64
    }
    /// Pitch radius at the large end.
    pub fn rp(&self) -> f64 {
        self.m * self.zf() / 2.0
    }
    /// Height of the apex above the outer pitch circle.
    pub fn apex(&self) -> f64 {
        self.re * self.delta.cos()
    }
    /// Base cone angle.
    pub fn delta_b(&self) -> f64 {
        (self.delta.sin() * self.alpha.cos()).asin()
    }
    /// Tip cone angle (tapered depth: the tip cone runs through the apex).
    pub fn delta_a(&self) -> f64 {
        self.delta + ((self.ha + self.x) * self.m / self.re).atan()
    }
    /// Root cone angle.
    pub fn delta_f(&self) -> f64 {
        self.delta - ((self.hf - self.x) * self.m / self.re).atan()
    }
    /// Half-azimuth of a tooth at polar angle θ on the sphere.
    pub fn psi(&self, th: f64) -> f64 {
        let s = PI * self.m / 2.0 + 2.0 * self.x * self.m * self.alpha.tan() - self.backlash;
        let db = self.delta_b();
        s / (2.0 * self.rp()) + sph_inv(self.delta, db) - sph_inv(th.max(db), db)
    }
    /// Outer tip diameter (on the back cone).
    pub fn tip_diameter(&self) -> f64 {
        2.0 * (self.rp() + (self.ha + self.x) * self.m * self.delta.cos())
    }

    /// The blank's outline in (r, z): back face, back cone, tip cone, front cone, front face.
    pub fn blank(&self) -> Vec<P> {
        let (sd, cd) = self.delta.sin_cos();
        let a = self.apex();
        let u = [sd, -cd]; // down the pitch cone from the apex
        let w = [cd, sd]; // back cone direction, outward
        let at = |d: f64| geom::add([0.0, a], geom::scale(u, d));
        let pe = at(self.re);
        let ri = self.re - self.width;
        let pi = at(ri);
        let ta_t = (self.ha + self.x) * self.m;
        let tf_t = (self.hf - self.x) * self.m;
        let ta = geom::add(pe, geom::scale(w, ta_t));
        let rl = geom::add(pe, geom::scale(w, -tf_t));
        let ti = geom::add(pi, geom::scale(w, ta_t * ri / self.re));
        let rin = geom::add(pi, geom::scale(w, -tf_t * ri / self.re));
        let bk = geom::add(rl, geom::scale(w, -self.m));
        // the front face: a little below the small end's root, never through the axis
        let room = (rin[0] / cd.max(1e-6)) * 0.8;
        let fk = geom::add(rin, geom::scale(w, -self.m.min(room)));
        vec![[0.0, bk[1]], bk, ta, ti, fk, [0.0, fk[1]]]
    }
}

pub struct BevelBuilt {
    pub body: String,
    pub info: Value,
}

/// One period of the gear's outline in the plane at distance `h` below the apex: from the tip of
/// tooth 0's upper flank down round the space after it and over the tip of tooth 1, extended a
/// little past the tip cone (the blank trims it).
fn period(g: &BevelGear, h: f64) -> Result<Vec<Curve>, String> {
    let gc = PI / g.zf();
    let (db, df, da) = (g.delta_b(), g.delta_f(), g.delta_a());
    let th_out = da + 0.08 * g.m / g.re; // just past the tip cone: further out the teeth sharpen and the kernel stumbles on the near-point
    if g.psi(th_out) <= 0.0 {
        return Err("the teeth come to a point: bring the profile shift closer to 0".into());
    }
    let rad = |th: f64| h * th.tan();
    let up = |th: f64| polar(rad(th), g.psi(th));
    let r_root = rad(df);
    let n = 300;
    let dense: Vec<P> = (0..=n).map(|i| up(df + (th_out - df) * i as f64 / n as f64)).collect();
    let rho = 0.25 * g.m * h / g.apex();
    let cf = corner_fillet(&dense, r_root, rho, true, gc).ok_or("no room for the root fillet of the teeth")?;
    let th_q = (geom::len(cf.on_flank) / h).atan();
    let m = |p: P| geom::rot([p[0], -p[1]], 2.0 * gc);
    let mut flank: Vec<Curve> = Vec::new(); // tooth 0's upper flank, from the fillet up
    let q = up(th_q);
    let th0 = if th_q < db {
        flank.push(Curve::Line(q, up(db)));
        db
    } else {
        th_q
    };
    let k = 8;
    let pts: Vec<P> = (0..k).map(|i| up(th0 + (th_out - th0) * i as f64 / (k - 1) as f64)).collect();
    flank.push(Curve::Spline(pts));
    let mut c = Vec::new();
    for cu in flank.iter().rev() {
        c.push(match cu {
            Curve::Line(a, b) => Curve::Line(*b, *a),
            Curve::Spline(v) => Curve::Spline(v.iter().rev().cloned().collect()),
            other => other.clone(),
        });
    }
    c.push(Curve::Arc(q, cf.on_circle, cf.mid));
    if geom::angle(cf.on_circle) < gc - 1e-9 {
        c.push(Curve::Arc(cf.on_circle, m(cf.on_circle), polar(r_root, gc)));
    }
    c.push(Curve::Arc(m(cf.on_circle), m(q), m(cf.mid)));
    for cu in &flank {
        c.push(cu.map(&m));
    }
    let r_out = rad(th_out);
    // over the tip of tooth 1 (centred on 2·gc) to the tip of its upper flank
    let tip1 = geom::rot(up(th_out), 2.0 * gc);
    c.push(Curve::Arc(m(up(th_out)), tip1, polar(r_out, 2.0 * gc)));
    Ok(c)
}

/// Rough build time in seconds.
pub fn est_seconds(g: &BevelGear) -> f64 {
    0.25 + 0.17 * g.zf()
}

/// Everything that can be checked without the engine. Fields: `teeth` (this gear's),
/// `face_width`, `profile_shift`.
pub fn check(g: &BevelGear) -> Result<(), Bad> {
    if g.delta.to_degrees() > 72.0 {
        return Err(bad(
            "teeth",
            format!(
                "a pitch cone angle of {:.1}° is above the 72° this version builds reliably (at a 90° shaft angle the ratio can be up to about 1:3)",
                g.delta.to_degrees()
            ),
        )
        .short(format!("cone {:.1}° > 72°", g.delta.to_degrees())));
    }
    if g.width > 0.42 * g.re {
        return Err(bad("face_width", format!("must be at most {:.2} mm (0.42 × cone distance {:.2} mm)", 0.42 * g.re, g.re))
            .short(format!("width ≤ {:.2} mm", 0.42 * g.re)));
    }
    let pointed = "the teeth come to a point: bring the profile shift closer to 0";
    if g.psi(g.delta_a()) * g.rp() * 2.0 < 0.1 * g.m {
        return Err(bad("profile_shift", pointed));
    }
    if g.blank().iter().any(|p| p[1] >= g.apex()) {
        return Err(bad("face_width", "the blank reaches the apex: use a smaller face width").short("width too large"));
    }
    period(g, g.apex()).map(|_| ()).map_err(|e| bad("profile_shift", e))
}

/// Build a gear that has passed `check`.
pub fn build(cad: &mut Cad, g: &BevelGear, name: &str) -> Result<BevelBuilt, String> {
    let blank = g.blank();
    let a = g.apex();
    let zmin = blank.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
    let zmax = blank.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
    let e = (0.2 * g.m).max(0.3);
    let (z0, z1) = (zmin - e, zmax + e.min(0.5 * (a - zmax)));
    // the blank: revolve its outline about Z
    let sk = cad.sketch_xz(&format!("{name} blank"))?;
    let pts: Vec<Value> = blank.iter().map(|p| json!([r9(p[0]), r9(-p[1])])).collect();
    cad.run("sketch.line", json!({"points": pts, "closed": true}))?;
    cad.finish()?;
    let v = cad
        .run("solid.revolve", json!({"sketch": sk, "axis": "Z", "operation": "new", "name": format!("{name} blank"), "body_name": name}))?;
    let body = created_body(&v, name);
    cad.host.progress(0.1, "blank done");
    // tooth spaces in the plane z0, swept up toward the apex
    let h0 = a - z0;
    let seed = period(g, h0)?;
    let gs = cad.sketch_xy(z0, &format!("{name} section"))?;
    cad.loops_circular(&seed, g.z as usize, 2.0 * PI / g.zf(), 0.0)?;
    cad.finish()?;
    let (path, line) = cad.path_z(&format!("{name} cone axis"), 0.0, z0, 0.0, z1)?;
    cad.host.progress(0.6, "making the teeth");
    // the toothed cone, swept toward the apex and kept where it lies inside the blank
    cad.run(
        "solid.sweep",
        json!({"sketch": gs, "path_sketch": path, "path": [line], "scale": r9((a - z1) / h0),
               "operation": "intersect", "targets": [body], "name": format!("{name} teeth")}),
    )?;
    let info = json!({
        "body": body,
        "teeth": g.z,
        "module_outer": r9(g.m),
        "pitch_diameter_outer": r9(2.0 * g.rp()),
        "tip_diameter_outer": r9(g.tip_diameter()),
        "pitch_cone_angle": r9(g.delta.to_degrees()),
        "tip_cone_angle": r9(g.delta_a().to_degrees()),
        "root_cone_angle": r9(g.delta_f().to_degrees()),
        "cone_distance_outer": r9(g.re),
        "face_width": r9(g.width),
        "apex_height": r9(a),
        "virtual_teeth": r9(g.zf() / g.delta.cos()),
    });
    Ok(BevelBuilt { body, info })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (BevelGear, BevelGear) {
        let (d1, d2) = cone_angles(15, 30, PI / 2.0);
        let re = 2.0 * 15.0 / (2.0 * d1.sin());
        let g =
            |z, d| BevelGear { z, delta: d, re, m: 2.0, alpha: 20f64.to_radians(), x: 0.0, ha: 1.0, hf: 1.25, backlash: 0.0, width: 8.0 };
        (g(15, d1), g(30, d2))
    }

    #[test]
    fn cone_angles_add_up() {
        let (d1, d2) = cone_angles(15, 30, PI / 2.0);
        assert!((d1 + d2 - PI / 2.0).abs() < 1e-12);
        assert!((d1.tan() - 0.5).abs() < 1e-12);
        let (p, g) = pair();
        assert!((p.re - g.re).abs() < 1e-9);
        assert!((g.rp() - g.re * g.delta.sin()).abs() < 1e-9);
    }

    #[test]
    fn spherical_involute_tends_to_planar() {
        // a tiny base cone near the pole behaves like a plane involute
        let db = 1e-3;
        let alpha: f64 = 0.4;
        let th = db / alpha.cos();
        let planar = alpha.tan() - alpha;
        assert!((sph_inv(th, db) - planar).abs() < 1e-5);
    }

    #[test]
    fn thickness_on_pitch_cone() {
        let (p, _) = pair();
        let s = 2.0 * p.rp() * p.psi(p.delta);
        assert!((s - PI).abs() < 1e-12);
    }

    #[test]
    fn periods_chain() {
        let (p, g) = pair();
        for b in [p, g] {
            let c = period(&b, b.apex() + 3.0).unwrap();
            for w in c.windows(2) {
                assert!(geom::dist(w[0].end(), w[1].start()) < 1e-9, "z={}", b.z);
            }
            let turned = geom::rot(c[0].start(), 2.0 * PI / b.zf());
            assert!(geom::dist(c.last().unwrap().end(), turned) < 1e-9);
        }
    }
}
