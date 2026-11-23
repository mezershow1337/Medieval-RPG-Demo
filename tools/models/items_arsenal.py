"""3D-модели кованого оружия из tools/arsenal_data.py (мечи, саксы, топоры, булавы, копья,
наконечники стрел). Каждая модель собирается из параметров записи — общими «семейными»
построителями, а не руками по одной.

Собрать: python3 tools/models/build_items.py <ключ> ...   (ключи — поле key в arsenal_data.py)
"""
import math
import os
import sys
import zlib

import itemkit as K
from itemkit import item

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import arsenal_data as D  # noqa: E402

# Ось Z — вдоль оружия (острие/навершие вверх), X — ширина клинка, Y — толщина.


def mats():
    K.common_materials()
    K.metal_mat("pattern_steel", (190, 192, 196), (120, 122, 128), rough=(0.2, 0.36), scratches=0.7, rust=0.08, dirt=0.4, polish_dir="Z")
    K.metal_mat("lead_metal", (120, 122, 128), (90, 92, 98), rough=(0.5, 0.7), scratches=0.2, dirt=0.6)


def rnd(key):
    return zlib.crc32(key.encode()) / 0xFFFFFFFF


# =========================================================================== рукояти и навершия


def grip(z0, z1, r, mat, ribs=3):
    rings = []
    n = 24
    for i in range(n + 1):
        t = i / n
        z = z0 + (z1 - z0) * t
        bulge = 1 + 0.16 * math.sin(math.pi * t)
        groove = 1 - 0.08 * max(0.0, math.cos(2 * math.pi * ribs * t)) ** 6 if ribs else 1
        rr = r * bulge * groove
        rings.append((z, rr, rr * 0.78))
    K.mesh_obj("grip", K.lathe(rings, 20), mat, subsurf=1)


def guard(style, mat, width, z=0.0, gems=0):
    if style == "lens":
        g = K.lathe([(z - 0.016, 0.0001), (z - 0.012, width, width * 0.28), (z, width * 1.08, width * 0.32), (z + 0.004, width, width * 0.28), (z + 0.006, 0.0001)], 24)
        K.mesh_obj("guard", g, mat, subsurf=1)
    elif style == "bar":
        K.mesh_obj("guard", K.cbox(0, 0, z - 0.006, width * 2.2, 0.024, 0.014), mat, bevel=0.003)
    elif style == "cross":
        K.mesh_obj("guard", K.cbox(0, 0, z - 0.006, width * 3.4, 0.02, 0.012), mat, bevel=0.003)
        for s in (-1, 1):
            K.mesh_obj("guard_end", K.lathe([(-0.006, 0.0001), (-0.003, 0.008), (0.003, 0.008), (0.006, 0.0001)], 12, axis="X"), mat, subsurf=1)
            K.PARTS[-1].location = (s * width * 1.7, 0, z - 0.006)
    elif style == "disk":
        K.mesh_obj("guard", K.lathe([(z - 0.012, 0.0001), (z - 0.01, width * 1.3, width * 0.9), (z, width * 1.3, width * 0.9), (z + 0.002, 0.0001)], 24), mat, subsurf=1)
    for i in range(gems):
        x = (i - (gems - 1) / 2) * width * 0.9
        cab = K.lathe([(0, 0.005, 0.004), (0.0015, 0.0045, 0.0035), (0.003, 0.0025, 0.002), (0.0035, 0.0001)], 16)
        K.rot(cab, "X", 90)
        K.move(cab, x, -0.012, z - 0.006)
        K.mesh_obj("gem", cab, "garnet", subsurf=1)


