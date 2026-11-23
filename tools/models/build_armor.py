"""Собирает assets/models/armor.glb: доспехи игроков сверх тех, что уже есть в humans.glb.

Скелет и тело — те же, что у build_humans.py (MakeHuman CC0 + humanrig), поэтому детали
надеваются на любого человека игры. Здесь только вещи, которые игрок может надеть из
инвентаря, а у врагов их нет:

- gambeson   — стёганая туника (поддоспешник): лён, простёганный ромбами, до середины бедра;
- gloves     — кожаные перчатки с крагами;
- vambraces  — ламеллярные наручи: вертикальные железные пластинки на ремешках;
- greaves    — железные поножи поверх обмоток, ремни сзади;
- helm.ridge — позднеримский гребенчатый шлем (тип Интерциза/Беркасово, IV–VI вв.):
               две половины купола на продольном гребне, обод, нащёчники, назатыльник.

Запуск (numpy и Pillow; Blender не нужен):
    python3 tools/models/build_armor.py            # пишет assets/models/armor.glb
    python3 tools/models/build_armor.py --fast     # текстуры поменьше, для проверки

Фото-детали (кожа, железо) берутся как в build_humans.py — из ambientCG (CC0); без
доступа к ambientCG можно указать папку MRPG_AMBIENTCG_DIR с файлами <Asset>_Color.jpg.
"""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import build_humans as bh  # noqa: E402
import geom  # noqa: E402
import props  # noqa: E402
from bake import fbm, smooth  # noqa: E402
from humanrig import B, BONES  # noqa: E402

ROOT = bh.ROOT
TEX, TEX_HI = bh.TEX, bh.TEX_HI
srgb = bh.srgb


# =============================================================================================
# Шейдеры


def gambeson_shader(P, N, hit):
    """Стёганка: несколько слоёв льна, прошитых ромбами; валики между швами пухлые."""
    x, y, z = P[..., 0], P[..., 1], P[..., 2]
    ang = np.arctan2(x, z)
    r = np.hypot(x, z)
    u = ang * np.maximum(r, 0.05) / 0.045
    v = y / 0.045
    # Ромбическая простёжка: две системы диагональных швов.
    d1 = np.abs(((u + v) % 1.0) - 0.5)
    d2 = np.abs(((u - v) % 1.0) - 0.5)
    seam = np.maximum(smooth(0.06, 0.0, 0.5 - d1), smooth(0.06, 0.0, 0.5 - d2))
    puff = np.minimum(d1, d2) * 2.0  # 0 у шва, 1 в середине валика
    weave, fuzz = bh.wool(P, 900.0, 140, N=N, cloth="Fabric030", meters=0.2, texel=2.4 / TEX_HI)
    dirt = fbm(P, 6.0, 3, 141)
    base = srgb(0.78, 0.7, 0.55)
    col = base * (0.7 + 0.25 * puff + 0.12 * weave + 0.06 * fuzz)[..., None]
    col *= (1 - 0.45 * seam)[..., None]
    grime = smooth(0.75, 0.5, y) * 0.3 + 0.15 * dirt
    col *= (1 - 0.35 * grime)[..., None]
    # Кожаная окантовка по подолу и манжетам.
    edge = smooth(0.018, 0.01, np.abs(y - 0.615)) + smooth(0.014, 0.008, np.abs(y - (WRIST_Y + 0.03))) * smooth(0.15, 0.18, np.abs(x))
    edge = np.clip(edge, 0, 1)
    col = col * (1 - edge[..., None]) + srgb(0.3, 0.19, 0.11) * edge[..., None]
    height = 1.4 * puff - 0.6 * seam + 0.3 * weave + 0.6 * edge
    return {"color": col, "rough": 0.85 + 0.08 * fuzz - 0.3 * edge, "height": height}


def gloves_shader(P, N, hit):
    col, grain, wear = bh.leather(P, 150, (0.27, 0.17, 0.1), N=N)
    y = P[..., 1]
    # Шов по краге и ремешок на запястье.
    strap = smooth(0.008, 0.004, np.abs(y - (WRIST_Y + 0.012)))
    stitch = smooth(0.0025, 0.001, np.abs(y - (WRIST_Y + 0.055))) * (np.sin(np.arctan2(P[..., 0], P[..., 2]) * 120) > 0)
    col = col * (1 - 0.5 * strap[..., None]) * (1 - 0.4 * stitch[..., None])
    # Ладонь и костяшки затёрты.
    col *= (1 + 0.15 * wear * smooth(WRIST_Y - 0.05, WRIST_Y - 0.15, y))[..., None]
    return {"color": col, "rough": 0.55 + 0.25 * wear, "height": grain * 0.5 + strap - stitch * 0.3}


