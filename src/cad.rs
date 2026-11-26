//! Thin wrappers over the SolidCraft engine commands the plugin uses. Every edit goes through the
//! host's `commands::run`, so it lands on the timeline as ordinary, editable features.
//!
//! A command that fails inside a plugin run ends the whole run (SolidCraft 0.49): the plugin never
//! sees the error, so nothing here may try a command to find out whether it works.
//!
//! Every command named here must be listed in the manifest (`schema::COMMANDS`).

use crate::curve::{circle_through, Curve};
use crate::geom::{r9, rp9, P};
use serde_json::{json, Value};
use std::collections::HashSet;

/// What the plugin needs from the host: the guest glue in `lib.rs` implements it over the WIT
/// imports, and tests can implement it with a recorder.
pub trait Host {
    fn run(&mut self, command: &str, args: Value) -> Result<Value, String>;
    fn log(&mut self, _message: &str) {}
    fn progress(&mut self, _fraction: f32, _message: &str) {}
    /// Ask SolidCraft to open `entry`'s dialog filled with `params` once this run ends
    /// (`forms@0.1`). `Ok(false)`: the caller is a program, which gets no dialogs.
    fn open_form(&mut self, _entry: &str, _params: &Value) -> Result<bool, String> {
        Ok(false)
    }
}

/// SolidCraft stops a plugin call after this long (the manifest's `deadline_ms`).
pub const DEADLINE_MS: u64 = 30_000;

/// Builds estimated to take longer than this are refused before anything is made; the rest of
/// the deadline is margin for slower machines.
pub const TIME_BUDGET_S: f64 = 24.0;

/// Why a build of about `est` seconds is refused, and `fix`, what to do instead.
pub fn too_slow(what: &str, est: f64, fix: &str) -> String {
    format!("{what} would take about {est:.0} s to build, and SolidCraft stops a plugin after {} s: {fix}", DEADLINE_MS / 1000)
}

/// `too_slow` in a few words, for the dialog's result line.
pub fn too_slow_short(est: f64) -> String {
    format!("~{est:.0} s: too slow")
}

/// The solver joins a pattern's copies into one group of at most 4000 unknowns (point coordinates
/// and radii), so long rows are split into chunks, each a seed drawn curve by curve and a pattern.
/// Every pattern re-evaluates the whole sketch, so the chunks are made as large as that allows.
pub const UNKNOWNS_PER_GROUP: usize = 1560;

/// Rough solver unknowns of a curve: two per fit point or end point, plus centre and radius.
fn unknowns(c: &Curve) -> usize {
    match c {
        Curve::Line(..) => 4,
        Curve::Arc(..) => 7,
        Curve::Spline(v) => 2 * v.len() + 2,
    }
}

/// How many copies of `seed` one pattern may make.
pub fn chunk_size(seed: &[Curve]) -> usize {
    let u: usize = seed.iter().map(unknowns).sum::<usize>().max(1);
    (UNKNOWNS_PER_GROUP / u).max(1)
}

pub struct Cad<'a> {
    pub host: &'a mut dyn Host,
    /// Sketches made in this run, hidden at the end (the features keep using them).
    pub sketches: Vec<u64>,
    /// Every timeline feature made in this run (sketches too), in order: they go in one folder.
    pub features: Vec<u64>,
    /// The names features and bodies have, read once when first needed: a sketch needs a name no
    /// feature has, and a body asked for by a taken name would come out as "Body1".
    names: Option<Names>,
}

struct Names {
    features: HashSet<String>,
    bodies: HashSet<String>,
}