def pommel(style, mat, z, r=0.02):
    if style == "mushroom":
        p = K.lathe([(z - 0.03, 0.0001), (z - 0.028, r * 0.9, r * 0.6), (z - 0.018, r * 1.5, r * 0.9), (z - 0.008, r * 1.4, r * 0.8), (z - 0.002, r * 0.8, r * 0.55), (z + 0.002, 0.0001)], 24)
        K.mesh_obj("pommel", p, mat, subsurf=1)
    elif style == "ring":
        K.mesh_obj("pommel", K.torus((0, 0, z - 0.03), 0.026, 0.0055, plane="XZ"), mat)
    elif style == "disk":
        K.mesh_obj("pommel", K.lathe([(z - 0.014, 0.0001), (z - 0.012, r * 1.3), (z, r * 1.3), (z + 0.002, 0.0001)], 24), mat, subsurf=1)
    elif style == "pyramid":
        p = K.lathe([(z - 0.022, 0.0001), (z - 0.02, r * 1.6, r * 0.8), (z - 0.012, r * 1.4, r * 0.7), (z, r * 0.3, r * 0.3), (z + 0.002, 0.0001)], 4)
        K.rot(p, "Z", 45)
        K.mesh_obj("pommel", p, mat, bevel=0.002)
    elif style == "cap":
        K.mesh_obj("pommel", K.lathe([(z - 0.016, 0.0001), (z - 0.014, r * 1.1, r * 0.8), (z - 0.004, r * 1.0, r * 0.75), (z + 0.002, 0.0001)], 20), mat, subsurf=1)
    elif style == "ball":
        rings = [(z - 0.04 + 0.04 * i / 12, max(0.0001, r * 1.2 * math.sin(math.pi * i / 12))) for i in range(13)]
        K.mesh_obj("pommel", K.lathe(rings, 20), mat, subsurf=1)


# =========================================================================== клинки


def blade(key, m, length, w0, w1):
    """Клинок с формой острия, изгибом, одним лезвием и прочим из параметров модели."""
    tip = m.get("tip", "round")
    curve = m.get("curve", 0.0)
    single = m.get("single", False)
    broken = m.get("broken_back", False)
    n = 44
    st = []
    widths = []
    for i in range(n + 1):
        t = i / n
        z = length * t
        w = w0 + (w1 - w0) * t
        if tip == "round":
            ts = 0.86
            if t > ts:
                w *= math.sqrt(max(0.0, 1 - ((t - ts) / (1 - ts)) ** 1.6))
        elif tip == "long":
            ts = 0.72
            if t > ts:
                w *= (1 - (t - ts) / (1 - ts)) ** 1.1
        elif tip == "leaf":
            w = w0 * (0.55 + 0.6 * math.sin(math.pi * min(1.0, t * 1.2) ** 0.8)) * (1 - t ** 3) ** 0.8
        elif tip == "square":
            if t > 0.95:
                w *= 1 - (t - 0.95) / 0.05 * 0.6
        if single and broken and t > 0.7:
            w *= 1 - (t - 0.7) / 0.3 * 0.85
        elif single and tip != "square" and t > 0.8:
            w *= max(0.0, 1 - ((t - 0.8) / 0.2) ** 1.4)
        w = max(w, 0.0004)
        widths.append(w)
        th = (0.0034 if not single else 0.0028) * (1 - 0.35 * t)
        c = curve * length * t * t
        if single:
            # Прямой обух на +X: центр сечения смещён к лезвию.
            c += -w + w0
        st.append((z, w, th, c))
    fuller = None if single else (0.02, length * 0.8, 0.42, 0.0011)
    bm = K.blade(st, 12, fuller=fuller)
    if single:
        back_th = 0.0035

        def thick(v):
            # Обух толстый, к лезвию сечение сходится клином.
            t = min(1.0, max(0.0, v.z / max(length, 1e-6)))
            i = min(n, int(t * n))
            w = widths[i]
            c = st[i][3]
            u = (v.x - c) / max(w, 1e-6)  # -1 — лезвие, +1 — обух
            k = max(0.0, (u + 1) / 2)
            want = back_th * (1 - 0.5 * t) * k ** 0.7
            y = v.y if abs(v.y) >= want else math.copysign(want, v.y if v.y != 0 else 1)
            return (v.x, y, v.z)

        K.deform(bm, thick)
    K.mesh_obj("blade", bm, m.get("bmat", "blade_steel"), smooth_angle=60)


