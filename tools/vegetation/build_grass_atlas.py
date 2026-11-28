"""Атлас травы 2×2 (1024×1024, RGBA) для карточек травы — рисуется процедурно, без исходников.

    python3 tools/vegetation/build_grass_atlas.py assets/textures/vegetation/grass_atlas.png

Клетки (ряд сверху, слева направо):
  0 — луговой пучок: много тонких травин разной высоты, пара колосков;
  1 — ковыль: длинные дуги с бледными перистыми остями;
  2 — разнотравье: стебли с листьями, белые зонтики тысячелистника, жёлтые корзинки;
  3 — низкая дернина: короткая густая щётка (ковёр под ногами).
Низ клетки — земля. У корня травины тёмные (затеняют друг друга), к концам светлее;
задние травины темнее передних. Прозрачные пиксели залиты цветом ближайшей травины,
чтобы на мип-уровнях края не темнели.
"""
import math
import random
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw
from scipy import ndimage

CELL = 512
SS = 3  # сглаживание: рисуем втрое крупнее
S = CELL * SS


def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def shade(c, k):
    return tuple(max(0, min(255, int(round(v * k)))) for v in c)


ROOT = (22, 30, 14)
GREENS = [(74, 96, 44), (62, 88, 40), (84, 100, 48), (56, 82, 46), (92, 104, 52)]
DRY = (136, 124, 78)
PALE = (176, 168, 128)


def blade(d, x0, y0, h, lean, curve, w0, top, dry_tip, depth, segs=14, min_w=3.4):
    """Одна травина: полоса из сегментов, сужается к концу, сложена вдоль жилки."""
    pts = []
    for i in range(segs + 1):
        t = i / segs
        x = x0 + lean * h * t + curve * h * t * t
        x = min(max(x, S * 0.02), S * 0.98)
        y = y0 - h * t * (1.0 - 0.22 * abs(curve) * t)
        pts.append((x, y, t))
    for i in range(segs):
        (xa, ya, ta), (xb, yb, tb) = pts[i], pts[i + 1]
        dx, dy = xb - xa, yb - ya
        ln = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / ln, dx / ln
        wa = max(w0 * (1.0 - ta) ** 0.75, min_w) * 0.5
        wb = max(w0 * (1.0 - tb) ** 0.75, min_w) * 0.5
        tm = (ta + tb) * 0.5
        # Высота над землёй решает, насколько травина в тени соседей.
        height = (y0 - (ya + yb) * 0.5) / S
        ao = 0.3 + 0.7 * min(1.0, height / 0.42) ** 0.7
        c = mix(ROOT, top, min(1.0, tm * 1.5) ** 0.8)
        if dry_tip > 0.0:
            c = mix(c, DRY, max(0.0, (tm - (1.0 - dry_tip)) / max(dry_tip, 1e-3)) ** 1.5)
        k = ao * (0.72 + 0.28 * depth)
        mx, my = (xa + xb) * 0.5, (ya + yb) * 0.5
        # Две половинки листа: одна темнее.
        d.polygon([(xa - nx * wa, ya - ny * wa), (xa, ya), (xb, yb), (xb - nx * wb, yb - ny * wb)], fill=shade(c, k * 0.86) + (255,))
        d.polygon([(xa, ya), (xa + nx * wa, ya + ny * wa), (xb + nx * wb, yb + ny * wb), (xb, yb)], fill=shade(c, k * 1.06) + (255,))
        _ = (mx, my)
    return pts


def seed_head(d, pts, rnd, color):
    """Колосок на конце стебля: ряд зёрен."""
    n = len(pts)
    for i in range(int(n * 0.72), n):
        x, y, _ = pts[i]
        for side in (-1, 1):
            r = rnd.uniform(3.0, 5.5) * SS / 3
            ox = side * rnd.uniform(1.5, 4.0) * SS / 3
            d.ellipse([x + ox - r * 0.55, y - r, x + ox + r * 0.55, y + r], fill=shade(color, rnd.uniform(0.85, 1.1)) + (255,))


