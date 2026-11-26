"""3D-модели снастей и рыбы Чёрного моря (fishing.rs).

Собрать: python3 tools/models/build_items.py rod_reed rod_juniper fish_hooks sea_worms shrimp \
    fish_hamsa fish_stavrida fish_barabulka fish_kefal fish_yorsh fish_lufar fish_kalkan fish_osetr

Рыба лежит вдоль Z (хвост внизу, рыло вверху), высота тела — вдоль X (спина в +X),
толщина — вдоль Y. Лицевой бок (-Y) смотрит в камеру иконки и вверх, когда рыба лежит на земле.
Окраска — процедурная: тёмная спина, светлое брюхо, чешуя, у каждого вида свой узор.
"""
import math
import random
import zlib

import itemkit as K
from itemkit import item


def _rng(key):
    return random.Random(zlib.crc32(key.encode()))


# =========================================================================== материалы


def fish_mat(name, height, back, side, belly, metal=0.55, rough=(0.22, 0.4), scale_size=0.006, pattern=None, accent=None):
    """Рыбья кожа: спина -> бок -> брюхо по высоте тела (X, ±height), мелкая чешуя
    (ячейки Вороного с тёмной кромкой), влажный блеск. pattern:
      "stripes" — тёмные продольные полосы по верху (кефаль);
      "gold" — жёлтые продольные полосы по боку (барабулька);
      "mottle" — пятна и разводы (ёрш, калкан);
      "scutes" — без чешуи, шершавая кожа (осётр)."""
    m, nb, bsdf = K.new_material(name)
    vec = nb.coord()
    sep = nb.new("ShaderNodeSeparateXYZ")
    nb.link(vec, sep.inputs["Vector"])
    x = sep.outputs["X"]
    z = sep.outputs["Z"]
    hx = nb.math("ADD", nb.math("MULTIPLY", nb.math("DIVIDE", x, height), 0.5), 0.5, clamp=True)
    # Неровная граница спины и бока.
    wob = nb.math("MULTIPLY", nb.math("SUBTRACT", nb.noise(18.0, 4, 0.5, vec), 0.5), 0.12)
    hx = nb.math("ADD", hx, wob)
    tone = nb.ramp(hx, [(0.0, K.lin(belly)), (0.42, K.lin(belly)), (0.6, K.lin(side)), (0.78, K.lin(back)), (1.0, K.lin(back))])
    if pattern == "scutes":
        sc = nb.noise(900.0, 6, 0.7, vec)
        height_map = sc
    else:
        cells = nb.voronoi(1.0 / scale_size, nb.coord((1.0, 1.4, 0.75)), feature="F1", out="Distance")
        edge = nb.ramp(cells, [(0.0, 0.0), (0.55, 0.0), (0.95, 1.0)])
        tone = nb.mix(nb.math("MULTIPLY", edge, 0.35), tone, tuple(v * 0.45 for v in K.lin(back)))
        height_map = nb.math("SUBTRACT", 1.0, cells)
    if pattern == "stripes":
        # Полосы вдоль тела — по X (высоте), видны на верхней половине.
        bands = nb.wave(1.0 / (height * 0.28), vec, "BANDS", "X", distortion=1.5, detail=2.0)
        bands = nb.ramp(bands, [(0.0, 0.0), (0.18, 0.7), (0.3, 0.0)])
        upper = nb.ramp(hx, [(0.45, 0.0), (0.6, 1.0)])
        tone = nb.mix(nb.math("MULTIPLY", bands, upper), tone, tuple(v * 0.5 for v in K.lin(back)))
    if pattern == "gold":
        bands = nb.wave(1.0 / (height * 0.33), vec, "BANDS", "X", distortion=0.8, detail=1.0)
        bands = nb.ramp(bands, [(0.0, 0.0), (0.2, 0.9), (0.32, 0.0)])
        mid = nb.ramp(hx, [(0.25, 0.0), (0.4, 1.0), (0.7, 1.0), (0.8, 0.0)])
        tone = nb.mix(nb.math("MULTIPLY", bands, mid), tone, K.lin(accent or (230, 190, 60)))
    if pattern in ("mottle", "scutes"):
        blot = nb.noise(1.0 / (height * 0.35), 3, 0.6, vec, dist=0.8)
        blot = nb.ramp(blot, [(0.45, 0.0), (0.62, 1.0)])
        tone = nb.mix(nb.math("MULTIPLY", blot, 0.75), tone, K.lin(accent or tuple(int(v * 0.45) for v in back)))
        if pattern == "mottle":
            spec = nb.voronoi(1.0 / (height * 0.12), vec, feature="F1", out="Distance")
            spec = nb.ramp(spec, [(0.0, 1.0), (0.18, 0.0)])
            tone = nb.mix(nb.math("MULTIPLY", spec, 0.6), tone, tuple(min(1.0, v * 1.5) for v in K.lin(side)))
    # Грязь в складках (жабры, плавники).
    d = nb.math("MULTIPLY", nb.math("SUBTRACT", 1.0, nb.ao(0.01)), 0.8, clamp=True)
    tone = nb.mix(d, tone, tuple(v * 0.3 for v in K.lin(back)))
    nb.val(bsdf.inputs["Base Color"], tone)
    nb.val(bsdf.inputs["Metallic"], nb.mixf(hx, metal, metal * 0.35))
    roughv = nb.math("ADD", rough[0], nb.math("MULTIPLY", nb.noise(40.0, 4, 0.5, vec), rough[1] - rough[0]))
    nb.val(bsdf.inputs["Roughness"], roughv)
    bsdf.inputs["Coat Weight"].default_value = 0.35
    bsdf.inputs["Coat Roughness"].default_value = 0.12
    nb.val(bsdf.inputs["Normal"], nb.bump(height_map, 0.25, 0.0006))
    _ = z
    return m


