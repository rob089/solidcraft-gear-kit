#!/usr/bin/env python3
"""Integration tests: run every entry of the built plugin (dist/gear-kit) on the real SolidCraft
plugin host through solidcraft-cli and check the solids.

    ./build.sh && tests/integration.py [name-filter]

Checks per case: the run succeeds within the host's 30 s, the expected number of closed bodies,
the volume against the exact cross-section area (from `cargo run --example expect`), the bounding
box where it is simple, and for gears in mesh that inspect.interference finds no overlap, and that
the parts collide once nudged 0.4 mm either way (so they really touch). A last case turns a mating
gear by half a pitch and expects the check to find the overlap, so a silent interference check
cannot pass the others. The designs are saved to out/ for a look.

Timing pulleys: each belt profile alone (volume against the exact outline area), a large and a
clearance case, a flanged pair whose pulley 2 must sit at
the centre distance computed here from the belt length and must not touch pulley 1, and a pair
built in two runs.

A last check edits a pair in place: build it, cut a bore into gear 1 by name, read the pair's
record from the design (as a program does; Edit Gears opens a dialog, which a program can't),
and run the entry again with `replace` and a changed tooth count. The new pair must take the old
one's place in the timeline, the bore must cut the new gear 1, nothing may fail, and the
design's record must follow. Then a sketch on gear 1's face, or a fillet on one of its edges,
must make the next edit be refused, naming it, before anything changes. A second build with the
same names gets bodies of its own names ("Gear 1 (2)").
"""
import json
import math
import os
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools"))
import run as gk  # noqa: E402

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(ROOT, "out")
os.makedirs(OUT, exist_ok=True)


def expect_area(**kw):
    """The exact cross-section area (mm²) from the plugin's own profile code (examples/expect.rs)."""
    p = subprocess.run(["cargo", "run", "-q", "--example", "expect", "--", json.dumps(kw)], cwd=ROOT,
                       capture_output=True, text=True, check=True)
    return float(p.stdout.strip())


INTERFERENCE = {"command": "inspect.interference", "params": {}}


def pulley(profile, z, what="area", clearance=0.0):
    """A pulley's exact outline area (mm², no flanges), or its outside ("od") or pitch ("pd") diameter."""
    return expect_area(kind="pulley", profile=profile, z=z, what=what, clearance=clearance)


def centre_distance(d1, d2, length):
    """The centre distance a belt of pitch `length` sets between pulleys of pitch diameters d1, d2."""
    big, small = max(d1, d2) / 2, min(d1, d2) / 2
    belt = lambda c: 2 * c * math.cos(math.asin((big - small) / c)) + math.pi * (big + small) + 2 * math.asin((big - small) / c) * (big - small)
    lo, hi = big - small + 1e-9, length / 2
    for _ in range(100):
        mid = (lo + hi) / 2
        lo, hi = (mid, hi) if belt(mid) < length else (lo, mid)
    return (lo + hi) / 2


def case(name, entry, params, bodies=1, volume=None, vol_tol=2e-4, bbox=None, mesh=False, expect_overlap=False,
         extra=(), second=None, apart=False, centre2=None):
    """One test case. `volume`: the expected total volume (mm³) within `vol_tol` (relative);
    `bbox`: the first body's expected bounding box; `mesh`: the bodies mesh, so they must not
    overlap but must collide when nudged; `expect_overlap`: they must overlap (the sanity case);
    `extra`: steps after the run; `second`: parameters of a second run of the same entry into the
    same design (the other side of a pair); `apart`: the bodies must not touch (pulleys);
    `centre2`: where the middle of the body called "Pulley 2" must be in X."""
    return dict(name=name, entry=entry, params=params, bodies=bodies, volume=volume, vol_tol=vol_tol, bbox=bbox,
                mesh=mesh, expect_overlap=expect_overlap, extra=list(extra), second=second, apart=apart, centre2=centre2)


