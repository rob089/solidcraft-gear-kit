#!/usr/bin/env python3
"""Check icons against the rules SolidCraft applies to a plugin's images (its icon lint,
crates/look/icons/design/tools/lint_icons.py, and docs/plugins.md "Icons and pictures"):
viewBox 0 0 64 64, only <svg title path circle defs mask clipPath g>, no transform/style/class,
path data in absolute M L C Q A Z only, colours from the icon palette (or currentColor), round
joins and caps on every stroke, at most 64 KiB.

usage: tools/icons/check.py icons/*.svg
"""
import os
import re
import sys
import xml.etree.ElementTree as ET

# the `source` colours of crates/look/icons/design/palette.toml (SolidCraft 0.46)
PALETTE = {
    "#34465b",                                  # ink, line
    "#e2e8ef", "#b8c3cf", "#8e9faf",            # passive 1-3
    "#ffffff",                                  # gap
    "#6f879c",                                  # guide
    "#b9ddf8", "#328bd3", "#236ead",            # active 1-3
    "#f1d2af", "#c06f00",                       # component orange (inspection icons)
}
TAGS = {"svg", "title", "path", "circle", "defs", "mask", "clipPath", "g"}
NS = "{http://www.w3.org/2000/svg}"

bad = 0
for f in sys.argv[1:]:
    errs = []
    root = ET.parse(f).getroot()
    if root.get("viewBox") != "0 0 64 64":
        errs.append("viewBox must be 0 0 64 64")
    for a in ("width", "height"):
        if root.get(a):
            errs.append(f"{a} attribute on <svg>")
    for e in root.iter():
        tag = e.tag.replace(NS, "")
        if tag not in TAGS:
            errs.append(f"<{tag}> not allowed")
        for a in ("transform", "style", "class"):
            if e.get(a):
                errs.append(f"{a} on <{tag}>")
        if tag == "path" and not re.fullmatch(r"[MLCQAZ0-9 .\-]+", e.get("d", "")):
            errs.append("path data: absolute M L C Q A Z only")
        for k in ("fill", "stroke"):
            v = e.get(k)
            if v in (None, "none", "currentColor") or v.startswith("url("):
                continue
            if v.lower() not in PALETTE:
                errs.append(f'{k}="{v}" is not a palette colour')
        if e.get("stroke") not in (None, "none"):
            if not e.get("stroke-width") or e.get("stroke-linejoin") != "round" or e.get("stroke-linecap") != "round":
                errs.append("strokes need a width and round joins and caps")
    size = os.path.getsize(f)
    if size > 64 * 1024:
        errs.append(f"{size} bytes, over 64 KiB")
    errs = sorted(set(errs))
    print(("ok   " if not errs else "FAIL ") + f"{f} ({size} bytes)" + ("" if not errs else ": " + "; ".join(errs)))
    bad += bool(errs)
sys.exit(1 if bad else 0)