def fin_mat(name, color, color2):
    K.organic_mat(name, color, color2, rough=(0.35, 0.55), bump=0.3, grain="Z", grain_scale=60.0, dirt=0.3, sheen=0.2)


def eye_mats():
    K.organic_mat("fish_eye", (16, 14, 12), (6, 6, 6), rough=(0.05, 0.1), bump=0.0, dirt=0.0)
    K.metal_mat("fish_iris", (214, 196, 140), (150, 130, 84), rough=(0.2, 0.3), scratches=0.0, dirt=0.2, bump=0.0)


# =========================================================================== тело рыбы


def body_profile(t, belly=0.6, snout=0.35, peduncle=0.14):
    """Высота тела 0..1 вдоль длины: t=0 — хвостовой стебель, t=1 — кончик рыла."""
    t = min(max(t, 0.0), 1.0)
    rise = math.sin(math.pi * 0.5 * min(1.0, t / belly)) ** 0.8
    fall = (1.0 - max(0.0, (t - belly) / (1.0 - belly))) ** snout if t > belly else 1.0
    return peduncle + (1.0 - peduncle) * rise * fall


def fish_body(L, H, T, mat, belly=0.6, snout=0.35, peduncle=0.14, arch=0.0, droop=0.0, n=30):
    """Тело длиной L от хвостового стебля (z=0) до рыла (z=L); полувысота H, полутолщина T.
    arch — горб спины (сдвиг осевой линии вверх посередине), droop — рыло ниже оси."""
    path, radii = [], []
    for i in range(n + 1):
        t = i / n
        p = body_profile(t, belly, snout, peduncle)
        cx = arch * H * math.sin(math.pi * t) - droop * H * max(0.0, t - 0.8) / 0.2
        path.append((cx, 0.0, L * t))
        radii.append((max(T * p ** 0.9, 0.0015), max(H * p, 0.002)))
    K.mesh_obj("body", K.loft(path, radii, 20), mat, subsurf=1, smooth_angle=85)
    return path, radii


def fin(stations, mat, rot=None, move=(0, 0, 0)):
    """Плавник — тонкая пластина: stations (z, полуширина по X, полутолщина, центр по X)."""
    bm = K.blade(stations, 8, edge=0.0002)
    if rot:
        for axis, deg in rot:
            K.rot(bm, axis, deg)
    K.move(bm, *move)
    K.mesh_obj("fin", bm, mat, smooth_angle=60)


def tail(L, H, mat, fork=0.5, spread=34.0, lobe=None, upper=1.0):
    """Хвостовой плавник у z=0: две лопасти, разведённые на spread градусов. upper — во сколько
    раз верхняя лопасть длиннее (у осетра хвост неравнолопастной)."""
    lobe = lobe or H * 1.1
    for s, k in ((1, upper), (-1, 1.0)):
        ln = lobe * k
        st = [(0.0, H * 0.18, H * 0.03), (ln * 0.45, H * 0.26 * (1 - fork * 0.3), H * 0.02), (ln * 0.85, H * 0.18 * (1 - fork * 0.5), H * 0.012), (ln, H * 0.02, H * 0.006)]
        bm = K.blade(st, 8, edge=0.0002)
        K.rot(bm, "Y", 180 - s * spread)
        K.move(bm, 0.0, 0.0, H * 0.06)
        K.mesh_obj("tail", bm, mat, smooth_angle=60)


def eye(L, H, T, z, x, r):
    """Глаз на обоих боках: тёмный зрачок в светлой радужке."""
    for y in (-1, 1):
        iris = K.lathe([(0.0, r), (r * 0.25, r * 0.98), (r * 0.5, r * 0.6), (r * 0.55, 0.0001)], 16)
        K.rot(iris, "X", 90 * y)
        K.move(iris, x, y * (T * 0.62), z)
        K.mesh_obj("iris", iris, "fish_iris")
        pupil = K.lathe([(0.0, r * 0.6), (r * 0.2, r * 0.55), (r * 0.35, 0.0001)], 14)
        K.rot(pupil, "X", 90 * y)
        K.move(pupil, x, y * (T * 0.62 + r * 0.32), z)
        K.mesh_obj("eye", pupil, "fish_eye")