def cases():
    pi = math.pi
    c = []
    # --- single gears: one side of a pair turned off
    one = {"create_2": False}
    a = expect_area(kind="external", z=20)
    c.append(case("spur z20", "spur-gears", {**one, "teeth_1": 20}, volume=a * 10, bbox=([-22, -22, 0], [22, 22, 10])))
    c.append(case("spur z8 undercut", "spur-gears", {**one, "teeth_1": 8, "module": 3, "face_width": 6}, volume=expect_area(kind="external", z=8, m=3) * 6))
    a = expect_area(kind="external", z=12, m=1.5, x=0.5, backlash=0.1)
    # the pair's backlash is taken half from each gear
    c.append(case("spur shifted+backlash", "spur-gears", {**one, "teeth_1": 12, "module": 1.5, "profile_shift_1": 0.5, "backlash": 0.2, "face_width": 8},
                  volume=a * 8, vol_tol=4e-4))  # small, strongly shifted: splines within ~2 µm
    a = expect_area(kind="external", z=30, m=2, beta=20)
    c.append(case("helical z30 20deg", "spur-gears", {**one, "teeth_1": 30, "helix_angle": 20, "face_width": 15}, volume=a * 15, vol_tol=5e-4))
    a = expect_area(kind="external", z=24, m=2, beta=25)
    c.append(case("herringbone z24", "spur-gears", {**one, "teeth_1": 24, "helix_angle": 25, "herringbone": True, "helix_hand": "left", "face_width": 16},
                  volume=a * 16, vol_tol=5e-4))
    # large shift: the cutter's tip rounding has its centre above the rolling line
    a = expect_area(kind="external", z=24, x=1.2)
    c.append(case("spur x1.2", "spur-gears", {**one, "teeth_1": 24, "profile_shift_1": 1.2}, volume=a * 10, vol_tol=4e-4))
    a = expect_area(kind="external", z=120, m=1)
    c.append(case("spur z120 timing", "spur-gears", {**one, "teeth_1": 120, "module": 1, "face_width": 5}, volume=a * 5))
    # gear 2 alone (its placement is checked by the two-run pairs below)
    c.append(case("gear 2 alone", "spur-gears", {"create_1": False, "teeth_2": 40},
                  volume=expect_area(kind="external", z=40) * 10))
    # --- ring gears and racks alone
    ring = {"mate": "ring", "create_1": False}
    a = expect_area(kind="internal", z=60, rim=140)
    c.append(case("internal z60", "spur-gears", {**ring, "teeth_2": 60, "rim_diameter": 140}, volume=a * 10))
    a = expect_area(kind="internal", z=72, beta=15, rim=170)
    c.append(case("internal helical z72", "spur-gears", {**ring, "teeth_2": 72, "rim_diameter": 170, "helix_angle": 15}, volume=a * 10, vol_tol=5e-4))
    rack = {"mate": "rack", "create_1": False}
    a = expect_area(kind="rack", z=10, height=12)
    c.append(case("rack 10", "spur-gears", {**rack, "teeth_2": 10}, volume=a * 10))
    a = expect_area(kind="rack", z=12, height=12, beta=20)
    c.append(case("rack helical", "spur-gears", {**rack, "teeth_2": 12, "helix_angle": 20}, volume=a * 10, vol_tol=5e-4))
    # --- pairs (no overlap when meshed with a little backlash)
    c.append(case("pair external", "spur-gears", {"teeth_1": 15, "teeth_2": 40, "backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("pair external odd", "spur-gears", {"teeth_1": 13, "teeth_2": 27, "backlash": 0.1, "profile_shift_1": 0.4}, bodies=2, mesh=True))
    c.append(case("pair large shifts", "spur-gears", {"teeth_1": 16, "teeth_2": 30, "backlash": 0.1, "profile_shift_1": 0.9, "profile_shift_2": -0.2},
                  bodies=2, mesh=True))
    c.append(case("pair helical", "spur-gears", {"teeth_1": 18, "teeth_2": 31, "helix_angle": 20, "backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("pair herringbone", "spur-gears", {"teeth_1": 16, "teeth_2": 24, "helix_angle": 30, "herringbone": True, "backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("pair internal", "spur-gears", {"mate": "ring", "teeth_1": 18, "teeth_2": 60, "backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("pair internal helical", "spur-gears", {"mate": "ring", "teeth_1": 20, "teeth_2": 64, "helix_angle": 15, "backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("pair rack", "spur-gears", {"mate": "rack", "teeth_1": 16, "teeth_2": 12, "backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("pair rack helical", "spur-gears", {"mate": "rack", "teeth_1": 16, "teeth_2": 12, "helix_angle": 20, "backlash": 0.1}, bodies=2, mesh=True))
    # the result line is accepted (and ignored) when a program sends it
    c.append(case("pair with result line", "spur-gears", {"teeth_1": 15, "teeth_2": 40, "backlash": 0.1, "summary": "anything"}, bodies=2, mesh=True))
    # --- a pair built in two runs, one side each, still meshes
    for name, p in [("two runs external", {"teeth_1": 17, "teeth_2": 29, "helix_angle": 12, "backlash": 0.1}),
                    ("two runs ring", {"mate": "ring", "teeth_1": 18, "teeth_2": 54, "backlash": 0.1}),
                    ("two runs rack", {"mate": "rack", "teeth_1": 16, "teeth_2": 10, "backlash": 0.1})]:
        c.append(case(name, "spur-gears", {**p, "create_2": False}, bodies=2, mesh=True,
                      second={**p, "create_1": False}))
    # --- bevel and worm
    c.append(case("bevel 15/30", "bevel-gears", {"teeth_1": 15, "teeth_2": 30, "backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("bevel mitre 20/20", "bevel-gears", {"teeth_1": 20, "teeth_2": 20, "module": 1.5, "face_width": 6, "backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("bevel 60deg shaft", "bevel-gears", {"teeth_1": 14, "teeth_2": 28, "shaft_angle": 60, "backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("bevel two runs", "bevel-gears", {"teeth_1": 15, "teeth_2": 30, "backlash": 0.1, "create_2": False}, bodies=2, mesh=True,
                  second={"teeth_1": 15, "teeth_2": 30, "backlash": 0.1, "create_1": False}))
    c.append(case("worm drive", "worm-gears", {"backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("worm 2 starts left", "worm-gears", {"starts": 2, "helix_hand": "left", "wheel_teeth": 40, "backlash": 0.1}, bodies=2, mesh=True))
    c.append(case("worm two runs", "worm-gears", {"backlash": 0.1, "create_1": False}, bodies=2, mesh=True,
                  second={"backlash": 0.1, "create_2": False}))
    # a small wheel: its outside diameter shrinks before the teeth would come to a point
    c.append(case("worm small wheel", "worm-gears", {"wheel_teeth": 12, "backlash": 0.1}, bodies=2, mesh=True))
    # --- a second build with the same body names (its sketches get numbered names)
    c.append(case("same names twice", "spur-gears", {"teeth_1": 15, "teeth_2": 40}, bodies=4,
                  second={"teeth_1": 15, "teeth_2": 40}))
    # --- timing pulleys alone, each profile: SE/N grooves, DIN's table diameters, HTD's N'
    for prof, z in [("gt2-2", 20), ("gt2-3", 16), ("htd-3m", 20), ("htd-5m", 24), ("htd-8m", 28), ("t2.5", 15),
                    ("t5", 10), ("t10", 30), ("mxl", 18), ("xl", 12), ("gt2-2", 120), ("htd-5m", 90)]:
        # (a groove faces +X, so the outside diameter shows in the volume, not the bounding box)
        c.append(case(f"pulley {prof} z{z}", "timing-pulleys", {"profile": prof, "teeth_1": z, "create_2": False, "flanges": "none",
                                                                 "face_width": 6}, volume=pulley(prof, z) * 6))
    c.append(case("pulley gt2 clearance", "timing-pulleys", {"teeth_1": 20, "create_2": False, "flanges": "none", "groove_clearance": 0.2},
                  volume=pulley("gt2-2", 20, clearance=0.2) * 7))
    # --- a flanged pair: pulley 2 where the belt puts it, clear of pulley 1
    disc = lambda prof, z, h, t: math.pi * (pulley(prof, z, "od") / 2 + h) ** 2 * t
    v = (pulley("gt2-2", 20) + pulley("gt2-2", 40)) * 7 + 2 * disc("gt2-2", 20, 1.5, 1) + 2 * disc("gt2-2", 40, 1.5, 1)
    c.append(case("pulley pair gt2 flanged", "timing-pulleys", {"flange_height": 1.5}, bodies=2, volume=v, apart=True,
                  centre2=centre_distance(pulley("gt2-2", 20, "pd"), pulley("gt2-2", 40, "pd"), 200)))
    # --- two runs, one flange each
    p = {"profile": "t5", "teeth_1": 12, "teeth_2": 18, "belt_teeth": 60, "flanges": "one", "flange_height": 2, "face_width": 10}
    v = (pulley("t5", 12) + pulley("t5", 18)) * 10 + disc("t5", 12, 2, 1) + disc("t5", 18, 2, 1)
    c.append(case("pulleys two runs", "timing-pulleys", {**p, "create_2": False}, bodies=2, volume=v, apart=True,
                  centre2=centre_distance(pulley("t5", 12, "pd"), pulley("t5", 18, "pd"), 300), second={**p, "create_1": False}))
    # --- the check itself: a gear turned by half a pitch must collide
    c.append(case("sanity: misphased pair collides", "spur-gears", {"teeth_1": 15, "teeth_2": 40, "backlash": 0.1}, bodies=2, mesh=True,
                  expect_overlap=True,
                  extra=[{"command": "solid.move", "params": {"bodies": ["Gear 2"], "axis": [0, 0, 1], "angle": 4.5,
                                                              "origin": [55, 0, 0]}}]))
    return c


def check(k):
    """Run a case and list what is wrong with the result."""
    extra = list(k["extra"])
    if k["second"] is not None:
        pkg = os.path.abspath(gk.PKG)
        extra.insert(0, {"command": "plugin.run", "params": {"path": pkg, "approval": gk.approval(pkg), "entry": k["entry"],
                                                           "params": k["second"]}})
    if k["mesh"] or k["apart"]:
        extra.append(INTERFERENCE)
    save = os.path.join(OUT, k["name"].replace(" ", "_").replace("/", "-").replace(":", "") + ".sc")
    r = gk.run(k["entry"], k["params"], save=save, extra_steps=extra)
    problems = []
    if not r["ok"]:
        return [f"run failed: {r['error'][:500]}"], r
    if r["plugin_ms"] > 28000:
        problems.append(f"too slow: {r['plugin_ms']} ms")
    bodies = r["measure"]["bodies"]
    if len(bodies) != k["bodies"]:
        problems.append(f"{len(bodies)} bodies, expected {k['bodies']}: {[b['name'] for b in bodies]}")
    for b in bodies:
        if not b.get("closed", False):
            problems.append(f"body {b['name']} not closed")
    if k["volume"] is not None:
        v = r["measure"]["total"]["volume_mm3"]
        rel = abs(v - k["volume"]) / k["volume"]
        if rel > k["vol_tol"]:
            problems.append(f"volume {v:.3f}, expected {k['volume']:.3f} ({rel:.2e})")
    if k["bbox"] is not None:
        bb = bodies[0]["bbox"]
        for got, want in zip(bb["min"] + bb["max"], k["bbox"][0] + k["bbox"][1]):
            if abs(got - want) > 0.05:
                problems.append(f"bbox {bb} != {k['bbox']}")
                break
    if k["centre2"] is not None:
        b2 = next((b for b in bodies if b["name"] == "Pulley 2"), None)
        x = (b2["bbox"]["min"][0] + b2["bbox"]["max"][0]) / 2 if b2 else None
        if x is None or abs(x - k["centre2"]) > 0.01:
            problems.append(f"pulley 2 at x = {x}, expected {k['centre2']:.3f}")
    if k["apart"]:
        hits = r["extra"][-1].get("interferences", [])
        if sum(h.get("volume_mm3", h.get("volume", 0)) for h in hits) > 1e-6:
            problems.append(f"the pulleys touch: {json.dumps(hits)[:300]}")
    if k["mesh"]:
        inter = r["extra"][-1]
        hits = inter.get("interferences", [])
        unchecked = inter.get("unchecked", [])
        if unchecked:
            problems.append(f"interference unchecked: {unchecked}")
        vol = sum(h.get("volume_mm3", h.get("volume", 0)) for h in hits)
        if k["expect_overlap"] and vol < 1.0:
            problems.append("expected an overlap, found none")
        if not k["expect_overlap"] and vol > 1e-3:
            problems.append(f"gears overlap: {vol:.4f} mm3 {json.dumps(hits)[:300]}")
    return problems, r


def nudges(k, r):
    """Steps that move the mating part 0.4 mm (at its pitch circle) out of place, both ways."""
    res = r["result"]
    p = k["params"]
    m = p.get("module", 2.0)
    out = []
    for sign in (1, -1):
        if k["entry"] == "spur-gears":
            beta = math.radians(p.get("helix_angle", 0.0))
            mt = m / math.cos(beta)
            kind = p.get("mate", "gear")
            if kind == "rack":
                out.append({"command": "solid.move", "params": {"bodies": ["Gear rack"], "translate": [0.4 * sign, 0, 0]}})
                continue
            if kind == "ring":
                body, axis_at, rp = "Gear 1", res["gear_1_axis"], mt * p.get("teeth_1", 18) / 2
            else:
                body, axis_at, rp = "Gear 2", res["gear_2_axis"], mt * p.get("teeth_2", 36) / 2
            out.append({"command": "solid.move", "params": {"bodies": [body], "axis": [0, 0, 1], "origin": axis_at,
                                                          "angle": sign * math.degrees(0.4 / rp)}})
        elif k["entry"] == "bevel-gears":
            sh = math.radians(p.get("shaft_angle", 90.0))
            rp = m * p["teeth_2"] / 2
            out.append({"command": "solid.move", "params": {"bodies": ["Gear 2"], "axis": [math.sin(sh), 0, -math.cos(sh)],
                                                          "origin": res["apex"], "angle": sign * math.degrees(0.4 / rp)}})
        elif k["entry"] == "worm-gears":
            lead = math.pi * m * p.get("starts", 1)
            ax = res.get("worm_axis") or {"through": [res["centre_distance"], 0, p.get("wheel_face_width", 12) / 2], "direction": [0, 1, 0]}
            out.append({"command": "solid.move", "params": {"bodies": ["Worm"], "axis": ax["direction"], "origin": ax["through"],
                                                          "angle": sign * 360.0 * 0.4 / lead}})
    return out


def nudge_check(k, r, save):
    """In mesh means touching: moved 0.4 mm either way, the parts must collide."""
    problems = []
    for step in nudges(k, r):
        res, err = gk.cli([step, INTERFERENCE], "--in", save)
        if err:
            problems.append(f"nudge failed: {err[:300]}")
            continue
        hits = res["results"][-1].get("interferences", [])
        vol = sum(h.get("volume_mm3", h.get("volume", 0)) for h in hits)
        if vol <= 1e-4:
            problems.append(f"not in mesh: nudged by 0.4 mm ({step['params'].get('angle', step['params'].get('translate'))}) and still clear")
    return problems


def edit_check():
    """Edit a pair in place (see the module doc); returns the problems."""
    pkg = os.path.abspath(gk.PKG)
    a = gk.approval(pkg)
    pair = {"teeth_1": 15, "teeth_2": 40, "backlash": 0.1}
    save = os.path.join(OUT, "edit_in_place.sc")
    first = gk.run("spur-gears", pair, save=save)
    if not first["ok"]:
        return [f"first run failed: {first['error'][:300]}"]
    old = first["result"]["folder"]["id"]
    run = lambda entry, params: {"command": "plugin.run", "params": {"path": pkg, "approval": a, "entry": entry, "params": params}}
    problems = []
    # Edit Gears is for people: a program is told to call the entry with parameters instead
    _, err = gk.cli([run("edit-gears", {"gear": [{"type": "body", "name": "Gear 2"}]})], "--in", save)
    if not err or "interactive_required" not in err:
        problems.append(f"edit-gears from a program: {err or 'ran'}")
    store, err = gk.cli([{"command": "storage.get", "params": {"plugin": "gear-kit"}}], "--in", save)
    if err:
        return problems + [f"storage.get failed: {err[:300]}"]
    record = store["results"][0].get("result", store["results"][0])["document"]["sets"][str(old)]
    if record["entry"] != "spur-gears" or record["params"]["teeth_2"] != 40 or record["bodies"] != ["Gear 1", "Gear 2"]:
        problems.append(f"record: {json.dumps(record)[:300]}")
    steps = [
        {"command": "solid.cylinder", "params": {"radius": 4, "height": 30, "base": [0, 0, -10], "operation": "cut",
                                                  "targets": ["Gear 1"], "name": "Bore"}},
        run("spur-gears", {**record["params"], "teeth_1": 16, "replace": old}),
        {"command": "storage.get", "params": {"plugin": "gear-kit"}},
        {"command": "document.inspect", "params": {"timeline": True, "bodies": False}},
        {"command": "inspect.measure", "params": {"summary": True}},
        INTERFERENCE,
    ]
    r, err = gk.cli(steps, "--in", save, "--save", save)
    if err:
        return problems + [f"edit failed: {err[:500]}"]
    rebuilt, store, doc, measure, inter = [x.get("result", x) for x in r["results"][1:]]
    new = rebuilt["folder"]["id"]
    sets = (store.get("document") or {}).get("sets", {})
    if list(sets) != [str(new)] or sets[str(new)]["params"]["teeth_1"] != 16:
        problems.append(f"record: {json.dumps(sets)[:300]}")
    timeline = doc["timeline"]
    if timeline[-1]["name"] != "Bore":
        problems.append(f"the bore is not last: {[f['name'] for f in timeline][-4:]}")
    failing = [f["name"] for f in timeline if f.get("error")]
    if failing:
        problems.append(f"failing features: {failing}")
    unexpected = [w for w in rebuilt.get("warnings", []) if "undercut" not in w]
    if unexpected:
        problems.append(f"warnings: {unexpected}")
    bodies = measure["bodies"]
    if sorted(b["name"] for b in bodies) != ["Gear 1", "Gear 2"] or not all(b["closed"] for b in bodies):
        problems.append(f"bodies: {[(b['name'], b['closed']) for b in bodies]}")
    # the bore went through the new gear 1 (z 16, m 2, half the pair's backlash): its volume is the
    # profile's minus the hole
    want = expect_area(kind="external", z=16, backlash=0.05) * 10 - math.pi * 16 * 10
    got = next((b["volume_mm3"] for b in bodies if b["name"] == "Gear 1"), 0)
    if abs(got - want) / want > 4e-4:
        problems.append(f"gear 1 volume {got:.2f}, expected {want:.2f} with the bore")
    if sum(h.get("volume_mm3", h.get("volume", 0)) for h in inter.get("interferences", [])) > 1e-3:
        problems.append("the new pair overlaps")
    # a sketch on gear 1's top face would lose it: the next edit is refused, naming it
    steps = [
        {"command": "sketch.create", "params": {"plane": {"face": [8, 0, 10]}, "name": "Mark"}},
        {"command": "sketch.circle.center", "params": {"center": [0, 0], "diameter": 2}},
        {"command": "sketch.finish", "params": {}},
        run("spur-gears", {**record["params"], "teeth_1": 17, "replace": new}),
    ]
    _, err = gk.cli(steps, "--in", save)
    if not err or "“Mark” sits on a face" not in err:
        problems.append(f"an edit that breaks a sketch on a gear face: {err or 'ran'}")
    # gear 1 has a tooth on +X: its tip edge on the top face passes through (18, 0, 10)
    steps = [
        {"command": "solid.fillet", "params": {"edges": [[18, 0, 10]], "radius": 0.3, "tangent_chain": False, "name": "Tip fillet"}},
        run("spur-gears", {**record["params"], "teeth_1": 17, "replace": new}),
    ]
    _, err = gk.cli(steps, "--in", save)
    if not err or "“Tip fillet” sits on a face" not in err:
        problems.append(f"an edit that breaks a fillet on a gear edge: {err or 'ran'}")
    # a second pair with the same names: its bodies are numbered, not the engine's "Body1"
    r, err = gk.cli([run("spur-gears", pair), {"command": "inspect.measure", "params": {"summary": True}}], "--in", save)
    if err:
        problems.append(f"a second build failed: {err[:300]}")
    else:
        names = sorted(b["name"] for b in r["results"][-1]["bodies"])
        if names != ["Gear 1", "Gear 1 (2)", "Gear 2", "Gear 2 (2)"]:
            problems.append(f"bodies after a second build: {names}")
    return problems


def main():
    flt = sys.argv[1] if len(sys.argv) > 1 else ""
    failed = 0
    for k in cases():
        if flt and flt not in k["name"]:
            continue
        problems, r = check(k)
        if not problems and k["mesh"] and not k["expect_overlap"]:
            save = os.path.join(OUT, k["name"].replace(" ", "_").replace("/", "-").replace(":", "") + ".sc")
            problems += nudge_check(k, r, save)
        t = r.get("plugin_ms", "-")
        if problems:
            failed += 1
            print(f"FAIL {k['name']} ({t} ms)")
            for p in problems:
                print("     ", p)
        else:
            v = r["measure"]["total"]["volume_mm3"]
            print(f"ok   {k['name']} ({t} ms, {v:.1f} mm3)")
    if not flt or flt in "edit in place":
        problems = edit_check()
        failed += bool(problems)
        print("FAIL edit in place" if problems else "ok   edit in place")
        for p in problems:
            print("     ", p)
    print("failed:", failed)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
