"""Домашняя утварь горожан Херсона около 510 г.: то, что стоит на столах и полках в домах
и что можно взять (и украсть).

Образцы — находки VI в.: светильники позднеримских типов с крестом на щитке, краснолаковые
миски (понтийская и фокейская посуда), стеклянные кубки на ножке, бронзовые кувшины,
серебряные ложки-кохлеарии, двусторонние костяные гребни, бронзовые зеркальца,
паломнические ампулы святого Мины (Абу-Мина, Египет), энколпионы, игральные кости,
детские лошадки на колёсиках, веретёна с глиняными пряслицами, пружинные ножницы.
Все модели собраны так, как вещь лежит на столе (pose="upright").
"""
import math

import itemkit as K
from itemkit import item

M = K.common_materials


def at(bm, x=0.0, y=0.0, z=0.0):
    """Сдвинуть bmesh (положение задаём вершинам, а не объекту: так его видит запекание)."""
    K.move(bm, x, y, z)
    return bm


def dark_mats():
    K.organic_mat("bone_dark", (120, 96, 66), (70, 52, 34), rough=(0.5, 0.7), bump=0.1, dirt=0.6)
    K.metal_mat("bronze_dark", (96, 70, 40), (60, 44, 24), rough=(0.4, 0.6), dirt=0.9, tarnish=((50, 70, 56), 0.7))


def _clay(name="lamp_clay", color=(178, 104, 66)):
    return K.organic_mat(name, color, tuple(int(c * 0.72) for c in color), rough=(0.6, 0.85), bump=0.25, bump_scale=90.0, dirt=0.8, wear=0.4, wear_color=tuple(min(255, int(c * 1.15)) for c in color))


def _red_slip():
    """Краснолаковый ангоб: гладкий, чуть блестящий, красно-оранжевый."""
    return K.organic_mat("red_slip", (176, 74, 44), (120, 46, 26), rough=(0.32, 0.5), bump=0.1, bump_scale=60.0, dirt=0.6, wear=0.5, wear_color=(196, 120, 84))


def _cross_relief(z, size, mat, t=0.0025):
    """Крест рельефом на плоской поверхности (вид сверху)."""
    K.mesh_obj("cross", K.cbox(0, 0, z + t / 2, size, size * 0.22, t), mat, bevel=0.0008)
    K.mesh_obj("cross", K.cbox(0, 0, z + t / 2, size * 0.22, size, t), mat, bevel=0.0008)


# =========================================================================== светильники


def _lamp_body(mat, nozzle=0.075, r=0.038, h=0.028):
    body = K.lathe([(0.0, 0.0001), (0.0, r * 0.55), (0.003, r * 0.62), (h * 0.35, r * 0.98), (h * 0.7, r), (h * 0.95, r * 0.85), (h, r * 0.7), (h - 0.002, r * 0.55), (h - 0.004, 0.0001)], 40)
    K.scale(body, 1.0, 0.86, 1.0)
    K.mesh_obj("body", body, mat, subsurf=1, smooth_angle=70)
    # Рожок с отверстием для фитиля.
    path = [(r * 0.6, 0, h * 0.5), (r * 0.95, 0, h * 0.52), (nozzle - 0.006, 0, h * 0.55)]
    K.mesh_obj("nozzle", K.loft(path, [(0.013, 0.009), (0.011, 0.008), (0.01, 0.008)], 16, up=(0, 0, 1)), mat, subsurf=1)
    K.mesh_obj("wickhole", at(K.lathe([(h * 0.55 + 0.0075, 0.0045), (h * 0.55 + 0.0082, 0.0001)], 12), nozzle - 0.008), "soot")
    return h


@item("clay_lamp", (1, 1), name="Глиняный светильник", pose="upright", azimuth=30, elevation=34, margin=0.86)
def clay_lamp():
    """Литой светильник VI в.: овальный, с рожком, ручкой-шишечкой и крестом на щитке."""
    M()
    _clay()
    K.organic_mat("soot", (34, 26, 22), (18, 14, 12), rough=(0.8, 0.95), bump=0.1, dirt=0.2)
    h = _lamp_body("lamp_clay")
    # Ручка-шишечка сзади.
    knob = K.lathe([(0.0, 0.0001), (0.0, 0.006), (0.008, 0.007), (0.016, 0.004), (0.018, 0.0001)], 16)
    K.rot(knob, "Y", -90)
    K.mesh_obj("knob", at(knob, -0.03, 0, h * 0.55), "lamp_clay", subsurf=1)
    # Щиток с наливным отверстием и рельефным крестом.
    K.mesh_obj("discus", K.torus((0, 0, h - 0.001), 0.022, 0.0022), "lamp_clay", )
    K.mesh_obj("fillhole", at(K.lathe([(h - 0.0005, 0.005), (h + 0.0004, 0.0001)], 12), 0.012), "soot")
    K.mesh_obj("cross", K.cbox(-0.008, 0, h + 0.0005, 0.02, 0.004, 0.002), "lamp_clay", bevel=0.0006)
    K.mesh_obj("cross", K.cbox(-0.008, 0, h + 0.0005, 0.004, 0.02, 0.002), "lamp_clay", bevel=0.0006)