def make_vambrace_shader(centers):
    """Пластины идут вдоль предплечья; угол считаем вокруг оси своей руки."""

    def f(P, N, hit):
        x, y, z = P[..., 0], P[..., 1], P[..., 2]
        side = x > 0
        cx = np.where(side, centers[0][0], centers[1][0])
        cz = np.where(side, centers[0][1], centers[1][1])
        ang = np.arctan2(x - cx, z - cz)
        u = ang / (2 * np.pi) * 16  # шестнадцать пластин вокруг руки
        fu = u % 1.0
        v = (y - (WRIST_Y + 0.015)) / 0.2
        plate = smooth(0.0, 0.07, fu) * smooth(1.0, 0.93, fu) * smooth(-0.02, 0.03, v) * smooth(1.02, 0.97, v)
        # Округлые концы пластин и заклёпки шнуровки по краям.
        rivet = smooth(0.12, 0.06, np.hypot((fu - 0.5) * 2.2, (np.minimum(np.abs(v - 0.15), np.abs(v - 0.85))) * 12))
        strap = smooth(0.06, 0.03, np.minimum(np.abs(v - 0.2), np.abs(v - 0.78))) * 1.0
        ic, ir, im, ih = props.iron(P, 160)
        lc, _, _ = bh.leather(P, 161, (0.25, 0.15, 0.09))
        m = (plate * (1 - strap))[..., None]
        col = ic * m + lc * (1 - m)
        col = col * (1 - 0.3 * rivet[..., None]) + srgb(0.72, 0.52, 0.26) * 0.3 * rivet[..., None]
        metal = im * m[..., 0]
        rough = ir * m[..., 0] + 0.7 * (1 - m[..., 0])
        return {"color": col, "rough": rough, "metal": metal, "height": plate * 1.6 + ih * 0.5 + rivet - strap * 0.3}

    return f


def make_greave_shader(centers):
    """Поножи: кованая пластина спереди с рёбром посередине, сзади — кожаные ремни с пряжками."""

    def f(P, N, hit):
        x, y, z = P[..., 0], P[..., 1], P[..., 2]
        side = x > 0
        cx = np.where(side, centers[0][0], centers[1][0])
        cz = np.where(side, centers[0][1], centers[1][1])
        ang = np.arctan2(x - cx, -(z - cz))  # 0 — вперёд (к −Z)
        front = smooth(1.75, 1.45, np.abs(ang))
        ridge = smooth(0.12, 0.0, np.abs(ang)) * front
        # Кромка пластины загнута и потемнела, по ней — ряд заклёпок под подкладку.
        rim = smooth(1.45, 1.6, np.abs(ang)) * front + smooth(0.02, 0.0, np.abs(y - 0.455)) + smooth(0.015, 0.0, np.abs(y - 0.11))
        rivets = smooth(0.35, 0.2, np.abs(((y * 30) % 1.0) - 0.5)) * smooth(1.62, 1.5, np.abs(ang)) * smooth(1.4, 1.5, np.abs(ang))
        straps = smooth(0.016, 0.009, np.minimum(np.abs(y - 0.18), np.abs(y - 0.38))) * (1 - front)
        ic, ir, im, ih = props.iron(P, 170, polish=0.25)
        lc, lg, lw = bh.leather(P, 171, (0.22, 0.14, 0.08))
        m = front[..., None]
        col = ic * m + lc * (1 - m)
        col *= (1 - 0.25 * np.clip(rim, 0, 1))[..., None]
        sc = srgb(0.2, 0.12, 0.07)
        col = col * (1 - straps[..., None]) + sc * straps[..., None]
        col = col * (1 - 0.4 * rivets[..., None]) + srgb(0.7, 0.66, 0.6) * 0.4 * rivets[..., None]
        metal = im * front
        rough = ir * front + (0.6 + 0.2 * lw) * (1 - front)
        return {"color": col, "rough": rough, "metal": metal, "height": ridge * 2.0 + ih * 0.4 + rivets + straps - rim * 0.3}

    return f


# =============================================================================================
# Сборка

WRIST_Y = 0.0  # заполняется в build()


