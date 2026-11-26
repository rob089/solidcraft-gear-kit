#!/usr/bin/env python3
"""Gear Kit's icons in SolidCraft's icon style: isometric, 64×64, palette role colours, ink outlines
(the rules of SolidCraft's crates/look/icons/design/tools/lint_icons.py; tools/icons/check.py).

  spur_gears, bevel_gears, worm_gears   the menu and ribbon icons of the three gear entries
  edit_gears                            the icon of Edit Gears
  timing_pulleys                        the icon of Timing Pulleys
  mate_gear, mate_ring, mate_rack       the pictures of the "Gear 2 type" choices

Blue (active) is what the entry makes, grey (passive) its partner, as in the app's own icons.

Each icon is drawn as a scene of filled polygons in paint order and then fitted into the 64×64
box with a margin, so every icon fills the frame the same way.

usage: tools/icons/make_icons.py icons
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import iso3d  # noqa: E402

# palette sources (crates/look/icons/design/palette.toml)
INK = "#34465b"
P1, P2, P3 = "#e2e8ef", "#b8c3cf", "#8e9faf"      # passive faces, light to dark
A1, A2, A3 = "#b9ddf8", "#328bd3", "#236ead"      # active (result) faces, light to dark
SW = 1.9
STROKE = f'stroke="{INK}" stroke-width="{SW}" stroke-linejoin="round" stroke-linecap="round"'
MARGIN = 3.0


def f(x):
    s = f"{x:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


class Scene:
    """Polygons and polylines in 2D, painted in order, fitted into the icon box at the end."""

    def __init__(self):
        self.items = []          # (kind, [loops], fill, evenodd)

    def poly(self, pts, fill, evenodd=False):
        self.items.append(("poly", [pts], fill, evenodd))

    def holed(self, loops, fill):
        self.items.append(("poly", loops, fill, True))

    def line(self, pts):
        self.items.append(("line", [pts], None, False))

    def flat(self, pts, fill, width):
        """A face without ink: stroked in its own colour so neighbouring faces show no seam."""
        self.items.append(("flat", [pts], fill, width))

    def svg(self, title):
        xs = [x for _, loops, _, _ in self.items for l in loops for x, _ in l]
        ys = [y for _, loops, _, _ in self.items for l in loops for _, y in l]
        x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
        s = min((64 - 2 * MARGIN) / (x1 - x0), (64 - 2 * MARGIN) / (y1 - y0))
        ox = (64 - s * (x1 - x0)) / 2 - s * x0
        oy = (64 - s * (y1 - y0)) / 2 - s * y0
        tr = lambda p: (ox + s * p[0], oy + s * p[1])
        out = []
        for kind, loops, fill, evenodd in self.items:
            d = " ".join("M" + "L".join(f"{f(a)} {f(b)}" for a, b in map(tr, l)) + ("Z" if kind == "poly" else "") for l in loops)
            if kind == "flat":
                out.append(f'<path d="{d}Z" fill="{fill}" stroke="{fill}" stroke-width="{evenodd}" stroke-linejoin="round" stroke-linecap="round"/>')
            elif kind == "poly":
                fr = ' fill-rule="evenodd"' if evenodd else ""
                out.append(f'<path d="{d}" fill="{fill}" {STROKE}{fr}/>')
            else:
                out.append(f'<path d="{d}" fill="none" {STROKE}/>')
        return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">\n  <title>{title}</title>\n'
                + "\n".join("  " + b for b in out) + "\n</svg>\n")


# ------------------------------------------------------------------ 2D gear outlines

# chunky teeth that read at 16 px: wide tips, short height (fractions of the pitch and modules)
TIP, BASE, ADD, DED = 0.36, 0.60, 0.80, 0.95
M = 1.0                                      # module of the drawings


def gear_outline(z, rp, m, phase=0.0, internal=False):
    """Trapezoid teeth around pitch radius rp; internal: the tips point inward."""
    p = 2 * math.pi / z
    r_tip, r_root = (rp - ADD * m, rp + DED * m) if internal else (rp + ADD * m, rp - DED * m)
    pts = []
    for i in range(z):
        c = phase + i * p
        for r, a in ((r_root, -BASE / 2), (r_tip, -TIP / 2), (r_tip, TIP / 2), (r_root, BASE / 2)):
            pts.append((r * math.cos(c + a * p), r * math.sin(c + a * p)))
    return pts


def circle_pts(c, r, n=48):
    """A circle as a polygon."""
    return [(c[0] + r * math.cos(2 * math.pi * i / n), c[1] + r * math.sin(2 * math.pi * i / n)) for i in range(n)]


def shift(pts, c):
    """Points moved by c."""
    return [(x + c[0], y + c[1]) for x, y in pts]


def mesh_centre(c1, r1, r2, ang, internal=False):
    """Centre of a gear of pitch radius r2 meshing with one of r1 at c1, in direction ang."""
    d = r1 - r2 if internal else r1 + r2
    return (c1[0] + d * math.cos(ang), c1[1] + d * math.sin(ang))


def rack_outline(x0, x1, y_pitch, m, depth, phase_x, teeth_up=True):
    """Rack along x, pitch line at y_pitch; teeth toward -y (up on screen) when teeth_up."""
    p = math.pi * m
    sgn = -1 if teeth_up else 1
    y_tip, y_root, y_back = y_pitch + sgn * ADD * m, y_pitch - sgn * DED * m, y_pitch - sgn * depth
    pts = [(x0, y_back), (x0, y_root)]
    x = phase_x - p * math.ceil((phase_x - x0) / p + 1)
    while x - BASE * p / 2 < x1:
        for px, py in ((x - BASE * p / 2, y_root), (x - TIP * p / 2, y_tip), (x + TIP * p / 2, y_tip), (x + BASE * p / 2, y_root)):
            if x0 < px < x1:
                pts.append((px, py))
        x += p
    pts += [(x1, y_root), (x1, y_back)]
    return pts


# ------------------------------------------------------------------ isometric icons

ACT, PAS = (A1, A2, A3), (P1, P2, P3)
H = 3.4                                      # gear thickness in modules


def prism(cols, outline, h, z0=0.0):
    """A vertical prism over a 2D outline (any orientation), axis through its centroid."""
    cx = sum(x for x, _ in outline) / len(outline)
    cy = sum(y for _, y in outline) / len(outline)
    b = iso3d.revolution(cols, (cx, cy, 0), (0, 0, 1), z0 + h / 2)
    lo = [(x, y, z0) for x, y in outline]
    hi = [(x, y, z0 + h) for x, y in outline]
    b.strip(lo, hi)
    b.cap(hi)
    b.cap(lo)
    return b


def b_spur():
    """Gear 1 (blue) in mesh with gear 2 (grey) behind it."""
    z1, z2 = 8, 6
    r1, r2 = z1 * M / 2, z2 * M / 2
    ang = math.atan2(-1.0, -0.2)             # gear 2 behind, above and a little right on screen
    c2 = mesh_centre((0, 0), r1, r2, ang)
    g1 = prism(ACT, gear_outline(z1, r1, M, ang), H)
    g2 = prism(PAS, shift(gear_outline(z2, r2, M, ang + math.pi + math.pi / z2), c2), H)
    sc = Scene()
    iso3d.render(sc, [g2, g1])
    return sc.svg("Spur and helical gears")


def pencil(sc, tip, end, width):
    """A pencil from its tip to the end of its eraser (2D, after the scene is drawn): blue body,
    wooden cone, graphite point, eraser."""
    (tx, ty), (ex, ey) = tip, end
    length = math.hypot(ex - tx, ey - ty)
    u = ((ex - tx) / length, (ey - ty) / length)
    v = (-u[1], u[0])
    at = lambda a, b: (tx + u[0] * a + v[0] * b, ty + u[1] * a + v[1] * b)
    cone, eraser, hw = 0.24 * length, 0.16 * length, width / 2
    body = length - cone - eraser
    sc.poly([at(cone, -hw), at(cone + body, -hw), at(cone + body, hw), at(cone, hw)], A2)
    sc.line([at(cone, 0), at(cone + body, 0)])
    sc.poly([at(cone + body, -hw), at(length, -hw), at(length, hw), at(cone + body, hw)], P2)
    sc.poly([at(0, 0), at(cone, -hw), at(cone, hw)], P1)
    sc.poly([at(0, 0), at(0.4 * cone, -0.4 * hw), at(0.4 * cone, 0.4 * hw)], INK)


def b_edit():
    """The gear pair (grey: what is there) with a pencil (blue: the change) across it."""
    z1, z2 = 8, 6
    r1, r2 = z1 * M / 2, z2 * M / 2
    ang = math.atan2(-1.0, -0.2)
    c2 = mesh_centre((0, 0), r1, r2, ang)
    g1 = prism(PAS, gear_outline(z1, r1, M, ang), H)
    g2 = prism(PAS, shift(gear_outline(z2, r2, M, ang + math.pi + math.pi / z2), c2), H)
    sc = Scene()
    iso3d.render(sc, [g2, g1])
    xs = [x for _, loops, _, _ in sc.items for l in loops for x, _ in l]
    ys = [y for _, loops, _, _ in sc.items for l in loops for _, y in l]
    x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
    w, h = x1 - x0, y1 - y0
    pencil(sc, (x0 + 0.34 * w, y0 + 0.97 * h), (x1 + 0.04 * w, y0 + 0.30 * h), 0.19 * max(w, h))
    return sc.svg("Edit gears")


def hull(pts):
    """Convex hull, counterclockwise (monotone chain)."""
    pts = sorted(set(pts))
    cross = lambda o, a, b: (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    lo, hi = [], []
    for p in pts:
        while len(lo) >= 2 and cross(lo[-2], lo[-1], p) <= 0:
            lo.pop()
        lo.append(p)
    for p in reversed(pts):
        while len(hi) >= 2 and cross(hi[-2], hi[-1], p) <= 0:
            hi.pop()
        hi.append(p)
    return lo[:-1] + hi[:-1]


def pulley(cols, c, r, z, h, flange=0.75, ft=0.45):
    """A flanged timing pulley on a vertical axis at c: flange, grooved core, flange."""
    core = shift(gear_outline(z, r, 0.55, math.pi / z), c)
    disc = shift(circle_pts((0, 0), r + flange, 24), c)
    return [prism(cols, disc, ft), prism(cols, core, h - 2 * ft, ft), prism(cols, disc, ft, h - ft)]


def b_pulleys():
    """Pulley 1 (blue) and pulley 2 (grey, larger, behind) with the belt round both."""
    r1, r2, h = 2.6, 4.2, 3.0
    ang = math.atan2(-1.0, -0.2)
    d = 10.5
    c2 = (d * math.cos(ang), d * math.sin(ang))
    t = 0.45                                   # belt thickness
    lo, hi = 0.75, h - 0.75                    # the belt runs between the flanges
    ring = lambda r_add: hull(circle_pts((0, 0), r1 + r_add, 24) + circle_pts(c2, r2 + r_add, 24))
    outer, inner = ring(0.2 + t), ring(0.2)
    mid = ((c2[0]) / 2, c2[1] / 2)
    # outward: away from the line between the axes, and up/down from the belt's middle height
    def out(p):
        ex, ey = c2[0] / d, c2[1] / d
        rx, ry = p[0] - mid[0], p[1] - mid[1]
        along = rx * ex + ry * ey
        nx, ny = rx - along * ex, ry - along * ey
        k = max(0.0, abs(along) - d / 2 + 1.0)
        nx += k * ex * (1 if along > 0 else -1)
        ny += k * ey * (1 if along > 0 else -1)
        return (nx, ny, p[2] - (lo + hi) / 2)
    belt = iso3d.Body(PAS, out)
    belt.strip([(x, y, lo) for x, y in outer], [(x, y, hi) for x, y in outer])
    belt.cap([(x, y, hi) for x, y in outer], [(x, y, hi) for x, y in inner])
    sc = Scene()
    iso3d.render(sc, pulley(PAS, c2, r2, 16, h) + [belt] + pulley(ACT, (0, 0), r1, 10, h))
    return sc.svg("Timing pulleys")


# ------------------------------------------------------------------ dropdown pictures
#
# A choice's pictures are drawn in a single colour (the text colour), so they are flat top views:
# the chosen part solid, its partner (gear 1) as an outline, a knocked-out gap between them.

GAP = "#ffffff"                              # the palette's gap role: erases what is below


class Flat:
    """Loops in 2D for a single-colour picture, painted in order and fitted into the box."""

    def __init__(self):
        self.items = []                      # (kind, loops)

    def outline(self, *loops):
        self.items.append(("outline", loops))

    def solid(self, *loops):
        self.items.append(("gap", loops))
        self.items.append(("solid", loops))

    def hole(self, *loops):
        self.items.append(("hole", loops))

    def svg(self, title, margin=4.0):
        pts = [p for _, loops in self.items for l in loops for p in l]
        x0, x1 = min(p[0] for p in pts), max(p[0] for p in pts)
        y0, y1 = min(p[1] for p in pts), max(p[1] for p in pts)
        k = min((64 - 2 * margin) / (x1 - x0), (64 - 2 * margin) / (y1 - y0))
        ox, oy = (64 - k * (x1 - x0)) / 2 - k * x0, (64 - k * (y1 - y0)) / 2 - k * y0
        d = lambda loops: " ".join("M" + "L".join(f"{f(ox + k * x)} {f(oy + k * y)}" for x, y in l) + "Z" for l in loops)
        rnd = 'stroke-linejoin="round" stroke-linecap="round"'
        out = []
        for kind, loops in self.items:
            if kind == "outline":
                out.append(f'<path d="{d(loops)}" fill="none" stroke="{INK}" stroke-width="3.2" {rnd}/>')
            elif kind == "gap":
                out.append(f'<path d="{d(loops)}" fill="{GAP}" stroke="{GAP}" stroke-width="5" {rnd} fill-rule="evenodd"/>')
            elif kind == "solid":
                out.append(f'<path d="{d(loops)}" fill="{INK}" fill-rule="evenodd"/>')
            else:
                out.append(f'<path d="{d(loops)}" fill="{GAP}"/>')
        return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">\n  <title>{title}</title>\n'
                + "\n".join("  " + b for b in out) + "\n</svg>\n")


def flat_partner(fl, c, z, phase):
    """Gear 1: an outlined pinion with its bore."""
    r = z * M / 2
    fl.outline(shift(gear_outline(z, r, M, phase), c), circle_pts(c, 0.9 * M, 20))


def b_mate_gear():
    z1, z2 = 8, 12
    r1, r2 = z1 * M / 2, z2 * M / 2
    ang = math.radians(-35)
    c2 = mesh_centre((0, 0), r1, r2, ang)
    fl = Flat()
    flat_partner(fl, (0, 0), z1, ang)
    fl.solid(shift(gear_outline(z2, r2, M, ang + math.pi + math.pi / z2), c2))
    fl.hole(circle_pts(c2, 1.4 * M, 24))
    return fl.svg("External gear")


def b_mate_ring():
    zr, zp = 16, 8
    rr, rp = zr * M / 2, zp * M / 2
    ang = math.radians(-135)
    cp = mesh_centre((0, 0), rr, rp, ang, internal=True)
    fl = Flat()
    flat_partner(fl, cp, zp, ang)
    fl.solid(circle_pts((0, 0), rr + 2.6 * M, 64), gear_outline(zr, rr, M, ang + math.pi / zr, internal=True))
    return fl.svg("Ring gear")


def b_mate_rack():
    zp = 8
    rp = zp * M / 2
    fl = Flat()
    flat_partner(fl, (0, 0), zp, math.pi / 2)
    fl.solid(rack_outline(-7.0, 7.0, rp, M, 2.6, math.pi * M / 2))
    return fl.svg("Rack")


def b_bevel():
    """90° bevel pair sharing the apex at the origin: gear 1 below it (axis -Z), gear 2 behind
    (axis -Y), so both show their toothed cones and small ends."""
    z1 = z2 = 10
    d1 = math.radians(45)
    d2 = math.radians(90) - d1
    a0, a1 = 7.0, 10.0                       # distances from the apex: small end, large end

    def ring(z, delta, d, phase, place):
        rp = d * math.tan(delta)
        return [place(d, u, v) for u, v in gear_outline(z, rp, 1.5 * rp / z, phase)]

    def gear(cols, z, delta, axis, place, phase):
        b = iso3d.revolution(cols, (0, 0, 0), axis, (a0 + a1) / 2)
        small, large = ring(z, delta, a0, phase, place), ring(z, delta, a1, phase, place)
        b.strip(large, small)
        b.cap(small)
        b.cap(large)
        return b

    g1 = gear(ACT, z1, d1, (0, 0, -1), lambda d, u, v: (u, v, -d), -math.pi / 2)
    g2 = gear(PAS, z2, d2, (0, -1, 0), lambda d, u, v: (u, -d, v), -math.pi / 2 + math.pi / z2)
    sc = Scene()
    iso3d.render(sc, [g2, g1])
    return sc.svg("Bevel gears")


def b_worm():
    """The worm wheel lying flat (axis Z); the worm behind it along X, at mid height."""
    z = 9
    r = z * M / 2
    h = 3.2
    wheel = prism(PAS, gear_outline(z, r, M, math.pi / 2 + math.pi / z), h)
    rw = 2.5
    yc, zc = -(r + rw - 0.35 * M), h / 2
    L = r + 2.0
    n = 28
    worm = iso3d.revolution(ACT, (0, yc, zc), (1, 0, 0), 0.0)
    ring = lambda x: [(x, yc + rw * math.cos(2 * math.pi * i / n), zc + rw * math.sin(2 * math.pi * i / n)) for i in range(n)]
    worm.strip(ring(-L), ring(L))
    worm.cap(ring(-L))
    worm.cap(ring(L))
    # thread crests: a helix on the cylinder, where it faces the viewer
    lead = math.pi * M
    lines, seg = [], []
    steps = 480
    turns = 2 * L / lead
    for i in range(steps + 1):
        th = 2 * math.pi * turns * i / steps
        x = -L + lead * th / (2 * math.pi)
        c, s = math.cos(th), math.sin(th)
        p = (x, yc + rw * c, zc + rw * s)
        if c + s > 0.25 and -L + 0.3 < x < L - 0.3:
            seg.append(p)
        else:
            if len(seg) > 1:
                lines.append((seg, 0.3))
            seg = []
    if len(seg) > 1:
        lines.append((seg, 0.3))
    sc = Scene()
    iso3d.render(sc, [worm, wheel], lines)
    return sc.svg("Worm gears")


ICONS = {
    "spur_gears": b_spur,
    "bevel_gears": b_bevel,
    "worm_gears": b_worm,
    "edit_gears": b_edit,
    "timing_pulleys": b_pulleys,
    "mate_gear": b_mate_gear,
    "mate_ring": b_mate_ring,
    "mate_rack": b_mate_rack,
}



def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    for name, fn in ICONS.items():
        with open(os.path.join(out, name + ".svg"), "w") as f:
            f.write(fn())
    print("wrote", len(ICONS), "icons to", out)


if __name__ == "__main__":
    main()
