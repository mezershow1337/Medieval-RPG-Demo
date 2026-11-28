"""Приглушить жёлто-салатовую листву до спокойной зелени: атлас листьев и снимки-импосторы
лиственных пород (дуб, инжир, кусты). Запускать один раз после build_atlases.py и рендера
импосторов — повторный запуск приглушит ещё раз.

    python3 tools/vegetation/retone_broadleaf.py assets/textures/vegetation
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image

# Насколько оставить насыщенность и множители каналов (в линейном свете).
KEEP = 0.72
GAIN = np.array([0.62, 0.82, 1.0], np.float32)


def retone(path):
    im = np.asarray(Image.open(path).convert("RGBA"), np.float32) / 255.0
    lin = im[..., :3] ** 2.2
    luma = (lin * np.array([0.2126, 0.7152, 0.0722], np.float32)).sum(-1, keepdims=True)
    lin = (luma + (lin - luma) * KEEP) * GAIN
    im[..., :3] = np.clip(lin, 0, 1) ** (1 / 2.2)
    Image.fromarray((im * 255 + 0.5).astype(np.uint8), "RGBA").save(path, optimize=True)
    solid = im[..., 3] > 0.5
    print(path.name, (im[solid][:, :3].mean(0) * 255).round())


if __name__ == "__main__":
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "assets/textures/vegetation")
    for name in ["broadleaf_atlas.png"] + [f"{k}_{i}_imp.png" for k in ("oak", "fig", "shrub") for i in range(3)]:
        if (root / name).exists():
            retone(root / name)
