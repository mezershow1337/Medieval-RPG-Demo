"""Генератор деревьев и кустов Таврики для игры (запускается в Blender на ПК через BlenderMCP).

Дерево = ствол и ветви (трубки с корой Bark014, ambientCG CC0) + листва-карточки с атласом
веточек (атлас отрендерен из фотоскана Poly Haven island_tree_02 (CC0) и из процедурных
пучков хвои/чешуйчатых веточек, см. build_atlases.py) + дальний «импостор» (две скрещённые
плоскости с видом дерева сбоку).

В каждом .glb три меша на LOD: `<id>_lod0_bark`, `<id>_lod0_leaf`, `<id>_lod1_bark`,
`<id>_lod1_leaf`, `<id>_imp`. Второй UV-канал (TEXCOORD_1): x — сила качания на ветру
(0 у корня, 1 на концах ветвей), y — фаза. Материалы в игре свои (src/vegetation.rs),
из glb берутся только меши.

Запуск: exec этого файла в Blender, затем `build_all(OUT_DIR)`.
"""
import bmesh
import bpy
import math
import os
import random
from mathutils import Matrix, Quaternion, Vector

ATLAS_DIR = os.environ.get("MRPG_VEG_ATLAS", r"E:\Downloads\mrpg_veg\atlas")
BARK_DIR = os.environ.get("MRPG_VEG_BARK", r"E:\Downloads\mrpg_veg\bark")
# Размер картинки импостора: по правилу проекта текстуры 1024×1024.
IMP_SIZE = 1024


# Множитель цвета коры (линейный); тот же стоит в src/vegetation.rs (BARK_TINT).
BARK_TINT = (0.42, 0.36, 0.31)


class Species:
    def __init__(self, **kw):
        self.__dict__.update(kw)


# Высоты — в метрах; в игре модель масштабируется под нужную высоту.
SPECIES = {
    # Сосна крымская (Pinus nigra subsp. pallasiana): прямой ствол, мутовки ветвей,
    # к старости зонтичная крона; хвоя пучками на концах побегов.
    "pine": Species(height=11.0, trunk_r=0.26, levels=3, children=(10, 5, 3), crown_start=0.45,
                    angle=(78, 50, 40), length=(0.36, 0.45, 0.5), up=0.12, gravity=0.05, wobble=0.12,
                    atlas="pine", card=1.3, cards_per_tip=5, whorl=True, taper=0.85, multi_stem=1),
    # Дуб пушистый (Quercus pubescens): узловатый ствол, широкая раскидистая крона.
    "oak": Species(height=8.0, trunk_r=0.34, levels=3, children=(6, 4, 3), crown_start=0.25,
                   angle=(55, 45, 40), length=(0.6, 0.5, 0.45), up=0.25, gravity=0.06, wobble=0.5,
                   atlas="broadleaf", card=1.5, cards_per_tip=3, whorl=False, taper=0.75, multi_stem=1),
    # Можжевельник высокий (Juniperus excelsa): многоствольный, плотный, неправильный конус.
    "juniper": Species(height=4.0, trunk_r=0.12, levels=2, children=(9, 5), crown_start=0.08,
                       angle=(35, 38), length=(0.6, 0.45), up=0.5, gravity=0.02, wobble=0.25,
                       atlas="scale", card=1.0, cards_per_tip=3, whorl=False, taper=0.7, multi_stem=3),
    # Кипарис вечнозелёный: узкая колонна.
    "cypress": Species(height=9.0, trunk_r=0.18, levels=2, children=(26, 4), crown_start=0.06,
                       angle=(22, 25), length=(0.22, 0.35), up=0.85, gravity=0.0, wobble=0.05,
                       atlas="scale", card=1.0, cards_per_tip=3, whorl=False, taper=0.9, multi_stem=1, narrow=True),
    # Смоковница (инжир): низкая, многоствольная, широкая крона.
    "fig": Species(height=4.5, trunk_r=0.14, levels=3, children=(4, 4, 3), crown_start=0.2,
                   angle=(45, 45, 40), length=(0.6, 0.5, 0.45), up=0.35, gravity=0.04, wobble=0.3,
                   atlas="broadleaf", card=1.3, cards_per_tip=3, whorl=False, taper=0.7, multi_stem=3),
    # Стройная горная сосна в лесу (та же крымская, но молодая и в тесноте бора): прямой ствол,
    # частые мутовки, ветви ярусами — нижние длинные и поникшие, верхние короткие и вверх;
    # крона — узкий конус до самой земли. Строится отдельно (`make_conifer`): лапы-полосы из
    # атласа bough лежат вдоль ветвей и свисают под ними.
    "fir": Species(height=14.0, trunk_r=0.19, whorls=24, per_whorl=(6, 8), crown_start=0.1, spread=0.215,
                   atlas="bough", conifer=True),
    # Кустарник шибляка (шиповник, боярышник, держидерево): 1–2 м.
    "shrub": Species(height=1.7, trunk_r=0.04, levels=2, children=(7, 4), crown_start=0.0,
                     angle=(45, 40), length=(0.7, 0.45), up=0.4, gravity=0.03, wobble=0.4,
                     atlas="broadleaf", card=0.85, cards_per_tip=3, whorl=False, taper=0.6, multi_stem=5),
}


