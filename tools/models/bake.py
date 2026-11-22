"""Запекание процедурных текстур в UV меша.

Каждый тексель получает 3D-положение и нормаль точки на поверхности (в позе покоя игры),
а функция-«шейдер» по ним считает цвет, шероховатость, металл и рельеф. Так полосы на тунике,
брови, губы, щетина и ряды пластин ламелляра рисуются в пространстве тела и не зависят
от раскладки UV.
"""

import numpy as np


def rasterize(uv, tris, attrs, size):
    """uv (N,2) в [0,1], tris (T,3), attrs (N,C). Возвращает (size,size,C) и маску."""
    C = attrs.shape[1]
    img = np.zeros((size, size, C))
    hit = np.zeros((size, size), bool)
    puv = uv[tris] * (size - 1)  # (T,3,2)
    ext = np.maximum(np.ptp(puv[:, :, 0], axis=1), np.ptp(puv[:, :, 1], axis=1))
    k_all = np.clip(np.ceil(ext * 1.6).astype(int), 1, 512)
    buckets = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512]
    for k in buckets:
        sel = np.nonzero((k_all <= k) & (k_all > (k // 2 if k > 1 else 0)))[0]
        if len(sel) == 0:
            continue
        ii, jj = np.meshgrid(np.arange(k + 1), np.arange(k + 1), indexing="ij")
        m = ii + jj <= k
        a = (ii[m] / k)[None, :, None]
        b = (jj[m] / k)[None, :, None]
        c = 1 - a - b
        for chunk in np.array_split(sel, max(1, len(sel) * a.shape[1] // 2_000_000 + 1)):
            t = tris[chunk]
            p = a * uv[t[:, 0]][:, None] + b * uv[t[:, 1]][:, None] + c * uv[t[:, 2]][:, None]
            val = a * attrs[t[:, 0]][:, None] + b * attrs[t[:, 1]][:, None] + c * attrs[t[:, 2]][:, None]
            px = np.clip(np.rint(p[..., 0] * (size - 1)).astype(int), 0, size - 1).ravel()
            py = np.clip(np.rint((1 - p[..., 1]) * (size - 1)).astype(int), 0, size - 1).ravel()
            img[py, px] = val.reshape(-1, C)
            hit[py, px] = True
    return img, hit


def dilate(img, hit, steps=8):
    """Заливает пустые тексели вокруг островов UV, чтобы на швах не было тёмных щелей."""
    img = img.copy()
    hit = hit.copy()
    for _ in range(steps):
        acc = np.zeros_like(img)
        cnt = np.zeros(hit.shape)
        for dy, dx in ((0, 1), (0, -1), (1, 0), (-1, 0), (1, 1), (-1, -1), (1, -1), (-1, 1)):
            h = np.roll(np.roll(hit, dy, 0), dx, 1)
            acc += np.roll(np.roll(img, dy, 0), dx, 1) * h[..., None]
            cnt += h
        new = (~hit) & (cnt > 0)
        img[new] = acc[new] / cnt[new][:, None]
        hit |= new
    return img


def height_to_normal(h, strength=1.0):
    """Карта нормалей (касательное пространство, OpenGL/glTF: +Y вверх по V) из рельефа."""
    gx = (np.roll(h, -1, 1) - np.roll(h, 1, 1)) * 0.5 * strength
    gy = (np.roll(h, -1, 0) - np.roll(h, 1, 0)) * 0.5 * strength
    n = np.stack([-gx, gy, np.ones_like(h)], axis=-1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    return n * 0.5 + 0.5


# ---------------------------------------------------------------------------------------------
# Шум

def _hash3(i, j, k, seed):
    h = (i * 73856093) ^ (j * 19349663) ^ (k * 83492791) ^ (seed * 2654435761)
    h = (h ^ (h >> 13)) * 1274126177
    h = h ^ (h >> 16)
    return (h & 0xFFFFFF) / float(0xFFFFFF)


def value_noise(p, freq, seed=0):
    """Гладкий шум по 3D-точкам p (...,3)."""
    q = p * freq
    i = np.floor(q).astype(np.int64)
    f = q - i
    f = f * f * (3 - 2 * f)
    out = 0.0
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                w = (f[..., 0] if dx else 1 - f[..., 0]) * (f[..., 1] if dy else 1 - f[..., 1]) * (f[..., 2] if dz else 1 - f[..., 2])
                out = out + w * _hash3(i[..., 0] + dx, i[..., 1] + dy, i[..., 2] + dz, seed)
    return out


def fbm(p, freq, octaves=4, seed=0):
    s, a, tot = 0.0, 1.0, 0.0
    for o in range(octaves):
        s = s + a * value_noise(p, freq * 2 ** o, seed + o * 17)
        tot += a
        a *= 0.5
    return s / tot


def smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3 - 2 * t)
