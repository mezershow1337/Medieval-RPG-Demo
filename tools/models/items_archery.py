"""3D-модели луков, стрел и того, из чего стрелы собирают (archery.rs).

Собрать: python3 tools/models/build_items.py bow_simple bow_hunnic arrows_leaf arrows_bodkin arrows_trilobate arrows_broad arrow_shafts flight_feathers fish_glue sinew
"""
import math

import itemkit as K
from itemkit import item
from items_arsenal import arrowhead

# Ось Z — вдоль предмета, X — в сторону (изгиб лука), Y — толщина.


def mats():
    K.common_materials()
    K.organic_mat("ash_shaft", (196, 166, 120), (150, 118, 80), rough=(0.45, 0.65), grain="Z", grain_scale=10.0, dirt=0.3)
    K.organic_mat("feather_grey", (214, 210, 200), (120, 116, 108), rough=(0.7, 0.85), grain="Z", grain_scale=30.0, dirt=0.2, sheen=0.4)
    K.organic_mat("feather_dark", (92, 84, 76), (40, 36, 32), rough=(0.7, 0.85), grain="Z", grain_scale=30.0, dirt=0.2, sheen=0.4)
    K.organic_mat("birch_bark", (226, 214, 190), (120, 92, 64), rough=(0.6, 0.8), bump=0.3, cells=120.0, dirt=0.4)
    K.organic_mat("sinew_mat", (196, 160, 108), (140, 104, 60), rough=(0.4, 0.6), grain="Z", grain_scale=20.0, dirt=0.3)
    K.organic_mat("glue_mat", (190, 140, 70), (120, 80, 36), rough=(0.15, 0.3), bump=0.1, dirt=0.2)
    K.organic_mat("horn", (60, 48, 40), (28, 22, 18), rough=(0.3, 0.45), grain="Z", grain_scale=6.0, dirt=0.3)


# =========================================================================== лук


def limb_path(half, brace, tip_curl, n=24):
    """Плечо натянутого лука от рукояти (z=0) до конца (z=half). Стрелок и тетива — на стороне +x:
    плечи выгнуты к тетиве; у составного лука концы (сиахи) отогнуты обратно, от стрелка."""
    pts = []
    for i in range(n + 1):
        t = i / n
        z = half * (t - 0.06 * t * t)
        x = brace * t**1.7 - tip_curl * max(0.0, (t - 0.75) / 0.25) ** 2
        pts.append((x, 0.0, z))
    return pts


def bow(length, reflex, curl, width, thick, mat, ears=None, grip_mat="leather", string_mat="cord"):
    half = length / 2
    path = limb_path(half, reflex, curl)
    radii = []
    for i in range(len(path)):
        t = i / (len(path) - 1)
        k = 1.0 - 0.55 * t
        radii.append((thick * k * (1.25 if t < 0.12 else 1.0), width * k))
    for s in (1, -1):
        p = [(x, y, z * s) for x, y, z in path]
        K.mesh_obj("limb", K.loft(p, radii, segs=10), mat, subsurf=1)
        if ears:
            # Костяные накладки на концах (сиахи гуннского лука).
            e0 = int(len(path) * 0.74)
            pe = [(x + 0.002, y, z * s) for x, y, z in path[e0:]]
            re = [(thick * 0.75, width * 1.15) for _ in pe]
            K.mesh_obj("ear", K.loft(pe, re, segs=8), ears, subsurf=1)
    # Рукоять: обмотка; у составного — костяные пластины.
    K.mesh_obj("grip", K.loft([(0.0, 0.0, -0.06), (0.0, 0.0, 0.06)], [(thick * 1.5, width * 1.35)] * 2, segs=12), grip_mat, subsurf=1)
    if ears:
        for s in (1, -1):
            K.mesh_obj("plate", K.cbox(-thick * 1.1, 0, s * 0.07, 0.004, width * 2.0, 0.08), ears, bevel=0.001)
    # Тетива между концами.
    tip = path[-1]
    nock = (tip[0] + thick * 0.5, 0.0, tip[2] - 0.012)
    K.mesh_obj("string", K.loft([(nock[0], 0, nock[2]), (nock[0], 0, -nock[2])], [0.0018, 0.0018], segs=6), string_mat)


@item("bow_simple", (1, 4), name="Простой лук", pose="flat", azimuth=20, elevation=10, margin=0.95)
def bow_simple():
    mats()
    bow(1.5, 0.14, 0.0, 0.016, 0.012, "wood_dark")


@item("bow_hunnic", (1, 4), name="Гуннский составной лук", pose="flat", azimuth=20, elevation=10, margin=0.95)
def bow_hunnic():
    mats()
    bow(1.35, 0.2, 0.07, 0.017, 0.012, "horn", ears="bone", grip_mat="birch_bark", string_mat="sinew_mat")


# =========================================================================== стрелы


def shaft(x, y, length=0.78, r=0.0045, rot=0.0, fletch=True, dark=False):
    """Древко вдоль Z от z=0 (хвост) до length; перья у хвоста."""
    parts = []
    s = K.lathe([(0.0, 0.0001), (0.002, r * 0.9), (0.03, r), (length * 0.6, r * 1.05), (length, r * 0.85)], 10)
    parts.append(("shaft", s, "ash_shaft"))
    if fletch:
        for j in range(3):
            vane = K.blade([(0.03, 0.0002, 0.0003, 0.0), (0.05, 0.007, 0.0003, 0.007), (0.13, 0.009, 0.0003, 0.009), (0.15, 0.0002, 0.0003, 0.0)], 6, edge=0.0)
            K.move(vane, r * 0.6, 0, 0)
            K.rot(vane, "Z", j * 120 + 30)
            parts.append(("vane", vane, "feather_dark" if (dark and j == 0) else "feather_grey"))
        parts.append(("wrap", K.lathe([(0.026, r * 1.12), (0.034, r * 1.12)], 10), "sinew_mat"))
    for name, bm, mat in parts:
        K.rot(bm, "Z", rot)
        K.move(bm, x, y, 0)
        K.mesh_obj(name, bm, mat, bevel=0.0003 if name == "shaft" else 0.0)


