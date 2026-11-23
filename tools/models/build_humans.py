"""Собирает assets/models/humans.glb: реалистичные люди Херсонеса 510 г. н. э.

Тело — базовый меш MakeHuman (CC0, см. mhbase.py), подогнанный под скелет игры (humanrig.py).
Одежда — оболочки поверх «колготок» MakeHuman и его «юбки» (helper-tights / helper-skirt),
шлемы, щиты и оружие — процедурная геометрия (geom.py). Все текстуры запекаются кодом (bake.py).

Запуск (нужны numpy и Pillow; Blender не нужен):
    python3 tools/models/build_humans.py            # пишет assets/models/humans.glb и src/rig_dims.rs
    python3 tools/models/build_humans.py --fast     # текстуры поменьше, для проверки
"""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import bake  # noqa: E402
import geom  # noqa: E402
import glbwrite  # noqa: E402
import humanrig  # noqa: E402
import mhbase  # noqa: E402
import photo  # noqa: E402
from bake import fbm, smooth, value_noise  # noqa: E402
from humanrig import B, BONES  # noqa: E402

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
FAST = "--fast" in sys.argv
TEX = 512 if FAST else 1024
TEX_HI = 1024 if FAST else 2048


def srgb(*c):
    return np.array(c, float)


# =============================================================================================
# Части меша


def run_shader(shader, P, N, hit):
    """Считает шейдер только в покрытых текселях (их обычно половина) и раскладывает обратно."""
    idx = np.nonzero(hit)
    out = shader(P[idx][:, None, :], N[idx][:, None, :], hit[idx][:, None])
    res = {}
    for k, val in out.items():
        val = np.asarray(val, float)
        if val.ndim < 2:
            val = np.full((len(idx[0]), 1), float(val))
        full = np.zeros(hit.shape + ((val.shape[2],) if val.ndim == 3 else ()))
        full[idx] = val[:, 0]
        res[k] = full
    return res


def as3(val, shape):
    val = np.asarray(val, float)
    if val.ndim < 2:
        val = np.broadcast_to(val, shape).copy()
    return val if val.ndim == 3 else val[..., None]


class Part:
    """Кусок скинованного меша: исходные индексы вершин MakeHuman + свои UV и треугольники."""

    def __init__(self, src, uv, tris, pos=None, W=None):
        self.src = np.asarray(src)
        self.uv = np.asarray(uv, float)
        self.tris = np.asarray(tris)
        self.pos = pos  # (N,3) позиция в позе привязки (если None — берём из базы)
        self.W = W