def build_sword(key, m):
    mats()
    L = m["blade"]
    blade(key, m, L, m["w0"], m["w1"])
    g = m.get("guard", "lens")
    if g != "none":
        guard(g, m.get("gmat", "iron"), m["w0"] * 1.6, gems=m.get("gems", 0))
    gl = m.get("grip", 0.1)
    gmat = m.get("gripmat", "wood_dark")
    grip(-gl - 0.016, -0.016, 0.0145, gmat, ribs=4 if m.get("finger_grip") else 3)
    if m.get("wire"):
        for i in range(8):
            z = -gl - 0.01 + i * gl / 8
            K.mesh_obj("wire", K.torus((0, 0, z), 0.0168, 0.0011, ry=0.0132), "silver")
    p = m.get("pommel", "mushroom")
    if p != "none":
        pommel(p, m.get("pmat", "iron"), -gl - 0.016)
    if m.get("ring"):
        K.mesh_obj("oath_ring", K.torus((0.028, 0, -gl - 0.03), 0.012, 0.0028, plane="XZ"), "silver")


# =========================================================================== топоры


def build_axe(key, m):
    mats()
    H = m["haft"]
    rings = [(H * i / 30, 0.016 - 0.002 * i / 30, 0.012 - 0.001 * i / 30) for i in range(31)]
    haft = K.lathe(rings, 16)
    K.deform(haft, lambda v: (v.x + 0.02 * math.sin(math.pi * v.z / H) * 0.6, v.y, v.z))
    K.mesh_obj("haft", haft, "wood", subsurf=0)
    head = m.get("head", "beard")
    hw, hh = m["hw"], m["hh"]
    top_z = H - 0.03
    sock = 0.12 if m.get("long_socket") else 0.08
    K.mesh_obj("eye", K.lathe([(top_z - sock, 0.023, 0.018), (top_z - sock / 2, 0.026, 0.02), (top_z, 0.023, 0.017)], 20), "iron", bevel=0.002, subsurf=1)
    mid = top_z - sock / 2
    st = []
    n = 24
    for i in range(n + 1):
        t = i / n
        x = 0.02 + hw * t
        if head == "beard":
            top, bot = mid + 0.03 + 0.02 * t ** 2, mid - 0.02 - (hh - 0.05) * t ** 2.2
        elif head == "francisca":
            top, bot = mid + 0.02 + hh * 0.6 * t ** 1.6, mid - 0.015 - hh * 0.35 * t ** 1.8
        elif head == "broad":
            top, bot = mid + 0.02 + hh * 0.45 * t ** 2, mid - 0.02 - hh * 0.45 * t ** 2
        elif head in ("narrow", "alan"):
            top, bot = mid + 0.015 + hh * 0.3 * t ** 2, mid - 0.015 - hh * 0.3 * t ** 2
        elif head == "slav":
            top, bot = mid + 0.02 + 0.01 * t, mid - 0.02 - hh * 0.75 * t ** 1.8
        else:  # light, dolabra
            top, bot = mid + 0.015 + hh * 0.35 * t ** 2, mid - 0.015 - hh * 0.4 * t ** 2
        th = 0.009 * (1 - t) ** 1.2 + 0.0008
        st.append((x, (top - bot) / 2, th, (top + bot) / 2))
    bm = K.blade(st, 12, edge=0.0004)
    K.deform(bm, lambda v: (-v.z, v.y, v.x))
    K.mesh_obj("head", bm, "iron", smooth_angle=50)
    if head == "dolabra":
        spike = K.lathe([(0.0, 0.014, 0.01), (0.12, 0.004), (0.14, 0.0001)], 4, axis="X")
        K.move(spike, 0.02, 0, mid)
        K.mesh_obj("pick", spike, "iron", bevel=0.001)
    elif head == "alan":
        K.mesh_obj("hammer", K.lathe([(0.0, 0.012), (0.045, 0.014), (0.05, 0.0001)], 8, axis="X"), "iron", bevel=0.001)
        K.PARTS[-1].location = (0.02, 0, mid)
    K.mesh_obj("butt", K.lathe([(-0.005, 0.0001), (0.0, 0.018, 0.014), (0.012, 0.017, 0.013)], 16), "iron", bevel=0.001)


