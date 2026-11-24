"""Где на моделях оружия проходит рукоять/топорище: src/icons/grips.rs.

Коробка модели (shapes.rs) отцентрована по X, а у топора топорище не в центре коробки —
сбоку от него торчит лезвие. Здесь по вершинам задней четверти модели (у торца, +Z), где
только рукоять, находится её ось, и записывается её сдвиг от центра коробки по X и Y.
Рука в игре (worn.rs) обхватывает именно эту ось. Пишутся только заметные сдвиги (> 4 мм).

Запуск: python3 tools/models/item_grips.py (Blender не нужен).
"""
import glob
import json
import os
import struct

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))


def positions(path):
    data = open(path, "rb").read()
    length = struct.unpack("<I", data[12:16])[0]
    gltf = json.loads(data[20:20 + length])
    bin_at = 20 + length
    bin_len = struct.unpack("<I", data[bin_at:bin_at + 4])[0]
    blob = data[bin_at + 8:bin_at + 8 + bin_len]
    out = []
    for mesh in gltf.get("meshes", []):
        for prim in mesh["primitives"]:
            acc = gltf["accessors"][prim["attributes"]["POSITION"]]
            view = gltf["bufferViews"][acc["bufferView"]]
            stride = view.get("byteStride", 12)
            start = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
            for i in range(acc["count"]):
                out.append(struct.unpack_from("<3f", blob, start + i * stride))
    return out


def main():
    rows = []
    for path in sorted(glob.glob(os.path.join(ROOT, "assets", "models", "items", "*.glb"))):
        key = os.path.splitext(os.path.basename(path))[0]
        pts = positions(path)
        if not pts:
            continue
        lo = [min(p[i] for p in pts) for i in range(3)]
        hi = [max(p[i] for p in pts) for i in range(3)]
        length = hi[2] - lo[2]
        # Длинные предметы: задняя четверть у торца — только рукоять.
        if length < 0.2:
            continue
        tail = [p for p in pts if p[2] > hi[2] - 0.25 * length]
        cx = (min(p[0] for p in tail) + max(p[0] for p in tail)) / 2 - (lo[0] + hi[0]) / 2
        cy = (min(p[1] for p in tail) + max(p[1] for p in tail)) / 2 - (lo[1] + hi[1]) / 2
        if abs(cx) > 0.004 or abs(cy) > 0.004:
            rows.append(f'    ("{key}", [{cx:.4f}, {cy:.4f}]),')
    out = os.path.join(ROOT, "src", "icons", "grips.rs")
    with open(out, "w", encoding="utf-8") as f:
        f.write("//! Создаётся tools/models/item_grips.py — руками не править.\n\n")
        f.write("/// Ключ 3D-модели -> сдвиг оси рукояти от центра коробки модели по X и Y (м).\n")
        f.write("pub const ITEM_GRIPS: &[(&str, [f32; 2])] = &[\n" + "\n".join(rows) + "\n];\n")
    print(f"{len(rows)} моделей со сдвигом рукояти -> {out}")


if __name__ == "__main__":
    main()
