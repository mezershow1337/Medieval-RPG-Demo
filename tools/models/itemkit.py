"""Инструменты для 3D-моделей предметов: геометрия, процедурные материалы,
рендер иконки, запекание текстур и экспорт в .glb.

Сами предметы описаны в build_items.py. Здесь — только общая «кухня».

Соглашения (Blender, метры, Z вверх):
* Предмет строится в «витринной» позе: длинная ось по Z, лицевая сторона смотрит в -Y.
  Иконка снимается спереди (камера в -Y), чуть сбоку и сверху.
* Поза на земле: "flat" — предмет кладётся лицом вверх (поворот -90° вокруг X),
  "upright" — стоит как построен (шлем, сапоги, фляга). В .glb он уже лежит
  на земле: низ на z=0, центр над началом координат.
* Все детали строятся сразу в мировых координатах (без трансформаций объектов),
  потому что процедурные текстуры берут координаты объекта.
"""
import math
import os

import bpy  # noqa: E402  (bpy раньше bmesh)
import bmesh
from mathutils import Matrix, Vector

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CHAR_TEX = os.path.join(ROOT, "assets", "models", "textures")
OUT_MODELS = os.path.join(ROOT, "assets", "models", "items")
OUT_ICONS = os.path.join(ROOT, "assets", "icons", "items")

# Пикселей на клетку инвентаря в файле иконки (в игре клетка 44 px; запас под 4K).
ICON_PX_PER_CELL = 128
ICON_SAMPLES = 160
BAKE_SIZE = 512


# =========================================================================== сцена


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    MATS.clear()


def link(obj):
    bpy.context.scene.collection.objects.link(obj)
    return obj


# =========================================================================== геометрия
# Все функции возвращают bmesh; превращает его в объект `mesh_obj`.


