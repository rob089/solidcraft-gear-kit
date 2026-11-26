//! Timing belt pulleys (synchronous pulleys): the grooved outline of one pulley as the standards
//! give it, the drive two pulleys and a belt make, and the build.
//!
//! The grooves are drawn as the standards dimension them, from lines and arcs only:
//!
//! * MXL, XL: ISO 5294:2012, straight-sided grooves (Table 2: bottom width bw, depth hg, flank
//!   angle φ, bottom radius rb, top radius rt). Outside diameter d − 2a. Flange height: Annex A.
//!   (The standard also gives a generating rack for involute grooves; the straight-sided form is
//!   the one it dimensions directly.)
//! * T2.5, T5, T10: DIN 7721-2:1989, straight flanks at 50°, groove form SE up to 20 grooves and
//!   N above (Table 1: width br at the outside diameter, depth hg). The pitch diameters are those
//!   of Table 4, which are 0.05 to 0.24 mm larger than z·p/π; between the tooth counts it lists,
//!   the difference is interpolated. Flange height: Table 8.
//! * HTD 3M, 5M, 8M: ISO 13050:2022, profile system H, the nominal groove for each range of
//!   groove counts (Table 14): a bottom flat 2X wide, an arc of radius R1 on each side whose
//!   centre is X off the groove axis, a straight flank at ϕ to the axis, and the rounding rt into
//!   the outside diameter, d − 2a + N' (Table 16). Flange height ht + a (Annex D).
//! * GT2 2 mm and 3 mm: in no standard. The groove is the belt tooth Gates publishes for its
//!   2MGT and 3MGT belts (tip arc, flank arcs centred on the land line, land rounding), with the
//!   pitch line differential of HTD; flange height by ISO 13050's rule. A test print with a real
//!   belt is the only real check.
//!
//! The groove clearance moves the whole groove outline outward by half its value (the flanks
//! apart, the bottom deeper), keeping the outside diameter: printed pulleys usually want some.
//!
//! Pulley 1 sits on the Z axis, pulley 2 on +X at the centre distance the belt length sets. A
//! pulley's flanges and teeth stack up from z = 0: the lower flange, the toothed part, the upper
//! flange. As with the gears, bores and hubs are left to the user.

use crate::cad::{self, created_body, Cad};
use crate::curve::Curve;
use crate::geom::{self, polar, r9, P};
use crate::schema::{bad, Bad};
use serde_json::{json, Value};
use std::f64::consts::PI;

/// Straight flanks (ISO 5294 Table 2, DIN 7721-2 Table 1).
#[derive(Clone, Copy, Debug)]
pub struct Trap {
    /// The groove's width: at the outside diameter when `at_top` (DIN 7721 br), else at the
    /// bottom (ISO 5294 bw).
    pub width: f64,
    pub at_top: bool,
    /// Depth below the outside diameter, hg.
    pub depth: f64,
    /// Angle of each flank to the groove axis (degrees).
    pub half_angle: f64,
    /// Bottom and top rounding radii.
    pub rb: f64,
    pub rt: f64,
}

/// An HTD groove (ISO 13050 Table 14).
#[derive(Clone, Copy, Debug)]
pub struct Htd {
    pub hg: f64,
    /// The arcs' centres lie this far off the groove axis; the bottom is flat between them.
    pub x: f64,
    pub r1: f64,
    /// Angle of the straight flank to the groove axis (degrees).
    pub phi: f64,
    pub rt: f64,
}

/// A GT2 groove: the shape of the belt tooth.
#[derive(Clone, Copy, Debug)]
pub struct Gt {
    /// Tooth height.
    pub h: f64,
    pub r_tip: f64,
    /// The flank arcs, centred on the land line.
    pub r_flank: f64,
    /// The rounding where the flank meets the land.
    pub r_land: f64,
}

/// The groove of one pulley.
#[derive(Clone, Copy, Debug)]
pub enum Groove {
    Trap(Trap),
    Htd(Htd),
    Gt(Gt),
}

#[derive(Debug)]
enum Kind {
    Iso5294(Trap),
    /// The SE groove (up to 20 grooves), the N groove, and Table 4: (grooves, pitch diameter).
    Din7721 {
        se: Trap,
        n: Trap,
        d: &'static [(u32, f64)],
    },
    /// Groove per range (from this many grooves on), and N' (grooves, value).
    Htd {
        ranges: &'static [(u32, Htd)],
        n_corr: &'static [(u32, f64)],
    },
    Gt(Gt),
}

#[derive(Debug)]
pub struct Profile {
    /// The dialog's value for it.
    pub key: &'static str,
    pub title: &'static str,
    pub pitch: f64,
    /// Twice the pitch line's distance outside the outside diameter (2a).
    pub two_a: f64,
    pub min_teeth: u32,
    /// The most grooves the standard's groove table covers (0: no limit but time).
    pub max_teeth: u32,
    /// The standard's minimum flange height above the outside diameter.
    pub flange_h: f64,
    /// (belt width, minimum pulley width with flanges, without), as the standard lists them.
    pub widths: &'static [(f64, f64, f64)],
    /// The standard, or why there is none.
    pub source: &'static str,
    kind: Kind,
}