def gill(L, H, T, z, mat):
    """Жаберная крышка: тонкая дуга-складка на обоих боках."""
    for y in (-1, 1):
        pts = []
        for i in range(9):
            a = -1.0 + 2.0 * i / 8
            pts.append((a * H * 0.62, y * (T * 0.88), z - H * 0.18 * (1 - a * a)))
        K.mesh_obj("gill", K.loft(pts, [T * 0.06] * len(pts), 6), mat, subsurf=0)


def standard_fish(key, L, H, T, mat, finm, belly=0.58, snout=0.4, dorsal=((0.45, 0.7, 0.55),), anal=(0.25, 0.42, 0.35), fork=0.6, spread=32.0, eye_r=None, arch=0.0, droop=0.0, pectoral=True):
    """Обычная веретенообразная рыба. dorsal — спинные плавники (z0, z1 в долях длины, высота в долях H)."""
    fish_body(L, H, T, mat, belly, snout, arch=arch, droop=droop)
    tail(L, H, finm, fork=fork, spread=spread)
    for z0, z1, hgt in dorsal:
        top = H * body_profile((z0 + z1) / 2, belly, snout) + arch * H * math.sin(math.pi * (z0 + z1) / 2)
        st = []
        for i in range(7):
            u = i / 6
            w = H * hgt * 0.5 * math.sin(math.pi * min(1.0, u * 1.15)) ** 0.7 * (1.0 - 0.35 * u)
            st.append((L * (z0 + (z1 - z0) * u), max(w, 0.0004), T * 0.08, top * 0.92 + w))
        fin(st, finm)
    if anal:
        z0, z1, hgt = anal
        bot = -H * body_profile((z0 + z1) / 2, belly, snout)
        st = []
        for i in range(6):
            u = i / 5
            w = H * hgt * 0.5 * math.sin(math.pi * min(1.0, u * 1.1)) ** 0.7
            st.append((L * (z0 + (z1 - z0) * u), max(w, 0.0004), T * 0.08, bot * 0.92 - w))
        fin(st, finm)
    if pectoral:
        for y in (-1, 1):
            st = [(0.0, H * 0.12, T * 0.05), (H * 0.4, H * 0.14, T * 0.04), (H * 0.7, H * 0.04, T * 0.03)]
            bm = K.blade(st, 6, edge=0.0002)
            K.rot(bm, "X", 180 + y * 18)
            K.rot(bm, "Z", y * 70)
            K.move(bm, -H * 0.25, y * T * 0.75, L * 0.74)
            K.mesh_obj("pectoral", bm, finm, smooth_angle=60)
    eye(L, H, T, L * 0.9, H * 0.18, eye_r or H * 0.17)
    gill(L, H, T, L * 0.82, mat)


# =========================================================================== рыба


@item("fish_hamsa", (1, 1), name="Хамса", pose="flat", azimuth=10, elevation=8, margin=0.9)
def fish_hamsa():
    """Хамса — три рыбки рядом: зеленоватая спинка, серебряный бок с синей полоской."""
    K.common_materials()
    eye_mats()
    fish_mat("hamsa_skin", 0.009, (52, 96, 98), (150, 186, 196), (226, 232, 236), metal=0.75, scale_size=0.0018, pattern="stripes")
    fin_mat("hamsa_fin", (190, 200, 196), (120, 140, 138))
    for k, (dx, dz) in enumerate(((-0.024, 0.0), (0.0, 0.012), (0.024, -0.006))):
        start = len(K.PARTS)
        standard_fish("hamsa", 0.1, 0.009, 0.006, "hamsa_skin", "hamsa_fin", belly=0.5, snout=0.5, dorsal=((0.42, 0.55, 0.7),), anal=(0.18, 0.28, 0.5), fork=0.8, spread=36, eye_r=0.0028)
        for o in K.PARTS[start:]:
            o.data.transform(K.Matrix.Translation((dx, 0.0, dz)))


@item("fish_stavrida", (1, 2), name="Ставрида", pose="flat", azimuth=10, elevation=8, margin=0.92)
def fish_stavrida():
    K.common_materials()
    eye_mats()
    fish_mat("stavrida_skin", 0.028, (54, 84, 92), (168, 182, 186), (232, 232, 226), metal=0.7, scale_size=0.003)
    fin_mat("stavrida_fin", (200, 196, 150), (130, 126, 90))
    standard_fish("stavrida", 0.24, 0.028, 0.012, "stavrida_skin", "stavrida_fin", belly=0.55, snout=0.45, dorsal=((0.55, 0.68, 0.9), (0.2, 0.5, 0.45)), anal=(0.15, 0.42, 0.4), fork=0.9, spread=38, eye_r=0.008)
    # Костяные щитки вдоль боковой линии (у ставриды они колючие).
    for y in (-1, 1):
        pts = []
        for i in range(18):
            t = i / 17
            z = 0.24 * (0.02 + 0.62 * t)
            x = 0.028 * (0.05 + 0.25 * max(0.0, t - 0.6) / 0.4)
            pts.append((x, y * 0.012 * body_profile(z / 0.24, 0.55, 0.45) ** 0.9 * 1.02, z))
        K.mesh_obj("scutes", K.loft(pts, [0.0012] * len(pts), 6), "stavrida_skin")