class Builder:
    def __init__(self):
        self.base = mhbase.Base()
        self.rig = humanrig.HumanRig(self.base)
        self.v = self.base.v.copy()  # поза привязки (A-поза)
        self.rest = self.rig.skin(self.v, self.rig.W, self.rig.rest_world())  # поза покоя игры
        self.glb = glbwrite.Glb()
        self.skin = None
        self.normals = self._normals()

    def _normals(self):
        tris = []
        for f, g in zip(self.base.faces, self.base.fgroups):
            for k in range(1, len(f) - 1):
                tris.append((f[0], f[k], f[k + 1]))
        return glbwrite.vertex_normals(self.v, np.array(tris))

    # -----------------------------------------------------------------------------------------
    def extract(self, group, keep=None, uv_box=None):
        """Грани группы MakeHuman (опционально — только где keep(rest-центр грани) истинно)."""
        b = self.base
        corners = {}
        src, uv, tris = [], [], []
        for f, t, g in zip(b.faces, b.fuvs, b.fgroups):
            if g != group:
                continue
            if keep is not None and not keep(self.rest[f].mean(0)):
                continue
            ids = []
            for vi, ti in zip(f, t):
                key = (vi, ti)
                if key not in corners:
                    corners[key] = len(src)
                    src.append(vi)
                    uv.append(b.uv[ti] if ti >= 0 else (0, 0))
                ids.append(corners[key])
            for k in range(1, len(ids) - 1):
                tris.append((ids[0], ids[k], ids[k + 1]))
        uv = np.array(uv, float)
        if uv_box:
            lo, hi = uv.min(0), uv.max(0)
            uv = (uv - lo) / np.maximum(hi - lo, 1e-9)
            uv = uv * 0.98 + 0.01
        return Part(src, uv, tris)

    def shell(self, part, offset, smooth_iters=0, scale_xz=None):
        """Оболочка: вершины сдвинуты по нормали на offset(rest) метров и сглажены."""
        src = part.src
        uniq, inv = np.unique(src, return_inverse=True)
        p = self.v[uniq].copy()
        n = self.normals[uniq]
        r = self.rest[uniq]
        off = offset(r) if callable(offset) else np.full(len(uniq), offset)
        p += n * off[:, None]
        # Кромки (ворот, подол, манжеты) сглаживаем вдоль самих себя — иначе они «зубчатые».
        from collections import Counter
        cnt = Counter()
        for a, b_, c in inv[part.tris]:
            for u, w in ((a, b_), (b_, c), (c, a)):
                cnt[(min(u, w), max(u, w))] += 1
        bnd = [k for k, c in cnt.items() if c == 1]
        if bnd:
            be = np.array(bnd)
            be = np.r_[be, be[:, ::-1]]
            bdeg = np.bincount(be[:, 0], minlength=len(uniq))
            on = bdeg > 0
            for _ in range(12):
                acc = np.zeros_like(p)
                np.add.at(acc, be[:, 0], p[be[:, 1]])
                avg = acc / np.maximum(bdeg, 1)[:, None]
                p[on] = 0.4 * p[on] + 0.6 * avg[on]
        if smooth_iters:
            edges = set()
            for a, b_, c in inv[part.tris]:
                edges |= {(a, b_), (b_, c), (c, a), (b_, a), (c, b_), (a, c)}
            e = np.array(sorted(edges))
            deg = np.bincount(e[:, 0], minlength=len(uniq))
            # Границы (кромки) не двигаем, иначе подол «уползает».
            for _ in range(smooth_iters):
                acc = np.zeros_like(p)
                np.add.at(acc, e[:, 0], p[e[:, 1]])
                p = 0.5 * p + 0.5 * acc / np.maximum(deg, 1)[:, None]
        if scale_xz is not None:
            s = scale_xz(r)
            p[:, 0] *= s
        part.pos = p[inv]
        return part

    def part_arrays(self, part):
        pos = part.pos if part.pos is not None else self.v[part.src]
        W = part.W if part.W is not None else self.rig.W[part.src]
        rest = self.rest[part.src] if part.pos is None else self.rig.skin(pos, W, self.rig.rest_world())
        return pos, W, rest

    # -----------------------------------------------------------------------------------------
    def bake(self, part, size, shader):
        import time
        t0 = time.time()
        pos, W, rest = self.part_arrays(part)
        n = glbwrite.vertex_normals(rest, part.tris)
        attrs = np.c_[rest, n]
        img, hit = bake.rasterize(part.uv, part.tris, attrs, size)
        P = img[..., :3]
        N = img[..., 3:6]
        N /= np.maximum(np.linalg.norm(N, axis=-1, keepdims=True), 1e-9)
        t1 = time.time()
        out = run_shader(shader, P, N, hit)
        t2 = time.time()
        res = {k: bake.dilate(as3(val, hit.shape), hit, 6) for k, val in out.items()}
        print(f"  запекание {size}: растр {t1 - t0:.1f} c, шейдер {t2 - t1:.1f} c, расширение {time.time() - t2:.1f} c")
        return res

    def material(self, name, tex, normal_strength=1.0, alpha_cutoff=None, double=False, metallic=1.0):
        """tex: dict(color (H,W,3), [alpha], rough, [metal], [height])."""
        col = tex["color"]
        if "alpha" in tex:
            base = glbwrite.to_image(np.concatenate([col, tex["alpha"]], axis=-1), "RGBA")
        else:
            base = glbwrite.to_image(col)
        rough = tex.get("rough")
        metal = tex.get("metal", np.zeros_like(rough))
        mr = glbwrite.to_image(np.concatenate([np.ones_like(rough), rough, metal], axis=-1))
        normal = None
        if "height" in tex:
            normal = glbwrite.to_image(bake.height_to_normal(tex["height"][..., 0], normal_strength))
        return self.glb.material(name, base=base, mr=mr, normal=normal, metallic=metallic, roughness=1.0,
                                 alpha_cutoff=alpha_cutoff, double=double)

    def emit(self, name, items):
        """items: список (Part, material). Пишет скинованный меш с именем name."""
        prims = []
        for part, mat in items:
            pos, W, _ = self.part_arrays(part)
            n = glbwrite.vertex_normals(pos, part.tris)
            # Нормали усредняем по исходной вершине, чтобы на швах UV не было ступенек.
            acc = np.zeros((len(self.v), 3))
            np.add.at(acc, part.src, n)
            n2 = acc[part.src]
            n = n2 / np.maximum(np.linalg.norm(n2, axis=1, keepdims=True), 1e-9)
            order = np.argsort(-W, axis=1)[:, :4]
            w4 = np.take_along_axis(W, order, axis=1)
            w4 /= np.maximum(w4.sum(1, keepdims=True), 1e-9)
            prims.append({"v": pos, "n": n, "uv": part.uv, "tris": part.tris, "material": mat, "joints": order, "weights": w4})
        self.glb.mesh(name, prims, skin=self.skin)

    def emit_rigid(self, name, items, bone=None):
        """Жёсткий меш. bone — привязать целиком к кости (шлем к голове), иначе — без скина (оружие)."""
        prims = []
        for (v, tris, uv), mat in items:
            n = glbwrite.vertex_normals(v, tris)
            p = {"v": v, "n": n, "uv": uv, "tris": tris, "material": mat}
            if bone is not None:
                # Вершины заданы в позе покоя игры -> в позу привязки.
                v_bind = self.rest_to_bind(v, bone)
                p["v"] = v_bind
                p["n"] = glbwrite.vertex_normals(v_bind, tris)
                p["joints"] = np.tile([B[bone], 0, 0, 0], (len(v), 1))
                p["weights"] = np.tile([1.0, 0, 0, 0], (len(v), 1))
            prims.append(p)
        self.glb.mesh(name, prims, skin=self.skin if bone is not None else None)

    def rest_to_bind(self, v, bone):
        i = B[bone]
        M = np.linalg.inv(self.rig.rest_world()[i] @ self.rig.inverse_binds()[i])
        return (np.c_[v, np.ones(len(v))] @ M.T)[:, :3]

    def rigid_bake(self, mesh, size, shader, name, **kw):
        """Запекание для жёсткой детали: позиции берутся из её собственных координат."""
        v, tris, uv = mesh
        n = glbwrite.vertex_normals(v, tris)
        img, hit = bake.rasterize(uv, tris, np.c_[v, n], size)
        P, N = img[..., :3], img[..., 3:6]
        N /= np.maximum(np.linalg.norm(N, axis=-1, keepdims=True), 1e-9)
        out = run_shader(shader, P, N, hit)
        tex = {k: bake.dilate(as3(val, hit.shape), hit, 6) for k, val in out.items()}
        return self.material(name, tex, **kw)


