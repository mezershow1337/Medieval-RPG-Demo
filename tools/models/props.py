"""Шлемы, щиты и оружие Херсонеса 510 г. н. э. (процедурная геометрия и запечённые текстуры).

Шлемы строятся в позе покоя игры и привязываются к кости головы. Оружие — в своих координатах:
клинок/древко идёт по +Y; у спаты, топора и сакса начало координат — у гарды (правая рука на
(0,-0.06,0)), у копья — у передней руки, у лука — на рукояти. Щит смотрит лицом в -Z.
"""

import numpy as np

import geom  # noqa: E402
import photo
from bake import fbm, smooth, value_noise

HEAD_C = np.array([0.0, 1.655, -0.054])  # центр обода шлема
HEAD_RX, HEAD_RZ = 0.085, 0.114


def srgb(*c):
    return np.array(c, float)


# ---------------------------------------------------------------------------------------------
# Материалы-шейдеры


def iron(P, seed=80, polish=0.0, tone=(0.46, 0.46, 0.48)):
    n = fbm(P, 60.0, 3, seed)
    # Фото кованого железа (ambientCG Metal052C) и ржавчины (Metal053C), CC0.
    forged = photo.here(P + seed * 0.37, "Metal052C", 0.3)
    rust_photo = photo.rust_here(P + seed * 0.53)
    rust = np.clip(smooth(0.6, 0.85, fbm(P, 18.0, 3, seed + 1)) * 0.5 + rust_photo * 0.8, 0, 1) * (1 - polish) ** 1.5
    scratches = value_noise(P * np.array([1, 20, 1]), 300.0, seed + 2)
    n = np.clip(n * 0.5 + (forged - 1.0) * 1.5 + 0.25, 0, 1)
    col = srgb(*tone) * (0.72 + 0.32 * n + 0.08 * scratches)[..., None]
    col = col * (1 - 0.5 * rust[..., None]) + srgb(0.36, 0.2, 0.09) * 0.5 * rust[..., None]
    rough = 0.32 + 0.25 * n + 0.35 * rust - 0.15 * polish
    metal = 0.95 - 0.6 * rust
    return col, rough, metal, n * 0.3 + rust * 0.5


def bronze(P, seed=90):
    n = np.clip(fbm(P, 50.0, 3, seed) * 0.5 + (photo.here(P + seed * 0.29, "Metal047B", 0.3) - 1.0) * 1.2 + 0.25, 0, 1)
    patina = smooth(0.62, 0.85, fbm(P, 15.0, 3, seed + 1))
    col = srgb(0.78, 0.56, 0.28) * (0.8 + 0.3 * n)[..., None]
    col = col * (1 - 0.6 * patina[..., None]) + srgb(0.25, 0.42, 0.33) * 0.6 * patina[..., None]
    return col, 0.3 + 0.2 * n + 0.4 * patina, 0.95 - 0.7 * patina, n * 0.3


def wood(P, seed=100, color=(0.45, 0.3, 0.17), axis=1):
    q = P.copy()
    rings = np.sin((np.hypot(q[..., (axis + 1) % 3], q[..., (axis + 2) % 3]) * 300 + fbm(P, 8.0, 3, seed) * 8.0))
    grain = fbm(P * np.where(np.arange(3) == axis, 0.08, 1.0), 120.0, 3, seed + 1)
    # Волокна настоящего дерева (Wood060, CC0), вытянутые вдоль оси детали.
    stretch = np.where(np.arange(3) == axis, 0.35, 1.0)
    fib = photo.here(P * stretch + seed * 0.41, "Wood060", 0.4)
    grain = np.clip(grain * 0.4 + (fib - 1.0) * 1.3 + 0.3, 0, 1)
    col = srgb(*color) * (0.72 + 0.12 * rings + 0.4 * grain)[..., None]
    return col, 0.7 + 0.1 * grain, np.zeros_like(grain), grain * 0.6 + rings * 0.2


