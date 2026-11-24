"""Товары, травы, вещества, посуда, книги и снадобья Херсона (около 510 г.).

Модели собраны из нескольких «семейств» (пучок трав, горка в плошке, флакон, кувшин,
кодекс…) с разными параметрами. Регистрируются тем же декоратором, что и оружие.
"""
import math
import random

import itemkit as K
from itemkit import Vector, item
import zlib


def _seed(key):
    """Стабильное зерно случайности (hash() в Python меняется от запуска к запуску)."""
    return zlib.crc32(key.encode()) % 1000

M = K.common_materials

# =========================================================================== материалы


def ceramic(name, color, color2=None, rough=(0.65, 0.85)):
    return K.organic_mat(name, color, color2 or tuple(int(c * 0.7) for c in color), rough=rough, bump=0.25, bump_scale=90.0, dirt=0.7, wear=0.4, wear_color=tuple(min(255, int(c * 1.15)) for c in color))


def substance(name, color, color2=None, rough=(0.5, 0.8), bump=0.4):
    return K.organic_mat(name, color, color2 or tuple(int(c * 0.65) for c in color), rough=rough, bump=bump, bump_scale=160.0, dirt=0.5)


def resin(name, color, transmission=0.4):
    m = K.gem_mat(name, color, transmission=transmission, rough=0.25)
    bsdf = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["IOR"].default_value = 1.54
    bsdf.inputs["Coat Weight"].default_value = 0.2
    return m


def leaf_mat(name, color, color2=None):
    return K.organic_mat(name, color, color2 or tuple(int(c * 0.6) for c in color), rough=(0.45, 0.7), bump=0.3, bump_scale=200.0, dirt=0.3, grain="Z", grain_scale=6.0)


# =========================================================================== семейства моделей

SEED = [1]


def rng():
    SEED[0] += 1
    return random.Random(SEED[0] * 7919)


def lump(center, size, rough=0.25, segs=10, squash=1.0, seed=0):
    """Неровный комок (смола, сера, кристалл…): деформированная сфера."""
    r = random.Random(seed)
    bm = K.lathe([(-size * squash, 0.0001)] + [(size * squash * math.sin(-math.pi / 2 + math.pi * i / 6), size * math.cos(-math.pi / 2 + math.pi * i / 6)) for i in range(1, 6)] + [(size * squash, 0.0001)], segs)
    ph = [r.uniform(0, 6.28) for _ in range(6)]
    K.deform(bm, lambda v: v * (1 + rough * (0.5 * math.sin(v.x / size * 3 + ph[0]) * math.sin(v.y / size * 2.6 + ph[1]) + 0.5 * math.sin(v.z / size * 3.3 + ph[2]))))
    K.move(bm, *center)
    return bm


def crystal(center, size, seed=0):
    """Кристалл: вытянутая призма с гранями (квасцы, купорос)."""
    r = random.Random(seed)
    h = size * r.uniform(1.0, 1.8)
    bm = K.lathe([(0, 0.0001), (h * 0.15, size * 0.5), (h * 0.85, size * 0.5), (h, 0.0001)], 6)
    K.rot(bm, "X", r.uniform(-50, 50))
    K.rot(bm, "Z", r.uniform(0, 360))
    K.move(bm, *center)
    return bm


def dish(radius=0.045, depth=0.012, mat="clay"):
    """Плоская глиняная плошка."""
    prof = [(0.0, radius * 0.55), (0.002, radius * 0.6), (depth * 0.4, radius * 0.85), (depth, radius), (depth + 0.002, radius * 1.02), (depth + 0.0025, radius * 0.94), (depth * 0.5 + 0.002, radius * 0.78), (0.004, radius * 0.5), (0.004, 0.0001)]
    K.mesh_obj("dish", K.lathe([(0.0, 0.0001)] + prof, 40), mat, subsurf=1, smooth_angle=70)
    return depth


def heap(items, mat, base_z=0.004, spread=0.03, count=18, size=0.006, shape="lump", seed=1, height=0.02):
    """Горка из комочков/ягод/кристаллов."""
    r = random.Random(seed)
    import bmesh  # noqa: F401

    parts = []
    for i in range(count):
        a = r.uniform(0, 2 * math.pi)
        d = spread * math.sqrt(r.random())
        z = base_z + size + height * (1 - d / spread) * r.uniform(0.3, 1.0)
        c = (d * math.cos(a), d * math.sin(a), z)
        s = size * r.uniform(0.7, 1.3)
        if shape == "sphere":
            b = K.lathe([(-s, 0.0001), (-s * 0.7, s * 0.7), (0, s), (s * 0.7, s * 0.7), (s, 0.0001)], 10)
            K.move(b, *c)
        elif shape == "oval":
            b = K.lathe([(-s * 1.4, 0.0001), (-s, s * 0.7), (0, s), (s, s * 0.7), (s * 1.4, 0.0001)], 10)
            K.rot(b, "X", r.uniform(30, 90))
            K.rot(b, "Z", r.uniform(0, 360))
            K.move(b, *c)
        elif shape == "crystal":
            b = crystal(c, s, seed=seed * 100 + i)
        else:
            b = lump(c, s, rough=0.3, squash=r.uniform(0.6, 1.0), seed=seed * 100 + i)
        parts.append(b)
    K.mesh_obj(items, K.merge(*parts), mat, subsurf=0 if shape == "crystal" else 1, smooth_angle=35 if shape == "crystal" else 70)


def powder_mound(radius, height, mat, z=0.004):
    rings = []
    for i in range(9):
        t = i / 8
        rings.append((z + height * (1 - t**2) ** 1.2, radius * t + 1e-4))
    rings.reverse()
    b = K.lathe([(z - 0.001, radius)] + [(z + height * (1 - (1 - i / 8) ** 2), radius * (1 - i / 8) + 1e-4) for i in range(9)], 32, cap_top=False)
    r = random.Random(3)
    ph = [r.uniform(0, 6) for _ in range(3)]
    K.deform(b, lambda v: (v.x, v.y, v.z + 0.0015 * math.sin(v.x * 300 + ph[0]) * math.sin(v.y * 260 + ph[1]) * (v.z - z) / max(height, 1e-4)))
    K.mesh_obj("powder", b, mat, subsurf=1, smooth_angle=80)


def cloth_square(size=0.11, mat="linen", z=0.0):
    """Кусок ткани, на котором лежит вещество."""
    rows = []
    n = 14
    r = random.Random(4)
    ph = [r.uniform(0, 6) for _ in range(4)]
    for i in range(n + 1):
        row = []
        for j in range(n + 1):
            x = (i / n - 0.5) * size
            y = (j / n - 0.5) * size
            d = math.hypot(x, y) / size
            zz = z + 0.004 * math.sin(x * 60 + ph[0]) * math.sin(y * 50 + ph[1]) + 0.012 * max(0, d - 0.35)
            row.append((x, y, zz))
        rows.append(row)
    K.mesh_obj("cloth", K.grid_surface(rows), mat, solidify=0.0015, subsurf=1, smooth_angle=80)


def herb_bundle(leaf, flower=None, leaf_shape="lance", n=7, leaf_size=0.016, flower_kind="dots", flower_size=0.004, stem="stem", length=0.17, seed=1, leaves_per=6):
    """Пучок сушёной травы, перевязанный бечёвкой. Лежит плашмя, верхушки вверх по Z."""
    r = random.Random(seed)
    stems, leaves, flowers = [], [], []
    for i in range(n):
        ang = (i - (n - 1) / 2) * 0.16 + r.uniform(-0.05, 0.05)
        depth = r.uniform(-0.012, 0.012)
        L = length * r.uniform(0.85, 1.05)
        path = []
        for k in range(9):
            t = k / 8
            # Ниже бечёвки стебли сходятся, выше — расходятся веером.
            spread = (t - 0.25) if t > 0.25 else (0.25 - t) * 0.3
            path.append((math.sin(ang) * spread * L * 0.9, depth * spread * 2, L * t))
        stems.append(K.loft(path, [0.0016 * (1 - 0.4 * k / 8) for k in range(9)], 6))
        for k in range(leaves_per):
            t = 0.35 + 0.6 * k / leaves_per + r.uniform(-0.03, 0.03)
            px = math.sin(ang) * (t - 0.25) * L * 0.9
            pz = L * t
            side = 1 if k % 2 else -1
            s = leaf_size * (1 - 0.4 * (t - 0.35)) * r.uniform(0.8, 1.15)
            if leaf_shape == "feather":
                lf = K.lathe([(0, 0.0001), (s * 0.2, s * 0.08), (s, 0.0001)], 4)
            elif leaf_shape == "round":
                lf = K.lathe([(0, 0.0001), (s * 0.3, s * 0.4), (s * 0.7, s * 0.42), (s, 0.0001)], 8)
            elif leaf_shape == "small":
                lf = K.lathe([(0, 0.0001), (s * 0.3, s * 0.22), (s * 0.6, 0.0001)], 6)
            else:
                lf = K.lathe([(0, 0.0001), (s * 0.35, s * 0.22), (s, 0.0001)], 8)
            K.scale(lf, 1.0, 0.25, 1.0)
            K.rot(lf, "Y", side * r.uniform(35, 65))
            K.rot(lf, "Z", r.uniform(-40, 40))
            K.move(lf, px, depth * (t - 0.25) * 2, pz)
            leaves.append(lf)
        if flower:
            tip = Vector(path[-1])
            for k in range(6 if flower_kind == "dots" else 1):
                if flower_kind == "dots":
                    c = tip + Vector((r.uniform(-0.008, 0.008), r.uniform(-0.004, 0.004), r.uniform(-0.016, 0.004)))
                    b = K.lathe([(-flower_size, 0.0001), (0, flower_size), (flower_size, 0.0001)], 8)
                elif flower_kind == "umbel":
                    c = tip
                    b = K.lathe([(-flower_size * 0.4, 0.0001), (0, flower_size * 3), (flower_size * 0.6, flower_size * 2.6), (flower_size, 0.0001)], 16)
                elif flower_kind == "capsule":
                    c = tip + Vector((0, 0, flower_size))
                    b = K.lathe([(-flower_size * 1.2, 0.0001), (-flower_size, flower_size * 0.6), (0, flower_size), (flower_size * 0.8, flower_size * 0.8), (flower_size, flower_size * 0.4), (flower_size * 1.15, flower_size * 0.75), (flower_size * 1.25, 0.0001)], 14)
                    K.deform(b, lambda v, s=flower_size: (v.x * (1 + 0.08 * math.sin(math.atan2(v.y, v.x) * 8)), v.y * (1 + 0.08 * math.sin(math.atan2(v.y, v.x) * 8)), v.z))
                else:  # spike — колос мелких цветков
                    c = tip
                    b = K.lathe([(-flower_size * 4, 0.0001), (-flower_size * 3, flower_size * 0.9), (0, flower_size), (flower_size, 0.0001)], 8)
                K.move(b, *c)
                flowers.append(b)
    K.mesh_obj("stems", K.merge(*stems), stem, subsurf=0, smooth_angle=80)
    K.mesh_obj("leaves", K.merge(*leaves), leaf, subsurf=0, solidify=0.0006, smooth_angle=80)
    if flowers:
        K.mesh_obj("flowers", K.merge(*flowers), "flower", subsurf=1 if flower_kind != "dots" else 0, smooth_angle=80)
    K.mesh_obj("tie", K.torus((0, 0, length * 0.25), 0.0085, 0.0014, ry=0.007), "cord")
    K.mesh_obj("tie2", K.torus((0, 0, length * 0.25 + 0.004), 0.0085, 0.0014, ry=0.007), "cord")


