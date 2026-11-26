//! Sketch curves as the plugin draws them, and closed loops made of them.

use crate::geom::{self, P};
use std::f64::consts::TAU;

/// One sketch curve. A loop is a `Vec<Curve>` in order, each ending where the next starts.
#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    Line(P, P),
    /// Start, end and a point on the arc between them.
    Arc(P, P, P),
    /// A fit-point spline through these points.
    Spline(Vec<P>),
}

impl Curve {
    pub fn start(&self) -> P {
        match self {
            Curve::Line(a, _) | Curve::Arc(a, _, _) => *a,
            Curve::Spline(v) => v[0],
        }
    }

    pub fn end(&self) -> P {
        match self {
            Curve::Line(_, b) | Curve::Arc(_, b, _) => *b,
            Curve::Spline(v) => *v.last().unwrap(),
        }
    }

    /// The curve with every defining point mapped by `f` (exact for rigid motions and mirrors).
    pub fn map(&self, f: &dyn Fn(P) -> P) -> Curve {
        match self {
            Curve::Line(a, b) => Curve::Line(f(*a), f(*b)),
            Curve::Arc(a, b, t) => Curve::Arc(f(*a), f(*b), f(*t)),
            Curve::Spline(v) => Curve::Spline(v.iter().map(|p| f(*p)).collect()),
        }
    }

    /// Turned by `a` about the origin.
    pub fn rotated(&self, a: f64) -> Curve {
        self.map(&|p| geom::rot(p, a))
    }

    /// Moved by `d`.
    pub fn shifted(&self, d: P) -> Curve {
        self.map(&|p| geom::add(p, d))
    }

    /// The same curve run the other way.
    pub fn reversed(&self) -> Curve {
        match self {
            Curve::Line(a, b) => Curve::Line(*b, *a),
            Curve::Arc(a, b, t) => Curve::Arc(*b, *a, *t),
            Curve::Spline(v) => Curve::Spline(v.iter().rev().copied().collect()),
        }
    }

    /// ∫ (x dy − y dx) / 2 along the curve: summed over a closed loop, its signed area
    /// (counterclockwise positive). Exact for lines and arcs; a spline counts as its fit points
    /// joined straight.
    pub fn area_term(&self) -> f64 {
        let cross = |a: P, b: P| a[0] * b[1] - a[1] * b[0];
        match self {
            Curve::Line(a, b) => cross(*a, *b) / 2.0,
            Curve::Spline(v) => v.windows(2).map(|w| cross(w[0], w[1]) / 2.0).sum(),
            Curve::Arc(a, b, t) => match circle_through(*a, *t, *b) {
                None => cross(*a, *b) / 2.0,
                Some((c, r)) => {
                    // the signed sweep from a through t to b
                    let ang = |p: P| geom::angle(geom::sub(p, c));
                    let norm = |x: f64| x.rem_euclid(TAU);
                    let (dt, d1) = (norm(ang(*t) - ang(*a)), norm(ang(*b) - ang(*a)));
                    let sweep = if dt <= d1 { d1 } else { d1 - TAU };
                    (cross(c, geom::sub(*b, *a)) + r * r * sweep) / 2.0
                }
            },
        }
    }

    /// Points along the curve from start to end (arcs in `n` steps, splines as their fit points
    /// joined straight), for checks.
    pub fn sample(&self, n: usize) -> Vec<P> {
        match self {
            Curve::Line(a, b) => vec![*a, *b],
            Curve::Spline(v) => v.clone(),
            Curve::Arc(a, b, t) => match circle_through(*a, *t, *b) {
                None => vec![*a, *b],
                Some((c, r)) => {
                    let a0 = geom::angle(geom::sub(*a, c));
                    let at = geom::angle(geom::sub(*t, c));
                    let a1 = geom::angle(geom::sub(*b, c));
                    // sweep from a0 through at to a1
                    // into [0, 2π); rem_euclid alone can round a tiny negative angle up to 2π
                    let norm = |x: f64| {
                        let r = x.rem_euclid(TAU);
                        if r >= TAU {
                            0.0
                        } else {
                            r
                        }
                    };
                    let (dt, d1) = (norm(at - a0), norm(a1 - a0));
                    let sweep = if dt <= d1 { d1 } else { d1 - TAU };
                    (0..=n).map(|i| geom::add(c, geom::polar(r, a0 + sweep * i as f64 / n as f64))).collect()
                }
            },
        }
    }
}

/// Circle through three points: centre and radius, or `None` if they are in line.
pub fn circle_through(a: P, b: P, c: P) -> Option<(P, f64)> {
    let d = 2.0 * (a[0] * (b[1] - c[1]) + b[0] * (c[1] - a[1]) + c[0] * (a[1] - b[1]));
    if d.abs() < 1e-14 {
        return None;
    }
    let sq = |p: P| p[0] * p[0] + p[1] * p[1];
    let ux = (sq(a) * (b[1] - c[1]) + sq(b) * (c[1] - a[1]) + sq(c) * (a[1] - b[1])) / d;
    let uy = (sq(a) * (c[0] - b[0]) + sq(b) * (a[0] - c[0]) + sq(c) * (b[0] - a[0])) / d;
    let center = [ux, uy];
    Some((center, geom::dist(center, a)))
}

/// A loop's curves as one closed polygon, for area checks.
pub fn loop_polygon(curves: &[Curve]) -> Vec<P> {
    let mut out: Vec<P> = Vec::new();
    for c in curves {
        let pts = c.sample(24);
        out.extend_from_slice(&pts[..pts.len() - 1]);
    }
    out
}

/// Largest gap between one curve's end and the next one's start.
pub fn loop_gap(curves: &[Curve]) -> f64 {
    let n = curves.len();
    (0..n).map(|i| geom::dist(curves[i].end(), curves[(i + 1) % n].start())).fold(0.0, f64::max)
}