def leather_wrap(P, seed=110, color=(0.3, 0.18, 0.1), pitch=0.012):
    t = (P[..., 1] / pitch + np.arctan2(P[..., 0], P[..., 2]) / (2 * np.pi)) % 1.0
    ridge = smooth(0.0, 0.2, t) * smooth(1.0, 0.8, t)
    g = np.clip(fbm(P, 400.0, 2, seed) * 0.4 + (photo.here(P + seed * 0.3, "Leather014", 0.3) - 1.0) * 1.2 + 0.3, 0, 1)
    col = srgb(*color) * (0.58 + 0.4 * ridge + 0.25 * g)[..., None]
    return col, 0.6 + 0.2 * g, np.zeros_like(g), ridge


def pack(col, rough, metal, height):
    return {"color": col, "rough": rough, "metal": metal, "height": height}


def mix(mask, a, b):
    """Смешивает два набора (col, rough, metal, height) по маске (1 — b)."""
    m = mask[..., None]
    return (a[0] * (1 - m) + b[0] * m, a[1] * (1 - mask) + b[1] * mask, a[2] * (1 - mask) + b[2] * mask, a[3] * (1 - mask) + b[3] * mask)


# ---------------------------------------------------------------------------------------------
# Шлемы


def patch(rows, cols_fn, r_fn, center, rx, rz):
    """Кусок поверхности вокруг вертикали: для каждой высоты y — свой диапазон углов и радиус."""
    v, uv = [], []
    ncol = 12
    for i, y in enumerate(rows):
        a0, a1 = cols_fn(y)
        r = r_fn(y)
        for j, a in enumerate(np.linspace(a0, a1, ncol)):
            v.append((center[0] + r * rx * np.sin(a), center[1] + y, center[2] - r * rz * np.cos(a)))
            uv.append((j / (ncol - 1), i / (len(rows) - 1)))
    return np.array(v), geom.grid_tris(len(rows), ncol), np.array(uv)


def tilt(mesh, k=-0.18):
    """Обод шлема наклонён: спереди выше, сзади ниже."""
    v, t, uv = mesh
    v = v.copy()
    v[:, 1] += k * (v[:, 2] - HEAD_C[2])
    return v, t, uv


