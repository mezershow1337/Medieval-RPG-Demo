"""Тон камней-сканов под местный серый известняк.

    python3 tools/vegetation/tone_rocks.py assets/models/rocks

Сканы Poly Haven бурые и рыжие (каждый своего тона, самый тёмный вдвое темнее светлого) — на
сером обрыве глыбы лежали тёмно-бурыми «кирпичами». Скрипт правит картинку цвета внутри glb:
почти снимает окраску (рисунок трещин и пятен остаётся) и приводит среднюю яркость всех сканов
к одной, чуть теплее серого; заодно убирает цвета вершин, оставшиеся в одном из сканов.
Запускать один раз на свежем выводе blender_rocks.py (повторный
запуск ещё раз убавит окраску).
"""
import io
import json
import struct
import sys

import numpy as np
from PIL import Image

ROCKS = ["rock_07", "rock_09", "stone_01", "namaqualand_boulder_02", "rock_face_01"]
# Средний цвет (sRGB) и доля родной окраски скана, которая остаётся.
TARGET = np.array([118.0, 114.0, 105.0])
KEEP = 0.25


def tone(raw):
    a = np.asarray(Image.open(io.BytesIO(raw)).convert("RGB")).astype(np.float32)
    lum = a @ np.array([0.3, 0.59, 0.11], dtype=np.float32)
    grey = lum[..., None] * (TARGET / TARGET.mean())
    out = grey * (1.0 - KEEP) + a * KEEP
    out *= TARGET.mean() / out.mean()
    buf = io.BytesIO()
    Image.fromarray(np.clip(out, 0, 255).astype(np.uint8)).save(buf, "JPEG", quality=92)
    return buf.getvalue()


def process(path):
    d = open(path, "rb").read()
    jl = struct.unpack("<I", d[12:16])[0]
    j = json.loads(d[20:20 + jl])
    b0 = 20 + jl + 8
    views = [d[b0 + v.get("byteOffset", 0):b0 + v.get("byteOffset", 0) + v["byteLength"]] for v in j["bufferViews"]]
    # У rock_07 в скане остались цвета вершин (красные и синие пятна) — они перекрашивали камень.
    for mesh in j["meshes"]:
        for prim in mesh["primitives"]:
            prim["attributes"].pop("COLOR_0", None)
    for m in j["materials"]:
        img = j["images"][j["textures"][m["pbrMetallicRoughness"]["baseColorTexture"]["index"]]["source"]]
        views[img["bufferView"]] = tone(views[img["bufferView"]])
        img["mimeType"] = "image/jpeg"
    blob = bytearray()
    for v, data in zip(j["bufferViews"], views):
        blob += b"\0" * (-len(blob) % 4)
        v["byteOffset"] = len(blob)
        v["byteLength"] = len(data)
        blob += data
    blob += b"\0" * (-len(blob) % 4)
    j["buffers"][0]["byteLength"] = len(blob)
    js = json.dumps(j, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    out = struct.pack("<4sII", b"glTF", 2, 12 + 8 + len(js) + 8 + len(blob))
    out += struct.pack("<I4s", len(js), b"JSON") + js + struct.pack("<I4s", len(blob), b"BIN\0") + bytes(blob)
    open(path, "wb").write(out)


if __name__ == "__main__":
    for r in ROCKS:
        process(f"{sys.argv[1]}/{r}.glb")
        print("готово:", r)
