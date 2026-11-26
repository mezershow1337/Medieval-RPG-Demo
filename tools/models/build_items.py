"""3D-модели предметов и их иконки (Херсонес, около 510 г.).

Каждый предмет — функция, которая строит модель из процедурной геометрии
и процедурных материалов. Из модели получаются:
  * assets/icons/items/<ключ>.png — иконка для инвентаря (рендер Cycles);
  * assets/models/items/<ключ>.glb — модель, которая лежит на земле.

Запуск (Blender как модуль: pip install bpy):
    python3 tools/models/build_items.py              # все предметы
    python3 tools/models/build_items.py spatha gold_coin # только эти

Как добавить предмет: написать функцию с декоратором
@item(ключ, (ширина, высота), name="Название из items.rs", pose=...) и запустить скрипт с этим ключом.
Скрипт сам обновит таблицу src/icons/art.rs (название -> ключ) для всех готовых моделей.
Размер в клетках должен совпадать с размером предмета в src/items.rs (это проверяет cargo test).
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import itemkit as K  # noqa: E402
from itemkit import Vector  # noqa: E402

from itemkit import ITEMS, item  # noqa: E402


# =========================================================================== общие материалы


def mats_common():
    K.common_materials()


# =========================================================================== детали


def round_grip(z0, z1, r, ribs=3, mat="wood_dark", name="grip"):
    """Рукоять с утолщением в середине и бороздками под пальцы."""
    rings = []
    n = 24
    for i in range(n + 1):
        t = i / n
        z = z0 + (z1 - z0) * t
        bulge = 1 + 0.18 * math.sin(math.pi * t)
        groove = 1 - 0.07 * max(0.0, math.cos(2 * math.pi * ribs * t)) ** 6 if ribs else 1
        rr = r * bulge * groove
        rings.append((z, rr, rr * 0.78))
    return K.mesh_obj(name, K.lathe(rings, 20), mat, subsurf=1)


def spatha_blade(length, w0, w1, name="blade", mat="blade_steel", fuller_w=0.42):
    st = []
    n = 40
    for i in range(n + 1):
        t = i / n
        z = length * t
        # Почти параллельные лезвия, у конца — округлое острие (спаты VI века).
        w = w0 + (w1 - w0) * t
        tip_start = 0.86
        if t > tip_start:
            k = (t - tip_start) / (1 - tip_start)
            w *= math.sqrt(max(0.0, 1 - k**1.6))
        th = 0.0032 * (1 - 0.35 * t)
        st.append((z, w, th))
    return K.mesh_obj(name, K.blade(st, 12, fuller=(0.02, length * 0.82, fuller_w, 0.0011)), mat, smooth_angle=60)


# =========================================================================== оружие


@item("spatha", (1, 3), name="Спата", azimuth=24, elevation=10, margin=0.94)
def spatha():
    """Спата: прямой обоюдоострый меч ~90 см, маленькая гарда, костяная рукоять."""
    mats_common()
    spatha_blade(0.76, 0.026, 0.022)
    # Гарда — короткая, линзовидная.
    g = K.lathe([(-0.016, 0.0001), (-0.012, 0.042, 0.012), (0.0, 0.046, 0.014), (0.004, 0.044, 0.012), (0.006, 0.0001)], 24)
    K.mesh_obj("guard", g, "iron", subsurf=1)
    round_grip(-0.118, -0.016, 0.0145, ribs=3, mat="bone")
    # Навершие — «грибовидное».
    p = K.lathe([(-0.148, 0.0001), (-0.146, 0.018, 0.012), (-0.136, 0.03, 0.018), (-0.126, 0.028, 0.016), (-0.12, 0.016, 0.011), (-0.116, 0.0001)], 24)
    K.mesh_obj("pommel", p, "iron", subsurf=1)
    # Шляпка хвостовика.
    K.mesh_obj("peen", K.lathe([(-0.153, 0.0001), (-0.152, 0.005), (-0.147, 0.006), (-0.146, 0.0001)], 12), "iron", subsurf=1)



@item("cavalry_spatha", (1, 4), name="Кавалерийская спата", azimuth=24, elevation=10, margin=0.94)
def cavalry_spatha():
    """Длинная кавалерийская спата с золочёной гардой и гранатовыми вставками
    (как мечи из керченских погребений V–VI вв.)."""
    mats_common()
    K.metal_mat("pattern_steel", (190, 192, 196), (120, 122, 128), rough=(0.2, 0.36), scratches=0.7, rust=0.08, dirt=0.4, polish_dir="Z", bump=0.4, grind=True)
    spatha_blade(0.84, 0.027, 0.024, mat="pattern_steel", fuller_w=0.5)
    g = K.lathe([(-0.018, 0.0001), (-0.015, 0.048, 0.015), (0.0, 0.05, 0.016), (0.004, 0.047, 0.014), (0.006, 0.0001)], 28)
    K.mesh_obj("guard", g, "gilt", subsurf=1)
    # Гранаты на гарде (перегородчатая инкрустация).
    for x in (-0.03, 0.0, 0.03):
        cab = K.lathe([(0, 0.0055, 0.0045), (0.0015, 0.005, 0.004), (0.003, 0.003, 0.0025), (0.0035, 0.0001)], 16)
        K.rot(cab, "X", 90)
        K.move(cab, x, -0.0155, -0.006)
        K.mesh_obj("gem", cab, "garnet", subsurf=1)
    round_grip(-0.13, -0.018, 0.0145, ribs=0, mat="wood_dark")
    # Серебряная обмотка проволокой — кольца по рукояти.
    for i in range(9):
        z = -0.124 + i * 0.0125
        K.mesh_obj("wire", K.torus((0, 0, z), 0.0168, 0.0011, ry=0.0132), "silver")
    for z in (-0.13, -0.02):
        K.mesh_obj("ferrule", K.lathe([(z - 0.004, 0.017, 0.014), (z + 0.004, 0.017, 0.014)], 24), "gilt", bevel=0.0008)
    p = K.lathe([(-0.165, 0.0001), (-0.163, 0.02, 0.013), (-0.152, 0.032, 0.019), (-0.142, 0.03, 0.017), (-0.135, 0.018, 0.012), (-0.13, 0.0001)], 28)
    K.mesh_obj("pommel", p, "gilt", subsurf=1)
    cab = K.lathe([(0, 0.008, 0.006), (0.002, 0.0075, 0.0055), (0.004, 0.004, 0.003), (0.0045, 0.0001)], 16)
    K.rot(cab, "X", 90)
    K.move(cab, 0, -0.019, -0.147)
    K.mesh_obj("gem", cab, "garnet", subsurf=1)


@item("dagger", (1, 2), name="Кинжал", azimuth=24, elevation=10, margin=0.92)
def dagger():
    """Боевой нож-кинжал: листовидный клинок, железная гарда, костяная рукоять."""
    mats_common()
    st = []
    for i in range(25):
        t = i / 24
        w = 0.016 * (1 + 0.25 * math.sin(math.pi * t * 0.8)) * (1 - t**3) ** 0.7
        st.append((0.2 * t, w, 0.0028 * (1 - 0.5 * t)))
    K.mesh_obj("blade", K.blade(st, 10), "blade_steel", smooth_angle=60)
    K.mesh_obj("guard", K.cbox(0, 0, -0.005, 0.06, 0.016, 0.01), "iron", bevel=0.0025)
    round_grip(-0.1, -0.01, 0.012, ribs=2, mat="bone")
    K.mesh_obj("pommel", K.lathe([(-0.118, 0.0001), (-0.115, 0.014, 0.01), (-0.106, 0.018, 0.012), (-0.1, 0.012, 0.009), (-0.098, 0.0001)], 20), "iron", subsurf=1)


@item("mace", (1, 2), name="Булава", azimuth=25, elevation=12, margin=0.92)
def mace():
    """Булава степного образца: железное навершие с шипами-шишками на древке."""
    mats_common()
    rings = []
    for i in range(21):
        t = i / 20
        rings.append((0.52 * t, 0.015 + 0.003 * (1 - t)))
    K.mesh_obj("haft", K.lathe(rings, 16), "wood", bevel=0.0)
    # Кожаная обмотка рукояти.
    path = []
    for i in range(240):
        a = i * 0.45
        path.append((0.0185 * math.cos(a), 0.0185 * math.sin(a), 0.02 + i * 0.0006))
    K.mesh_obj("wrap", K.loft(path, [(0.0035, 0.0012)] * len(path), 6, up=(0, 0, 1)), "leather_dark", subsurf=0)
    K.mesh_obj("pommel", K.lathe([(0.0, 0.0001), (0.004, 0.02), (0.016, 0.021), (0.02, 0.0175)], 16), "iron", bevel=0.002)
    # Навершие: сфера-втулка и восемь пирамидальных шишек.
    head = K.lathe([(0.47, 0.022), (0.49, 0.034), (0.52, 0.04), (0.55, 0.034), (0.57, 0.022), (0.58, 0.012), (0.585, 0.0001)], 24)
    K.mesh_obj("head", head, "iron", subsurf=1)
    for k in range(2):
        for j in range(4):
            a = math.radians(j * 90 + k * 45)
            z = 0.505 + k * 0.03
            knob = K.lathe([(0, 0.012), (0.014, 0.007), (0.022, 0.0001)], 4)
            K.rot(knob, "Z", 45)
            K.rot(knob, "Y", 90)
            K.move(knob, 0.03, 0, 0)
            K.rot(knob, "Z", math.degrees(a))
            K.move(knob, 0, 0, z)
            K.mesh_obj("knob", knob, "iron", bevel=0.0015, smooth_angle=30)
    K.mesh_obj("collar", K.lathe([(0.455, 0.019), (0.47, 0.023), (0.475, 0.019)], 16), "iron", bevel=0.001)


@item("axe", (2, 3), name="Боевой топор", azimuth=20, elevation=10, margin=0.92)
def axe():
    """Боевой топор-«бородовик» на прямом ясеневом топорище."""
    mats_common()
    rings = []
    for i in range(31):
        t = i / 30
        rings.append((0.74 * t, 0.017 - 0.002 * t, 0.012 - 0.001 * t))
    haft = K.lathe(rings, 16)
    K.deform(haft, lambda v: (v.x + 0.025 * math.sin(math.pi * v.z / 0.74) * 0.6, v.y, v.z))
    K.mesh_obj("haft", haft, "wood", subsurf=0)
    # Обух с проушиной.
    eye = K.lathe([(0.6, 0.024, 0.018), (0.68, 0.026, 0.02), (0.72, 0.024, 0.017)], 20)
    K.mesh_obj("eye", eye, "iron", bevel=0.002, subsurf=1)
    # Лезвие: сечения от проушины к лезвию; низ вытянут в «бороду».
    st = []
    n = 24
    for i in range(n + 1):
        t = i / n
        x = 0.02 + 0.17 * t
        top = 0.71 + 0.03 * t**2
        bottom = 0.62 - 0.13 * t**2.2
        half = (top - bottom) / 2
        centre = (top + bottom) / 2
        th = 0.009 * (1 - t) ** 1.2 + 0.0008
        st.append((x, half, th, centre))
    head = K.blade(st, 12, edge=0.0004)
    # blade(): ось Z — от проушины к лезвию, X — высота. Повернём: Z -> -X, X -> Z.
    K.deform(head, lambda v: (-v.z, v.y, v.x))
    K.mesh_obj("head", head, "axe_iron", subsurf=0, smooth_angle=50)
    K.mesh_obj("butt", K.lathe([(-0.005, 0.0001), (0.0, 0.019, 0.014), (0.012, 0.018, 0.013)], 16), "iron", bevel=0.001)


@item("spear", (1, 5), name="Пехотное копьё", azimuth=22, elevation=10, margin=0.95)
def spear():
    """Пехотное копьё (контарион): листовидный наконечник с ребром, ясеневое древко, подток."""
    mats_common()
    L = 1.9
    rings = [(0.06 + (L - 0.06) * i / 40, 0.0155 - 0.003 * i / 40) for i in range(41)]
    K.mesh_obj("shaft", K.lathe(rings, 14), "wood", bevel=0.0)
    # Наконечник: втулка + перо с ребром жёсткости (ромбовидное сечение).
    K.mesh_obj("socket", K.lathe([(L - 0.12, 0.0165), (L - 0.03, 0.0125), (L, 0.01)], 16), "iron", subsurf=1)
    K.mesh_obj("ring", K.torus((0, 0, L - 0.115), 0.017, 0.0022), "iron")
    st = []
    for i in range(33):
        t = i / 32
        w = 0.032 * math.sin(math.pi * min(1.0, t * 1.15) ** 0.75) * (1 - t) ** 0.35 + 0.006 * (1 - t)
        st.append((L + 0.3 * t, w, 0.0055 * (1 - t) + 0.0008))
    tip = K.blade(st, 8, edge=0.0002)
    K.deform(tip, lambda v: (v.x, v.y * (1.0 + 2.2 * (1 - min(1, abs(v.x) / 0.008))) * 0.75, v.z))
    K.mesh_obj("blade", tip, "blade_steel", smooth_angle=35)
    K.mesh_obj("butt", K.lathe([(-0.05, 0.0001), (0.0, 0.009), (0.075, 0.0158)], 12), "iron", subsurf=1)


# =========================================================================== щиты


def shield_paint_oval(nb, vec):
    """Роспись щита: красное поле, белое кольцо, синий круг у умбона (по образцу
    эмблем позднеримских частей)."""
    sep = nb.new("ShaderNodeSeparateXYZ")
    nb.link(vec, sep.inputs["Vector"])
    x = nb.math("DIVIDE", sep.outputs["X"], 0.31)
    z = nb.math("DIVIDE", sep.outputs["Z"], 0.46)
    r = nb.math("SQRT", nb.math("ADD", nb.math("MULTIPLY", x, x), nb.math("MULTIPLY", z, z)))
    col = nb.ramp(r, [(0.0, K.lin((38, 58, 110))), (0.3, K.lin((38, 58, 110))), (0.305, K.lin((222, 210, 180))), (0.38, K.lin((222, 210, 180))), (0.385, K.lin((150, 28, 24))), (0.8, K.lin((150, 28, 24))), (0.805, K.lin((222, 210, 180))), (0.86, K.lin((222, 210, 180))), (0.865, K.lin((150, 28, 24)))], "CONSTANT")
    # Только лицевая сторона окрашена.
    g = nb.new("ShaderNodeNewGeometry")
    nsep = nb.new("ShaderNodeSeparateXYZ")
    nb.link(g.outputs["Normal"], nsep.inputs["Vector"])
    front = nb.math("LESS_THAN", nsep.outputs["Y"], -0.3)
    return nb.mix(front, K.lin((120, 82, 48)), col)


def shield_paint_round(nb, vec):
    sep = nb.new("ShaderNodeSeparateXYZ")
    nb.link(vec, sep.inputs["Vector"])
    ang = nb.math("ARCTAN2", sep.outputs["X"], sep.outputs["Z"])
    sector = nb.math("MODULO", nb.math("ADD", nb.math("MULTIPLY", ang, 4 / math.pi), 8.5), 2.0)
    stripe = nb.math("LESS_THAN", sector, 1.0)
    col = nb.mix(stripe, K.lin((196, 160, 60)), K.lin((58, 90, 70)))
    g = nb.new("ShaderNodeNewGeometry")
    nsep = nb.new("ShaderNodeSeparateXYZ")
    nb.link(g.outputs["Normal"], nsep.inputs["Vector"])
    front = nb.math("LESS_THAN", nsep.outputs["Y"], -0.3)
    return nb.mix(front, K.lin((120, 82, 48)), col)


def shield(rx, rz, dome, paint, umbo_r):
    rings = []
    n = 16
    for i in range(n + 1):
        t = i / n
        rings.append((-dome * (1 - t**2), rx * t + 1e-4, rz * t + 1e-4))
    # Ось вращения Y: лицо в -Y. lathe(axis="Y") кладёт (a, b) в (X, Z).
    board = K.lathe(rings, 48, cap_top=False, cap_bot=False, axis="Y")
    K.mesh_obj("board", board, paint, solidify=0.012, smooth_angle=60)
    rim = []
    for i in range(96):
        a = 2 * math.pi * i / 96
        rim.append((rx * math.cos(a), 0.0, rz * math.sin(a)))
    K.mesh_obj("rim", K.loft(rim, [(0.008, 0.012)] * len(rim), 10, up=(0, 1, 0), closed=True), "rawhide")
    # Умбон: полусфера на фланце, заклёпки по кругу.
    u = K.lathe([(-dome - 0.002, umbo_r * 1.45), (-dome - 0.006, umbo_r * 1.4), (-dome - 0.008, umbo_r * 0.98), (-dome - 0.045, umbo_r * 0.85), (-dome - 0.07, umbo_r * 0.55), (-dome - 0.08, 0.0001)], 40, axis="Y")
    K.mesh_obj("umbo", u, "iron", subsurf=1, smooth_angle=50)
    for i in range(6):
        a = 2 * math.pi * i / 6 + 0.3
        rv = K.lathe([(0, 0.0055), (0.003, 0.0045), (0.0045, 0.0001)], 10)
        K.rot(rv, "X", 90)
        K.move(rv, umbo_r * 1.22 * math.cos(a), -dome - 0.006, umbo_r * 1.22 * math.sin(a))
        K.mesh_obj("rivet", rv, "bronze", subsurf=1)


@item("shield_oval", (2, 3), name="Овальный щит", azimuth=20, elevation=10, margin=0.93)
def shield_oval():
    """Большой овальный щит (скутум позднего Рима): доски, сыромятный обод, умбон, роспись."""
    mats_common()
    K.painted_mat("paint_oval", shield_paint_oval, chip=0.45)
    shield(0.33, 0.48, 0.045, "paint_oval", 0.075)


@item("shield_round", (2, 2), name="Круглый щит", azimuth=20, elevation=10, margin=0.93)
def shield_round():
    """Круглый щит с расписными секторами."""
    mats_common()
    K.painted_mat("paint_round", shield_paint_round, chip=0.5)
    shield(0.3, 0.3, 0.03, "paint_round", 0.07)


# =========================================================================== шлемы


def bowl_rings(h, rx, ry, steps=14, point=0.0):
    """Купол шлема: point=0 — полусфера, point=1 — заострённый конус с округлой вершиной."""
    rings = []
    for i in range(steps + 1):
        t = i / steps
        dome = math.cos(t * math.pi / 2)
        cone = (1 - t**1.6) ** 0.9
        k = dome * (1 - point) + cone * point
        rings.append((h * t if point else h * math.sin(t * math.pi / 2), max(rx * k, 1e-4), max(ry * k, 1e-4)))
    return rings


def bowl_point(rings, a):
    """Точка на поверхности купола на угле a (0 — спереди) для каждого кольца."""
    out = []
    for z, rx, ry in rings:
        out.append(Vector((rx * math.sin(a), -ry * math.cos(a), z)))
    return out


@item("spangenhelm", (2, 2), name="Спангенхельм", pose="upright", azimuth=28, elevation=14, margin=0.9)
def spangenhelm():
    """Спангенхельм: купол из шести железных пластин на золочёных бронзовых полосах,
    налобный обруч, нащёчники на петлях, кольчужная бармица сзади."""
    mats_common()
    K.image_mat("mail", "mail_color", 0.03, mr="mail_mr", normal="mail_normal", tint=(255, 250, 240), dirt=0.2)
    H, RX, RY = 0.19, 0.098, 0.115
    rings = bowl_rings(H, RX, RY, 20, point=0.85)
    K.mesh_obj("bowl", K.lathe(rings, 48, cap_bot=False), "iron", solidify=0.002, smooth_angle=70)
    # Полосы-«шпанги» и заклёпки на них.
    for j in range(6):
        a = 2 * math.pi * j / 6 + math.pi / 6
        pts = bowl_point(rings[:-2], a)
        path = [p + Vector((p.x, p.y, 0)).normalized() * 0.003 for p in pts]
        up = Vector((math.sin(a), -math.cos(a), 0))
        K.mesh_obj("band", K.loft(path, [(0.011, 0.0017)] * len(path), 8, up=tuple(up)), "gilt", subsurf=0)
        for t in (0.25, 0.55):
            p = path[int(len(path) * t)]
            rv = K.lathe([(0, 0.0035), (0.002, 0.0025), (0.003, 0.0001)], 10)
            n = Vector((p.x, p.y, 0)).normalized()
            q = Vector((0, 0, 1)).rotation_difference(n)
            K.transform(rv, K.Matrix.Translation(p + n * 0.0015) @ q.to_matrix().to_4x4())
            K.mesh_obj("rivet", rv, "gilt", subsurf=1)
    K.mesh_obj("knob", K.lathe([(H - 0.01, 0.016), (H + 0.004, 0.012), (H + 0.016, 0.005), (H + 0.02, 0.0001)], 20), "gilt", subsurf=1)
    # Налобный обруч с чеканкой (бугорки).
    K.mesh_obj("brow", K.lathe([(-0.008, RX + 0.004, RY + 0.004), (0.032, RX + 0.002, RY + 0.002)], 64), "gilt", solidify=0.0018, bevel=0.0006)
    for i in range(28):
        a = 2 * math.pi * i / 28
        if 0.6 < a < 2 * math.pi - 0.6 and abs(a - math.pi) > 0.6:
            continue
        b = K.lathe([(0, 0.0032), (0.0018, 0.0022), (0.0026, 0.0001)], 8)
        n = Vector((math.sin(a), -math.cos(a), 0))
        K.transform(b, K.Matrix.Translation(Vector((RX * 1.06 * math.sin(a), -RY * 1.06 * math.cos(a), 0.012))) @ Vector((0, 0, 1)).rotation_difference(n).to_matrix().to_4x4())
        K.mesh_obj("boss", b, "gilt", subsurf=1)
    # Нащёчники.
    for side in (-1, 1):
        st = [(-0.0 - 0.1 * i / 12, 0.03 * (1 - 0.45 * (i / 12) ** 1.5), 0.0016, 0.01 * (i / 12)) for i in range(13)]
        ch = K.blade(st, 8, edge=0.0012)
        # Ось ширины X -> Y (вперёд-назад), лёгкий наклон внутрь.
        K.deform(ch, lambda v, s=side: (s * (RX + 0.005 - 0.018 * (-v.z / 0.1) ** 2) + v.y * s, -v.x - 0.045, v.z))
        K.mesh_obj("cheek", ch, "iron", smooth_angle=60)
    # Кольчужная бармица сзади.
    aw = K.partial_lathe([(-0.008, RX + 0.004, RY + 0.004), (-0.05, RX + 0.012, RY + 0.014), (-0.095, RX + 0.024, RY + 0.028)], 100, 260, 40)
    K.deform(aw, lambda v: (v.x, v.y, v.z + 0.004 * math.sin(v.x * 140)))
    K.mesh_obj("aventail", aw, "mail", solidify=0.004, smooth_angle=80)


@item("ridge_helmet", (2, 2), name="Гребенчатый шлем", pose="upright", azimuth=28, elevation=14, margin=0.9)
def ridge_helmet():
    """Гребенчатый шлем: две половины купола на продольном гребне, нащёчники, назатыльник."""
    mats_common()
    H, RX, RY = 0.165, 0.096, 0.112
    rings = bowl_rings(H, RX, RY, 16, point=0.0)
    K.mesh_obj("bowl", K.lathe(rings, 48, cap_bot=False), "iron", solidify=0.002, smooth_angle=70)
    # Гребень спереди назад.
    path = []
    for i in range(41):
        a = -math.pi / 2 + math.pi * i / 40
        z = H * math.cos(a) + 0.006
        y = (RY + 0.004) * math.sin(a)
        path.append((0, y, max(z, 0.01)))
    K.mesh_obj("ridge", K.loft(path, [(0.0045, 0.008)] * len(path), 10, up=(1, 0, 0)), "bronze", subsurf=1)
    for i in range(1, 8):
        p = Vector(path[i * 5])
        rv = K.lathe([(0, 0.0028), (0.0018, 0.0018), (0.0024, 0.0001)], 8)
        K.rot(rv, "Y", 90)
        K.move(rv, 0.0045, p.y, p.z - 0.002)
        rv2 = K.mirror_x(K.copy(rv))
        K.mesh_obj("rivet", rv, "bronze", subsurf=1)
        K.mesh_obj("rivet", rv2, "bronze", subsurf=1)
    K.mesh_obj("brow", K.lathe([(-0.006, RX + 0.004, RY + 0.004), (0.02, RX + 0.003, RY + 0.003)], 64), "iron_dark", solidify=0.002, bevel=0.0006)
    # Назатыльник.
    ng = K.partial_lathe([(0.0, RX + 0.003, RY + 0.003), (-0.03, RX + 0.02, RY + 0.03), (-0.055, RX + 0.04, RY + 0.055)], 120, 240, 20)
    K.mesh_obj("neck", ng, "iron", solidify=0.002, smooth_angle=70, bevel=0.0)
    for side in (-1, 1):
        st = [(-0.12 * i / 12, 0.04 * (1 - 0.4 * (i / 12) ** 1.4), 0.0016, 0.01 * (i / 12)) for i in range(13)]
        ch = K.blade(st, 8, edge=0.0012)
        K.deform(ch, lambda v, s=side: (s * (RX + 0.006 - 0.02 * (-v.z / 0.12) ** 2) + v.y * s, -v.x - 0.03, v.z - 0.004))
        K.mesh_obj("cheek", ch, "iron", smooth_angle=60)


# =========================================================================== доспехи


def torso_rings(z0, z1, n=24, flare=0.0, chest=1.0):
    """Кольца торса снизу вверх: бёдра — талия — грудь — плечи."""
    rings = []
    for i in range(n + 1):
        t = i / n
        z = z0 + (z1 - z0) * t
        rx = 0.175 + 0.02 * math.sin(math.pi * t) * chest + flare * (1 - t) ** 3
        ry = 0.115 + 0.02 * math.sin(math.pi * min(1, t * 1.2)) * chest + flare * 0.6 * (1 - t) ** 3
        if t > 0.85:
            k = (t - 0.85) / 0.15
            rx *= 1 - 0.12 * k
            ry *= 1 - 0.25 * k
        rings.append((z, rx, ry))
    return rings


def garment(name, mat, z0, z1, sleeve_len, sleeve_r, flare=0.0, quilt=0, wrinkle=0.0, solid=0.006):
    rings = torso_rings(z0, z1, 90 if quilt else 30, flare)
    body = K.lathe(rings, 160 if quilt else 64, cap_top=False, cap_bot=False)
    if quilt or wrinkle:
        def d(v):
            a = math.atan2(v.x, -v.y)
            r = Vector((v.x, v.y, 0))
            k = 1.0
            if quilt:
                k += 0.045 * (abs(math.sin(a * quilt / 2)) ** 0.35 - 0.7)
                k += 0.03 * (abs(math.sin(v.z * 36)) ** 0.35 - 0.7)
            if wrinkle:
                k += wrinkle * math.sin(a * 7 + v.z * 9) * (1 - (v.z - z0) / (z1 - z0)) ** 2
            return (v.x * k, v.y * k, v.z)
        K.deform(body, d)
    K.mesh_obj(name, body, mat, solidify=solid, smooth_angle=80)
    # Плечи и ворот: купол поверх.
    top = []
    rx, ry = rings[-1][1], rings[-1][2]
    for i in range(7):
        t = i / 6
        top.append((z1 + 0.04 * math.sin(t * math.pi / 2), rx * (1 - 0.55 * t), ry * (1 - 0.35 * t)))
    K.mesh_obj(name + "_shoulders", K.lathe(top, 64, cap_top=False, cap_bot=False), mat, solidify=solid, smooth_angle=80)
    for s in (-1, 1):
        sx = s * (rx - 0.02)
        path = [Vector((sx, 0, z1 - 0.02)), Vector((sx + s * 0.07, 0, z1 - 0.06)), Vector((sx + s * (0.07 + sleeve_len * 0.6), 0, z1 - 0.06 - sleeve_len * 0.8))]
        pts = []
        rad = []
        for i in range(13):
            t = i / 12
            if t < 0.5:
                p = path[0].lerp(path[1], t * 2)
            else:
                p = path[1].lerp(path[2], (t - 0.5) * 2)
            pts.append(p)
            rad.append((sleeve_r * (1.25 - 0.3 * t), sleeve_r * (1.0 - 0.15 * t)))
        sl = K.loft(pts, rad, 24, cap_start=False, cap_end=False, up=(0, 1, 0))
        K.mesh_obj(name + "_sleeve", sl, mat, solidify=solid, smooth_angle=80)


@item("padded_tunic", (2, 3), squash=0.22, name="Стёганая туника", azimuth=16, elevation=6, margin=0.92)
def padded_tunic():
    """Стёганая туника из льна с войлочной набивкой: вертикальные и поперечные строчки."""
    mats_common()
    K.cloth_mat("quilt_linen", (204, 186, 146), (160, 140, 100), weave=1100.0, dirt=0.6)
    garment("tunic", "quilt_linen", -0.14, 0.62, 0.15, 0.078, flare=0.03, quilt=26, solid=0.012)
    K.mesh_obj("collar", K.torus((0, 0, 0.66), 0.075, 0.012, ry=0.065), "linen")


@item("mail_shirt", (2, 3), squash=0.22, name="Кольчуга", azimuth=16, elevation=6, margin=0.92)
def mail_shirt():
    """Кольчуга (лорика хамата) с короткими рукавами до середины бедра."""
    mats_common()
    K.image_mat("mail", "mail_color", 0.035, mr="mail_mr", normal="mail_normal", tint=(255, 252, 245), dirt=0.25)
    garment("mail", "mail", -0.08, 0.62, 0.16, 0.072, flare=0.035, wrinkle=0.02, solid=0.005)
    # Подол и края рукавов — бронзовые кольца (частая отделка).
    K.mesh_obj("hem", K.torus((0, 0, -0.08), 0.2, 0.004, ry=0.14), "bronze")


@item("lamellar", (2, 3), squash=0.35, name="Ламеллярный панцирь", azimuth=16, elevation=6, margin=0.92)
def lamellar():
    """Ламеллярный панцирь (клибанион): ряды железных пластинок на кожаном шнуре,
    кожаные птеруги снизу и на плечах."""
    mats_common()
    K.metal_mat("lames", (150, 150, 154), (90, 88, 86), rough=(0.3, 0.55), rust=0.3, dirt=0.9, scale=3.0)
    rings = torso_rings(0.08, 0.6, 24)

    def ring_at(z):
        for (z0, rx0, ry0), (z1, rx1, ry1) in zip(rings, rings[1:]):
            if z0 <= z <= z1:
                t = (z - z0) / (z1 - z0)
                return rx0 + (rx1 - rx0) * t, ry0 + (ry1 - ry0) * t
        return rings[-1][1], rings[-1][2]

    import bmesh
    lames = bmesh.new()
    row_h = 0.058
    rows = 9
    for r in range(rows):
        z = 0.1 + r * row_h * 0.86
        rx, ry = ring_at(z + row_h / 2)
        rx += 0.006
        ry += 0.006
        circ = math.pi * (3 * (rx + ry) - math.sqrt((3 * rx + ry) * (rx + 3 * ry)))
        n = int(circ / 0.019)
        for i in range(n):
            a = 2 * math.pi * (i + 0.5 * (r % 2)) / n
            x, y = rx * math.sin(a), -ry * math.cos(a)
            b = K.cbox(0, 0, 0, 0.021, 0.0022, row_h)
            # Скруглённый низ пластинки.
            K.deform(b, lambda v: (v.x * (1 - 0.3 * max(0, -v.z / (row_h / 2)) ** 4), v.y, v.z))
            K.rot(b, "X", -7)
            nrm = Vector((math.sin(a) * ry, -math.cos(a) * rx, 0)).normalized()
            q = Vector((0, -1, 0)).rotation_difference(nrm)
            K.transform(b, K.Matrix.Translation((x, y, z + row_h / 2)) @ q.to_matrix().to_4x4())
            me = K.bpy.data.meshes.new("t")
            b.to_mesh(me)
            b.free()
            lames.from_mesh(me)
            K.bpy.data.meshes.remove(me)
    K.mesh_obj("lames", lames, "lames", bevel=0.0007, smooth_angle=40)
    # Подкладка (кожа) под пластинками.
    K.mesh_obj("backing", K.lathe(torso_rings(0.08, 0.62, 24), 48, cap_top=False, cap_bot=False), "leather_dark", solidify=0.004)
    # Шнуры по рядам.
    for r in range(rows):
        z = 0.1 + r * row_h * 0.86 + row_h * 0.82
        rx, ry = ring_at(z)
        K.mesh_obj("lace", K.torus((0, 0, z), rx + 0.0095, 0.0018, ry=ry + 0.0095), "leather")
    # Птеруги — кожаные полосы по подолу.
    strips = bmesh.new()
    rx, ry = ring_at(0.1)
    for i in range(30):
        a = 2 * math.pi * i / 30
        b = K.cbox(0, 0, 0, 0.034, 0.004, 0.15)
        K.deform(b, lambda v: (v.x, v.y, v.z) if v.z > -0.07 else (v.x * 0.9, v.y, v.z))
        K.rot(b, "X", -6)
        nrm = Vector((math.sin(a) * ry, -math.cos(a) * rx, 0)).normalized()
        q = Vector((0, -1, 0)).rotation_difference(nrm)
        K.transform(b, K.Matrix.Translation((rx * 1.03 * math.sin(a), -ry * 1.03 * math.cos(a), 0.055)) @ q.to_matrix().to_4x4())
        me = K.bpy.data.meshes.new("t")
        b.to_mesh(me)
        b.free()
        strips.from_mesh(me)
        K.bpy.data.meshes.remove(me)
    K.mesh_obj("pteruges", strips, "leather", bevel=0.0012)
    # Плечевые ремни и наплечные птеруги.
    for s in (-1, 1):
        path = [(s * 0.12, -0.12, 0.58), (s * 0.13, -0.07, 0.66), (s * 0.13, 0.07, 0.66), (s * 0.12, 0.12, 0.58)]
        K.mesh_obj("strap", K.loft(K.bezier(*path, n=12), [(0.026, 0.004)] * 13, 8, up=(0, 0, 1)), "leather", subsurf=1)
        for k in range(5):
            b = K.cbox(0, 0, -0.06, 0.03, 0.004, 0.12)
            K.rot(b, "Y", s * 22)
            K.move(b, s * 0.2, -0.07 + 0.035 * k, 0.6)
            K.mesh_obj("ptshoulder", b, "leather", bevel=0.0012)


@item("vambraces", (2, 2), name="Ламеллярные наручи", pose="flat", azimuth=20, elevation=10, margin=0.9)
def vambraces():
    """Пара ламеллярных наручей: железные шины на кожаной основе с ремнями."""
    mats_common()
    K.metal_mat("lames", (150, 150, 154), (90, 88, 86), rough=(0.3, 0.55), rust=0.3, dirt=0.9, scale=3.0)
    import bmesh
    for side, ox in ((-1, -0.065), (1, 0.065)):
        L = 0.24
        r0, r1 = 0.04, 0.052

        def rad(z):
            return r0 + (r1 - r0) * (z / L)

        base = K.partial_lathe([(0.0, r0 * 0.98, r0 * 0.85), (L, r1 * 0.98, r1 * 0.85)], -150, 150, 30)
        K.mesh_obj("base", base, "leather_dark", solidify=0.003)
        sp = bmesh.new()
        for i in range(11):
            a = math.radians(-125 + 250 * i / 10)
            b = K.cbox(0, 0, 0, 0.019, 0.0024, L * 0.96)
            K.deform(b, lambda v: (v.x * (1 + 0.25 * (v.z / L)), v.y, v.z))
            K.move(b, 0, 0, L / 2)
            nrm = Vector((math.sin(a), -math.cos(a) * 0.85, 0)).normalized()
            q = Vector((0, -1, 0)).rotation_difference(nrm)
            K.transform(b, q.to_matrix().to_4x4())
            K.deform(b, lambda v, a=a: (v.x + math.sin(a) * rad(v.z) * 1.03, v.y - math.cos(a) * rad(v.z) * 0.88, v.z))
            me = K.bpy.data.meshes.new("t")
            b.to_mesh(me)
            b.free()
            sp.from_mesh(me)
            K.bpy.data.meshes.remove(me)
        K.mesh_obj("splints", sp, "lames", bevel=0.0007)
        for z in (0.03, 0.12, 0.21):
            r = rad(z)
            st = K.partial_lathe([(z - 0.008, r * 1.1, r * 0.95), (z + 0.008, r * 1.1, r * 0.95)], -160, 160, 30)
            K.mesh_obj("strap", st, "leather", solidify=0.003)
            K.mesh_obj("buckle", K.cbox(math.sin(math.radians(150)) * r * 1.12, math.cos(math.radians(150)) * -r * 0.97, z, 0.012, 0.004, 0.02), "bronze", bevel=0.001)
        # Сдвигаем пару в стороны и чуть разворачиваем.
        for o in K.PARTS:
            if not o.get("placed"):
                o.data.transform(K.Matrix.Translation((ox, 0, 0)) @ K.Matrix.Rotation(math.radians(side * 8), 4, "Y"))
                o["placed"] = True


def hand(mat, side):
    """Перчатка (правая при side=1): ладонь, четыре пальца, большой палец, раструб.
    Тыльная сторона смотрит в -Y, пальцы вверх (+Z)."""
    import bmesh
    parts = []
    palm = K.loft([(0, 0, 0.0), (0, 0, 0.05), (0, 0.002, 0.098)], [(0.042, 0.016), (0.048, 0.015), (0.046, 0.013)], 20, up=(0, -1, 0))
    parts.append(palm)
    fingers = [(-0.031, 0.072), (-0.011, 0.082), (0.009, 0.078), (0.028, 0.064)]
    for x, l in fingers:
        path = [Vector((x * 1.0, 0.0, 0.088))]
        for i in range(1, 7):
            t = i / 6
            path.append(Vector((x * (1 + 0.05 * t), 0.02 * t * t, 0.088 + l * t * (1 - 0.08 * t))))
        radii = [(0.0136 * (1 - 0.12 * i / 6), 0.0112 * (1 - 0.12 * i / 6)) for i in range(7)]
        parts.append(K.loft(path, radii, 12, up=(0, -1, 0)))
    tpath = [Vector((-0.036, 0.004, 0.02)), Vector((-0.055, 0.0, 0.045)), Vector((-0.068, -0.004, 0.07)), Vector((-0.074, -0.004, 0.092))]
    parts.append(K.loft(tpath, [(0.013, 0.011), (0.012, 0.0105), (0.011, 0.0095), (0.0095, 0.0085)], 12, up=(0, -1, 0)))
    cuff = K.lathe([(-0.09, 0.056, 0.036), (-0.04, 0.047, 0.026), (0.005, 0.041, 0.018)], 24, cap_top=False, cap_bot=False)
    bm = K.merge(*parts)
    if side < 0:
        bm = K.mirror_x(bm)
        cuff = K.mirror_x(cuff)
    return bm, cuff


@item("gloves", (2, 2), name="Кожаные перчатки", azimuth=12, elevation=8, margin=0.9)
def gloves():
    """Пара кожаных перчаток с раструбами."""
    mats_common()
    for side, ox, rz in ((-1, -0.055, 8), (1, 0.06, -6)):
        bm, cuff = hand("leather", side)
        for b in (bm, cuff):
            K.rot(b, "Y", rz)
            K.move(b, ox, (0.01 if side > 0 else 0), 0.09 if side > 0 else 0.0)
        K.mesh_obj("glove", bm, "leather", subsurf=1, smooth_angle=80)
        K.mesh_obj("cuff", cuff, "leather_dark", solidify=0.003, smooth_angle=80)


@item("greaves", (2, 3), name="Железные поножи", azimuth=18, elevation=8, margin=0.92)
def greaves():
    """Пара железных поножей по форме голени с ремнями на икрах."""
    mats_common()
    for s in (-1, 1):
        rings = []
        for i in range(21):
            t = i / 20
            z = 0.4 * t
            r = 0.043 + 0.022 * math.sin(math.pi * min(1, t * 1.25) ** 1.3) * (1 if t < 0.85 else 1 - (t - 0.85) * 1.6)
            if t > 0.9:
                r += 0.007 * ((t - 0.9) / 0.1) ** 2
            rings.append((z, r * 1.02, r * 0.95))
        sh = K.partial_lathe(rings, -105, 105, 32)
        K.deform(sh, lambda v: (v.x + s * 0.07, v.y - 0.012 * math.sin(math.pi * v.z / 0.4), v.z))
        K.mesh_obj("greave", sh, "iron", solidify=0.0022, smooth_angle=70, bevel=0.0)
        # Ребро по голени.
        path = [(s * 0.07, -rings[i][2] - 0.0005 - 0.012 * math.sin(math.pi * rings[i][0] / 0.4), rings[i][0]) for i in range(1, 20)]
        K.mesh_obj("rib", K.loft(path, [(0.0035, 0.0015)] * len(path), 8, up=(0, -1, 0)), "iron", subsurf=1)
        for z in (0.07, 0.25):
            r = [rr for zz, rr, _ in rings if zz >= z][0]
            st = K.partial_lathe([(z - 0.01, r * 1.04, r * 0.99), (z + 0.01, r * 1.04, r * 0.99)], 100, 260, 20)
            K.deform(st, lambda v: (v.x + s * 0.07, v.y, v.z))
            K.mesh_obj("strap", st, "leather", solidify=0.003)


@item("boots", (2, 2), name="Кожаные сапоги", pose="upright", azimuth=30, elevation=14, margin=0.9)
def boots():
    """Пара кожаных сапог до середины икры с завязками (цангии)."""
    mats_common()
    for s, ox, oy, rz in ((-1, -0.07, 0.02, -8), (1, 0.07, -0.02, 14)):
        foot_path = [Vector((0, 0.085, 0.042)), Vector((0, 0.03, 0.04)), Vector((0, -0.045, 0.032)), Vector((0, -0.11, 0.022)), Vector((0, -0.15, 0.017))]
        radii = [(0.036, 0.042), (0.04, 0.042), (0.043, 0.034), (0.038, 0.024), (0.022, 0.016)]
        foot = K.loft(foot_path, radii, 20, up=(0, 0, 1))
        K.deform(foot, lambda v: (v.x, v.y, max(v.z, 0.003) if v.z < 0.01 else v.z))
        shaft = K.lathe([(0.02, 0.04, 0.046), (0.1, 0.041, 0.045), (0.18, 0.046, 0.049), (0.24, 0.05, 0.052)], 24, cap_top=False, cap_bot=False)
        K.move(shaft, 0, 0.06, 0)
        K.deform(shaft, lambda v: (v.x * (1 + 0.04 * math.sin(v.z * 60) * (v.z / 0.24)), v.y, v.z + 0.004 * math.sin(math.atan2(v.x, v.y - 0.06) * 3) * (v.z / 0.24)))
        for b in (foot, shaft):
            K.rot(b, "Z", rz)
            K.move(b, ox, oy, 0)
        K.mesh_obj("foot", foot, "leather", subsurf=1, smooth_angle=80)
        K.mesh_obj("shaft", shaft, "leather", solidify=0.004, subsurf=1, smooth_angle=80)
        # Подошва и завязки.
        sole = K.loft(foot_path, [(r[0] * 1.03, 0.006) for r in radii], 20, up=(0, 0, 1))
        K.deform(sole, lambda v: (v.x, v.y, 0.006 + (v.z - 0.04) * 0.15))
        K.rot(sole, "Z", rz)
        K.move(sole, ox, oy, 0)
        K.mesh_obj("sole", sole, "leather_dark", subsurf=1)
        for z in (0.2, 0.15):
            t = K.torus((0, 0.06, z), 0.049, 0.0028, ry=0.051)
            K.rot(t, "Z", rz)
            K.move(t, ox, oy, 0)
            K.mesh_obj("tie", t, "cord")


# =========================================================================== украшения и мелочь


@item("signet_ring", (1, 1), name="Серебряный перстень", azimuth=28, elevation=22, margin=0.82)
def signet_ring():
    """Серебряный перстень-печатка со вставкой сердолика."""
    mats_common()
    K.gem_mat("carnelian", (178, 60, 26), transmission=0.3, rough=0.1)
    K.mesh_obj("hoop", K.torus((0, 0, 0), 0.0105, 0.0018, plane="XZ", segs=12, tube_ry=0.0026), "silver", subsurf=1)
    b = K.lathe([(0.0098, 0.0075, 0.0062), (0.0125, 0.0085, 0.007), (0.014, 0.0082, 0.0068)], 32)
    K.mesh_obj("bezel", b, "silver", subsurf=1, bevel=0.0004)
    g = K.lathe([(0.0138, 0.0068, 0.0055), (0.0148, 0.0062, 0.005), (0.0153, 0.0001)], 32)
    K.mesh_obj("stone", g, "carnelian", subsurf=1)


@item("pectoral_cross", (1, 1), name="Нательный крест", azimuth=18, elevation=12, margin=0.88)
def pectoral_cross():
    """Нательный золотой крест с расширяющимися концами и гранатом в средокрестии, на шнурке."""
    mats_common()
    import bmesh
    for ang in (0, 90, 180, 270):
        st = [(0.0, 0.0045, 0.0018), (0.015, 0.0055, 0.0017), (0.022, 0.009, 0.0016), (0.0235, 0.0092, 0.0012)]
        arm = K.blade(st, 6, edge=0.0008)
        K.rot(arm, "Y", ang)
        K.mesh_obj("arm", arm, "gold", bevel=0.0003)
    K.mesh_obj("centre", K.lathe([(-0.002, 0.0062), (0.002, 0.0062)], 24, axis="Y"), "gold", bevel=0.0004)
    cab = K.lathe([(0, 0.0045), (0.0012, 0.004), (0.0022, 0.0022), (0.0026, 0.0001)], 16)
    K.rot(cab, "X", 90)
    K.move(cab, 0, -0.0018, 0)
    K.mesh_obj("garnet", cab, "garnet", subsurf=1)
    K.mesh_obj("loop", K.torus((0, 0, 0.0265), 0.0028, 0.0009, plane="YZ"), "gold")
    # Шнурок: петля вверх.
    path = []
    for i in range(48):
        a = 2 * math.pi * i / 48
        path.append((0.022 * math.sin(a), 0.002 * math.sin(2 * a), 0.0265 + 0.026 * (1 - math.cos(a)) * 0.95))
    K.mesh_obj("cord", K.loft(path, [0.0011] * len(path), 8, up=(0, 1, 0), closed=True), "leather_dark")


@item("bandage", (1, 1), name="Бинт", pose="upright", azimuth=24, elevation=28, margin=0.88)
def bandage():
    """Скатка льняного бинта с размотанным концом."""
    mats_common()
    K.cloth_mat("bandage_linen", (222, 212, 188), (180, 166, 138), weave=500.0, dirt=0.4, bump=0.6)
    rings = [(0, 0.006), (0, 0.03)]
    for i in range(1, 12):
        rings.append((0.002 * (i % 2) + 0.0, 0.03 - i * 0.0022))
    prof = [(0.0, 0.007), (0.0, 0.034), (0.042, 0.034), (0.042, 0.007), (0.0, 0.007)]
    import bmesh
    roll = K.lathe(prof, 48, cap_top=False, cap_bot=False)
    bmesh.ops.remove_doubles(roll, verts=roll.verts[:], dist=1e-6)
    K.deform(roll, lambda v: (v.x * (1 + 0.03 * math.sin(v.z * 300)), v.y * (1 + 0.03 * math.sin(v.z * 300)), v.z + (0.0008 * math.sin(math.hypot(v.x, v.y) * 1400) if v.z > 0.038 or v.z < 0.004 else 0)))
    K.mesh_obj("roll", roll, "bandage_linen", smooth_angle=60, bevel=0.003)
    # Размотанный конец.
    path = []
    for i in range(25):
        t = i / 24
        a = -0.3 + t * 1.2
        path.append((0.0345 * math.cos(a) + 0.05 * t * t, -0.0345 * math.sin(a) - 0.055 * t, 0.021 * (1 - t) ** 2 + 0.001))
    tail = K.loft(path, [(0.0007, 0.02)] * 25, 6, up=(0, 0, 1))
    K.deform(tail, lambda v: (v.x, v.y, max(v.z, 0.0008) + 0.002 * math.sin(v.x * 300)))
    K.mesh_obj("tail", tail, "bandage_linen", smooth_angle=70)
    # Скатку перевязывают тонкой полоской того же полотна.
    K.mesh_obj("band", K.torus((0, -0.0205, 0.021), 0.0148, 0.0011, plane="YZ", ry=0.0228, tube_ry=0.0045), "bandage_linen")


@item("flask", (1, 2), name="Целебное зелье", pose="upright", azimuth=20, elevation=10, margin=0.9)
def flask():
    """Стеклянный флакон с лекарством: шаровидное тело, длинное горло, пробка под воском."""
    mats_common()
    K.glass_mat("glass", (205, 228, 205))
    K.liquid_mat("tincture", (120, 18, 14))
    prof = [(0.0, 0.0001), (0.003, 0.03), (0.01, 0.044), (0.03, 0.052), (0.055, 0.05), (0.075, 0.04), (0.09, 0.022), (0.1, 0.0125), (0.15, 0.0105), (0.156, 0.0145), (0.161, 0.0125)]
    K.mesh_obj("glass", K.lathe(prof, 40, cap_top=False), "glass", solidify=0.0018, subsurf=1, glass=True, smooth_angle=89)
    liq = [(0.004, 0.0001), (0.006, 0.028), (0.012, 0.04), (0.03, 0.048), (0.055, 0.046), (0.068, 0.04)]
    K.mesh_obj("liquid", K.lathe(liq, 40), "tincture", subsurf=1, smooth_angle=89)
    K.mesh_obj("cork", K.lathe([(0.14, 0.0095), (0.165, 0.0105)], 16), "wood", bevel=0.001)
    # Пробку заливают воском и обвязывают бечёвкой.
    K.organic_mat("wax", (120, 34, 26), (80, 20, 16), rough=(0.3, 0.5), bump=0.3, bump_scale=60.0, dirt=0.3)
    seal = K.lathe([(0.152, 0.0152), (0.158, 0.0168), (0.166, 0.0145), (0.171, 0.009), (0.173, 0.0001)], 32)
    K.deform(seal, lambda v: (v.x * (1 + 0.08 * math.sin(math.atan2(v.y, v.x) * 5)), v.y * (1 + 0.08 * math.sin(math.atan2(v.y, v.x) * 5)), v.z - 0.004 * max(0, math.sin(math.atan2(v.y, v.x) * 3)) * max(0, (0.16 - v.z) / 0.008)))
    K.mesh_obj("seal", seal, "wax", subsurf=1, smooth_angle=89)
    K.mesh_obj("tie", K.torus((0, 0, 0.146), 0.0118, 0.0013), "cord")
    K.mesh_obj("tie2", K.torus((0, 0, 0.142), 0.0116, 0.0013), "cord")


@item("ruby", (1, 1), name="Рубин", pose="upright", azimuth=24, elevation=30, margin=0.8)
def ruby():
    """Полированный рубин-кабошон (огранки в VI веке ещё не знали)."""
    mats_common()
    K.gem_mat("ruby", (170, 8, 30), transmission=0.7, rough=0.03)
    rings = []
    for i in range(13):
        t = i / 12
        rings.append((0.011 * math.sin(t * math.pi / 2) ** 0.8, 0.012 * math.cos(t * math.pi / 2) ** 0.6 + 1e-4, 0.0095 * math.cos(t * math.pi / 2) ** 0.6 + 1e-4))
    rings = [(0.0, 0.0105, 0.0083)] + rings
    g = K.lathe(rings, 48)
    K.deform(g, lambda v: (v.x * (1 + 0.04 * math.sin(v.y * 600)), v.y, v.z))
    K.mesh_obj("ruby", g, "ruby", subsurf=1, smooth_angle=89)


@item("sack", (1, 1), name=None, pose="upright", azimuth=24, elevation=16, margin=0.88)
def sack():
    """Холщовый мешочек на шнурке — запасная модель для предметов без своей."""
    mats_common()
    rings = [(0.0, 0.0001), (0.004, 0.03), (0.02, 0.05), (0.045, 0.055), (0.07, 0.045), (0.088, 0.02), (0.094, 0.017), (0.105, 0.026), (0.112, 0.03)]
    b = K.lathe(rings, 40, cap_top=False)
    K.deform(b, lambda v: (v.x * (1 + 0.06 * math.sin(math.atan2(v.y, v.x) * 9 + v.z * 40) * min(1, v.z / 0.06)), v.y * (1 + 0.06 * math.sin(math.atan2(v.y, v.x) * 9 + v.z * 40) * min(1, v.z / 0.06)), v.z))
    K.mesh_obj("sack", b, "linen", solidify=0.002, subsurf=1, smooth_angle=89)
    K.mesh_obj("cord", K.torus((0, 0, 0.091), 0.0185, 0.0022), "cord")


# =========================================================================== запуск


import items_goods  # noqa: E402,F401  (товары, травы, посуда, книги, снадобья)
import items_arsenal  # noqa: E402,F401  (кованое оружие арсенала)
import items_archery  # noqa: E402,F401  (луки, стрелы и их части)
import items_home  # noqa: E402,F401  (домашняя утварь горожан)
import items_fishing  # noqa: E402,F401  (снасти и рыба)
import items_coins  # noqa: E402,F401  (золотые, серебряные и медные монеты)


def build(key):
    spec = ITEMS[key]
    K.reset_scene()
    K.PARTS.clear()
    spec["fn"]()
    K.setup_studio()
    K.icon_camera(K.PARTS, spec["cells"], spec["azimuth"], spec["elevation"], spec["margin"], spec["roll"])
    K.render_icon(os.path.join(K.OUT_ICONS, key + ".png"))
    w, h = spec["cells"]
    # Текстуры не меньше 1024 (правило проекта), даже у мелких предметов.
    bake = max(K.BAKE_SIZE, 1024)
    _ = (w, h)
    K.bake_and_export(key, spec["pose"], bake_size=bake, squash=spec["squash"])
    print(f"[items] {key}: готово")


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    keys = args or list(ITEMS)
    for k in keys:
        if k not in ITEMS:
            raise SystemExit(f"нет предмета {k}; есть: {', '.join(ITEMS)}")
        build(k)
    K.write_art_table()


if __name__ == "__main__":
    main()