class Branch:
    def __init__(self, pts, radii, level, parent_t=0.0):
        self.pts = pts
        self.radii = radii
        self.level = level
        self.children = []


def _rot_towards(v, target, amount):
    v = v.normalized()
    t = target.normalized()
    if amount <= 0:
        return v
    return (v.lerp(t, amount)).normalized()


def grow(sp, rnd):
    """Скелет дерева: список ветвей (полилиния, радиусы, уровень)."""
    branches = []
    H = sp.height

    def make_branch(origin, direction, length, radius, level, sway0):
        segs = max(3, int(length / (0.35 if level == 0 else 0.25)))
        segs = min(segs, 14)
        pts = [origin]
        radii = [radius]
        d = direction.normalized()
        for i in range(1, segs + 1):
            t = i / segs
            jitter = Vector((rnd.uniform(-1, 1), rnd.uniform(-1, 1), rnd.uniform(-1, 1))) * sp.wobble * 0.6
            d = (d + jitter * (1.0 / segs) * 3.0).normalized()
            d = _rot_towards(d, Vector((0, 0, 1)), sp.up * 0.12 if level > 0 else 0.05)
            d = (d + Vector((0, 0, -sp.gravity * t))).normalized()
            pts.append(pts[-1] + d * (length / segs))
            radii.append(max(radius * (1.0 - t * sp.taper), 0.006))
        b = Branch(pts, radii, level)
        b.sway = [sway0 + (1.0 - sway0) * (i / segs) ** 1.5 * (0.35 if level == 0 else 1.0) for i in range(segs + 1)]
        b.length = length
        branches.append(b)
        return b

    def spawn_children(b, level):
        if level >= sp.levels:
            b.tip = True
            return
        b.tip = False
        n = sp.children[level]
        if getattr(sp, "narrow", False) and level == 0:
            n = sp.children[0]
        start = sp.crown_start if level == 0 else 0.2
        whorl_phase = rnd.uniform(0, math.tau)
        for i in range(n):
            if sp.whorl and level == 0:
                # Мутовки: по 3–4 ветви на одной высоте.
                tier = i // 3
                tiers = max(1, (n + 2) // 3)
                t = start + (1 - start) * (tier + rnd.uniform(0.0, 0.3)) / tiers
                az = whorl_phase + (i % 3) * math.tau / 3 + tier * 0.9
            else:
                t = start + (1 - start) * (i + rnd.uniform(0.2, 0.8)) / n
                az = whorl_phase + i * 2.39996 + rnd.uniform(-0.3, 0.3)
            t = min(t, 0.97)
            k = t * (len(b.pts) - 1)
            i0 = int(k)
            i1 = min(i0 + 1, len(b.pts) - 1)
            f = k - i0
            p = b.pts[i0].lerp(b.pts[i1], f)
            r_par = b.radii[i0] * (1 - f) + b.radii[i1] * f
            axis = (b.pts[i1] - b.pts[i0]).normalized()
            side = axis.orthogonal().normalized()
            side = Quaternion(axis, az) @ side
            ang = math.radians(sp.angle[level] + rnd.uniform(-10, 10))
            d = (axis * math.cos(ang) + side * math.sin(ang)).normalized()
            # Ветви выше по стволу короче (конус кроны) — у сосны сильнее.
            shape = 1.0 - (t - start) / max(1e-3, 1 - start) * (0.55 if level == 0 else 0.3)
            if getattr(sp, "narrow", False) and level == 0:
                shape = 0.5 + 0.5 * math.sin(min(1.0, (t - start) / (1 - start)) * math.pi) ** 0.7
            L = b.length * sp.length[level] * shape * rnd.uniform(0.8, 1.15)
            r = min(r_par * 0.7, max(0.008, r_par * (0.55 if level == 0 else 0.6)))
            sway0 = b.sway[i0]
            c = make_branch(p - axis * r_par * 0.2, d, max(L, 0.25), r, level + 1, sway0)
            b.children.append(c)
            spawn_children(c, level + 1)

    stems = sp.multi_stem
    for s in range(stems):
        if stems == 1:
            d = Vector((rnd.uniform(-0.04, 0.04), rnd.uniform(-0.04, 0.04), 1))
            o = Vector((0, 0, -0.15))
            h = H * 0.95
            r = sp.trunk_r
        else:
            az = s * math.tau / stems + rnd.uniform(-0.4, 0.4)
            lean = rnd.uniform(0.15, 0.4)
            d = Vector((math.cos(az) * lean, math.sin(az) * lean, 1))
            o = Vector((math.cos(az) * sp.trunk_r * 0.6, math.sin(az) * sp.trunk_r * 0.6, -0.1))
            h = H * rnd.uniform(0.75, 0.95)
            r = sp.trunk_r * rnd.uniform(0.7, 1.0)
        trunk = make_branch(o, d, h, r, 0, 0.0)
        spawn_children(trunk, 0)
    return branches


def tube(bm, uv0, uv1, b, sides, cut_level):
    """Трубка вдоль полилинии ветви. UV0 — кора (метры), UV1 — (качание, фаза)."""
    if b.level > cut_level:
        return
    rings = []
    vacc = 0.0
    phase = (hash(tuple(round(c, 2) for c in b.pts[0])) % 1000) / 1000.0
    for i, p in enumerate(b.pts):
        if i == 0:
            d = b.pts[1] - b.pts[0]
        elif i == len(b.pts) - 1:
            d = b.pts[i] - b.pts[i - 1]
        else:
            d = b.pts[i + 1] - b.pts[i - 1]
        d.normalize()
        side = d.orthogonal().normalized() if i == 0 else (rings[-1][1] - (rings[-1][1].dot(d)) * d).normalized()
        if i > 0:
            vacc += (b.pts[i] - b.pts[i - 1]).length
        ring = []
        for k in range(sides):
            a = k / sides * math.tau
            n = (Quaternion(d, a) @ side).normalized()
            ring.append(bm.verts.new(p + n * b.radii[i]))
        rings.append((ring, side, vacc, b.sway[i]))
    circ = max(0.15, 2 * math.pi * b.radii[0])
    for i in range(len(rings) - 1):
        ra, _, va, sa = rings[i]
        rb, _, vb, sb = rings[i + 1]
        for k in range(sides):
            k2 = (k + 1) % sides
            f = bm.faces.new((ra[k], ra[k2], rb[k2], rb[k]))
            us = [k / sides, (k + 1) / sides, (k + 1) / sides, k / sides]
            vs = [va, va, vb, vb]
            ss = [sa, sa, sb, sb]
            for l, u, v, s in zip(f.loops, us, vs, ss):
                l[uv0].uv = (u * circ / 0.8, v / 0.8)
                l[uv1].uv = (s, phase)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])


