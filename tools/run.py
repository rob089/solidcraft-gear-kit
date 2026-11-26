#!/usr/bin/env python3
"""Run one Gear Kit entry through solidcraft-cli on the real plugin host.

    tools/run.py ENTRY '{"teeth": 20}' [--save out.sc] [--png out.png] [--view iso]

Inspects dist/gear-kit (to get the approval hash), runs the entry in a fresh design, measures
the bodies and prints the plugin's reply, the time and the measurement as JSON.
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
PKG = os.path.join(HERE, "..", "dist", "gear-kit")
CLI = os.environ.get("SOLIDCRAFT_CLI", "/Applications/SolidCraft.app/Contents/MacOS/solidcraft-cli")


def cli(steps, *extra):
    """Run a script of steps; returns (results, None) or (None, error text)."""
    with tempfile.TemporaryDirectory() as d:
        script = os.path.join(d, "s.json")
        out = os.path.join(d, "o.json")
        with open(script, "w") as f:
            json.dump(steps, f)
        p = subprocess.run([CLI, "run", script, "--results", "--timings", "--quiet", "--json-out", out, *extra],
                           capture_output=True, text=True)
        if p.returncode != 0:
            return None, (p.stderr or p.stdout).strip()
        with open(out) as f:
            return json.load(f), None


def approval(pkg):
    """The approval hash SolidCraft wants for exactly this package."""
    r, err = cli([{"command": "plugin.inspect", "params": {"path": pkg}}])
    if err:
        sys.exit(f"inspect failed: {err}")
    res = r["results"][0] if isinstance(r, dict) else r[0]
    return res["approval"]


def run(entry, params, save=None, pkg=PKG, extra_steps=()):
    """Run `entry` in a fresh design, then `extra_steps`, then measure. Returns a dict with `ok`
    and either `error` or the plugin's time, its reply, the extra steps' results and the
    measurement."""
    pkg = os.path.abspath(pkg)
    a = approval(pkg)
    steps = [{"command": "plugin.run", "params": {"path": pkg, "approval": a, "entry": entry, "params": params}}]
    steps += list(extra_steps)
    steps.append({"command": "inspect.measure", "params": {"summary": True}})
    args = ["--save", save] if save else []
    r, err = cli(steps, *args)
    if err:
        return {"ok": False, "error": err}
    results = r["results"]
    return {
        "ok": True,
        "plugin_ms": round(r["timings"][0]["ms"]),
        "result": results[0].get("result"),
        "extra": results[1:-1],
        "measure": results[-1],
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("entry")
    ap.add_argument("params", nargs="?", default="{}")
    ap.add_argument("--save")
    ap.add_argument("--png")
    ap.add_argument("--view", default="iso")
    a = ap.parse_args()
    save = a.save
    if a.png and not save:
        save = os.path.join(tempfile.mkdtemp(), "g.sc")
    out = run(a.entry, json.loads(a.params), save)
    print(json.dumps(out, indent=1))
    if a.png and out.get("ok"):
        subprocess.run([CLI, "snapshot", "--in", save, "--out", a.png, "--view", a.view, "--width", "900", "--height", "700",
                        "--no-grid"], check=True)


if __name__ == "__main__":
    main()