const fn trap(width: f64, at_top: bool, depth: f64, half_angle: f64, rb: f64, rt: f64) -> Trap {
    Trap { width, at_top, depth, half_angle, rb, rt }
}
const fn htd(hg: f64, x: f64, r1: f64, phi: f64, rt: f64) -> Htd {
    Htd { hg, x, r1, phi, rt }
}

// DIN 7721-2 Table 4: pitch diameters
const DIN_T2_5: &[(u32, f64)] = &[
    (10, 8.05),
    (11, 8.85),
    (12, 9.60),
    (13, 10.40),
    (14, 11.20),
    (15, 12.00),
    (16, 12.80),
    (17, 13.60),
    (18, 14.40),
    (19, 15.20),
    (20, 16.00),
    (22, 17.60),
    (25, 19.95),
    (28, 22.35),
    (32, 25.55),
    (36, 28.75),
    (40, 31.90),
    (48, 38.30),
    (60, 47.85),
    (72, 57.40),
    (84, 66.95),
    (96, 76.50),
];
const DIN_T5: &[(u32, f64)] = &[
    (10, 16.05),
    (11, 17.65),
    (12, 19.25),
    (13, 20.85),
    (14, 22.45),
    (15, 24.05),
    (16, 25.60),
    (17, 27.20),
    (18, 28.80),
    (19, 30.40),
    (20, 32.00),
    (22, 35.25),
    (25, 39.95),
    (28, 44.75),
    (32, 51.10),
    (36, 57.45),
    (40, 63.85),
    (48, 76.55),
    (60, 95.65),
    (72, 114.75),
    (84, 133.90),
    (96, 153.00),
];
const DIN_T10: &[(u32, f64)] = &[
    (12, 38.35),
    (13, 41.55),
    (14, 44.70),
    (15, 47.90),
    (16, 51.10),
    (17, 54.25),
    (18, 57.45),
    (19, 60.65),
    (20, 63.80),
    (22, 70.20),
    (25, 79.75),
    (28, 89.25),
    (32, 102.00),
    (36, 114.75),
    (40, 127.45),
    (48, 152.95),
    (60, 191.15),
    (72, 229.30),
    (84, 267.50),
    (96, 305.70),
];

// ISO 13050 Table 14 (hg, X, R1, ϕ, rt) by the first groove count of each range
const H3M: &[(u32, Htd)] = &[
    (10, htd(1.190, 0.029, 0.991, 15.0, 0.181)),
    (14, htd(1.179, 0.112, 0.889, 9.0, 0.229)),
    (26, htd(1.219, 0.028, 0.927, 8.0, 0.191)),
    (81, htd(1.234, 0.074, 0.925, 4.0, 0.301)),
];
const H5M: &[(u32, Htd)] = &[
    (12, htd(1.989, 0.307, 1.265, 10.0, 0.432)),
    (17, htd(2.009, 0.320, 1.270, 6.0, 0.508)),
    (26, htd(2.052, 0.081, 1.438, 2.0, 0.488)),
    (81, htd(2.056, 0.028, 1.552, 5.0, 0.569)),
];
const H8M: &[(u32, Htd)] =
    &[(22, htd(3.295, 0.0, 2.675, 11.3, 0.874)), (28, htd(3.604, 0.0, 2.629, 7.0, 1.024)), (90, htd(3.630, 0.0, 2.639, 6.6, 1.008))];
// ISO 13050 Table 16; N' is 0 for H3M and H5M
const H8M_N: &[(u32, f64)] = &[(28, 0.15), (29, 0.15), (30, 0.10), (31, 0.08), (32, 0.05), (33, 0.03)];