# =============================================================================================
# Шейдеры (всё в позе покоя игры: Y вверх, лицо -Z, правая рука +X)

EYE_R = np.array([0.03, 1.641, -0.13])
MOUTH = np.array([0.0, 1.568, -0.158])


def face_dist(P, c, sx, sy):
    return np.sqrt(((P[..., 0] - c[0]) / sx) ** 2 + ((P[..., 1] - c[1]) / sy) ** 2)


def beard_mask(P, N, style):
    """1 там, где растёт борода. style: 'stubble', 'short', 'full'."""
    x, y, z = P[..., 0], P[..., 1], P[..., 2]
    ax = np.abs(x)
    front = smooth(-0.02, -0.08, z)
    # Линия скул: от уха вниз к углу рта.
    cheek_top = 1.612 - 0.55 * np.maximum(0.047 - ax, 0)
    m = smooth(cheek_top + 0.004, cheek_top - 0.008, y) * front
    # Снизу — по шее, под подбородком.
    lower = 1.50 if style == "full" else 1.515
    m *= smooth(lower - 0.01, lower + 0.012, y)
    # Сзади до ушей.
    m *= smooth(0.075, 0.062, ax) + smooth(-0.07, -0.12, z)
    m = np.clip(m, 0, 1)
    # Губы и полоса под нижней губой — без волос (у короткой бороды).
    lips = face_dist(P, MOUTH, 0.03, 0.011)
    m *= smooth(0.85, 1.15, lips)
    if style != "full":
        m *= 1 - smooth(1.4, 0.9, face_dist(P, MOUTH + [0, -0.018, 0], 0.012, 0.006)) * 0.7
    return m


def skin_shader(P, N, hit):
    x, y, z = P[..., 0], P[..., 1], P[..., 2]
    ax = np.abs(x)
    base = srgb(0.74, 0.55, 0.43)
    n1 = fbm(P, 9.0, 4, 1)
    n2 = fbm(P, 60.0, 3, 2)
    col = base[None, None, :] * (0.9 + 0.2 * n1[..., None])
    # Покраснения: щёки, нос, уши, костяшки, локти, колени.
    red = np.zeros_like(x)
    for s in (-1, 1):
        red += np.exp(-(((x - s * 0.045) / 0.022) ** 2 + ((y - 1.603) / 0.02) ** 2)) * smooth(-0.08, -0.12, z)
        red += 0.6 * np.exp(-(((x - s * 0.075) / 0.012) ** 2 + ((y - 1.635) / 0.03) ** 2))
    red += 0.7 * np.exp(-((x / 0.012) ** 2 + ((y - 1.598) / 0.012) ** 2 + ((z + 0.175) / 0.02) ** 2))
    hands = smooth(0.95, 0.85, y) * smooth(0.15, 0.19, ax)
    red += 0.4 * hands * (0.5 + n2)
    col = col * (1 - 0.18 * red[..., None]) + srgb(0.72, 0.36, 0.32) * 0.18 * red[..., None]
    # Загар: кисти, лицо, шея темнее, тело светлее (под одеждой).
    tan = smooth(1.42, 1.48, y) + hands
    col *= (0.93 + 0.07 * (1 - np.clip(tan, 0, 1)))[..., None]
    # Губы.
    lips = face_dist(P, MOUTH, 0.026, 0.0085) * smooth(-0.135, -0.15, z)[..., None][..., 0] ** 0
    lipm = smooth(1.05, 0.75, lips) * smooth(-0.13, -0.15, z)
    col = col * (1 - 0.55 * lipm[..., None]) + srgb(0.6, 0.33, 0.3) * 0.55 * lipm[..., None]
    # Брови.
    brow = np.zeros_like(x)
    for s in (-1, 1):
        xb = (x - s * 0.033) * s
        yc = 1.664 + 0.004 * np.cos(xb / 0.025 * 1.3) - 0.003 * (xb > 0.012) * (xb - 0.012) / 0.01
        d = np.abs(y - yc) / (0.0045 - 0.0018 * smooth(0.0, 0.025, xb))
        brow = np.maximum(brow, smooth(1.2, 0.6, d) * smooth(0.032, 0.022, np.abs(xb)) * smooth(-0.13, -0.15, z))
    brow *= 0.75 + 0.25 * value_noise(P, 900.0, 5)
    col = col * (1 - 0.8 * brow[..., None]) + srgb(0.16, 0.11, 0.08) * 0.8 * brow[..., None]
    # Щетина там, где бреют бороду.
    st = beard_mask(P, N, "short") * (0.6 + 0.4 * value_noise(P, 1500.0, 9))
    col = col * (1 - 0.28 * st[..., None]) + srgb(0.32, 0.27, 0.24) * 0.28 * st[..., None]
    # Веки и тени вокруг глаз.
    for s in (-1, 1):
        e = face_dist(P, EYE_R * [s, 1, 1], 0.02, 0.012)
        col *= (1 - 0.12 * smooth(1.2, 0.6, e) * smooth(-0.12, -0.14, z))[..., None]
    # Ногти.
    rough = 0.52 + 0.1 * n2 - 0.08 * np.exp(-((x / 0.03) ** 2 + ((y - 1.6) / 0.05) ** 2))
    height = 0.6 * fbm(P, 400.0, 2, 3) + 0.25 * n2
    return {"color": col, "rough": rough, "height": height}