# =========================================================================== булавы, кистени, молоты


def haft_wrapped(H, wood="wood", r=0.015):
    K.mesh_obj("haft", K.lathe([(H * i / 20, r + 0.003 * (1 - i / 20)) for i in range(21)], 16), wood)
    path = [(0.0185 * math.cos(i * 0.45), 0.0185 * math.sin(i * 0.45), 0.02 + i * 0.0006) for i in range(200)]
    K.mesh_obj("wrap", K.loft(path, [(0.0035, 0.0012)] * len(path), 6, up=(0, 0, 1)), "leather_dark")


def spikes(z, r, n_rings, per_ring, length, mat="iron"):
    for k in range(n_rings):
        for j in range(per_ring):
            a = math.radians(j * 360 / per_ring + k * 180 / per_ring)
            zz = z + (k - (n_rings - 1) / 2) * r * 0.8
            knob = K.lathe([(0, r * 0.35), (length * 0.7, r * 0.18), (length, 0.0001)], 6)
            K.rot(knob, "Y", 90)
            K.move(knob, r * 0.9, 0, 0)
            K.rot(knob, "Z", math.degrees(a))
            K.move(knob, 0, 0, zz)
            K.mesh_obj("spike", knob, mat, bevel=0.001, smooth_angle=30)


def build_mace(key, m):
    mats()
    H = m["haft"]
    head = m["head"]
    if head in ("flail", "strap"):
        # Короткая рукоять (или ремень), цепь, гиря.
        if head == "flail":
            K.mesh_obj("haft", K.lathe([(0, 0.017), (H, 0.015), (H + 0.01, 0.0001)], 16), "wood_dark")
            K.mesh_obj("cap", K.lathe([(H - 0.02, 0.017), (H + 0.01, 0.017), (H + 0.02, 0.0001)], 16), "iron", bevel=0.001)
            z = H + 0.02
            for i in range(6):
                plane = "XZ" if i % 2 == 0 else "YZ"
                K.mesh_obj("link", K.torus((0, 0, z + 0.012), 0.012, 0.0025, plane=plane, ry=0.018), "iron")
                z += 0.026
            rings = [(z + 0.06 * i / 12, max(0.0001, 0.032 * math.sin(math.pi * i / 12))) for i in range(13)]
            K.mesh_obj("weight", K.lathe(rings, 20), "iron", subsurf=1)
            spikes(z + 0.03, 0.032, 2, 4, 0.018)
        else:
            path = [(0.03 * math.sin(i / 20 * math.pi), 0, 0.3 * i / 20) for i in range(21)]
            K.mesh_obj("strap", K.loft(path, [(0.006, 0.002)] * len(path), 6), "rawhide")
            K.mesh_obj("loop", K.torus((0, 0, -0.03), 0.03, 0.004, plane="XZ"), "rawhide")
            rings = [(0.3 + 0.05 * i / 12, max(0.0001, 0.026 * math.sin(math.pi * i / 12))) for i in range(13)]
            K.mesh_obj("weight", K.lathe(rings, 16), "iron", subsurf=1)
        return
    if head == "club":
        K.mesh_obj("club", K.lathe([(H * i / 20, 0.018 + 0.03 * (i / 20) ** 1.6) for i in range(21)] + [(H + 0.02, 0.0001)], 16), "wood_dark", subsurf=1)
        for k in range(3):
            z = H * (0.6 + 0.15 * k)
            r = 0.018 + 0.03 * (z / H) ** 1.6 + 0.002
            K.mesh_obj("band", K.lathe([(z - 0.01, r), (z + 0.01, r)], 16), "iron", bevel=0.001)
        spikes(H * 0.85, 0.045, 2, 5, 0.012)
        return
    haft_wrapped(H, "wood_dark" if head in ("bull", "pick") else "wood")
    top = H
    if head == "ribbed":
        K.mesh_obj("core", K.lathe([(top - 0.1, 0.02), (top - 0.02, 0.024), (top + 0.01, 0.0001)], 16), "iron", subsurf=1)
        for j in range(6):
            fl = K.cbox(0.03, 0, top - 0.045, 0.034, 0.006, 0.09)
            K.rot(fl, "Z", j * 60)
            K.mesh_obj("flange", fl, "iron", bevel=0.002)
    elif head == "spiked":
        rings = [(top - 0.06 + 0.08 * i / 12, max(0.0001, 0.036 * math.sin(math.pi * i / 12))) for i in range(13)]
        K.mesh_obj("ball", K.lathe(rings, 20), "iron", subsurf=1)
        spikes(top - 0.02, 0.036, 2, 6, 0.03)
    elif head == "ball":
        rings = [(top - 0.06 + 0.08 * i / 12, max(0.0001, 0.04 * math.sin(math.pi * i / 12))) for i in range(13)]
        K.mesh_obj("ball", K.lathe(rings, 24), "iron", subsurf=1)
    elif head == "bull":
        rings = [(top - 0.08, 0.02), (top - 0.05, 0.034, 0.03), (top, 0.038, 0.034), (top + 0.04, 0.026, 0.024), (top + 0.07, 0.016, 0.02), (top + 0.08, 0.0001)]
        bm = K.lathe(rings, 20)
        K.deform(bm, lambda v: (v.x, v.y + (0.02 * max(0.0, (v.z - top) / 0.08)), v.z))
        K.mesh_obj("bull_head", bm, "bronze", subsurf=1)
        for s in (-1, 1):
            path = [(s * (0.03 + 0.05 * t), -0.01 * t, top + 0.01 + 0.05 * math.sin(math.pi * t * 0.8)) for t in [i / 12 for i in range(13)]]
            K.mesh_obj("horn", K.loft(path, [0.008 * (1 - 0.8 * i / 12) for i in range(13)], 10), "bronze", subsurf=1)
    elif head == "hammer":
        K.mesh_obj("hammer", K.cbox(0.02, 0, top, 0.14, 0.045, 0.045), "iron", bevel=0.004)
        K.mesh_obj("face", K.lathe([(0.0, 0.024), (0.02, 0.026)], 16, axis="X"), "iron", bevel=0.002)
        K.PARTS[-1].location = (0.09, 0, top)
    elif head == "pick":
        K.mesh_obj("socket", K.lathe([(top - 0.06, 0.019), (top + 0.01, 0.02), (top + 0.02, 0.0001)], 16), "iron", subsurf=1)
        spike = K.lathe([(0.0, 0.012, 0.01), (0.15, 0.003), (0.17, 0.0001)], 4, axis="X")
        K.deform(spike, lambda v: (v.x, v.y, v.z - 0.12 * (v.x / 0.17) ** 2 * 0.25))
        K.move(spike, 0.015, 0, top - 0.02)
        K.mesh_obj("pick", spike, "iron", bevel=0.001)
        K.mesh_obj("butt", K.lathe([(0.0, 0.014), (0.04, 0.016), (0.045, 0.0001)], 8, axis="X"), "iron", bevel=0.001)
        K.PARTS[-1].location = (-0.055, 0, top - 0.02)


