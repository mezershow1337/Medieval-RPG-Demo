"""Листва лиственных пород гуще и темнее, веточки — тёмная кора вместо белёсой.

После retone_broadleaf.py атлас вышел бледным: листья серо-салатовые, а веточки между ними
почти белые — на мип-уровнях они смешивались с листьями, и дуб издали стоял выцветшим пятном.
Здесь веточки (светлые и почти бесцветные пикселы) перекрашиваются в тёмную кору, листьям
добавляются глубина тени и насыщенность. Запускать один раз: повторный запуск затемнит ещё.

    python3 tools/vegetation/deepen_broadleaf.py assets/textures/vegetation
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image

BARK = np.array([0.045, 0.034, 0.024], np.float32)  # линейный цвет коры веточек
LUMA = np.array([0.2126, 0.7152, 0.0722], np.float32)


def deepen(path, twigs):
    im = np.asarray(Image.open(path).convert("RGBA"), np.float32) / 255.0
    rgb = im[..., :3]
    mx, mn = rgb.max(-1), rgb.min(-1)
    sat = (mx - mn) / np.maximum(mx, 1e-3)
    lin = rgb ** 2.2
    luma = (lin * LUMA).sum(-1, keepdims=True)
    solid = im[..., 3] > 0.5
    mid = float(luma[solid].mean()) if solid.any() else 0.08
    # Листья: тени глубже (контраст вокруг среднего), зелень насыщеннее и чуть теплее.
    k = np.clip(luma / mid, 0.05, 4.0) ** 0.4 * 0.62
    leaf = (luma + (lin - luma) * 1.35) * k * np.array([1.0, 1.0, 0.82], np.float32)
    out = np.clip(leaf, 0, 1)
    if twigs:
        t = np.clip((0.3 - sat) / 0.12, 0, 1) * np.clip((mx - 0.4) / 0.15, 0, 1)
        twig = BARK * (0.7 + 0.8 * mx[..., None])
        out = out * (1 - t[..., None]) + twig * t[..., None]
    im[..., :3] = out ** (1 / 2.2)
    Image.fromarray((im * 255 + 0.5).astype(np.uint8), "RGBA").save(path, optimize=True)
    print(path.name, (im[solid][:, :3].mean(0) * 255).round())


if __name__ == "__main__":
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "assets/textures/vegetation")
    deepen(root / "broadleaf_atlas.png", True)
    for name in [f"{k}_{i}_imp.png" for k in ("oak", "fig", "shrub") for i in range(3)]:
        if (root / name).exists():
            # На снимке-импосторе веточки уже слились с листвой: только глубина и насыщенность.
            deepen(root / name, False)