def roots_bundle(mat, n=6, thick=0.006, length=0.13, gnarl=0.4, seed=2, tie=True, cut=False):
    """Связка корней или кусков корня."""
    r = random.Random(seed)
    parts = []
    for i in range(n):
        x0 = r.uniform(-0.02, 0.02)
        y0 = r.uniform(-0.01, 0.01)
        L = length * r.uniform(0.7, 1.0)
        path = []
        ph = r.uniform(0, 6)
        for k in range(13):
            t = k / 12
            path.append((x0 + gnarl * 0.02 * math.sin(t * 7 + ph) + (t - 0.5) * r.uniform(-0.02, 0.02), y0 + gnarl * 0.01 * math.cos(t * 5 + ph), L * t))
        th = thick * r.uniform(0.7, 1.3)
        radii = [th * (1 - 0.55 * k / 12) * (1 + 0.15 * math.sin(k * 2.3 + ph)) for k in range(13)]
        parts.append(K.loft(path, radii, 10, cap_start=True, cap_end=True))
    K.mesh_obj("roots", K.merge(*parts), mat, subsurf=1, smooth_angle=80)
    if tie:
        K.mesh_obj("tie", K.torus((0, 0, length * 0.35), 0.024, 0.0016, ry=0.014), "cord")


def flask_body(kind="round", liquid=None, seal="wax", liquid_level=0.45, scale=1.0, glass="glass"):
    """Стеклянный сосуд: round — шаровидный флакон, tall — веретенообразный унгвентарий,
    small — маленький пузырёк."""
    if kind == "tall":
        prof = [(0.0, 0.0001), (0.004, 0.012), (0.02, 0.022), (0.045, 0.028), (0.07, 0.022), (0.1, 0.009), (0.15, 0.008), (0.155, 0.012), (0.158, 0.01)]
    elif kind == "small":
        prof = [(0.0, 0.0001), (0.002, 0.015), (0.012, 0.022), (0.03, 0.024), (0.045, 0.016), (0.055, 0.008), (0.072, 0.007), (0.075, 0.0105), (0.078, 0.0085)]
    else:
        prof = [(0.0, 0.0001), (0.003, 0.03), (0.01, 0.044), (0.03, 0.052), (0.055, 0.05), (0.075, 0.04), (0.09, 0.022), (0.1, 0.0125), (0.15, 0.0105), (0.156, 0.0145), (0.161, 0.0125)]
    prof = [(z * scale, r * scale) for z, r in prof]
    top = prof[-1][0]
    K.mesh_obj("glass", K.lathe(prof, 40, cap_top=False), glass, solidify=0.0016, subsurf=1, glass=True, smooth_angle=89)
    if liquid:
        # Жидкость: профиль сосуда чуть внутрь, до уровня.
        hmax = top * liquid_level
        inner = [(max(z, 0.002), r * 0.92) for z, r in prof if z <= hmax]
        rr = next((r for z, r in prof if z > hmax), prof[-1][1]) * 0.92
        inner.append((hmax, rr))
        inner = [(0.002, 0.0001)] + inner[1:]
        K.mesh_obj("liquid", K.lathe(inner, 40), liquid, subsurf=1, smooth_angle=89)
    neck_r = prof[-3][1]
    if seal == "wax":
        s = K.lathe([(top - 0.009 * scale, neck_r * 1.2), (top - 0.003 * scale, neck_r * 1.45), (top + 0.005 * scale, neck_r * 1.2), (top + 0.01 * scale, neck_r * 0.7), (top + 0.012 * scale, 0.0001)], 28)
        K.deform(s, lambda v: (v.x * (1 + 0.08 * math.sin(math.atan2(v.y, v.x) * 5)), v.y * (1 + 0.08 * math.sin(math.atan2(v.y, v.x) * 5)), v.z))
        K.mesh_obj("seal", s, "wax", subsurf=1, smooth_angle=89)
        K.mesh_obj("tie", K.torus((0, 0, top - 0.014 * scale), neck_r * 1.12, 0.0012), "cord")
    elif seal == "cork":
        K.mesh_obj("cork", K.lathe([(top - 0.02 * scale, neck_r * 0.9), (top + 0.008 * scale, neck_r * 1.0)], 16), "wood", bevel=0.001)
    return top


def jug(mat, height=0.2, seal="cork", seal_mat="wood"):
    """Глиняный кувшин с ручкой (ойнохоя)."""
    h = height
    prof = [(0.0, 0.0001), (0.0, 0.03), (0.006, 0.034), (0.02, 0.042), (0.06, 0.058), (0.1, 0.06), (0.13, 0.05), (0.15, 0.03), (0.17, 0.022), (0.19, 0.024), (0.198, 0.03), (0.2, 0.026)]
    prof = [(z * h / 0.2, r) for z, r in prof]
    body = K.lathe(prof, 40, cap_top=False)
    # Лёгкие следы гончарного круга.
    K.deform(body, lambda v: (v.x * (1 + 0.006 * math.sin(v.z * 600)), v.y * (1 + 0.006 * math.sin(v.z * 600)), v.z))
    K.mesh_obj("jug", body, mat, solidify=0.004, subsurf=1, smooth_angle=80)
    hp = K.bezier((0, 0.023, h * 0.93), (0, 0.06, h * 0.98), (0, 0.075, h * 0.75), (0, 0.055, h * 0.55), n=14)
    K.mesh_obj("handle", K.loft(hp, [(0.009, 0.005)] * len(hp), 10, up=(1, 0, 0)), mat, subsurf=1)
    if seal == "cork":
        K.mesh_obj("stopper", K.lathe([(h * 0.93, 0.019), (h + 0.012, 0.021), (h + 0.016, 0.0001)], 20), seal_mat, subsurf=1)
    elif seal == "cloth":
        cl = K.lathe([(h + 0.01, 0.0001), (h + 0.006, 0.026), (h - 0.004, 0.034), (h - 0.016, 0.033)], 28, cap_top=False, cap_bot=False)
        K.deform(cl, lambda v: (v.x * (1 + 0.1 * math.sin(math.atan2(v.y, v.x) * 7) * max(0, (h - v.z) / 0.016)), v.y * (1 + 0.1 * math.sin(math.atan2(v.y, v.x) * 7) * max(0, (h - v.z) / 0.016)), v.z))
        K.mesh_obj("cloth", cl, seal_mat, solidify=0.0012, subsurf=1)
        K.mesh_obj("tie", K.torus((0, 0, h - 0.008), 0.027, 0.0014), "cord")


def pot(mat, r=0.05, h=0.07, lid=None, lid_mat=None, neck=0.7, filling=None, cloth=None, handles=False):
    """Глиняный горшок / банка. lid: 'lid' — крышка, 'cloth' — тряпица на бечёвке."""
    prof = [(0.0, 0.0001), (0.0, r * 0.6), (0.004, r * 0.7), (h * 0.3, r * 0.98), (h * 0.55, r), (h * 0.85, r * neck * 1.05), (h * 0.95, r * neck), (h, r * neck * 1.1), (h + 0.003, r * neck * 1.05)]
    body = K.lathe(prof, 40, cap_top=False)
    K.deform(body, lambda v: (v.x * (1 + 0.005 * math.sin(v.z * 500)), v.y * (1 + 0.005 * math.sin(v.z * 500)), v.z))
    K.mesh_obj("pot", body, mat, solidify=0.004, subsurf=1, smooth_angle=80)
    top = h + 0.003
    nr = r * neck
    if filling:
        K.mesh_obj("filling", K.lathe([(h * 0.8, nr * 0.95), (h * 0.84, nr * 0.6), (h * 0.85, 0.0001)], 32), filling, subsurf=1)
    if lid == "lid":
        K.mesh_obj("lid", K.lathe([(top - 0.004, nr * 0.95), (top, nr * 1.15), (top + 0.006, nr * 1.1), (top + 0.012, nr * 0.4), (top + 0.018, nr * 0.25), (top + 0.022, 0.0001)], 32), lid_mat or mat, subsurf=1)
    elif lid == "cloth":
        cl = K.lathe([(top + 0.008, 0.0001), (top + 0.005, nr * 1.0), (top - 0.002, nr * 1.3), (top - 0.018, nr * 1.32)], 28, cap_top=False, cap_bot=False)
        K.deform(cl, lambda v: (v.x * (1 + 0.1 * math.sin(math.atan2(v.y, v.x) * 7) * max(0, (top - v.z) / 0.018)), v.y * (1 + 0.1 * math.sin(math.atan2(v.y, v.x) * 7) * max(0, (top - v.z) / 0.018)), v.z))
        K.mesh_obj("cloth", cl, cloth or "linen", solidify=0.0012, subsurf=1)
        K.mesh_obj("tie", K.torus((0, 0, top - 0.006), nr * 1.15, 0.0015), "cord")
    if handles:
        for s in (-1, 1):
            hp = K.bezier((s * r * 0.95, 0, h * 0.75), (s * (r + 0.025), 0, h * 0.8), (s * (r + 0.025), 0, h * 0.45), (s * r * 0.98, 0, h * 0.42), n=10)
            K.mesh_obj("handle", K.loft(hp, [(0.006, 0.004)] * len(hp), 8, up=(0, 1, 0)), mat, subsurf=1)
    return top


