"""Собирает атласы листвы 2×2 из спрайтов, отрендеренных в Blender (blender_vegetation.py).

    python3 tools/vegetation/build_atlases.py <папка со спрайтами> <папка для атласов>

Прозрачные пиксели заливаются цветом ближайшего листа (dilation), чтобы на мип-уровнях
края листвы не темнели.
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage

ATLASES = {
    "broadleaf": ["broadleaf_spr_0", "broadleaf_spr_2", "broadleaf_spr_3", "broadleaf_spr_5"],
    "pine": ["pine_0", "pine_1", "pine_2", "pine_3"],
    "scale": ["scale_0", "scale_1", "scale_2", "scale_3"],
}
CELL = 512


def dilate(rgba: np.ndarray) -> np.ndarray:
    a = rgba[..., 3]
    solid = a > 32
    _, (iy, ix) = ndimage.distance_transform_edt(~solid, return_indices=True)
    out = rgba.copy()
    out[..., :3] = rgba[iy, ix, :3]
    return out


def main(src: Path, dst: Path) -> None:
    dst.mkdir(parents=True, exist_ok=True)
    for name, cells in ATLASES.items():
        atlas = np.zeros((CELL * 2, CELL * 2, 4), np.uint8)
        for k, cell in enumerate(cells):
            im = Image.open(src / f"{cell}.png").convert("RGBA").resize((CELL, CELL), Image.LANCZOS)
            y, x = divmod(k, 2)
            atlas[y * CELL:(y + 1) * CELL, x * CELL:(x + 1) * CELL] = np.asarray(im)
        atlas = dilate(atlas)
        Image.fromarray(atlas).save(dst / f"{name}_atlas.png", optimize=True)
        print(name, (atlas[..., 3] > 128).mean())


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
