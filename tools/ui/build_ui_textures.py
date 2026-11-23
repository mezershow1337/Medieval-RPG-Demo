#!/usr/bin/env python3
"""Текстуры средневекового интерфейса: assets/ui/*.png.

Рамки — рельеф (карта высот: бусины, витой жгут, бляхи с крестом), освещённый
как металл: бронза по фотоскану Metal048C, железо по Metal053C. Подложки —
фотосканы ambientCG (CC0): тёмное дерево Wood060, кожа Leather014; пергамент
процедурный (периодический шум через БПФ, поэтому бесшовный).

Всё, что Bevy тайлит при 9-slice (стороны и центр), сделано бесшовным: узор
вдоль стороны имеет период, кратный длине стороны, а подложка — период,
равный центру.

Запуск:  MRPG_AMBIENTCG_DIR=<папка с *_1K-JPG_Color.jpg> python3 tools/ui/build_ui_textures.py
"""
import os
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "assets", "ui")
SRC = os.environ.get("MRPG_AMBIENTCG_DIR", os.path.expanduser("~/.cache/mrpg-ambientcg"))
SS = 2  # суперсэмплинг: считаем вдвое крупнее, потом уменьшаем (сглаживание)
RNG = np.random.default_rng(510)


# ---------------------------------------------------------------------------
# Общие инструменты
# ---------------------------------------------------------------------------

def photo(name):
    path = os.path.join(SRC, f"{name}_1K-JPG_Color.jpg")
    if not os.path.exists(path):
        sys.exit(f"нет {path}: скачайте {name} с ambientcg.com (CC0) и укажите MRPG_AMBIENTCG_DIR")
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.float32) / 255.0


def tile_to(img, period, shape, offset=(0, 0)):
    """Бесшовная текстура, уменьшенная до period×period и разложенная по shape со сдвигом."""
    p = np.asarray(Image.fromarray((img * 255).astype(np.uint8)).resize((period, period), Image.LANCZOS), dtype=np.float32) / 255.0
    h, w = shape
    ys = (np.arange(h) - offset[0]) % period
    xs = (np.arange(w) - offset[1]) % period
    return p[ys][:, xs]


def fnoise(n, beta, seed):
    """Периодический шум 1/f^beta размера n×n в диапазоне ~[-1, 1]."""
    r = np.random.default_rng(seed)
    f = np.fft.fft2(r.standard_normal((n, n)))
    ky = np.fft.fftfreq(n)[:, None]
    kx = np.fft.fftfreq(n)[None, :]
    k = np.sqrt(kx * kx + ky * ky)
    k[0, 0] = 1.0
    out = np.real(np.fft.ifft2(f / k**beta))
    out -= out.mean()
    return out / (np.abs(out).max() + 1e-6)


def aniso_noise(n, sx, sy, seed):
    """Периодический шум, вытянутый по оси x (волокна)."""
    r = np.random.default_rng(seed)
    f = np.fft.fft2(r.standard_normal((n, n)))
    ky = np.fft.fftfreq(n)[:, None]
    kx = np.fft.fftfreq(n)[None, :]
    g = np.exp(-((kx * sx) ** 2 + (ky * sy) ** 2))
    out = np.real(np.fft.ifft2(f * g))
    return out / (np.abs(out).max() + 1e-6)


def normals(h, strength):
    gy, gx = np.gradient(h)
    n = np.dstack([-gx * strength, -gy * strength, np.ones_like(h)])
    return n / np.linalg.norm(n, axis=2, keepdims=True)


LIGHT = np.array([-0.45, -0.62, 0.64])
LIGHT /= np.linalg.norm(LIGHT)
HALF = LIGHT + np.array([0, 0, 1.0])
HALF /= np.linalg.norm(HALF)