def codex(cover, w=0.15, h=0.21, t=0.04, clasp="bronze", page="parchment", boss=True, seed=1, cross=False):
    """Кодекс: деревянные доски в коже, блок пергамента, застёжки."""
    K.mesh_obj("block", K.cbox(0.004, 0, 0, w - 0.01, t - 0.008, h - 0.012), page, bevel=0.002)
    for s in (-1, 1):
        K.mesh_obj("board", K.cbox(0, s * (t / 2 - 0.003), 0, w, 0.006, h), cover, bevel=0.003)
    K.mesh_obj("spine", K.cbox(-w / 2 + 0.002, 0, 0, 0.012, t, h), cover, bevel=0.005, subsurf=1)
    # Бинты корешка и застёжки.
    for z in (-h * 0.3, 0, h * 0.3):
        K.mesh_obj("band", K.cbox(-w / 2 - 0.002, 0, z, 0.006, t + 0.004, 0.008), cover, bevel=0.002, subsurf=1)
    for z in (-h * 0.25, h * 0.25):
        K.mesh_obj("strap", K.cbox(w / 2 + 0.004, -0.002, z, 0.02, t + 0.004, 0.01), "leather_dark", bevel=0.0015)
        K.mesh_obj("clasp", K.cbox(w / 2 - 0.004, -t / 2 - 0.0015, z, 0.016, 0.003, 0.014), clasp, bevel=0.0012)
    if boss:
        for x, z in ((-w * 0.3, -h * 0.35), (w * 0.3, -h * 0.35), (-w * 0.3, h * 0.35), (w * 0.3, h * 0.35), (0, 0)):
            b = K.lathe([(0, 0.008 if (x, z) == (0, 0) else 0.0055), (0.003, 0.006 if (x, z) == (0, 0) else 0.004), (0.005, 0.0001)], 14)
            K.rot(b, "X", 90)
            K.move(b, x, -t / 2 - 0.006 + 0.0015, z)
            K.mesh_obj("boss", b, clasp, subsurf=1)
    if cross:
        # Крест на доске — украшение церковных книг.
        K.mesh_obj("cross_v", K.cbox(0, -t / 2 - 0.0062, 0, 0.008, 0.002, h * 0.55), clasp, bevel=0.0008)
        K.mesh_obj("cross_h", K.cbox(0, -t / 2 - 0.0062, h * 0.08, w * 0.45, 0.002, 0.008), clasp, bevel=0.0008)
    else:
        # Светские книги: тиснёная рамка и ромб на коже.
        y = -t / 2 - 0.0062
        for dx, dz, ww, hh in ((0, h * 0.42, w * 0.8, 0.003), (0, -h * 0.42, w * 0.8, 0.003), (w * 0.4, 0, 0.003, h * 0.84), (-w * 0.4, 0, 0.003, h * 0.84)):
            K.mesh_obj("tool", K.cbox(dx, y + 0.0012, dz, ww, 0.0016, hh), "leather_dark")
        d = math.hypot(w * 0.3, h * 0.3)
        ang = math.degrees(math.atan2(h * 0.3, w * 0.3))
        for sx, sz in ((1, 1), (-1, 1), (1, -1), (-1, -1)):
            bar = K.cbox(0, 0, 0, d, 0.0016, 0.003)
            K.rot(bar, "Y", sx * sz * ang)
            K.move(bar, sx * w * 0.15, y + 0.0012, sz * h * 0.15)
            K.mesh_obj("tool", bar, "leather_dark")


def scroll(length=0.2, r=0.016, paper="papyrus", rod="wood", tag=None, tie="cord"):
    """Свиток папируса на палочке, перевязанный, с ярлыком-ситтибосом."""
    K.mesh_obj("roll", K.lathe([(-length / 2, r * 0.96), (-length / 2 + 0.003, r), (length / 2 - 0.003, r), (length / 2, r * 0.96)], 32, axis="Z"), paper, subsurf=1, smooth_angle=60)
    for s in (-1, 1):
        K.mesh_obj("knob", K.lathe([(s * length / 2, 0.005), (s * (length / 2 + 0.012), 0.006), (s * (length / 2 + 0.016), 0.0001)] if s > 0 else [(s * (length / 2 + 0.016), 0.0001), (s * (length / 2 + 0.012), 0.006), (s * length / 2, 0.005)], 14), rod, subsurf=1)
    K.mesh_obj("tie", K.torus((0, 0, 0.0), r + 0.0012, 0.0012), tie)
    if tag:
        t = K.cbox(0, -r - 0.003, -0.03, 0.022, 0.0012, 0.05)
        K.rot(t, "Y", 8)
        K.mesh_obj("tag", t, tag, bevel=0.0005)


def coin_pile(mat, r, t, n, relief="bust", seed=3):
    rnd = random.Random(seed)

    def coin(x, y, z, tx, ty, rz):
        c = K.lathe([(0, 0.0001), (0, r * 0.82), (0.0002, r * 0.86), (0.00012, r * 0.9), (0.0, r * 0.98), (0.0004, r), (t - 0.0004, r), (t, r * 0.98), (t - 0.00012, r * 0.9), (t - 0.0002, r * 0.86), (t, r * 0.82), (t, 0.0001)], 40, cap_top=False, cap_bot=False)
        if relief == "M":
            # Большая «M» — знак 40 нуммиев на фоллисе.
            def m(v):
                if v.z < t * 0.5:
                    return v
                u, w = v.x / r, v.y / r
                strokes = [abs(u + 0.32) < 0.07 and abs(w) < 0.4, abs(u - 0.32) < 0.07 and abs(w) < 0.4, abs((u + 0.32) * 0.75 - (w - 0.4) * -0.4) < 0.05 and -0.32 < u < 0 and w > -0.05, abs((u - 0.32) * 0.75 + (w - 0.4) * -0.4) < 0.05 and 0 < u < 0.32 and w > -0.05]
                return Vector((v.x, v.y, v.z + (0.00035 if any(strokes) else 0)))

            K.deform(c, m)
        else:
            K.deform(c, lambda v: (v.x, v.y, v.z + (0.0002 * math.exp(-((v.x + r * 0.1) ** 2 + (v.y - r * 0.1) ** 2) / (r * r * 0.18)) if v.z > t * 0.5 else 0)))
        K.rot(c, "Z", rz)
        K.rot(c, "X", tx)
        K.rot(c, "Y", ty)
        K.move(c, x, y, z)
        K.mesh_obj("coin", c, mat, smooth_angle=50)

    z = 0.0
    for i in range(n):
        coin(rnd.uniform(-r * 0.12, r * 0.12), rnd.uniform(-r * 0.12, r * 0.12), z, rnd.uniform(-2, 2), rnd.uniform(-2, 2), rnd.uniform(0, 360))
        z += t * 1.02
    for k, (dx, dy) in enumerate(((1.8, -0.6), (-1.6, -0.9), (0.3, -1.85))):
        coin(dx * r, dy * r, 0.0, 0, 0, rnd.uniform(0, 360))
    coin(-1.1 * r, 1.1 * r, t * 2.5, 25, -10, 20)


def amphora(dipinto=(150, 40, 30), stopper="resin"):
    """Амфора LRA 1 (Киликия, Кипр — «винная» и «масляная» тара VI в.): рифлёное тело,
    две ручки, короткая ножка."""
    prof = [(0.0, 0.0001), (0.0, 0.016), (0.02, 0.022), (0.06, 0.07), (0.12, 0.12), (0.22, 0.13), (0.32, 0.125), (0.4, 0.105), (0.45, 0.07), (0.48, 0.04), (0.53, 0.032), (0.545, 0.038), (0.55, 0.034)]
    body = K.lathe(prof, 48, cap_top=False)
    # Рифление тела — признак LRA 1.
    K.deform(body, lambda v: (v.x * (1 + 0.012 * max(0, math.sin(v.z * 220)) * (1 if 0.08 < v.z < 0.42 else 0)), v.y * (1 + 0.012 * max(0, math.sin(v.z * 220)) * (1 if 0.08 < v.z < 0.42 else 0)), v.z))
    K.mesh_obj("amphora", body, "amphora_clay", solidify=0.006, subsurf=1, smooth_angle=80)
    for s in (-1, 1):
        hp = K.bezier((s * 0.035, 0, 0.51), (s * 0.08, 0, 0.53), (s * 0.1, 0, 0.47), (s * 0.095, 0, 0.42), n=14)
        K.mesh_obj("handle", K.loft(hp, [(0.012, 0.008)] * len(hp), 10, up=(0, 1, 0)), "amphora_clay", subsurf=1)
    K.mesh_obj("stopper", K.lathe([(0.53, 0.031), (0.552, 0.036), (0.56, 0.0001)], 24), stopper, subsurf=1)


# =========================================================================== монеты и товары


@item("tremissis", (1, 1), name="Тремисс", pose="upright", azimuth=24, elevation=32, margin=0.88)
def tremissis():
    M()
    coin_pile("gold", 0.0075, 0.001, 5, seed=7)


@item("follis", (1, 1), name="Фоллис", pose="upright", azimuth=24, elevation=32, margin=0.88)
def follis():
    M()
    K.metal_mat("follis_bronze", (176, 120, 70), (110, 70, 40), rough=(0.4, 0.6), dirt=0.9, tarnish=((70, 96, 74), 0.9))
    coin_pile("follis_bronze", 0.0145, 0.0022, 5, relief="M", seed=8)


@item("decanummium", (1, 1), name="Декануммий", pose="upright", azimuth=24, elevation=32, margin=0.88)
def decanummium():
    M()
    K.metal_mat("small_bronze", (170, 118, 72), (100, 66, 40), rough=(0.45, 0.65), dirt=0.9, tarnish=((70, 96, 74), 0.9))
    coin_pile("small_bronze", 0.009, 0.0014, 6, seed=9)


@item("amphora_wine", (2, 3), name="Амфора вина", pose="flat", azimuth=20, elevation=8, margin=0.92)
def amphora_wine():
    M()
    ceramic("amphora_clay", (196, 150, 112), (150, 104, 72))
    resin("resin", (60, 30, 18), 0.1)
    amphora(stopper="resin")


@item("amphora_oil", (2, 3), name="Амфора оливкового масла", pose="flat", azimuth=20, elevation=8, margin=0.92)
def amphora_oil():
    M()
    ceramic("amphora_clay", (206, 168, 130), (160, 120, 86))
    ceramic("plug", (150, 104, 70))
    amphora(stopper="plug")


@item("glass_bowl", (1, 1), name="Стеклянная чаша", pose="upright", azimuth=20, elevation=26, margin=0.86)
def glass_bowl():
    M()
    K.glass_mat("glass", (190, 222, 200))
    prof = [(0.0, 0.0001), (0.0, 0.025), (0.004, 0.03), (0.02, 0.055), (0.045, 0.068), (0.05, 0.07)]
    K.mesh_obj("bowl", K.lathe(prof, 48, cap_top=False), "glass", solidify=0.0022, subsurf=1, glass=True, smooth_angle=89)
    K.mesh_obj("rim", K.torus((0, 0, 0.05), 0.07, 0.0022), "glass", glass=True)


def folded_cloth(mat, w=0.22, d=0.16, layers=5, t=0.012):
    for i in range(layers):
        b = K.cbox(0, 0, t * (i + 0.5), w - i * 0.004, d - i * 0.002, t * 0.95)
        K.deform(b, lambda v, i=i: (v.x, v.y, v.z + 0.002 * math.sin(v.x * 40 + i)))
        K.mesh_obj("fold", b, mat, bevel=t * 0.45, subsurf=1, smooth_angle=80)
    # Свисающий край.
    rows = []
    for k in range(9):
        z = t * layers - k * 0.008
        row = [(x, d / 2 + 0.004 + 0.012 * math.sin(k / 8 * math.pi / 2), z) for x in [(-w / 2 + 0.01) + (w - 0.02) * j / 8 for j in range(9)]]
        rows.append(row)
    K.mesh_obj("flap", K.grid_surface(rows), mat, solidify=0.003, subsurf=1)


@item("linen_bolt", (2, 2), name="Отрез льна", pose="upright", azimuth=24, elevation=30, margin=0.9)
def linen_bolt():
    M()
    K.cloth_mat("raw_linen", (214, 202, 176), (176, 162, 132), weave=700.0)
    folded_cloth("raw_linen")


@item("wool_bolt", (2, 2), name="Отрез шерсти", pose="upright", azimuth=24, elevation=30, margin=0.9)
def wool_bolt():
    M()
    K.cloth_mat("dyed_wool", (128, 46, 34), (86, 28, 22), weave=420.0, bump=0.5)
    folded_cloth("dyed_wool", t=0.014)