@item("bronze_lamp", (1, 1), name="Бронзовый светильник", pose="upright", azimuth=30, elevation=26, margin=0.88)
def bronze_lamp():
    """Бронзовый светильник на поддоне с крышкой на шарнире и ручкой-крестом."""
    M()
    K.organic_mat("soot", (34, 26, 22), (18, 14, 12), rough=(0.8, 0.95), bump=0.1, dirt=0.2)
    K.mesh_obj("foot", K.lathe([(0.0, 0.0001), (0.0, 0.022), (0.004, 0.024), (0.008, 0.016), (0.012, 0.012)], 32), "bronze", subsurf=1)
    lift = 0.012
    body = K.lathe([(0.0, 0.0001), (0.0, 0.02), (0.006, 0.034), (0.016, 0.038), (0.026, 0.032), (0.03, 0.02), (0.031, 0.0001)], 40)
    K.scale(body, 1.0, 0.82, 1.0)
    K.move(body, 0, 0, lift)
    K.mesh_obj("body", body, "bronze", subsurf=1)
    path = [(0.025, 0, lift + 0.016), (0.05, 0, lift + 0.018), (0.072, 0, lift + 0.02)]
    K.mesh_obj("nozzle", K.loft(path, [(0.012, 0.008), (0.009, 0.007), (0.009, 0.007)], 16, up=(0, 0, 1)), "bronze", subsurf=1)
    K.mesh_obj("wick", at(K.lathe([(lift + 0.026, 0.004), (lift + 0.027, 0.0001)], 10), 0.068), "soot")
    K.mesh_obj("lid", K.lathe([(lift + 0.03, 0.012), (lift + 0.034, 0.011), (lift + 0.036, 0.004), (lift + 0.04, 0.0001)], 20), "bronze", subsurf=1)
    # Ручка — дуга сзади с крестом наверху.
    hp = K.bezier((-0.03, 0, lift + 0.02), (-0.05, 0, lift + 0.03), (-0.05, 0, lift + 0.06), (-0.036, 0, lift + 0.07), n=12)
    K.mesh_obj("handle", K.loft(hp, [0.003] * len(hp), 8, up=(0, 1, 0)), "bronze", subsurf=1)
    K.mesh_obj("crossv", K.cbox(-0.036, 0, lift + 0.084, 0.005, 0.003, 0.032), "bronze", bevel=0.0008)
    K.mesh_obj("crossh", K.cbox(-0.036, 0, lift + 0.09, 0.022, 0.003, 0.005), "bronze", bevel=0.0008)


# =========================================================================== посуда


@item("red_slip_bowl", (1, 1), name="Краснолаковая миска", pose="upright", azimuth=24, elevation=34, margin=0.88)
def red_slip_bowl():
    """Фокейская краснолаковая миска (форма 3): широкая, с вертикальным бортиком,
    крест-штамп на дне."""
    M()
    _red_slip()
    prof = [(0.0, 0.0001), (0.0, 0.038), (0.008, 0.04), (0.012, 0.05), (0.03, 0.088), (0.034, 0.1), (0.05, 0.102), (0.052, 0.1)]
    K.mesh_obj("bowl", K.lathe(prof, 56, cap_top=False), "red_slip", solidify=0.004, subsurf=1, smooth_angle=60)
    # Насечки-рулетка по бортику.
    for i in range(48):
        a = 2 * math.pi * i / 48
        n = K.cbox(0.1035, 0, 0.042, 0.002, 0.0035, 0.004)
        K.rot(n, "Z", math.degrees(a))
        K.mesh_obj("notch", n, "red_slip")
    _cross_relief(0.012, 0.026, "red_slip", t=0.0015)


@item("clay_cup", (1, 1), name="Глиняная кружка", pose="upright", azimuth=24, elevation=18, margin=0.88)
def clay_cup():
    """Кружка с одной ручкой, следы гончарного круга."""
    M()
    _clay("cup_clay", (166, 108, 72))
    prof = [(0.0, 0.0001), (0.0, 0.026), (0.004, 0.03), (0.03, 0.038), (0.06, 0.04), (0.085, 0.036), (0.095, 0.038), (0.097, 0.037)]
    body = K.lathe(prof, 40, cap_top=False)
    K.deform(body, lambda v: (v.x * (1 + 0.012 * math.sin(v.z * 420)), v.y * (1 + 0.012 * math.sin(v.z * 420)), v.z))
    K.mesh_obj("cup", body, "cup_clay", solidify=0.004, subsurf=1, smooth_angle=70)
    hp = K.bezier((0.035, 0, 0.082), (0.07, 0, 0.085), (0.072, 0, 0.03), (0.038, 0, 0.025), n=14)
    K.mesh_obj("handle", K.loft(hp, [(0.007, 0.004)] * len(hp), 10, up=(0, 1, 0)), "cup_clay", subsurf=1)