def lathe(rings, segs=32, cap_top=True, cap_bot=True, axis="Z"):
    """Тело вращения. rings — список (h, rx[, ry]) от h меньшего к большему,
    r=0 — полюс. axis — ось вращения ("Z", "X" или "Y")."""
    bm = bmesh.new()
    loops = []
    for ring in rings:
        h, rx = ring[0], ring[1]
        ry = ring[2] if len(ring) > 2 else rx
        if rx <= 1e-6:
            loops.append([bm.verts.new(_axis_pt(axis, 0.0, 0.0, h))])
        else:
            loops.append([bm.verts.new(_axis_pt(axis, rx * math.cos(2 * math.pi * i / segs), ry * math.sin(2 * math.pi * i / segs), h)) for i in range(segs)])
    for a, b in zip(loops, loops[1:]):
        for i in range(segs):
            j = (i + 1) % segs
            if len(a) == 1 and len(b) == 1:
                continue
            if len(a) == 1:
                bm.faces.new((a[0], b[i], b[j]))
            elif len(b) == 1:
                bm.faces.new((a[i], b[0], a[j]))
            else:
                bm.faces.new((a[i], b[i], b[j], a[j]))
    if cap_bot and len(loops[0]) > 1:
        bm.faces.new(list(reversed(loops[0])))
    if cap_top and len(loops[-1]) > 1:
        bm.faces.new(loops[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    return bm


def partial_lathe(rings, a0, a1, segs=24):
    """Открытая поверхность вращения вокруг Z на дуге [a0, a1] градусов
    (0° — спереди, -Y; 90° — справа, +X). rings: (z, rx[, ry]). Толщину даёт solidify."""
    rows = []
    for ring in rings:
        z, rx = ring[0], ring[1]
        ry = ring[2] if len(ring) > 2 else rx
        row = []
        for i in range(segs + 1):
            a = math.radians(a0 + (a1 - a0) * i / segs)
            row.append((rx * math.sin(a), -ry * math.cos(a), z))
        rows.append(row)
    return grid_surface(rows)


def circle_path(center, radius, plane="XZ", n=48, start=0.0, end=360.0, ry=None):
    cx, cy, cz = center
    ry = radius if ry is None else ry
    out = []
    for i in range(n + 1):
        a = math.radians(start + (end - start) * i / n)
        u, v = radius * math.cos(a), ry * math.sin(a)
        if plane == "XZ":
            out.append((cx + u, cy, cz + v))
        elif plane == "XY":
            out.append((cx + u, cy + v, cz))
        else:
            out.append((cx, cy + u, cz + v))
    return out


def torus(center, radius, r_tube, plane="XY", n=48, segs=12, ry=None, tube_ry=None):
    path = circle_path(center, radius, plane, n, ry=ry)[:-1]
    up = {"XY": (0, 0, 1), "XZ": (0, 1, 0), "YZ": (1, 0, 0)}[plane]
    return loft(path, [(r_tube, tube_ry or r_tube)] * len(path), segs, up=up, closed=True)


def _axis_pt(axis, a, b, h):
    if axis == "Z":
        return (a, b, h)
    if axis == "X":
        return (h, a, b)
    return (a, h, b)


def box(x0, x1, y0, y1, z0, z1):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co = Vector((x0 + (v.co.x + 0.5) * (x1 - x0), y0 + (v.co.y + 0.5) * (y1 - y0), z0 + (v.co.z + 0.5) * (z1 - z0)))
    return bm


def cbox(cx, cy, cz, sx, sy, sz):
    """Коробка по центру и размерам."""
    return box(cx - sx / 2, cx + sx / 2, cy - sy / 2, cy + sy / 2, cz - sz / 2, cz + sz / 2)


def grid_surface(rows, closed_u=False, cap=False):
    """Поверхность из сетки точек rows[i][j] (i — вдоль, j — поперёк)."""
    bm = bmesh.new()
    vs = [[bm.verts.new(p) for p in row] for row in rows]
    n = len(rows[0])
    for a, b in zip(vs, vs[1:]):
        rng = range(n) if closed_u else range(n - 1)
        for j in rng:
            k = (j + 1) % n
            try:
                bm.faces.new((a[j], a[k], b[k], b[j]))
            except ValueError:
                pass
    if cap and closed_u:
        bm.faces.new(list(reversed(vs[0])))
        bm.faces.new(vs[-1])
    bmesh.ops.remove_doubles(bm, verts=bm.verts[:], dist=1e-5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    return bm


def loft(path, radii, segs=16, cap_start=True, cap_end=True, up=(0, 0, 1), twist=0.0, closed=False):
    """Труба вдоль ломаной path с эллиптическим сечением radii[i] = r или (rx, ry).
    Рамка переносится параллельно, так что труба не перекручивается."""
    pts = [Vector(p) for p in path]
    tangents = []
    for i in range(len(pts)):
        if closed:
            a, b = pts[i - 1], pts[(i + 1) % len(pts)]
        else:
            a = pts[max(i - 1, 0)]
            b = pts[min(i + 1, len(pts) - 1)]
        tangents.append((b - a).normalized())
    upv = Vector(up)
    if abs(tangents[0].dot(upv)) > 0.95:
        upv = Vector((1, 0, 0)) if abs(tangents[0].x) < 0.9 else Vector((0, 1, 0))
    side = tangents[0].cross(upv).normalized()
    normal = side.cross(tangents[0]).normalized()
    rows = []
    for i, (p, t) in enumerate(zip(pts, tangents)):
        if i > 0:
            # Параллельный перенос рамки.
            side = (side - t * side.dot(t)).normalized()
            normal = side.cross(t).normalized()
        r = radii[i]
        rx, ry = (r, r) if isinstance(r, (int, float)) else r
        ang0 = twist * i / max(len(pts) - 1, 1)
        row = []
        for j in range(segs):
            a = 2 * math.pi * j / segs + ang0
            row.append(p + side * (math.cos(a) * rx) + normal * (math.sin(a) * ry))
        rows.append(row)
    bm = bmesh.new()
    vs = [[bm.verts.new(q) for q in row] for row in rows]
    pairs = list(zip(vs, vs[1:])) + ([(vs[-1], vs[0])] if closed else [])
    for a, b in pairs:
        for j in range(segs):
            k = (j + 1) % segs
            bm.faces.new((a[j], a[k], b[k], b[j]))
    if closed:
        cap_start = cap_end = False
    if cap_start:
        bm.faces.new(list(reversed(vs[0])))
    if cap_end:
        bm.faces.new(vs[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    return bm


def bezier(p0, p1, p2, p3, n=16):
    p0, p1, p2, p3 = map(Vector, (p0, p1, p2, p3))
    out = []
    for i in range(n + 1):
        t = i / n
        u = 1 - t
        out.append(p0 * u**3 + p1 * 3 * u * u * t + p2 * 3 * u * t * t + p3 * t**3)
    return out


def blade(stations, samples=10, fuller=None, edge=0.0006):
    """Клинок вдоль Z с ромбовидно-линзовидным сечением.
    stations: (z, полуширина, полутолщина) снизу вверх; полуширина 0 — острие.
    fuller: (z0, z1, доля ширины, глубина) — дол посередине клинка."""
    rows = []
    for st in stations:
        z, w, t = st[:3]
        c = st[3] if len(st) > 3 else 0.0
        w = max(w, 1e-4)
        row = []
        # Обходим сечение: лицевая сторона (-Y) слева направо, затем тыльная справа налево.
        us = [-1 + 2 * i / samples for i in range(samples + 1)]
        front, back = [], []
        for u in us:
            # Линза с тонкой «подточкой» у кромки.
            th = t * (1 - abs(u) ** 2.2) ** 0.85 + edge * (1 - abs(u))
            if fuller and fuller[0] <= z <= fuller[1]:
                fw = fuller[2]
                if abs(u) < fw:
                    k = 1 - (abs(u) / fw) ** 2
                    # Плавный вход в дол по длине.
                    fade = min(1.0, (z - fuller[0]) / 0.03, (fuller[1] - z) / 0.06)
                    th -= fuller[3] * k * max(fade, 0.0)
            th = max(th, 0.0003)
            front.append((c + u * w, -th, z))
            back.append((c + u * w, th, z))
        row = front + list(reversed(back[1:-1]))
        rows.append(row)
    return grid_surface(rows, closed_u=True, cap=True)


def merge(*bms):
    out = bmesh.new()
    for b in bms:
        me = bpy.data.meshes.new("tmp")
        b.to_mesh(me)
        out.from_mesh(me)
        bpy.data.meshes.remove(me)
        b.free()
    return out


def transform(bm, mat):
    bmesh.ops.transform(bm, matrix=mat, verts=bm.verts[:])
    return bm


def rot(bm, axis, deg, center=(0, 0, 0)):
    c = Vector(center)
    m = Matrix.Translation(c) @ Matrix.Rotation(math.radians(deg), 4, axis) @ Matrix.Translation(-c)
    return transform(bm, m)


def move(bm, x=0.0, y=0.0, z=0.0):
    return transform(bm, Matrix.Translation((x, y, z)))


def scale(bm, sx, sy=None, sz=None, center=(0, 0, 0)):
    sy = sx if sy is None else sy
    sz = sx if sz is None else sz
    c = Vector(center)
    m = Matrix.Translation(c) @ Matrix.Diagonal((sx, sy, sz, 1)) @ Matrix.Translation(-c)
    return transform(bm, m)


def deform(bm, fn):
    for v in bm.verts:
        v.co = Vector(fn(v.co.copy()))
    return bm


def mirror_x(bm):
    """Зеркальная копия по X (нормали исправляются)."""
    bm = transform(bm, Matrix.Diagonal((-1, 1, 1, 1)))
    bmesh.ops.reverse_faces(bm, faces=bm.faces[:])
    return bm


def copy(bm):
    return bm.copy()


PARTS = []


def mesh_obj(name, bm, mat, subsurf=0, bevel=0.0, smooth_angle=40.0, solidify=0.0, glass=False):
    """Создаёт объект из bmesh с материалом `mat` (имя из MATS).
    bevel — фаска рёбер в метрах (для металла и дерева: блики на кромках)."""
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = link(bpy.data.objects.new(name, me))
    me.materials.append(MATS[mat])
    if solidify:
        m = obj.modifiers.new("solid", "SOLIDIFY")
        m.thickness = solidify
        m.offset = 0.0
    if bevel:
        m = obj.modifiers.new("bevel", "BEVEL")
        m.width = bevel
        m.segments = 2
        m.limit_method = "ANGLE"
        m.angle_limit = math.radians(35)
        m.harden_normals = False
    if subsurf:
        m = obj.modifiers.new("sub", "SUBSURF")
        m.levels = subsurf
        m.render_levels = subsurf
    apply_modifiers(obj)
    obj.data.set_sharp_from_angle(angle=math.radians(smooth_angle))
    obj["glass"] = glass
    PARTS.append(obj)
    return obj


def apply_modifiers(obj):
    if not obj.modifiers:
        return
    dg = bpy.context.evaluated_depsgraph_get()
    ev = obj.evaluated_get(dg)
    new = bpy.data.meshes.new_from_object(ev)
    old = obj.data
    obj.modifiers.clear()
    obj.data = new
    bpy.data.meshes.remove(old)


# =========================================================================== материалы

MATS = {}


class Nodes:
    """Короткая запись узлов материала."""

    def __init__(self, mat):
        self.nt = mat.node_tree
        self.n = self.nt.nodes
        self.l = self.nt.links

    def new(self, kind, **props):
        node = self.n.new(kind)
        for k, v in props.items():
            setattr(node, k, v)
        return node

    def link(self, out, inp):
        self.l.new(out, inp)

    def val(self, sock, v):
        """Подключить к сокету либо выход узла, либо число/цвет."""
        if isinstance(v, bpy.types.NodeSocket):
            self.link(v, sock)
        else:
            sock.default_value = v

    def coord(self, scale_xyz=(1, 1, 1)):
        tc = self.new("ShaderNodeTexCoord")
        mp = self.new("ShaderNodeMapping")
        mp.inputs["Scale"].default_value = scale_xyz
        self.link(tc.outputs["Object"], mp.inputs["Vector"])
        return mp.outputs["Vector"]

    def noise(self, scale, detail=6.0, rough=0.55, vec=None, dist=0.0):
        n = self.new("ShaderNodeTexNoise")
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = detail
        n.inputs["Roughness"].default_value = rough
        n.inputs["Distortion"].default_value = dist
        if vec is not None:
            self.link(vec, n.inputs["Vector"])
        return n.outputs["Fac"]

    def voronoi(self, scale, vec=None, feature="F1", out="Distance"):
        n = self.new("ShaderNodeTexVoronoi")
        n.feature = feature
        n.inputs["Scale"].default_value = scale
        if vec is not None:
            self.link(vec, n.inputs["Vector"])
        return n.outputs[out]

    def wave(self, scale, vec=None, kind="BANDS", direction="X", distortion=0.0, detail=2.0, profile="SIN"):
        n = self.new("ShaderNodeTexWave")
        n.wave_type = kind
        if kind == "BANDS":
            n.bands_direction = direction
        else:
            n.rings_direction = direction
        n.wave_profile = profile
        n.inputs["Scale"].default_value = scale
        n.inputs["Distortion"].default_value = distortion
        n.inputs["Detail"].default_value = detail
        if vec is not None:
            self.link(vec, n.inputs["Vector"])
        return n.outputs["Fac"]

    def ramp(self, fac, stops, interp="LINEAR"):
        """stops: [(pos, (r,g,b) или число)]."""
        r = self.new("ShaderNodeValToRGB")
        r.color_ramp.interpolation = interp
        els = r.color_ramp.elements
        while len(els) > 1:
            els.remove(els[-1])
        for i, (pos, col) in enumerate(stops):
            e = els[0] if i == 0 else els.new(pos)
            e.position = pos
            if isinstance(col, (int, float)):
                col = (col, col, col)
            e.color = (*col, 1.0)
        self.link(fac, r.inputs["Fac"])
        return r.outputs["Color"]

    def math(self, op, a, b=0.0, clamp=False):
        m = self.new("ShaderNodeMath", operation=op, use_clamp=clamp)
        self.val(m.inputs[0], a)
        self.val(m.inputs[1], b)
        return m.outputs[0]

    def mix(self, fac, a, b, blend="MIX"):
        m = self.new("ShaderNodeMix", data_type="RGBA", blend_type=blend)
        self.val(m.inputs["Factor"], fac)
        self.val(m.inputs[6], a if not isinstance(a, tuple) else (*a, 1.0))
        self.val(m.inputs[7], b if not isinstance(b, tuple) else (*b, 1.0))
        return m.outputs[2]

    def mixf(self, fac, a, b):
        m = self.new("ShaderNodeMix", data_type="FLOAT")
        self.val(m.inputs["Factor"], fac)
        self.val(m.inputs[2], a)
        self.val(m.inputs[3], b)
        return m.outputs[0]

    def ao(self, distance=0.02):
        a = self.new("ShaderNodeAmbientOcclusion", samples=8)
        a.inputs["Distance"].default_value = distance
        return a.outputs["AO"]

    def pointiness(self):
        g = self.new("ShaderNodeNewGeometry")
        return g.outputs["Pointiness"]

    def image(self, name, vec, non_color=False, box_blend=0.25):
        path = os.path.join(CHAR_TEX, name + ".png")
        img = bpy.data.images.get(name) or bpy.data.images.load(path)
        img.name = name
        if non_color:
            img.colorspace_settings.name = "Non-Color"
        t = self.new("ShaderNodeTexImage", projection="BOX", projection_blend=box_blend)
        t.image = img
        self.link(vec, t.inputs["Vector"])
        return t.outputs["Color"]

    def bump(self, height, strength=0.3, distance=0.002, normal=None):
        b = self.new("ShaderNodeBump")
        b.inputs["Strength"].default_value = strength
        b.inputs["Distance"].default_value = distance
        self.link(height, b.inputs["Height"])
        if normal is not None:
            self.link(normal, b.inputs["Normal"])
        return b.outputs["Normal"]


def new_material(name):
    m = bpy.data.materials.new(name)
    try:
        m.use_nodes = True
    except Exception:
        pass
    nb = Nodes(m)
    bsdf = next(n for n in nb.n if n.type == "BSDF_PRINCIPLED")
    MATS[name] = m
    return m, nb, bsdf


def lin(c):
    """sRGB 0..255 -> линейный цвет Blender."""

    def f(x):
        x = x / 255.0
        return x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4

    return tuple(f(v) for v in c)


def metal_mat(name, color, color2=None, rough=(0.28, 0.5), scratches=0.5, rust=0.0, dirt=0.6, scale=1.0, polish_dir=None, bump=0.25, tarnish=None):
    """Металл: оттенки, царапины, грязь в углублениях, пятна ржавчины.
    polish_dir — ось ("X"/"Y"/"Z"), вдоль которой идут следы шлифовки (клинки)."""
    m, nb, bsdf = new_material(name)
    c1 = lin(color)
    c2 = lin(color2) if color2 else tuple(v * 0.7 for v in c1)
    vec = nb.coord((scale, scale, scale))
    n_big = nb.noise(6.0, 4, 0.5, vec)
    n_fine = nb.noise(60.0, 8, 0.6, vec)
    base = nb.mix(n_big, c2, c1)
    # Следы шлифовки или хаотичные царапины.
    if polish_dir:
        sc = {"X": (1, 40, 40), "Y": (40, 1, 40), "Z": (40, 40, 1)}[polish_dir]
        sv = nb.coord(tuple(s * scale for s in sc))
        streak = nb.noise(8.0, 2, 0.5, sv)
        scr = nb.ramp(streak, [(0.45, 0.0), (0.75, 1.0)])
    else:
        sv = nb.coord((scale * 3, scale * 3, scale * 3))
        w = nb.wave(30.0, sv, "BANDS", "DIAGONAL", distortion=12.0, detail=4)
        scr = nb.ramp(w, [(0.0, 1.0), (0.06, 0.0)])
    scr = nb.math("MULTIPLY", nb.math("MULTIPLY", scr, scratches), nb.ramp(n_fine, [(0.4, 0.0), (0.6, 1.0)]))
    base = nb.mix(nb.math("MULTIPLY", scr, 0.35), base, tuple(min(1, v * 1.35) for v in c1))
    roughv = nb.math("ADD", rough[0], nb.math("MULTIPLY", nb.ramp(nb.noise(14.0, 5, 0.6, vec), [(0.3, 0.0), (0.7, 1.0)]), rough[1] - rough[0]))
    roughv = nb.math("SUBTRACT", roughv, nb.math("MULTIPLY", scr, 0.12), clamp=True)
    metallic = 1.0
    if tarnish:
        # Патина/потускнение в углублениях (бронза, серебро).
        tn = nb.math("MULTIPLY", nb.math("SUBTRACT", 1.0, nb.ao(0.015)), 1.6, clamp=True)
        base = nb.mix(nb.math("MULTIPLY", tn, tarnish[1]), base, lin(tarnish[0]))
    if rust > 0:
        rn = nb.noise(9.0, 8, 0.65, vec, dist=0.4)
        rmask = nb.ramp(nb.math("ADD", rn, nb.math("MULTIPLY", nb.math("SUBTRACT", 1.0, nb.ao(0.02)), 0.5)), [(0.72 - rust * 0.25, 0.0), (0.8 - rust * 0.2, 1.0)])
        rust_col = nb.mix(n_fine, lin((84, 40, 20)), lin((140, 74, 36)))
        base = nb.mix(rmask, base, rust_col)
        roughv = nb.mixf(rmask, roughv, 0.85)
        metallic = nb.mixf(rmask, 1.0, 0.15)
    if dirt > 0:
        d = nb.math("MULTIPLY", nb.math("SUBTRACT", 1.0, nb.ao(0.025)), dirt * 1.5, clamp=True)
        base = nb.mix(d, base, tuple(v * 0.25 for v in c2))
        roughv = nb.mixf(d, roughv, 0.8)
    nb.val(bsdf.inputs["Base Color"], base)
    nb.val(bsdf.inputs["Roughness"], roughv)
    nb.val(bsdf.inputs["Metallic"], metallic)
    nb.val(bsdf.inputs["Normal"], nb.bump(nb.math("ADD", nb.math("MULTIPLY", n_fine, 0.3), nb.math("MULTIPLY", scr, -0.4)), bump, 0.001))
    return m


def organic_mat(name, color, color2, rough=(0.6, 0.85), scale=1.0, bump=0.4, bump_scale=80.0, grain=None, grain_scale=3.0, dirt=0.5, wear=0.0, wear_color=None, sheen=0.0, cells=0.0):
    """Дерево, кожа, ткань, кость: два оттенка, «зерно» вдоль оси grain, грязь в складках,
    потёртости на выпуклостях (wear)."""
    m, nb, bsdf = new_material(name)
    c1, c2 = lin(color), lin(color2)
    vec = nb.coord((scale, scale, scale))
    n_big = nb.noise(4.0, 5, 0.55, vec)
    if grain:
        sc = {"X": (1, 25, 25), "Y": (25, 1, 25), "Z": (25, 25, 1)}[grain]
        gv = nb.coord(tuple(s * scale * grain_scale for s in sc))
        g = nb.noise(3.0, 10, 0.7, gv, dist=1.5)
        g = nb.ramp(g, [(0.35, 0.0), (0.65, 1.0)])
        tone = nb.mix(nb.math("ADD", nb.math("MULTIPLY", g, 0.7), nb.math("MULTIPLY", n_big, 0.3)), c2, c1)
        height = g
    else:
        tone = nb.mix(n_big, c2, c1)
        height = nb.noise(bump_scale, 6, 0.6, vec)
    if cells:
        # Мелкая фактура кожи/ткани.
        v = nb.voronoi(cells, vec)
        height = nb.math("ADD", nb.math("MULTIPLY", height, 0.5), nb.math("MULTIPLY", v, 0.5))
    roughv = nb.math("ADD", rough[0], nb.math("MULTIPLY", nb.noise(10.0, 4, 0.5, vec), rough[1] - rough[0]))
    if wear > 0:
        wmask = nb.ramp(nb.math("ADD", nb.pointiness(), nb.math("MULTIPLY", nb.noise(30.0, 4, 0.6, vec), 0.15)), [(0.52, 0.0), (0.6, 1.0)])
        wmask = nb.math("MULTIPLY", wmask, wear)
        tone = nb.mix(wmask, tone, lin(wear_color) if wear_color else tuple(min(1, v * 1.6) for v in c1))
        roughv = nb.mixf(wmask, roughv, rough[0] * 0.7)
    if dirt > 0:
        d = nb.math("MULTIPLY", nb.math("SUBTRACT", 1.0, nb.ao(0.03)), dirt * 1.4, clamp=True)
        tone = nb.mix(d, tone, tuple(v * 0.3 for v in c2))
    nb.val(bsdf.inputs["Base Color"], tone)
    nb.val(bsdf.inputs["Roughness"], roughv)
    nb.val(bsdf.inputs["Metallic"], 0.0)
    if sheen:
        bsdf.inputs["Sheen Weight"].default_value = sheen
    nb.val(bsdf.inputs["Normal"], nb.bump(height, bump, 0.002))
    return m


def cloth_mat(name, color, color2, weave=900.0, rough=0.9, dirt=0.5, scale=1.0, bump=0.35):
    """Льняное/шерстяное полотно: полотняное переплетение двумя волнами."""
    m, nb, bsdf = new_material(name)
    c1, c2 = lin(color), lin(color2)
    vec = nb.coord((scale, scale, scale))
    wx = nb.wave(weave, vec, "BANDS", "X", distortion=0.6, detail=1)
    wz = nb.wave(weave, vec, "BANDS", "Z", distortion=0.6, detail=1)
    wy = nb.wave(weave, vec, "BANDS", "Y", distortion=0.6, detail=1)
    weavev = nb.math("MAXIMUM", nb.math("MAXIMUM", wx, wz), wy)
    n_big = nb.noise(5.0, 5, 0.6, vec)
    slub = nb.noise(40.0, 3, 0.5, nb.coord((scale * 0.2, scale * 6, scale * 0.2)))
    tone = nb.mix(nb.math("ADD", nb.math("MULTIPLY", n_big, 0.7), nb.math("MULTIPLY", slub, 0.3)), c2, c1)
    tone = nb.mix(nb.math("MULTIPLY", weavev, 0.25), tone, tuple(v * 0.75 for v in c2))
    if dirt > 0:
        d = nb.math("MULTIPLY", nb.math("SUBTRACT", 1.0, nb.ao(0.04)), dirt * 1.4, clamp=True)
        tone = nb.mix(d, tone, tuple(v * 0.35 for v in c2))
    nb.val(bsdf.inputs["Base Color"], tone)
    nb.val(bsdf.inputs["Roughness"], rough)
    bsdf.inputs["Sheen Weight"].default_value = 0.4
    nb.val(bsdf.inputs["Normal"], nb.bump(weavev, bump, 0.0008))
    return m


def painted_mat(name, paint_fn, wood_color=(120, 80, 45), chip=0.4, scale=1.0, rough=0.6):
    """Крашеное дерево (щиты). paint_fn(nb, vec) -> цвет краски (сокет).
    Краска сколота на кромках и местами (chip), под ней — дерево."""
    m, nb, bsdf = new_material(name)
    vec = nb.coord((scale, scale, scale))
    paint = paint_fn(nb, vec)
    n = nb.noise(5.0, 6, 0.6, vec)
    paint = nb.mix(nb.math("MULTIPLY", n, 0.35), paint, nb.mix(0.5, paint, (0.02, 0.015, 0.01)))
    wood = nb.mix(nb.noise(3.0, 10, 0.7, nb.coord((scale * 2, scale * 40, scale * 2)), dist=1.5), lin(tuple(v * 0.6 for v in wood_color)), lin(wood_color))
    chips = nb.math("ADD", nb.noise(14.0, 8, 0.7, vec), nb.math("MULTIPLY", nb.math("SUBTRACT", nb.pointiness(), 0.5), 2.0))
    chipmask = nb.ramp(chips, [(0.72 - chip * 0.2, 0.0), (0.74 - chip * 0.2, 1.0)])
    tone = nb.mix(chipmask, paint, wood)
    d = nb.math("MULTIPLY", nb.math("SUBTRACT", 1.0, nb.ao(0.03)), 0.8, clamp=True)
    tone = nb.mix(d, tone, (0.02, 0.015, 0.01))
    nb.val(bsdf.inputs["Base Color"], tone)
    nb.val(bsdf.inputs["Roughness"], nb.mixf(chipmask, rough, 0.85))
    nb.val(bsdf.inputs["Normal"], nb.bump(nb.math("ADD", nb.math("MULTIPLY", chipmask, -1.0), nb.math("MULTIPLY", n, 0.2)), 0.25, 0.001))
    return m


def image_mat(name, tex, scale_m, metallic=0.0, rough=0.5, mr=None, normal=None, tint=(255, 255, 255), dirt=0.4):
    """Материал на текстурах персонажей (кольчуга и т.п.) с кубической проекцией."""
    m, nb, bsdf = new_material(name)
    vec = nb.coord((1 / scale_m, 1 / scale_m, 1 / scale_m))
    col = nb.image(tex, vec)
    col = nb.mix(1.0, col, lin(tint), "MULTIPLY")
    if dirt:
        d = nb.math("MULTIPLY", nb.math("SUBTRACT", 1.0, nb.ao(0.03)), dirt * 1.4, clamp=True)
        col = nb.mix(d, col, (0.01, 0.01, 0.01))
    nb.val(bsdf.inputs["Base Color"], col)
    if mr:
        sep = nb.new("ShaderNodeSeparateColor")
        nb.link(nb.image(mr, vec, True), sep.inputs["Color"])
        nb.link(sep.outputs["Green"], bsdf.inputs["Roughness"])
        nb.link(sep.outputs["Blue"], bsdf.inputs["Metallic"])
    else:
        bsdf.inputs["Roughness"].default_value = rough
        bsdf.inputs["Metallic"].default_value = metallic
    if normal:
        nm = nb.new("ShaderNodeNormalMap")
        nb.link(nb.image(normal, vec, True), nm.inputs["Color"])
        nb.link(nm.outputs["Normal"], bsdf.inputs["Normal"])
    return m


def gem_mat(name, color, transmission=0.6, rough=0.04):
    m, nb, bsdf = new_material(name)
    bsdf.inputs["Base Color"].default_value = (*lin(color), 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Transmission Weight"].default_value = transmission
    bsdf.inputs["IOR"].default_value = 1.77
    bsdf.inputs["Coat Weight"].default_value = 0.5
    return m


def glass_mat(name, tint=(200, 225, 205), rough=0.06, alpha=0.32):
    """Античное стекло: зеленоватое, с пузырьками-неровностями.
    В .glb уходит как полупрозрачный материал (без запекания)."""
    m, nb, bsdf = new_material(name)
    bsdf.inputs["Base Color"].default_value = (*lin(tint), 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Transmission Weight"].default_value = 1.0
    bsdf.inputs["IOR"].default_value = 1.5
    vec = nb.coord((1, 1, 1))
    nb.val(bsdf.inputs["Normal"], nb.bump(nb.noise(25.0, 3, 0.5, vec), 0.08, 0.002))
    m["export_alpha"] = alpha
    return m


def liquid_mat(name, color, rough=0.05):
    m, nb, bsdf = new_material(name)
    bsdf.inputs["Base Color"].default_value = (*lin(color), 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Coat Weight"].default_value = 1.0
    bsdf.inputs["Subsurface Weight"].default_value = 0.3
    bsdf.inputs["Subsurface Radius"].default_value = (0.3, 0.05, 0.02)
    return m


def common_materials():
    """Общие материалы: железо, бронза, золото, дерево, кожа, лён, бечёвка…"""
    metal_mat("iron", (150, 150, 152), (92, 90, 88), rough=(0.32, 0.55), rust=0.35, dirt=0.7)
    metal_mat("iron_dark", (110, 108, 106), (60, 58, 56), rough=(0.4, 0.62), rust=0.45, dirt=0.8)
    metal_mat("blade_steel", (196, 198, 200), (140, 142, 146), rough=(0.18, 0.34), scratches=0.8, rust=0.12, dirt=0.4, polish_dir="Z")
    metal_mat("bronze", (196, 146, 82), (130, 90, 46), rough=(0.3, 0.5), dirt=0.8, tarnish=((60, 92, 70), 0.7))
    metal_mat("gilt", (235, 186, 96), (175, 125, 52), rough=(0.18, 0.32), dirt=0.6, scratches=0.3)
    metal_mat("gold", (250, 200, 110), (200, 150, 70), rough=(0.14, 0.26), dirt=0.5, scratches=0.3)
    metal_mat("silver", (222, 222, 226), (170, 168, 170), rough=(0.18, 0.34), dirt=0.6, tarnish=((40, 36, 34), 0.8))
    organic_mat("wood", (118, 90, 62), (66, 48, 32), rough=(0.6, 0.85), grain="Z", grain_scale=4.0, dirt=0.7, wear=0.35, wear_color=(150, 120, 88))
    organic_mat("wood_dark", (92, 60, 36), (52, 32, 18), rough=(0.45, 0.7), grain="Z", dirt=0.6, wear=0.5)
    organic_mat("leather", (104, 70, 44), (58, 38, 22), rough=(0.5, 0.75), bump=0.35, cells=260.0, dirt=0.6, wear=0.6, wear_color=(150, 105, 70))
    organic_mat("leather_dark", (72, 46, 28), (38, 24, 14), rough=(0.45, 0.7), bump=0.35, cells=260.0, dirt=0.6, wear=0.6, wear_color=(110, 76, 50))
    organic_mat("rawhide", (196, 168, 120), (150, 120, 78), rough=(0.5, 0.7), bump=0.2, dirt=0.5)
    organic_mat("bone", (226, 212, 184), (176, 158, 124), rough=(0.4, 0.6), bump=0.15, bump_scale=40.0, dirt=0.7, grain="Z", grain_scale=1.0)
    cloth_mat("linen", (214, 200, 170), (176, 160, 128))
    cloth_mat("linen_white", (232, 226, 210), (196, 186, 164), dirt=0.3)
    cloth_mat("wool_red", (140, 36, 30), (96, 22, 20))
    organic_mat("cord", (150, 120, 80), (100, 76, 48), rough=(0.7, 0.9), grain="Z", grain_scale=8.0, dirt=0.3)
    gem_mat("garnet", (150, 12, 24))



# =========================================================================== реестр предметов

ITEMS = {}


def item(key, cells, name=None, pose="flat", azimuth=18.0, elevation=12.0, margin=0.9, roll=0.0, squash=1.0):
    """Регистрирует модель предмета. name — название из ITEM_BASES (None — служебная модель);
    squash — насколько сплющить модель на земле (одежда лежит плоско)."""

    def deco(fn):
        ITEMS[key] = dict(fn=fn, cells=cells, name=name, pose=pose, azimuth=azimuth, elevation=elevation, margin=margin, roll=roll, squash=squash)
        return fn

    return deco


def write_art_table():
    """src/icons/art.rs: название предмета -> ключ модели (только для готовых файлов)."""
    rows = []
    for key, spec in ITEMS.items():
        if not spec["name"]:
            continue
        if os.path.exists(os.path.join(OUT_ICONS, key + ".png")) and os.path.exists(os.path.join(OUT_MODELS, key + ".glb")):
            rows.append(f'    ("{spec["name"]}", "{key}"),')
    path = os.path.join(ROOT, "src", "icons", "art.rs")
    with open(path, "w", encoding="utf-8") as f:
        f.write("//! Создаётся tools/models/build_items.py — руками не править.\n\n")
        f.write("/// Название предмета (как в `ITEM_BASES`) -> ключ 3D-модели и иконки.\n")
        f.write("pub const ITEM_ART: &[(&str, &str)] = &[\n" + "\n".join(rows) + "\n];\n")


# =========================================================================== студия и иконка


def setup_studio():
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = ICON_SAMPLES
    scene.cycles.use_denoising = True
    scene.cycles.max_bounces = 8
    scene.cycles.transmission_bounces = 8
    scene.render.film_transparent = True
    scene.cycles.film_transparent_glass = True
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.view_settings.view_transform = "AgX"
    try:
        scene.view_settings.look = "AgX - Medium High Contrast"
    except TypeError:
        pass
    # Окружение «фотостудии»: светлый верх, тёмный низ — металл получает отражения.
    world = bpy.data.worlds.new("studio")
    scene.world = world
    try:
        world.use_nodes = True
    except Exception:
        pass
    nb = Nodes(world)
    bg = next(n for n in nb.n if n.type == "BACKGROUND")
    tc = nb.new("ShaderNodeTexCoord")
    sep = nb.new("ShaderNodeSeparateXYZ")
    nb.link(tc.outputs["Generated"], sep.inputs["Vector"])
    col = nb.ramp(nb.math("ADD", nb.math("MULTIPLY", sep.outputs["Z"], 0.5), 0.5), [(0.0, (0.02, 0.018, 0.016)), (0.25, (0.07, 0.065, 0.06)), (0.36, (0.85, 0.82, 0.78)), (0.5, (1.0, 0.97, 0.92)), (0.62, (0.4, 0.39, 0.37)), (0.78, (0.12, 0.115, 0.11)), (1.0, (0.7, 0.69, 0.66))])
    # Софтбоксы по азимуту: яркий слева-спереди, тусклый справа — даёт градиент на металле.
    side = nb.ramp(nb.math("ADD", nb.math("MULTIPLY", sep.outputs["X"], 0.5), 0.5), [(0.0, 0.5), (0.25, 1.0), (0.5, 0.7), (0.7, 0.25), (1.0, 0.45)])
    front = nb.ramp(nb.math("ADD", nb.math("MULTIPLY", sep.outputs["Y"], 0.5), 0.5), [(0.0, 1.0), (0.5, 0.75), (1.0, 0.45)])
    col = nb.mix(1.0, col, nb.mix(1.0, side, front, "MULTIPLY"), "MULTIPLY")
    nb.link(col, bg.inputs["Color"])
    bg.inputs["Strength"].default_value = 0.8
    # Свет: тёплый ключевой софтбокс слева-сверху, холодный контровой сзади справа, заполняющий снизу.
    lights = [
        ("key", (-2.2, -3.0, 3.0), 260.0, 2.2, (1.0, 0.93, 0.84)),
        ("rim", (2.6, 2.4, 2.0), 180.0, 1.4, (0.78, 0.86, 1.0)),
        ("fill", (2.4, -2.6, -0.6), 45.0, 2.5, (1.0, 0.97, 0.94)),
    ]
    for name, loc, power, size, color in lights:
        ld = bpy.data.lights.new(name, "AREA")
        ld.energy = power
        ld.size = size
        ld.color = color
        lo = link(bpy.data.objects.new(name, ld))
        lo.location = loc
        lo.rotation_euler = (Vector((0, 0, 0)) - Vector(loc)).to_track_quat("-Z", "Y").to_euler()


def icon_camera(objs, cells, azimuth=18.0, elevation=12.0, margin=0.9, roll=0.0):
    """Ортокамера, вписывающая объекты в прямоугольник клеток cells=(w,h)."""
    scene = bpy.context.scene
    w, h = cells
    scene.render.resolution_x = w * ICON_PX_PER_CELL
    scene.render.resolution_y = h * ICON_PX_PER_CELL
    scene.render.resolution_percentage = 100
    cam_d = bpy.data.cameras.new("icon")
    cam_d.type = "ORTHO"
    cam = link(bpy.data.objects.new("icon", cam_d))
    scene.camera = cam
    az, el = math.radians(azimuth), math.radians(elevation)
    # Направление «от предмета к камере»: спереди (-Y), повернуть вправо на az, вверх на el.
    d = Vector((math.sin(az) * math.cos(el), -math.cos(az) * math.cos(el), math.sin(el)))
    q = (-d).to_track_quat("-Z", "Y")
    if roll:
        q = q @ Matrix.Rotation(math.radians(roll), 4, "Z").to_quaternion()
    cam.rotation_euler = q.to_euler()
    rot_inv = q.to_matrix().inverted()
    pts = []
    dg = bpy.context.evaluated_depsgraph_get()
    for o in objs:
        for v in o.evaluated_get(dg).data.vertices:
            pts.append(rot_inv @ (o.matrix_world @ v.co))
    xs = [p.x for p in pts]
    ys = [p.y for p in pts]
    zs = [p.z for p in pts]
    cx, cy = (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
    rx, ry = max(xs) - min(xs), max(ys) - min(ys)
    aspect = w / h
    if aspect >= 1:
        cam_d.ortho_scale = max(rx, ry * aspect) / margin
    else:
        cam_d.ortho_scale = max(ry, rx / aspect) / margin
    cam_d.clip_start = 0.01
    cam_d.clip_end = 100
    centre = q.to_matrix() @ Vector((cx, cy, max(zs) + 5.0))
    cam.location = centre
    return cam


def render_icon(path):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    bpy.context.scene.render.filepath = path
    bpy.ops.render.render(write_still=True)


# =========================================================================== запекание и экспорт


def _socket_source(nt, sock):
    """Выход, подключённый к сокету, или None."""
    for lk in nt.links:
        if lk.to_socket == sock:
            return lk.from_socket
    return None


def bake_and_export(key, pose, bake_size=BAKE_SIZE, squash=1.0):
    """Сливает непрозрачные детали в один меш, запекает цвет/шероховатость/металл/нормали
    в текстуры и сохраняет assets/models/items/<key>.glb в позе на земле."""
    scene = bpy.context.scene
    opaque = [o for o in PARTS if not o["glass"]]
    glass = [o for o in PARTS if o["glass"]]
    body = None
    if opaque:  # бывают предметы целиком из стекла
        bpy.ops.object.select_all(action="DESELECT")
        for o in opaque:
            o.select_set(True)
        bpy.context.view_layer.objects.active = opaque[0]
        if len(opaque) > 1:
            bpy.ops.object.join()
        body = bpy.context.view_layer.objects.active
        body.name = key
        me = body.data
        # UV для запекания.
        for uv in list(me.uv_layers):
            me.uv_layers.remove(uv)
        me.uv_layers.new(name="UVMap")
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="SELECT")
        bpy.ops.uv.smart_project(angle_limit=math.radians(55), island_margin=0.006, area_weight=0.0, correct_aspect=True, scale_to_bounds=False)
        bpy.ops.uv.pack_islands(margin=0.004, rotate=True)
        bpy.ops.object.mode_set(mode="OBJECT")

        imgs = {}
        for kind in ("color", "rough", "metal", "normal"):
            img = bpy.data.images.new(f"{key}_{kind}", bake_size, bake_size, alpha=False, float_buffer=(kind == "normal"))
            if kind != "color":
                img.colorspace_settings.name = "Non-Color"
            imgs[kind] = img

        mats = list(me.materials)
        tex_nodes = []
        for m in mats:
            t = m.node_tree.nodes.new("ShaderNodeTexImage")
            m.node_tree.nodes.active = t
            tex_nodes.append((m, t))

        scene.cycles.samples = 16
        scene.render.bake.margin = 6
        bpy.ops.object.select_all(action="DESELECT")
        body.select_set(True)
        bpy.context.view_layer.objects.active = body

        def bake_input(kind, input_name):
            saved = []
            for m, t in tex_nodes:
                t.image = imgs[kind]
                nt = m.node_tree
                bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
                out = next(n for n in nt.nodes if n.type == "OUTPUT_MATERIAL")
                src = _socket_source(nt, bsdf.inputs[input_name])
                em = nt.nodes.new("ShaderNodeEmission")
                if src is not None:
                    if src.type in ("VALUE", "INT"):
                        nt.links.new(src, em.inputs["Color"])
                    else:
                        nt.links.new(src, em.inputs["Color"])
                else:
                    v = bsdf.inputs[input_name].default_value
                    em.inputs["Color"].default_value = (*v[:3], 1.0) if hasattr(v, "__len__") else (v, v, v, 1.0)
                old = _socket_source(nt, out.inputs["Surface"])
                nt.links.new(em.outputs["Emission"], out.inputs["Surface"])
                saved.append((nt, out, old, em))
            bpy.ops.object.bake(type="EMIT")
            for nt, out, old, em in saved:
                nt.links.new(old, out.inputs["Surface"])
                nt.nodes.remove(em)

        bake_input("color", "Base Color")
        bake_input("rough", "Roughness")
        bake_input("metal", "Metallic")
        for m, t in tex_nodes:
            t.image = imgs["normal"]
        scene.cycles.samples = 4
        bpy.ops.object.bake(type="NORMAL", normal_space="TANGENT")
        for m, t in tex_nodes:
            m.node_tree.nodes.remove(t)

        # Упаковка: ORM (G — шероховатость, B — металл) как ждёт glTF.
        import numpy as np

        def px(img):
            a = np.empty(bake_size * bake_size * 4, dtype=np.float32)
            img.pixels.foreach_get(a)
            return a.reshape(-1, 4)

        rough = px(imgs["rough"])[:, 0]
        metal = px(imgs["metal"])[:, 0]
        orm = bpy.data.images.new(f"{key}_orm", bake_size, bake_size, alpha=False)
        orm.colorspace_settings.name = "Non-Color"
        arr = np.stack([np.ones_like(rough), rough, metal, np.ones_like(rough)], -1).astype(np.float32)
        orm.pixels.foreach_set(arr.ravel())
        nrm8 = bpy.data.images.new(f"{key}_nrm", bake_size, bake_size, alpha=False)
        nrm8.colorspace_settings.name = "Non-Color"
        nrm8.pixels.foreach_set(px(imgs["normal"]).ravel())

        baked, nb, bsdf = new_material(f"{key}_baked")
        t_col = nb.new("ShaderNodeTexImage")
        t_col.image = imgs["color"]
        nb.link(t_col.outputs["Color"], bsdf.inputs["Base Color"])
        t_orm = nb.new("ShaderNodeTexImage")
        t_orm.image = orm
        sep = nb.new("ShaderNodeSeparateColor")
        nb.link(t_orm.outputs["Color"], sep.inputs["Color"])
        nb.link(sep.outputs["Green"], bsdf.inputs["Roughness"])
        nb.link(sep.outputs["Blue"], bsdf.inputs["Metallic"])
        t_n = nb.new("ShaderNodeTexImage")
        t_n.image = nrm8
        nm = nb.new("ShaderNodeNormalMap")
        nb.link(t_n.outputs["Color"], nm.inputs["Color"])
        nb.link(nm.outputs["Normal"], bsdf.inputs["Normal"])
        me.materials.clear()
        me.materials.append(baked)

    # Стекло: простой полупрозрачный материал.
    for g in glass:
        src = g.data.materials[0]
        a = src.get("export_alpha", 0.35)
        gm, gnb, gb = new_material(src.name + "_export")
        col = src.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value
        gb.inputs["Base Color"].default_value = col
        gb.inputs["Roughness"].default_value = 0.05
        gb.inputs["Alpha"].default_value = a
        try:
            gm.surface_render_method = "BLENDED"
        except AttributeError:
            pass
        g.data.materials.clear()
        g.data.materials.append(gm)

    # Поза на земле.
    allobj = ([body] if body else []) + glass
    if pose == "flat":
        for o in allobj:
            o.data.transform(Matrix.Rotation(math.radians(-90), 4, "X"))
    if squash != 1.0:
        lo = min(v.co.z for o in allobj for v in o.data.vertices)
        for o in allobj:
            o.data.transform(Matrix.Translation((0, 0, lo)) @ Matrix.Diagonal((1, 1, squash, 1)) @ Matrix.Translation((0, 0, -lo)))
    pts = [v.co for o in allobj for v in o.data.vertices]
    minz = min(p.z for p in pts)
    cx = (min(p.x for p in pts) + max(p.x for p in pts)) / 2
    cy = (min(p.y for p in pts) + max(p.y for p in pts)) / 2
    for o in allobj:
        o.data.transform(Matrix.Translation((-cx, -cy, -minz)))
        o.data.update()

    # Удаляем всё лишнее и экспортируем.
    for o in list(scene.objects):
        if o not in allobj:
            bpy.data.objects.remove(o)
    os.makedirs(OUT_MODELS, exist_ok=True)
    path = os.path.join(OUT_MODELS, key + ".glb")
    bpy.ops.export_scene.gltf(
        filepath=path,
        export_format="GLB",
        use_selection=False,
        export_yup=True,
        export_apply=False,
        export_animations=False,
        export_cameras=False,
        export_lights=False,
        export_image_format="JPEG",
        export_jpeg_quality=88,
        export_materials="EXPORT",
    )
    return path
