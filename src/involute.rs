//! Transverse tooth profiles of cylindrical involute gears (spur and helical), external and
//! internal, written from the textbook definitions.
//!
//! External teeth are generated the way a hob or rack cutter generates them: the involute flank is
//! the envelope of the cutter's straight flank, and the root is the envelope of the cutter's tip
//! rounding (a trochoid offset). So the root fillet is the real generated one, and small tooth
//! counts come out undercut just as they would on a machine.
//!
//! Conventions: millimetres and radians. Helical gears are described in the transverse plane
//! (`mt`, `alpha_t`); module and pressure angle given by the user are normal values. A tooth is
//! centred on +X; a "half-angle" θ(r) is the angular distance from the tooth centre line to the
//! flank at radius r.

use crate::curve::Curve;
use crate::geom::{self, inv, polar, P};
use std::f64::consts::PI;

/// The tooth of a cylindrical gear: everything its profile depends on.
#[derive(Clone, Copy, Debug)]
pub struct Tooth {
    /// Number of teeth.
    pub z: u32,
    /// Normal module (mm).
    pub mn: f64,
    /// Normal pressure angle (rad).
    pub alpha_n: f64,
    /// Helix angle (rad, >= 0; the hand is applied when the gear is built).
    pub beta: f64,
    /// Profile shift coefficient (internal gears: positive outward, see `pair`).
    pub x: f64,
    /// Addendum, dedendum and cutter tip radius, as multiples of the normal module.
    pub ha: f64,
    pub hf: f64,
    pub rho: f64,
    /// Circular backlash taken off this gear's tooth (transverse, on the pitch circle, mm).
    pub backlash: f64,
}

impl Tooth {
    /// A spur gear with the DIN 867 basic rack and no shift or backlash.
    pub fn standard(z: u32, mn: f64, alpha_n_deg: f64) -> Tooth {
        Tooth { z, mn, alpha_n: alpha_n_deg.to_radians(), beta: 0.0, x: 0.0, ha: 1.0, hf: 1.25, rho: 0.38, backlash: 0.0 }
    }
    /// The tooth count as a float.
    pub fn zf(&self) -> f64 {
        self.z as f64
    }
    /// Transverse module.
    pub fn mt(&self) -> f64 {
        self.mn / self.beta.cos()
    }
    /// Transverse pressure angle.
    pub fn alpha_t(&self) -> f64 {
        (self.alpha_n.tan() / self.beta.cos()).atan()
    }
    /// Pitch (reference) radius.
    pub fn rp(&self) -> f64 {
        self.mt() * self.zf() / 2.0
    }
    /// Base radius.
    pub fn rb(&self) -> f64 {
        self.rp() * self.alpha_t().cos()
    }
    /// External gear: tip radius.
    pub fn ra(&self) -> f64 {
        self.rp() + self.mn * (self.ha + self.x)
    }
    /// External gear: root radius.
    pub fn rf(&self) -> f64 {
        self.rp() - self.mn * (self.hf - self.x)
    }
    /// External gear: transverse tooth thickness on the pitch circle.
    pub fn st(&self) -> f64 {
        PI * self.mt() / 2.0 + 2.0 * self.x * self.mn * self.alpha_t().tan() - self.backlash
    }
    /// The rounding of the rack cutter's tip (mm) as it generates this gear: the asked one, but
    /// smaller where the cutter tip is narrow, so a short flat stays between the two roundings
    /// and the root keeps a circular arc. `None` if the cutter tip comes to a point.
    pub fn cutter_tip_radius(&self) -> Option<f64> {
        let at = self.alpha_t();
        // the cutter tooth's half-width at its tip line, the same for any profile shift
        let w_tip = (PI * self.mt() / 2.0 + self.backlash) / 2.0 - self.hf * self.mn * at.tan();
        if w_tip <= 0.0 {
            return None;
        }
        let rho_max = w_tip * at.cos() / (1.0 - at.sin());
        Some((self.rho * self.mn).min(0.95 * rho_max).max(0.0))
    }
    /// External gear: half-angle of the involute flank at radius r (r >= rb).
    pub fn theta_inv(&self, r: f64) -> f64 {
        let a = (self.rb() / r).min(1.0).acos();
        self.st() / (2.0 * self.rp()) + inv(self.alpha_t()) - inv(a)
    }
    /// Internal gear: tip (inner) radius. A positive shift moves tip and root outward and widens
    /// the tooth spaces.
    pub fn ra_int(&self) -> f64 {
        self.rp() - self.mn * (self.ha - self.x)
    }
    /// Internal gear: root (outer) radius.
    pub fn rf_int(&self) -> f64 {
        self.rp() + self.mn * (self.hf + self.x)
    }
    /// Internal gear: transverse space width on the pitch circle.
    pub fn et_int(&self) -> f64 {
        PI * self.mt() / 2.0 + 2.0 * self.x * self.mn * self.alpha_t().tan() + self.backlash
    }
    /// Internal gear: half-angle of a tooth space at radius r (r >= rb).
    pub fn psi_space(&self, r: f64) -> f64 {
        let a = (self.rb() / r).min(1.0).acos();
        self.et_int() / (2.0 * self.rp()) + inv(self.alpha_t()) - inv(a)
    }
}