pub static PROFILES: &[Profile] = &[
    Profile {
        key: "gt2-2",
        title: "GT2 2 mm",
        pitch: 2.0,
        two_a: 0.508,
        min_teeth: 12,
        max_teeth: 0,
        flange_h: 1.0,
        widths: &[(6.0, 7.0, 8.5), (9.0, 10.0, 11.5)],
        source: "Gates' 2MGT belt tooth; in no standard",
        kind: Kind::Gt(Gt { h: 0.75, r_tip: 0.555, r_flank: 1.0, r_land: 0.15 }),
    },
    Profile {
        key: "gt2-3",
        title: "GT2 3 mm",
        pitch: 3.0,
        two_a: 0.762,
        min_teeth: 12,
        max_teeth: 0,
        flange_h: 1.5,
        widths: &[(9.0, 10.0, 12.0), (15.0, 16.0, 18.0)],
        source: "Gates' 3MGT belt tooth; in no standard",
        kind: Kind::Gt(Gt { h: 1.14, r_tip: 0.85, r_flank: 1.52, r_land: 0.25 }),
    },
    Profile {
        key: "htd-3m",
        title: "HTD 3M",
        pitch: 3.0,
        two_a: 0.762,
        min_teeth: 10,
        max_teeth: 200,
        flange_h: 1.6,
        widths: &[(6.0, 8.0, 11.0), (9.0, 11.0, 14.0), (15.0, 17.0, 20.0)],
        source: "ISO 13050 profile H3M",
        kind: Kind::Htd { ranges: H3M, n_corr: &[] },
    },
    Profile {
        key: "htd-5m",
        title: "HTD 5M",
        pitch: 5.0,
        two_a: 1.144,
        min_teeth: 12,
        max_teeth: 200,
        flange_h: 2.7,
        widths: &[(9.0, 11.0, 15.0), (15.0, 17.0, 21.0), (25.0, 27.0, 31.0)],
        source: "ISO 13050 profile H5M",
        kind: Kind::Htd { ranges: H5M, n_corr: &[] },
    },
    Profile {
        key: "htd-8m",
        title: "HTD 8M",
        pitch: 8.0,
        two_a: 1.372,
        min_teeth: 22,
        max_teeth: 200,
        flange_h: 4.1,
        widths: &[(20.0, 22.0, 30.0), (30.0, 32.0, 40.0), (50.0, 53.0, 60.0), (85.0, 89.0, 96.0)],
        source: "ISO 13050 profile H8M",
        kind: Kind::Htd { ranges: H8M, n_corr: H8M_N },
    },
    Profile {
        key: "t2.5",
        title: "T2.5",
        pitch: 2.5,
        two_a: 0.6,
        min_teeth: 10,
        max_teeth: 0,
        flange_h: 0.8,
        widths: &[(4.0, 5.5, 8.0), (6.0, 7.5, 10.0), (10.0, 11.5, 14.0)],
        source: "DIN 7721-2",
        kind: Kind::Din7721 { se: trap(1.75, true, 0.75, 25.0, 0.2, 0.3), n: trap(1.83, true, 1.0, 25.0, 0.2, 0.3), d: DIN_T2_5 },
    },
    Profile {
        key: "t5",
        title: "T5",
        pitch: 5.0,
        two_a: 1.0,
        min_teeth: 10,
        max_teeth: 0,
        flange_h: 1.2,
        widths: &[(6.0, 7.5, 10.0), (10.0, 11.5, 14.0), (16.0, 17.5, 20.0), (25.0, 26.5, 29.0)],
        source: "DIN 7721-2",
        kind: Kind::Din7721 { se: trap(2.96, true, 1.25, 25.0, 0.4, 0.6), n: trap(3.32, true, 1.95, 25.0, 0.4, 0.6), d: DIN_T5 },
    },
    Profile {
        key: "t10",
        title: "T10",
        pitch: 10.0,
        two_a: 2.0,
        min_teeth: 12,
        max_teeth: 0,
        flange_h: 2.2,
        widths: &[(16.0, 18.0, 21.0), (25.0, 27.0, 30.0), (32.0, 34.0, 37.0), (50.0, 52.0, 55.0)],
        source: "DIN 7721-2",
        kind: Kind::Din7721 { se: trap(6.02, true, 2.6, 25.0, 0.6, 0.8), n: trap(6.57, true, 3.4, 25.0, 0.6, 0.8), d: DIN_T10 },
    },
    Profile {
        key: "mxl",
        title: "MXL",
        pitch: 2.032,
        two_a: 0.508,
        min_teeth: 10,
        max_teeth: 0,
        flange_h: 0.5,
        widths: &[(3.2, 3.8, 5.6), (4.8, 5.3, 7.1), (6.4, 7.1, 8.9)],
        source: "ISO 5294, straight-sided grooves",
        kind: Kind::Iso5294(trap(0.84, false, 0.69, 20.0, 0.25, 0.13)),
    },
    Profile {
        key: "xl",
        title: "XL",
        pitch: 5.08,
        two_a: 0.508,
        min_teeth: 10,
        max_teeth: 0,
        flange_h: 1.0,
        widths: &[(6.4, 7.1, 8.9), (7.9, 8.6, 10.4), (9.5, 10.4, 12.2)],
        source: "ISO 5294, straight-sided grooves",
        kind: Kind::Iso5294(trap(1.32, false, 1.65, 25.0, 0.41, 0.64)),
    },
];

/// The profile the dialog calls `key`.
pub fn profile(key: &str) -> Option<&'static Profile> {
    PROFILES.iter().find(|p| p.key.eq_ignore_ascii_case(key))
}

impl Profile {
    /// The groove of a pulley with `z` grooves.
    pub fn groove(&self, z: u32) -> Groove {
        match &self.kind {
            Kind::Iso5294(t) => Groove::Trap(*t),
            Kind::Din7721 { se, n, .. } => Groove::Trap(if z <= 20 { *se } else { *n }),
            Kind::Htd { ranges, .. } => Groove::Htd(ranges.iter().rev().find(|(from, _)| z >= *from).unwrap_or(&ranges[0]).1),
            Kind::Gt(g) => Groove::Gt(*g),
        }
    }

    /// Which of the standard's grooves `z` grooves get, in words (for a tooltip).
    pub fn groove_name(&self, z: u32) -> String {
        match &self.kind {
            Kind::Din7721 { .. } => format!("groove form {} (DIN 7721-2: SE up to 20 grooves, N above)", if z <= 20 { "SE" } else { "N" }),
            Kind::Htd { ranges, .. } => {
                let i = ranges.iter().rposition(|(from, _)| z >= *from).unwrap_or(0);
                let to = ranges.get(i + 1).map_or(self.max_teeth, |r| r.0 - 1);
                format!("the ISO 13050 groove for {} to {to} grooves", ranges[i].0)
            }
            Kind::Iso5294(_) => "ISO 5294 straight-sided groove".into(),
            Kind::Gt(_) => "the shape of Gates' belt tooth (not standardised)".into(),
        }
    }