def side_centers(bld, part, lo, hi):
    """Центры (x, z) правой и левой конечности в позе покоя на высотах lo..hi."""
    _, _, rest = bld.part_arrays(part)
    out = []
    for s in (1, -1):
        m = (np.sign(rest[:, 0]) == s) & (rest[:, 1] > lo) & (rest[:, 1] < hi)
        out.append((float(rest[m, 0].mean()), float(rest[m, 2].mean())))
    return out


def build_ridge_helm(bld):
    C, RX, RZ = props.HEAD_C, props.HEAD_RX, props.HEAD_RZ
    tilt, patch = props.tilt, props.patch
    # Купол чуть ниже спангенхельма; две половины чуть расходятся у гребня.
    dome = tilt(geom.lathe([(1.0, -0.004), (1.0, 0.028), (0.96, 0.058), (0.87, 0.09), (0.72, 0.118), (0.52, 0.14), (0.28, 0.154), (0.08, 0.16), (0.0, 0.161)], 56, RX, RZ, C))
    band = tilt(geom.lathe([(1.0, -0.008), (1.04, -0.006), (1.048, 0.018), (1.04, 0.03), (1.0, 0.032)], 56, RX, RZ, C))
    # Гребень: узкая полоса по средней линии от лба до затылка, приподнята над куполом.
    prof = [(1.0, -0.004), (1.0, 0.028), (0.96, 0.058), (0.87, 0.09), (0.72, 0.118), (0.52, 0.14), (0.28, 0.154), (0.08, 0.16), (0.0, 0.161)]
    # Средняя линия купола (x = 0): от обода надо лбом через макушку к затылку.
    front = [(0.0, C[1] + y + 0.005, C[2] - r * RZ * 1.02) for r, y in prof]
    back = [(0.0, C[1] + y + 0.005, C[2] + r * RZ * 1.02) for r, y in reversed(prof[:-1])]
    pts = np.array(front + back)
    secs = [np.array([[-0.006, 0.0], [0.006, 0.0], [0.004, 0.008], [-0.004, 0.008]])] * len(pts)
    ridge_v, ridge_t, ridge_uv = [], [], []
    for i, p in enumerate(pts):
        nxt = pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]
        nxt /= max(np.linalg.norm(nxt), 1e-9)
        up = p - (C + np.array([0, 0.02, 0]))
        up -= nxt * up.dot(nxt)
        up /= max(np.linalg.norm(up), 1e-9)
        side = np.cross(nxt, up)
        for j, (a, b) in enumerate(secs[i]):
            ridge_v.append(p + side * a + up * b)
            ridge_uv.append((j / 3, i / (len(pts) - 1)))
    ridge = (np.array(ridge_v), geom.grid_tris(len(pts), 4, closed=True), np.array(ridge_uv))
    ridge = tilt(ridge)
    cheeks = []
    for s in (1, -1):
        def cols(y, s=s):
            w = 0.6 + 0.3 * (y + 0.12) / 0.12
            mid = s * 1.45
            return (mid - w / 2, mid + w / 2)
        cheeks.append(tilt(patch(np.linspace(-0.12, 0.0, 8), cols, lambda y: 1.06 - 0.2 * max(-y - 0.06, 0), C, RX, RZ)))
    neck = tilt(patch(np.linspace(-0.07, 0.0, 6), lambda y: (np.pi * 0.7, np.pi * 1.3), lambda y: 1.05 + 0.6 * min(-y, 0.07) / 0.07, C, RX, RZ))
    nasal = tilt(geom.box(0.014, 0.06, 0.005, C + [0, -0.024, -RZ * 1.06]))

    def dome_shader(P, N, hit):
        # Железо с серебрёной обкладкой полос у обода (как у богатых шлемов Беркасово).
        q = P - C
        col, rough, metal, h = props.iron(P, 180, polish=0.3)
        rivets = smooth(0.35, 0.2, np.abs(((np.arctan2(q[..., 0], -q[..., 2]) * 14 / np.pi) % 1.0) - 0.5)) * smooth(0.006, 0.003, np.abs(q[..., 1] - 0.03 + 0.18 * q[..., 2]))
        return props.pack(col * (1 + 0.25 * rivets[..., None]), rough - 0.1 * rivets, metal, h + rivets * 1.5)

    def ridge_shader(P, N, hit):
        col, rough, metal, h = props.bronze(P, 181)
        studs = smooth(0.3, 0.15, np.abs(((P[..., 2] * 60) % 1.0) - 0.5))
        return props.pack(col, rough, metal, h + studs)

    def iron_shader(P, N, hit):
        return props.pack(*props.iron(P, 182))

    m_dome = bld.rigid_bake(dome, 1024 if not bh.FAST else 512, dome_shader, "helm_ridge", normal_strength=3.0)
    m_ridge = bld.rigid_bake(geom.merge(ridge, band), 512, ridge_shader, "helm_ridge_band", normal_strength=2.5)
    m_cheek = bld.rigid_bake(geom.merge(*cheeks, neck, nasal), 512, iron_shader, "helm_ridge_cheeks", normal_strength=2.0, double=True)
    bld.emit_rigid("helm.ridge", [(dome, m_dome), (geom.merge(ridge, band), m_ridge), (geom.merge(*cheeks, neck, nasal), m_cheek)], bone="head")