def wool(P, freq=700.0, seed=0, N=None, cloth="Fabric045", meters=0.25, texel=0.002):
    """Шерстяное полотно: плетение + ворс.

    С нормалями N плетение берётся с фото настоящей ткани (ambientCG, CC0): грубая
    домотканая шерсть Fabric045 или полотняное переплетение Fabric030 (лён).
    weave ~0.5 в среднем, как у прежнего синусного узора.
    """
    fuzz = fbm(P, 220.0, 3, seed)
    if N is not None:
        d = photo.detail(P, N, cloth, meters, texel)
        weave = np.clip(0.5 + (d - 1.0) * 1.4, 0.0, 1.0)
        return weave, fuzz
    weave = 0.5 + 0.25 * np.sin(P[..., 1] * freq * 2.2) * np.sin((P[..., 0] + P[..., 2]) * freq * 2.2)
    return weave, fuzz


def tunic_shader(style):
    """style: 'byz' (некрашеная шерсть с пурпурными клавами), 'goth' (простая с тканой каймой),
    'kaftan' (кочевой кафтан, войлок и мех по краям)."""

    def f(P, N, hit):
        x, y, z = P[..., 0], P[..., 1], P[..., 2]
        ax = np.abs(x)
        weave, fuzz = wool(P, seed=4, N=N, cloth="Fabric030" if style == "byz" else "Fabric045", texel=2.4 / TEX_HI)
        dirt = fbm(P, 6.0, 3, 7)
        hem_dist = y - 0.5  # подол туники ~ на уровне колена
        if style == "kaftan":
            base = srgb(0.86, 0.8, 0.7)
        else:
            base = srgb(0.93, 0.9, 0.84)
        col = base * (0.78 + 0.1 * fuzz + 0.24 * weave)[..., None]
        # Грязь снизу и на локтях.
        grime = smooth(0.75, 0.48, y) * 0.25 + 0.12 * dirt
        col *= (1 - 0.35 * grime)[..., None]
        band = np.zeros_like(x)
        if style == "byz":
            purple = srgb(0.36, 0.12, 0.16)
            # Клавы: две вертикальные полосы с плеч до подола спереди и сзади.
            clavi = smooth(0.012, 0.006, np.abs(ax - 0.075)) * smooth(1.47, 1.44, y) * smooth(0.3, 0.6, -0.0 + np.abs(N[..., 2]))
            # Орбикулы: круглые нашивки на плечах и у подола.
            orb = np.zeros_like(x)
            for cy, cx in ((0.62, 0.11),):
                orb = np.maximum(orb, smooth(0.035, 0.028, np.hypot(ax - cx, y - cy)))
            # Полосы на рукавах у запястья.
            cuff = smooth(0.012, 0.006, np.abs(y - 0.93)) * smooth(0.17, 0.2, ax)
            cuff = np.maximum(cuff, smooth(0.008, 0.004, np.abs(y - 0.905)) * smooth(0.17, 0.2, ax))
            band = np.clip(clavi + orb + cuff, 0, 1)
            pattern = 0.85 + 0.15 * np.sin(y * 900) * np.sin(x * 900)
            col = col * (1 - band[..., None]) + (purple * pattern[..., None] * (0.9 + 0.2 * fuzz[..., None])) * band[..., None]
        elif style == "goth":
            # Тканая кайма (таблетками) по подолу, вороту и рукавам: красно-жёлтый ромб.
            hem = smooth(0.03, 0.02, np.abs(y - 0.53))
            neck = smooth(0.02, 0.012, np.abs(y - 1.445)) * smooth(0.12, 0.09, ax)
            cuff = smooth(0.02, 0.012, np.abs(y - 0.925)) * smooth(0.16, 0.19, ax)
            band = np.clip(hem + neck + cuff, 0, 1)
            ang = np.arctan2(x, z)
            diam = (np.abs(((ang * 30) % 2) - 1) + np.abs(((y * 120) % 2) - 1)) < 0.9
            bcol = np.where(diam[..., None], srgb(0.75, 0.6, 0.2), srgb(0.5, 0.1, 0.07))
            col = col * (1 - band[..., None]) + bcol * band[..., None]
        else:
            # Кафтан: запах налево, оторочка мехом/войлоком по краям.
            edge = smooth(0.03, 0.015, np.abs(y - 0.5)) + smooth(0.025, 0.012, np.abs(y - 0.925)) * smooth(0.16, 0.19, ax)
            flap = smooth(0.02, 0.008, np.abs(x + 0.02 + 0.06 * smooth(1.2, 1.45, y) - 0.0)) * smooth(0.0, -0.05, z) * smooth(0.5, 0.55, y)
            band = np.clip(edge + flap, 0, 1)
            fur = srgb(0.3, 0.22, 0.15) * (0.7 + 0.5 * fbm(P, 400.0, 3, 11))[..., None]
            col = col * (1 - band[..., None]) + fur * band[..., None]
        rough = 0.85 + 0.1 * fuzz
        height = 0.4 * weave + 0.6 * fuzz + 1.5 * band
        return {"color": col, "rough": rough, "height": height * 0.6}

    return f