    /// Pitch diameter: z·p/π, except for DIN 7721's table.
    pub fn pitch_diameter(&self, z: u32) -> f64 {
        let base = |z: u32| z as f64 * self.pitch / PI;
        let Kind::Din7721 { d, .. } = &self.kind else { return base(z) };
        // the table's excess over z·p/π, interpolated between the tooth counts it lists
        let dev = |i: usize| d[i].1 - base(d[i].0);
        let i = d.iter().position(|(zz, _)| *zz >= z);
        let extra = match i {
            None => dev(d.len() - 1),
            Some(0) => dev(0),
            Some(i) if d[i].0 == z => dev(i),
            Some(i) => {
                let (z0, z1) = (d[i - 1].0 as f64, d[i].0 as f64);
                dev(i - 1) + (dev(i) - dev(i - 1)) * (z as f64 - z0) / (z1 - z0)
            }
        };
        base(z) + extra
    }

    /// Outside diameter: d − 2a, plus ISO 13050's chordal correction N'.
    pub fn outside_diameter(&self, z: u32) -> f64 {
        let n = match &self.kind {
            Kind::Htd { n_corr, .. } => n_corr.iter().find(|(zz, _)| *zz == z).map_or(0.0, |c| c.1),
            _ => 0.0,
        };
        self.pitch_diameter(z) - self.two_a + n
    }

    /// The groove's depth below the outside diameter.
    pub fn depth(&self, z: u32) -> f64 {
        match self.groove(z) {
            Groove::Trap(t) => t.depth,
            Groove::Htd(h) => h.hg,
            Groove::Gt(g) => g.h,
        }
    }
}

// ---------------------------------------------------------------- the groove outline

/// Roundings are kept at least this large when the clearance eats into them.
const MIN_R: f64 = 0.02;

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn unit(v: P) -> P {
    geom::scale(v, 1.0 / geom::len(v))
}

/// An arc of the circle centred at `c` from `a` to `b`, the shorter way round.
fn arc(c: P, a: P, b: P) -> Curve {
    let r = geom::dist(c, a);
    let mid = geom::add(c, geom::scale(unit(geom::add(unit(geom::sub(a, c)), unit(geom::sub(b, c)))), r));
    Curve::Arc(a, b, mid)
}

/// Where two circles meet: the point left of the line from `c0` to `c1` first.
fn circles_meet(c0: P, r0: f64, c1: P, r1: f64) -> Option<(P, P)> {
    let d = geom::dist(c0, c1);
    if d < 1e-12 || d > r0 + r1 || d < (r0 - r1).abs() {
        return None;
    }
    let a = (r0 * r0 - r1 * r1 + d * d) / (2.0 * d);
    let h = (r0 * r0 - a * a).max(0.0).sqrt();
    let e = geom::scale(geom::sub(c1, c0), 1.0 / d);
    let m = geom::add(c0, geom::scale(e, a));
    let perp = [-e[1], e[0]];
    Some((geom::add(m, geom::scale(perp, h)), geom::sub(m, geom::scale(perp, h))))
}

/// The rounding of radius `r` where a straight flank meets the outside circle (radius `r_o`):
/// its centre lies `r` inside that circle and `r` off the flank, on the side `n` points to (away
/// from the groove). The flank runs through `p0` along `u`, outward. Returns the rounding's
/// centre and the points where it meets the flank and the circle; the caller checks that the
/// flank reaches up to it.
fn edge_rounding(r_o: f64, p0: P, u: P, n: P, r: f64) -> Result<(P, P, P), String> {
    let q = geom::add(p0, geom::scale(n, r));
    let b = dot(q, u);
    let c = dot(q, q) - (r_o - r).powi(2);
    let disc = b * b - c;
    if disc < 0.0 {
        return Err("a flank misses the outside diameter".into());
    }
    let centre = geom::add(q, geom::scale(u, -b + disc.sqrt()));
    let on_flank = geom::sub(centre, geom::scale(n, r));
    let on_circle = geom::scale(centre, r_o / geom::len(centre));
    Ok((centre, on_flank, on_circle))
}

/// The +y half of a straight-sided groove, from the groove axis up to the outside circle. The
/// groove axis is +X; `d` is half the clearance.
fn trap_half(r_o: f64, g: &Trap, d: f64) -> Result<Vec<Curve>, String> {
    let phi = g.half_angle.to_radians();
    let (u, n) = ([phi.cos(), phi.sin()], [-phi.sin(), phi.cos()]);
    let half = g.width / 2.0;
    let f0 = if g.at_top { [(r_o * r_o - half * half).sqrt(), half] } else { [r_o - g.depth, half] };
    let f0 = geom::add(f0, geom::scale(n, d));
    let x_b = r_o - g.depth - d;
    // the bottom rounding: centre rb above the bottom and rb inside the flank
    let rb = g.rb + d;
    let cx = x_b + rb;
    let cy = f0[1] + (phi.sin() * (cx - f0[0]) - rb) / phi.cos();
    if cy < -1e-9 {
        return Err("the groove bottom is too narrow for its rounding".into());
    }
    let cy = cy.max(0.0);
    let (b1, b2) = ([x_b, cy], geom::add([cx, cy], geom::scale(n, rb)));
    let (ct, t1, t2) = edge_rounding(r_o, f0, u, n, (g.rt - d).max(MIN_R))?;
    if dot(geom::sub(t1, b2), u) <= 0.0 {
        return Err("the groove's roundings overlap".into());
    }
    let mut c = Vec::new();
    if cy > 1e-6 {
        c.push(Curve::Line([x_b, 0.0], b1));
    }
    c.push(arc([cx, cy], b1, b2));
    c.push(Curve::Line(b2, t1));
    c.push(arc(ct, t1, t2));
    Ok(c)
}