@item("glass_goblet", (1, 1), name="Стеклянный кубок", pose="upright", azimuth=20, elevation=14, margin=0.86)
def glass_goblet():
    """Кубок на ножке — самая частая стеклянная посуда VI–VII вв."""
    M()
    K.glass_mat("glass", (184, 214, 178))
    prof = [(0.0, 0.0001), (0.0, 0.03), (0.003, 0.032), (0.004, 0.006), (0.04, 0.004), (0.046, 0.012), (0.06, 0.028), (0.085, 0.036), (0.1, 0.037)]
    K.mesh_obj("goblet", K.lathe(prof, 40, cap_top=False), "glass", solidify=0.0018, subsurf=1, glass=True, smooth_angle=89)
    K.mesh_obj("knop", K.torus((0, 0, 0.022), 0.006, 0.003), "glass", glass=True)


@item("bronze_ewer", (1, 2), name="Бронзовый кувшин", pose="upright", azimuth=22, elevation=10, margin=0.9)
def bronze_ewer():
    """Бронзовый кувшин на кольцевом поддоне, с высоким горлом и литой ручкой."""
    M()
    prof = [(0.0, 0.0001), (0.0, 0.04), (0.012, 0.038), (0.016, 0.03), (0.03, 0.05), (0.07, 0.066), (0.11, 0.058), (0.15, 0.03), (0.2, 0.022), (0.23, 0.028), (0.24, 0.034), (0.242, 0.032)]
    body = K.lathe(prof, 48, cap_top=False)
    # Носик: вытягиваем край горла вперёд.
    K.deform(body, lambda v: (v.x + max(0.0, v.z - 0.22) * 0.8 * max(0.0, v.x) / 0.03, v.y, v.z))
    K.mesh_obj("ewer", body, "bronze", solidify=0.002, subsurf=1, smooth_angle=60)
    hp = K.bezier((-0.026, 0, 0.225), (-0.09, 0, 0.25), (-0.1, 0, 0.12), (-0.06, 0, 0.09), n=16)
    K.mesh_obj("handle", K.loft(hp, [(0.007, 0.005)] * len(hp), 10, up=(0, 1, 0)), "bronze", subsurf=1)
    for z in (0.07, 0.11):
        K.mesh_obj("band", K.torus((0, 0, z), 0.066 if z < 0.09 else 0.058, 0.0015), "bronze")


@item("silver_spoon", (1, 1), name="Серебряная ложка", pose="upright", azimuth=30, elevation=42, margin=0.9)
def silver_spoon():
    """Ложка-кохлеарий: грушевидная чаша, ступенька-диск, длинная ручка."""
    M()
    bowl = K.lathe([(0.0, 0.0001), (0.0, 0.006), (0.004, 0.014), (0.008, 0.018), (0.009, 0.0185)], 32, cap_top=False)
    K.scale(bowl, 1.0, 0.68, 1.0)
    K.deform(bowl, lambda v: (v.x * (1.25 if v.x > 0 else 0.9), v.y, v.z))
    K.mesh_obj("bowl", bowl, "silver", solidify=0.0012, subsurf=1, smooth_angle=70)
    K.mesh_obj("disc", at(K.lathe([(0.0, 0.0001), (0.0, 0.006), (0.003, 0.006), (0.003, 0.0001)], 20), -0.024, 0, 0.008), "silver", subsurf=1)
    hp = [(-0.024, 0, 0.009), (-0.07, 0, 0.012), (-0.13, 0, 0.004)]
    K.mesh_obj("handle", K.loft(hp, [0.0022, 0.0018, 0.0011], 10), "silver", subsurf=1)