def build_headgear(bld):
    C = HEAD_C

    # --- Спангенхельм: шесть бронзовых полос-«спанг», железные пластины между ними, обод,
    # нащёчники и кольчужная бармица сзади.
    dome = tilt(geom.lathe([(1.0, -0.004), (1.0, 0.03), (0.96, 0.06), (0.86, 0.095), (0.7, 0.125), (0.48, 0.148), (0.24, 0.162), (0.06, 0.168), (0.0, 0.169)], 48, HEAD_RX, HEAD_RZ, C))
    band = tilt(geom.lathe([(1.0, -0.006), (1.045, -0.004), (1.05, 0.016), (1.045, 0.036), (1.0, 0.038)], 48, HEAD_RX, HEAD_RZ, C))
    finial = geom.lathe([(0.0, 0.0), (0.012, 0.0), (0.01, 0.012), (0.004, 0.022), (0.0, 0.026)], 12, 1.0, 1.0, C + [0, 0.165, 0])
    cheeks = []
    for s in (1, -1):
        def cols(y, s=s):
            w = 0.55 + 0.35 * (y + 0.12) / 0.12
            mid = s * 1.42
            return (mid - w / 2, mid + w / 2) if s > 0 else (mid - w / 2, mid + w / 2)
        cheeks.append(tilt(patch(np.linspace(-0.115, 0.002, 8), cols, lambda y: 1.06 - 0.25 * max(-y - 0.05, 0), C, HEAD_RX, HEAD_RZ)))
    aventail = tilt(patch(np.linspace(-0.17, 0.002, 10), lambda y: (np.pi * 0.62, np.pi * 1.38), lambda y: 1.05 + 0.25 * min(-y, 0.12) / 0.12, C, HEAD_RX, HEAD_RZ))

    def spangen_shader(P, N, hit):
        q = P - C
        ang = np.arctan2(q[..., 0] / HEAD_RX, -q[..., 2] / HEAD_RZ)
        seg = (ang / (2 * np.pi) * 6 + 0.5) % 1.0
        strap = smooth(0.1, 0.07, np.abs(seg - 0.5) - 0.0) * 0 + smooth(0.07, 0.05, np.minimum(seg, 1 - seg))
        rivets = strap * smooth(0.5, 0.3, np.abs(((q[..., 1] * 45) % 1.0) - 0.5)) * smooth(0.02, 0.0, np.minimum(seg, 1 - seg))
        a = iron(P, 81)
        b = bronze(P, 91)
        col, rough, metal, h = mix(np.clip(strap, 0, 1), a, b)
        return pack(col, rough, metal, h + strap * 1.5 + rivets * 2)

    def bronze_shader(P, N, hit):
        col, rough, metal, h = bronze(P, 92)
        # Чеканный узор на ободе: ряд точек.
        dots = smooth(0.35, 0.2, np.abs(((np.arctan2(P[..., 0], P[..., 2]) * 40) % 1.0) - 0.5)) * smooth(0.006, 0.003, np.abs(P[..., 1] - C[1] - 0.016 + 0.18 * (P[..., 2] - C[2])))
        return pack(col, rough, metal, h + dots)

    def iron_shader(P, N, hit):
        return pack(*iron(P, 83))

    import build_humans as bh  # noqa: E402

    m_dome = bld.rigid_bake(dome, 1024, spangen_shader, "helm_spangen", normal_strength=3.0)
    m_band = bld.rigid_bake(geom.merge(band, finial), 512, bronze_shader, "helm_band", normal_strength=3.0)
    m_cheek = bld.rigid_bake(geom.merge(*cheeks), 512, iron_shader, "helm_cheeks", normal_strength=2.0, double=True)
    m_mail = bld.rigid_bake(aventail, 1024, bh.mail_shader, "helm_aventail", normal_strength=4.0, double=True)
    bld.emit_rigid("helm.spangen", [(dome, m_dome), (geom.merge(band, finial), m_band), (geom.merge(*cheeks), m_cheek), (aventail, m_mail)], bone="head")

    # --- Высокий конический шлем (аланы, гвардия): склёпанный из сегментов, бармица кругом.
    cone = tilt(geom.lathe([(1.0, -0.004), (1.0, 0.025), (0.93, 0.07), (0.78, 0.12), (0.56, 0.165), (0.32, 0.2), (0.12, 0.225), (0.0, 0.235)], 48, HEAD_RX, HEAD_RZ, C))
    band2 = tilt(geom.lathe([(1.0, -0.006), (1.04, -0.004), (1.045, 0.02), (1.0, 0.024)], 48, HEAD_RX, HEAD_RZ, C))
    nasal = tilt(geom.box(0.016, 0.075, 0.006, C + [0, -0.03, -HEAD_RZ * 1.05]))
    aventail2 = tilt(patch(np.linspace(-0.18, 0.002, 10), lambda y: (np.pi * 0.3, np.pi * 1.7), lambda y: 1.06 + 0.3 * min(-y, 0.12) / 0.12, C, HEAD_RX, HEAD_RZ))
    m_cone = bld.rigid_bake(cone, 1024, spangen_shader, "helm_cone", normal_strength=3.0)
    m_b2 = bld.rigid_bake(geom.merge(band2, nasal), 512, iron_shader, "helm_cone_band", normal_strength=2.0)
    m_m2 = bld.rigid_bake(aventail2, 1024, bh.mail_shader, "helm_cone_aventail", normal_strength=4.0, double=True)
    bld.emit_rigid("helm.conical", [(cone, m_cone), (geom.merge(band2, nasal), m_b2), (aventail2, m_m2)], bone="head")

    # --- Войлочная шапка кочевника с меховым отворотом.
    cap = tilt(geom.lathe([(1.02, -0.01), (1.02, 0.03), (0.95, 0.08), (0.78, 0.13), (0.52, 0.17), (0.25, 0.2), (0.0, 0.215)], 40, HEAD_RX, HEAD_RZ, C + [0, 0, 0.006]), -0.12)
    brim = tilt(geom.lathe([(1.0, -0.016), (1.12, -0.012), (1.16, 0.01), (1.12, 0.035), (1.02, 0.04)], 40, HEAD_RX, HEAD_RZ, C), -0.12)

    def felt_shader(P, N, hit):
        n = fbm(P, 150.0, 4, 120)
        col = srgb(0.5, 0.36, 0.22) * (0.8 + 0.3 * n)[..., None]
        return pack(col, 0.95 + 0.0 * n, np.zeros_like(n), n)

    def fur_shader(P, N, hit):
        n = fbm(P * np.array([1, 0.3, 1]), 900.0, 3, 121)
        col = srgb(0.28, 0.2, 0.13) * (0.5 + 0.8 * n)[..., None]
        return pack(col, 0.9 + 0.0 * n, np.zeros_like(n), n * 2)

    bld.emit_rigid("cap.felt", [(cap, bld.rigid_bake(cap, 512, felt_shader, "cap_felt", normal_strength=2.0, metallic=0.0)),
                                (brim, bld.rigid_bake(brim, 512, fur_shader, "cap_fur", normal_strength=3.0, metallic=0.0))], bone="head")