def plume(d, pts, rnd):
    """Перистая ость ковыля: тонкие волоски в конце дуги."""
    n = len(pts)
    for i in range(int(n * 0.55), n):
        x, y, t = pts[i]
        for _ in range(5):
            a = rnd.uniform(-2.6, -0.5)
            ln = rnd.uniform(8, 22) * SS / 3 * (0.6 + t)
            c = shade(PALE, rnd.uniform(0.8, 1.12))
            d.line([(x, y), (x + math.cos(a) * ln, y + math.sin(a) * ln)], fill=c + (255,), width=max(2, int(SS * 0.9)))


def leaf(d, x, y, ang, ln, wd, color, k):
    """Листок-ромб на стебле."""
    ca, sa = math.cos(ang), math.sin(ang)
    tip = (x + ca * ln, y + sa * ln)
    mid = (x + ca * ln * 0.45, y + sa * ln * 0.45)
    px, py = -sa * wd, ca * wd
    d.polygon([(x, y), (mid[0] + px, mid[1] + py), tip], fill=shade(color, k * 1.05) + (255,))
    d.polygon([(x, y), (mid[0] - px, mid[1] - py), tip], fill=shade(color, k * 0.84) + (255,))


def meadow(rnd):
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    n = 170
    y0 = S * 0.995
    for k in range(n):
        depth = k / (n - 1)
        x0 = S * (0.5 + rnd.gauss(0, 0.15))
        x0 = min(max(x0, S * 0.1), S * 0.9)
        off = (x0 / S - 0.5) / 0.5
        h = S * rnd.uniform(0.3, 0.95) * (1.0 - 0.45 * abs(off) ** 1.5) * (0.75 + 0.25 * (1 - depth * 0.4))
        lean = off * rnd.uniform(0.15, 0.4) + rnd.gauss(0, 0.08)
        curve = rnd.gauss(0, 0.18) + off * 0.15
        top = rnd.choice(GREENS)
        pts = blade(d, x0, y0, h, lean, curve, rnd.uniform(11, 22) * SS / 3, top, rnd.choice([0, 0, 0.25, 0.4]), depth)
        if rnd.random() < 0.06 and h > S * 0.6:
            seed_head(d, pts, rnd, (150, 136, 84))
    return im


def feather(rnd):
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    n = 130
    y0 = S * 0.995
    side = rnd.choice([-1, 1])
    for k in range(n):
        depth = k / (n - 1)
        x0 = S * (0.5 + rnd.gauss(0, 0.09) - side * 0.14)
        h = S * rnd.uniform(0.45, 0.98)
        lean = side * rnd.uniform(0.02, 0.3) + rnd.gauss(0, 0.12)
        curve = side * rnd.uniform(0.1, 0.42)
        top = mix(rnd.choice(GREENS), DRY, rnd.uniform(0.25, 0.75))
        pts = blade(d, x0, y0, h, lean, curve, rnd.uniform(7, 13) * SS / 3, top, 0.5, depth, segs=18, min_w=2.8)
        if rnd.random() < 0.3 and h > S * 0.65:
            plume(d, pts, rnd)
    return im