@item("kitchen_knife", (1, 2), name="Кухонный нож", pose="upright", azimuth=24, elevation=50, margin=0.92, roll=90)
def kitchen_knife():
    """Железный нож с горбатой спинкой и деревянной рукоятью на заклёпках."""
    M()
    st = []
    for i in range(13):
        t = i / 12
        spine = 0.011 * (1 - max(0.0, (t - 0.55) / 0.45) ** 2 * 0.92)
        edge = -0.011 + 0.0118 * t ** 2.2
        st.append((-0.006 + 0.136 * t, max((spine - edge) / 2, 0.0003), 0.0013 * (1 - 0.6 * t), (spine + edge) / 2))
    blade = K.blade(st, 10, edge=0.0003)
    K.deform(blade, lambda v: (v.z, v.x, v.y + 0.009))
    K.mesh_obj("blade", blade, "iron", smooth_angle=50)
    handle = K.cbox(-0.047, 0.0, 0.009, 0.094, 0.02, 0.018)
    K.mesh_obj("handle", handle, "wood_dark", bevel=0.004, subsurf=1)
    K.mesh_obj("bolster", K.cbox(-0.001, 0.0, 0.009, 0.004, 0.021, 0.0185), "iron", bevel=0.001)
    for x in (-0.075, -0.03):
        K.mesh_obj("rivet", at(K.lathe([(0.0, 0.0001), (0.0, 0.0025), (0.019, 0.0025), (0.019, 0.0001)], 10), x, 0.0), "bronze")


# =========================================================================== еда


@item("bread_loaf", (1, 1), name="Каравай хлеба", pose="upright", azimuth=24, elevation=30, margin=0.86)
def bread_loaf():
    """Круглый хлеб, разделённый надрезами на доли, с оттиском хлебного штампа."""
    M()
    K.organic_mat("crust", (168, 104, 50), (110, 62, 26), rough=(0.6, 0.85), bump=0.6, bump_scale=60.0, dirt=0.3, wear=0.6, wear_color=(206, 160, 100))
    prof = [(0.0, 0.0001), (0.0, 0.08), (0.006, 0.088), (0.02, 0.092), (0.04, 0.085), (0.058, 0.06), (0.068, 0.03), (0.071, 0.0001)]
    loaf = K.lathe(prof, 48)
    # Восемь долей: надрезы по радиусам.
    K.deform(loaf, lambda v: (v.x, v.y, v.z - 0.006 * max(0.0, math.cos(4 * math.atan2(v.y, v.x))) ** 40 * min(1.0, v.z / 0.05) * min(1.0, math.hypot(v.x, v.y) / 0.02)))
    K.mesh_obj("loaf", loaf, "crust", subsurf=1, smooth_angle=80)
    K.mesh_obj("stamp", K.torus((0, 0, 0.0705), 0.012, 0.0018), "crust")


@item("cheese_wheel", (1, 1), name="Головка сыра", pose="upright", azimuth=24, elevation=26, margin=0.86)
def cheese_wheel():
    """Овечий сыр головкой, с отпечатком плетёной формы на корке."""
    M()
    K.organic_mat("rind", (214, 188, 128), (168, 138, 84), rough=(0.5, 0.75), bump=0.5, bump_scale=120.0, dirt=0.6, wear=0.3)
    prof = [(0.0, 0.0001), (0.0, 0.07), (0.006, 0.078), (0.03, 0.082), (0.055, 0.078), (0.061, 0.07), (0.062, 0.0001)]
    wheel = K.lathe(prof, 48)
    K.deform(wheel, lambda v: (v.x * (1 + 0.012 * math.sin(v.z * 700)), v.y * (1 + 0.012 * math.sin(v.z * 700)), v.z))
    K.mesh_obj("cheese", wheel, "rind", subsurf=1, smooth_angle=70)


@item("dried_figs", (1, 1), name="Сушёные смоквы", pose="upright", azimuth=24, elevation=40, margin=0.88)
def dried_figs():
    """Сушёные смоквы, нанизанные кольцом на тростинку (так их возили и хранили)."""
    M()
    K.organic_mat("fig", (122, 80, 46), (70, 42, 22), rough=(0.5, 0.8), bump=0.6, bump_scale=200.0, dirt=0.4, wear=0.3, wear_color=(170, 130, 90))
    ring_r = 0.045
    n = 11
    for i in range(n):
        a = 2 * math.pi * i / n
        f = K.lathe([(0.0, 0.0001), (0.0, 0.006), (0.004, 0.014), (0.01, 0.015), (0.016, 0.01), (0.02, 0.004), (0.021, 0.0001)], 14)
        K.scale(f, 1.0, 1.0, 0.55)
        K.rot(f, "Y", 90)
        K.deform(f, lambda v, s=i: (v.x * (1 + 0.15 * math.sin(v.y * 300 + s)), v.y, v.z * (1 + 0.1 * math.cos(v.x * 260 + s))))
        K.move(f, 0, 0, 0.012)
        K.rot(f, "Z", math.degrees(a) + 90)
        K.move(f, ring_r * math.cos(a), ring_r * math.sin(a), 0)
        K.mesh_obj("fig", f, "fig", subsurf=1, smooth_angle=80)
    K.mesh_obj("reed", K.torus((0, 0, 0.012), ring_r, 0.0018), "cord")


# =========================================================================== для себя