# ---------------------------------------------------------------------------------------------
# Оружие и щиты


def blade_profile(length, w0, w1, tip, t, n=14, y0=0.0):
    """Сечения клинка: ширина от w0 у гарды до w1 перед остриём, остриё длиной tip."""
    ys, secs = [], []
    for i in range(n):
        y = y0 + (length - tip) * i / (n - 1)
        k = i / (n - 1)
        ys.append(y)
        secs.append(geom.lens_section(w0 + (w1 - w0) * k, t * (1 - 0.3 * k), 16))
    for i in range(1, 6):
        k = i / 5
        y = y0 + length - tip + tip * k
        w = w1 * np.sqrt(max(1 - k, 0)) * (1 - 0.15 * k) + 0.0005
        ys.append(y)
        secs.append(geom.lens_section(w, t * 0.7 * (1 - 0.7 * k) + 0.0006, 16))
    return geom.sweep(secs, ys, cap=True)


def build_weapons(bld):
    # --- Спата: клинок ~78 см с долом, гарда, рукоять с кольцами, грибовидное навершие.
    blade = blade_profile(0.78, 0.052, 0.046, 0.09, 0.006, y0=0.012)

    def blade_shader(P, N, hit):
        col, rough, metal, h = iron(P, 130, polish=0.8, tone=(0.62, 0.62, 0.64))
        fuller = smooth(0.009, 0.005, np.abs(P[..., 0])) * smooth(0.02, 0.05, P[..., 1]) * smooth(0.66, 0.6, P[..., 1])
        # Узорная сварка (дамаск) в доле.
        pattern = 0.5 + 0.5 * np.sin(P[..., 1] * 400 + np.sin(P[..., 0] * 900) * 2)
        col = col * (1 - 0.25 * fuller * pattern)[..., None]
        return pack(col, rough, metal, h - fuller * 2)

    guard = geom.box(0.085, 0.014, 0.026, (0, 0.005, 0))
    grip = geom.sweep([geom.ellipse(0.014, 0.012, 12)] * 2, [-0.11, -0.002], cap=True)
    pommel = geom.lathe([(0.0, -0.152), (0.022, -0.15), (0.03, -0.138), (0.026, -0.126), (0.012, -0.118), (0.0, -0.116)], 16, 1, 0.8)

    def hilt_shader(P, N, hit):
        w = wood(P, 140, (0.35, 0.22, 0.12))
        rings = smooth(0.004, 0.002, np.abs(((P[..., 1] + 0.11) / 0.022 % 1.0) - 0.5) * 0.022) * (P[..., 1] > -0.115) * (P[..., 1] < 0.0)
        br = bronze(P, 141)
        metal_mask = np.clip((P[..., 1] > -0.004) + (P[..., 1] < -0.112) + rings, 0, 1).astype(float)
        col, rough, metal, h = mix(metal_mask, w, br)
        return pack(col, rough, metal, h + rings)

    hilt = geom.merge(guard, grip, pommel)
    m_blade = bld.rigid_bake(blade, 1024, blade_shader, "blade", normal_strength=2.0)
    m_hilt = bld.rigid_bake(hilt, 512, hilt_shader, "spatha_hilt", normal_strength=2.0)
    bld.emit_rigid("spatha.blade", [(blade, m_blade)])
    bld.emit_rigid("spatha.hilt", [(hilt, m_hilt)])

    # --- Сакс: однолезвийный нож ~35 см, без гарды.
    secs, ys = [], []
    for i in range(12):
        k = i / 11
        y = 0.0 + 0.36 * k
        w = 0.038 * (1 - 0.15 * k) if k < 0.75 else 0.038 * 0.89 * np.sqrt(max(1 - (k - 0.75) / 0.25, 0)) + 0.001
        s = geom.lens_section(w, 0.007 * (1 - 0.5 * k) + 0.0008, 16)
        # Спинка толстая и прямая, лезвие тонкое: сдвигаем сечение.
        s[:, 0] += 0.5 * w - 0.019
        s[:, 1] *= np.where(s[:, 0] > 0.0, 1.3, 0.7)
        secs.append(s)
        ys.append(y)
    seax_blade = geom.sweep(secs, ys)
    seax_hilt = geom.merge(geom.sweep([geom.ellipse(0.013, 0.011, 12), geom.ellipse(0.015, 0.012, 12), geom.ellipse(0.012, 0.01, 12)], [-0.13, -0.06, 0.0]),
                           geom.lathe([(0.0, -0.142), (0.014, -0.14), (0.016, -0.132), (0.0, -0.128)], 12, 1, 0.8))
    bld.emit_rigid("seax.blade", [(seax_blade, m_blade)])
    bld.emit_rigid("seax.hilt", [(seax_hilt, bld.rigid_bake(seax_hilt, 512, lambda P, N, h: pack(*wood(P, 150, (0.3, 0.2, 0.12))), "seax_hilt", metallic=0.0))])

    # --- Боевой топор готов: топорище ~70 см, бородовидное лезвие вперёд (-Z).
    haft = geom.sweep([geom.ellipse(0.015, 0.019, 12)] * 2, [-0.12, 0.62], cap=True)
    head_v = []
    prof = [(-0.025, 0.52), (-0.03, 0.47), (-0.06, 0.45), (-0.13, 0.43), (-0.16, 0.48), (-0.165, 0.56), (-0.15, 0.6), (-0.11, 0.59), (-0.05, 0.6), (-0.02, 0.62), (0.03, 0.61), (0.035, 0.53)]
    poly = np.array(prof)
    m = len(poly)
    secs_front = np.c_[np.zeros(m), poly[:, 1], poly[:, 0]]
    thick = np.where(poly[:, 0] < -0.1, 0.002, 0.012)
    top = secs_front + np.c_[thick / 2, np.zeros(m), np.zeros(m)]
    bot = secs_front - np.c_[thick / 2, np.zeros(m), np.zeros(m)]
    head_v = np.r_[top, bot]
    tris = []
    for j in range(m):
        a, b = j, (j + 1) % m
        tris += [(a, b, m + b), (a, m + b, m + a)]
    for j in range(1, m - 1):
        tris += [(0, j + 1, j), (m, m + j, m + j + 1)]
    head_uv = np.c_[(head_v[:, 2] + 0.17) / 0.21, (head_v[:, 1] - 0.42) / 0.21]
    axe_head = (head_v, np.array(tris), head_uv)

    def axe_shader(P, N, hit):
        col, rough, metal, h = iron(P, 160, polish=0.3)
        edge = smooth(-0.14, -0.16, P[..., 2])
        col = col * (1 + 0.3 * edge)[..., None]
        return pack(col, rough - 0.15 * edge, metal, h)

    bld.emit_rigid("axe.blade", [(axe_head, bld.rigid_bake(axe_head, 512, axe_shader, "axe_head", normal_strength=2.0))])
    bld.emit_rigid("axe.hilt", [(haft, bld.rigid_bake(haft, 512, lambda P, N, h: pack(*wood(P, 161, (0.5, 0.36, 0.22))), "axe_haft", metallic=0.0))])

    # --- Контос: древко ~2,55 м, листовидный наконечник; начало координат — у передней руки.
    shaft = geom.sweep([geom.ellipse(0.016, 0.016, 10), geom.ellipse(0.017, 0.017, 10), geom.ellipse(0.014, 0.014, 10)], [-0.72, 0.4, 1.86])
    socket = geom.lathe([(0.0, 1.84), (0.018, 1.84), (0.016, 1.92), (0.012, 1.95)], 12)
    tip_secs, tip_ys = [], []
    for i in range(10):
        k = i / 9
        w = 0.045 * np.sin(np.pi * min(k * 1.25, 1.0) * 0.5 + 0.2) * (1 - k) ** 0.6 + 0.0015
        tip_secs.append(geom.lens_section(w, 0.008 * (1 - 0.6 * k) + 0.002, 12))
        tip_ys.append(1.95 + 0.22 * k)
    spear_tip = geom.merge(socket, geom.sweep(tip_secs, tip_ys))
    butt = geom.lathe([(0.0, -0.76), (0.012, -0.76), (0.018, -0.72), (0.0, -0.72)], 10)
    bld.emit_rigid("spear.blade", [(spear_tip, bld.rigid_bake(spear_tip, 512, lambda P, N, h: pack(*iron(P, 170, polish=0.6)), "spear_head", normal_strength=2.0))])
    spear_shaft = geom.merge(shaft, butt)
    bld.emit_rigid("spear.hilt", [(spear_shaft, bld.rigid_bake(spear_shaft, 1024, lambda P, N, h: pack(*wood(P, 171, (0.55, 0.42, 0.27))), "spear_shaft", metallic=0.0))])

    # --- Составной гуннский лук: несимметричный (верхнее плечо длиннее), костяные накладки
    # на рукояти и на жёстких концах (сиях). Тетива со стороны стрелка (+Z).
    pts = []
    for t in np.linspace(-1, 1, 25):
        L = 0.62 if t < 0 else 0.76
        y = t * L
        z = 0.06 * (1 - t * t) - 0.04 * smooth(0.75, 1.0, abs(t)) * (abs(t) > 0.75)
        z = 0.05 * (1 - abs(t)) ** 0.5 - 0.09 * smooth(0.72, 1.0, abs(t))
        pts.append((0.0, y, z))
    pts = np.array(pts)
    secs = []
    for i, p in enumerate(pts):
        t = np.linspace(-1, 1, len(pts))[i]
        w = 0.032 if abs(t) < 0.12 else (0.026 * (1 - 0.5 * abs(t)) + 0.006)
        th = 0.026 if abs(t) < 0.12 else 0.016 * (1 - 0.4 * abs(t)) + 0.006
        secs.append(geom.ellipse(w / 2, th / 2, 10))
    limbs = geom.sweep(secs, path=pts)
    string = geom.sweep([geom.ellipse(0.0015, 0.0015, 6)] * 2, path=[pts[0] + [0, 0.01, 0.006], pts[-1] + [0, -0.01, 0.006]])

    def bow_shader(P, N, hit):
        y = P[..., 1]
        sinew = (0.55 + 0.25 * fbm(P, 300.0, 3, 180))
        col = srgb(0.42, 0.28, 0.15) * sinew[..., None]
        bone = np.clip(smooth(0.1, 0.08, np.abs(y)) + smooth(0.5, 0.55, -y) + smooth(0.6, 0.66, y), 0, 1)
        col = col * (1 - bone[..., None]) + srgb(0.85, 0.8, 0.68) * bone[..., None] * (0.85 + 0.2 * fbm(P, 500.0, 2, 181))[..., None]
        birch = smooth(0.12, 0.14, np.abs(y)) * smooth(0.5, 0.45, np.abs(y))
        col = col * (1 - 0.3 * birch[..., None]) + srgb(0.6, 0.5, 0.35) * 0.3 * birch[..., None]
        return pack(col, 0.6 - 0.2 * bone, np.zeros_like(y), bone * 0.5 + sinew)

    bld.emit_rigid("bow.blade", [(limbs, bld.rigid_bake(limbs, 1024, bow_shader, "bow", normal_strength=1.5, metallic=0.0)),
                                 (string, bld.rigid_bake(string, 64, lambda P, N, h: pack(np.full(P.shape, 0.75), np.full(P.shape[:2], 0.7), np.zeros(P.shape[:2]), np.zeros(P.shape[:2])), "bowstring", metallic=0.0))])
    # У лука нет отдельной рукояти, но деталь .hilt нужна по соглашению.
    grip_wrap = geom.sweep([geom.ellipse(0.019, 0.016, 10)] * 2, [-0.06, 0.06])
    bld.emit_rigid("bow.hilt", [(grip_wrap, bld.rigid_bake(grip_wrap, 256, lambda P, N, h: pack(*leather_wrap(P, 182)), "bow_grip", metallic=0.0))])

    build_shields(bld)