@item("fish_barabulka", (1, 1), name="Барабулька", pose="flat", azimuth=10, elevation=8, margin=0.9)
def fish_barabulka():
    """Барабулька (султанка): розово-красная с жёлтыми полосами, два уса под рылом."""
    K.common_materials()
    eye_mats()
    fish_mat("barabulka_skin", 0.022, (176, 58, 50), (226, 128, 104), (244, 214, 196), metal=0.35, scale_size=0.0035, pattern="gold", accent=(236, 184, 70))
    fin_mat("barabulka_fin", (232, 170, 140), (170, 100, 80))
    L, H, T = 0.17, 0.022, 0.012
    standard_fish("barabulka", L, H, T, "barabulka_skin", "barabulka_fin", belly=0.55, snout=0.55, dorsal=((0.62, 0.76, 0.9), (0.3, 0.48, 0.6)), anal=(0.2, 0.36, 0.5), fork=0.7, eye_r=0.0055, droop=0.35)
    for y in (-1, 1):
        pts = [(-H * 0.55, y * 0.002, L * 0.97), (-H * 0.95, y * 0.006, L * 0.92), (-H * 1.25, y * 0.008, L * 0.86)]
        K.mesh_obj("barbel", K.loft(pts, [0.0012, 0.0009, 0.0005], 6), "barabulka_fin")


@item("fish_kefal", (1, 3), name="Кефаль", pose="flat", azimuth=10, elevation=8, margin=0.92)
def fish_kefal():
    """Кефаль (лобан): серебристо-серая, с тёмными продольными полосами по спине."""
    K.common_materials()
    eye_mats()
    fish_mat("kefal_skin", 0.05, (60, 72, 82), (160, 170, 176), (234, 234, 230), metal=0.6, scale_size=0.0075, pattern="stripes")
    fin_mat("kefal_fin", (120, 126, 130), (70, 74, 78))
    standard_fish("kefal", 0.42, 0.05, 0.03, "kefal_skin", "kefal_fin", belly=0.5, snout=0.55, dorsal=((0.6, 0.68, 0.7), (0.3, 0.42, 0.55)), anal=(0.24, 0.36, 0.45), fork=0.5, spread=30, eye_r=0.011)


@item("fish_yorsh", (1, 2), name="Морской ёрш", pose="flat", azimuth=10, elevation=8, margin=0.92)
def fish_yorsh():
    """Скорпена: красно-бурая в разводах, большая голова, колючий спинной плавник, «рожки» над глазами."""
    K.common_materials()
    eye_mats()
    fish_mat("yorsh_skin", 0.05, (110, 46, 34), (176, 92, 64), (214, 164, 132), metal=0.08, rough=(0.45, 0.65), scale_size=0.005, pattern="mottle", accent=(70, 32, 24))
    fin_mat("yorsh_fin", (170, 90, 60), (90, 40, 28))
    L, H, T = 0.24, 0.05, 0.03
    standard_fish("yorsh", L, H, T, "yorsh_skin", "yorsh_fin", belly=0.68, snout=0.6, dorsal=(), anal=(0.2, 0.32, 0.6), fork=0.0, spread=8, eye_r=0.011, arch=0.12)
    # Колючий спинной плавник: лучи-шипы с перепонкой.
    top = H * 0.95
    for i in range(11):
        z = L * (0.32 + 0.4 * i / 10)
        h = H * (0.55 + 0.25 * math.sin(math.pi * i / 10))
        K.mesh_obj("spine", K.loft([(top, 0, z), (top + h, 0, z - 0.006)], [0.0016, 0.0003], 6), "yorsh_fin")
    st = [(L * (0.32 + 0.4 * u / 6), H * 0.3, 0.0012, top + H * 0.3) for u in range(7)]
    fin(st, "yorsh_fin")
    for y in (-1, 1):
        for k in range(3):
            K.mesh_obj("horn", K.loft([(H * 0.32, y * T * 0.5, L * (0.84 + k * 0.03)), (H * 0.5, y * T * 0.62, L * (0.85 + k * 0.03))], [0.0022, 0.0003], 6), "yorsh_skin")


@item("fish_lufar", (1, 3), name="Луфарь", pose="flat", azimuth=10, elevation=8, margin=0.92)
def fish_lufar():
    """Луфарь: зеленовато-синяя спина, серебряный бок, большой рот с зубами."""
    K.common_materials()
    eye_mats()
    fish_mat("lufar_skin", 0.055, (38, 84, 96), (126, 166, 170), (230, 234, 230), metal=0.65, scale_size=0.006)
    fin_mat("lufar_fin", (160, 170, 150), (90, 104, 90))
    L, H, T = 0.5, 0.055, 0.028
    standard_fish("lufar", L, H, T, "lufar_skin", "lufar_fin", belly=0.52, snout=0.42, dorsal=((0.58, 0.7, 0.5), (0.22, 0.5, 0.5)), anal=(0.18, 0.45, 0.45), fork=0.75, spread=34, eye_r=0.011)
    # Пасть: тёмная щель.
    K.mesh_obj("mouth", K.loft([(-H * 0.05, -T * 0.42, L * 0.99), (-H * 0.2, -T * 0.5, L * 0.93), (-H * 0.26, -T * 0.55, L * 0.88)], [0.0016, 0.0015, 0.001], 6), "fish_eye")