@item("bone_comb", (1, 1), name="Костяной гребень", pose="upright", azimuth=24, elevation=48, margin=0.88)
def bone_comb():
    """Двусторонний костяной гребень: частые и редкие зубья, накладки на бронзовых заклёпках."""
    M()
    dark_mats()
    w, d, t = 0.1, 0.052, 0.006
    K.mesh_obj("core", K.cbox(0, 0, t / 2, w, 0.014, t), "bone", bevel=0.0008)
    for side, n, gap in ((1, 34, 0.0008), (-1, 22, 0.0016)):
        pitch = w / n
        for i in range(n):
            x = -w / 2 + pitch * (i + 0.5)
            tooth = K.box(x - pitch / 2 + gap / 2, x + pitch / 2 - gap / 2, 0.007 if side > 0 else -d / 2, d / 2 if side > 0 else -0.007, 0.0005, t - 0.0005)
            K.mesh_obj("tooth", tooth, "bone")
    for s in (-1, 1):
        K.mesh_obj("plate", K.cbox(0, 0, t / 2 + s * (t / 2 + 0.0008), w * 0.96, 0.016, 0.0016), "bone", bevel=0.0006)
    for x in (-0.04, -0.013, 0.013, 0.04):
        K.mesh_obj("rivet", at(K.lathe([(-0.001, 0.0016), (t + 0.0026, 0.0016), (t + 0.0028, 0.0001)], 10), x), "bronze")
    # Резной узор — циркульные кружки на накладке.
    for x in (-0.027, 0.0, 0.027):
        K.mesh_obj("ring", K.torus((x, 0, t + 0.0017), 0.004, 0.0006), "bone_dark")


@item("bronze_mirror", (1, 1), name="Бронзовое зеркальце", pose="upright", azimuth=24, elevation=50, margin=0.88)
def bronze_mirror():
    """Круглое зеркальце из белой бронзы с ушком; на обороте — концентрические пояски."""
    M()
    K.metal_mat("speculum", (214, 206, 186), (150, 140, 118), rough=(0.12, 0.3), dirt=0.7, tarnish=((70, 96, 80), 0.5))
    K.mesh_obj("disc", K.lathe([(0.0, 0.0001), (0.0, 0.044), (0.002, 0.046), (0.004, 0.045), (0.0045, 0.0001)], 48), "speculum", subsurf=1)
    for r in (0.018, 0.032, 0.041):
        K.mesh_obj("rib", K.torus((0, 0, 0.0047), r, 0.0012), "bronze")
    K.mesh_obj("boss", K.lathe([(0.004, 0.006), (0.008, 0.004), (0.009, 0.0001)], 16), "bronze", subsurf=1)
    K.mesh_obj("loop", K.torus((0.05, 0, 0.003), 0.007, 0.0016, plane="XY"), "bronze")


@item("menas_ampulla", (1, 1), name="Ампула святого Мины", pose="upright", azimuth=24, elevation=36, margin=0.86)
def menas_ampulla():
    """Паломническая ампула из Абу-Мины: плоская круглая фляжка с двумя ушками,
    на боку — святой Мина с поднятыми руками между двумя верблюдами (рельеф)."""
    M()
    _clay("menas_clay", (206, 176, 136))
    r = 0.038
    body = K.lathe([(0.0, 0.0001), (0.0, r * 0.8), (0.004, r), (0.012, r * 0.9), (0.014, 0.0001)], 40)
    K.move(body, 0, 0, -0.007)
    K.rot(body, "X", 90)
    K.move(body, 0, 0, r + 0.001)
    K.mesh_obj("body", body, "menas_clay", subsurf=1, smooth_angle=70)
    K.mesh_obj("neck", at(K.lathe([(0.0, 0.009), (0.016, 0.0075), (0.019, 0.01), (0.021, 0.0095)], 20, cap_top=False), 0, 0, 2 * r - 0.004), "menas_clay", solidify=0.002, subsurf=1)
    for s in (-1, 1):
        hp = K.bezier((s * 0.008, 0, 2 * r + 0.008), (s * 0.03, 0, 2 * r + 0.012), (s * 0.034, 0, 2 * r - 0.004), (s * 0.026, 0, 2 * r - 0.012), n=10)
        K.mesh_obj("ear", K.loft(hp, [0.0028] * len(hp), 8, up=(0, 1, 0)), "menas_clay", subsurf=1)
    # Рельеф: фигура святого, верблюды, бусинный ободок — на обеих сторонах.
    for s in (-1, 1):
        y = s * 0.0075
        K.mesh_obj("bead", K.torus((0, y, r + 0.001), r * 0.78, 0.0014, plane="XZ"), "menas_clay")
        K.mesh_obj("saint", K.cbox(0, y, r - 0.002, 0.009, 0.003, 0.03), "menas_clay", bevel=0.001)
        K.mesh_obj("head", at(K.lathe([(-0.003, 0.0001), (-0.003, 0.0045), (0.003, 0.0045), (0.003, 0.0001)], 12, axis="Y"), 0, y, r + 0.017), "menas_clay", subsurf=1)
        for side in (-1, 1):
            K.mesh_obj("arm", K.cbox(side * 0.009, y, r + 0.01, 0.012, 0.003, 0.003), "menas_clay")
            K.mesh_obj("camel", K.cbox(side * 0.02, y, r - 0.012, 0.012, 0.003, 0.007), "menas_clay", bevel=0.001)