def shield_disc(rx, ry, bulge, rings=10, m=48):
    """Слегка выпуклый щит (лицо в -Z) с толщиной: лицевая и тыльная стороны + кромка."""
    v, uv = [], []
    for i in range(rings + 1):
        r = i / rings
        for j in range(m):
            a = 2 * np.pi * j / m
            x, y = rx * r * np.cos(a), ry * r * np.sin(a)
            z = -bulge * (1 - r * r)
            v.append((x, y, z))
            uv.append((0.5 + 0.5 * r * np.cos(a), 0.5 + 0.5 * r * np.sin(a)))
    front = np.array(v)
    back = front + [0, 0, 0.012]
    nf = len(front)
    tris = []
    for i in range(rings):
        for j in range(m):
            a = i * m + j
            b = i * m + (j + 1) % m
            c = (i + 1) * m + (j + 1) % m
            d = (i + 1) * m + j
            tris += [(a, b, c), (a, c, d)]
            tris += [(nf + a, nf + c, nf + b), (nf + a, nf + d, nf + c)]
    uvb = np.array(uv) * 0.48 + [0.51, 0.01]
    uvf = np.array(uv) * 0.48 + [0.01, 0.01]
    # Кромка — отдельные вершины со своей полосой UV сверху развёртки.
    o = rings * m
    rim_f = front[o:o + m]
    rim_b = back[o:o + m]
    rv = np.r_[rim_f, rim_f[:1], rim_b, rim_b[:1]]
    ruv = np.r_[np.c_[np.linspace(0, 1, m + 1), np.full(m + 1, 0.93)], np.c_[np.linspace(0, 1, m + 1), np.full(m + 1, 0.99)]]
    base = 2 * nf
    for j in range(m):
        a, b = base + j, base + j + 1
        c, d = base + m + 1 + j, base + m + 2 + j
        tris += [(a, d, b), (a, c, d)]
    return np.r_[front, back, rv], np.array(tris), np.r_[uvf, uvb, ruv]