def herbs(rnd):
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    y0 = S * 0.995
    # Задний план — трава.
    for k in range(60):
        x0 = S * rnd.uniform(0.15, 0.85)
        blade(d, x0, y0, S * rnd.uniform(0.2, 0.55), rnd.gauss(0, 0.15), rnd.gauss(0, 0.25), rnd.uniform(11, 20) * SS / 3, rnd.choice(GREENS), 0.0, k / 120)
    stalks = 9
    for k in range(stalks):
        depth = 0.5 + 0.5 * k / (stalks - 1)
        x0 = S * rnd.uniform(0.22, 0.78)
        h = S * rnd.uniform(0.5, 0.9)
        lean = rnd.gauss(0, 0.12)
        curve = rnd.gauss(0, 0.12)
        stem_c = rnd.choice([(70, 92, 46), (86, 96, 58), (64, 84, 50)])
        pts = blade(d, x0, y0, h, lean, curve, 6.5 * SS / 3, stem_c, 0.0, depth, segs=16, min_w=2.2)
        leaf_c = rnd.choice([(66, 98, 44), (78, 104, 60), (58, 88, 52)])
        for i in range(2, len(pts) - 2):
            x, y, t = pts[i]
            if rnd.random() < 0.75:
                for sgn in (-1, 1):
                    ang = -math.pi / 2 + sgn * rnd.uniform(0.7, 1.25)
                    ln = S * rnd.uniform(0.05, 0.1) * (1.1 - t * 0.6)
                    height = (y0 - y) / S
                    ao = 0.4 + 0.6 * min(1.0, height / 0.4)
                    leaf(d, x, y, ang, ln, ln * 0.2, leaf_c, ao * (0.75 + 0.25 * depth))
        x, y, _ = pts[-1]
        kind = rnd.random()
        if kind < 0.45:
            # Тысячелистник: плоский белый зонтик.
            for _ in range(26):
                ox, oy = rnd.gauss(0, S * 0.03), rnd.gauss(0, S * 0.008)
                r = rnd.uniform(2.5, 4.5) * SS / 3 * 1.6
                d.ellipse([x + ox - r, y + oy - r, x + ox + r, y + oy + r], fill=shade((206, 204, 188), rnd.uniform(0.8, 1.05)) + (255,))
        elif kind < 0.7:
            # Жёлтая корзинка (девясил, пижма).
            for _ in range(9):
                ox, oy = rnd.gauss(0, S * 0.014), rnd.gauss(0, S * 0.008)
                r = rnd.uniform(4, 6.5) * SS / 3 * 1.5
                d.ellipse([x + ox - r, y + oy - r, x + ox + r, y + oy + r], fill=shade((196, 160, 44), rnd.uniform(0.8, 1.08)) + (255,))
    return im


def turf(rnd):
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    n = 300
    y0 = S * 0.995
    for k in range(n):
        depth = k / (n - 1)
        x0 = S * rnd.uniform(0.04, 0.96)
        edge = min(x0 / S, 1 - x0 / S) / 0.5
        h = S * rnd.uniform(0.16, 0.62) * (0.6 + 0.4 * edge ** 0.5)
        blade(d, x0, y0, h, rnd.gauss(0, 0.22), rnd.gauss(0, 0.3), rnd.uniform(12, 22) * SS / 3, rnd.choice(GREENS), rnd.choice([0, 0, 0, 0.3]), depth, segs=10)
    return im


def dilate(rgba):
    solid = rgba[..., 3] > 24
    _, (iy, ix) = ndimage.distance_transform_edt(~solid, return_indices=True)
    out = rgba.copy()
    out[..., :3] = rgba[iy, ix, :3]
    return out


def main(out):
    rnd = random.Random(510)
    cells = [meadow(rnd), feather(rnd), herbs(rnd), turf(rnd)]
    atlas = np.zeros((CELL * 2, CELL * 2, 4), np.uint8)
    for k, im in enumerate(cells):
        # Уменьшаем с предумножением альфы — без тёмной каймы.
        a = np.asarray(im, np.float32) / 255.0
        a[..., :3] *= a[..., 3:4]
        pm = Image.fromarray((a * 255).astype(np.uint8), "RGBA").resize((CELL, CELL), Image.LANCZOS)
        b = np.asarray(pm, np.float32) / 255.0
        al = np.clip(b[..., 3:4], 1e-4, 1.0)
        b[..., :3] = np.clip(b[..., :3] / al, 0, 1)
        y, x = divmod(k, 2)
        atlas[y * CELL:(y + 1) * CELL, x * CELL:(x + 1) * CELL] = (b * 255 + 0.5).astype(np.uint8)
    atlas = dilate(atlas)
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    Image.fromarray(atlas).save(out, optimize=True)
    print("покрытие", [(atlas[(k // 2) * CELL:(k // 2 + 1) * CELL, (k % 2) * CELL:(k % 2 + 1) * CELL, 3] > 110).mean().round(3) for k in range(4)])


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "assets/textures/vegetation/grass_atlas.png")
