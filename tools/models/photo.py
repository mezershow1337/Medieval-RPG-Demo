"""Фото-детали для запекания текстур персонажей: настоящее плетение шерсти и льна, зерно кожи.

Материалы — ambientCG (CC0), те же архивы, что качает tools/textures/build_surface_textures.py
(кэш ~/.cache/mrpg-ambientcg или папка из переменной MRPG_AMBIENTCG_DIR).
Фото проецируется трипланарно по 3D-положению текселя в метрах, поэтому плетение
везде одного масштаба и не тянется по швам развёртки.
"""
import os
import sys

import numpy as np
from PIL import Image, ImageFilter

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "textures"))
import build_surface_textures as bst  # noqa: E402

_cache = {}


def _load(asset, meters, texel):
    """Яркость фото (H,W), нормированная к среднему 1, размытая до размера текселя запекания."""
    key = (asset, meters, texel)
    if key in _cache:
        return _cache[key]
    m = bst.fetch(asset, os.environ.get("MRPG_AMBIENTCG_DIR"))
    im = m["color"].convert("L")
    # Пиксель фото не мельче половины текселя — иначе плетение превращается в муар.
    px = max(64, min(im.size[0], int(round(meters / (texel * 0.5)))))
    im = im.resize((px, px), Image.LANCZOS).filter(ImageFilter.GaussianBlur(0.5))
    lum = np.asarray(im, np.float32) / 255.0
    lum = lum / max(lum.mean(), 1e-3)
    _cache[key] = lum
    return lum


def _bilinear(img, u, v):
    h, w = img.shape
    x = (u % 1.0) * w - 0.5
    y = (v % 1.0) * h - 0.5
    x0 = np.floor(x).astype(int)
    y0 = np.floor(y).astype(int)
    fx = x - x0
    fy = y - y0
    x0 %= w
    y0 %= h
    x1 = (x0 + 1) % w
    y1 = (y0 + 1) % h
    a = img[y0, x0] * (1 - fx) + img[y0, x1] * fx
    b = img[y1, x0] * (1 - fx) + img[y1, x1] * fx
    return a * (1 - fy) + b * fy


def detail(P, N, asset, meters, texel=0.0015):
    """Множитель яркости (~1) по фото `asset`, тайл `meters` метров, трипланарно по P/N."""
    lum = _load(asset, meters, texel)
    w = np.abs(N) ** 4
    w = w / np.maximum(w.sum(-1, keepdims=True), 1e-9)
    x, y, z = P[..., 0] / meters, P[..., 1] / meters, P[..., 2] / meters
    out = _bilinear(lum, z, -y) * w[..., 0] + _bilinear(lum, x, -z) * w[..., 1] + _bilinear(lum, x, -y) * w[..., 2]
    return out


# Нормали текущего запекания: run_shader кладёт их сюда, чтобы шейдеры деталей
# (железо, дерево, кожа в props.py) могли брать фото-детали, не меняя своих сигнатур.
_current_n = None


def set_normals(N):
    global _current_n
    _current_n = N


def _rgb(asset, meters, texel):
    key = ("rgb", asset, meters, texel)
    if key in _cache:
        return _cache[key]
    m = bst.fetch(asset, os.environ.get("MRPG_AMBIENTCG_DIR"))
    im = m["color"].convert("RGB")
    px = max(64, min(im.size[0], int(round(meters / (texel * 0.5)))))
    im = im.resize((px, px), Image.LANCZOS)
    rgb = np.asarray(im, np.float32) / 255.0
    _cache[key] = rgb
    return rgb


def here(P, asset, meters, texel=0.001):
    """Как detail(), но нормали берутся из текущего запекания (None — проекция сверху/сбоку по P)."""
    N = _current_n
    if N is None or N.shape != P.shape:
        N = np.broadcast_to(np.array([0.0, 0.0, 1.0]), P.shape)
    return detail(P, N, asset, meters, texel)


def rust_here(P, asset="Metal053C", meters=0.35, texel=0.001):
    """Маска ржавчины (0..1) по фото ржавого железа: там, где красный канал заметно выше синего."""
    rgb = _rgb(asset, meters, texel)
    r = np.clip((rgb[..., 0] - rgb[..., 2] - 0.08) * 3.0, 0.0, 1.0)
    N = _current_n
    if N is None or N.shape != P.shape:
        N = np.broadcast_to(np.array([0.0, 0.0, 1.0]), P.shape)
    w = np.abs(N) ** 4
    w = w / np.maximum(w.sum(-1, keepdims=True), 1e-9)
    x, y, z = P[..., 0] / meters, P[..., 1] / meters, P[..., 2] / meters
    return _bilinear(r, z, -y) * w[..., 0] + _bilinear(r, x, -z) * w[..., 1] + _bilinear(r, x, -y) * w[..., 2]


def color_path(asset):
    """Путь к JPG цвета материала ambientCG (для узлов-картинок Blender) и его средняя яркость."""
    src = os.environ.get("MRPG_AMBIENTCG_DIR")
    if src:
        p = os.path.join(src, asset + "_1K-JPG_Color.jpg")
        if os.path.exists(p):
            return p, _mean(p)
    cache = os.path.expanduser("~/.cache/mrpg-ambientcg")
    p = os.path.join(cache, asset + "_Color.jpg")
    if not os.path.exists(p):
        m = bst.fetch(asset, src)
        os.makedirs(cache, exist_ok=True)
        m["color"].convert("RGB").save(p, quality=95)
    return p, _mean(p)


def _mean(p):
    key = ("mean", p)
    if key not in _cache:
        _cache[key] = float(np.asarray(Image.open(p).convert("L"), np.float32).mean() / 255.0)
    return _cache[key]