def trousers_shader(P, N, hit):
    weave, fuzz = wool(P, 900.0, 12, N=N)
    dirt = fbm(P, 5.0, 3, 13)
    col = srgb(0.62, 0.6, 0.57) * (0.8 + 0.12 * fuzz + 0.2 * weave)[..., None]
    col *= (1 - 0.4 * smooth(0.4, 0.05, P[..., 1]) - 0.15 * dirt)[..., None]
    return {"color": col, "rough": 0.9 + 0.05 * fuzz, "height": 0.5 * weave + 0.5 * fuzz}


def wraps_shader(P, N, hit):
    """Обмотки (фасции): полоса ткани спиралью от щиколотки до колена."""
    ang = np.arctan2(P[..., 0] - np.sign(P[..., 0]) * 0.1, P[..., 2])
    t = (P[..., 1] * 22.0 + ang / (2 * np.pi) * (np.sign(P[..., 0]) + 0.001)) % 1.0
    ridge = smooth(0.0, 0.12, t) * smooth(1.0, 0.85, t)
    weave, fuzz = wool(P, 800.0, 14, N=N, cloth="Fabric030", meters=0.2)
    col = srgb(0.7, 0.64, 0.52) * (0.62 + 0.3 * ridge + 0.08 * fuzz + 0.16 * weave)[..., None]
    col *= (1 - 0.35 * smooth(0.35, 0.08, P[..., 1]))[..., None]
    # Перекрёстные ремешки (подвязки).
    straps = smooth(0.006, 0.002, np.abs(((P[..., 1] * 9 + ang / np.pi * 0.5) % 1.0) - 0.5) / 9)
    col = col * (1 - 0.8 * straps[..., None]) + srgb(0.25, 0.17, 0.1) * 0.8 * straps[..., None]
    return {"color": col, "rough": 0.9, "height": ridge + 0.5 * straps + 0.2 * fuzz}


def leather(P, seed=20, color=(0.32, 0.21, 0.13), N=None):
    grain = fbm(P, 500.0, 3, seed)
    wear = fbm(P, 30.0, 3, seed + 1)
    if N is not None:
        # Зерно, складки и потёртости настоящей кожи (Leather014, CC0).
        d = photo.detail(P, N, "Leather014", 0.5, 2.0 / TEX)
        grain = np.clip(0.5 + (d - 1.0) * 1.2, 0.0, 1.0)
        wear = np.clip(wear * 0.6 + (d - 1.0) * 0.8 + 0.2, 0.0, 1.0)
    col = srgb(*color) * (0.75 + 0.35 * grain + 0.25 * wear)[..., None]
    return col, grain, wear


def boots_shader(P, N, hit):
    col, grain, wear = leather(P, 21, N=N)
    # Подошва темнее, шнуровка по подъёму.
    sole = smooth(0.012, 0.004, P[..., 1])
    col *= (1 - 0.5 * sole)[..., None]
    lace = smooth(0.004, 0.002, np.abs(((P[..., 1] * 40) % 1.0) - 0.5) / 40) * smooth(-0.06, -0.09, P[..., 2] - 0.0) * smooth(0.07, 0.05, np.abs(np.abs(P[..., 0]) - 0.115))
    col = col * (1 - lace[..., None]) + srgb(0.2, 0.13, 0.08) * lace[..., None]
    return {"color": col, "rough": 0.55 + 0.25 * wear, "height": grain * 0.5 + lace}


def belt_shader(P, N, hit):
    col, grain, wear = leather(P, 30, (0.25, 0.15, 0.09), N=N)
    x, y, z = P[..., 0], P[..., 1], P[..., 2]
    # Бронзовые накладки по поясу и пряжка спереди.
    ang = np.arctan2(x, -z)
    plates = (np.abs(((ang * 7 / np.pi) % 1.0) - 0.5) < 0.18) & (np.abs(y - 0.985) < 0.012)
    buckle = (np.abs(x) < 0.03) & (np.abs(y - 0.985) < 0.022) & (z < 0)
    metal = (plates | buckle).astype(float)
    bronze = srgb(0.72, 0.52, 0.26) * (0.8 + 0.3 * fbm(P, 200.0, 2, 31))[..., None]
    col = col * (1 - metal[..., None]) + bronze * metal[..., None]
    return {"color": col, "rough": 0.6 - 0.25 * metal, "metal": metal, "height": grain * 0.4 + metal}


def mail_shader(P, N, hit):
    """Кольчуга: ряды колец (пространственный узор) + потёртости."""
    x, y, z = P[..., 0], P[..., 1], P[..., 2]
    ang = np.arctan2(x, z)
    r = np.hypot(x, z)
    u = ang * np.maximum(r, 0.05) / 0.009
    v = y / 0.0075
    row = np.floor(v)
    uu = u + 0.5 * (row % 2)
    fu = (uu % 1.0) - 0.5
    fv = (v % 1.0) - 0.5
    d = np.hypot(fu, fv * 1.3)
    ring = smooth(0.22, 0.32, d) * smooth(0.55, 0.42, d)
    rust = fbm(P, 25.0, 3, 41)
    col = srgb(0.5, 0.5, 0.52) * (0.35 + 0.75 * ring)[..., None]
    col = col * (1 - 0.35 * smooth(0.55, 0.8, rust)[..., None]) + srgb(0.35, 0.2, 0.1) * 0.35 * smooth(0.55, 0.8, rust)[..., None]
    metal = 0.9 * ring + 0.1
    return {"color": col, "rough": 0.45 + 0.3 * (1 - ring) + 0.2 * rust, "metal": metal, "height": ring * 2.0}