@item("enkolpion", (1, 1), name="Энколпион", pose="upright", azimuth=24, elevation=50, margin=0.86)
def enkolpion():
    """Бронзовый крест-мощевик из двух створок на шарнире, с ушком для шнурка."""
    M()
    dark_mats()
    t = 0.006

    def arm(cx, cy, w, h):
        b = K.cbox(cx, cy, t / 2, w, h, t)
        return b

    for b in (arm(0, 0.004, 0.016, 0.07), arm(0, 0.012, 0.05, 0.016)):
        K.mesh_obj("cross", b, "bronze", bevel=0.0015)
    # Расширенные концы.
    for (x, y) in ((0, 0.039), (0, -0.031), (0.025, 0.012), (-0.025, 0.012)):
        K.mesh_obj("end", at(K.lathe([(0.0, 0.0001), (0.0, 0.0045), (t + 0.001, 0.0045), (t + 0.001, 0.0001)], 16), x, y), "bronze", subsurf=1)
    K.mesh_obj("hinge", at(K.lathe([(-0.006, 0.003), (0.006, 0.003)], 12, axis="X"), 0, 0.046, t / 2), "bronze")
    K.mesh_obj("loop", K.torus((0, 0.054, t / 2), 0.005, 0.0015, plane="YZ"), "bronze")
    # Гравировка: распятие — тонкие линии на лицевой створке.
    K.mesh_obj("line", K.cbox(0, 0.004, t + 0.0003, 0.002, 0.05, 0.0006), "bronze_dark")
    K.mesh_obj("line", K.cbox(0, 0.012, t + 0.0003, 0.036, 0.002, 0.0006), "bronze_dark")


@item("dice", (1, 1), name="Игральные кости", pose="upright", azimuth=30, elevation=34, margin=0.8)
def dice():
    """Две костяные кости с точками-кружками (сумма противоположных граней — семь)."""
    M()
    K.organic_mat("pip", (40, 28, 20), (20, 14, 10), rough=(0.7, 0.9), bump=0.1)
    s = 0.016
    for k, (x, y, rz) in enumerate(((-0.012, 0.0, 12), (0.012, 0.004, -28))):
        # Точки на верхней грани (у первой — 5, у второй — 3).
        tops = [(-1, -1), (1, 1), (0, 0), (-1, 1), (1, -1)] if k == 0 else [(-1, -1), (0, 0), (1, 1)]
        parts = [("die", K.cbox(0, 0, s / 2, s, s, s), "bone", 0.0018)]
        for (u, v) in tops:
            parts.append(("pip", at(K.lathe([(s - 0.0006, 0.0015), (s + 0.0002, 0.0015), (s + 0.0003, 0.0001)], 8), u * 0.0045, v * 0.0045), "pip", 0.0))
        for name, bm, mat, bev in parts:
            K.rot(bm, "Z", rz)
            K.mesh_obj(name, at(bm, x, y), mat, bevel=bev)


@item("toy_horse", (2, 2), name="Деревянная лошадка", pose="upright", azimuth=30, elevation=16, margin=0.9)
def toy_horse():
    """Детская лошадка на дощечке с колёсиками (такие тянули за верёвочку)."""
    M()
    K.cloth_mat("mane", (60, 40, 26), (34, 22, 14))
    K.mesh_obj("board", K.cbox(0, 0, 0.022, 0.14, 0.05, 0.012), "wood", bevel=0.002)
    for x in (-0.05, 0.05):
        for y in (-0.03, 0.03):
            K.mesh_obj("wheel", at(K.lathe([(-0.005, 0.016), (0.005, 0.016)], 20, axis="Y"), x, y, 0.016), "wood_dark", bevel=0.0015)
        K.mesh_obj("axle", at(K.lathe([(-0.036, 0.003), (0.036, 0.003)], 8, axis="Y"), x, 0, 0.016), "wood")
    for x in (-0.04, 0.04):
        for y in (-0.012, 0.012):
            K.mesh_obj("leg", K.cbox(x, y, 0.045, 0.012, 0.01, 0.036), "wood", bevel=0.002)
    K.mesh_obj("body", K.cbox(0, 0, 0.075, 0.11, 0.036, 0.034), "wood", bevel=0.01, subsurf=1)
    neck = K.cbox(0, 0, 0, 0.026, 0.026, 0.06)
    K.rot(neck, "Y", 30)
    K.move(neck, 0.055, 0, 0.105)
    K.mesh_obj("neck", neck, "wood", bevel=0.006, subsurf=1)
    head = K.cbox(0, 0, 0, 0.05, 0.024, 0.024)
    K.rot(head, "Y", 25)
    K.move(head, 0.08, 0, 0.13)
    K.mesh_obj("head", head, "wood", bevel=0.008, subsurf=1)
    K.mesh_obj("mane", K.cbox(0.048, 0, 0.122, 0.05, 0.008, 0.012), "mane", bevel=0.003)
    tail = K.bezier((-0.055, 0, 0.085), (-0.07, 0, 0.08), (-0.075, 0, 0.06), (-0.07, 0, 0.045), n=8)
    K.mesh_obj("tail", K.loft(tail, [0.004] * len(tail), 8), "mane", subsurf=1)
    K.mesh_obj("rope", K.torus((0.085, 0, 0.022), 0.012, 0.0015, plane="XZ"), "cord")