def shade_metal(h, albedo, strength=2.2, shine=46, spec=0.9, patina=None):
    """Освещённый рельефный металл. h — высоты в пикселях (уже в масштабе SS)."""
    n = normals(h, strength)
    diff = np.clip((n * LIGHT).sum(2), 0, 1)
    sp = np.clip((n * HALF).sum(2), 0, 1) ** shine
    # «Небо» сверху: грани, смотрящие вверх, светлее; снизу — темнее (отражение тёмного зала).
    sky = np.clip(0.55 - n[..., 1] * 0.9, 0, 1)
    ao = np.clip(1.0 - np.clip(ndimage.gaussian_filter(h, 3 * SS) - h, 0, None) * 0.16, 0.25, 1)
    col = albedo * (0.16 + 0.72 * diff[..., None] + 0.32 * sky[..., None]) * ao[..., None]
    col += sp[..., None] * spec * np.array([1.0, 0.90, 0.70]) * 0.8
    if patina is not None:
        cav = np.clip((1 - ao) * 2.2, 0, 1)[..., None]
        col = col * (1 - cav * 0.7) + np.array(patina) * cav * 0.7 * (0.5 + 0.5 * diff[..., None])
    return np.clip(col, 0, 1)


def bump(d, center, width):
    """Скруглённая бусина: полукруг по расстоянию d."""
    x = (d - center) / width
    return np.sqrt(np.clip(1 - x * x, 0, 1))


def save(name, rgb, alpha=None, size=None):
    rgb = np.clip(rgb, 0, 1)
    if alpha is None:
        alpha = np.ones(rgb.shape[:2], np.float32)
    rgba = np.dstack([rgb, np.clip(alpha, 0, 1)])
    img = Image.fromarray((rgba * 255 + 0.5).astype(np.uint8), "RGBA")
    if size is not None:
        img = img.resize(size, Image.LANCZOS)
    img.save(os.path.join(OUT, name), optimize=True)
    print("  ", name, img.size)


def frame_coords(H, W, B):
    """d — расстояние до внешнего края, t — координата вдоль стороны (от начала стороны),
    corner — маска угловых квадратов B×B."""
    y, x = np.mgrid[0:H, 0:W].astype(np.float32) + 0.5
    dl, dr, dt, db = x, W - x, y, H - y
    d = np.minimum(np.minimum(dl, dr), np.minimum(dt, db))
    horiz = np.minimum(dt, db) <= np.minimum(dl, dr)
    t = np.where(horiz, x - B, y - B)
    corner = ((x < B) | (x > W - B)) & ((y < B) | (y > H - B))
    cx = np.where(x < W / 2, B / 2, W - B / 2)
    cy = np.where(y < H / 2, B / 2, H - B / 2)
    return x, y, d, t, corner, cx, cy


def rope(d, t, d0, d1, period):
    """Витой жгут (византийская «верёвка»): косые валики между d0 и d1."""
    w = d1 - d0
    u = (d - d0) / w  # 0..1 поперёк
    phase = (t / period + u * 0.9) % 1.0
    # Каждая прядь — скрученный цилиндр: выпукла и поперёк, и вдоль витка.
    strand = np.sqrt(np.clip(np.sin(np.pi * phase), 0, 1))
    across = np.sqrt(np.clip(1 - (2 * u - 1) ** 2, 0, 1))
    return np.where((u >= 0) & (u <= 1), (0.35 + 0.65 * strand) * (0.4 + 0.6 * across), 0)


def cross_boss(x, y, cx, cy, r):
    """Круглая бляха с равноконечным крестом «патте» и кольцом."""
    dx, dy = x - cx, y - cy
    rr = np.sqrt(dx * dx + dy * dy)
    dome = np.sqrt(np.clip(1 - (rr / r) ** 2, 0, 1)) * 0.6
    ring = bump(rr, r * 0.86, r * 0.12) * 0.35
    ax, ay = np.abs(dx), np.abs(dy)
    # Крест патте: плечи расширяются к краю.
    arm = r * 0.68
    wid = lambda a: r * 0.12 + a * 0.32
    crs = ((ax <= wid(ay)) & (ay <= arm)) | ((ay <= wid(ax)) & (ax <= arm))
    crs = ndimage.gaussian_filter(crs.astype(np.float32), 0.7 * SS)
    return np.where(rr <= r, dome + ring + crs * 0.55, 0)