def build():
    global WRIST_Y
    bld = bh.Builder()
    rig = bld.rig
    bld.skin = bld.glb.skeleton(BONES, bh.humanrig.PARENT, [rig.rest_offsets()[n] for n in BONES], rig.inverse_binds())
    WRIST_Y = rig.rest_world()[B["hand_r"]][1, 3]
    wy = WRIST_Y

    def tights(keep, off, iters=2):
        part = bld.extract("helper-tights", keep, uv_box=True)
        return bld.shell(part, off, iters)

    def skirt(hem, off, narrow=0.78):
        return bh.make_skirt(bld, hem, off, narrow)

    def collar(c, r):
        return c[1] > 1.5 or (c[1] > 1.43 and abs(c[0]) < r and c[2] > -0.11 and c[2] < 0.06)

    # --- Стёганка: поверх туники (её оболочка 9–21 мм), рукава до запястья, подол до бедра.
    top = tights(lambda c: c[1] > 0.84 and not collar(c, 0.078) and (c[1] > wy + 0.03 or abs(c[0]) < 0.15),
                 lambda r: 0.024 + 0.012 * smooth(1.1, 0.88, r[:, 1]), 3)
    sk = skirt(0.6, 0.03, 0.86)
    m_top = bld.material("gambeson", bld.bake(top, TEX_HI, gambeson_shader), normal_strength=2.5, double=True, metallic=0.0)
    m_sk = bld.material("gambeson.skirt", bld.bake(sk, TEX_HI, gambeson_shader), normal_strength=2.5, double=True, metallic=0.0)
    bld.emit("gambeson", [(top, m_top), (sk, m_sk)])

    # --- Перчатки: оболочка кожи кисти, краги расширяются над рукавом.
    def is_glove(c):
        return c[1] < wy + 0.075 and abs(c[0]) > 0.14 and c[1] > 0.55
    gl = bld.extract("body", is_glove)
    gl = bld.shell(gl, lambda r: 0.0022 + 0.022 * smooth(wy - 0.005, wy + 0.035, r[:, 1]), 1)
    bld.emit("gloves", [(gl, bld.material("gloves", bld.bake(gl, TEX, gloves_shader), normal_strength=2.0, metallic=0.0))])

    # --- Наручи: от запястья почти до локтя, поверх рукава туники.
    vb = tights(lambda c: wy + 0.005 < c[1] < wy + 0.23 and abs(c[0]) > 0.15, 0.03, 2)
    vb_c = side_centers(bld, vb, wy, wy + 0.25)
    bld.emit("vambraces", [(vb, bld.material("vambraces", bld.bake(vb, TEX, make_vambrace_shader(vb_c)), normal_strength=3.0))])

    # --- Поножи: голень от щиколотки до колена, поверх обмоток (9–13 мм).
    gr = tights(lambda c: 0.1 < c[1] < 0.465 and abs(c[0]) < 0.17, lambda r: 0.02 + 0.004 * smooth(0.15, 0.3, r[:, 1]), 2)
    gr_c = side_centers(bld, gr, 0.1, 0.47)
    bld.emit("greaves", [(gr, bld.material("greaves", bld.bake(gr, TEX, make_greave_shader(gr_c)), normal_strength=3.0))])

    build_ridge_helm(bld)

    out = os.path.join(ROOT, "assets", "models", "armor.glb")
    bld.glb.save(out)
    print("записано", out, os.path.getsize(out) // 1024, "КБ")


if __name__ == "__main__":
    build()