@item("salted_fish", (1, 2), name="Солёная рыба", pose="flat", azimuth=14, elevation=8, margin=0.92)
def salted_fish():
    """Две вяленые рыбы (тарань/кефаль), связанные бечёвкой за хвосты."""
    M()
    K.organic_mat("fish_skin", (150, 130, 98), (84, 68, 46), rough=(0.35, 0.6), bump=0.5, bump_scale=420.0, dirt=0.6)
    K.organic_mat("fish_fin", (120, 92, 58), (70, 50, 30), rough=(0.5, 0.7), bump=0.3, dirt=0.3)
    K.organic_mat("fish_eye", (30, 24, 18), (16, 12, 10), rough=(0.2, 0.3), bump=0.0)
    L = 0.2
    for s, dz in ((-1, 0.0), (1, -0.012)):
        # Тело: веретено, сплюснутое с боков (высота — вдоль X, толщина — вдоль Y).
        path, radii = [], []
        for i in range(25):
            t = i / 24
            h = 0.03 * math.sin(math.pi * min(1.0, 0.1 + t * 0.95)) ** 0.8 * (1 - 0.7 * t**2) + 0.004
            path.append((0.004 * math.sin(t * 3.0), 0, dz + L * (1 - t)))
            radii.append((h * 0.32, h))
        body = K.loft(path, radii, 18)
        K.move(body, s * 0.024, 0, 0)
        K.mesh_obj("fish", body, "fish_skin", subsurf=1, smooth_angle=80)
        # Хвост: две лопасти.
        for side in (-1, 1):
            fin = K.blade([(0, 0.004, 0.0012), (0.018, 0.009, 0.0008), (0.03, 0.006, 0.0005)], 6)
            K.rot(fin, "Y", 180 + side * 28)
            K.move(fin, s * 0.024, 0, dz + 0.004)
            K.mesh_obj("tail", fin, "fish_fin")
        # Спинной плавник и глаз.
        dorsal = K.blade([(0, 0.012, 0.0008), (0.03, 0.006, 0.0006), (0.04, 0.001, 0.0004)], 6)
        K.move(dorsal, s * 0.024 + 0.028, 0, dz + L * 0.4)
        K.mesh_obj("dorsal", dorsal, "fish_fin")
        for y in (-1, 1):
            eye = K.lathe([(0, 0.0034), (0.0012, 0.0024), (0.0016, 0.0001)], 12)
            K.rot(eye, "X", 90 * y)
            K.move(eye, s * 0.024 + 0.006, y * 0.0062, dz + L * 0.93)
            K.mesh_obj("eye", eye, "fish_eye")
    K.mesh_obj("cord", K.torus((0, 0, 0.012), 0.032, 0.0016, ry=0.008), "cord")


@item("salt_sack", (2, 2), name="Мешок соли", pose="upright", azimuth=24, elevation=24, margin=0.9)
def salt_sack():
    M()
    K.cloth_mat("sacking", (178, 156, 118), (130, 110, 78), weave=300.0, bump=0.7)
    substance("salt", (236, 232, 224), (200, 196, 190), rough=(0.6, 0.85), bump=0.8)
    rings = [(0.0, 0.0001), (0.006, 0.09), (0.05, 0.115), (0.13, 0.11), (0.17, 0.095), (0.185, 0.1), (0.2, 0.11)]
    b = K.lathe(rings, 48, cap_top=False)
    K.deform(b, lambda v: (v.x * (1 + 0.05 * math.sin(math.atan2(v.y, v.x) * 7 + v.z * 30) * min(1, v.z / 0.1)), v.y * (1 + 0.05 * math.sin(math.atan2(v.y, v.x) * 7 + v.z * 30) * min(1, v.z / 0.1)), v.z))
    K.mesh_obj("sack", b, "sacking", solidify=0.003, subsurf=1, smooth_angle=89)
    powder_mound(0.1, 0.03, "salt", z=0.17)
    # Отвёрнутый край мешка.
    K.mesh_obj("rim", K.torus((0, 0, 0.198), 0.11, 0.008, ry=0.11), "sacking")


def hide_sheet(mat, w=0.3, d=0.22, fold=True, fur=None, seed=4):
    r = random.Random(seed)
    rows = []
    n = 18
    ph = [r.uniform(0, 6) for _ in range(4)]
    for i in range(n + 1):
        row = []
        for j in range(n + 1):
            u, v = i / n - 0.5, j / n - 0.5
            # Неровный контур шкуры.
            edge = 1 - 0.12 * math.sin(u * 9 + ph[0]) * math.sin(v * 7 + ph[1])
            x, y = u * w * edge, v * d * edge
            z = 0.006 * math.sin(u * 14 + ph[2]) * math.sin(v * 11 + ph[3])
            if fold and u > 0.1:
                # Сложена пополам: правая часть лежит сверху.
                x = 0.1 * w - (x - 0.1 * w) * 0.95
                z += 0.014
            row.append((x, y, z + 0.004))
        rows.append(row)
    K.mesh_obj("hide", K.grid_surface(rows), mat, solidify=0.004, subsurf=1, smooth_angle=80)
    if fur:
        # Руно — плотные завитки шерсти на верхней стороне.
        parts = []
        for k in range(160):
            x = r.uniform(-0.4, 0.1) * w
            y = r.uniform(-0.45, 0.45) * d
            s = r.uniform(0.008, 0.013)
            parts.append(lump((x, y, 0.024 + r.uniform(0, 0.006)), s, rough=0.35, squash=0.6, seed=k))
        K.mesh_obj("fleece", K.merge(*parts), fur, subsurf=1, smooth_angle=80)


@item("raw_hide", (2, 2), name="Сырая шкура", pose="upright", azimuth=20, elevation=40, margin=0.9)
def raw_hide():
    M()
    K.organic_mat("hide", (150, 112, 78), (96, 68, 44), rough=(0.6, 0.85), bump=0.5, cells=180.0, dirt=0.6)
    hide_sheet("hide")


@item("sheepskin", (2, 2), name="Овчина", pose="upright", azimuth=20, elevation=40, margin=0.9)
def sheepskin():
    M()
    K.organic_mat("hide_in", (196, 170, 130), (150, 124, 90), rough=(0.6, 0.85), bump=0.4, dirt=0.5)
    K.organic_mat("wool", (222, 210, 184), (170, 156, 128), rough=(0.85, 1.0), bump=0.8, bump_scale=300.0, dirt=0.8, sheen=0.6)
    hide_sheet("hide_in", fold=False, fur="wool")


@item("felt", (2, 2), name="Войлок", pose="upright", azimuth=24, elevation=30, margin=0.9)
def felt():
    """Скатка войлока, перевязанная ремешком."""
    M()
    K.organic_mat("felt_mat", (150, 136, 112), (104, 92, 72), rough=(0.85, 1.0), bump=0.6, bump_scale=260.0, dirt=0.5, sheen=0.5)
    K.organic_mat("felt_end", (128, 114, 92), (90, 78, 60), rough=(0.85, 1.0), bump=0.6, bump_scale=260.0, dirt=0.6)
    outer = K.lathe([(-0.13, 0.05), (-0.128, 0.054), (0.128, 0.054), (0.13, 0.05)], 40, axis="X")
    K.move(outer, 0, 0, 0.054)
    K.mesh_obj("roll", outer, "felt_mat", subsurf=1, smooth_angle=80)
    # Слои скатки видны на торцах тёмными кольцами.
    for x in (-0.1302, 0.1302):
        for i in range(6):
            rr = 0.01 + i * 0.0075
            K.mesh_obj("layer", K.torus((x, 0, 0.054), rr, 0.0011, plane="YZ", tube_ry=0.0011), "felt_end")
    for x in (-0.07, 0.07):
        t = K.torus((x, 0, 0.054), 0.056, 0.002, plane="YZ", tube_ry=0.007)
        K.mesh_obj("strap", t, "leather")


@item("beeswax", (1, 1), name="Воск", pose="upright", azimuth=24, elevation=32, margin=0.86)
def beeswax():
    M()
    K.organic_mat("wax_y", (214, 168, 72), (160, 112, 40), rough=(0.35, 0.55), bump=0.3, bump_scale=60.0, dirt=0.4)
    b = K.lathe([(0.0, 0.0001), (0.0, 0.042), (0.004, 0.046), (0.024, 0.043), (0.028, 0.038), (0.029, 0.0001)], 40)
    K.deform(b, lambda v: (v.x, v.y, v.z + 0.0012 * math.sin(v.x * 120) * math.sin(v.y * 90) * (v.z / 0.03)))
    K.mesh_obj("wax", b, "wax_y", subsurf=1, smooth_angle=70)


@item("honey_pot", (1, 1), name="Горшок мёда", pose="upright", azimuth=24, elevation=20, margin=0.86)
def honey_pot():
    M()
    ceramic("clay", (176, 110, 72))
    K.liquid_mat("honey", (210, 130, 30), rough=0.1)
    top = pot("clay", 0.045, 0.07, lid="cloth", neck=0.72)
    # Потёк мёда по боку.
    path = [(0.03, -0.012, top - 0.004), (0.036, -0.016, top - 0.015), (0.044, -0.018, top - 0.03), (0.046, -0.019, top - 0.04)]
    K.mesh_obj("drip", K.loft(path, [0.003, 0.0028, 0.0025, 0.003], 10), "honey", subsurf=1)


# =========================================================================== травы


def herb_mats(stem=(96, 104, 58), leaf=(98, 120, 70), flower=None):
    M()
    leaf_mat("stem", stem)
    leaf_mat("herb", leaf)
    if flower:
        K.organic_mat("flower", flower, tuple(int(c * 0.65) for c in flower), rough=(0.5, 0.75), bump=0.2, dirt=0.2)


HERBS = [
    # key, name, leaf, flower, shape, flower_kind, kwargs
    ("sage", "Шалфей", (128, 140, 112), (120, 96, 160), "lance", "spike", dict(leaf_size=0.022)),
    ("thyme", "Чабрец", (92, 108, 66), (170, 110, 170), "small", "dots", dict(leaf_size=0.012, n=9, leaves_per=9)),
    ("st_johns_wort", "Зверобой", (90, 112, 58), (226, 186, 40), "lance", "dots", dict(leaf_size=0.012, flower_size=0.005)),
    ("henbane", "Белена", (110, 128, 84), (200, 186, 120), "round", "dots", dict(leaf_size=0.022, flower_size=0.006)),
    ("wormwood", "Полынь", (150, 164, 146), (190, 184, 120), "feather", "dots", dict(leaf_size=0.02, flower_size=0.0025)),
    ("yarrow", "Тысячелистник", (88, 112, 66), (226, 222, 206), "feather", "umbel", dict(leaf_size=0.02, flower_size=0.005)),
    ("plantain", "Подорожник", (76, 110, 54), (120, 112, 76), "round", "spike", dict(leaf_size=0.03, n=5, leaves_per=3, flower_size=0.003)),
    ("mallow", "Просвирник", (84, 116, 60), (190, 120, 170), "round", "dots", dict(leaf_size=0.02, flower_size=0.006)),
    ("poppy", "Маковые коробочки", (120, 128, 92), (150, 150, 110), "lance", "capsule", dict(leaf_size=0.012, flower_size=0.012, n=5, leaves_per=2)),
    ("mint", "Мята", (70, 110, 56), (180, 150, 200), "lance", "spike", dict(leaf_size=0.018)),
    ("woad", "Вайда", (84, 120, 96), (220, 200, 60), "lance", "dots", dict(leaf_size=0.03, leaves_per=4, flower_size=0.0035)),
]


