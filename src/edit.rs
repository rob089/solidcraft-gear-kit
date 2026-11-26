//! Editing gears after they are built.
//!
//! Every build goes in a timeline folder of its own, and the design keeps a record of it in Gear
//! Kit's plugin data (`storage@0.1`, `{"sets": {"<folder id>": record}}`): the entry that made it,
//! every value it was built with, the folder's name (and the name Gear Kit gave it, to tell a
//! name the user chose), its features and its bodies. SolidCraft never
//! reuses a folder id, so the id names the build for as long as the folder exists. A record is
//! part of the run that made it: Undo removes both.
//!
//! `edit-gears` takes a picked body, finds the record whose bodies include it, and asks SolidCraft
//! (`forms@0.1`) to open that entry's dialog with the recorded values and `replace`, the folder
//! id. Applying the dialog runs the entry again with `replace`, and the new gears take the old
//! ones' place in the timeline:
//!
//! 1. move the history marker to just before the old folder, so the later features are not built
//!    and nothing fails while the old gears go;
//! 2. delete the old folder with its features (first: SolidCraft keeps a sketch's name taken even
//!    while it is rolled back, and a plugin run is one transaction, so a failed build after this
//!    leaves the design as it was);
//! 3. build there, and put the new features in a folder of their own, where the old one was;
//! 4. move the marker back where it was (shifted by the change in the number of features).
//!
//! Later features that use the gear bodies by name rebuild on the new gears. A feature attached to
//! a face or an edge of the old gears (a sketch on a gear's face, a picked edge) would lose it, and
//! SolidCraft ends a plugin run the moment a command leaves a feature failing, with the plugin
//! never seeing the error. So the edit looks first: such features are named and the edit refused
//! before anything changes. The whole edit is one undo step.
//!
//! For the same reason nothing here tries a command to see whether it works: whether the old
//! gears are still there is read from the timeline, not from `timeline.inspect` failing.

use crate::cad::{Cad, Host};
use crate::entries::R;
use crate::schema::{self, bad, Args, REPLACE};
use serde_json::{json, Map, Value};

/// The format of Gear Kit's data in a design; a later version that changes it raises this and
/// converts older data when it reads it.
pub const DATA_FORMAT: u64 = 1;

/// The plugin's id, as its manifest gives it: the owner of its data in a design.
pub const PLUGIN_ID: &str = "gear-kit";

/// Gear Kit's data in the open design.
pub struct Store {
    /// Records by folder id.
    pub sets: Map<String, Value>,
    /// False when the data comes from a newer Gear Kit (or the design can't be changed):
    /// SolidCraft refuses writes then.
    pub writable: bool,
}

impl Store {
    pub fn load(cad: &mut Cad) -> Result<Store, String> {
        // named explicitly: the dialog hook may not count as the plugin's own run
        let v = cad.run("storage.get", json!({"plugin": PLUGIN_ID}))?;
        let newer = v.get("format").and_then(Value::as_u64).is_some_and(|f| f > DATA_FORMAT);
        let read_only = v.get("read_only").and_then(Value::as_bool).unwrap_or(false);
        let sets = v.pointer("/document/sets").and_then(Value::as_object).cloned().unwrap_or_default();
        Ok(Store { sets, writable: !newer && !read_only })
    }

    /// The record of the build in folder `id`.
    pub fn get(&self, id: u64) -> Option<&Value> {
        self.sets.get(&id.to_string())
    }

    /// The build that made the body called `name`: its folder id and record. The newest one, if
    /// a body name was used twice.
    pub fn find_body(&self, name: &str) -> Option<(u64, &Value)> {
        self.sets
            .iter()
            .filter(|(_, r)| r["bodies"].as_array().is_some_and(|b| b.iter().any(|b| b.as_str() == Some(name))))
            .filter_map(|(k, r)| Some((k.parse().ok()?, r)))
            .max_by_key(|(id, _)| *id)
    }
}

/// Note the build in folder `folder` (and drop the record of `replaced`, the build it replaces).
pub fn record(cad: &mut Cad, folder: u64, record: Value, replaced: Option<u64>) -> Result<(), String> {
    let mut sets = Map::new();
    if let Some(old) = replaced.filter(|&old| old != folder) {
        sets.insert(old.to_string(), Value::Null);
    }
    sets.insert(folder.to_string(), record);
    cad.run("storage.patch", json!({"patch": {"sets": sets}}))?;
    Ok(())
}