# ---------------------------------------------------------------------------
# Материалы
# ---------------------------------------------------------------------------

def bronze_albedo(shape, offset=(0, 0)):
    brass = tile_to(photo("Metal048C"), 256 * SS, shape, offset)
    g = brass.mean(2, keepdims=True)
    return np.clip((brass * 0.5 + g * 0.5) * np.array([0.80, 0.58, 0.34]) * 0.92, 0, 1)


def iron_albedo(shape, offset=(0, 0)):
    iron = tile_to(photo("Metal053C"), 256 * SS, shape, offset)
    g = iron.mean(2, keepdims=True)
    return np.clip(g * np.array([0.62, 0.62, 0.64]) + 0.04, 0, 1)


def wood_tile(period, shape, offset, dark=0.42):
    w = tile_to(photo("Wood060"), period, shape, offset)
    g = w.mean(2, keepdims=True)
    w = (w * 0.75 + g * 0.25) * dark
    return w * np.array([1.0, 0.93, 0.86])


def leather_tile(period, shape, offset, dark=0.62):
    lt = tile_to(photo("Leather014"), period, shape, offset)
    return lt * dark * np.array([1.0, 0.86, 0.78])


def parchment_tile(n, seed=7):
    base = np.array([0.87, 0.77, 0.58])
    low = fnoise(n, 1.6, seed)
    mid = fnoise(n, 1.1, seed + 1)
    fib = aniso_noise(n, 30, 200, seed + 2)
    spots = np.clip(fnoise(n, 1.3, seed + 3) - 0.35, 0, None) * 2.0
    lum = 1 + low * 0.07 + mid * 0.035 + fib * 0.03 - spots * 0.10
    col = base * lum[..., None]
    col = col * (1 - spots[..., None] * np.array([0.0, 0.05, 0.15]))
    return np.clip(col, 0, 1)


# ---------------------------------------------------------------------------
# Окна
# ---------------------------------------------------------------------------

def panel_wood():
    """Главное окно: тёмная доска в бронзовой раме с жгутом и бляхами-крестами по углам."""
    S, B = 512, 32
    s, b = S * SS, B * SS
    x, y, d, t, corner, cx, cy = frame_coords(s, s, b)
    period = 32 * SS  # 448 / 32 = 14 витков на сторону
    h = np.zeros((s, s), np.float32)
    metal = d < 25 * SS
    h += np.where(d < 2.5 * SS, d / (2.5 * SS) * 3, 0)
    h += np.where((d >= 2.5 * SS) & metal, 3, 0)
    h += bump(d, 4.5 * SS, 2.2 * SS) * 3.0 * SS
    h += rope(d, t, 7.5 * SS, 19.5 * SS, period) * 5.0 * SS
    h += bump(d, 22 * SS, 2.0 * SS) * 2.6 * SS
    # Углы: бляха с крестом поверх жгута.
    boss = cross_boss(x, y, cx, cy, 13.5 * SS)
    in_boss = corner & (boss > 0)
    h = np.where(in_boss, 3 + boss * 8 * SS, h)
    # Мелкие заклёпки посреди каждой стороны не нужны (сторона тайлится) — только углы.
    metal = metal | in_boss
    alb = bronze_albedo((s, s))
    col_metal = shade_metal(h, alb, patina=(0.22, 0.36, 0.30))
    wood = wood_tile((S - 2 * B) * SS, (s, s), (b, b))
    # Тень от рамы на доску (только в пределах рамы, чтобы центр оставался бесшовным).
    shadow = np.clip((d - 25 * SS) / (7 * SS), 0, 1)
    wood = wood * (0.45 + 0.55 * shadow[..., None] ** 0.8)
    col = np.where(metal[..., None], col_metal, wood)
    alpha = np.where(d < 1.0 * SS, 0.0, 1.0) + np.where((d >= 1.0 * SS) & (d < 2 * SS), 0.6, 0)
    save("panel_wood.png", col, np.clip(alpha, 0, 1), (S, S))