def lamellar_shader(P, N, hit):
    """Ламеллярный доспех: ряды узких железных пластин, связанных кожаным шнуром."""
    x, y, z = P[..., 0], P[..., 1], P[..., 2]
    ang = np.arctan2(x, z)
    r = np.hypot(x, z)
    u = ang * np.maximum(r, 0.05) / 0.022
    v = y / 0.065
    row = np.floor(v)
    uu = u + 0.5 * (row % 2)
    fu = (uu % 1.0)
    fv = (v % 1.0)
    edge = smooth(0.0, 0.08, fu) * smooth(1.0, 0.92, fu) * smooth(0.0, 0.05, fv)
    rounded = smooth(1.0, 0.88, fv) + (1 - smooth(1.0, 0.88, fv)) * smooth(0.5, 0.35, np.abs(fu - 0.5))
    plate = edge * rounded
    # Шнуровка: горизонтальная полоса внизу каждого ряда.
    lace = smooth(0.06, 0.03, np.abs(fv - 0.22)) * (np.abs(fu - 0.5) < 0.3)
    rust = fbm(P, 20.0, 3, 51)
    iron = srgb(0.42, 0.42, 0.44) * (0.75 + 0.3 * fbm(P, 150.0, 2, 52))[..., None]
    iron = iron * (1 - 0.4 * smooth(0.55, 0.85, rust)[..., None]) + srgb(0.38, 0.22, 0.1) * 0.4 * smooth(0.55, 0.85, rust)[..., None]
    lace_c = srgb(0.35, 0.18, 0.1)
    col = iron * plate[..., None] * (1 - lace[..., None]) + lace_c * lace[..., None] + srgb(0.12, 0.09, 0.07) * ((1 - plate) * (1 - lace))[..., None]
    metal = plate * (1 - lace) * 0.9
    rough = 0.35 + 0.3 * rust + 0.4 * lace
    return {"color": col, "rough": rough, "metal": metal, "height": plate * 2.0 - lace * 0.5 + 0.3 * (1 - fv) * plate}


def hair_shader(style, color=(0.2, 0.13, 0.08)):
    def f(P, N, hit):
        x, y, z = P[..., 0], P[..., 1], P[..., 2]
        ax = np.abs(x)
        # Пряди: шум, вытянутый вдоль направления роста (от макушки вниз и назад).
        q = np.stack([x * 1.0, y * 0.15, z * 1.0], -1)
        strands = fbm(q, 900.0, 3, 61)
        col = srgb(*color) * (0.6 + 0.7 * strands)[..., None]
        if style in ("beard", "beard_short"):
            m = beard_mask(P, N, "full" if style == "beard" else "short")
            edge = m * (1.0 if style == "beard" else 0.8)
        else:
            # Линия роста волос: лоб, виски, затылок.
            fore = 1.705 + 0.012 * np.cos(x / 0.05)
            temple = smooth(0.055, 0.07, ax)
            hl = fore * (1 - temple) + 1.655 * temple
            front = smooth(-0.04, -0.09, z)
            line = hl * front + (1.58 if style == "short" else 1.53) * (1 - front)
            edge = smooth(line - 0.004, line + 0.012, y)
            # Над ушами — выемка.
            ear = np.exp(-(((ax - 0.075) / 0.012) ** 2 + ((y - 1.635) / 0.025) ** 2 + ((z + 0.035) / 0.03) ** 2))
            edge *= 1 - smooth(0.3, 0.6, ear)
        alpha = smooth(0.35, 0.75, edge * (0.75 + 0.5 * fbm(P, 700.0, 2, 62)))
        return {"color": col, "alpha": alpha, "rough": 0.55 + 0.2 * strands, "height": strands}

    return f


# =============================================================================================
# Сборка