def _herb(key, name, leaf, flower, shape, kind, kw):
    @item(key, (1, 1), name=name, pose="flat", azimuth=14, elevation=10, margin=0.9, roll=-28)
    def build():
        herb_mats(leaf=leaf, flower=flower)
        herb_bundle("herb", flower=flower, leaf_shape=shape, flower_kind=kind, seed=_seed(key), **kw)

    return build


for _h in HERBS:
    _herb(*_h)


@item("juniper", (1, 1), name="Ягоды можжевельника", pose="upright", azimuth=20, elevation=34, margin=0.86)
def juniper():
    M()
    ceramic("clay", (176, 110, 72))
    substance("berry", (56, 62, 96), (28, 30, 50), rough=(0.3, 0.5), bump=0.1)
    d = dish()
    heap("berries", "berry", base_z=0.006, spread=0.032, count=28, size=0.0045, shape="sphere", height=0.012)


@item("rosehip", (1, 1), name="Шиповник", pose="upright", azimuth=20, elevation=34, margin=0.86)
def rosehip():
    M()
    ceramic("clay", (176, 110, 72))
    substance("hip", (170, 52, 28), (110, 30, 18), rough=(0.3, 0.5), bump=0.2)
    dish()
    heap("hips", "hip", base_z=0.006, spread=0.03, count=16, size=0.007, shape="oval", height=0.014)


@item("rhubarb", (1, 1), name="Ревень понтийский", pose="upright", azimuth=20, elevation=34, margin=0.86)
def rhubarb():
    """Сушёный корень «ра понтийского» — куски корня на мешковине."""
    M()
    K.cloth_mat("sacking", (178, 156, 118), (130, 110, 78), weave=300.0, bump=0.7)
    substance("rhubarb_root", (196, 140, 62), (130, 78, 36), rough=(0.6, 0.85), bump=0.9)
    cloth_square(0.12, "sacking")
    heap("chunks", "rhubarb_root", base_z=0.004, spread=0.036, count=7, size=0.011, shape="oval", height=0.008, seed=7)


@item("oak_galls", (1, 1), name="Дубовые галлы", pose="upright", azimuth=20, elevation=34, margin=0.86)
def oak_galls():
    M()
    K.cloth_mat("sacking", (178, 156, 118), (130, 110, 78), weave=300.0, bump=0.7)
    substance("gall", (128, 96, 62), (80, 56, 34), rough=(0.5, 0.7), bump=0.8)
    cloth_square(0.12, "sacking")
    heap("galls", "gall", base_z=0.004, spread=0.03, count=9, size=0.009, shape="lump", height=0.01)


ROOTS = [
    ("madder", "Корень марены", (150, 56, 40), dict(n=8, thick=0.003, length=0.14, gnarl=0.6)),
    ("comfrey", "Корень окопника", (60, 44, 34), dict(n=4, thick=0.009, length=0.12, gnarl=0.3)),
    ("elecampane", "Корень девясила", (150, 116, 80), dict(n=4, thick=0.011, length=0.1, gnarl=0.25)),
    ("liquorice", "Солодковый корень", (160, 116, 66), dict(n=7, thick=0.005, length=0.15, gnarl=0.1)),
    ]


def _root(key, name, color, kw):
    @item(key, (1, 1), name=name, pose="flat", azimuth=14, elevation=10, margin=0.9, roll=-30)
    def build():
        M()
        substance("root", color, rough=(0.6, 0.85), bump=0.6)
        roots_bundle("root", seed=_seed(key), **kw)

    return build


for _r in ROOTS:
    _root(*_r)


@item("mutton_fat", (1, 1), name="Бараний жир", pose="upright", azimuth=24, elevation=24, margin=0.86)
def mutton_fat():
    M()
    ceramic("clay", (160, 100, 66))
    substance("fat", (236, 228, 206), (200, 190, 166), rough=(0.4, 0.6), bump=0.3)
    pot("clay", 0.045, 0.06, filling="fat", neck=0.85)


# =========================================================================== привозное и материалы

HEAPS = [
    # key, name, color, shape, mat_kind, on ("dish"/"cloth"), kwargs
    ("myrrh", "Мирра", (130, 50, 24), "lump", "resin", "cloth", dict(count=12, size=0.008)),
    ("frankincense", "Ладан", (222, 196, 140), "lump", "resin", "dish", dict(count=16, size=0.006)),
    ("mastic", "Хиосская мастика", (236, 226, 180), "oval", "resin", "dish", dict(count=20, size=0.004)),
    ("gum", "Камедь", (204, 140, 60), "lump", "resin", "dish", dict(count=12, size=0.007)),
    ("pine_resin", "Сосновая смола", (176, 104, 30), "lump", "resin", "cloth", dict(count=7, size=0.011)),
    ("sulfur", "Сера", (226, 200, 56), "lump", "solid", "cloth", dict(count=8, size=0.01)),
    ("alum", "Квасцы", (236, 236, 232), "crystal", "resin", "dish", dict(count=16, size=0.006)),
    ("cinnabar", "Киноварь", (170, 34, 24), "lump", "solid", "cloth", dict(count=6, size=0.011)),
    ("pepper", "Перец", (40, 32, 26), "sphere", "solid", "dish", dict(count=40, size=0.003, spread=0.03)),
    ("natron", "Натр", (226, 220, 200), "lump", "solid", "cloth", dict(count=8, size=0.01)),
    ("quicklime", "Негашёная известь", (232, 230, 222), "lump", "solid", "cloth", dict(count=5, size=0.014)),
    ("blue_vitriol", "Медный купорос", (40, 110, 190), "crystal", "resin", "dish", dict(count=14, size=0.007)),
]


def _heap(key, name, color, shape, kind, on, kw):
    @item(key, (1, 1), name=name, pose="upright", azimuth=20, elevation=34, margin=0.86)
    def build():
        M()
        ceramic("clay", (176, 110, 72))
        if kind == "resin":
            resin("stuff", color, 0.5 if shape != "crystal" else 0.7)
        else:
            substance("stuff", color, bump=0.7)
        if on == "dish":
            dish()
            base = 0.006
        else:
            cloth_square(0.12, "linen")
            base = 0.004
        heap("heap", "stuff", base_z=base, shape=shape, seed=_seed(key), height=0.012, **kw)

    return build


for _h in HEAPS:
    _heap(*_h)

POWDERS = [
    ("verdigris", "Ярь-медянка", (64, 150, 120)),
    ("white_lead", "Свинцовые белила", (238, 236, 228)),
    ("minium", "Сурик", (214, 82, 34)),
    ("madder_lake", "Мареновый лак", (170, 30, 50)),
    ("woad_blue", "Вайдовая синь", (46, 70, 140)),
    ("wood_ash", "Древесная зола", (130, 126, 120)),
    ("saffron", "Шафран", (196, 60, 24)),
]


def _powder(key, name, color):
    @item(key, (1, 1), name=name, pose="upright", azimuth=20, elevation=34, margin=0.86)
    def build():
        M()
        ceramic("clay", (176, 110, 72))
        substance("powder_mat", color, rough=(0.8, 0.95), bump=0.3)
        dish()
        if key == "saffron":
            # Рыльца шафрана — тонкие нити.
            r = random.Random(5)
            parts = []
            for i in range(40):
                a = r.uniform(0, 6.28)
                d = r.uniform(0, 0.026)
                c = Vector((d * math.cos(a), d * math.sin(a), 0.008 + r.uniform(0, 0.006)))
                b = r.uniform(0, 6.28)
                path = [c + Vector((math.cos(b), math.sin(b), 0)) * (0.012 * t) + Vector((0, 0, 0.002 * math.sin(t * 3))) for t in (0, 0.5, 1)]
                parts.append(K.loft(path, [0.0007, 0.0009, 0.0012], 5))
            K.mesh_obj("threads", K.merge(*parts), "powder_mat")
        else:
            powder_mound(0.032, 0.016, "powder_mat", z=0.005)

    return build


for _p in POWDERS:
    _powder(*_p)


@item("opium", (1, 1), name="Опий", pose="upright", azimuth=24, elevation=30, margin=0.84)
def opium():
    """Лепёшка опия, завёрнутая в лист."""
    M()
    leaf_mat("wrap_leaf", (112, 116, 70))
    substance("opium_mat", (70, 44, 28), (40, 24, 14), rough=(0.3, 0.5), bump=0.3)
    b = lump((0, 0, 0.012), 0.022, rough=0.12, squash=0.5, seed=4)
    K.mesh_obj("cake", b, "opium_mat", subsurf=1)
    for a in (0, 120, 240):
        lf = K.lathe([(0, 0.0001), (0.012, 0.012), (0.03, 0.0001)], 8)
        K.scale(lf, 1, 0.15, 1)
        K.rot(lf, "Y", 95)
        K.rot(lf, "Z", a)
        K.move(lf, 0, 0, 0.004)
        K.mesh_obj("leaf", lf, "wrap_leaf", solidify=0.0006)


