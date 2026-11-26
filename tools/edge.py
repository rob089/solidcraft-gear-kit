#!/usr/bin/env python3
"""Edge cases and bad inputs on the real host: each case either builds closed bodies or fails with
a message (printed). Run after ./build.sh.

    tools/edge.py [first-index] [count]
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import run as gk  # noqa: E402

ONE = {"create_2": False}
RING = {"mate": "ring", "create_1": False}
RACK = {"mate": "rack", "create_1": False}
CASES = [
    ("spur-gears", {**ONE, "teeth_1": 4}),
    ("spur-gears", {**ONE, "teeth_1": 6, "profile_shift_1": 0.6}),
    ("spur-gears", {**ONE, "teeth_1": 400, "module": 0.5, "face_width": 3}),
    ("spur-gears", {**ONE, "teeth_1": 200, "module": 1, "helix_angle": 30, "face_width": 10}),
    ("spur-gears", {**ONE, "teeth_1": 40, "helix_angle": 45, "herringbone": True, "face_width": 30}),
    ("spur-gears", {**ONE, "teeth_1": 20, "module": 0.2, "face_width": 1}),
    ("spur-gears", {**ONE, "teeth_1": 20, "module": 50, "face_width": 100}),
    ("spur-gears", {**ONE, "teeth_1": 20, "herringbone": True}),
    ("spur-gears", {**ONE, "teeth_1": 20, "pressure_angle": 14.5}),
    ("spur-gears", {**ONE, "teeth_1": 20, "pressure_angle": 30, "profile_shift_1": 1.0}),
    ("spur-gears", {**ONE, "teeth_1": 24, "profile_shift_1": 1.2}),
    ("spur-gears", {**RING, "teeth_2": 12, "teeth_1": 6}),
    ("spur-gears", {**RING, "teeth_2": 30}),
    ("spur-gears", {**RING, "teeth_2": 300, "module": 1, "face_width": 5}),
    ("spur-gears", {**RACK, "teeth_2": 200, "module": 1}),
    ("spur-gears", {**RACK, "teeth_2": 10, "rack_height": 2}),
    ("spur-gears", {"teeth_1": 150, "teeth_2": 150, "helix_angle": 20}),
    ("spur-gears", {"create_1": False, "create_2": False}),
    ("spur-gears", {"mate": "ring", "teeth_1": 20, "teeth_2": 24}),
    ("spur-gears", {"teeth_1": 24, "teeth_2": 24, "profile_shift_1": -0.5, "profile_shift_2": -0.5}),
    ("spur-gears", {"helix_angle": 0, "herringbone": True}),
    ("bevel-gears", {"teeth_1": 6, "teeth_2": 6, "face_width": 2}),
    ("bevel-gears", {"teeth_1": 10, "teeth_2": 60, "face_width": 12}),
    ("bevel-gears", {"teeth_1": 20, "teeth_2": 20, "shaft_angle": 150, "face_width": 5}),
    ("bevel-gears", {"teeth_1": 15, "teeth_2": 30, "face_width": 20}),
    ("worm-gears", {"module": 0.5, "worm_diameter": 5, "wheel_teeth": 60, "worm_length": 15, "wheel_face_width": 4}),
    ("worm-gears", {"starts": 4, "wheel_teeth": 40, "worm_diameter": 24}),
    ("worm-gears", {"wheel_teeth": 100, "module": 1, "worm_diameter": 10, "wheel_face_width": 6}),
    ("worm-gears", {"wheel_teeth": 10}),
    ("worm-gears", {"pressure_angle": 30, "wheel_teeth": 20}),
]


def main():
    first = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    count = int(sys.argv[2]) if len(sys.argv) > 2 else len(CASES)
    for i, (entry, params) in enumerate(CASES[first:first + count], first):
        r = gk.run(entry, params)
        if r["ok"]:
            b = r["measure"]["bodies"]
            closed = all(x["closed"] for x in b)
            print(f"{i:2d} ok   {r['plugin_ms']:6d} ms  {entry} {json.dumps(params)}  bodies={len(b)} closed={closed}", flush=True)
        else:
            err = r["error"]
            print(f"{i:2d} ERR  {entry} {json.dumps(params)} -> {err[err.find('plugin'):][:230]}", flush=True)


if __name__ == "__main__":
    main()