def panel_leather():
    """Малое окно (HUD, подсказки, всплывающие описания): тёмная кожа, строчка, железные уголки."""
    S, B = 256, 18
    s, b = S * SS, B * SS
    x, y, d, t, corner, cx, cy = frame_coords(s, s, b)
    lt = leather_tile((S - 2 * B) * SS, (s, s), (b, b), dark=0.5)
    # Подвёрнутый край кожи: светлее и выпуклый.
    edge = bump(d, 3 * SS, 3 * SS)
    lum = 0.75 + 0.5 * edge
    # Строчка: стежки вдоль стороны, период 8 (кратен 220).
    stitch_period = 10 * SS
    ph = (t % stitch_period) / stitch_period
    stitch = (np.abs(d - 9 * SS) < 0.9 * SS) & (ph > 0.15) & (ph < 0.75) & ~corner
    holes = (np.abs(d - 9 * SS) < 1.4 * SS) & ((ph < 0.12) | (ph > 0.78)) & ~corner
    col = lt * lum[..., None]
    col = np.where(holes[..., None], col * 0.45, col)
    col = np.where(stitch[..., None], np.array([0.72, 0.62, 0.46]) * (0.8 + 0.2 * edge[..., None]), col)
    # Железные уголки-накладки.
    cxl, cyl = x - np.where(x < s / 2, 0, s), y - np.where(y < s / 2, 0, s)
    ax, ay = np.abs(cxl), np.abs(cyl)
    plate = corner & (ax + ay < 26 * SS) & (ax < 17 * SS) & (ay < 17 * SS)
    ph_h = np.where(plate, 2.5 * SS, 0) + cross_boss(x, y, np.where(x < s / 2, 6.5 * SS, s - 6.5 * SS), np.where(y < s / 2, 6.5 * SS, s - 6.5 * SS), 3.5 * SS) * 4 * SS
    ph_h = ndimage.gaussian_filter(ph_h, 0.8 * SS)
    iron = shade_metal(ph_h, iron_albedo((s, s)), strength=2.5, shine=30, spec=0.5)
    col = np.where((ph_h > 0.3 * SS)[..., None], iron, col)
    shadow = np.clip((d - 3 * SS) / (8 * SS), 0, 1)
    col = col * (0.7 + 0.3 * shadow[..., None])
    alpha = np.clip((d - 0.5 * SS) / SS, 0, 1)
    save("panel_leather.png", col, alpha, (S, S))


def panel_parchment():
    """Книга и окно знаний: пергамент с двойной рамкой чернилами и киноварью, обожжённые края."""
    S, B = 512, 40
    s, b = S * SS, B * SS
    x, y, d, t, corner, cx, cy = frame_coords(s, s, b)
    L = (S - 2 * B) * SS
    par = parchment_tile(L)
    ys = (np.arange(s) - b) % L
    xs = (np.arange(s) - b) % L
    col = par[ys][:, xs]
    # Края темнее и «обтрёпаны»: шум вдоль стороны (период кратен стороне).
    edge_noise = np.sin(t / L * 2 * np.pi * 7) * 0.5 + np.sin(t / L * 2 * np.pi * 19 + 1.3) * 0.3 + np.sin(t / L * 2 * np.pi * 41 + 0.4) * 0.2
    rag = 3.5 * SS + edge_noise * 1.6 * SS
    burn = np.clip(1 - (d - rag) / (14 * SS), 0, 1) ** 1.6
    col = col * (1 - burn[..., None] * np.array([0.55, 0.62, 0.72]))
    ink = np.array([0.20, 0.12, 0.07])
    red = np.array([0.62, 0.16, 0.10])  # киноварь
    line = lambda c, w: np.clip(1 - np.abs(d - c) / w, 0, 1)
    outer = line(20 * SS, 1.1 * SS)
    inner = line(25 * SS, 0.6 * SS)
    col = col * (1 - outer[..., None]) + red * outer[..., None]
    col = col * (1 - inner[..., None] * 0.85) + ink * inner[..., None] * 0.85
    # По углам — киноварный ромб с крестиком.
    dx, dy = np.abs(x - cx), np.abs(y - cy)
    ccx = np.where(x < s / 2, 22.5 * SS, s - 22.5 * SS)
    ccy = np.where(y < s / 2, 22.5 * SS, s - 22.5 * SS)
    dx, dy = np.abs(x - ccx), np.abs(y - ccy)
    dia = corner & (dx + dy < 7 * SS)
    dia_rim = corner & (np.abs(dx + dy - 7 * SS) < 0.8 * SS)
    crs = corner & (((dx < 0.7 * SS) & (dy < 4 * SS)) | ((dy < 0.7 * SS) & (dx < 4 * SS)))
    col = np.where(dia[..., None], col * 0.4 + red * 0.6, col)
    col = np.where(dia_rim[..., None] | crs[..., None], ink, col)
    alpha = np.clip((d - rag + 1.5 * SS) / (1.5 * SS), 0, 1)
    save("panel_parchment.png", col, alpha, (S, S))