def build_shields(bld):
    # --- Большой овальный щит гарнизона (~1,05 х 0,85 м): красное поле, концентрические кольца,
    # хризма (☧) — так расписывали щиты позднеримских и ранневизантийских частей.
    oval = shield_disc(0.42, 0.52, 0.05)

    def oval_shader(P, N, hit):
        x, y, z = P[..., 0], P[..., 1], P[..., 2]
        back = N[..., 2] > 0.3
        r = np.hypot(x / 0.42, y / 0.52)
        paint_n = fbm(P, 40.0, 3, 190)
        red = srgb(0.55, 0.12, 0.08)
        col = np.broadcast_to(red, P.shape).copy()
        ring1 = smooth(0.03, 0.015, np.abs(r - 0.62))
        ring2 = smooth(0.02, 0.01, np.abs(r - 0.42))
        col = col * (1 - ring1[..., None]) + srgb(0.85, 0.72, 0.35) * ring1[..., None]
        col = col * (1 - ring2[..., None]) + srgb(0.9, 0.88, 0.8) * ring2[..., None]
        # Хризма: вертикаль с петлёй (Р) и косой крест (Х) в центральном поле.
        rr = np.hypot(x, y)
        chi = (smooth(0.012, 0.006, np.abs(x - y) / 1.414) + smooth(0.012, 0.006, np.abs(x + y) / 1.414)) * smooth(0.17, 0.16, rr) * smooth(0.07, 0.08, rr)
        rho = smooth(0.012, 0.006, np.abs(x)) * smooth(0.17, 0.16, rr) * smooth(0.07, 0.08, rr)
        loop = smooth(0.012, 0.006, np.abs(np.hypot(x - 0.03, y - 0.13) - 0.035)) * (x > -0.005)
        sym = np.clip(chi + rho + loop, 0, 1)
        col = col * (1 - sym[..., None]) + srgb(0.92, 0.9, 0.82) * sym[..., None]
        col *= (0.85 + 0.2 * paint_n)[..., None]
        # Кожаная окантовка.
        rim = smooth(0.95, 0.98, r)
        col = col * (1 - rim[..., None]) + srgb(0.3, 0.2, 0.12) * rim[..., None]
        w = wood(P * [1, 1, 1], 191, (0.5, 0.38, 0.24))
        col = np.where(back[..., None], w[0], col)
        rough = np.where(back, 0.8, 0.55 + 0.2 * paint_n)
        scratches = smooth(0.7, 0.9, value_noise(P * [1, 1, 1], 120.0, 192))
        col *= (1 - 0.25 * scratches)[..., None]
        return pack(col, rough, np.zeros_like(rough), paint_n * 0.3 + rim + scratches * 0.3)

    boss = geom.lathe([(0.075, 0.0), (0.075, 0.008), (0.065, 0.012), (0.06, 0.03), (0.045, 0.055), (0.02, 0.068), (0.0, 0.07)], 24, 1, 1, (0, 0, 0))
    # Умбон торчит вперёд (-Z): поворачиваем ось Y лейзы в -Z.
    R = np.array([[1, 0, 0], [0, 0, 1], [0, -1, 0]], float)
    boss = geom.transform(boss, R, (0, 0, -0.045))
    grip_bar = geom.box(0.03, 0.2, 0.02, (0, -0.02, 0.07 - 0.0))
    grip_bar = geom.transform(grip_bar, np.array([[0, 1, 0], [-1, 0, 0], [0, 0, 1]], float).T, (0, 0, 0))
    m_oval = bld.rigid_bake(oval, 2048, oval_shader, "shield_oval", normal_strength=2.0, metallic=0.0, double=True)
    m_boss = bld.rigid_bake(geom.merge(boss, grip_bar), 512, lambda P, N, h: pack(*iron(P, 193)), "shield_boss", normal_strength=2.0)
    bld.emit_rigid("shield_oval.body", [(oval, m_oval)])
    bld.emit_rigid("shield_oval.boss", [(geom.merge(boss, grip_bar), m_boss)])

    # --- Круглый щит готов (~0,85 м): доски, обтянутые кожей, высокий умбон.
    rnd = shield_disc(0.43, 0.43, 0.025)

    def round_shader(P, N, hit):
        x, y = P[..., 0], P[..., 1]
        back = N[..., 2] > 0.3
        r = np.hypot(x, y) / 0.43
        planks = np.abs(((x + 0.43) / 0.11) % 1.0 - 0.5)
        seam = smooth(0.48, 0.5, planks)
        w = wood(P, 200, (0.46, 0.33, 0.2), axis=1)
        col = w[0] * (1 - 0.5 * seam[..., None])
        # Лицо: выцветшая краска — чёрные и охристые четверти.
        q = (np.arctan2(y, x) // (np.pi / 2)) % 2
        paint = np.where(q[..., None] > 0, srgb(0.18, 0.15, 0.12), srgb(0.62, 0.45, 0.2))
        wear = smooth(0.4, 0.7, fbm(P, 12.0, 3, 201))
        col = np.where(back[..., None], col, paint * (1 - wear[..., None] * 0.6) + col * wear[..., None] * 0.6)
        rim = smooth(0.95, 0.98, r)
        col = col * (1 - rim[..., None]) + srgb(0.28, 0.18, 0.1) * rim[..., None]
        return pack(col, 0.75 + 0.1 * wear, np.zeros_like(r), seam + rim + w[3])

    boss2 = geom.lathe([(0.08, 0.0), (0.08, 0.008), (0.07, 0.012), (0.065, 0.04), (0.035, 0.07), (0.015, 0.1), (0.012, 0.11), (0.0, 0.112)], 24)
    boss2 = geom.transform(boss2, R, (0, 0, -0.02))
    m_r = bld.rigid_bake(rnd, 2048, round_shader, "shield_round", normal_strength=2.0, metallic=0.0, double=True)
    bld.emit_rigid("shield_round.body", [(rnd, m_r)])
    bld.emit_rigid("shield_round.boss", [(geom.merge(boss2, grip_bar), m_boss)])
