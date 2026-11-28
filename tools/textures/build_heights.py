"""Карты высот для объёмного рельефа (параллакса) фотоматериалов.

Высота восстанавливается из карты нормалей слоя (assets/textures/surface/<слой>_nrm.jpg)
интегрированием уклонов через преобразование Фурье (Frankot–Chellappa). Текстуры бесшовные,
поэтому периодическое решение точное и шва на стыке тайлов нет.

Результат: <слой>_h.jpg, серый: 1 — самые выпуклые места (верх черепицы, лицо камня),
0 — самые глубокие (швы, щели). Игра кладёт его в альфа-канал массива цвета.

    python3 tools/textures/build_heights.py
"""
import os
import sys

import numpy as np
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "assets", "textures", "surface")

# Сколько крупных волн оставить: у кладки и черепицы форма целиком в рисунке, а у земли и
# травы длинные «холмы» на плоскости выглядят как плывущая картинка — их срезаем.
HIGHPASS = {"grass": 10, "earth": 6, "soil": 6, "gravel": 8, "rock": 3, "plaster": 6, "wood": 4, "beam": 6}


def height(nrm, highpass):
    n = nrm.astype(np.float64) / 255.0
    x = n[..., 0] * 2 - 1
    y = n[..., 1] * 2 - 1
    z = np.sqrt(np.clip(1 - x * x - y * y, 0.04, 1))
    # Нормаль в стиле OpenGL: +Y — вверх по картинке (строки растут вниз).
    gx = -x / z
    gy = y / z
    h, w = gx.shape
    fx = np.fft.fftfreq(w)[None, :] * 2 * np.pi
    fy = np.fft.fftfreq(h)[:, None] * 2 * np.pi
    d = fx * fx + fy * fy
    d[0, 0] = 1
    f = (-1j * fx * np.fft.fft2(gx) - 1j * fy * np.fft.fft2(gy)) / d
    f[0, 0] = 0
    if highpass:
        r = np.sqrt((np.fft.fftfreq(w)[None, :] * w) ** 2 + (np.fft.fftfreq(h)[:, None] * h) ** 2)
        f *= 1 - np.exp(-((r / highpass) ** 2))
    out = np.real(np.fft.ifft2(f))
    lo, hi = np.percentile(out, [1.5, 99.5])
    return np.clip((out - lo) / max(hi - lo, 1e-6), 0, 1)


def main():
    only = sys.argv[1:]
    for name in sorted(os.listdir(OUT)):
        if not name.endswith("_nrm.jpg"):
            continue
        layer = name[: -len("_nrm.jpg")]
        if only and layer not in only:
            continue
        nrm = np.asarray(Image.open(os.path.join(OUT, name)).convert("RGB"))
        # Варианты (roof_b, wood_c…) — с настройкой основного слоя.
        base = layer[:-2] if layer[-2:] in ("_b", "_c") else layer
        hm = height(nrm, HIGHPASS.get(base, 0))
        Image.fromarray((hm * 255 + 0.5).astype(np.uint8), "L").convert("RGB").save(os.path.join(OUT, layer + "_h.jpg"), quality=90)
        print("%-10s средняя высота %.2f" % (layer, hm.mean()))


if __name__ == "__main__":
    main()
