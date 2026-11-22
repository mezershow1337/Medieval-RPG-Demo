"""Процедурные текстуры для персонажей (без сторонних ассетов).

Запуск: python3 tools/models/textures.py  -> assets/models/textures/*.png
"""
import os
import numpy as np
from PIL import Image

OUT = os.path.join(os.path.dirname(__file__), "..", "..", "assets", "models", "textures")
N = 512
rng = np.random.default_rng(7)


def tileable_noise(size, cells, octaves=4, seed=0):
    """Бесшовный value-noise с несколькими октавами, 0..1."""
    r = np.random.default_rng(seed)
    out = np.zeros((size, size))
    amp, total = 1.0, 0.0
    for o in range(octaves):
        c = cells * (2 ** o)
        grid = r.random((c, c))
        xs = np.arange(size) * c / size
        i0 = np.floor(xs).astype(int)
        f = xs - i0
        f = f * f * (3 - 2 * f)
        i1 = (i0 + 1) % c
        i0 %= c
        a = grid[i0][:, i0]
        b = grid[i0][:, i1]
        cc = grid[i1][:, i0]
        d = grid[i1][:, i1]
        fx = f[None, :]
        fy = f[:, None]
        val = (a * (1 - fx) + b * fx) * (1 - fy) + (cc * (1 - fx) + d * fx) * fy
        out += val * amp
        total += amp
        amp *= 0.5
    return out / total


def scratches(size, count, seed):
    r = np.random.default_rng(seed)
    img = np.zeros((size, size))
    for _ in range(count):
        x, y = r.random(2) * size
        ang = r.random() * np.pi
        length = r.random() * size * 0.25 + 10
        steps = int(length)
        for s in range(steps):
            px = int(x + np.cos(ang) * s) % size
            py = int(y + np.sin(ang) * s) % size
            img[py, px] = max(img[py, px], 0.4 + 0.6 * r.random())
    return img


def save(name, arr):
    arr = np.clip(arr, 0, 1)
    if arr.ndim == 2:
        arr = np.stack([arr] * 3, -1)
    Image.fromarray((arr * 255).astype(np.uint8)).save(os.path.join(OUT, name + ".png"))


