"""Атлас хвойных лап (1024×1024, RGBA): четыре горизонтальные полосы 1024×256, в каждой —
ветка сосны с боковыми побегами и хвоей, комель слева, конец справа. Рисуется процедурно.

    python3 tools/vegetation/build_bough_atlas.py assets/textures/vegetation/bough_atlas.png

Этими полосами обшиты ярусы ветвей конической сосны (tools/vegetation/blender_trees.py,
`make_conifer`): лапа лежит вдоль ветви и свисает занавесом под ней.
"""
import math
import random
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage

W, H = 1024, 256
SS = 3

NEEDLES = [(34, 58, 30), (42, 70, 36), (28, 50, 30), (50, 80, 40), (38, 62, 40)]
LIGHT = (86, 112, 58)
WOOD = (66, 46, 30)


def shade(c, k):
    return tuple(max(0, min(255, int(round(v * k)))) for v in c)


def needles(d, rnd, x, y, ax, ay, length, count, spread, k_shade, w):
    """Пучок хвои в точке побега: иглы веером вокруг направления побега."""
    base = math.atan2(ay, ax)
    for _ in range(count):
        a = base + rnd.uniform(-spread, spread)
        ln = length * rnd.uniform(0.6, 1.1)
        c = rnd.choice(NEEDLES)
        if rnd.random() < 0.22:
            c = LIGHT
        c = shade(c, k_shade * rnd.uniform(0.75, 1.15))
        d.line([(x, y), (x + math.cos(a) * ln, y + math.sin(a) * ln)], fill=c + (255,), width=w)


def twig(d, rnd, x0, y0, ang, length, width, level, sparse):
    """Побег: ломаная с хвоей по всей длине; боковые побеги ёлочкой (два уровня)."""
    segs = max(4, int(length / (7 * SS)))
    x, y = x0, y0
    a = ang
    pts = [(x, y, a)]
    for i in range(segs):
        a += rnd.gauss(0, 0.035)
        if level == 0:
            a = a * 0.8 + ang * 0.2
        x += math.cos(a) * length / segs
        y += math.sin(a) * length / segs
        pts.append((x, y, a))
    thin = 0.55 if sparse else 1.0
    n_len = (44, 36, 30)[level] * SS
    nw = max(3, int(SS * 1.5))
    # Хвоя под побегом (рисуется первой — она «дальше» и темнее).
    for i, (px, py, pa) in enumerate(pts):
        t = i / segs
        if level == 0 and t < 0.05:
            continue
        needles(d, rnd, px, py, math.cos(pa), math.sin(pa), n_len * (1.0 - 0.25 * t), int(7 * thin), 1.3, 0.72, nw)
    # Боковые побеги.
    if level < 2:
        n_side = 30 if level == 0 else int(length / (26 * SS))
        for i in range(n_side):
            t = 0.05 + 0.9 * (i + rnd.uniform(0.0, 0.6)) / max(n_side, 1)
            k = t * segs
            i0 = min(int(k), segs - 1)
            px = pts[i0][0] + (pts[i0 + 1][0] - pts[i0][0]) * (k - i0)
            py = pts[i0][1] + (pts[i0 + 1][1] - pts[i0][1]) * (k - i0)
            side = 1 if i % 2 == 0 else -1
            sa = pts[i0][2] + side * rnd.uniform(0.55, 0.9)
            if level == 0:
                # К концу ветви побеги короче — лапа сходится клином; у комля тоже короче.
                shape = (1.0 - t) ** 0.6 * min(1.0, 0.45 + t * 2.5)
                sl = H * SS * 0.46 * shape * rnd.uniform(0.8, 1.05)
            else:
                sl = length * 0.42 * (1.0 - t) * rnd.uniform(0.7, 1.1)
            if sl > 14 * SS:
                twig(d, rnd, px, py, sa, sl, width * 0.5, level + 1, sparse)
    # Сам побег.
    for i in range(segs):
        t = i / segs
        wd = max(2, int(width * (1.0 - 0.75 * t)))
        d.line([pts[i][:2], pts[i + 1][:2]], fill=shade(WOOD, rnd.uniform(0.85, 1.1)) + (255,), width=wd)
    # Хвоя поверх побега — светлее (на неё падает свет).
    for i, (px, py, pa) in enumerate(pts):
        t = i / segs
        if level == 0 and t < 0.08:
            continue
        needles(d, rnd, px, py, math.cos(pa), math.sin(pa), n_len * (1.0 - 0.25 * t) * 0.8, int(6 * thin), 0.9, 1.08, nw)


def strip(rnd, sparse):
    im = Image.new("RGBA", (W * SS, H * SS), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    twig(d, rnd, 6 * SS, H * SS * 0.5 + rnd.uniform(-6, 6) * SS, rnd.uniform(-0.02, 0.02), W * SS * 0.9, 9 * SS, 0, sparse)
    return im


def dilate(rgba):
    solid = rgba[..., 3] > 24
    _, (iy, ix) = ndimage.distance_transform_edt(~solid, return_indices=True)
    out = rgba.copy()
    out[..., :3] = rgba[iy, ix, :3]
    return out


def main(out):
    rnd = random.Random(1510)
    atlas = np.zeros((H * 4, W, 4), np.uint8)
    for k in range(4):
        im = strip(rnd, sparse=(k == 3))
        a = np.asarray(im, np.float32) / 255.0
        a[..., :3] *= a[..., 3:4]
        pm = Image.fromarray((a * 255).astype(np.uint8), "RGBA").resize((W, H), Image.LANCZOS)
        b = np.asarray(pm, np.float32) / 255.0
        al = np.clip(b[..., 3:4], 1e-4, 1.0)
        b[..., :3] = np.clip(b[..., :3] / al, 0, 1)
        atlas[k * H:(k + 1) * H] = (b * 255 + 0.5).astype(np.uint8)
    atlas = dilate(atlas)
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(atlas).save(out, optimize=True)
    print("покрытие", [(atlas[k * H:(k + 1) * H, :, 3] > 110).mean().round(3) for k in range(4)])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "assets/textures/vegetation/bough_atlas.png")
