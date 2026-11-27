"""Выравнивает яркость и цвет импостеров под атлас листвы.

Импостер рендерится в Cycles с освещением, а в игре на него ещё раз падает солнце, поэтому
"сырой" рендер получается вдвое темнее живой кроны и издали деревья выглядят чёрными кляксами.
Здесь средний цвет непрозрачных пикселей приводится к ~85 % среднего цвета атласа (оставшиеся
15 % — самозатенение кроны), с сохранением светотени внутри картинки.

Запуск: python3 tools/vegetation/fix_impostors.py assets/textures/vegetation
"""
import glob
import os
import sys

import numpy as np
from PIL import Image

ATLAS_OF = {"fir": "pine", "pine": "pine", "oak": "broadleaf", "fig": "broadleaf", "shrub": "broadleaf", "juniper": "scale", "cypress": "scale"}
TARGET = 0.85


def mean_rgb(a):
    m = a[..., 3] > 128
    return a[m][:, :3].mean(0)


def main(d):
    atlases = {n: mean_rgb(np.asarray(Image.open(os.path.join(d, f"{n}_atlas.png")).convert("RGBA")).astype(np.float32)) for n in set(ATLAS_OF.values())}
    for f in sorted(glob.glob(os.path.join(d, "*_imp.png"))):
        species = os.path.basename(f).split("_")[0]
        a = np.asarray(Image.open(f).convert("RGBA")).astype(np.float32)
        gain = TARGET * atlases[ATLAS_OF[species]] / np.maximum(mean_rgb(a), 1.0)
        a[..., :3] = np.clip(a[..., :3] * gain, 0, 255)
        Image.fromarray(a.astype(np.uint8), "RGBA").save(f, optimize=True)
        print(os.path.basename(f), "gain", gain.round(2), "->", mean_rgb(a).round(1))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "assets/textures/vegetation")