def normal_from_height(h, strength):
    dx = (np.roll(h, -1, 1) - np.roll(h, 1, 1)) * strength
    dy = (np.roll(h, -1, 0) - np.roll(h, 1, 0)) * strength
    n = np.stack([-dx, dy, np.ones_like(h)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    return n * 0.5 + 0.5


def metal(name, base, seed):
    n = tileable_noise(N, 4, 5, seed)
    sc = scratches(N, 140, seed + 1)
    tone = base * (0.82 + 0.3 * n)[..., None] + sc[..., None] * 0.05
    save(name + "_color", tone)
    # glTF: roughness в зелёном канале, metallic в синем.
    rough = 0.32 + 0.3 * tileable_noise(N, 8, 3, seed + 2) - sc * 0.15
    mr = np.stack([np.zeros_like(rough), rough, np.full_like(rough, 0.95)], -1)
    save(name + "_mr", mr)


def mail():
    # Кольчуга: сетка колец со смещением рядов, 16 колец на текстуру.
    rings = 16
    y, x = np.mgrid[0:N, 0:N] / N * rings
    h = np.zeros((N, N))
    for off in (0.0, 0.5):
        cx = (x + off) % 1 - 0.5
        cy = (y * 1.0 + off) % 1 - 0.5
        d = np.sqrt(cx ** 2 + cy ** 2)
        ring = np.clip(1 - np.abs(d - 0.33) / 0.11, 0, 1)
        h = np.maximum(h, ring)
    n = tileable_noise(N, 8, 3, 11)
    col = np.array([0.55, 0.56, 0.58]) * (0.35 + 0.75 * h)[..., None] * (0.85 + 0.3 * n)[..., None]
    save("mail_color", col)
    rough = 0.35 + (1 - h) * 0.5
    save("mail_mr", np.stack([np.zeros_like(h), rough, 0.25 + 0.7 * h], -1))
    save("mail_normal", normal_from_height(h, 6.0))


def quilted():
    # Стёганка: ромбовидная стёжка + переплетение ткани. Цвет почти белый — оттенок задаёт материал.
    y, x = np.mgrid[0:N, 0:N] / N
    lines = 6
    u = (x + y) * lines % 1
    v = (x - y) * lines % 1
    seam = np.minimum(np.minimum(u, 1 - u), np.minimum(v, 1 - v))
    puff = np.clip(seam * 6, 0, 1) ** 0.5
    weave = (np.sin(x * N * np.pi * 0.9) * np.sin(y * N * np.pi * 0.9)) * 0.5 + 0.5
    n = tileable_noise(N, 6, 4, 21)
    tone = (0.72 + 0.25 * puff) * (0.9 + 0.1 * weave) * (0.85 + 0.25 * n)
    save("quilted_color", tone)
    save("quilted_normal", normal_from_height(puff * 0.8 + weave * 0.05, 5.0))


def cloth():
    y, x = np.mgrid[0:N, 0:N] / N
    weave = (np.sin(x * N * np.pi * 0.7) * 0.5 + 0.5) * 0.5 + (np.sin(y * N * np.pi * 0.7) * 0.5 + 0.5) * 0.5
    n = tileable_noise(N, 5, 5, 31)
    dirt = tileable_noise(N, 3, 4, 32)
    tone = (0.78 + 0.12 * weave) * (0.8 + 0.3 * n) * (1 - 0.25 * np.clip(dirt * 2 - 1, 0, 1))
    save("cloth_color", tone)
    save("cloth_normal", normal_from_height(weave, 1.5))


def leather():
    n = tileable_noise(N, 10, 5, 41)
    big = tileable_noise(N, 3, 3, 42)
    cr = scratches(N, 120, 43)
    base = np.array([0.36, 0.22, 0.12])
    tone = base * (0.7 + 0.5 * n * big)[..., None] * (1 - 0.3 * cr)[..., None]
    save("leather_color", tone)
    save("leather_normal", normal_from_height(n * 0.6 + cr * 0.4, 3.0))


def skin():
    n = tileable_noise(N, 12, 4, 51)
    base = np.array([0.72, 0.52, 0.42])
    save("skin_color", base * (0.92 + 0.12 * n)[..., None])


def wood():
    y, x = np.mgrid[0:N, 0:N] / N
    n = tileable_noise(N, 4, 4, 61)
    grain = np.sin((y * 40 + n * 6) * np.pi) * 0.5 + 0.5
    base = np.array([0.42, 0.28, 0.16])
    save("wood_color", base * (0.7 + 0.35 * grain)[..., None])


def heraldry():
    # Лицевая сторона щита: поле + шеврон. Красит материал (белый -> цвет рода).
    y, x = np.mgrid[0:N, 0:N] / N
    xc = np.abs(x - 0.5)
    chevron = (np.abs((1 - y) - (0.62 - xc * 0.9)) < 0.09)
    n = tileable_noise(N, 6, 5, 71)
    worn = tileable_noise(N, 10, 3, 72)
    field = np.array([0.12, 0.2, 0.48])
    gold = np.array([0.85, 0.68, 0.25])
    col = np.where(chevron[..., None], gold, field) * (0.8 + 0.3 * n)[..., None]
    chips = (worn > 0.8)[..., None]
    col = np.where(chips, np.array([0.45, 0.33, 0.2]) * (0.8 + 0.3 * n)[..., None], col)
    save("heraldry_color", col)


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    metal("steel", np.array([0.78, 0.79, 0.8]), 1)
    metal("darksteel", np.array([0.28, 0.27, 0.27]), 3)
    mail()
    quilted()
    cloth()
    leather()
    skin()
    wood()
    heraldry()
    print("ok")