/// The generated upper flank of an external tooth.
#[derive(Clone, Debug)]
pub struct ExtFlank {
    /// Root fillet as (r, θ) from the root circle up to the start of the involute, sorted by r.
    pub fillet: Vec<(f64, f64)>,
    /// Radius where the fillet hands over to the involute.
    pub r_t: f64,
    /// θ where the fillet leaves the root circle.
    pub theta_b: f64,
    /// Cutter tip radius actually used (mm).
    pub rho: f64,
    pub undercut: bool,
}

/// Generate the flank of an external tooth with a rack cutter. Fails when the tooth cannot be
/// made (no thickness, pointed, undercut all the way up), with the reason in the user's words.
pub fn external_flank(t: &Tooth) -> Result<ExtFlank, String> {
    let z = t.zf();
    let mt = t.mt();
    let at = t.alpha_t();
    let (rp, rb, ra, rf) = (t.rp(), t.rb(), t.ra(), t.rf());
    let st = t.st();
    if st <= 0.0 {
        return Err("the tooth has no thickness: reduce the backlash".into());
    }
    // Rack cutter (in the transverse plane): u along the rack, v away from the gear centre, v = 0
    // on the rolling line, which touches the pitch circle. The cutter tooth cuts the gear's space.
    let w0 = (PI * mt - st) / 2.0; // cutter half-width on the rolling line
    let v_tip = rf - rp; // cutter tip line
    let mut rho = t.cutter_tip_radius().ok_or("the cutter tip would be pointed: lower the dedendum or the pressure angle")?;
    if (v_tip + rho).abs() < 1e-6 * t.mn {
        // a centre exactly on the rolling line would cut with the whole rounding at one instant
        rho *= 0.999;
    }
    let c_v = v_tip + rho; // the rounding's centre: below the rolling line, or above it for a large shift
    let v1 = v_tip + rho * (1.0 - at.sin());
    let c_u = w0 + v1 * at.tan() - rho * at.cos();
    // Rolling: the gear turns by φ while the rack moves rp·φ. A rack point (u, v) is at
    // S = (rp + v, u + rp φ) in space and at R(-φ)·S on the gear. The cutter's +u side cuts the
    // lower flank of the tooth at angle +π/z; its half-angle is π/z minus the point's angle.
    let to_gear = |u: f64, v: f64, phi: f64| -> (f64, f64) {
        let (sx, sy) = (rp + v, u + rp * phi);
        let (s, c) = phi.sin_cos();
        let (gx, gy) = (sx * c + sy * s, -sx * s + sy * c);
        ((gx * gx + gy * gy).sqrt(), PI / z - gy.atan2(gx))
    };
    // The rounding cuts from its lowest point (at φ_b, on the tip line) round to where it meets
    // the flank (at φ_j); with its centre above the rolling line φ runs backwards.
    let phi_b = -c_u / rp;
    let phi_j = (-c_v / at.tan() - c_u) / rp;
    let n = 400;
    let mut fil: Vec<(f64, f64)> = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let phi = phi_b + (phi_j - phi_b) * i as f64 / n as f64;
        // Contact on the rounding: its normal passes through the pitch point I = (-rp φ, 0), so
        // it is the point of the line through the centre and I on the cutting (lower) side.
        let (du, dv) = (c_u + rp * phi, c_v);
        let l = (du * du + dv * dv).sqrt();
        let k = if c_v < 0.0 { rho / l } else { -rho / l };
        fil.push(to_gear(c_u + k * du, c_v + k * dv, phi));
    }
    let theta_b = fil[0].1;
    let r_j = fil[n].0.max(rb);
    // The straight cutter flank makes the true involute only where it touches the gear between
    // the pitch point and the interference point T (rack depth rp·sin²α); deeper down it cuts
    // into the tooth instead (undercut), and the involute then starts at the base circle.
    let inv_start = if -v1 > rp * at.sin().powi(2) { rb } else { r_j };
    let r_fmax = fil.iter().map(|p| p.0).fold(0.0, f64::max);
    // Smallest half-angle the rounding leaves at radius r (it may pass a radius more than once).
    let by_r = ByRadius::new(&fil);
    let fil_min = |r: f64| by_r.min_at(r);
    // Undercut: the rounding cuts into the involute above its start.
    let cuts = |r: f64| fil_min(r).is_some_and(|f| f < t.theta_inv(r) - 1e-12);
    let mut r_t = r_j;
    let mut undercut = false;
    if r_fmax > inv_start + 1e-9 {
        let steps = 400;
        let mut last_cut: Option<usize> = None;
        for k in 0..=steps {
            let r = inv_start + (r_fmax - inv_start) * k as f64 / steps as f64;
            if cuts(r) {
                last_cut = Some(k);
            }
        }
        if let Some(k) = last_cut {
            undercut = true;
            let h = (r_fmax - inv_start) / steps as f64;
            let (mut lo, mut hi) = (inv_start + h * k as f64, (inv_start + h * (k + 1) as f64).min(r_fmax));
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                if cuts(mid) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            r_t = 0.5 * (lo + hi);
        }
    }
    if r_t >= ra - 1e-6 {
        return Err("the teeth would be undercut all the way up: use more teeth or a positive profile shift".into());
    }
    // The fillet part of the boundary: the generated points that are the deepest at their radius.
    let mut pts: Vec<(f64, f64)> = fil
        .iter()
        .cloned()
        .filter(|&(r, th)| r <= r_t + 1e-12 && fil_min(r).is_none_or(|m| th <= m + 1e-9) && (r < inv_start || th <= t.theta_inv(r) + 1e-9))
        .collect();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    pts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-12);
    if let Some(last) = pts.last() {
        if (last.0 - r_t).abs() < 1e-9 {
            pts.pop();
        }
    }
    pts.push((r_t, t.theta_inv(r_t)));
    if pts.len() < 2 {
        return Err("could not build the root fillet".into());
    }
    let tip = t.theta_inv(ra);
    if tip * ra * 2.0 < 0.05 * t.mn {
        return Err("the teeth come to a point: lower the profile shift or the addendum".into());
    }
    Ok(ExtFlank { fillet: pts, r_t, theta_b, rho, undercut })
}