def button(name, state):
    """Кнопка: бронзовый ободок и кожаная/деревянная вставка. state: normal/hover/pressed/disabled."""
    W, H, B = 192, 48, 14
    w, h_, b = W * SS, H * SS, B * SS
    x, y, d, t, corner, cx, cy = frame_coords(h_, w, b)
    hgt = np.zeros((h_, w), np.float32)
    hgt += np.clip(d / (2 * SS), 0, 1) * 2 * SS
    hgt += bump(d, 3.5 * SS, 2.0 * SS) * 2.6 * SS
    hgt += np.where(d > 6 * SS, -1.5 * SS, 0)
    if state == "pressed":
        hgt = hgt.max() - hgt  # свет как будто снизу: кнопка вдавлена
    rim = d < 6.5 * SS
    alb = iron_albedo((h_, w)) if state == "disabled" else bronze_albedo((h_, w))
    metal = shade_metal(hgt, alb, patina=(0.22, 0.36, 0.30) if state != "disabled" else None, spec=0.6 if state == "disabled" else 0.9)
    fill = leather_tile(256 * SS, (h_, w), (0, 0), dark=0.55) * np.array([1.05, 0.82, 0.68])
    yy = (y / h_)[..., None]
    if state == "normal":
        fill = fill * (1.15 - 0.35 * yy)
    elif state == "hover":
        fill = fill * (1.45 - 0.35 * yy) + np.array([0.10, 0.05, 0.0])
        metal = np.clip(metal * 1.18 + 0.03, 0, 1)
    elif state == "pressed":
        fill = fill * (0.65 + 0.35 * yy)
    else:
        g = fill.mean(2, keepdims=True)
        fill = (g * 0.8 + fill * 0.2) * 0.75
        metal = metal * 0.8
    inner = np.clip((d - 6.5 * SS) / (3 * SS), 0, 1)[..., None]
    fill = fill * (0.6 + 0.4 * inner)
    col = np.where(rim[..., None], metal, fill)
    alpha = np.clip((d - 0.4 * SS) / SS, 0, 1)
    save(name, col, alpha, (W, H))


def slot(name, state):
    """Ячейка сумки/экипировки: углубление с тонким бронзовым ободком."""
    S, B = 64, 10
    s, b = S * SS, B * SS
    x, y, d, t, corner, cx, cy = frame_coords(s, s, b)
    hgt = bump(d, 1.6 * SS, 1.4 * SS) * 1.6 * SS
    rim = d < 3 * SS
    alb = bronze_albedo((s, s))
    metal = shade_metal(hgt, alb, strength=2.0, patina=(0.22, 0.36, 0.30))
    floor = leather_tile(128 * SS, (s, s), (0, 0), dark=0.22)
    # Внутренняя тень: от верхнего и левого края сильнее.
    yy, xx = y / s, x / s
    sh = np.clip(np.minimum((y - 3 * SS) / (9 * SS), (x - 3 * SS) / (9 * SS)), 0, 1)
    floor = floor * (0.35 + 0.65 * sh[..., None])
    if state == "hover":
        metal = np.clip(metal * 1.35 + 0.05, 0, 1)
        floor = floor * 1.35 + np.array([0.035, 0.025, 0.01])
    elif state == "selected":
        metal = np.clip(metal * 1.5 + np.array([0.12, 0.08, 0.0]), 0, 1)
        glow = np.clip(1 - (d - 3 * SS) / (6 * SS), 0, 1)[..., None]
        floor = floor + glow * np.array([0.45, 0.30, 0.08]) * 0.6
    col = np.where(rim[..., None], metal, floor)
    alpha = np.where(rim, 1.0, 0.92)
    save(name, col, alpha, (S, S))