@item("cinnamon", (1, 1), name="Корица", pose="flat", azimuth=14, elevation=10, margin=0.88, roll=-30)
def cinnamon():
    """Свёрнутые трубочки коры (кассия), связанные лыком."""
    M()
    substance("bark", (150, 84, 46), (100, 54, 28), rough=(0.6, 0.8), bump=0.6)
    r = random.Random(6)
    for i in range(6):
        x = (i % 3 - 1) * 0.011
        y = (i // 3 - 0.5) * 0.011
        L = 0.12 * r.uniform(0.9, 1.05)
        q = K.partial_lathe([(0, 0.0055, 0.0055), (L, 0.0055, 0.0055)], 0, 300, 16)
        K.rot(q, "Z", r.uniform(0, 360))
        K.move(q, x, y, -L / 2 + r.uniform(-0.006, 0.006))
        K.mesh_obj("quill", q, "bark", solidify=0.0012, subsurf=1)
    K.mesh_obj("tie", K.torus((0, 0, 0.0), 0.02, 0.0016, ry=0.014), "cord")


@item("naphtha", (1, 2), name="Нефть", pose="upright", azimuth=20, elevation=10, margin=0.9)
def naphtha():
    M()
    K.glass_mat("glass", (150, 170, 150))
    K.liquid_mat("naphtha_l", (20, 14, 8), rough=0.05)
    K.organic_mat("wax", (90, 30, 24), (60, 20, 16), rough=(0.3, 0.5), bump=0.3, bump_scale=60.0, dirt=0.3)
    flask_body("tall", "naphtha_l", seal="wax", liquid_level=0.6)


@item("lead_ingot", (1, 1), name="Свинец", pose="upright", azimuth=26, elevation=30, margin=0.84)
def lead_ingot():
    M()
    K.metal_mat("lead", (118, 120, 126), (80, 82, 88), rough=(0.45, 0.7), dirt=0.8, tarnish=((150, 150, 146), 0.6))
    b = K.box(-0.05, 0.05, -0.022, 0.022, 0.0, 0.022)
    K.deform(b, lambda v: (v.x * (0.82 if v.z > 0.01 else 1), v.y * (0.75 if v.z > 0.01 else 1), v.z))
    K.mesh_obj("ingot", b, "lead", bevel=0.004, subsurf=1, smooth_angle=50)


@item("copper_scrap", (1, 1), name="Медный лом", pose="upright", azimuth=26, elevation=30, margin=0.86)
def copper_scrap():
    """Обрезки листовой меди и гнутая проволока."""
    M()
    K.metal_mat("copper", (196, 118, 76), (120, 66, 40), rough=(0.3, 0.55), dirt=0.8, tarnish=((60, 120, 100), 0.6))
    r = random.Random(11)
    for i in range(6):
        rows = []
        w, d = r.uniform(0.03, 0.05), r.uniform(0.02, 0.035)
        bend = r.uniform(-0.6, 0.6)
        for a in range(5):
            row = []
            for b in range(5):
                x = (a / 4 - 0.5) * w
                y = (b / 4 - 0.5) * d
                row.append((x, y, bend * x * x * 8 + 0.002))
            rows.append(row)
        s = K.grid_surface(rows)
        K.rot(s, "X", r.uniform(-25, 25))
        K.rot(s, "Z", r.uniform(0, 360))
        K.move(s, r.uniform(-0.02, 0.02), r.uniform(-0.02, 0.02), 0.004 * i)
        K.mesh_obj("sheet", s, "copper", solidify=0.0012)
    path = [(0.03 * math.cos(t * 0.8) - 0.01, 0.025 * math.sin(t * 0.8), 0.006 + 0.002 * t) for t in range(10)]
    K.mesh_obj("wire", K.loft(path, [0.0014] * len(path), 6), "copper", subsurf=1)


@item("wine_jug", (1, 2), name="Кувшин вина", pose="upright", azimuth=24, elevation=10, margin=0.9)
def wine_jug():
    M()
    ceramic("jug_clay", (168, 92, 60))
    jug("jug_clay", seal="cork")


@item("oil_jug", (1, 2), name="Кувшин оливкового масла", pose="upright", azimuth=24, elevation=10, margin=0.9)
def oil_jug():
    M()
    ceramic("jug_clay", (200, 160, 118))
    jug("jug_clay", seal="cloth", seal_mat="linen")


# =========================================================================== посуда и приборы


@item("mortar", (2, 2), name="Ступка с пестиком", pose="upright", azimuth=24, elevation=22, margin=0.9)
def mortar():
    """Мраморная ступка с пестиком."""
    M()
    K.organic_mat("marble", (226, 220, 208), (180, 172, 160), rough=(0.3, 0.5), bump=0.15, dirt=0.6, wear=0.3)
    prof = [(0.0, 0.0001), (0.0, 0.05), (0.01, 0.055), (0.05, 0.065), (0.07, 0.068), (0.075, 0.062), (0.06, 0.05), (0.03, 0.035), (0.022, 0.0001)]
    K.mesh_obj("mortar", K.lathe(prof, 48), "marble", subsurf=1, smooth_angle=60)
    for a in (0, 90, 180, 270):
        lug = K.cbox(0.068, 0, 0.055, 0.014, 0.018, 0.02)
        K.rot(lug, "Z", a + 45)
        K.mesh_obj("lug", lug, "marble", bevel=0.004, subsurf=1)
    p = K.lathe([(0.0, 0.0001), (0.004, 0.012), (0.02, 0.014), (0.05, 0.009), (0.11, 0.0085), (0.12, 0.0001)], 24)
    K.rot(p, "Y", 32)
    K.move(p, -0.02, 0, 0.03)
    K.mesh_obj("pestle", p, "marble", subsurf=1)


@item("clay_pot", (2, 2), name="Глиняный горшок", pose="upright", azimuth=24, elevation=20, margin=0.9)
def clay_pot():
    M()
    ceramic("clay", (156, 92, 60))
    pot("clay", 0.08, 0.12, lid="lid", neck=0.78, handles=True)


@item("copper_cauldron", (2, 2), name="Медный котелок", pose="upright", azimuth=24, elevation=24, margin=0.9)
def copper_cauldron():
    M()
    K.metal_mat("copper", (200, 120, 76), (120, 64, 38), rough=(0.25, 0.5), dirt=0.9, tarnish=((60, 120, 100), 0.5))
    prof = [(0.0, 0.0001), (0.0, 0.05), (0.01, 0.075), (0.04, 0.095), (0.08, 0.098), (0.1, 0.095), (0.106, 0.1)]
    K.mesh_obj("pot", K.lathe(prof, 48, cap_top=False), "copper", solidify=0.0025, subsurf=1, smooth_angle=70)
    hp = [(0.098 * math.cos(a), 0, 0.106 + 0.11 * math.sin(a)) for a in [math.pi * i / 20 for i in range(21)]]
    K.mesh_obj("bail", K.loft(hp, [0.0025] * len(hp), 8, up=(0, 1, 0)), "iron", subsurf=1)
    for s in (-1, 1):
        K.mesh_obj("lug", K.torus((s * 0.1, 0, 0.103), 0.008, 0.0025, plane="XZ"), "iron")
    # Заклёпки по шву.
    for i in range(16):
        a = 2 * math.pi * i / 16
        rv = K.lathe([(0, 0.003), (0.002, 0.002), (0.0025, 0.0001)], 8)
        K.rot(rv, "Y", 90)
        K.move(rv, 0.0955, 0, 0.06)
        K.rot(rv, "Z", math.degrees(a))
        K.mesh_obj("rivet", rv, "copper", subsurf=1)


@item("scales", (2, 2), name="Весы с гирями", pose="upright", azimuth=10, elevation=12, margin=0.9)
def scales():
    """Бронзовые равноплечие весы с чашами на цепочках и гирьки-«экзагии»."""
    M()
    K.mesh_obj("beam", K.lathe([(-0.12, 0.002), (-0.115, 0.003), (0.0, 0.0045), (0.115, 0.003), (0.12, 0.002)], 12, axis="X"), "bronze", subsurf=1)
    K.mesh_obj("pivot", K.cbox(0, 0, 0.012, 0.006, 0.004, 0.024), "bronze", bevel=0.001)
    K.mesh_obj("ring", K.torus((0, 0, 0.03), 0.007, 0.0016, plane="XZ"), "bronze")
    for s in (-1, 1):
        x = s * 0.115
        for a in (0, 120, 240):
            ax, ay = 0.03 * math.cos(math.radians(a)), 0.03 * math.sin(math.radians(a))
            K.mesh_obj("chain", K.loft([(x, 0, 0), (x + ax, ay, -0.085)], [0.0009, 0.0009], 6), "bronze")
        K.mesh_obj("pan", K.lathe([(-0.095, 0.0001), (-0.094, 0.018), (-0.09, 0.03), (-0.086, 0.033)], 32, cap_top=False), "bronze", solidify=0.0015, subsurf=1)
        for o in K.PARTS[-1:]:
            o.data.transform(K.Matrix.Translation((x, 0, 0)))
    # Гирьки: бронзовые квадратные экзагии с крестом.
    for i, (x, s) in enumerate(((-0.05, 0.011), (-0.028, 0.009), (-0.01, 0.007), (0.006, 0.006))):
        g = K.cbox(x, -0.04, -0.09 + s * 0.2, s * 1.6, s * 1.6, s * 0.4)
        K.mesh_obj("weight", g, "bronze", bevel=0.0015, subsurf=1)


@item("empty_vial", (1, 1), name="Пустой флакон", pose="upright", azimuth=20, elevation=14, margin=0.88)
def empty_vial():
    M()
    K.glass_mat("glass", (196, 226, 206))
    flask_body("small", None, seal="cork")


@item("crucible", (1, 1), name="Тигель", pose="upright", azimuth=24, elevation=24, margin=0.86)
def crucible():
    M()
    ceramic("fireclay", (150, 136, 118), (90, 80, 70))
    prof = [(0.0, 0.0001), (0.0, 0.018), (0.01, 0.024), (0.05, 0.03), (0.062, 0.032), (0.064, 0.034)]
    b = K.lathe(prof, 3 * 12, cap_top=False)
    # Треугольное устье — носики для слива.
    K.deform(b, lambda v: (v.x * (1 + 0.12 * math.cos(3 * math.atan2(v.y, v.x)) * (v.z / 0.064) ** 2), v.y * (1 + 0.12 * math.cos(3 * math.atan2(v.y, v.x)) * (v.z / 0.064) ** 2), v.z))
    K.mesh_obj("crucible", b, "fireclay", solidify=0.004, subsurf=1)
    K.metal_mat("slag", (60, 70, 60), (30, 34, 30), rough=(0.3, 0.5), dirt=0.3)
    K.mesh_obj("slag", K.lathe([(0.004, 0.0001), (0.012, 0.02), (0.014, 0.0001)], 24), "slag", subsurf=1)


def glass_vessel(prof, name="glass"):
    K.mesh_obj(name, K.lathe(prof, 40, cap_top=False), "glass", solidify=0.0018, subsurf=1, glass=True, smooth_angle=89)


@item("alembic", (2, 3), name="Алембик", pose="upright", azimuth=20, elevation=10, margin=0.92)
def alembic():
    """Перегонный куб: глиняная тыква-кукурбита и стеклянный шлем-амбикс с носиком."""
    M()
    K.glass_mat("glass", (196, 226, 206))
    ceramic("clay", (160, 100, 66))
    cuc = [(0.0, 0.0001), (0.0, 0.05), (0.03, 0.08), (0.09, 0.085), (0.14, 0.06), (0.17, 0.04), (0.19, 0.038)]
    K.mesh_obj("cucurbit", K.lathe(cuc, 40, cap_top=False), "clay", solidify=0.004, subsurf=1)
    amb = [(0.18, 0.042), (0.2, 0.05), (0.24, 0.055), (0.28, 0.045), (0.3, 0.02), (0.305, 0.0001)]
    glass_vessel(amb, "ambix")
    sp = K.bezier((0.04, 0, 0.23), (0.1, 0, 0.23), (0.15, 0, 0.18), (0.2, 0, 0.12), n=14)
    K.mesh_obj("spout", K.loft(sp, [0.008 - 0.004 * i / 14 for i in range(15)], 12, cap_start=False, cap_end=False), "glass", solidify=0.0015, glass=True)
    # Приёмник.
    rec = [(0.0, 0.0001), (0.0, 0.025), (0.03, 0.04), (0.06, 0.035), (0.08, 0.015), (0.09, 0.014)]
    rb = K.lathe(rec, 32, cap_top=False)
    K.move(rb, 0.21, 0, 0.0)
    K.mesh_obj("receiver", rb, "glass", solidify=0.0015, subsurf=1, glass=True)


@item("retort", (2, 2), name="Стеклянная реторта", pose="flat", azimuth=20, elevation=10, margin=0.9)
def retort():
    M()
    K.glass_mat("glass", (196, 226, 206))
    body = K.lathe([(0.0, 0.0001), (0.006, 0.035), (0.03, 0.055), (0.06, 0.05), (0.085, 0.03), (0.095, 0.012)], 40, cap_top=False)
    K.mesh_obj("bulb", body, "glass", solidify=0.0016, subsurf=1, glass=True)
    neck = K.bezier((0, 0, 0.09), (0, 0, 0.13), (0.06, 0, 0.13), (0.16, 0, 0.06), n=16)
    K.mesh_obj("neck", K.loft(neck, [0.011 - 0.006 * i / 16 for i in range(17)], 14, cap_start=False, cap_end=False), "glass", solidify=0.0014, glass=True)


@item("kerotakis", (2, 3), name="Керотакис", pose="upright", azimuth=22, elevation=12, margin=0.92)
def kerotakis():
    """Керотакис Зосима: глиняный сосуд над огнём, палитра для металла и колпак сверху."""
    M()
    ceramic("clay", (160, 100, 66))
    K.metal_mat("copper", (200, 120, 76), (120, 64, 38), rough=(0.25, 0.5), dirt=0.9, tarnish=((60, 120, 100), 0.5))
    # Подставка-жаровня.
    for a in (0, 120, 240):
        leg = K.loft([(0.05 * math.cos(math.radians(a)), 0.05 * math.sin(math.radians(a)), 0.0), (0.04 * math.cos(math.radians(a)), 0.04 * math.sin(math.radians(a)), 0.07)], [0.004, 0.004], 8)
        K.mesh_obj("leg", leg, "iron", subsurf=1)
    K.mesh_obj("ring", K.torus((0, 0, 0.07), 0.045, 0.003), "iron")
    vessel = [(0.065, 0.0001), (0.065, 0.03), (0.08, 0.045), (0.14, 0.05), (0.16, 0.045)]
    K.mesh_obj("vessel", K.lathe(vessel, 40, cap_top=False), "clay", solidify=0.004, subsurf=1)
    K.mesh_obj("palette", K.lathe([(0.155, 0.0001), (0.155, 0.044), (0.16, 0.046), (0.162, 0.0001)], 32), "copper", subsurf=1)
    cap = [(0.16, 0.047), (0.2, 0.05), (0.25, 0.04), (0.28, 0.018), (0.29, 0.0001)]
    K.mesh_obj("cap", K.lathe(cap, 40, cap_top=False, cap_bot=False), "clay", solidify=0.004, subsurf=1)


# =========================================================================== тексты


def book_mats():
    M()
    K.organic_mat("parchment", (232, 214, 176), (196, 170, 126), rough=(0.6, 0.8), bump=0.2, dirt=0.6)
    K.organic_mat("papyrus", (214, 190, 140), (168, 140, 92), rough=(0.6, 0.85), bump=0.4, grain="X", grain_scale=10.0, dirt=0.5)


BOOKS = [
    # key, name, cells, kind, cover color, kwargs
    ("dioscorides", "Диоскорид. О лекарственных веществах", (1, 2), "codex", (96, 42, 34), dict(w=0.14, h=0.24, t=0.05)),
    ("herbal", "Иллюстрированный травник", (2, 2), "codex", (62, 72, 48), dict(w=0.22, h=0.26, t=0.06)),
    ("hippocrates", "Гиппократ. О язвах", (1, 2), "scroll", None, dict(length=0.22, tag="tag_red")),
    ("galen_simples", "Гален. О простых лекарствах", (2, 2), "codex", (104, 72, 44), dict(w=0.2, h=0.24, t=0.07)),
    ("galen_antidotes", "Гален. О противоядиях", (2, 2), "codex", (66, 50, 62), dict(w=0.2, h=0.24, t=0.05, clasp="silver")),
    ("oribasius", "Орибасий. Синопсис", (2, 2), "codex", (84, 36, 32), dict(w=0.2, h=0.25, t=0.055, clasp="gilt")),
    ("zosimos", "Зосим. О приборах и печах", (1, 2), "scroll", None, dict(length=0.2, tag="tag_black")),
    ("dyer", "Книга красильщика", (1, 2), "codex", (50, 60, 84), dict(w=0.13, h=0.2, t=0.04, boss=False)),
    ("aeneas", "Эней Тактик. Об обороне городов", (1, 2), "scroll", None, dict(length=0.21, tag="tag_red")),
    ("theophrastus", "Феофраст. О камнях", (1, 2), "scroll", None, dict(length=0.18, tag="tag_black")),
    ("celsus", "Цельс. О медицине", (2, 2), "codex", (112, 88, 60), dict(w=0.2, h=0.24, t=0.06, clasp="iron")),
    # Школьные книги: Псалтирь — азбука церковной школы, Дионисий — грамматика, Никомах — арифметика.
    ("psalter", "Псалтирь", (1, 2), "codex", (92, 30, 52), dict(w=0.12, h=0.19, t=0.045, clasp="gilt", cross=True, seed=3)),
    ("dionysius_thrax", "Дионисий Фракиец. Искусство грамматики", (1, 2), "scroll", None, dict(length=0.19, tag="tag_black")),
    ("nicomachus", "Никомах. Введение в арифметику", (1, 2), "codex", (70, 64, 92), dict(w=0.13, h=0.2, t=0.04, boss=False)),
]


def _book(key, name, cells, kind, cover, kw):
    if kind == "codex":
        opts = dict(pose="flat", azimuth=24, elevation=14, margin=0.9)
    else:
        opts = dict(pose="flat", azimuth=14, elevation=10, margin=0.92)

    @item(key, cells, name=name, **opts)
    def build():
        book_mats()
        if kind == "codex":
            K.organic_mat("cover", cover, tuple(int(c * 0.6) for c in cover), rough=(0.45, 0.7), bump=0.4, cells=220.0, dirt=0.7, wear=0.7, wear_color=tuple(min(255, int(c * 1.5)) for c in cover))
            codex("cover", **kw)
        else:
            K.organic_mat("tag_red", (150, 40, 30), (100, 24, 18), rough=(0.5, 0.7), bump=0.2)
            K.organic_mat("tag_black", (50, 44, 40), (30, 26, 24), rough=(0.5, 0.7), bump=0.2)
            scroll(**kw)
            # Свиток стоит вертикально в иконке (ось Z), на земле лежит.

    return build


for _b in BOOKS:
    _book(*_b)


@item("herbal_scrap", (1, 1), name="Обрывок травника", pose="flat", azimuth=10, elevation=10, margin=0.88)
def herbal_scrap():
    """Обрывок папируса с рисунком растения и строками текста."""
    book_mats()
    m, nb, bsdf = K.new_material("scrap_page")
    vec = nb.coord((1, 1, 1))
    sep = nb.new("ShaderNodeSeparateXYZ")
    nb.link(vec, sep.inputs["Vector"])
    lines = nb.math("LESS_THAN", nb.math("FRACT", nb.math("MULTIPLY", sep.outputs["Z"], 120.0)), 0.35)
    breaks = nb.math("GREATER_THAN", nb.noise(60.0, 2, 0.5, nb.coord((1, 1, 6))), 0.42)
    region = nb.math("GREATER_THAN", sep.outputs["X"], 0.0)
    ink = nb.math("MULTIPLY", nb.math("MULTIPLY", lines, breaks), region)
    # Рисунок растения слева: зелёные пятна.
    plant = nb.math("MULTIPLY", nb.math("LESS_THAN", sep.outputs["X"], -0.005), nb.math("GREATER_THAN", nb.noise(30.0, 4, 0.6, vec), 0.55))
    paper = nb.mix(nb.noise(5.0, 6, 0.6, vec), K.lin((168, 140, 92)), K.lin((220, 196, 146)))
    col = nb.mix(plant, paper, K.lin((70, 96, 50)))
    col = nb.mix(nb.math("MULTIPLY", ink, 0.85), col, K.lin((40, 30, 24)))
    nb.val(bsdf.inputs["Base Color"], col)
    bsdf.inputs["Roughness"].default_value = 0.8
    nb.val(bsdf.inputs["Normal"], nb.bump(nb.noise(3.0, 10, 0.7, nb.coord((8, 1, 120))), 0.3, 0.001))
    rows = []
    r = random.Random(3)
    edge = [r.uniform(0.85, 1.0) for _ in range(13)]
    for i in range(13):
        row = []
        for j in range(13):
            u, v = i / 12 - 0.5, j / 12 - 0.5
            k = edge[(i + j) % 13] if (i in (0, 12) or j in (0, 12)) else 1.0
            row.append((u * 0.09 * k, 0.003 * math.sin(u * 20 + v * 7), v * 0.08 * k))
        rows.append(row)
    K.mesh_obj("scrap", K.grid_surface(rows), "scrap_page", solidify=0.0006, subsurf=1, smooth_angle=80)


# =========================================================================== снадобья

DRINKS = [
    # key, name, liquid color, flask kind, level
    ("sage_decoction", "Отвар шалфея", (120, 110, 50), "round", 0.42),
    ("thyme_decoction", "Отвар чабреца", (140, 96, 40), "round", 0.42),
    ("poppy_decoction", "Маковый отвар", (70, 42, 24), "tall", 0.6),
    ("st_johns_oil", "Зверобойное масло", (150, 20, 16), "tall", 0.6),
    ("oxymel", "Оксимель", (190, 120, 30), "round", 0.45),
    ("vinegar", "Винный уксус", (150, 70, 50), "tall", 0.6),
    ("sulfur_water", "Серная вода", (210, 200, 120), "round", 0.45),
    ("tar_oil", "Смоляное масло", (90, 44, 14), "tall", 0.6),
    ("lye", "Щёлок", (200, 190, 150), "round", 0.45),
]


def _drink(key, name, color, kind, level):
    @item(key, (1, 2), name=name, pose="upright", azimuth=20, elevation=10, margin=0.9)
    def build():
        M()
        K.glass_mat("glass", (200, 226, 206))
        K.liquid_mat("liquid", color)
        K.organic_mat("wax", (120, 34, 26), (80, 20, 16), rough=(0.3, 0.5), bump=0.3, bump_scale=60.0, dirt=0.3)
        flask_body(kind, "liquid", seal="wax", liquid_level=level)

    return build


for _d in DRINKS:
    _drink(*_d)


@item("spoiled_brew", (1, 1), name="Испорченное варево", pose="upright", azimuth=20, elevation=14, margin=0.88)
def spoiled_brew():
    """Неудавшееся варево: мутная бурая жижа в пузырьке, заткнутом пробкой."""
    M()
    K.glass_mat("glass", (176, 190, 160))
    K.liquid_mat("murk", (120, 92, 44))
    flask_body("small", "murk", seal="cork", liquid_level=0.55)


@item("mercury", (1, 1), name="Ртуть", pose="upright", azimuth=20, elevation=14, margin=0.88)
def mercury():
    M()
    K.glass_mat("glass", (196, 226, 206))
    K.metal_mat("quicksilver", (226, 228, 232), (190, 192, 196), rough=(0.02, 0.06), scratches=0.0, dirt=0.0)
    K.organic_mat("wax", (120, 34, 26), (80, 20, 16), rough=(0.3, 0.5), bump=0.3, bump_scale=60.0, dirt=0.3)
    flask_body("small", "quicksilver", seal="wax", liquid_level=0.35)


@item("ink", (1, 1), name="Чернила", pose="upright", azimuth=24, elevation=26, margin=0.86)
def ink():
    """Бронзовая чернильница с крышкой и тростниковое перо (калам)."""
    M()
    K.liquid_mat("ink_l", (16, 12, 12))
    prof = [(0.0, 0.0001), (0.0, 0.024), (0.004, 0.026), (0.03, 0.026), (0.034, 0.02), (0.036, 0.012)]
    K.mesh_obj("inkwell", K.lathe(prof, 40, cap_top=False), "bronze", solidify=0.002, subsurf=1)
    K.mesh_obj("ink", K.lathe([(0.025, 0.0001), (0.028, 0.011)], 24), "ink_l")
    lid = K.lathe([(0.0, 0.0001), (0.0, 0.017), (0.004, 0.016), (0.006, 0.0001)], 24)
    K.rot(lid, "X", 70)
    K.move(lid, 0, 0.02, 0.044)
    K.mesh_obj("lid", lid, "bronze", subsurf=1)
    K.organic_mat("reed", (196, 170, 110), (150, 120, 70), rough=(0.4, 0.6), grain="Z", grain_scale=3.0, bump=0.3)
    pen = K.lathe([(0.0, 0.0001), (0.015, 0.003), (0.15, 0.0035)], 10)
    K.rot(pen, "Y", 28)
    K.move(pen, -0.004, 0, 0.022)
    K.mesh_obj("pen", pen, "reed", subsurf=1)


@item("wax_tablet", (2, 1), name="Восковая табличка", pose="flat", azimuth=22, elevation=34, margin=0.9)
def wax_tablet():
    """Школьная табличка: деревянная рамка, тёмный воск с процарапанными буквами, стиль."""
    M()
    K.organic_mat("wax_dark", (66, 40, 20), (40, 24, 12), rough=(0.25, 0.45), bump=0.25, bump_scale=120.0, dirt=0.3)
    K.organic_mat("wax_scratch", (176, 140, 92), (140, 108, 66), rough=(0.5, 0.7), bump=0.1)
    w, h, t = 0.2, 0.13, 0.012
    # Доска с выбранным углублением: четыре бруска рамки и дно.
    K.mesh_obj("base", K.cbox(0, 0, -t / 2 + 0.002, w, h, 0.004), "wood", bevel=0.0015)
    for x0, x1, y0, y1 in ((-w / 2, w / 2, h / 2 - 0.014, h / 2), (-w / 2, w / 2, -h / 2, -h / 2 + 0.014), (-w / 2, -w / 2 + 0.014, -h / 2, h / 2), (w / 2 - 0.014, w / 2, -h / 2, h / 2)):
        K.mesh_obj("frame", K.box(x0, x1, y0, y1, -t / 2, t / 2), "wood", bevel=0.0018)
    K.mesh_obj("wax", K.cbox(0, 0, 0.0005, w - 0.028, h - 0.028, 0.006), "wax_dark", bevel=0.0008)
    # Процарапанные строки: несколько коротких черт (буквы ученика).
    import random
    rnd = random.Random(7)
    for row in range(3):
        y = 0.03 - row * 0.028
        x = -w / 2 + 0.024
        while x < w / 2 - 0.03:
            n = rnd.randint(1, 3)
            for _ in range(n):
                ln = rnd.uniform(0.004, 0.011)
                bar = K.cbox(0, 0, 0, ln, 0.0012, 0.0006)
                K.rot(bar, "Z", rnd.choice((0, 90, 60, -60, 30)))
                K.move(bar, x + rnd.uniform(0, 0.004), y + rnd.uniform(-0.003, 0.003), 0.0036)
                K.mesh_obj("scratch", bar, "wax_scratch")
            x += rnd.uniform(0.009, 0.013)
    # Отверстия для шнура на левом краю и сам шнур.
    for yy in (-0.03, 0.03):
        K.mesh_obj("tie", K.torus((-w / 2 + 0.007, yy, 0.0), 0.0045, 0.0009, plane="XZ"), "cord")
    # Стиль: острый конец пишет, лопаточка стирает.
    st = K.lathe([(0.0, 0.0001), (0.012, 0.0016), (0.15, 0.0018), (0.156, 0.0012)], 10)
    K.rot(st, "Y", 90)
    K.rot(st, "Z", 14)
    K.move(st, -0.075, -h / 2 - 0.004, t / 2 + 0.002)
    K.mesh_obj("stylus", st, "bronze", subsurf=1)
    sp = K.cbox(0, 0, 0, 0.012, 0.009, 0.0016)
    K.rot(sp, "Z", 14)
    K.move(sp, -0.075 + 0.156 * math.cos(math.radians(14)), -h / 2 - 0.004 + 0.156 * math.sin(math.radians(14)), t / 2 + 0.002)
    K.mesh_obj("spatula", sp, "bronze", bevel=0.0006)


@item("reed_pen", (1, 1), name="Тростниковое перо", pose="flat", azimuth=14, elevation=12, margin=0.88, roll=-34)
def reed_pen():
    """Калам: срезанный наискось тростник с расщепом, кончик в чернилах."""
    M()
    K.organic_mat("reed", (196, 170, 110), (150, 120, 70), rough=(0.4, 0.6), grain="Z", grain_scale=3.0, bump=0.3)
    K.liquid_mat("ink_l", (16, 12, 12))
    body = K.lathe([(-0.08, 0.0035), (0.06, 0.0038), (0.064, 0.0042), (0.068, 0.0038), (0.08, 0.0036)], 14, axis="Z")
    K.mesh_obj("reed", body, "reed", subsurf=1, smooth_angle=60)
    nib = K.lathe([(-0.1, 0.0003), (-0.092, 0.0016), (-0.08, 0.0035)], 14, axis="Z")
    K.scale(nib, 1.0, 0.45, 1.0)
    K.mesh_obj("nib", nib, "reed", subsurf=1)
    tip = K.lathe([(-0.1, 0.0002), (-0.094, 0.0013), (-0.09, 0.0014)], 12, axis="Z")
    K.scale(tip, 1.05, 0.5, 1.0)
    K.mesh_obj("ink", tip, "ink_l", subsurf=1)


@item("parchment_quire", (1, 1), name="Тетрадь пергамента", pose="upright", azimuth=24, elevation=36, margin=0.86)
def parchment_quire():
    """Тетрадь: четыре сложенных листа пергамента, перевязанные бечёвкой."""
    M()
    book_mats()
    w, h = 0.13, 0.17
    for i in range(8):
        sheet = K.cbox(0, 0, 0, w - i * 0.0006, h - i * 0.0004, 0.0011)
        K.rot(sheet, "Z", (i % 3 - 1) * 0.9)
        K.move(sheet, (i % 2) * 0.0012, -(i % 3) * 0.0008, i * 0.0012)
        K.mesh_obj("sheet", sheet, "parchment", bevel=0.0004)
    # Сгиб — по левому краю.
    K.mesh_obj("fold", K.lathe([(-h / 2, 0.005), (h / 2, 0.005)], 16, axis="Y"), "parchment", subsurf=1)
    for yy in (-0.04, 0.04):
        band = K.cbox(0, yy, 0.005, w + 0.004, 0.003, 0.0125)
        K.mesh_obj("string", band, "cord", bevel=0.0012)


@item("poultice", (1, 1), name="Кровоостанавливающая припарка", pose="upright", azimuth=24, elevation=30, margin=0.86)
def poultice():
    """Льняная подушечка с травами, перевязанная накрест."""
    M()
    leaf_mat("herb", (84, 110, 58))
    b = K.cbox(0, 0, 0.012, 0.07, 0.07, 0.024)
    K.deform(b, lambda v: (v.x * (1 - 0.15 * (abs(v.z - 0.012) / 0.012) ** 2), v.y * (1 - 0.15 * (abs(v.z - 0.012) / 0.012) ** 2), v.z))
    K.mesh_obj("pad", b, "linen", bevel=0.01, subsurf=2, smooth_angle=80)
    for a in (0, 90):
        t = K.torus((0, 0, 0.012), 0.037, 0.0014, plane="XZ", ry=0.014)
        K.rot(t, "Z", a)
        K.mesh_obj("tie", t, "cord")
    # Торчащие листики.
    for i, a in enumerate((20, 140, 250)):
        lf = K.lathe([(0, 0.0001), (0.008, 0.006), (0.022, 0.0001)], 8)
        K.scale(lf, 1, 0.2, 1)
        K.rot(lf, "Y", 80)
        K.rot(lf, "Z", a)
        K.move(lf, 0.03 * math.cos(math.radians(a)), 0.03 * math.sin(math.radians(a)), 0.02)
        K.mesh_obj("leaf", lf, "herb", solidify=0.0006)


@item("ointment", (1, 1), name="Ранозаживляющая мазь", pose="upright", azimuth=24, elevation=26, margin=0.86)
def ointment():
    M()
    ceramic("clay", (190, 150, 110))
    substance("salve", (206, 186, 120), (150, 130, 80), rough=(0.3, 0.5), bump=0.3)
    pot("clay", 0.035, 0.035, filling="salve", neck=0.92)


@item("theriac", (1, 1), name="Териак", pose="upright", azimuth=24, elevation=24, margin=0.86)
def theriac():
    """Териак в маленьком алебастровом сосудике-алабастре с крышкой."""
    M()
    K.organic_mat("alabaster", (232, 224, 206), (200, 186, 160), rough=(0.25, 0.45), bump=0.15, dirt=0.6, grain="X", grain_scale=1.5)
    pot("alabaster", 0.03, 0.06, lid="lid", neck=0.6, lid_mat="bronze")


@item("soap", (1, 1), name="Мыло", pose="upright", azimuth=26, elevation=30, margin=0.84)
def soap():
    M()
    substance("soap_mat", (214, 196, 150), (170, 150, 104), rough=(0.4, 0.6), bump=0.2)
    b = K.box(-0.035, 0.035, -0.025, 0.025, 0.0, 0.025)
    K.mesh_obj("soap", b, "soap_mat", bevel=0.006, subsurf=1)


@item("incendiary_pot", (2, 2), name="Зажигательный горшок", pose="upright", azimuth=24, elevation=16, margin=0.9)
def incendiary_pot():
    """Глиняный шаровидный горшок с горючей смесью и тряпичным фитилём в горле."""
    M()
    ceramic("clay", (150, 110, 80))
    prof = [(0.0, 0.0001), (0.004, 0.03), (0.03, 0.065), (0.07, 0.075), (0.11, 0.06), (0.135, 0.03), (0.145, 0.018), (0.155, 0.02), (0.16, 0.017)]
    b = K.lathe(prof, 40, cap_top=False)
    # Насечки, чтобы горшок держался в руке.
    K.deform(b, lambda v: (v.x * (1 + 0.012 * max(0, math.sin(math.atan2(v.y, v.x) * 14)) * (1 if 0.03 < v.z < 0.11 else 0)), v.y * (1 + 0.012 * max(0, math.sin(math.atan2(v.y, v.x) * 14)) * (1 if 0.03 < v.z < 0.11 else 0)), v.z))
    K.mesh_obj("pot", b, "clay", solidify=0.005, subsurf=1)
    K.cloth_mat("rag", (150, 130, 100), (90, 76, 56), weave=300.0, dirt=0.9)
    path = K.bezier((0, 0, 0.13), (0, 0, 0.18), (0.03, 0, 0.2), (0.06, 0, 0.17), n=12)
    K.mesh_obj("fuse", K.loft(path, [(0.012 - 0.006 * i / 12, 0.004) for i in range(13)], 8, up=(0, 1, 0)), "rag", subsurf=1)
    K.mesh_obj("plug", K.lathe([(0.14, 0.016), (0.162, 0.017)], 16), "rag", subsurf=1)