def leaf_cards(bm, uv0, uv1, branches, sp, rnd, density, size_mul, crown_c):
    """Карточки листвы на концевых ветвях. Нормали — «сферические» от центра кроны (мягкий свет)."""
    nrm = {}
    tips = [b for b in branches if getattr(b, "tip", False)]
    # Верхушка ствола тоже в хвое/листве — иначе из кроны торчит голый шест.
    tips += [b for b in branches if b.level == 0 and not getattr(b, "tip", False)]
    for b in tips:
        trunk = b.level == 0
        n_cards = max(1, int(round(sp.cards_per_tip * density * rnd.uniform(0.8, 1.2) * (2.0 if trunk else 1 + b.length / 2.0))))
        for c in range(n_cards):
            if trunk:
                t = 1.0 - rnd.random() * 0.12
            else:
                t = 1.0 - (c / max(1, n_cards)) * 0.75 * rnd.uniform(0.7, 1.0)
            k = t * (len(b.pts) - 1)
            i0 = int(k)
            i1 = min(i0 + 1, len(b.pts) - 1)
            p = b.pts[i0].lerp(b.pts[i1], k - i0)
            axis = (b.pts[i1] - b.pts[i0]).normalized()
            s = sp.card * size_mul * rnd.uniform(0.8, 1.15)
            # Карточка вытянута вдоль ветви, повернута вокруг неё случайно.
            out = (p - crown_c)
            out.z *= 0.4
            out = out.normalized() if out.length > 1e-3 else Vector((1, 0, 0))
            up_dir = (axis * 0.6 + out * 0.3 + Vector((0, 0, 0.35))).normalized()
            side = up_dir.cross(Vector((0, 0, 1)))
            if side.length < 1e-3:
                side = Vector((1, 0, 0))
            side = Quaternion(up_dir, rnd.uniform(-1.2, 1.2)) @ side.normalized()
            base = p - up_dir * s * 0.15
            corners = [base - side * s / 2, base + side * s / 2, base + side * s / 2 + up_dir * s, base - side * s / 2 + up_dir * s]
            verts = [bm.verts.new(v) for v in corners]
            f = bm.faces.new(verts)
            cell = rnd.randrange(4)
            cx, cy = (cell % 2) * 0.5, (1 - cell // 2) * 0.5
            flip = rnd.random() < 0.5
            uvs = [(0, 0), (1, 0), (1, 1), (0, 1)]
            if flip:
                uvs = [(1, 0), (0, 0), (0, 1), (1, 1)]
            sway = b.sway[-1]
            phase = rnd.random()
            for l, (u, v) in zip(f.loops, uvs):
                l[uv0].uv = (cx + u * 0.5, cy + v * 0.5)
                l[uv1].uv = (sway, phase)
            for v in verts:
                sn = (v.co - crown_c)
                sn.z = sn.z * 0.7 + 0.25 * sn.length
                nrm[v.index if v.index >= 0 else id(v)] = sn.normalized()
    return nrm


def build_mesh(name, bm, spherical=None, crown_c=None):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    if spherical is not None:
        # Сферические нормали для листвы: от центра кроны.
        normals = []
        for lp in me.loops:
            v = me.vertices[lp.vertex_index].co
            sn = (v - crown_c)
            sn.z = sn.z * 0.7 + 0.25 * sn.length
            normals.append(sn.normalized())
        me.normals_split_custom_set(normals)
    return me


def make_tree(tid, sp, seed):
    rnd = random.Random(seed)
    branches = grow(sp, rnd)
    allpts = [p for b in branches if b.level > 0 for p in b.pts] or [p for b in branches for p in b.pts]
    crown_c = sum(allpts, Vector()) / len(allpts)
    meshes = {}
    for lod, (sides0, cut, dens, size) in enumerate([(8, 99, 1.0, 1.0), (5, sp.levels - 1, 0.45, 1.45)]):
        bm = bmesh.new()
        uv0 = bm.loops.layers.uv.new("UVMap")
        uv1 = bm.loops.layers.uv.new("Wind")
        for b in branches:
            sides = max(3, sides0 - b.level * 2)
            if b.radii[0] < 0.02:
                sides = 3
            tube(bm, uv0, uv1, b, sides, cut)
        meshes[f"{tid}_lod{lod}_bark"] = build_mesh(f"{tid}_lod{lod}_bark", bm)
        bm = bmesh.new()
        uv0 = bm.loops.layers.uv.new("UVMap")
        uv1 = bm.loops.layers.uv.new("Wind")
        rnd_cards = random.Random(seed * 7 + lod)
        leaf_cards(bm, uv0, uv1, branches, sp, rnd_cards, dens, size, crown_c)
        meshes[f"{tid}_lod{lod}_leaf"] = build_mesh(f"{tid}_lod{lod}_leaf", bm, spherical=True, crown_c=crown_c)
    return meshes, crown_c


def make_conifer(tid, sp, seed):
    """Коническое хвойное дерево: ствол, ярусы ветвей и лапы-полосы на каждой ветви.

    Лист: три полосы на ветвь — две половинки «крыши» вдоль ветви (лапа сверху) и занавес,
    свисающий под ней. Цвет вершин — затенение: у ствола и в нижних ярусах темно, концы лап
    светлые (крона читается объёмной и при плоском свете). Нормали — от оси ствола и вверх:
    солнечная сторона конуса светлая, теневая тёмная.
    """
    rnd = random.Random(seed)
    H = sp.height
    # Ствол: почти прямой.
    n_tr = 12
    tr_pts, tr_r = [], []
    wob = Vector((0, 0, 0))
    for i in range(n_tr + 1):
        t = i / n_tr
        wob += Vector((rnd.uniform(-1, 1), rnd.uniform(-1, 1), 0)) * 0.03
        tr_pts.append(Vector((wob.x * t, wob.y * t, -0.2 + (H + 0.2) * t)))
        tr_r.append(max(sp.trunk_r * (1.0 - t) ** 0.85, 0.012))
    trunk = Branch(tr_pts, tr_r, 0)
    trunk.sway = [0.3 * (i / n_tr) ** 1.6 for i in range(n_tr + 1)]

    def trunk_at(z):
        t = min(max((z + 0.2) / (H + 0.2), 0.0), 1.0) * n_tr
        i0 = min(int(t), n_tr - 1)
        return tr_pts[i0].lerp(tr_pts[i0 + 1], t - i0), tr_r[i0] + (tr_r[i0 + 1] - tr_r[i0]) * (t - i0)

    # Ветви ярусами.
    boughs = []
    phase0 = rnd.uniform(0, math.tau)
    for w in range(sp.whorls):
        tw = w / (sp.whorls - 1)
        zt = sp.crown_start + (0.955 - sp.crown_start) * tw ** 0.92
        n = rnd.randint(*sp.per_whorl) if tw < 0.8 else rnd.randint(3, 5)
        for k in range(n):
            az = phase0 + w * 2.4 + k * math.tau / n + rnd.uniform(-0.25, 0.25)
            z = H * (zt + rnd.uniform(-0.012, 0.012))
            o, r_tr = trunk_at(z)
            # Конус: внизу длинные, вверху короткие; самый нижний ярус чуть короче.
            L = H * sp.spread * (1.0 - tw) ** 0.8 * rnd.uniform(0.82, 1.12) + 0.25
            if w == 0:
                L *= 0.8
            elev = math.radians(-8 + 46 * tw + rnd.uniform(-6, 6))
            sag = 0.42 * (1.0 - tw) + 0.06
            out = Vector((math.cos(az), math.sin(az), 0))
            segs = 4
            pts, radii = [], []
            for i in range(segs + 1):
                q = i / segs
                zz = L * (math.tan(elev) * q - sag * q * q + 0.16 * q ** 3)
                pts.append(o + out * (r_tr * 0.5 + L * q) + Vector((0, 0, zz)))
                radii.append(max(0.035 * (L / 3.0) ** 0.5 * (1.0 - 0.8 * q), 0.006))
            b = Branch(pts, radii, 1)
            sw0 = 0.3 * zt ** 1.6
            b.sway = [sw0 + (0.25 + 0.75 * zt) * (i / segs) ** 1.4 for i in range(segs + 1)]
            b.out, b.L, b.tw = out, L, tw
            b.row = rnd.randrange(4)
            b.ph = rnd.random()
            b.primary = True
            boughs.append(b)
            # Боковые лапы: ярус получается сплошной «тарелкой», а не спицами.
            if L > 0.9:
                for j, q in enumerate((0.28, 0.5, 0.72)):
                    sgn = 1 if (j + k + w) % 2 == 0 else -1
                    a2 = az + sgn * math.radians(rnd.uniform(38, 58))
                    o2 = pts[0].lerp(pts[-1], q)
                    kq = q * segs
                    i0 = min(int(kq), segs - 1)
                    o2 = pts[i0].lerp(pts[i0 + 1], kq - i0)
                    L2 = (L * 0.5 * (1.0 - q) + 0.3) * rnd.uniform(0.85, 1.15)
                    out2 = Vector((math.cos(a2), math.sin(a2), 0))
                    p2, r2 = [], []
                    for i in range(3):
                        qq = i / 2
                        p2.append(o2 + out2 * L2 * qq + Vector((0, 0, L2 * (0.05 * qq - 0.28 * qq * qq))))
                        r2.append(0.006)
                    c = Branch(p2, r2, 2)
                    sq = b.sway[i0]
                    c.sway = [sq + (1.0 - sq) * 0.5 * (i / 2) for i in range(3)]
                    c.out, c.L, c.tw = out2, L2, tw
                    c.row = rnd.randrange(4)
                    c.ph = b.ph
                    c.primary = False
                    c.inner = 0.38 + 0.5 * q
                    boughs.append(c)

    crown_c = Vector((0, 0, H * 0.5))
    axis_at = lambda p: trunk_at(p.z)[0]

    def leaf_mesh(name, items, width_mul, curtains=True):
        bm = bmesh.new()
        uv0 = bm.loops.layers.uv.new("UVMap")
        uv1 = bm.loops.layers.uv.new("Wind")
        col = bm.loops.layers.float_color.new("Color")
        normals = {}

        def quad(ps, uvs, sways, shades, ph, nrms):
            vs = [bm.verts.new(p) for p in ps]
            f = bm.faces.new(vs)
            for l, uv, sw, sh, nn in zip(f.loops, uvs, sways, shades, nrms):
                l[uv0].uv = uv
                l[uv1].uv = (sw, ph)
                l[col] = (sh, sh, sh, 1.0)
            normals[f] = nrms

        for b in items:
            side = Vector((-b.out.y, b.out.x, 0))
            wd = min(max(b.L * 0.46, 0.42), 1.3) * width_mul
            # Лапа занимает середину своей полосы атласа — берём только её.
            v_mid = b.row * 0.25 + 0.125
            v_top = v_mid - 0.092
            v_bot = v_mid + 0.092
            inner = 0.38 if b.primary else b.inner
            segs = len(b.pts) - 1
            low = 0.62 + 0.38 * b.tw  # нижние ярусы темнее: над ними вся крона
            for i in range(segs):
                qa, qb = i / segs, (i + 1) / segs
                pa, pb = b.pts[i], b.pts[i + 1]
                # Лапа шире посередине, к концу сходится клином.
                wa = wd * (0.55 + 0.45 * math.sin(min(qa * 1.4, 1.0) * math.pi * 0.5)) * (1.0 - 0.35 * qa ** 2)
                wb = wd * (0.55 + 0.45 * math.sin(min(qb * 1.4, 1.0) * math.pi * 0.5)) * (1.0 - 0.35 * qb ** 2)
                ua, ub = 0.03 + 0.95 * qa, 0.03 + 0.95 * qb
                sha = (inner + (1.0 - inner) * qa ** 0.8) * low
                shb = (inner + (1.0 - inner) * qb ** 0.8) * low
                # Нормаль: от оси ствола наружу и вверх.
                def nrm_of(p, up):
                    r = p - axis_at(p)
                    r.z = 0
                    r = r.normalized() if r.length > 1e-4 else b.out
                    return (r * 0.75 + Vector((0, 0, up))).normalized()
                for sgn, v_edge in ((1.0, v_top), (-1.0, v_bot)):
                    droop = Vector((0, 0, -0.32))
                    ea = pa + (side * sgn + droop) * wa
                    eb = pb + (side * sgn + droop) * wb
                    ps = [pa, pb, eb, ea] if sgn > 0 else [pb, pa, ea, eb]
                    uvs = [(ua, v_mid), (ub, v_mid), (ub, v_edge), (ua, v_edge)] if sgn > 0 else [(ub, v_mid), (ua, v_mid), (ua, v_edge), (ub, v_edge)]
                    sws = [b.sway[i], b.sway[i + 1], b.sway[i + 1], b.sway[i]] if sgn > 0 else [b.sway[i + 1], b.sway[i], b.sway[i], b.sway[i + 1]]
                    shs = [sha, shb, shb * 1.08, sha * 1.08] if sgn > 0 else [shb, sha, sha * 1.08, shb * 1.08]
                    quad(ps, uvs, sws, shs, b.ph, [nrm_of(p, 0.75) for p in ps])
                if not b.primary or not curtains:
                    continue
                # Занавес под ветвью: хвоя свисает, сбоку ярус читается плотным.
                ha, hb = wa * 0.95, wb * 0.95
                ps = [pa + Vector((0, 0, 0.05)), pb + Vector((0, 0, 0.05)), pb - Vector((0, 0, hb)), pa - Vector((0, 0, ha))]
                uvs = [(ua, v_mid), (ub, v_mid), (ub, v_bot), (ua, v_bot)]
                sws = [b.sway[i], b.sway[i + 1], b.sway[i + 1], b.sway[i]]
                shs = [sha, shb, shb * 0.8, sha * 0.8]
                quad(ps, uvs, sws, shs, b.ph, [nrm_of(p, 0.45) for p in ps])
        # Макушка: две скрещённые полосы вдоль ствола.
        top = tr_pts[-1]
        hh = H * 0.13
        for ang in (0.3, 0.3 + math.pi / 2):
            d = Vector((math.cos(ang), math.sin(ang), 0)) * hh * 0.24 * width_mul
            base = top - Vector((0, 0, hh * 0.85))
            tip = top + Vector((0, 0, hh * 0.15))
            ps = [base - d, base + d, tip + d, tip - d]
            uvs = [(0.45, 0.02), (0.45, 0.23), (0.98, 0.23), (0.98, 0.02)]
            quad(ps, uvs, [0.8, 0.8, 1.0, 1.0], [0.9, 0.9, 1.05, 1.05], 0.5, [(Vector((math.cos(ang + math.pi / 2), math.sin(ang + math.pi / 2), 0)) * 0.4 + Vector((0, 0, 0.9))).normalized()] * 4)
        bm.faces.ensure_lookup_table()
        me = bpy.data.meshes.new(name)
        flat = []
        for f in bm.faces:
            flat.extend(normals[f])
        bm.to_mesh(me)
        bm.free()
        me.normals_split_custom_set(flat)
        return me

    def bark_mesh(name, sides, with_boughs):
        bm = bmesh.new()
        uv0 = bm.loops.layers.uv.new("UVMap")
        uv1 = bm.loops.layers.uv.new("Wind")
        tube(bm, uv0, uv1, trunk, sides, 99)
        if with_boughs:
            # Сучья видны только снизу, под кроной: выше их закрывает хвоя.
            for b in boughs:
                if b.primary and b.tw < 0.4:
                    tube(bm, uv0, uv1, b, 3, 99)
        return build_mesh(name, bm)

    meshes = {
        f"{tid}_lod0_bark": bark_mesh(f"{tid}_lod0_bark", 8, True),
        f"{tid}_lod0_leaf": leaf_mesh(f"{tid}_lod0_leaf", boughs, 1.0),
        f"{tid}_lod1_bark": bark_mesh(f"{tid}_lod1_bark", 5, False),
        # Вдали — каждая вторая ветвь, зато шире: силуэт тот же, треугольников вдвое меньше.
        f"{tid}_lod1_leaf": leaf_mesh(f"{tid}_lod1_leaf", [b for b in boughs if b.primary][::2], 1.75),
    }
    return meshes, crown_c


def materials():
    """Материалы для превью/рендера импостора (в игре материалы свои)."""
    def img(path, srgb=True):
        im = bpy.data.images.load(path, check_existing=True)
        if not srgb:
            im.colorspace_settings.name = "Non-Color"
        return im

    out = {}
    bark = bpy.data.materials.get("mrpg_bark") or bpy.data.materials.new("mrpg_bark")
    bark.use_nodes = True
    nt = bark.node_tree
    nt.nodes.clear()
    o = nt.nodes.new("ShaderNodeOutputMaterial")
    p = nt.nodes.new("ShaderNodeBsdfPrincipled")
    t = nt.nodes.new("ShaderNodeTexImage")
    t.image = img(os.path.join(BARK_DIR, "Bark014_1K-JPG_Color.jpg"))
    nm = nt.nodes.new("ShaderNodeNormalMap")
    tn = nt.nodes.new("ShaderNodeTexImage")
    tn.image = img(os.path.join(BARK_DIR, "Bark014_1K-JPG_NormalGL.jpg"), False)
    # Bark014 светлый: приглушаем до серо-бурой коры (в игре — тот же множитель).
    mul = nt.nodes.new("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    mul.inputs[0].default_value = 1.0
    mul.inputs[7].default_value = (*BARK_TINT, 1.0)
    nt.links.new(t.outputs[0], mul.inputs[6])
    nt.links.new(mul.outputs[2], p.inputs["Base Color"])
    nt.links.new(tn.outputs[0], nm.inputs["Color"])
    nt.links.new(nm.outputs[0], p.inputs["Normal"])
    p.inputs["Roughness"].default_value = 0.9
    nt.links.new(p.outputs[0], o.inputs[0])
    out["bark"] = bark
    for a in ["broadleaf", "pine", "scale", "bough"]:
        m = bpy.data.materials.get(f"mrpg_leaf_{a}") or bpy.data.materials.new(f"mrpg_leaf_{a}")
        m.use_nodes = True
        nt = m.node_tree
        nt.nodes.clear()
        o = nt.nodes.new("ShaderNodeOutputMaterial")
        p = nt.nodes.new("ShaderNodeBsdfPrincipled")
        t = nt.nodes.new("ShaderNodeTexImage")
        t.image = img(os.path.join(ATLAS_DIR, f"{a}_atlas.png"))
        # Цвет вершин (затенение кроны у хвойных) умножается на атлас, как в игре.
        vc = nt.nodes.new("ShaderNodeVertexColor")
        vc.layer_name = "Color"
        mulc = nt.nodes.new("ShaderNodeMix")
        mulc.data_type = "RGBA"
        mulc.blend_type = "MULTIPLY"
        mulc.inputs[0].default_value = 1.0 if a == "bough" else 0.0
        nt.links.new(t.outputs[0], mulc.inputs[6])
        nt.links.new(vc.outputs[0], mulc.inputs[7])
        nt.links.new(mulc.outputs[2], p.inputs["Base Color"])
        nt.links.new(t.outputs[1], p.inputs["Alpha"])
        p.inputs["Roughness"].default_value = 0.8
        nt.links.new(p.outputs[0], o.inputs[0])
        try:
            m.surface_render_method = "DITHERED"
        except Exception:
            pass
        out[a] = m
    return out


def impostor(tid, meshes, mats, sp, out_dir, scene):
    """Вид дерева сбоку (цвет + альфа) для дальнего LOD: две скрещённые плоскости."""
    for ob in list(scene.objects):
        bpy.data.objects.remove(ob, do_unlink=True)
    objs = []
    for name in (f"{tid}_lod0_bark", f"{tid}_lod0_leaf"):
        me = meshes[name]
        me.materials.clear()
        me.materials.append(mats["bark"] if name.endswith("bark") else mats[sp.atlas])
        ob = bpy.data.objects.new(name, me)
        scene.collection.objects.link(ob)
        objs.append(ob)
    pts = [ob.matrix_world @ v.co for ob in objs for v in ob.data.vertices]
    mn = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    mx = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    w = max(mx.x - mn.x, mx.y - mn.y)
    h = mx.z - max(mn.z, 0.0)
    size = max(w, h)
    cam = bpy.data.objects.new("imp_cam", bpy.data.cameras.new("imp_cam"))
    scene.collection.objects.link(cam)
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = size
    cz = max(mn.z, 0.0) + size / 2
    cam.location = Vector((0, -50, cz))
    cam.rotation_euler = (math.pi / 2, 0, 0)
    scene.camera = cam
    sun = bpy.data.objects.new("imp_sun", bpy.data.lights.new("imp_sun", "SUN"))
    scene.collection.objects.link(sun)
    sun.data.energy = 2.5
    sun.rotation_euler = (0.9, 0.0, 0.5)
    if not scene.world:
        scene.world = bpy.data.worlds.new("imp_world")
    scene.world.use_nodes = True
    scene.world.node_tree.nodes["Background"].inputs[0].default_value = (0.6, 0.65, 0.75, 1)
    scene.world.node_tree.nodes["Background"].inputs[1].default_value = 1.2
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 32
    scene.render.film_transparent = True
    scene.render.resolution_x = IMP_SIZE
    scene.render.resolution_y = IMP_SIZE
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.view_settings.view_transform = "Standard"
    path = os.path.join(out_dir, f"{tid}_imp.png")
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True, scene=scene.name)
    # Две скрещённые плоскости размером size × size, низ на земле (z = cz - size/2).
    bm = bmesh.new()
    uv0 = bm.loops.layers.uv.new("UVMap")
    uv1 = bm.loops.layers.uv.new("Wind")
    z0 = cz - size / 2
    for ang in (0.0, math.pi / 2):
        d = Vector((math.cos(ang), math.sin(ang), 0)) * size / 2
        vs = [bm.verts.new(c) for c in (-d + Vector((0, 0, z0)), d + Vector((0, 0, z0)), d + Vector((0, 0, z0 + size)), -d + Vector((0, 0, z0 + size)))]
        f = bm.faces.new(vs)
        for l, uv in zip(f.loops, [(0, 0), (1, 0), (1, 1), (0, 1)]):
            l[uv0].uv = uv
            l[uv1].uv = (uv[1] * 0.3, 0.0)
    me = bpy.data.meshes.new(f"{tid}_imp")
    bm.to_mesh(me)
    bm.free()
    # Нормали плоскостей — вверх-наружу, чтобы освещение не мигало при повороте.
    me.normals_split_custom_set([Vector((0, 0, 1)) for _ in me.loops])
    # Сначала отвязываем камеру от сцены: иначе экспорт glTF падает на висячей ссылке.
    scene.camera = None
    for ob in list(scene.objects):
        bpy.data.objects.remove(ob, do_unlink=True)
    return me, path


def export(tid, meshes, out_dir, scene):
    for ob in list(scene.objects):
        bpy.data.objects.remove(ob, do_unlink=True)
    for name, me in meshes.items():
        me.materials.clear()
        ob = bpy.data.objects.new(name, me)
        scene.collection.objects.link(ob)
    path = os.path.join(out_dir, f"{tid}.glb")
    with bpy.context.temp_override(scene=scene, view_layer=scene.view_layers[0]):
        bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=False, export_materials="NONE",
                                  export_yup=True, export_animations=False, use_active_scene=True,
                                  export_vertex_color="ACTIVE", export_active_vertex_color_when_no_material=True)
    for ob in list(scene.objects):
        bpy.data.objects.remove(ob, do_unlink=True)
    return path


VARIANTS = {"pine": 3, "oak": 3, "juniper": 3, "cypress": 2, "fig": 2, "shrub": 3, "fir": 3}


def build_all(out_dir, only=None):
    os.makedirs(out_dir, exist_ok=True)
    scene = bpy.data.scenes.get("mrpg_trees") or bpy.data.scenes.new("mrpg_trees")
    mats = materials()
    report = {}
    for key, sp in SPECIES.items():
        if only and key not in only:
            continue
        for v in range(VARIANTS[key]):
            tid = f"{key}_{v}"
            seed = 1000 + 97 * v + len(key) * 13
            meshes, _ = make_conifer(tid, sp, seed) if getattr(sp, "conifer", False) else make_tree(tid, sp, seed)
            imp, _ = impostor(tid, meshes, mats, sp, out_dir, scene)
            meshes[f"{tid}_imp"] = imp
            export(tid, meshes, out_dir, scene)
            report[tid] = {n: len(m.polygons) for n, m in meshes.items()}
    return report
