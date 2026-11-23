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