/// Put the features made (`made`, in timeline order) in a new folder called `name`, inside
/// `parent` if given; returns the folder's id and the features in it. A folder must hold one
/// unbroken run, and a failed command would end the run, so the run is read from the timeline:
/// everything from the first feature made to the last.
pub fn group(cad: &mut Cad, made: &[u64], name: &str, parent: Option<u64>) -> Result<(u64, Vec<u64>), String> {
    let t = Timeline::read(cad)?;
    let at = |id: &u64| t.ids.iter().position(|i| i == id);
    let run = match (made.first().and_then(at), made.last().and_then(at)) {
        (Some(a), Some(b)) if a <= b => t.ids[a..=b].to_vec(),
        _ => made.to_vec(),
    };
    let mut args = json!({"features": run, "name": name});
    if let Some(p) = parent {
        args["parent"] = json!(p);
    }
    let v = cad.run("timeline.group", args)?;
    let id = v.get("group").and_then(Value::as_u64).ok_or("timeline.group gave no folder id")?;
    Ok((id, run))
}

/// The design's timeline as `document.inspect` lists it (folders are not in it).
struct Timeline {
    ids: Vec<u64>,
    names: Vec<String>,
    failing: Vec<bool>,
    /// How many features are built (before the history marker).
    marker: usize,
}

impl Timeline {
    fn read(cad: &mut Cad) -> Result<Timeline, String> {
        let v = cad.run("document.inspect", json!({"timeline": true, "bodies": false}))?;
        let items = v.get("timeline").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut t = Timeline { ids: Vec::new(), names: Vec::new(), failing: Vec::new(), marker: 0 };
        let mut active = 0;
        for f in &items {
            let Some(id) = f.get("id").and_then(Value::as_u64) else { continue };
            t.ids.push(id);
            t.names.push(f.get("name").and_then(Value::as_str).unwrap_or("?").to_string());
            t.failing.push(f.get("error").is_some_and(|e| !e.is_null()));
            if f.get("rolled_back").and_then(Value::as_bool) != Some(true) {
                active += 1;
            }
        }
        t.marker = v.get("marker").and_then(Value::as_u64).map_or(active, |m| m as usize);
        Ok(t)
    }
}

/// The place of a build that is being replaced, held open: the old build is gone and the marker
/// sits where it began until `close`.
pub struct Slot {
    /// The folder the old one is in (the new one goes there too).
    pub parent: Option<u64>,
    /// The old folder's name, if the user renamed it (the new folder keeps it).
    renamed: Option<String>,
    /// Where its first feature is, and how many it has.
    start: usize,
    count: usize,
    /// The marker and the number of features before.
    marker: usize,
    total: usize,
}