/// A polyline in (r, θ), split into runs along which r only grows or only shrinks, so the
/// smallest θ at a radius takes one binary search per run instead of a pass over every segment.
/// (The dialog hook builds every profile on each keystroke, within 10⁷ fuel; see the README.)
struct ByRadius<'a> {
    pts: &'a [(f64, f64)],
    /// First and last index of each run.
    runs: Vec<(usize, usize)>,
}

impl<'a> ByRadius<'a> {
    fn new(pts: &'a [(f64, f64)]) -> ByRadius<'a> {
        let mut runs = Vec::new();
        let (mut start, mut dir) = (0, 0i8);
        for i in 1..pts.len() {
            let step = pts[i].0 - pts[i - 1].0;
            let d = if step > 0.0 {
                1
            } else if step < 0.0 {
                -1
            } else {
                0
            };
            if d != 0 {
                if dir != 0 && d != dir {
                    runs.push((start, i - 1));
                    start = i - 1;
                }
                dir = d;
            }
        }
        runs.push((start, pts.len() - 1));
        ByRadius { pts, runs }
    }

    /// The smallest θ of the polyline at radius r, interpolated, or `None` if it does not reach r.
    fn min_at(&self, r: f64) -> Option<f64> {
        let mut best: Option<f64> = None;
        for &(a, b) in &self.runs {
            let run = &self.pts[a..=b];
            let (first, last) = (run[0].0, run[run.len() - 1].0);
            if r < first.min(last) || r > first.max(last) {
                continue;
            }
            // the first point at or past r along the run
            let k = if first <= last { run.partition_point(|p| p.0 < r) } else { run.partition_point(|p| p.0 > r) };
            let th = match k {
                0 => run[0].1,
                k if k == run.len() => run[k - 1].1,
                k => {
                    let (p, q) = (run[k - 1], run[k]);
                    if q.0 == p.0 {
                        p.1.min(q.1)
                    } else {
                        p.1 + (q.1 - p.1) * (r - p.0) / (q.0 - p.0)
                    }
                }
            };
            best = Some(best.map_or(th, |b: f64| b.min(th)));
        }
        best
    }
}

/// Involute samples (r, θ) between two radii, evenly spread in the roll angle.
fn involute_samples(t: &Tooth, r0: f64, r1: f64, n: usize, theta: &dyn Fn(f64) -> f64) -> Vec<(f64, f64)> {
    let rb = t.rb();
    let roll = |r: f64| ((r / rb).powi(2) - 1.0).max(0.0).sqrt();
    let (u0, u1) = (roll(r0), roll(r1));
    (0..n)
        .map(|i| {
            let r = if i == 0 {
                r0
            } else if i == n - 1 {
                r1
            } else {
                // evenly in the roll angle: denser near the base circle, where the involute bends most
                let s = i as f64 / (n - 1) as f64;
                rb * (1.0 + (u0 + (u1 - u0) * s).powi(2)).sqrt()
            };
            (r, theta(r))
        })
        .collect()
}

/// How finely a profile is drawn: spline fit points per fillet and per involute.
#[derive(Clone, Copy, Debug)]
pub struct Detail {
    pub fillet: usize,
    pub flank: usize,
}

impl Detail {
    /// For sketches of gears up to 60 teeth.
    pub const SKETCH: Detail = Detail { fillet: 6, flank: 8 };
    /// For the exact areas the tests compare the built solids with.
    pub const DENSE: Detail = Detail { fillet: 60, flank: 120 };