# =========================================================================== копья


def build_spear(key, m):
    mats()
    L = m["L"]
    tip = m["tip"]
    tl, tw = m["tl"], m["tw"]
    r = 0.013 if L < 1.0 else 0.0155 + 0.002 * max(0.0, L - 2.2)
    K.mesh_obj("shaft", K.lathe([(0.06 + (L - 0.06) * i / 40, r - 0.003 * i / 40) for i in range(41)], 14), "wood")
    shank = m.get("shank", 0.0)
    sock_mat = "bronze" if m.get("bronze_socket") else "iron"
    K.mesh_obj("socket", K.lathe([(L - 0.12, r + 0.001), (L - 0.03, r - 0.003), (L, r - 0.005)], 16), sock_mat, subsurf=1)
    base = L
    if shank:
        K.mesh_obj("shank", K.lathe([(L, 0.006), (L + shank, 0.004)], 8), "iron", bevel=0.0)
        base = L + shank
    if tip in ("leaf", "rhomb", "narrow", "broad", "winged", "bayonet"):
        st = []
        for i in range(33):
            t = i / 32
            if tip == "leaf" or tip == "winged" or tip == "broad":
                w = tw * math.sin(math.pi * min(1.0, t * 1.15) ** 0.75) * (1 - t) ** 0.35 + 0.005 * (1 - t)
            elif tip == "rhomb":
                w = tw * (t / 0.35 if t < 0.35 else (1 - t) / 0.65) + 0.004 * (1 - t)
            else:
                w = tw * (1 - t) ** 0.8 + 0.003 * (1 - t)
            st.append((base + tl * t, w, (0.007 if tip in ("rhomb", "bayonet") else 0.0055) * (1 - t) + 0.0008))
        bm = K.blade(st, 8, edge=0.0002)
        if tip in ("rhomb", "bayonet", "narrow"):
            K.deform(bm, lambda v: (v.x, v.y * (1.0 + 2.6 * (1 - min(1, abs(v.x) / 0.008))) * 0.8, v.z))
        K.mesh_obj("blade", bm, "blade_steel", smooth_angle=35)
        if tip == "winged":
            for s in (-1, 1):
                wing = K.cbox(s * 0.03, 0, L + 0.01, 0.035, 0.008, 0.012)
                K.mesh_obj("wing", wing, "iron", bevel=0.002)
        if m.get("crossbar"):
            K.mesh_obj("crossbar", K.lathe([(-0.07, 0.006), (0.07, 0.006)], 8, axis="X"), "wood_dark")
            K.PARTS[-1].location = (0, 0, L - 0.08)
    elif tip == "barbed":
        st = [(base + tl * i / 16, tw * (1 - i / 16) + 0.002, 0.004 * (1 - i / 16) + 0.0006) for i in range(17)]
        K.mesh_obj("blade", K.blade(st, 6), "blade_steel")
        for s in (-1, 1):
            barb = K.lathe([(0.0, 0.003), (0.03, 0.0001)], 4)
            K.rot(barb, "Y", 180 + s * 25)
            K.move(barb, s * 0.012, 0, base + 0.03)
            K.mesh_obj("barb", barb, "iron")
    elif tip == "pilum":
        K.mesh_obj("blade", K.lathe([(base, 0.006), (base + tl * 0.3, 0.009), (base + tl, 0.0001)], 4), "blade_steel", bevel=0.0005)
    elif tip == "plumbata":
        K.mesh_obj("blade", K.lathe([(base, 0.005), (base + tl * 0.4, 0.012), (base + tl, 0.0001)], 4), "blade_steel", bevel=0.0005)
        rings = [(L * 0.62 + 0.07 * i / 12, max(r * 1.05, 0.026 * math.sin(math.pi * i / 12))) for i in range(13)]
        K.mesh_obj("lead", K.lathe(rings, 18), "lead_metal", subsurf=1)
        for j in range(3):
            v = K.cbox(0.012, 0, 0.08, 0.02, 0.0012, 0.1)
            K.rot(v, "Z", j * 120)
            K.mesh_obj("vane", v, "linen_white")
    elif tip == "trident":
        K.mesh_obj("yoke", K.cbox(0, 0, base + 0.01, tw * 2.2, 0.012, 0.018), "iron", bevel=0.002)
        for x in (-tw, 0.0, tw):
            K.mesh_obj("prong", K.lathe([(base, 0.005), (base + tl, 0.003), (base + tl + 0.03, 0.0001)], 6), "iron")
            K.PARTS[-1].location = (x, 0, 0)
            barb = K.lathe([(0.0, 0.003), (0.025, 0.0001)], 4)
            K.rot(barb, "Y", 160)
            K.move(barb, x + 0.006, 0, base + tl + 0.015)
            K.mesh_obj("barb", barb, "iron")
    elif tip == "scythe":
        st = []
        for i in range(33):
            t = i / 32
            w = tw * (1 - t ** 2) + 0.003
            st.append((base + tl * t, w, 0.003 * (1 - t) + 0.0006, -0.06 * t * t))
        K.mesh_obj("blade", K.blade(st, 8), "iron_dark", smooth_angle=40)
        K.mesh_obj("ring", K.torus((0, 0, base - 0.01), r + 0.003, 0.003), "iron")
    if m.get("flag"):
        rows = []
        for i in range(9):
            t = i / 8
            z0 = L - 0.2
            rows.append([(0.004 + 0.32 * t, 0.012 * math.sin(t * 6), z0 - 0.22 * (1 - t) * 0.0 + 0.0), (0.004 + 0.32 * t, 0.012 * math.sin(t * 6), z0 - 0.22 * (1 - t))])
        bm = K.grid_surface([[pt for pt in row] for row in rows])
        K.mesh_obj("bandon", bm, "wool_red", solidify=0.002)
    K.mesh_obj("butt", K.lathe([(-0.05, 0.0001), (0.0, r * 0.6), (0.075, r + 0.0003)], 12), "iron", subsurf=1)