@item("spindle", (1, 2), name="Веретено с пряслицем", pose="upright", azimuth=24, elevation=46, margin=0.9, roll=90)
def spindle():
    """Деревянное веретено с глиняным пряслицем и намотанной шерстяной нитью."""
    M()
    K.cloth_mat("wool_yarn", (222, 208, 180), (176, 158, 126), weave=1400.0)
    _clay("whorl_clay", (120, 86, 64))
    shaft = K.lathe([(-0.11, 0.0001), (-0.1, 0.0025), (0.0, 0.0045), (0.11, 0.0022), (0.12, 0.0001)], 12, axis="X")
    K.move(shaft, 0, 0, 0.016)
    K.mesh_obj("shaft", shaft, "wood", subsurf=1)
    whorl = K.lathe([(-0.008, 0.004), (-0.006, 0.014), (0.0, 0.016), (0.006, 0.012), (0.008, 0.004)], 24, axis="X")
    K.move(whorl, -0.075, 0, 0.016)
    K.mesh_obj("whorl", whorl, "whorl_clay", subsurf=1)
    cop = K.lathe([(-0.04, 0.005), (-0.025, 0.011), (0.02, 0.012), (0.045, 0.006)], 24, axis="X")
    K.deform(cop, lambda v: (v.x, v.y * (1 + 0.04 * math.sin(v.x * 900)), v.z * (1 + 0.04 * math.sin(v.x * 900))))
    K.move(cop, 0.01, 0, 0.016)
    K.mesh_obj("cop", cop, "wool_yarn", subsurf=1)


@item("wool_yarn", (1, 1), name="Моток пряжи", pose="upright", azimuth=24, elevation=28, margin=0.86)
def wool_yarn():
    """Клубок шерстяной пряжи, окрашенной мареной."""
    M()
    K.cloth_mat("yarn_red", (150, 48, 36), (100, 28, 22), weave=1200.0)
    K.mesh_obj("core", K.lathe([(0.0, 0.0001)] + [(0.04 - 0.04 * math.cos(math.pi * i / 10), 0.04 * math.sin(math.pi * i / 10)) for i in range(1, 10)] + [(0.08, 0.0001)], 28), "yarn_red", subsurf=1)
    for i in range(9):
        t = K.torus((0, 0, 0.04), 0.0405, 0.0026, plane="XZ")
        K.rot(t, "Z", i * 20)
        K.rot(t, "X", 18 + (i % 3) * 22, center=(0, 0, 0.04))
        K.mesh_obj("strand", t, "yarn_red", subsurf=1)
    tail = K.bezier((0.03, 0.02, 0.012), (0.05, 0.04, 0.002), (0.07, 0.03, 0.002), (0.09, 0.05, 0.002), n=10)
    K.mesh_obj("tail", K.loft(tail, [0.0022] * len(tail), 8), "yarn_red", subsurf=1)


@item("iron_shears", (1, 2), name="Железные ножницы", pose="upright", azimuth=24, elevation=50, margin=0.92, roll=90)
def iron_shears():
    """Пружинные ножницы (forfex): два лезвия, соединённых упругой дугой."""
    M()
    for s in (-1, 1):
        blade = K.box(0.03, 0.15, -0.0015, 0.0015, 0.0, 0.016)
        K.deform(blade, lambda v: (v.x, v.y, v.z * (1.0 - max(0.0, v.x - 0.09) / 0.07)))
        K.rot(blade, "X", 90)
        K.move(blade, 0, s * 0.006, 0.0016)
        K.rot(blade, "Z", s * 3)
        K.mesh_obj("blade", blade, "iron", bevel=0.0005)
        arm = K.cbox(-0.005, s * 0.012, 0.004, 0.075, 0.006, 0.006)
        K.mesh_obj("arm", arm, "iron", bevel=0.0015)
    bow = K.circle_path((-0.045, 0, 0.004), 0.014, plane="XY", n=24, start=90, end=270)
    K.mesh_obj("bow", K.loft(bow, [0.0032] * len(bow), 8), "iron", subsurf=1)


