"""Размеры 3D-моделей предметов для физики: src/icons/shapes.rs.

Читает заголовок каждого assets/models/items/<ключ>.glb (Blender не нужен) и записывает
половинные размеры коробки, в которую вписана модель. По этой коробке сервер считает
столкновения брошенного или перетаскиваемого предмета, а клиент — наведение прицела.

Запуск: python3 tools/models/item_bounds.py
(build_items.py вызывает его сам после обновления src/icons/art.rs).
"""
import glob
import json
import os
import struct

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))


def bounds(path):
    data = open(path, "rb").read()
    length = struct.unpack("<I", data[12:16])[0]
    gltf = json.loads(data[20:20 + length])
    lo = [1e9] * 3
    hi = [-1e9] * 3
    for mesh in gltf.get("meshes", []):
        for prim in mesh["primitives"]:
            acc = gltf["accessors"][prim["attributes"]["POSITION"]]
            lo = [min(a, b) for a, b in zip(lo, acc["min"])]
            hi = [max(a, b) for a, b in zip(hi, acc["max"])]
    return lo, hi


def main():
    rows = []
    for path in sorted(glob.glob(os.path.join(ROOT, "assets", "models", "items", "*.glb"))):
        key = os.path.splitext(os.path.basename(path))[0]
        lo, hi = bounds(path)
        half = [(hi[i] - lo[i]) / 2 for i in range(3)]
        rows.append(f'    ("{key}", [{half[0]:.4f}, {half[1]:.4f}, {half[2]:.4f}]),')
    out = os.path.join(ROOT, "src", "icons", "shapes.rs")
    with open(out, "w", encoding="utf-8") as f:
        f.write("//! Создаётся tools/models/item_bounds.py — руками не править.\n\n")
        f.write("/// Ключ 3D-модели -> половинные размеры коробки модели (м). Модель стоит на y = 0 и отцентрована по X/Z.\n")
        f.write("pub const ITEM_SHAPES: &[(&str, [f32; 3])] = &[\n" + "\n".join(rows) + "\n];\n")
    print(f"{len(rows)} моделей -> {out}")


if __name__ == "__main__":
    main()