@item("fish_kalkan", (2, 2), name="Калкан", pose="flat", azimuth=8, elevation=14, margin=0.92)
def fish_kalkan():
    """Калкан — черноморская камбала: почти круглый, плоский, глазастый бок бурый в пятнах
    и костяных бугорках, плавник бахромой вокруг тела."""
    K.common_materials()
    eye_mats()
    fish_mat("kalkan_skin", 0.2, (96, 82, 60), (128, 110, 82), (150, 132, 100), metal=0.05, rough=(0.45, 0.7), scale_size=0.006, pattern="mottle", accent=(58, 48, 34))
    fin_mat("kalkan_fin", (140, 120, 90), (80, 68, 48))
    K.organic_mat("kalkan_bone", (180, 166, 136), (110, 96, 72), rough=(0.5, 0.7), bump=0.4, bump_scale=300.0, dirt=0.6)
    L, H, T = 0.46, 0.2, 0.03
    path, radii = fish_body(L, H, T, "kalkan_skin", belly=0.5, snout=0.7, peduncle=0.12)
    tail(L, H * 0.35, "kalkan_fin", fork=0.0, spread=14, lobe=0.08)
    # Бахрома плавников по краю (спинной и анальный тянутся почти по всему телу).
    for s in (1, -1):
        st = []
        for i in range(12):
            u = i / 11
            z = L * (0.08 + 0.82 * u)
            edge = H * body_profile(z / L, 0.5, 0.7, 0.12)
            w = 0.018 * math.sin(math.pi * u) ** 0.5
            st.append((z, max(w, 0.0005), 0.0015, s * (edge * 0.96 + w)))
        fin(st, "kalkan_fin")
    rng = _rng("kalkan")
    # Бугорки на глазастом (лицевом, -Y) боку.
    for _ in range(46):
        z = rng.uniform(0.12, 0.86) * L
        hx = H * body_profile(z / L, 0.5, 0.7, 0.12) * rng.uniform(-0.85, 0.85)
        r = rng.uniform(0.004, 0.009)
        cone = K.lathe([(0.0, r * 0.9), (r * 0.6, r * 0.6), (r * 0.9, 0.0001)], 8)
        K.rot(cone, "X", -90)
        prof = body_profile(z / L, 0.5, 0.7, 0.12)
        k = max(0.0, 1 - (hx / max(H * prof, 1e-4)) ** 2)
        K.move(cone, hx, -T * prof ** 0.9 * math.sqrt(k) + 0.001, z)
        K.mesh_obj("tubercle", cone, "kalkan_bone")
    # Оба глаза — на одном (лицевом) боку.
    for k, (z, x) in enumerate(((L * 0.86, H * 0.2), (L * 0.8, H * 0.38))):
        r = 0.011
        iris = K.lathe([(0.0, r), (r * 0.4, r * 0.9), (r * 0.7, 0.0001)], 16)
        K.rot(iris, "X", -90)
        K.move(iris, x, -T * 0.8, z)
        K.mesh_obj("iris", iris, "fish_iris")
        pupil = K.lathe([(0.0, r * 0.55), (r * 0.3, r * 0.5), (r * 0.5, 0.0001)], 14)
        K.rot(pupil, "X", -90)
        K.move(pupil, x, -T * 0.8 - r * 0.45, z)
        K.mesh_obj("eye", pupil, "fish_eye")


