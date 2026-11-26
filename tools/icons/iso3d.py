"""A tiny isometric renderer for the icons.

View: screen u = (x - y)·cos30, v = (x + y)·sin30 - z, so the viewer looks along -(1, 1, 1) and a
point's nearness is x + y + z. A body is made of wall strips (quads between two rings) and caps.
Walls are filled without outlines, as the app's model icons draw curved and many-faceted sides;
ink lines go only where the eye needs them: cap outlines, the silhouette of the walls (where a
wall turns away) and the walls' free edges. Shading as in the app: faces turned up get the
lightest colour, faces toward +y (left on screen) the middle, toward +x (right) the darkest.
"""
import math

SEAM = 0.9          # walls are stroked in their own colour, so neighbours show no hairline seam


def iso(p):
    x, y, z = p
    return ((x - y) * 0.866, (x + y) * 0.5 - z)


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def newell(pts):
    n = [0.0, 0.0, 0.0]
    for i, p in enumerate(pts):
        q = pts[(i + 1) % len(pts)]
        n[0] += (p[1] - q[1]) * (p[2] + q[2])
        n[1] += (p[2] - q[2]) * (p[0] + q[0])
        n[2] += (p[0] - q[0]) * (p[1] + q[1])
    return tuple(n)


def centroid(pts):
    k = len(pts)
    return (sum(p[0] for p in pts) / k, sum(p[1] for p in pts) / k, sum(p[2] for p in pts) / k)


VIEW = (1.0, 1.0, 1.0)


def near(p):
    return p[0] + p[1] + p[2]


def shade(n, cols):
    top, left, right = cols
    l = math.sqrt(dot(n, n)) or 1.0
    nx, ny, nz = n[0] / l, n[1] / l, n[2] / l
    if nz >= max(nx, ny) - 0.05:
        return top
    return right if nx > ny else left


class Body:
    """`outward(point)`: a direction out of the body near a point, to orient the faces."""

    def __init__(self, cols, outward):
        self.cols = cols
        self.outward = outward
        self.strips = []        # (ring_a, ring_b)
        self.caps = []          # list of loops (first outer, rest holes)

    def strip(self, ring_a, ring_b):
        self.strips.append((ring_a, ring_b))

    def cap(self, *loops):
        self.caps.append(loops)

    def _orient(self, pts):
        n = newell(pts)
        return n if dot(n, self.outward(centroid(pts))) >= 0 else (-n[0], -n[1], -n[2])

    def items(self):
        out = []
        for ra, rb in self.strips:
            k = len(ra)
            quads = [[ra[i], ra[(i + 1) % k], rb[(i + 1) % k], rb[i]] for i in range(k)]
            norms = [self._orient(q) for q in quads]
            vis = [dot(n, VIEW) > 1e-9 for n in norms]
            for q, n, v in zip(quads, norms, vis):
                if v:
                    out.append((near(centroid(q)), "wall", q, shade(n, self.cols)))
            for i in range(k):
                q = quads[i]
                if vis[i]:
                    # free edges along both rings
                    for a, b in ((q[0], q[1]), (q[3], q[2])):
                        out.append((near(centroid(q)) + 0.01, "line", [a, b], None))
                if vis[i] != vis[i - 1]:
                    # silhouette: the edge between a wall that faces us and one that turns away
                    a, b = ra[i], rb[i]
                    out.append((near(centroid(q if vis[i] else quads[i - 1])) + 0.01, "line", [a, b], None))
        for loops in self.caps:
            n = self._orient(loops[0])
            if dot(n, VIEW) > 1e-9:
                out.append((near(centroid(loops[0])), "cap", loops, shade(n, self.cols)))
        return out


def render(scene, bodies, lines=()):
    items = []
    for b in bodies:
        items += b.items()
    for pts, lift in lines:
        for a, c in zip(pts, pts[1:]):
            items.append((near(centroid([a, c])) + lift, "line", [a, c], None))
    items.sort(key=lambda t: t[0])
    for _, kind, geo, col in items:
        if kind == "wall":
            scene.flat([iso(p) for p in geo], col, SEAM)
        elif kind == "cap":
            loops = [[iso(p) for p in l] for l in geo]
            if len(loops) == 1:
                scene.poly(loops[0], col)
            else:
                scene.holed(loops, col)
        else:
            scene.line([iso(p) for p in geo])


def revolution(cols, axis_pt, axis_dir, mid):
    """A body around an axis: outward is the radial part plus the axial offset from `mid`."""
    def out(c):
        rel = sub(c, axis_pt)
        t = dot(rel, axis_dir)
        radial = (rel[0] - t * axis_dir[0], rel[1] - t * axis_dir[1], rel[2] - t * axis_dir[2])
        k = t - mid
        return (radial[0] + k * axis_dir[0], radial[1] + k * axis_dir[1], radial[2] + k * axis_dir[2])
    return Body(cols, out)