/// The +y half of an HTD groove (see the module doc).
fn htd_half(r_o: f64, g: &Htd, d: f64) -> Result<Vec<Curve>, String> {
    let phi = g.phi.to_radians();
    let (u, n) = ([phi.cos(), phi.sin()], [-phi.sin(), phi.cos()]);
    let x_b = r_o - g.hg - d;
    let r1 = g.r1 + d;
    let c1 = [x_b + r1, g.x];
    // where the arc's tangent leans out by ϕ: there the straight flank starts
    let t = geom::add(c1, geom::scale(n, r1));
    let (ct, t1, t2) = edge_rounding(r_o, t, u, n, (g.rt - d).max(MIN_R))?;
    if dot(geom::sub(t1, t), u) <= 0.0 {
        return Err("the groove's roundings overlap".into());
    }
    let mut c = Vec::new();
    if g.x > 1e-6 {
        c.push(Curve::Line([x_b, 0.0], [x_b, g.x]));
    }
    c.push(arc(c1, [x_b, g.x], t));
    c.push(Curve::Line(t, t1));
    c.push(arc(ct, t1, t2));
    Ok(c)
}

/// The +y half of a GT2 groove: the belt tooth's tip arc, its flank arc (centred on the outside
/// circle, where the belt's land lies, and touching the tip arc) and the land rounding.
fn gt_half(r_o: f64, g: &Gt, d: f64) -> Result<Vec<Curve>, String> {
    let (rt, rf, rl) = (g.r_tip + d, g.r_flank + d, (g.r_land - d).max(MIN_R));
    let ct = [r_o - g.h + g.r_tip, 0.0];
    // the +y flank's centre is on the -y side: the flank bulges outward
    let (_, cf) = circles_meet(ct, g.r_flank - g.r_tip, [0.0, 0.0], r_o).ok_or("the belt tooth does not fit this pulley")?;
    let cf = if cf[1] < 0.0 { cf } else { [cf[0], -cf[1]] };
    let tp = geom::add(cf, geom::scale(unit(geom::sub(ct, cf)), rf));
    let (a, b) = circles_meet(cf, rf + rl, [0.0, 0.0], r_o - rl).ok_or("the land rounding does not fit")?;
    let f = if a[1] > b[1] { a } else { b };
    let f1 = geom::add(cf, geom::scale(unit(geom::sub(f, cf)), rf));
    let f2 = geom::scale(f, r_o / geom::len(f));
    Ok(vec![arc(ct, [ct[0] - rt, 0.0], tp), arc(cf, tp, f1), arc(f, f1, f2)])
}

/// One groove centred on +X, from where it leaves the outside circle on the -y side to where it
/// meets it again on +y: the half mirrored, then the half, joined on the axis.
pub fn groove_curves(r_o: f64, g: &Groove, clearance: f64) -> Result<Vec<Curve>, String> {
    let d = clearance / 2.0;
    let right = match g {
        Groove::Trap(t) => trap_half(r_o, t, d)?,
        Groove::Htd(h) => htd_half(r_o, h, d)?,
        Groove::Gt(t) => gt_half(r_o, t, d)?,
    };
    let mirror = |p: P| [p[0], -p[1]];
    let mut c: Vec<Curve> = right.iter().rev().map(|c| c.map(&mirror).reversed()).collect();
    // the two halves meet on the axis; a line or arc across it becomes one curve
    let (last, first) = (c.pop().expect("a half has curves"), &right[0]);
    let joined = match (&last, first) {
        (Curve::Line(a, _), Curve::Line(_, b)) => Some(Curve::Line(*a, *b)),
        (Curve::Arc(a, q, _), Curve::Arc(_, b, _)) => Some(Curve::Arc(*a, *b, *q)),
        _ => None,
    };
    match joined {
        Some(j) => {
            c.push(j);
            c.extend(right[1..].iter().cloned());
        }
        None => {
            c.push(last);
            c.extend(right.iter().cloned());
        }
    }
    Ok(c)
}

/// One period of the pulley's outline: groove 0 (centred on +X) and the land after it, up to
/// where groove 1 starts, so consecutive periods meet end to start.
pub fn period(p: &Profile, z: u32, clearance: f64) -> Result<Vec<Curve>, String> {
    let r_o = p.outside_diameter(z) / 2.0;
    let mut c = groove_curves(r_o, &p.groove(z), clearance)?;
    let (start, end) = (c[0].start(), c.last().expect("a groove").end());
    let pitch = 2.0 * PI / z as f64;
    let half_groove = geom::angle(end);
    // small pulleys keep little land once the roundings are in (T2.5, 10 grooves: 0.08 mm)
    if r_o * (pitch - 2.0 * half_groove) < 0.01 {
        return Err("the grooves leave no land between them: use less clearance".into());
    }
    c.push(Curve::Arc(end, geom::rot(start, pitch), polar(r_o, pitch / 2.0)));
    Ok(c)
}