def arrow_bundle(tip):
    mats()
    for i, (x, y) in enumerate([(-0.012, 0.0), (0.0, 0.006), (0.012, -0.002), (0.004, -0.012)]):
        z0 = 0.004 * (i % 2)
        shaft(x, y, rot=i * 47, dark=i % 2 == 1)
        K.PARTS[-1]  # перья уже созданы
        # Наконечник в торце древка: arrowhead строит его вдоль X — поворачиваем вверх.
        n0 = len(K.PARTS)
        arrowhead(tip, 0.0, 0.0, s=1.0)
        for o in K.PARTS[n0:]:
            o.rotation_euler = (0.0, -math.pi / 2, 0.0)
            o.location = (x, y, 0.78 + z0 + 0.002)
    K.mesh_obj("tie", K.lathe([(0.3, 0.0175), (0.318, 0.0175)], 16), "cord")


@item("arrows_leaf", (1, 2), name="Стрелы", pose="flat", azimuth=24, elevation=12, margin=0.94)
def arrows_leaf():
    arrow_bundle("leaf")


@item("arrows_bodkin", (1, 2), name="Бронебойные стрелы", pose="flat", azimuth=24, elevation=12, margin=0.94)
def arrows_bodkin():
    arrow_bundle("bodkin")


@item("arrows_trilobate", (1, 2), name="Трёхлопастные стрелы", pose="flat", azimuth=24, elevation=12, margin=0.94)
def arrows_trilobate():
    arrow_bundle("trilobate")


@item("arrows_broad", (1, 2), name="Стрелы-срезни", pose="flat", azimuth=24, elevation=12, margin=0.94)
def arrows_broad():
    arrow_bundle("broad")


@item("arrow_shafts", (1, 2), name="Древки стрел", pose="flat", azimuth=24, elevation=12, margin=0.94)
def arrow_shafts():
    mats()
    for i, (x, y) in enumerate([(-0.011, 0.0), (0.0, 0.007), (0.011, 0.0), (-0.005, -0.01), (0.006, -0.01)]):
        shaft(x, y, length=0.8, rot=i * 31, fletch=False)
    K.mesh_obj("tie", K.lathe([(0.38, 0.019), (0.395, 0.019)], 16), "cord")


@item("flight_feathers", (1, 1), name="Маховые перья", pose="flat", azimuth=30, elevation=30, margin=0.9)
def flight_feathers():
    mats()
    for i in range(5):
        a = -18 + i * 9
        quill = K.lathe([(0.0, 0.0001), (0.01, 0.0016), (0.2, 0.0009), (0.24, 0.0001)], 8)
        vane = K.blade([(0.04, 0.0003, 0.0004, 0.0), (0.07, 0.012, 0.0004, 0.011), (0.2, 0.014, 0.0004, 0.013), (0.24, 0.0003, 0.0003, 0.0)], 8, edge=0.0)
        for name, bm, mat in (("quill", quill, "bone"), ("vane", vane, "feather_dark" if i % 2 else "feather_grey")):
            K.rot(bm, "Y", a * 0.2)
            K.rot(bm, "X", -90)
            K.rot(bm, "Z", a)
            K.move(bm, 0, 0, 0.002 * i)
            K.mesh_obj(name, bm, mat)


@item("fish_glue", (1, 1), name="Рыбий клей", pose="flat", azimuth=24, elevation=30, margin=0.88)
def fish_glue():
    mats()
    # Сухие пластинки клея в горшочке.
    K.mesh_obj("pot", K.lathe([(0.0, 0.0001), (0.002, 0.03), (0.03, 0.036), (0.05, 0.03), (0.056, 0.026), (0.058, 0.028)], 28, cap_top=False), "wood", solidify=0.003)
    K.mesh_obj("glue", K.lathe([(0.0, 0.0001), (0.044, 0.029), (0.048, 0.0001)], 28), "glue_mat")
    for i in range(3):
        sheet = K.cbox(0.0, 0.0, 0.0, 0.03, 0.02, 0.002)
        K.rot(sheet, "Z", i * 50)
        K.rot(sheet, "X", 20 + i * 15)
        K.move(sheet, -0.01 + i * 0.01, 0.0, 0.054)
        K.mesh_obj("sheet", sheet, "glue_mat", bevel=0.0005)


@item("sinew", (1, 1), name="Сухожилия", pose="flat", azimuth=24, elevation=30, margin=0.9)
def sinew():
    mats()
    for i in range(6):
        a = i * 30
        pts = [(0.01 * math.sin(t * 3 + i), 0.006 * math.cos(t * 2 + i), -0.11 + 0.22 * t) for t in [k / 12 for k in range(13)]]
        rr = [0.0024 * (1 - abs(2 * k / 12 - 1) * 0.6) for k in range(13)]
        bm = K.loft(pts, rr, segs=8)
        K.rot(bm, "Z", a)
        K.rot(bm, "X", -90)
        K.move(bm, 0, 0, 0.004 * (i % 3))
        K.mesh_obj("sinew", bm, "sinew_mat", subsurf=1)
    K.mesh_obj("tie", K.torus((0, 0, 0.006), 0.012, 0.0016, plane="XZ"), "cord")