impl Slot {
    /// Open the place of the build in folder `id`, whose record is `record`: check that it can go,
    /// move the marker to where it begins and delete it. `None` when its features are gone
    /// (deleted, or their creation undone): the build is then a new one.
    pub fn open(cad: &mut Cad, id: u64, record: Option<&Value>) -> R<Option<Slot>> {
        // only a folder Gear Kit made and recorded is ever replaced: a stray id never deletes
        // someone's features
        let record = record.ok_or_else(|| bad(REPLACE, format!("timeline folder {id} holds no gears made by Gear Kit")))?;
        let ours = ids(&record["features"]);
        let t = Timeline::read(cad)?;
        if !ours.iter().any(|f| t.ids.contains(f)) {
            return Ok(None);
        }
        let g = cad.run("timeline.inspect", json!({"group": id}))?;
        let inside = ids(&g["features"]);
        let name_of = |f: u64| t.ids.iter().position(|i| *i == f).map_or_else(|| format!("feature {f}"), |i| format!("“{}”", t.names[i]));
        if let Some(&f) = inside.iter().find(|f| !ours.contains(f)) {
            let reason = format!("its folder also holds {}, which Gear Kit did not make: move it out of the folder first", name_of(f));
            return Err(bad(REPLACE, reason).into());
        }
        let Some(start) = inside.iter().filter_map(|f| t.ids.iter().position(|i| i == f)).min() else {
            return Ok(None);
        };
        // later features (built or rolled back) that would lose a face, an edge or a sketch of
        // the old gears, not just find a body again by its name. `timeline.inspect` is quick;
        // `timeline.dependents` would tell too, but rebuilds what follows for every feature asked
        // about (seconds each behind a large gear).
        let bodies: Vec<&str> = record["bodies"].as_array().map(|b| b.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        let mut attached = Vec::new();
        for &f in t.ids[start..].iter().filter(|f| !inside.contains(f)) {
            let v = cad.run("timeline.inspect", json!({"feature": f}))?;
            let def = v.get("definition").unwrap_or(&v);
            if refers_to(def, &inside) || picks_on(def, &bodies) {
                attached.push(name_of(f));
            }
        }
        if !attached.is_empty() {
            let reason = format!(
                "{} {} on a face, an edge or a sketch of these gears and would lose it: move {} off the gears or delete {} first",
                attached.join(", "),
                if attached.len() == 1 { "sits" } else { "sit" },
                if attached.len() == 1 { "it" } else { "them" },
                if attached.len() == 1 { "it" } else { "them" },
            );
            return Err(bad(REPLACE, reason).into());
        }
        cad.run("timeline.roll_to", json!({"position": start}))?;
        cad.run("timeline.delete", json!({"groups": [id]}))?;
        let name = g["name"].as_str().unwrap_or("");
        Ok(Some(Slot {
            parent: g["parent"].as_u64(),
            renamed: (name != record["auto_name"].as_str().unwrap_or("")).then(|| name.to_string()),
            start,
            count: inside.len(),
            marker: t.marker,
            total: t.ids.len(),
        }))
    }

    /// The name the new folder keeps: the old one's, if the user gave it one.
    pub fn kept_name(&self) -> Option<String> {
        self.renamed.clone()
    }

    /// Move the marker back, `made` features having taken the place of the old ones.
    pub fn close(self, cad: &mut Cad, made: usize) -> Result<(), String> {
        let marker = if self.marker >= self.total {
            json!("end")
        } else if self.marker <= self.start {
            json!(self.marker)
        } else if self.marker >= self.start + self.count {
            json!(self.marker - self.count + made)
        } else {
            // it was inside the old build: after the new one
            json!(self.start + made)
        };
        cad.run("timeline.roll_to", json!({"position": marker}))?;
        Ok(())
    }
}

/// The feature ids in a JSON list.
fn ids(v: &Value) -> Vec<u64> {
    v.as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

/// Whether a feature's definition picks faces or edges of one of `bodies` (a fillet:
/// `{"body": "Gear 1", "edges": [[17, 0, 10]]}`). SolidCraft keeps such picks as names of the
/// features that made them, which `timeline.inspect` leaves out, so the body is what tells.
fn picks_on(def: &Value, bodies: &[&str]) -> bool {
    const PICKS: [&str; 4] = ["edges", "faces", "edge", "face"];
    let picks = def.as_object().is_some_and(|m| m.iter().any(|(k, v)| PICKS.contains(&k.as_str()) && !is_empty(v)));
    picks && mentions(def, bodies)
}

fn is_empty(v: &Value) -> bool {
    v.is_null() || v.as_array().is_some_and(|a| a.is_empty())
}

/// Whether any string in `v` is one of `names`.
fn mentions(v: &Value, names: &[&str]) -> bool {
    match v {
        Value::String(s) => names.contains(&s.as_str()),
        Value::Array(a) => a.iter().any(|x| mentions(x, names)),
        Value::Object(m) => m.values().any(|x| mentions(x, names)),
        _ => false,
    }
}

/// Whether a feature's definition (`timeline.inspect`) names one of `features` as the origin of a
/// face or an edge, or as its sketch: `{"origin_feature": 3}`, `"sketch": 3` or a face called
/// "F3:top". A body named in `targets` is not such a reference: it is found again by name.
fn refers_to(def: &Value, features: &[u64]) -> bool {
    match def {
        Value::Object(m) => m.iter().any(|(k, v)| {
            let by_id = (k.contains("feature") || k == "sketch") && v.as_u64().is_some_and(|f| features.contains(&f));
            by_id || refers_to(v, features)
        }),
        Value::Array(a) => a.iter().any(|v| refers_to(v, features)),
        Value::String(s) => s
            .strip_prefix('F')
            .and_then(|r| r.split_once(':'))
            .and_then(|(n, _)| n.parse::<u64>().ok())
            .is_some_and(|f| features.contains(&f)),
        _ => false,
    }
}

/// The record of the build that made the picked body (`gear`): its folder id and record, or why
/// there is none.
fn picked(cad: &mut Cad, a: &Args) -> R<(u64, Value)> {
    let pick = a.pick("gear")?;
    let body = pick.get("name").and_then(Value::as_str).ok_or_else(|| bad("gear", "pick a body").short("pick a body"))?;
    let store = Store::load(cad)?;
    let (id, record) = store
        .find_body(body)
        .ok_or_else(|| bad("gear", format!("{body} was not made by Gear Kit, or was renamed")).short("not made by Gear Kit"))?;
    let t = Timeline::read(cad)?;
    if !ids(&record["features"]).iter().any(|f| t.ids.contains(f)) {
        return Err(bad("gear", format!("the features that made {body} were deleted")).short("its features are gone").into());
    }
    Ok((id, record.clone()))
}

/// The `edit-gears` entry: open the dialog the picked gear was made with, filled with its values
/// and `replace`. It is for people: SolidCraft 0.49 ends a program's call at `forms.open` with
/// `interactive_required` (programs read the records with `storage.get` instead). Should a host
/// refuse more gently, the reply gives the entry and the parameters to call it with.
pub fn open(host: &mut dyn Host, params: Value) -> R<Value> {
    let e = schema::entry(schema::EDIT).expect("the edit entry is in schema::ENTRIES");
    let a = Args::new(e, params)?;
    let mut cad = Cad::new(host);
    let (id, record) = picked(&mut cad, &a)?;
    let entry = record["entry"].as_str().unwrap_or_default().to_string();
    let mut params = record["params"].clone();
    params[REPLACE] = json!(id);
    let opened = cad.host.open_form(&entry, &params)?;
    let mut out = json!({"entry": entry, "params": params, "folder": {"id": id, "name": record["folder"]}, "opened": opened});
    if !opened {
        out["note"] = json!("call the entry with these parameters, changed as needed: replace puts the new gears where these are");
    }
    Ok(out)
}

/// The dialog hook for `edit-gears`: which gears the pick belongs to, on the result line.
pub fn hook(host: &mut dyn Host, values: Value) -> Value {
    let e = schema::entry(schema::EDIT).expect("the edit entry is in schema::ENTRIES");
    let mut cad = Cad::new(host);
    // the result line is narrow: the folder's name, or the problem in a few words with the whole
    // of it in the pick's tooltip
    let problem = |b: &schema::Bad| crate::dialog::problem(schema::EDIT, b);
    match Args::new(e, values) {
        Err(b) => problem(&b),
        Ok(a) if a.v.get("gear").and_then(Value::as_array).is_none_or(|p| p.is_empty()) => {
            crate::dialog::summary_only("Pick a gear or pulley")
        }
        Ok(a) => match picked(&mut cad, &a) {
            Ok((_, r)) => {
                let folder = r["folder"].as_str().unwrap_or("Gear Kit gears");
                let mut reply = crate::dialog::summary_only(folder);
                reply["gear"] = json!({"hint": format!("{folder}: Run opens its dialog with the values it was built with")});
                reply
            }
            Err(crate::entries::Error::Input(b)) => problem(&b),
            Err(crate::entries::Error::Failed(m)) => crate::dialog::summary_only(&format!("⚠ {m}")),
        },
    }
}

#[cfg(test)]
mod tests {
    //! The edit flow against a simulated design that behaves as SolidCraft 0.49's timeline does
    //! (measured headless): new features land at the history marker, folders hold one unbroken
    //! run, deleting a folder takes its features, rolled-back features are not built and so do not
    //! fail, a sketch name stays taken while it is rolled back, and a feature attached to a face
    //! loses it when the feature that made the face goes. Above all, a command that fails ends the
    //! run: `exec` fails a test whose run succeeded although a command failed on the way.
    use super::*;
    use crate::entries::{execute, Error};
    use std::collections::BTreeMap;

    struct Feat {
        id: u64,
        name: String,
        /// The body it makes.
        body: Option<String>,
        /// A body it works on, by name (like a cut's target).
        uses: Option<String>,
        /// A feature whose face it is attached to (like a sketch on a gear's face).
        on_face_of: Option<u64>,
        /// A feature whose edge it picked (like a fillet on a gear's edge, on the body `uses`).
        on_edge_of: Option<u64>,
    }

    struct Folder {
        name: String,
        parent: Option<u64>,
        features: Vec<u64>,
    }

    #[derive(Default)]
    struct Design {
        timeline: Vec<Feat>,
        marker: usize,
        next: u64,
        folders: BTreeMap<u64, Folder>,
        data: Value,
        forms: Vec<(String, Value)>,
        interactive: bool,
        /// The first command that failed in the current run: SolidCraft would have ended it there.
        failed: Option<String>,
    }

    impl Design {
        fn new() -> Design {
            Design { next: 1, interactive: true, ..Default::default() }
        }

        fn add(&mut self, name: &str, body: Option<&str>, uses: Option<&str>, on_face_of: Option<u64>) -> u64 {
            let id = self.next;
            self.next += 1;
            let f = Feat { id, name: name.into(), body: body.map(Into::into), uses: uses.map(Into::into), on_face_of, on_edge_of: None };
            // a feature between two of one folder joins it
            let (before, after) = (self.marker.checked_sub(1).map(|i| self.timeline[i].id), self.timeline.get(self.marker).map(|f| f.id));
            if let (Some(b), Some(a)) = (before, after) {
                for g in self.folders.values_mut() {
                    if let (Some(i), true) = (g.features.iter().position(|x| *x == b), g.features.contains(&a)) {
                        g.features.insert(i + 1, id);
                    }
                }
            }
            self.timeline.insert(self.marker, f);
            self.marker += 1;
            id
        }

        /// A fillet on an edge of the body `body`, made by feature `src`.
        fn add_fillet(&mut self, name: &str, body: &str, src: u64) -> u64 {
            let id = self.add(name, None, Some(body), None);
            self.timeline.iter_mut().find(|f| f.id == id).unwrap().on_edge_of = Some(src);
            id
        }

        /// The feature that makes the body called `body`.
        fn maker(&self, body: &str) -> u64 {
            self.timeline.iter().find(|f| f.body.as_deref() == Some(body)).unwrap().id
        }

        fn error(&self, i: usize) -> Option<String> {
            let f = &self.timeline[i];
            if i >= self.marker {
                return None;
            }
            let built = &self.timeline[..i];
            if let Some(src) = f.on_face_of {
                if !built.iter().any(|g| g.id == src) {
                    return Some("unknown face origin feature is gone".into());
                }
            }
            if let Some(src) = f.on_edge_of {
                if !built.iter().any(|g| g.id == src) {
                    return Some(format!("the reference `F{src}:side|F{src}:top` no longer exists as it was picked; reselect it"));
                }
            }
            if let Some(b) = &f.uses {
                if !built.iter().any(|g| g.body.as_ref() == Some(b)) {
                    return Some(format!("unknown body '{b}'"));
                }
            }
            None
        }

        fn names(&self) -> Vec<&str> {
            self.timeline.iter().map(|f| f.name.as_str()).collect()
        }

        fn record(&self, folder: u64) -> &Value {
            &self.data["sets"][folder.to_string()]
        }
    }

    fn merge(target: &mut Value, patch: &Value) {
        if let Value::Object(p) = patch {
            if !target.is_object() {
                *target = json!({});
            }
            let t = target.as_object_mut().unwrap();
            for (k, v) in p {
                if v.is_null() {
                    t.remove(k);
                } else {
                    merge(t.entry(k.clone()).or_insert(Value::Null), v);
                }
            }
        } else {
            *target = patch.clone();
        }
    }

    impl Host for Design {
        fn run(&mut self, command: &str, args: Value) -> Result<Value, String> {
            let r = self.command(command, args);
            if let Err(e) = &r {
                self.failed.get_or_insert_with(|| format!("{command}: {e}"));
            }
            r
        }

        fn open_form(&mut self, entry: &str, params: &Value) -> Result<bool, String> {
            if self.interactive {
                self.forms.push((entry.into(), params.clone()));
            }
            Ok(self.interactive)
        }
    }

    impl Design {
        fn command(&mut self, command: &str, args: Value) -> Result<Value, String> {
            // the manifest grants the plugin only the commands it lists
            if !schema::COMMANDS.contains(&command) {
                return Err(format!("permission denied: command {command} is not granted"));
            }
            let ids = |v: &Value| v.as_array().map(|a| a.iter().filter_map(Value::as_u64).collect::<Vec<_>>()).unwrap_or_default();
            Ok(match command {
                "sketch.create" => {
                    let name = args["name"].as_str().unwrap_or("Sketch");
                    if self.timeline.iter().any(|f| f.name == name) {
                        return Err(format!("command sketch.create failed: another feature is called `{name}`"));
                    }
                    json!({"sketch": self.add(name, None, None, None)})
                }
                c if c.starts_with("sketch.") => json!({"created": {"curves": ["c1", "c2"]}}),
                c if c.starts_with("solid.") => {
                    let body = args["body_name"].as_str();
                    let id = self.add(args["name"].as_str().unwrap_or(c), body, None, None);
                    json!({"feature": id, "created_bodies": body.map_or(json!([]), |b| json!([b]))})
                }
                "browser.visibility" => json!({}),
                "storage.get" => {
                    json!({"document": self.data, "format": if self.data.is_null() { Value::Null } else { json!(1) }, "read_only": false})
                }
                "storage.patch" => {
                    merge(&mut self.data, &args["patch"]);
                    json!({})
                }
                "document.inspect" => {
                    let t: Vec<Value> = (0..self.timeline.len())
                        .map(|i| {
                            let f = &self.timeline[i];
                            json!({"id": f.id, "name": f.name, "error": self.error(i), "rolled_back": i >= self.marker})
                        })
                        .collect();
                    let bodies: Vec<&str> = self.timeline[..self.marker].iter().filter_map(|f| f.body.as_deref()).collect();
                    json!({"timeline": t, "marker": self.marker, "bodies": bodies})
                }
                "timeline.inspect" => match args.get("group").and_then(Value::as_u64) {
                    Some(g) => {
                        let f = self.folders.get(&g).ok_or(format!("no timeline folder {g}"))?;
                        json!({"group": g, "name": f.name, "parent": f.parent, "features": f.features, "joints": []})
                    }
                    None => {
                        let key = &args["feature"];
                        let f = self
                            .timeline
                            .iter()
                            .find(|f| key.as_u64() == Some(f.id) || key.as_str() == Some(&f.name))
                            .ok_or(format!("no feature {key}"))?;
                        let id = f.id;
                        let mut def = json!({});
                        if let Some(src) = f.on_face_of {
                            def["plane"] = json!({"face": {"name": format!("F{src}:top"), "origin_feature": src}});
                        }
                        match (&f.uses, f.on_edge_of) {
                            (Some(b), Some(_)) => def = json!({"body": b, "edges": [[17.0, 0.0, 10.0]], "radius": "0.3"}),
                            (Some(b), None) => def["targets"] = json!([b]),
                            _ => {}
                        }
                        json!({"id": id, "name": f.name, "definition": def})
                    }
                },
                "timeline.group" => {
                    let feats = ids(&args["features"]);
                    let pos: Vec<usize> = feats.iter().map(|f| self.timeline.iter().position(|g| g.id == *f).unwrap()).collect();
                    if pos.windows(2).any(|w| w[1] != w[0] + 1) {
                        return Err("timeline folder must contain one contiguous run of features".into());
                    }
                    for g in self.folders.values_mut() {
                        g.features.retain(|f| !feats.contains(f));
                    }
                    let id = self.next;
                    self.next += 1;
                    let name = args["name"].as_str().unwrap().to_string();
                    self.folders.insert(id, Folder { name, parent: args["parent"].as_u64(), features: feats });
                    json!({"group": id})
                }
                "timeline.delete" => {
                    let mut deleted = Vec::new();
                    for g in ids(&args["groups"]) {
                        let f = self.folders.remove(&g).ok_or(format!("no timeline folder {g}"))?;
                        deleted.extend(f.features);
                    }
                    let before = self.timeline[..self.marker].iter().filter(|f| deleted.contains(&f.id)).count();
                    self.timeline.retain(|f| !deleted.contains(&f.id));
                    self.marker -= before;
                    json!({"deleted": deleted})
                }
                "timeline.roll_to" => {
                    self.marker = args["position"].as_u64().map_or(self.timeline.len(), |p| p as usize).min(self.timeline.len());
                    if let Some(e) = (0..self.marker).find_map(|i| self.error(i)) {
                        return Err(e);
                    }
                    json!({"features": self.timeline.len(), "marker": self.marker})
                }
                _ => return Err(format!("unknown command {command}")),
            })
        }
    }

    /// Run an entry as SolidCraft would: a run that succeeded although a command failed on the way
    /// would have been ended at that command, so it fails the test.
    fn exec(d: &mut Design, entry: &str, params: Value) -> R<Value> {
        d.failed = None;
        let r = execute(d, entry, params);
        if r.is_ok() {
            assert!(d.failed.is_none(), "the run went on after a failed command: {:?}", d.failed);
        }
        r
    }

    /// A small gear pair, quick to draw.
    fn small() -> Value {
        json!({"teeth_1": 8, "teeth_2": 10, "face_width": 4})
    }

    fn folder_of(out: &Value) -> u64 {
        out["folder"]["id"].as_u64().unwrap()
    }

    #[test]
    fn a_build_is_one_folder_and_a_record() {
        let mut d = Design::new();
        d.add("Base", Some("Base"), None, None);
        let out = exec(&mut d, "spur-gears", small()).unwrap();
        let id = folder_of(&out);
        let made: Vec<u64> = d.timeline[1..].iter().map(|f| f.id).collect();
        assert_eq!(d.folders[&id].features, made);
        assert_eq!(d.folders[&id].name, "Gear: 8:10 spur gears");
        let r = d.record(id);
        assert_eq!(r["entry"], "spur-gears");
        assert_eq!(r["bodies"], json!(["Gear 1", "Gear 2"]));
        assert_eq!(r["features"], json!(made));
        // every value as built, defaults too, so an edit starts from exactly this
        assert_eq!(r["params"]["teeth_1"], 8);
        assert_eq!(r["params"]["module"], 2.0);
        assert_eq!(r["params"]["mate"], "gear");
        assert!(r["params"].get("summary").is_none());
    }

    #[test]
    fn edit_opens_the_dialog_with_the_values() {
        let mut d = Design::new();
        let id = folder_of(&exec(&mut d, "spur-gears", small()).unwrap());
        let pick = |name: &str| json!({"gear": [{"type": "body", "name": name}]});
        let out = exec(&mut d, "edit-gears", pick("Gear 2")).unwrap();
        assert_eq!(out["opened"], true);
        let (entry, params) = &d.forms[0];
        assert_eq!(entry, "spur-gears");
        assert_eq!(params["teeth_2"], 10);
        assert_eq!(params[REPLACE], id);
        // the dialog hook says which gears the pick belongs to
        let h = hook(&mut d, pick("Gear 1"));
        assert!(h["summary"]["value"].as_str().unwrap().starts_with("Gear: 8:10 spur gears"), "{h}");
        // programs get the parameters instead of a dialog
        d.interactive = false;
        let out = exec(&mut d, "edit-gears", pick("Gear 1")).unwrap();
        assert_eq!(out["opened"], false);
        assert_eq!(out["params"][REPLACE], id);
        // a body Gear Kit did not make
        d.add("Plate", Some("Plate"), None, None);
        match exec(&mut d, "edit-gears", pick("Plate")) {
            Err(Error::Input(b)) => assert_eq!(b.field, "gear"),
            other => panic!("{other:?}"),
        }
        assert!(hook(&mut d, pick("Plate"))["summary"]["value"].as_str().unwrap().starts_with('⚠'));
    }

    #[test]
    fn replacing_keeps_the_place_and_what_uses_the_gears() {
        let mut d = Design::new();
        d.add("Base", Some("Base"), None, None);
        let old = folder_of(&exec(&mut d, "spur-gears", small()).unwrap());
        let old_names: Vec<String> = d.timeline[1..].iter().map(|f| f.name.clone()).collect();
        d.add("Bore", None, Some("Gear 1"), None);
        let mut p = small();
        p["teeth_1"] = json!(9);
        p[REPLACE] = json!(old);
        let out = exec(&mut d, "spur-gears", p).unwrap();
        let new = folder_of(&out);
        // the old build is gone, the new one is where it was, the marker back at the end
        assert!(!d.folders.contains_key(&old));
        let names = d.names();
        assert_eq!(names[0], "Base");
        assert_eq!(names[names.len() - 1], "Bore");
        assert_eq!(d.folders[&new].features.len(), names.len() - 2);
        assert_eq!(d.marker, d.timeline.len());
        // the old build went first, so the new one has its feature names, unnumbered
        assert_eq!(names[1..names.len() - 1], old_names[..]);
        // the bore uses Gear 1 by name and rebuilds on the new one
        assert!(out["warnings"].to_string().contains("undercut") && !out["warnings"].to_string().contains("gone"), "{out}");
        assert!((0..d.timeline.len()).all(|i| d.error(i).is_none()));
        // the record moved to the new folder, with the new values
        assert!(d.data["sets"].get(old.to_string()).is_none());
        assert_eq!(d.record(new)["params"]["teeth_1"], 9);
        assert_eq!(d.folders[&new].name, "Gear: 9:10 spur gears");
    }

    #[test]
    fn replacing_refuses_to_break_what_sits_on_the_gears() {
        let mut d = Design::new();
        let old = folder_of(&exec(&mut d, "spur-gears", small()).unwrap());
        let profile = d.folders[&old].features[0];
        let teeth = d.maker("Gear 1");
        d.add("Bore", None, Some("Gear 1"), None);
        d.add("Keyway sketch", None, None, Some(profile));
        d.add_fillet("Tip fillet", "Gear 1", teeth);
        let before = d.names().join("|");
        let mut p = small();
        p[REPLACE] = json!(old);
        match exec(&mut d, "spur-gears", p) {
            Err(Error::Input(b)) => {
                assert_eq!(b.field, REPLACE);
                assert!(b.reason.starts_with("“Keyway sketch”, “Tip fillet” sit on a face"), "{}", b.reason);
                assert!(!b.reason.contains("Bore"), "{}", b.reason);
            }
            other => panic!("{other:?}"),
        }
        // refused before anything changed
        assert_eq!(d.names().join("|"), before);
        assert!(d.failed.is_none());
    }

    #[test]
    fn replacing_keeps_a_renamed_folder_and_the_marker() {
        let mut d = Design::new();
        let old = folder_of(&exec(&mut d, "spur-gears", small()).unwrap());
        d.folders.get_mut(&old).unwrap().name = "Drive".into();
        d.add("Later", None, None, None);
        // the marker before "Later"
        d.marker -= 1;
        let mut p = small();
        p[REPLACE] = json!(old);
        let new = folder_of(&exec(&mut d, "spur-gears", p.clone()).unwrap());
        assert_eq!(d.folders[&new].name, "Drive");
        assert_eq!(d.marker, d.timeline.len() - 1);
        assert_eq!(d.timeline[d.marker].name, "Later");
        // and again: the name stays the user's
        p[REPLACE] = json!(new);
        p["teeth_1"] = json!(9);
        let newer = folder_of(&exec(&mut d, "spur-gears", p).unwrap());
        assert_eq!(d.folders[&newer].name, "Drive");
        // a name Gear Kit gave follows the gears
        d.folders.get_mut(&newer).unwrap().name = "Gear: 9:10 spur gears".into();
        let mut p = small();
        p[REPLACE] = json!(newer);
        let last = folder_of(&exec(&mut d, "spur-gears", p).unwrap());
        assert_eq!(d.folders[&last].name, "Gear: 8:10 spur gears");
    }

    #[test]
    fn replacing_refuses_what_it_should_not_delete() {
        let mut d = Design::new();
        let old = folder_of(&exec(&mut d, "spur-gears", small()).unwrap());
        // a folder Gear Kit did not record
        let mut p = small();
        p[REPLACE] = json!(old + 100);
        match exec(&mut d, "spur-gears", p.clone()) {
            Err(Error::Input(b)) => assert_eq!(b.field, REPLACE),
            other => panic!("{other:?}"),
        }
        // a feature of the user's own in the gears' folder
        let mine = d.add("Mine", None, None, None);
        d.folders.get_mut(&old).unwrap().features.push(mine);
        p[REPLACE] = json!(old);
        match exec(&mut d, "spur-gears", p) {
            Err(Error::Input(b)) => assert!(b.reason.contains("Mine"), "{}", b.reason),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_second_build_with_the_same_names_gets_numbered_sketches() {
        let mut d = Design::new();
        exec(&mut d, "spur-gears", small()).unwrap();
        let n = d.timeline.len();
        exec(&mut d, "spur-gears", small()).unwrap();
        assert_eq!(d.timeline.len(), 2 * n);
        assert!(d.names().contains(&"Gear 1 profile 2"), "{:?}", d.names());
        // the bodies too: not the engine's "Body1"
        let bodies: Vec<&str> = d.timeline.iter().filter_map(|f| f.body.as_deref()).collect();
        assert_eq!(bodies, ["Gear 1", "Gear 2", "Gear 1 (2)", "Gear 2 (2)"]);
    }

    #[test]
    fn the_dialog_shows_replace_only_when_editing() {
        let r = crate::dialog::update_fields("spur-gears", json!({}));
        assert_eq!(r[REPLACE], json!({"visible": false}));
        let r = crate::dialog::update_fields("worm-gears", json!({REPLACE: 6}));
        assert!(r[REPLACE]["hint"].as_str().unwrap().contains("folder 6"), "{r}");
        assert!(crate::dialog::update_fields("bevel-gears", json!({REPLACE: 0}))[REPLACE]["visible"] == false);
    }

    #[test]
    fn pulleys_are_built_edited_and_replaced_like_gears() {
        let mut d = Design::new();
        let p = json!({"profile": "t5", "teeth_1": 12, "teeth_2": 18, "belt_teeth": 60, "flanges": "one"});
        let out = exec(&mut d, "timing-pulleys", p.clone()).unwrap();
        let old = folder_of(&out);
        assert_eq!(d.folders[&old].name, "Pulley: 12:18 T5 pulleys");
        assert_eq!(out["bodies"], json!(["Pulley 1", "Pulley 2"]));
        // one flange each, joined to its pulley, and pulley 2 moved to the centre distance
        let names = d.names();
        for n in ["Pulley 1 teeth", "Pulley 1 flange", "Pulley 2 flange", "Pulley 2 placement"] {
            assert!(names.contains(&n), "{n}: {names:?}");
        }
        assert!(!names.iter().any(|x| x.contains("upper flange")));
        // Edit Gears opens the pulleys' own dialog
        exec(&mut d, "edit-gears", json!({"gear": [{"type": "body", "name": "Pulley 2"}]})).unwrap();
        let (entry, params) = &d.forms[0];
        assert_eq!(entry, "timing-pulleys");
        assert_eq!(params["teeth_2"], 18);
        assert_eq!(params["flanges"], "one");
        let mut q = params.clone();
        q["teeth_2"] = json!(24);
        let new = folder_of(&exec(&mut d, "timing-pulleys", q).unwrap());
        assert!(!d.folders.contains_key(&old));
        assert_eq!(d.folders[&new].name, "Pulley: 12:24 T5 pulleys");
        assert_eq!(d.record(new)["params"]["profile"], "t5");
    }

    #[test]
    fn replacing_gears_that_are_gone_builds_new_ones() {
        let mut d = Design::new();
        let old = folder_of(&exec(&mut d, "spur-gears", small()).unwrap());
        exec(&mut d, "spur-gears", json!({"teeth_1": 8, "teeth_2": 10, "face_width": 4, "name": "Other"})).unwrap();
        d.run("timeline.delete", json!({"groups": [old]})).unwrap();
        let mut p = small();
        p[REPLACE] = json!(old);
        let out = exec(&mut d, "spur-gears", p).unwrap();
        assert!(out["warnings"].to_string().contains("gone"));
        assert!(d.data["sets"].get(old.to_string()).is_none());
        assert_eq!(d.data["sets"].as_object().unwrap().len(), 2);
    }
}