/// The pulley's cross-section area (outline without flanges), exactly.
pub fn section_area(p: &Profile, z: u32, clearance: f64) -> Result<f64, String> {
    Ok(z as f64 * period(p, z, clearance)?.iter().map(Curve::area_term).sum::<f64>())
}

// ---------------------------------------------------------------- the drive

/// Belt pitch length of two pulleys with pitch radii `r1`, `r2` at centre distance `c`.
pub fn belt_length(r1: f64, r2: f64, c: f64) -> f64 {
    let (big, small) = if r1 >= r2 { (r1, r2) } else { (r2, r1) };
    let s = ((big - small) / c).asin();
    2.0 * c * s.cos() + PI * (big + small) + 2.0 * s * (big - small)
}

/// The centre distance at which a belt of pitch length `l` fits round pulleys of pitch radii `r1`
/// and `r2`, if it is long enough to go round them at all.
pub fn centre_distance(r1: f64, r2: f64, l: f64) -> Option<f64> {
    let lo0 = (r1 - r2).abs() + 1e-9;
    if l <= belt_length(r1, r2, lo0) {
        return None;
    }
    let (mut lo, mut hi) = (lo0, l / 2.0);
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if belt_length(r1, r2, mid) < l {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(0.5 * (lo + hi))
}

/// The angle the belt wraps round a pulley of pitch radius `r`, the other having `r_other`, at
/// centre distance `c` (radians).
pub fn wrap(r: f64, r_other: f64, c: f64) -> f64 {
    PI + 2.0 * ((r - r_other) / c).asin()
}

// ---------------------------------------------------------------- one pulley

#[derive(Clone, Debug)]
pub struct PulleySpec {
    pub profile: &'static Profile,
    pub z: u32,
    /// Toothed width.
    pub width: f64,
    pub clearance: f64,
    /// Flanges: 0, 1 (below the teeth) or 2.
    pub flanges: u32,
    pub flange_d: f64,
    pub flange_t: f64,
    /// Body name; the features are named after it.
    pub name: String,
}

impl PulleySpec {
    pub fn outside_diameter(&self) -> f64 {
        self.profile.outside_diameter(self.z)
    }
    /// The outermost radius: the flanges', or the outside diameter's.
    pub fn r_max(&self) -> f64 {
        if self.flanges > 0 {
            self.flange_d / 2.0
        } else {
            self.outside_diameter() / 2.0
        }
    }
    /// Height of the whole pulley, flanges included.
    pub fn height(&self) -> f64 {
        self.width + self.flanges as f64 * self.flange_t
    }
}

/// Rough build time in seconds: the sketch grows with the square of the groove count.
pub fn est_seconds(s: &PulleySpec) -> f64 {
    let z = s.z as f64;
    0.4 + 0.0005 * z * z + 0.3 * s.flanges as f64
}

/// Everything that can be checked without the engine. Fields: `teeth`, `groove_clearance`.
pub fn check(s: &PulleySpec) -> Result<(), Bad> {
    let p = s.profile;
    if s.z < p.min_teeth {
        return Err(
            bad("teeth", format!("{} pulleys need at least {} grooves", p.title, p.min_teeth)).short(format!("≥ {} grooves", p.min_teeth))
        );
    }
    if p.max_teeth > 0 && s.z > p.max_teeth {
        return Err(bad("teeth", format!("{} grooves are given for up to {} grooves", p.source, p.max_teeth))
            .short(format!("≤ {} grooves", p.max_teeth)));
    }
    let est = est_seconds(s);
    if est > cad::TIME_BUDGET_S {
        return Err(bad("teeth", cad::too_slow("this pulley", est, "use fewer grooves")).short(cad::too_slow_short(est)));
    }
    period(p, s.z, s.clearance).map_err(|e| bad("groove_clearance", e))?;
    Ok(())
}

pub struct Built {
    pub body: String,
    pub info: Value,
}

/// Build one pulley on the Z axis: its lower flange from z = 0, then the toothed part (one sketch
/// of the whole outline, extruded), then the upper flange.
pub fn build(cad: &mut Cad, s: &PulleySpec) -> Result<Built, String> {
    let z = s.z;
    let name = &s.name;
    let below = if s.flanges > 0 { s.flange_t } else { 0.0 };
    let period = period(s.profile, z, s.clearance)?;
    let sk = cad.sketch_xy(below, &format!("{name} profile"))?;
    cad.loops_circular(&period, z as usize, 2.0 * PI / z as f64, 0.0)?;
    cad.finish()?;
    cad.host.progress(0.5, "making the grooves");
    let v = cad.extrude_new(sk, s.width, &format!("{name} teeth"), name)?;
    let body = created_body(&v, name);
    let r_f = s.flange_d / 2.0;
    if s.flanges > 0 {
        cad.cylinder_at(0.0, r_f, s.flange_t, "join", Some(&body), &format!("{name} flange"), None)?;
    }
    if s.flanges > 1 {
        cad.cylinder_at(below + s.width, r_f, s.flange_t, "join", Some(&body), &format!("{name} upper flange"), None)?;
    }
    let p = s.profile;
    let mut info = json!({
        "body": body,
        "grooves": z,
        "pitch_diameter": r9(p.pitch_diameter(z)),
        "outside_diameter": r9(p.outside_diameter(z)),
        "groove_depth": r9(p.depth(z) + s.clearance / 2.0),
        "width": r9(s.width),
        "height": r9(s.height()),
    });
    if s.flanges > 0 {
        info["flange_diameter"] = json!(r9(s.flange_d));
    }
    Ok(Built { body, info })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::loop_gap;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    /// Points along the curves, lines included (`loop_polygon` keeps only their ends).
    fn dense(curves: &[Curve]) -> Vec<P> {
        let mut out = Vec::new();
        for c in curves {
            match c {
                Curve::Line(a, b) => out.extend((0..=200).map(|i| geom::add(*a, geom::scale(geom::sub(*b, *a), i as f64 / 200.0)))),
                _ => out.extend(c.sample(400)),
            }
        }
        out
    }

    #[test]
    fn diameters_match_the_standards_tables() {
        // ISO 5294 Table 5
        let mxl = profile("mxl").unwrap();
        for (z, d, d_o) in [(10, 6.47, 5.96), (16, 10.35, 9.84)] {
            assert!(close(mxl.pitch_diameter(z), d, 0.006) && close(mxl.outside_diameter(z), d_o, 0.006), "MXL {z}");
        }
        let xl = profile("xl").unwrap();
        for (z, d, d_o) in [(10, 16.17, 15.66), (13, 21.02, 20.51)] {
            assert!(close(xl.pitch_diameter(z), d, 0.006) && close(xl.outside_diameter(z), d_o, 0.006), "XL {z}");
        }
        // ISO 13050 Table 15, N' included
        for (key, z, d, d_o) in [
            ("htd-3m", 14, 13.37, 12.61),
            ("htd-3m", 90, 85.94, 85.18),
            ("htd-5m", 20, 31.83, 30.69),
            ("htd-5m", 120, 190.99, 189.84),
            ("htd-8m", 22, 56.02, 54.65),
            ("htd-8m", 28, 71.30, 70.08),
            ("htd-8m", 30, 76.39, 75.13),
            ("htd-8m", 40, 101.86, 100.49),
        ] {
            let p = profile(key).unwrap();
            assert!(close(p.pitch_diameter(z), d, 0.006), "{key} {z}: d {}", p.pitch_diameter(z));
            assert!(close(p.outside_diameter(z), d_o, 0.011), "{key} {z}: do {}", p.outside_diameter(z));
        }
        // DIN 7721-2 Table 4: every listed size exactly, others between
        for key in ["t2.5", "t5", "t10"] {
            let p = profile(key).unwrap();
            let Kind::Din7721 { d, .. } = &p.kind else { panic!() };
            for (z, dd) in d.iter() {
                assert!(close(p.pitch_diameter(*z), *dd, 1e-9), "{key} {z}");
                assert!(close(p.outside_diameter(*z), dd - p.two_a, 1e-9));
            }
        }
        let t5 = profile("t5").unwrap();
        assert!(close(t5.outside_diameter(10), 15.05, 1e-9));
        let d21 = t5.pitch_diameter(21);
        assert!(d21 > 32.0 && d21 < 35.25, "{d21}");
        assert!(t5.pitch_diameter(120) > 120.0 * 5.0 / PI);
    }

    #[test]
    fn grooves_follow_the_tables() {
        let t5 = profile("t5").unwrap();
        assert!(matches!(t5.groove(20), Groove::Trap(Trap { depth, .. }) if depth == 1.25), "SE up to 20");
        assert!(matches!(t5.groove(21), Groove::Trap(Trap { depth, .. }) if depth == 1.95), "N above");
        let h5 = profile("htd-5m").unwrap();
        for (z, hg) in [(12, 1.989), (16, 1.989), (17, 2.009), (25, 2.009), (26, 2.052), (81, 2.056), (200, 2.056)] {
            assert!(matches!(h5.groove(z), Groove::Htd(h) if h.hg == hg), "{z}");
        }
        assert_eq!(h5.groove_name(30), "the ISO 13050 groove for 26 to 80 grooves");
    }

    /// The outline of every profile closes, its grooves have the standard's depth and width,
    /// and its land is a real one, for the smallest and some large pulleys.
    #[test]
    fn outlines_are_closed_and_have_the_standard_shape() {
        for p in PROFILES {
            for z in [
                p.min_teeth,
                p.min_teeth + 1,
                20.max(p.min_teeth),
                21.max(p.min_teeth),
                60,
                if p.max_teeth > 0 { p.max_teeth } else { 250 },
            ] {
                for clearance in [0.0, 0.15] {
                    let per = period(p, z, clearance).unwrap_or_else(|e| panic!("{} z{z}: {e}", p.key));
                    // consecutive periods meet end to start
                    let next = per[0].rotated(2.0 * PI / z as f64);
                    assert!(geom::dist(per.last().unwrap().end(), next.start()) < 1e-9, "{} z{z}", p.key);
                    assert!(loop_gap(&per[..per.len() - 1]) > 0.0); // an open chain...
                    for w in per.windows(2) {
                        assert!(geom::dist(w[0].end(), w[1].start()) < 1e-9, "{} z{z}: a gap", p.key);
                    }
                    // depth: the deepest point is hg (+ half the clearance) below the outside diameter
                    let r_o = p.outside_diameter(z) / 2.0;
                    let pts = dense(&per);
                    let r_min = pts.iter().map(|q| geom::len(*q)).fold(f64::MAX, f64::min);
                    let want = p.depth(z) + clearance / 2.0;
                    assert!(close(r_o - r_min, want, 2e-3), "{} z{z}: depth {} not {want}", p.key, r_o - r_min);
                    // nothing outside the outside diameter
                    let r_max = pts.iter().map(|q| geom::len(*q)).fold(0.0, f64::max);
                    assert!(r_max <= r_o + 1e-9, "{} z{z}", p.key);
                }
            }
        }
    }

    #[test]
    fn groove_widths_are_the_standards() {
        // DIN 7721-2 T5, form N: br = 3.32 at the outside diameter (between the flank lines)
        let t5 = profile("t5").unwrap();
        let z = 40;
        let r_o = t5.outside_diameter(z) / 2.0;
        let half = trap_half(
            r_o,
            &match t5.groove(z) {
                Groove::Trap(t) => t,
                _ => panic!(),
            },
            0.0,
        )
        .unwrap();
        // the flank line, extended, meets the outside circle at y = br/2
        let Curve::Line(a, b) = half[2] else { panic!("{half:?}") };
        let u = unit(geom::sub(b, a));
        let t = {
            let (bq, c) = (dot(a, u), dot(a, a) - r_o * r_o);
            -bq + (bq * bq - c).sqrt()
        };
        let top = geom::add(a, geom::scale(u, t));
        assert!(close(top[1], 3.32 / 2.0, 1e-9), "{top:?}");
        // ISO 5294 XL: bw = 1.32 at the bottom, between the flank lines
        let xl = profile("xl").unwrap();
        let r_o = xl.outside_diameter(20) / 2.0;
        let Groove::Trap(g) = xl.groove(20) else { panic!() };
        let half = trap_half(r_o, &g, 0.0).unwrap();
        let Curve::Line(a, b) = half[2] else { panic!() };
        let u = unit(geom::sub(b, a));
        let k = (r_o - g.depth - a[0]) / u[0];
        assert!(close(a[1] + k * u[1], 0.66, 1e-9));
        // GT2: the groove is the belt tooth, 1.2 mm wide where the flanks meet the land line
        let gt = profile("gt2-2").unwrap();
        let r_o = gt.outside_diameter(40) / 2.0;
        let per = period(gt, 40, 0.0).unwrap();
        let Curve::Arc(_, _, _) = per[0] else { panic!() };
        let widest = crate::curve::loop_polygon(&per[..per.len() - 1]).iter().map(|q| q[1].abs()).fold(0.0, f64::max);
        assert!(widest > 0.6 && widest < 0.8, "{widest}");
        assert!(r_o > 0.0);
    }

    #[test]
    fn clearance_takes_area_and_keeps_the_outside() {
        for p in PROFILES {
            let a0 = section_area(p, 30.max(p.min_teeth), 0.0).unwrap();
            let a1 = section_area(p, 30.max(p.min_teeth), 0.2).unwrap_or_else(|e| panic!("{}: {e}", p.key));
            let r_o = p.outside_diameter(30.max(p.min_teeth)) / 2.0;
            assert!(a1 < a0 && a0 < PI * r_o * r_o, "{}", p.key);
        }
        // far too much clearance leaves no land
        assert!(period(profile("t2.5").unwrap(), 10, 0.5).is_err());
    }

    #[test]
    fn area_is_the_outline_area() {
        // against the sampled polygon of the whole outline
        let p = profile("htd-5m").unwrap();
        let z = 24;
        let per = period(p, z, 0.1).unwrap();
        let mut all = Vec::new();
        for k in 0..z {
            all.extend(per.iter().map(|c| c.rotated(2.0 * PI * k as f64 / z as f64)));
        }
        let poly = crate::curve::loop_polygon(&all);
        let exact = section_area(p, z, 0.1).unwrap();
        assert!(close(geom::polygon_area(&poly).abs(), exact, 1e-3 * exact), "{exact}");
    }

    #[test]
    fn centre_distance_and_wrap() {
        // equal pulleys: L = 2C + πd
        let c = centre_distance(10.0, 10.0, 2.0 * 50.0 + PI * 20.0).unwrap();
        assert!(close(c, 50.0, 1e-9));
        let c = centre_distance(6.0, 12.0, 200.0).unwrap();
        assert!(close(belt_length(6.0, 12.0, c), 200.0, 1e-9));
        assert!(wrap(6.0, 12.0, c) < PI && wrap(12.0, 6.0, c) > PI);
        assert!(centre_distance(6.0, 12.0, 50.0).is_none());
    }
}