@item("wooden_casket", (2, 2), name="Деревянная шкатулка", pose="upright", azimuth=26, elevation=26, margin=0.9)
def wooden_casket():
    """Шкатулка с костяными накладками (циркульный орнамент) и бронзовым замочком."""
    M()
    dark_mats()
    w, d, h = 0.16, 0.1, 0.075
    K.mesh_obj("box", K.cbox(0, 0, h * 0.4, w, d, h * 0.8), "wood_dark", bevel=0.002)
    lid = K.cbox(0, 0, h * 0.9, w + 0.004, d + 0.004, h * 0.2)
    K.deform(lid, lambda v: (v.x, v.y, v.z + (0.008 * (1 - (v.y / (d / 2)) ** 2) if v.z > h * 0.95 else 0.0)))
    K.mesh_obj("lid", lid, "wood_dark", bevel=0.002, subsurf=0)
    for y in (-1, 1):
        K.mesh_obj("plaque", K.cbox(0, y * (d / 2 + 0.0008), h * 0.42, w * 0.9, 0.0016, h * 0.5), "bone", bevel=0.0006)
        for x in (-0.05, 0.0, 0.05):
            K.mesh_obj("ring", K.torus((x, y * (d / 2 + 0.0018), h * 0.42), 0.01, 0.0009, plane="XZ"), "bone_dark")
    K.mesh_obj("lock", K.cbox(0, -(d / 2 + 0.003), h * 0.72, 0.026, 0.002, 0.03), "bronze", bevel=0.0008)
    K.mesh_obj("hasp", K.cbox(0, -(d / 2 + 0.004), h * 0.86, 0.01, 0.002, 0.03), "bronze", bevel=0.0008)


@item("tabula_board", (3, 2), name="Доска для тавлей", pose="upright", azimuth=20, elevation=40, margin=0.92)
def tabula_board():
    """Доска для тавлей (tabula — предок нард): по 12 полей-клиньев с каждой стороны,
    шашки двух цветов, стаканчик для костей."""
    M()
    K.organic_mat("inlay_light", (206, 186, 150), (166, 146, 110), rough=(0.45, 0.65), grain="X", grain_scale=5.0, dirt=0.4)
    K.organic_mat("inlay_dark", (62, 40, 26), (34, 22, 14), rough=(0.4, 0.6), grain="X", grain_scale=5.0, dirt=0.4)
    w, d, t = 0.4, 0.3, 0.016
    K.mesh_obj("board", K.cbox(0, 0, t / 2, w, d, t), "wood", bevel=0.003)
    for s in (-1, 1):
        K.mesh_obj("rim", K.cbox(0, s * (d / 2 - 0.006), t + 0.004, w, 0.012, 0.008), "wood_dark", bevel=0.002)
        K.mesh_obj("rim", K.cbox(s * (w / 2 - 0.006), 0, t + 0.004, 0.012, d, 0.008), "wood_dark", bevel=0.002)
    K.mesh_obj("bar", K.cbox(0, 0, t + 0.003, 0.014, d - 0.02, 0.006), "wood_dark", bevel=0.002)
    pw = (w / 2 - 0.02) / 6
    for half in (-1, 1):
        for i in range(6):
            x0 = half * 0.01 + (i * pw if half > 0 else -(i + 1) * pw)
            for side in (-1, 1):
                y0 = side * (d / 2 - 0.012)
                tri = K.box(x0 + 0.002, x0 + pw - 0.002, min(y0, y0 - side * 0.11), max(y0, y0 - side * 0.11), t, t + 0.0008)
                K.deform(tri, lambda v, x0=x0, y0=y0, side=side: (x0 + pw / 2 + (v.x - x0 - pw / 2) * max(0.05, 1 - abs(v.y - y0) / 0.11), v.y, v.z))
                K.mesh_obj("point", tri, "inlay_light" if (i + (side > 0)) % 2 else "inlay_dark")
    for k, (x, y) in enumerate(((-0.15, -0.1), (-0.15, -0.075), (0.12, 0.1), (0.05, -0.1), (0.09, 0.08), (-0.06, 0.1))):
        K.mesh_obj("man", at(K.lathe([(0.0, 0.0001), (0.0, 0.011), (0.005, 0.011), (0.006, 0.009), (0.006, 0.0001)], 20), x, y, t + 0.0008), "bone" if k % 2 else "wood_dark", subsurf=1)