@item("fish_osetr", (2, 4), name="Осётр", pose="flat", azimuth=12, elevation=10, margin=0.95)
def fish_osetr():
    """Осётр: длинное серо-бурое тело с пятью рядами костяных жучек, острое рыло с усиками,
    неравнолопастный хвост."""
    K.common_materials()
    eye_mats()
    fish_mat("osetr_skin", 0.09, (70, 66, 58), (118, 110, 96), (220, 214, 196), metal=0.05, rough=(0.45, 0.65), pattern="scutes")
    fin_mat("osetr_fin", (110, 100, 86), (60, 54, 46))
    K.organic_mat("osetr_scute", (196, 186, 160), (120, 110, 90), rough=(0.4, 0.6), bump=0.4, bump_scale=260.0, dirt=0.7)
    L, H, T = 1.35, 0.09, 0.075
    path, radii = fish_body(L, H, T, "osetr_skin", belly=0.45, snout=1.6, peduncle=0.2, droop=0.15)
    tail(L, H * 0.9, "osetr_fin", fork=0.0, spread=26, lobe=0.22, upper=1.7)
    # Жучки: ряд по спине, по бокам и по брюху.
    for row, (ax, ay) in enumerate(((1.0, 0.0), (0.05, -1.0), (0.05, 1.0), (-0.75, -0.6), (-0.75, 0.6))):
        n = 13 if row == 0 else 30
        for i in range(n):
            t = 0.1 + 0.72 * i / (n - 1)
            p = body_profile(t, 0.45, 1.6, 0.2)
            r = (0.016 if row == 0 else 0.009) * (0.6 + 0.4 * p)
            x = ax * H * p * 0.98 - 0.15 * H * max(0.0, t - 0.8) / 0.2
            y = ay * T * p ** 0.9 * 0.98
            sc = K.lathe([(0.0, r * 0.8), (r * 0.5, r * 0.6), (r, 0.0001)], 6)
            nrm = K.Vector((ax * H, ay * T, 0.0)).normalized() if (ax or ay) else K.Vector((1, 0, 0))
            rot = K.Vector((0, 0, 1)).rotation_difference(nrm).to_matrix().to_4x4()
            K.transform(sc, rot)
            K.move(sc, x, y, L * t)
            K.mesh_obj("scute", sc, "osetr_scute")
    # Спинной плавник у хвоста, усики под рылом.
    st = [(L * (0.12 + 0.12 * u / 5), H * 0.35 * math.sin(math.pi * (0.2 + 0.8 * u / 5)), 0.003, H * 0.42 + H * 0.35 * math.sin(math.pi * (0.2 + 0.8 * u / 5))) for u in range(6)]
    fin(st, "osetr_fin")
    for y in (-1, 1):
        for k in range(2):
            z = L * (0.93 - k * 0.012)
            K.mesh_obj("barbel", K.loft([(-H * 0.25, y * 0.006 * (k + 1), z), (-H * 0.55, y * 0.008 * (k + 1), z - 0.012)], [0.0016, 0.0006], 6), "osetr_fin")
    for y in (-1, 1):
        for x0, r in ((H * 0.25, 0.007),):
            eye_ball = K.lathe([(0.0, r), (r * 0.6, r * 0.7), (r * 0.8, 0.0001)], 12)
            K.rot(eye_ball, "X", 90 * y)
            K.move(eye_ball, x0, y * T * 0.42, L * 0.86)
            K.mesh_obj("eye", eye_ball, "fish_eye")
    gill(L, H, T * 0.85, L * 0.8, "osetr_skin")


# =========================================================================== снасти


def rod_mats():
    K.common_materials()
    K.organic_mat("reed", (206, 186, 128), (150, 128, 74), rough=(0.35, 0.55), grain="Z", grain_scale=14.0, dirt=0.4, sheen=0.1)
    K.organic_mat("reed_node", (150, 120, 70), (96, 74, 40), rough=(0.4, 0.6), grain="Z", grain_scale=14.0, dirt=0.6)
    K.organic_mat("juniper", (148, 98, 60), (90, 56, 32), rough=(0.4, 0.6), grain="Z", grain_scale=8.0, dirt=0.5, wear=0.3, wear_color=(190, 140, 96))
    K.organic_mat("horsehair", (214, 206, 186), (160, 150, 128), rough=(0.35, 0.5), grain="Z", grain_scale=40.0, dirt=0.1, sheen=0.3)
    K.organic_mat("cork", (176, 130, 84), (120, 84, 50), rough=(0.8, 0.95), bump=0.7, bump_scale=600.0, dirt=0.4)
    K.organic_mat("float_paint", (176, 40, 30), (120, 24, 18), rough=(0.4, 0.6), bump=0.1, dirt=0.3)
    K.metal_mat("lead", (120, 122, 126), (80, 82, 86), rough=(0.5, 0.7), scratches=0.2, dirt=0.6, bump=0.3)


def tackle_at(x, z, line_len=0.12):
    """Поплавок, грузило и крючок, примотанные к удилищу у рукояти (так носят удочку)."""
    # Леса, намотанная восьмёркой на две колышка-рогульки.
    for zz in (z, z + 0.16):
        K.mesh_obj("peg", K.loft([(x - 0.012, 0, zz), (x + 0.03, 0, zz + 0.012)], [0.004, 0.003], 6), "wood_dark")
    for k in range(6):
        a = (k - 2.5) * 0.004
        pts = [(x + 0.022, a, z + 0.004), (x + 0.03, a * 1.2, z + 0.08), (x + 0.022, a, z + 0.156)]
        K.mesh_obj("wound", K.loft(pts, [0.0012] * 3, 5), "horsehair")
    fl = K.lathe([(0.0, 0.0001), (0.012, 0.008), (0.03, 0.011), (0.05, 0.007), (0.065, 0.0001)], 12)
    K.move(fl, x + 0.034, 0.0, z - 0.09)
    K.mesh_obj("float", fl, "cork", subsurf=1)
    cap = K.lathe([(0.0, 0.0081), (0.012, 0.0088), (0.024, 0.0001)], 12)
    K.move(cap, x + 0.034, 0.0, z - 0.09 + 0.042)
    K.mesh_obj("float_top", cap, "float_paint", subsurf=1)
    hook_mesh(x + 0.034, z - 0.16, 0.8, 0.0)
    K.mesh_obj("snood", K.loft([(x + 0.034, 0, z - 0.09), (x + 0.034, 0, z - 0.13)], [0.0006, 0.0006], 5), "horsehair")