# =========================================================================== наконечники стрел


def arrowhead(tip, x, rotz, s=1.0):
    tang = K.lathe([(-0.03, 0.0001), (-0.028, 0.0018), (0.0, 0.0022)], 4)
    parts = [("tang", tang, "iron_dark")]
    if tip == "bodkin":
        parts.append(("head", K.lathe([(0.0, 0.0035), (0.035, 0.0028), (0.055, 0.0001)], 4), "iron"))
    elif tip == "trilobate":
        parts.append(("head", K.lathe([(0.0, 0.0028), (0.04, 0.0001)], 8), "iron"))
        for j in range(3):
            fin = K.blade([(0.0, 0.003, 0.0008, 0.004), (0.012, 0.007, 0.0007, 0.007), (0.03, 0.004, 0.0005, 0.004), (0.04, 0.0003, 0.0003, 0.0)], 6)
            K.rot(fin, "Z", j * 120)
            parts.append(("fin", fin, "iron"))
    else:
        st = [(0.0, 0.004, 0.0012), (0.01, 0.014, 0.001), (0.03, 0.017, 0.0008), (0.036, 0.0001, 0.0005)]
        parts.append(("head", K.blade(st, 6), "iron"))
    for name, bm, mat in parts:
        K.scale(bm, s)
        K.rot(bm, "Y", 90)
        K.rot(bm, "Z", rotz)
        K.move(bm, x, 0, 0.004)
        K.mesh_obj(name, bm, mat, bevel=0.0004)


def build_heads(key, m):
    mats()
    for i, (x, a) in enumerate([(-0.03, 8), (-0.01, -6), (0.012, 14), (0.03, -10), (0.0, 175)]):
        arrowhead(m["tip"], x, a, s=0.95 + 0.05 * (i % 2))
        K.PARTS[-1].location.y = (i - 2) * 0.014


BUILD = {"sword": build_sword, "seax": build_sword, "axe": build_axe, "mace": build_mace, "spear": build_spear, "head": build_heads}

for _a in D.W:
    _fam = _a["fam"]
    _key = _a["key"]
    _m = dict(_a["model"])
    if _fam == "seax":
        _m.setdefault("guard", "none")
    _pose = "flat"
    _az = 22 + 6 * rnd(_key)

    def _fn(key=_key, fam=_fam, m=_m):
        BUILD[fam](key, m)

    item(_key, tuple(_a["size"]), name=_a["name"], pose=_pose, azimuth=_az, elevation=10 if _fam != "head" else 28, margin=0.94 if _fam != "head" else 0.84)(_fn)