fn ids_of(v: &Value) -> Vec<String> {
    v.pointer("/created/curves")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// `want`, or the first numbered form of it (`numbered(2)`, `numbered(3)`, ...) not in `taken`;
/// the name returned is taken from then on.
fn free(taken: &mut HashSet<String>, want: &str, numbered: impl Fn(u32) -> String) -> String {
    let mut name = want.to_string();
    let mut n = 1;
    while taken.contains(&name) {
        n += 1;
        name = numbered(n);
    }
    taken.insert(name.clone());
    name
}

/// The first body a feature created, as the engine named it (it numbers a name that is taken),
/// or `asked`, the name the feature asked for, if the reply names none.
pub fn created_body(v: &Value, asked: &str) -> String {
    v.get("created_bodies")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .map_or_else(|| asked.to_string(), String::from)
}

impl<'a> Cad<'a> {
    pub fn new(host: &'a mut dyn Host) -> Cad<'a> {
        Cad { host, sketches: Vec::new(), features: Vec::new(), names: None }
    }

    /// Hide the sketches this run made. A failure here costs nothing but looks, so it is ignored.
    pub fn hide_sketches(&mut self) {
        if !self.sketches.is_empty() {
            let items = std::mem::take(&mut self.sketches);
            let _ = self.run("browser.visibility", json!({"visible": false, "items": items}));
        }
    }

    /// Run any engine command; its reply as JSON. A feature it makes is noted in `features`, and a
    /// new body asked for by a name that is taken gets a number: "Gear 1 (2)".
    pub fn run(&mut self, command: &str, mut args: Value) -> Result<Value, String> {
        if args.get("operation").and_then(Value::as_str) == Some("new") {
            if let Some(want) = args.get("body_name").and_then(Value::as_str).map(String::from) {
                let names = self.names()?;
                let name = free(&mut names.bodies, &want, |n| format!("{want} ({n})"));
                args["body_name"] = json!(name);
            }
        }
        let v = self.host.run(command, args)?;
        let made = match command {
            "sketch.create" => v.get("sketch"),
            c if c.starts_with("solid.") => v.get("feature"),
            _ => None,
        };
        if let Some(id) = made.and_then(Value::as_u64) {
            self.features.push(id);
        }
        Ok(v)
    }

    /// A sketch on the XY plane at height `z`, made the active one; returns its id.
    pub fn sketch_xy(&mut self, z: f64, name: &str) -> Result<u64, String> {
        let mut args = json!({"plane": "XY", "name": name});
        if z != 0.0 {
            args["offset"] = json!(r9(z));
        }
        self.sketch(args)
    }

    /// A sketch on the XZ plane (sketch x = world X, sketch y = world -Z), made the active one;
    /// returns its id.
    pub fn sketch_xz(&mut self, name: &str) -> Result<u64, String> {
        self.sketch(json!({"plane": "XZ", "name": name}))
    }

    /// The names the design's features (rolled back or not) and bodies have, read once.
    fn names(&mut self) -> Result<&mut Names, String> {
        if self.names.is_none() {
            let v = self.host.run("document.inspect", json!({"timeline": true, "bodies": "names"}))?;
            let list = |key: &str| -> HashSet<String> {
                let items = v.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
                items.iter().filter_map(|x| x.as_str().or_else(|| x.get("name").and_then(Value::as_str)).map(String::from)).collect()
            };
            self.names = Some(Names { features: list("timeline"), bodies: list("bodies") });
        }
        Ok(self.names.as_mut().expect("read above"))
    }

    /// A new sketch. SolidCraft refuses a sketch name any feature has, rolled back or not (a second
    /// build with the same body names), so a taken name gets a number: "Gear 1 profile 2".
    fn sketch(&mut self, mut args: Value) -> Result<u64, String> {
        let want = args["name"].as_str().unwrap_or("Sketch").to_string();
        let names = self.names()?;
        let name = free(&mut names.features, &want, |n| format!("{want} {n}"));
        args["name"] = json!(name);
        let v = self.run("sketch.create", args)?;
        let id = v.get("sketch").and_then(Value::as_u64).ok_or("sketch.create gave no sketch id")?;
        self.sketches.push(id);
        Ok(id)
    }

    /// Close the active sketch.
    pub fn finish(&mut self) -> Result<Value, String> {
        self.run("sketch.finish", json!({}))
    }

    /// Draw curves into the active sketch; returns the new curve ids in order.
    pub fn draw(&mut self, curves: &[Curve]) -> Result<Vec<String>, String> {
        let mut ids = Vec::new();
        for c in curves {
            let v = match c {
                Curve::Line(a, b) => self.run("sketch.line", json!({"points": [rp9(*a), rp9(*b)]}))?,
                Curve::Arc(a, b, t) => {
                    // By centre, start and end: a short arc given by three rounded points has a
                    // badly conditioned centre, which the kernel later finds off its circle.
                    let (center, _) = circle_through(*a, *t, *b).ok_or("an arc's three points are in line")?;
                    let turn = (t[0] - a[0]) * (b[1] - t[1]) - (t[1] - a[1]) * (b[0] - t[0]);
                    self.run(
                        "sketch.arc.center_point",
                        json!({"center": center, "start": a, "end": b, "clockwise": turn < 0.0, "connect": false}),
                    )?
                }
                Curve::Spline(pts) => {
                    let pts: Vec<P> = pts.iter().map(|p| rp9(*p)).collect();
                    self.run("sketch.spline.fit_point", json!({"points": pts}))?
                }
            };
            ids.extend(ids_of(&v));
        }
        Ok(ids)
    }

    /// A circle in the active sketch; returns its curve ids.
    pub fn circle(&mut self, center: P, diameter: f64) -> Result<Vec<String>, String> {
        let v = self.run("sketch.circle.center", json!({"center": rp9(center), "diameter": r9(diameter)}))?;
        Ok(ids_of(&v))
    }

    /// An axis-aligned rectangle in the active sketch between two corners; returns its curve ids.
    pub fn rectangle(&mut self, p0: P, p1: P) -> Result<Vec<String>, String> {
        let v = self.run("sketch.rectangle.two_point", json!({"p0": rp9(p0), "p1": rp9(p1)}))?;
        Ok(ids_of(&v))
    }

    /// Draw `count` copies of a closed loop around the origin, `pitch` radians apart, the first at
    /// `start` radians.
    pub fn loops_circular(&mut self, seed: &[Curve], count: usize, pitch: f64, start: f64) -> Result<(), String> {
        self.loops(seed, count, &|c, k| c.rotated(start + pitch * k as f64), &|ids, n| {
            (
                "sketch.pattern.circular",
                json!({"entities": ids, "center": [0.0, 0.0], "count": n, "angle": r9((pitch * (n - 1) as f64).to_degrees())}),
            )
        })
    }

    /// Draw `count` copies of a closed loop along +X, `pitch` mm apart, the first where it is.
    pub fn loops_linear(&mut self, seed: &[Curve], count: usize, pitch: f64) -> Result<(), String> {
        self.loops(seed, count, &|c, k| c.shifted([pitch * k as f64, 0.0]), &|ids, n| {
            ("sketch.pattern.rectangular", json!({"entities": ids, "count": n, "spacing": r9(pitch), "dir": [1.0, 0.0]}))
        })
    }

    /// The copies in chunks of at most `chunk_size` (see `UNKNOWNS_PER_GROUP`): each chunk's first
    /// copy (`place(curve, k)` for copy k) drawn curve by curve, then a sketch pattern
    /// (`pattern(ids, n)`, a command and its arguments) for the chunk's other copies. The seeds go
    /// in first, while the sketch is still small: every command re-evaluates the whole sketch.
    fn loops(
        &mut self,
        seed: &[Curve],
        count: usize,
        place: &dyn Fn(&Curve, usize) -> Curve,
        pattern: &dyn Fn(Vec<String>, usize) -> (&'static str, Value),
    ) -> Result<(), String> {
        let mut chunks: Vec<(Vec<String>, usize)> = Vec::new();
        let mut k = 0;
        while k < count {
            let n = chunk_size(seed).min(count - k);
            let curves: Vec<Curve> = seed.iter().map(|c| place(c, k)).collect();
            chunks.push((self.draw(&curves)?, n));
            k += n;
        }
        let total = chunks.len();
        for (i, (ids, n)) in chunks.into_iter().enumerate() {
            if n > 1 {
                let (command, args) = pattern(ids, n);
                self.run(command, args)?;
            }
            self.host.progress(0.1 + 0.4 * (i + 1) as f32 / total as f32, "drawing the teeth");
        }
        Ok(())
    }

    /// A cylinder on the Z axis from z = 0 to `height`. `op` is "new" (a body called `body_name`)
    /// or an operation on `target`.
    pub fn cylinder(
        &mut self,
        radius: f64,
        height: f64,
        op: &str,
        target: Option<&str>,
        name: &str,
        body_name: Option<&str>,
    ) -> Result<Value, String> {
        self.cylinder_at(0.0, radius, height, op, target, name, body_name)
    }

    /// A cylinder on the Z axis from z = `z0` up by `height` (see `cylinder`).
    #[allow(clippy::too_many_arguments)]
    pub fn cylinder_at(
        &mut self,
        z0: f64,
        radius: f64,
        height: f64,
        op: &str,
        target: Option<&str>,
        name: &str,
        body_name: Option<&str>,
    ) -> Result<Value, String> {
        let base = [0.0, 0.0, r9(z0)];
        let mut args = json!({"radius": r9(radius), "height": r9(height), "base": base, "operation": op, "name": name});
        if let Some(t) = target {
            args["targets"] = json!([t]);
        }
        if let Some(b) = body_name {
            args["body_name"] = json!(b);
        }
        self.run("solid.cylinder", args)
    }

    /// Cut `target` with the sketch's regions, extruded `distance` along +Z.
    pub fn extrude_cut(&mut self, sketch: u64, distance: f64, target: &str, name: &str) -> Result<Value, String> {
        self.run(
            "solid.extrude",
            json!({"sketch": sketch, "distance": r9(distance), "operation": "cut", "targets": [target], "name": name}),
        )
    }

    /// A new body called `body_name` from the sketch's regions, extruded `distance` along +Z.
    pub fn extrude_new(&mut self, sketch: u64, distance: f64, name: &str, body_name: &str) -> Result<Value, String> {
        self.run(
            "solid.extrude",
            json!({"sketch": sketch, "distance": r9(distance), "operation": "new", "name": name, "body_name": body_name}),
        )
    }

    /// A straight sweep path in a sketch on the XZ plane, from world (x0, 0, z0) to (x1, 0, z1);
    /// returns the sketch and the line.
    pub fn path_z(&mut self, name: &str, x0: f64, z0: f64, x1: f64, z1: f64) -> Result<(u64, String), String> {
        let id = self.sketch_xz(name)?;
        let ids = self.draw(&[Curve::Line([x0, -z0], [x1, -z1])])?;
        self.finish()?;
        let line = ids.first().cloned().ok_or("the sweep path has no line")?;
        Ok((id, line))
    }

    /// Turn `body` by `angle_deg` about `axis` through `origin`, then move it by `translate`.
    pub fn mov(
        &mut self,
        body: &str,
        axis: [f64; 3],
        angle_deg: f64,
        origin: [f64; 3],
        translate: [f64; 3],
        name: &str,
    ) -> Result<Value, String> {
        self.run(
            "solid.move",
            json!({"bodies": [body], "axis": axis, "angle": r9(angle_deg), "origin": [r9(origin[0]), r9(origin[1]), r9(origin[2])],
                   "translate": [r9(translate[0]), r9(translate[1]), r9(translate[2])], "name": name}),
        )
    }
}