    /// Fit points for a gear of `z` teeth. Big gears have nearly straight, short flanks, and
    /// every curve point costs sketch time, which grows with the tooth count.
    pub fn for_teeth(z: u32) -> Detail {
        match z {
            0..=60 => Detail::SKETCH,
            61..=120 => Detail { fillet: 5, flank: 6 },
            _ => Detail { fillet: 4, flank: 5 },
        }
    }
}

/// One tooth space of an external gear, between tooth 0 (centred on +X) and tooth 1, as a closed
/// loop reaching from the root circle out to `r_out` (just past the tip circle).
pub fn external_gap(t: &Tooth, f: &ExtFlank, r_out: f64, d: Detail) -> Vec<Curve> {
    let g = PI / t.zf(); // gap centre
    let up = |(r, th): (f64, f64)| polar(r, th); // tooth 0, upper flank
    let lo = |(r, th): (f64, f64)| polar(r, 2.0 * g - th); // tooth 1, lower flank
    let fil_xy: Vec<P> = f.fillet.iter().map(|&p| up(p)).collect();
    let fil = geom::resample(&fil_xy, d.fillet);
    // back to (r, θ) to mirror exactly
    let fil_rt: Vec<(f64, f64)> = fil.iter().map(|p| (geom::len(*p), geom::angle(*p))).collect();
    let inv = involute_samples(t, f.r_t, r_out, d.flank, &|r| t.theta_inv(r));
    let mut c = Vec::new();
    c.push(Curve::Spline(inv.iter().rev().map(|&p| up(p)).collect()));
    c.push(Curve::Spline(fil_rt.iter().rev().map(|&p| up(p)).collect()));
    let rf = f.fillet[0].0;
    if f.theta_b < g - 1e-9 {
        c.push(Curve::Arc(polar(rf, f.theta_b), polar(rf, 2.0 * g - f.theta_b), polar(rf, g)));
    }
    c.push(Curve::Spline(fil_rt.iter().map(|&p| lo(p)).collect()));
    c.push(Curve::Spline(inv.iter().map(|&p| lo(p)).collect()));
    let th_out = t.theta_inv(r_out);
    c.push(Curve::Arc(polar(r_out, 2.0 * g - th_out), polar(r_out, th_out), polar(r_out, g)));
    c
}

/// A fillet circle in the corner between the circle |p| = rc and a flank.
#[derive(Clone, Copy, Debug)]
pub struct CornerFillet {
    pub center: P,
    pub radius: f64,
    pub on_flank: P,
    pub on_circle: P,
    pub mid: P,
}

/// Round the corner between the circle |p| = `rc` and the flank curve `flank` (dense polyline).
/// The fillet sits on the side of the flank toward angle `a_open` (the middle of the space).
/// `outside`: the circle bounds the space from inside (an external root, centre at rc + ρ);
/// otherwise from outside (an internal root, centre at rc - ρ). The radius shrinks if the
/// space is too narrow. Returns `None` when even a tiny fillet does not fit.
pub fn corner_fillet(flank: &[P], rc: f64, rho: f64, outside: bool, a_open: f64) -> Option<CornerFillet> {
    // The closest flank point to a fillet centre, if within `reach` of it. A flank that runs
    // outward (radius growing point by point, as both callers' do) is searched only where its
    // radius is within reach (plus one segment length) of the centre's: the bisection below asks
    // this 54 times per try, and the dialog hook has a tight fuel budget (see `ByRadius`).
    let radii: Vec<f64> = flank.iter().map(|p| geom::len(*p)).collect();
    let outward = radii.windows(2).all(|w| w[0] <= w[1]);
    let step = flank.windows(2).map(|w| geom::dist(w[0], w[1])).fold(0.0, f64::max);
    let closest = |c: P, reach: f64| -> (f64, P) {
        let (lo, hi) = if outward {
            let r = geom::len(c);
            let lo = radii.partition_point(|&x| x < r - reach - step).saturating_sub(1);
            let hi = (radii.partition_point(|&x| x <= r + reach + step) + 1).min(flank.len());
            (lo, hi)
        } else {
            (0, flank.len())
        };
        if hi < lo + 2 {
            return (f64::MAX, flank[0]); // nothing within reach
        }
        let (d, q, _) = geom::poly_closest(c, &flank[lo..hi]);
        (d, q)
    };
    let mut rho = rho;
    for _ in 0..30 {
        let rcen = if outside { rc + rho } else { rc - rho };
        // the flank's angle at the centre radius: closest flank point on that circle
        let mut a_fl: Option<f64> = None;
        for w in flank.windows(2) {
            let (r0, r1) = (geom::len(w[0]), geom::len(w[1]));
            if (r0 - rcen) * (r1 - rcen) <= 0.0 && (r0 - r1).abs() > 0.0 {
                let k = (rcen - r0) / (r1 - r0);
                let p = geom::add(w[0], geom::scale(geom::sub(w[1], w[0]), k));
                a_fl = Some(geom::angle(p));
                break;
            }
        }
        let Some(a_fl) = a_fl else {
            rho *= 0.7;
            continue;
        };
        let f = |a: f64| closest(polar(rcen, a), rho).0 - rho;
        if f(a_open) <= 0.0 {
            rho *= 0.7;
            continue;
        }
        // halving the angle bracket (well under a radian) 54 times reaches double precision
        let (mut lo, mut hi) = (a_open, a_fl);
        for _ in 0..54 {
            let mid = 0.5 * (lo + hi);
            if f(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let a = 0.5 * (lo + hi);
        let center = polar(rcen, a);
        let (_, on_flank) = closest(center, 2.0 * rho);
        let on_circle = polar(rc, a);
        let (u, v) = (geom::sub(on_flank, center), geom::sub(on_circle, center));
        let bis = geom::add(geom::scale(u, 1.0 / geom::len(u)), geom::scale(v, 1.0 / geom::len(v)));
        let mid = geom::add(center, geom::scale(bis, rho / geom::len(bis)));
        return Some(CornerFillet { center, radius: rho, on_flank, on_circle, mid });
    }
    None
}

/// The internal gear's numbers after the checks: the tip may be raised to the base circle.
#[derive(Clone, Copy, Debug)]
pub struct IntInfo {
    pub r_tip: f64,
    pub r_root: f64,
    /// Root fillet radius as drawn (it shrinks where the space is narrow).
    pub fillet: f64,
    pub tip_raised: bool,
}

/// The upper flank of an internal tooth space centred on angle 0, from the tip circle to the
/// root fillet, and the fillet.
struct IntFlank {
    up: Vec<P>,
    cf: CornerFillet,
    /// Half-angle of the space at the tip circle.
    a_tip: f64,
    info: IntInfo,
}

/// The internal flank after the checks. The tip circle is raised to the base circle if it lies
/// inside it, as there is no involute below the base circle.
fn internal_flank(t: &Tooth, d: Detail) -> Result<IntFlank, String> {
    let g = PI / t.zf();
    let rb = t.rb();
    let r_root = t.rf_int();
    let tip_raised = t.ra_int() < rb * (1.0 + 1e-6);
    let r_tip = if tip_raised { rb * (1.0 + 1e-6) } else { t.ra_int() };
    if r_root <= r_tip + 0.2 * t.mn {
        return Err("the teeth have no height left: the tip circle has to stay outside the base circle (use more teeth)".into());
    }
    if t.psi_space(r_root) <= 0.0 {
        return Err("the tooth spaces close up at the root: lower the profile shift".into());
    }
    let a_tip = t.psi_space(r_tip);
    if 2.0 * r_tip * (g - a_tip) < 0.05 * t.mn {
        return Err("the internal teeth come to a point: lower the addendum or the profile shift".into());
    }
    let n = 300;
    let dense: Vec<P> = (0..=n)
        .map(|i| {
            let r = r_tip + (r_root - r_tip) * i as f64 / n as f64;
            polar(r, t.psi_space(r))
        })
        .collect();
    let cf = corner_fillet(&dense, r_root, t.rho * t.mn, false, 0.0).ok_or("no room for the root fillet of the internal teeth")?;
    let r_q = geom::len(cf.on_flank);
    let up = involute_samples(t, r_tip, r_q, d.flank, &|r| t.psi_space(r)).iter().map(|&(r, a)| polar(r, a)).collect();
    Ok(IntFlank { up, cf, a_tip, info: IntInfo { r_tip, r_root, fillet: cf.radius, tip_raised } })
}

/// Inner end of an internal tooth-space loop: a little inside the tip circle.
fn internal_gap_inner(t: &Tooth, r_tip: f64) -> f64 {
    r_tip - (0.15 * t.mn).min(0.5 * r_tip)
}

/// One tooth space of an internal gear, centred on angle π/z (between tooth 0 on +X and tooth 1),
/// as a closed loop from a little inside the tip circle out to the root circle.
pub fn internal_gap(t: &Tooth, d: Detail) -> Result<(Vec<Curve>, IntInfo), String> {
    let IntFlank { up, cf, a_tip, info } = internal_flank(t, d)?;
    let mirror = |p: P| [p[0], -p[1]];
    let r_in = internal_gap_inner(t, info.r_tip);
    let q_up = *up.last().unwrap();
    let mut c = vec![
        Curve::Arc(polar(r_in, a_tip), polar(r_in, -a_tip), [r_in, 0.0]),
        Curve::Line(polar(r_in, -a_tip), mirror(up[0])),
        Curve::Spline(up.iter().map(|p| mirror(*p)).collect()),
        Curve::Arc(mirror(q_up), mirror(cf.on_circle), mirror(cf.mid)),
    ];
    if cf.on_circle[1] > 1e-9 {
        c.push(Curve::Arc(mirror(cf.on_circle), cf.on_circle, [info.r_root, 0.0]));
    }
    c.push(Curve::Arc(cf.on_circle, q_up, cf.mid));
    c.push(Curve::Spline(up.iter().rev().cloned().collect()));
    c.push(Curve::Line(up[0], polar(r_in, a_tip)));
    let g = PI / t.zf();
    Ok((c.into_iter().map(|cu| cu.rotated(g)).collect(), info))
}

/// One period of an external gear's outline: tooth 0 (on +X) and the space after it. It starts
/// where fillet and involute meet on tooth 0's lower flank and ends at the same point of tooth 1,
/// so consecutive periods meet spline to spline. The sketch does not tie splines together, so
/// pattern chunks stay separate solver groups. `r_tip` may exceed the tip circle (worm wheels).
pub fn external_period(t: &Tooth, f: &ExtFlank, r_tip: f64, d: Detail) -> Vec<Curve> {
    let g = PI / t.zf();
    let lo0 = |(r, th): (f64, f64)| polar(r, -th);
    let up0 = |(r, th): (f64, f64)| polar(r, th);
    let lo1 = |(r, th): (f64, f64)| polar(r, 2.0 * g - th);
    let fil_xy: Vec<P> = f.fillet.iter().map(|&p| up0(p)).collect();
    let fil: Vec<(f64, f64)> = geom::resample(&fil_xy, d.fillet).iter().map(|p| (geom::len(*p), geom::angle(*p))).collect();
    let inv = involute_samples(t, f.r_t, r_tip, d.flank, &|r| t.theta_inv(r));
    let tip = *inv.last().unwrap();
    let rf = f.fillet[0].0;
    let mut c = vec![
        Curve::Spline(inv.iter().map(|&p| lo0(p)).collect()),
        Curve::Arc(lo0(tip), up0(tip), [r_tip, 0.0]),
        Curve::Spline(inv.iter().rev().map(|&p| up0(p)).collect()),
        Curve::Spline(fil.iter().rev().map(|&p| up0(p)).collect()),
    ];
    if f.theta_b < g - 1e-9 {
        c.push(Curve::Arc(polar(rf, f.theta_b), polar(rf, 2.0 * g - f.theta_b), polar(rf, g)));
    }
    c.push(Curve::Spline(fil.iter().map(|&p| lo1(p)).collect()));
    c
}

/// One period of an internal gear's toothed outline: from the tip of tooth 0's upper flank, out
/// round the space and back over tooth 1's tip to the same point of tooth 1. Internal teeth are
/// centred on angle 2πk/z.
pub fn internal_period(t: &Tooth, d: Detail) -> Result<(Vec<Curve>, IntInfo), String> {
    let g = PI / t.zf();
    let IntFlank { up, mut cf, a_tip, info } = internal_flank(t, d)?;
    let (r_tip, r_root) = (info.r_tip, info.r_root);
    let q = *up.last().unwrap();
    let mirror = |p: P| [p[0], -p[1]];
    // a root arc too short to draw: let the two fillets meet in the middle of the space
    let root_arc = cf.on_circle[1] > 1e-3 * t.mn;
    if !root_arc {
        cf.on_circle = [r_root, 0.0];
    }
    let mut c = vec![Curve::Spline(up.iter().map(|p| mirror(*p)).collect()), Curve::Arc(mirror(q), mirror(cf.on_circle), mirror(cf.mid))];
    if root_arc {
        c.push(Curve::Arc(mirror(cf.on_circle), cf.on_circle, [r_root, 0.0]));
    }
    c.push(Curve::Arc(cf.on_circle, q, cf.mid));
    c.push(Curve::Spline(up.iter().rev().cloned().collect()));
    c.push(Curve::Arc(polar(r_tip, a_tip), polar(r_tip, 2.0 * g - a_tip), polar(r_tip, g)));
    Ok((c.into_iter().map(|cu| cu.rotated(g)).collect(), info))
}

/// Exact cross-section area of an external gear (tip disc minus the tooth spaces), from densely
/// sampled loops. For checking the built solid: volume = area × face width.
pub fn section_area_external(t: &Tooth) -> Result<f64, String> {
    let f = external_flank(t)?;
    let gap = crate::curve::loop_polygon(&external_gap(t, &f, t.ra(), Detail::DENSE));
    Ok(PI * t.ra().powi(2) - t.zf() * geom::polygon_area(&gap).abs())
}

/// Exact cross-section area of an internal gear's ring with outside radius `rim`.
pub fn section_area_internal(t: &Tooth, rim: f64) -> Result<f64, String> {
    let (c, ii) = internal_gap(t, Detail::DENSE)?;
    let gap = geom::polygon_area(&crate::curve::loop_polygon(&c)).abs();
    // the gap loop starts inside the tip circle: take off the ring sector it adds there
    let r_in = internal_gap_inner(t, ii.r_tip);
    let a_tip = t.psi_space(ii.r_tip);
    let inside = a_tip * (ii.r_tip.powi(2) - r_in.powi(2));
    Ok(PI * (rim * rim - ii.r_tip.powi(2)) - t.zf() * (gap - inside))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::{loop_gap, loop_polygon};

    fn tooth(z: u32) -> Tooth {
        Tooth::standard(z, 2.0, 20.0)
    }

    #[test]
    fn standard_diameters() {
        let t = tooth(20);
        assert!((t.rp() - 20.0).abs() < 1e-12);
        assert!((t.ra() - 22.0).abs() < 1e-12);
        assert!((t.rf() - 17.5).abs() < 1e-12);
    }

    #[test]
    fn fillet_meets_involute() {
        for z in [18, 20, 40, 100] {
            let t = tooth(z);
            let f = external_flank(&t).unwrap();
            assert!(!f.undercut, "z={z} should not be undercut");
            let (r, th) = *f.fillet.last().unwrap();
            assert!((th - t.theta_inv(r)).abs() < 1e-9);
            // the fillet starts on the root circle
            assert!((f.fillet[0].0 - t.rf()).abs() < 1e-6, "z={z}: {}", f.fillet[0].0);
            // the generated point just before the junction is close to the involute too (tangent)
            let (r2, th2) = f.fillet[f.fillet.len() - 2];
            if r2 >= t.rb() {
                assert!((th2 - t.theta_inv(r2)).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn small_counts_are_undercut() {
        for z in [6, 8, 10, 12, 14] {
            let t = tooth(z);
            let f = external_flank(&t).unwrap();
            assert!(f.undercut, "z={z}");
            // the boundary is continuous where the fillet hands over to the involute
            let (r, th) = *f.fillet.last().unwrap();
            assert!((th - t.theta_inv(r)).abs() < 1e-9);
            // and the neck is narrower than the involute would be at the base circle
            let neck = f.fillet.iter().map(|p| p.1).fold(f64::MAX, f64::min);
            assert!(neck < t.theta_inv(t.rb()), "z={z}");
        }
        // a profile shift of 0.5 removes the undercut of 10 teeth
        let mut t = tooth(10);
        t.x = 0.5;
        assert!(!external_flank(&t).unwrap().undercut);
    }

    #[test]
    fn large_profile_shifts() {
        // From x ≈ 0.87 (standard dedendum and rounding) the cutter's tip rounding has its centre
        // above the rolling line. The flank still runs from the root circle into the involute,
        // and nothing jumps where the centre crosses the line.
        let mut last: Option<f64> = None;
        for x in [0.80, 0.86, 0.87, 0.88, 0.9, 1.0, 1.2] {
            let mut t = tooth(24);
            t.x = x;
            let f = external_flank(&t).unwrap_or_else(|e| panic!("x = {x}: {e}"));
            assert!((f.fillet[0].0 - t.rf()).abs() < 1e-6, "x = {x}: fillet starts at r = {}", f.fillet[0].0);
            let (r, th) = *f.fillet.last().unwrap();
            assert!((th - t.theta_inv(r)).abs() < 1e-9, "x = {x}");
            assert!(f.fillet.windows(2).all(|w| w[1].0 > w[0].0), "x = {x}: fillet sorted by radius");
            let c = external_gap(&t, &f, t.ra() + 0.2, Detail::SKETCH);
            assert!(loop_gap(&c) < 1e-9, "x = {x}");
            // the root space's half-angle at the root circle changes smoothly with x
            if let Some(prev) = last {
                assert!((f.theta_b - prev).abs() < 0.02, "x = {x}: θb {} after {prev}", f.theta_b);
            }
            last = Some(f.theta_b);
        }
        for (z, x) in [(20, 1.0), (30, 1.2), (40, 1.5)] {
            let mut t = tooth(z);
            t.x = x;
            let f = external_flank(&t).unwrap_or_else(|e| panic!("z = {z}, x = {x}: {e}"));
            chains(&external_period(&t, &f, t.ra(), Detail::SKETCH), z);
        }
    }

    #[test]
    fn tooth_thickness_on_pitch_circle() {
        let t = tooth(24);
        let s = 2.0 * t.rp() * t.theta_inv(t.rp());
        assert!((s - PI * 2.0 / 2.0).abs() < 1e-12);
    }

    #[test]
    fn gap_loop_closes() {
        for z in [6, 9, 20, 57, 150] {
            let t = tooth(z);
            let f = external_flank(&t).unwrap();
            let c = external_gap(&t, &f, t.ra() + 0.2, Detail::SKETCH);
            assert!(loop_gap(&c) < 1e-9, "z={z}");
        }
    }

    /// Area of the whole gear from the dense gap loops: disc of the tip circle minus z spaces.
    fn gear_area(t: &Tooth) -> f64 {
        let f = external_flank(t).unwrap();
        let r_out = t.ra() + 0.3;
        let gap = loop_polygon(&external_gap(t, &f, r_out, Detail::DENSE));
        let gap_area = geom::polygon_area(&gap).abs();
        let g = PI / t.zf();
        // the part of the gap outside the tip circle, by integration of its angular width
        let n = 200;
        let mut outside = 0.0;
        for i in 0..n {
            let r = t.ra() + (r_out - t.ra()) * (i as f64 + 0.5) / n as f64;
            let w = 2.0 * (g - t.theta_inv(r));
            outside += w * r * (r_out - t.ra()) / n as f64;
        }
        PI * t.ra().powi(2) - t.zf() * (gap_area - outside)
    }

    #[test]
    fn area_close_to_known_values() {
        // A standard 20-tooth gear's cross-section lies between the root and tip disc areas and
        // close to the pitch disc area (the teeth fill about half the ring between root and tip).
        let t = tooth(20);
        let a = gear_area(&t);
        let lo = PI * t.rf().powi(2);
        let hi = PI * t.ra().powi(2);
        assert!(a > lo && a < hi);
        let pitch = PI * t.rp().powi(2);
        assert!((a - pitch).abs() / pitch < 0.03, "{a} vs {pitch}");
    }

    /// Consecutive periods chain up: within a period end meets start, and the period's end is
    /// its start turned by one pitch.
    fn chains(c: &[Curve], z: u32) {
        for w in c.windows(2) {
            assert!(geom::dist(w[0].end(), w[1].start()) < 1e-9);
        }
        let turned = geom::rot(c[0].start(), 2.0 * PI / z as f64);
        assert!(geom::dist(c.last().unwrap().end(), turned) < 1e-9);
    }

    #[test]
    fn periods_chain() {
        for z in [6, 9, 20, 57, 150] {
            let t = tooth(z);
            let f = external_flank(&t).unwrap();
            chains(&external_period(&t, &f, t.ra(), Detail::SKETCH), z);
        }
        for z in [40, 61, 120] {
            let (c, _) = internal_period(&tooth(z), Detail::SKETCH).unwrap();
            chains(&c, z);
        }
    }

    #[test]
    fn internal_gap_closes() {
        for z in [40, 60, 120] {
            let t = tooth(z);
            let (c, info) = internal_gap(&t, Detail::SKETCH).unwrap();
            assert!(loop_gap(&c) < 1e-9, "z={z}");
            assert!(!info.tip_raised || z < 34);
        }
        let (_, info) = internal_gap(&tooth(30), Detail::SKETCH).unwrap();
        assert!(info.tip_raised);
    }

    #[test]
    fn helical_transverse_values() {
        let mut t = tooth(20);
        t.beta = 30f64.to_radians();
        assert!((t.mt() - 2.0 / 30f64.to_radians().cos()).abs() < 1e-12);
        assert!(t.alpha_t() > t.alpha_n);
        let f = external_flank(&t).unwrap();
        assert!(!f.undercut);
    }
}
