"""Выравнивает яркость и цвет импостеров под атлас листвы.

Импостер рендерится в Cycles с освещением, а в игре на него ещё раз падает солнце, поэтому
"сырой" рендер получается вдвое темнее живой кроны и издали деревья выглядят чёрными кляксами.
Здесь средний цвет непрозрачных пикселей приводится к ~85 % среднего цвета атласа (оставшиеся
15 % — самозатенение кроны), с сохранением светотени внутри картинки.

Глубокие тени рендера при этом поднимаются: в Cycles тень внутри кроны почти чёрная, а в игре
на неё ложится ещё и своя тень — дальние можжевельники и кусты выходили чёрными кляксами.
Тёмная половина картинки сжимается к середине так, чтобы десятая доля самых тёмных пикселей
была не темнее 0,7 медианы (повторный запуск ничего не меняет).

Запуск: python3 tools/vegetation/fix_impostors.py assets/textures/vegetation
"""
import glob
import os
import sys

import numpy as np
from PIL import Image

ATLAS_OF = {"pine": "pine", "oak": "broadleaf", "fig": "broadleaf", "shrub": "broadleaf", "juniper": "scale", "cypress": "scale", "fir": "bough"}
TARGET = 0.85
# У хвойных лап цвет вершин затеняет крону (в среднем ~0.7) — импостер темнее на столько же.
TARGET_OF = {"fir": 0.6}


def mean_rgb(a):
    m = a[..., 3] > 128
    return a[m][:, :3].mean(0)


def lift_shadows(a, tone):
    """Сжимает тени к медиане: 10-й перцентиль яркости — не ниже 0,7 медианы."""
    m = a[..., 3] > 128
    lum = a[..., :3] @ np.array([0.2126, 0.7152, 0.0722], np.float32)
    med, p10 = np.percentile(lum[m], [50, 10])
    k = min(1.0, 0.3 * med / max(med - p10, 1e-3))
    if k >= 0.999:
        return
    new = np.where(lum < med, med + (lum - med) * k, lum)
    # Совсем чёрным пикселям нечего масштабировать — им даём цвет атласа.
    hue = tone / max(float(tone @ np.array([0.2126, 0.7152, 0.0722], np.float32)), 1.0)
    rgb = np.where((lum > 3.0)[..., None], a[..., :3] * (new / np.maximum(lum, 1e-3))[..., None], hue * new[..., None])
    a[..., :3] = np.where(m[..., None], np.clip(rgb, 0, 255), a[..., :3])


def main(d):
    atlases = {n: mean_rgb(np.asarray(Image.open(os.path.join(d, f"{n}_atlas.png")).convert("RGBA")).astype(np.float32)) for n in set(ATLAS_OF.values())}
    for f in sorted(glob.glob(os.path.join(d, "*_imp.png"))):
        species = os.path.basename(f).split("_")[0]
        a = np.asarray(Image.open(f).convert("RGBA")).astype(np.float32)
        lift_shadows(a, atlases[ATLAS_OF[species]])
        gain = TARGET_OF.get(species, TARGET) * atlases[ATLAS_OF[species]] / np.maximum(mean_rgb(a), 1.0)
        a[..., :3] = np.clip(a[..., :3] * gain, 0, 255)
        Image.fromarray(a.astype(np.uint8), "RGBA").save(f, optimize=True)
        print(os.path.basename(f), "gain", gain.round(2), "->", mean_rgb(a).round(1))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "assets/textures/vegetation")