def reed_rod(L):
    """Удилище из тростника: колена с узлами, к вершине тоньше; леса привязана к вершине
    и намотана на рогульки у комля."""
    n_seg = 9
    zs = [L * (i / n_seg) ** 1.05 for i in range(n_seg + 1)]
    for i in range(n_seg):
        z0, z1 = zs[i], zs[i + 1]
        r0 = 0.014 * (1 - 0.72 * z0 / L)
        r1 = 0.014 * (1 - 0.72 * z1 / L)
        bend = lambda z: 0.04 * (z / L) ** 2.2  # noqa: E731  (вершина чуть гнётся)
        pts = [(bend(z0 + (z1 - z0) * t), 0.0, z0 + (z1 - z0) * t) for t in (0.0, 0.5, 1.0)]
        K.mesh_obj("reed", K.loft(pts, [r0, (r0 + r1) / 2, r1], 12), "reed", subsurf=0)
        if i > 0:
            K.mesh_obj("node", K.loft([(bend(z0), 0, z0 - 0.006), (bend(z0), 0, z0 + 0.006)], [r0 * 1.18, r0 * 1.18], 12), "reed_node")
    return lambda z: 0.04 * (z / L) ** 2.2


@item("rod_reed", (1, 5), name="Тростниковая удочка", pose="flat", azimuth=20, elevation=10, margin=0.95)
def rod_reed():
    rod_mats()
    L = 2.9
    bend = reed_rod(L)
    tip = (bend(L), 0.0, L)
    # Леса от вершины вдоль удилища вниз (уложена вдоль, как носят).
    pts = [tip, (bend(L * 0.6) + 0.018, 0, L * 0.6), (0.03, 0, 0.62)]
    K.mesh_obj("line", K.loft(pts, [0.0007] * 3, 5), "horsehair")
    # Обмотка рукояти бечёвкой.
    for k in range(10):
        z = 0.1 + k * 0.022
        K.mesh_obj("wrap", K.torus((0, 0, z), 0.0155, 0.0022), "cord")
    tackle_at(0.0, 0.45)


@item("rod_juniper", (1, 5), name="Можжевеловая удочка", pose="flat", azimuth=20, elevation=10, margin=0.95)
def rod_juniper():
    """Гибкая можжевеловая жердь, к вершине обструганная; кожаная рукоять, бронзовое кольцо-обойма."""
    rod_mats()
    L = 3.2
    pts, radii = [], []
    for i in range(25):
        t = i / 24
        z = L * t
        pts.append((0.06 * t ** 2.4 + 0.002 * math.sin(t * 17), 0.0015 * math.sin(t * 11), z))
        radii.append(0.017 * (1 - 0.8 * t ** 0.85) + 0.0015)
    K.mesh_obj("rod", K.loft(pts, radii, 12), "juniper", subsurf=1)
    K.mesh_obj("grip", K.loft([(0, 0, 0.05), (0, 0, 0.36)], [0.021, 0.02], 14), "leather_dark", subsurf=1)
    for z in (0.05, 0.37):
        K.mesh_obj("ferrule", K.loft([(0, 0, z - 0.008), (0, 0, z + 0.008)], [0.0225, 0.0225], 16), "bronze", bevel=0.001)
    tip = pts[-1]
    K.mesh_obj("line", K.loft([tip, (pts[15][0] + 0.02, 0, pts[15][2]), (0.032, 0, 0.62)], [0.0007] * 3, 5), "horsehair")
    tackle_at(0.0, 0.45)


def hook_mesh(x, z, s=1.0, turn=0.0):
    """Бронзовый крючок с ушком и жалом (длина ~3 см)."""
    pts = []
    for i in range(16):
        t = i / 15
        if t < 0.5:
            pts.append((0.0, 0.0, 0.03 * s * (1 - t * 2)))
        else:
            a = (t - 0.5) / 0.5 * math.pi * 1.1
            pts.append((0.008 * s * (1 - math.cos(a)), 0.0, -0.008 * s * math.sin(a)))
    bm = K.loft(pts, [0.0011 * s] * len(pts), 6)
    K.rot(bm, "Z", turn)
    K.move(bm, x, 0.0, z)
    K.mesh_obj("hook", bm, "bronze", subsurf=0)
    eye = K.torus((x, 0.0, z + 0.031 * s), 0.0025 * s, 0.0008 * s, plane="XZ")
    K.mesh_obj("hook_eye", eye, "bronze")