def bar_frame():
    """Рамка полосы (здоровье, выносливость): железо с заклёпками на концах, середина прозрачна."""
    W, H, B = 128, 24, 10
    w, h_, b = W * SS, H * SS, B * SS
    x, y, d, t, corner, cx, cy = frame_coords(h_, w, b)
    hgt = bump(d, 2.2 * SS, 2.2 * SS) * 2.2 * SS + np.where(d < 4.5 * SS, 1.0 * SS, 0)
    rim = d < 4.5 * SS
    metal = shade_metal(hgt, iron_albedo((h_, w)), strength=2.4, shine=30, spec=0.6)
    inner_shadow = np.clip((d - 4.5 * SS) / (4 * SS), 0, 1)
    alpha = np.where(rim, np.clip((d - 0.4 * SS) / SS, 0, 1), (1 - inner_shadow) * 0.55)
    col = np.where(rim[..., None], metal, 0.0)
    save("bar_frame.png", col, alpha, (W, H))


def divider():
    """Разделитель: бронзовая линия (тайлится) и средник с крестом."""
    W, H = 64, 12
    w, h_ = W * SS, H * SS
    y, x = np.mgrid[0:h_, 0:w].astype(np.float32) + 0.5
    hgt = bump(y, h_ / 2, 1.6 * SS) * 1.5 * SS
    col = shade_metal(hgt, bronze_albedo((h_, w)), patina=(0.22, 0.36, 0.30))
    alpha = np.clip(1.6 * SS - np.abs(y - h_ / 2) + 0.5, 0, 1)
    save("divider_line.png", col, alpha, (W, H))
    W, H = 48, 24
    w, h_ = W * SS, H * SS
    y, x = np.mgrid[0:h_, 0:w].astype(np.float32) + 0.5
    cxm, cym = w / 2, h_ / 2
    dia = np.clip(1 - (np.abs(x - cxm) / (20 * SS) + np.abs(y - cym) / (10 * SS)), 0, 1)
    hgt = np.sqrt(dia) * 2.5 * SS + cross_boss(x, y, cxm, cym, 6.5 * SS) * 4 * SS
    line = bump(y, cym, 1.6 * SS) * 1.5 * SS
    hgt = np.maximum(hgt, line)
    col = shade_metal(hgt, bronze_albedo((h_, w)), patina=(0.22, 0.36, 0.30))
    alpha = np.clip(hgt * 4 / SS, 0, 1)
    save("divider_mid.png", col, alpha, (W, H))


def vignette():
    """Затемнение мира за открытым окном: к краям темнее."""
    S = 256
    y, x = np.mgrid[0:S, 0:S].astype(np.float32) / (S - 1) * 2 - 1
    r = np.sqrt(x * x * 0.8 + y * y)
    a = 0.38 + 0.42 * np.clip(r / 1.3, 0, 1) ** 1.8
    col = np.zeros((S, S, 3)) + np.array([0.03, 0.02, 0.01])
    save("vignette.png", col, a)


def main():
    os.makedirs(OUT, exist_ok=True)
    print("assets/ui:")
    panel_wood()
    panel_leather()
    panel_parchment()
    for st in ["normal", "hover", "pressed", "disabled"]:
        button(f"button_{st}.png", st)
    for st in ["normal", "hover", "selected"]:
        slot(f"slot_{st}.png", st)
    bar_frame()
    divider()
    vignette()


if __name__ == "__main__":
    main()