def build():
    bld = Builder()
    rig = bld.rig
    bld.skin = bld.glb.skeleton(BONES, humanrig.PARENT, [rig.rest_offsets()[n] for n in BONES], rig.inverse_binds())
    rest = bld.rest

    # --- Тело: голова (с шеей), кисти и остальное (оно всегда под одеждой).
    wrist_y = rig.rest_world()[B["hand_r"]][1, 3]

    def is_head(c):
        # Голова и шея (под воротом туники кожа заходит на пару сантиметров).
        return c[1] > 1.5 or (c[1] > 1.405 and abs(c[0]) < 0.075)

    def is_hand(c):
        return c[1] < wrist_y + 0.035 and abs(c[0]) > 0.14 and c[1] > 0.55

    skin_part_all = bld.extract("body")
    skin_tex = bld.bake(skin_part_all, TEX_HI, skin_shader)
    skin_mat = bld.material("skin", skin_tex, normal_strength=2.0, metallic=0.0)
    for name, keep in (("body.head", is_head), ("body.hands", is_hand), ("body.trunk", lambda c: not is_head(c) and not is_hand(c))):
        bld.emit(name, [(bld.extract("body", keep), skin_mat)])

    # --- Глаза.
    eye_mat = None
    eyes = []
    for s in (1, -1):
        c = EYE_R * [s, 1, 1]
        sph = geom.sphere(0.0122, 24, 16)
        v, t, uv = sph
        # Повернуть так, чтобы полюс смотрел вперёд (-Z).
        R = humanrig.rot_axis([1, 0, 0], -np.pi / 2)
        v = v @ R.T + c
        eyes.append((v, t, uv))
    eye_mesh = geom.merge(*eyes)

    def eye_shader(P, N, hit):
        cosang = -N[..., 2]
        iris = smooth(0.86, 0.9, cosang)
        pupil = smooth(0.965, 0.975, cosang)
        sclera = srgb(0.86, 0.83, 0.78)
        irc = srgb(0.3, 0.2, 0.1) * (0.6 + 0.6 * fbm(P, 3000.0, 2, 71))[..., None]
        col = sclera * (1 - iris[..., None]) + irc * iris[..., None]
        col *= (1 - pupil * 0.95)[..., None]
        return {"color": col, "rough": np.full_like(cosang, 0.08), "height": iris * 0.0}

    eye_mat = bld.rigid_bake(eye_mesh, 256, eye_shader, "eyes", metallic=0.0)
    bld.emit_rigid("eyes", [(eye_mesh, eye_mat)], bone="head")

    # --- Волосы и борода (оболочки над кожей головы).
    def head_shell(keep, off, iters):
        part = bld.extract("body", keep)
        return bld.shell(part, off, iters)

    for style, color in (("short", (0.8, 0.78, 0.75)), ("long", (0.85, 0.83, 0.8))):
        keep = lambda c: c[1] > (1.56 if style == "short" else 1.50) and (c[2] > -0.11 or c[1] > 1.69) and not (c[1] < 1.62 and c[2] < -0.04)
        part = head_shell(keep, (lambda r: 0.004 + 0.006 * smooth(1.6, 1.76, r[:, 1])) if style == "short" else (lambda r: 0.008 + 0.012 * smooth(1.66, 1.76, r[:, 1]) + 0.01 * smooth(1.6, 1.5, r[:, 1])), 2)
        mat = bld.material(f"hair_{style}", bld.bake(part, TEX, hair_shader(style, color)), normal_strength=3.0, alpha_cutoff=0.5, double=True, metallic=0.0)
        bld.emit(f"hair.{style}", [(part, mat)])

    for style, thick in (("short", 0.0025), ("full", 0.01)):
        keep = lambda c: 1.49 < c[1] < 1.63 and c[2] < -0.02 and abs(c[0]) < 0.085
        part = head_shell(keep, lambda r: thick * (0.4 + 0.6 * smooth(1.6, 1.53, r[:, 1])) + 0.0015, 2)
        mat = bld.material(f"beard_{style}", bld.bake(part, TEX // 2, hair_shader("beard" if style == "full" else "beard_short", (0.8, 0.78, 0.75))), normal_strength=3.0, alpha_cutoff=0.5, double=True, metallic=0.0)
        bld.emit(f"beard.{style}", [(part, mat)])

    # --- Одежда поверх колготок MakeHuman.
    def tights(keep, off, iters=2):
        part = bld.extract("helper-tights", keep, uv_box=True)
        return bld.shell(part, off, iters)

    def skirt(hem, off, narrow=0.78):
        part = bld.extract("helper-skirt", lambda c: c[1] > hem, uv_box=True)
        part = bld.shell(part, off, 1)
        # Юбка MakeHuman рассчитана на расставленные ноги: сужаем к подолу и перевешиваем на бёдра.
        pos = part.pos.copy()
        y = pos[:, 1]
        k = 1 - (1 - narrow) * smooth(1.0, hem, y)
        pos[:, 0] *= k
        pos[:, 2] = (pos[:, 2] + 0.015) * (0.92 + 0.08 * k) - 0.015
        part.pos = pos
        W = np.zeros((len(pos), len(BONES)))
        a = 0.75 * smooth(0.98, hem + 0.05, y)
        side = 1 / (1 + np.exp(-pos[:, 0] / 0.03))
        W[:, B["pelvis"]] = 1 - a
        W[:, B["thigh_r"]] = a * side
        W[:, B["thigh_l"]] = a * (1 - side)
        part.W = W
        return part

    neck = 1.455

    def collar(c, r):
        """Вырез ворота: всё выше 1.5 м и шея внутри радиуса r."""
        return c[1] > 1.5 or (c[1] > 1.43 and abs(c[0]) < r and c[2] > -0.11 and c[2] < 0.06)
    upper = lambda c: 0.86 < c[1] < neck
    # Туники (три кроя различаются тканью и отделкой).
    for style in ("byz", "goth", "kaftan"):
        sleeves_to = wrist_y + 0.01
        top = tights(lambda c: c[1] > 0.84 and not collar(c, 0.068) and (c[1] > sleeves_to or abs(c[0]) < 0.15), lambda r: 0.009 + 0.012 * smooth(1.1, 0.88, r[:, 1]), 3)
        sk = skirt(0.5 if style != "goth" else 0.53, 0.012)
        sh = tunic_shader(style)
        m_top = bld.material(f"tunic_{style}", bld.bake(top, TEX_HI, sh), normal_strength=1.5, double=True, metallic=0.0)
        m_sk = bld.material(f"tunic_{style}.skirt", bld.bake(sk, TEX_HI, sh), normal_strength=1.5, double=True, metallic=0.0)
        bld.emit(f"tunic.{style}", [(top, m_top), (sk, m_sk)])

    trousers = tights(lambda c: 0.06 < c[1] < 1.0 and abs(c[0]) < 0.17, 0.004, 1)
    bld.emit("trousers", [(trousers, bld.material("trousers", bld.bake(trousers, TEX, trousers_shader), metallic=0.0))])

    wraps = tights(lambda c: 0.07 < c[1] < 0.47 and abs(c[0]) < 0.17, lambda r: 0.009 + 0.004 * smooth(0.12, 0.3, r[:, 1]), 2)
    bld.emit("wraps", [(wraps, bld.material("wraps", bld.bake(wraps, TEX, wraps_shader), normal_strength=2.0, metallic=0.0))])

    def flatten_sole(part, off):
        pos = part.pos.copy()
        low = pos[:, 1] < 0.03
        pos[low, 1] = np.maximum(pos[low, 1] - 0.006, -0.004)
        part.pos = pos
        return part

    boots = flatten_sole(tights(lambda c: c[1] < 0.3 and abs(c[0]) < 0.2, lambda r: 0.007 + 0.006 * smooth(0.2, 0.28, r[:, 1]), 4), 0)
    bld.emit("boots", [(boots, bld.material("boots", bld.bake(boots, TEX, boots_shader), normal_strength=2.0, metallic=0.0))])
    shoes = flatten_sole(tights(lambda c: c[1] < 0.115 and abs(c[0]) < 0.2, 0.006, 4), 0)
    bld.emit("shoes", [(shoes, bld.material("shoes", bld.bake(shoes, TEX // 2, boots_shader), normal_strength=2.0, metallic=0.0))])

    belt = tights(lambda c: 0.965 < c[1] < 1.005 and abs(c[0]) < 0.17, lambda r: 0.026 + 0.0 * r[:, 1], 2)
    bld.emit("belt", [(belt, bld.material("belt", bld.bake(belt, TEX // 2, belt_shader), normal_strength=2.0))])

    # Кольчуга: до середины бедра, короткие рукава.
    mail_top = tights(lambda c: c[1] > 0.9 and not collar(c, 0.074) and (abs(c[0]) < 0.15 or c[1] > 1.17), lambda r: 0.02 + 0.006 * smooth(1.1, 0.9, r[:, 1]), 3)
    mail_sk = skirt(0.66, 0.024, 0.85)
    bld.emit("mail", [(mail_top, bld.material("mail", bld.bake(mail_top, TEX_HI, mail_shader), normal_strength=4.0)),
                      (mail_sk, bld.material("mail.skirt", bld.bake(mail_sk, TEX_HI, mail_shader), normal_strength=4.0, double=True))])

    # Ламелляр: корпус и юбка из пластин, плечи — кожаные.
    lam_top = tights(lambda c: c[1] > 0.93 and not collar(c, 0.085) and (abs(c[0]) < 0.17 or c[1] > 1.3), lambda r: 0.03 + 0.008 * smooth(1.1, 0.95, r[:, 1]), 4)
    lam_sk = skirt(0.7, 0.034, 0.88)
    bld.emit("lamellar", [(lam_top, bld.material("lamellar", bld.bake(lam_top, TEX_HI, lamellar_shader), normal_strength=3.0)),
                          (lam_sk, bld.material("lamellar.skirt", bld.bake(lam_sk, TEX_HI, lamellar_shader), normal_strength=3.0, double=True))])

    import props  # noqa: E402

    props.build_headgear(bld)
    props.build_weapons(bld)

    out = os.path.join(ROOT, "assets", "models", "humans.glb")
    bld.glb.save(out)
    print("записано", out, os.path.getsize(out) // 1024, "КБ")
    write_dims(rig)
    return bld


def write_dims(rig):
    off = rig.rest_offsets()
    gr, ga = rig.grip_point("r")
    gl, gla = rig.grip_point("l")
    f = lambda v: "Vec3::new({:.4}, {:.4}, {:.4})".format(*[float(x) for x in v])
    lines = [
        "// Создано tools/models/build_humans.py — не править руками.",
        "// Размеры скелета под тело из assets/models/humans.glb.",
        "use bevy::math::Vec3;",
        "",
        "/// Смещения суставов от родителя в покое (конечности висят прямо вниз).",
        "pub const REST: [Vec3; 15] = [",
    ] + [f"    {f(off[n])}, // {n}" for n in BONES] + [
        "];",
        f"pub const UPPER_ARM: f32 = {rig.lengths['upper_arm']:.4};",
        f"pub const FOREARM: f32 = {rig.lengths['forearm']:.4};",
        f"pub const THIGH: f32 = {rig.lengths['thigh']:.4};",
        f"pub const SHIN: f32 = {rig.lengths['shin']:.4};",
        "/// Высота голеностопа над землёй, когда стопа стоит ровно.",
        f"pub const ANKLE_HEIGHT: f32 = {rig.J['foot_r'][1]:.4};",
        "/// Центр кулака (где рукоять) и ось отверстия кулака в системе кисти: правая и левая.",
        f"pub const GRIP_R: Vec3 = {f(gr)};",
        f"pub const GRIP_AXIS_R: Vec3 = {f(ga)};",
        f"pub const GRIP_L: Vec3 = {f(gl)};",
        f"pub const GRIP_AXIS_L: Vec3 = {f(gla)};",
        "",
    ]
    path = os.path.join(ROOT, "src", "rig_dims.rs")
    open(path, "w").write("\n".join(lines))
    print("записано", path)


if __name__ == "__main__":
    build()