@item("fish_hooks", (1, 1), name="Бронзовые крючки", pose="flat", azimuth=14, elevation=20, margin=0.88)
def fish_hooks():
    """Четыре бронзовых крючка с поводками из конского волоса на лоскуте кожи."""
    rod_mats()
    K.mesh_obj("scrap", K.cbox(0, 0.004, 0.0, 0.07, 0.003, 0.075), "leather", bevel=0.001)
    for k in range(4):
        x = -0.045 + k * 0.03
        hook_mesh(x, -0.012 + (k % 2) * 0.01, 1.0, (k - 1.5) * 6)
        K.mesh_obj("snood", K.loft([(x, 0, 0.02 + (k % 2) * 0.01), (x + 0.004, -0.001, 0.05), (x - 0.01 + k * 0.004, -0.001, 0.068)], [0.0006] * 3, 5), "horsehair")
    sinker = K.lathe([(0.0, 0.0001), (0.004, 0.005), (0.014, 0.006), (0.02, 0.0001)], 10)
    K.move(sinker, 0.045, -0.006, -0.045)
    K.mesh_obj("sinker", sinker, "lead")


@item("sea_worms", (1, 1), name="Морские черви", pose="upright", azimuth=20, elevation=28, margin=0.9)
def sea_worms():
    """Нереиды в глиняной плошке с мокрым песком — наживка на мелкую рыбу."""
    K.common_materials()
    K.organic_mat("clay_pot", (170, 98, 60), (120, 64, 38), rough=(0.65, 0.85), bump=0.3, bump_scale=90.0, dirt=0.7, wear=0.3, wear_color=(200, 130, 90))
    K.organic_mat("wet_sand", (120, 104, 80), (82, 70, 52), rough=(0.5, 0.75), bump=0.8, bump_scale=700.0, dirt=0.4)
    K.organic_mat("worm", (166, 84, 74), (96, 46, 44), rough=(0.15, 0.3), bump=0.4, bump_scale=900.0, dirt=0.3, sheen=0.3)
    K.mesh_obj("pot", K.lathe([(0.0, 0.0001), (0.004, 0.04), (0.035, 0.05), (0.05, 0.055), (0.052, 0.05), (0.048, 0.044), (0.038, 0.04), (0.01, 0.0001)], 24), "clay_pot", subsurf=1)
    K.mesh_obj("sand", K.lathe([(0.036, 0.0001), (0.04, 0.03), (0.043, 0.0001)], 20), "wet_sand", subsurf=1)
    rng = _rng("worms")
    for k in range(6):
        a = rng.uniform(0, math.tau)
        pts = []
        for i in range(14):
            t = i / 13
            r = 0.012 + 0.03 * t
            pts.append((math.cos(a + t * 2.4) * r, math.sin(a + t * 2.4) * r, 0.042 + 0.012 * math.sin(t * 6 + k) + (0.012 * (t - 0.6) * 4 if t > 0.75 and k % 2 else 0.0)))
        radii = [0.0028 * (1 - 0.6 * abs(2 * i / 13 - 1) ** 2) for i in range(14)]
        K.mesh_obj("worm", K.loft(pts, radii, 8), "worm", subsurf=1)


@item("shrimp", (1, 1), name="Креветки", pose="flat", azimuth=14, elevation=18, margin=0.9)
def shrimp():
    """Горстка черноморских травяных креветок — наживка на хищника."""
    K.common_materials()
    K.organic_mat("shrimp_shell", (200, 176, 150), (150, 110, 86), rough=(0.2, 0.35), bump=0.3, bump_scale=400.0, dirt=0.2, sheen=0.3)
    K.organic_mat("shrimp_eye", (20, 18, 16), (8, 8, 8), rough=(0.1, 0.2), bump=0.0, dirt=0.0)
    rng = _rng("shrimp")
    for k in range(5):
        cx, cz = rng.uniform(-0.03, 0.03), rng.uniform(-0.03, 0.03)
        turn = rng.uniform(0, 360)
        start = len(K.PARTS)
        pts, radii = [], []
        for i in range(16):
            t = i / 15
            a = t * 2.3
            pts.append((0.022 * (1 - math.cos(a)), 0.0, 0.034 * math.sin(a) * 0.95))
            radii.append((0.006 * (1 - 0.55 * t) + 0.0008, 0.0075 * (1 - 0.6 * t) + 0.001))
        K.mesh_obj("shell", K.loft(pts, radii, 10), "shrimp_shell", subsurf=1)
        for y in (-1, 1):
            e = K.lathe([(0.0, 0.0014), (0.0012, 0.0012), (0.0018, 0.0001)], 8)
            K.rot(e, "X", 90 * y)
            K.move(e, 0.002, y * 0.005, -0.002)
            K.mesh_obj("eye", e, "shrimp_eye")
        for y in (-1, 1):
            K.mesh_obj("antenna", K.loft([(0.0, y * 0.003, 0.0), (-0.02, y * 0.012, -0.02), (-0.03, y * 0.02, -0.05)], [0.0005, 0.0004, 0.0002], 4), "shrimp_shell")
        for o in K.PARTS[start:]:
            o.data.transform(K.Matrix.Rotation(math.radians(turn), 4, "Y"))
            o.data.transform(K.Matrix.Translation((cx, -0.004 * k, cz)))
