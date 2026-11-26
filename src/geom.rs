//! Small 2D helpers. Millimetres and radians throughout.

/// A point or vector in a sketch plane.
pub type P = [f64; 2];

/// The point at radius `r` and angle `a`.
pub fn polar(r: f64, a: f64) -> P {
    [r * a.cos(), r * a.sin()]
}

/// `p` turned by `a` about the origin.
pub fn rot(p: P, a: f64) -> P {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

pub fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}

pub fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

pub fn scale(a: P, k: f64) -> P {
    [a[0] * k, a[1] * k]
}

/// Length of a vector. (Not `hypot`: in WebAssembly that is a long library routine, and lengths
/// here are millimetres, far from overflow.)
pub fn len(a: P) -> f64 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}

pub fn dist(a: P, b: P) -> f64 {
    len(sub(a, b))
}

/// Polar angle of `a`, in (-π, π].
pub fn angle(a: P) -> f64 {
    a[1].atan2(a[0])
}

/// Involute function inv(a) = tan(a) - a.
pub fn inv(a: f64) -> f64 {
    a.tan() - a
}

/// Squared distance from `p` to segment `a`-`b` and the closest point on it.
fn seg_closest_sq(p: P, a: P, b: P) -> (f64, P) {
    let ab = sub(b, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 > 0.0 { (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let q = add(a, scale(ab, t));
    let d = sub(p, q);
    (d[0] * d[0] + d[1] * d[1], q)
}

/// Closest point of a polyline to `p`: (distance, point, segment index). `pts` must not be empty.
pub fn poly_closest(p: P, pts: &[P]) -> (f64, P, usize) {
    let mut best = (f64::MAX, pts[0], 0);
    for i in 0..pts.len().saturating_sub(1) {
        let (d2, q) = seg_closest_sq(p, pts[i], pts[i + 1]);
        if d2 < best.0 {
            best = (d2, q, i);
        }
    }
    (best.0.sqrt(), best.1, best.2)
}

/// Pick `n` points spread evenly by arc length along a polyline, keeping both ends exactly.
/// `pts` must not be empty.
pub fn resample(pts: &[P], n: usize) -> Vec<P> {
    if pts.len() <= 2 || n <= 2 {
        return vec![pts[0], *pts.last().unwrap()];
    }
    let mut cum = vec![0.0];
    for i in 1..pts.len() {
        let d = dist(pts[i - 1], pts[i]);
        cum.push(cum[i - 1] + d);
    }
    let total = *cum.last().unwrap();
    if total <= 0.0 {
        return vec![pts[0], *pts.last().unwrap()];
    }
    let mut out = vec![pts[0]];
    let mut j = 1;
    for k in 1..n - 1 {
        let s = total * k as f64 / (n - 1) as f64;
        while j < pts.len() - 1 && cum[j] < s {
            j += 1;
        }
        let (s0, s1) = (cum[j - 1], cum[j]);
        let t = if s1 > s0 { (s - s0) / (s1 - s0) } else { 0.0 };
        out.push(add(pts[j - 1], scale(sub(pts[j], pts[j - 1]), t)));
    }
    out.push(*pts.last().unwrap());
    out
}

/// Area of a closed polygon (shoelace), positive when counter-clockwise.
pub fn polygon_area(pts: &[P]) -> f64 {
    let n = pts.len();
    let mut a = 0.0;
    for i in 0..n {
        let p = pts[i];
        let q = pts[(i + 1) % n];
        a += p[0] * q[1] - q[0] * p[1];
    }
    a / 2.0
}

/// Round to 1e-9 (mm or degrees) so the JSON stays short and stable, without "-0".
pub fn r9(x: f64) -> f64 {
    let v = (x * 1e9).round() / 1e9;
    if v == 0.0 {
        0.0
    } else {
        v
    }
}

/// A point rounded with `r9`.
pub fn rp9(p: P) -> P {
    [r9(p[0]), r9(p[1])]
}
