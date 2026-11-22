"""Минимальный писатель glTF 2.0 (.glb): меши со скином, материалы PBR с текстурами PNG.

Координаты — как в игре (Y вверх). Blender для экспорта не нужен: так скелет и обратные
матрицы привязки получаются ровно такими, как их ждёт src/rig.rs.
"""

import io
import json
import struct

import numpy as np
from PIL import Image


def vertex_normals(v, tris):
    n = np.zeros_like(v)
    fn = np.cross(v[tris[:, 1]] - v[tris[:, 0]], v[tris[:, 2]] - v[tris[:, 0]])
    for k in range(3):
        np.add.at(n, tris[:, k], fn)
    ln = np.linalg.norm(n, axis=1, keepdims=True)
    return n / np.maximum(ln, 1e-12)


def tangents(v, n, uv, tris):
    """Касательные по UV (как MikkTSpace в первом приближении), w — знак битангенса."""
    t = np.zeros_like(v)
    b = np.zeros_like(v)
    p0, p1, p2 = v[tris[:, 0]], v[tris[:, 1]], v[tris[:, 2]]
    u0, u1, u2 = uv[tris[:, 0]], uv[tris[:, 1]], uv[tris[:, 2]]
    e1, e2 = p1 - p0, p2 - p0
    d1, d2 = u1 - u0, u2 - u0
    r = d1[:, 0] * d2[:, 1] - d2[:, 0] * d1[:, 1]
    r = np.where(np.abs(r) < 1e-12, 1e-12, r)
    sdir = (e1 * d2[:, 1:2] - e2 * d1[:, 1:2]) / r[:, None]
    tdir = (e2 * d1[:, 0:1] - e1 * d2[:, 0:1]) / r[:, None]
    for k in range(3):
        np.add.at(t, tris[:, k], sdir)
        np.add.at(b, tris[:, k], tdir)
    t = t - n * np.sum(n * t, axis=1, keepdims=True)
    ln = np.linalg.norm(t, axis=1, keepdims=True)
    fallback = np.cross(n, np.array([0.0, 1.0, 0.0]))
    t = np.where(ln > 1e-9, t / np.maximum(ln, 1e-12), fallback)
    t /= np.maximum(np.linalg.norm(t, axis=1, keepdims=True), 1e-9)
    w = np.where(np.sum(np.cross(n, t) * b, axis=1) < 0.0, -1.0, 1.0)
    # glTF: UV с началом вверху — переворачиваем знак, т.к. v мы пишем как 1 - v.
    return np.c_[t, -w]


class Glb:
    def __init__(self):
        self.bin = bytearray()
        self.j = {"asset": {"version": "2.0", "generator": "medieval-rpg tools/models"}, "scenes": [{"nodes": []}], "scene": 0,
                  "nodes": [], "meshes": [], "materials": [], "accessors": [], "bufferViews": [], "buffers": [],
                  "images": [], "textures": [], "samplers": [{"magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497}]}
        self.images = {}

    def _view(self, data, target=None):
        while len(self.bin) % 4:
            self.bin.append(0)
        off = len(self.bin)
        self.bin += data
        bv = {"buffer": 0, "byteOffset": off, "byteLength": len(data)}
        if target:
            bv["target"] = target
        self.j["bufferViews"].append(bv)
        return len(self.j["bufferViews"]) - 1

    def accessor(self, arr, ctype, typ, target=None, minmax=False):
        arr = np.ascontiguousarray(arr)
        view = self._view(arr.tobytes(), target)
        acc = {"bufferView": view, "componentType": ctype, "count": int(arr.shape[0]), "type": typ}
        if minmax:
            acc["min"] = arr.min(axis=0).tolist()
            acc["max"] = arr.max(axis=0).tolist()
        self.j["accessors"].append(acc)
        return len(self.j["accessors"]) - 1

    def image(self, key, img):
        """img — PIL.Image или путь к PNG. Одинаковые ключи используют одну текстуру."""
        if key in self.images:
            return self.images[key]
        if isinstance(img, str):
            data = open(img, "rb").read()
            mime = "image/png"
        else:
            buf = io.BytesIO()
            if img.mode == "RGBA":
                img.save(buf, "PNG", optimize=True)
                mime = "image/png"
            else:
                # Шумные процедурные текстуры в PNG весят слишком много — пишем JPEG.
                img.save(buf, "JPEG", quality=88, optimize=True)
                mime = "image/jpeg"
            data = buf.getvalue()
        view = self._view(data)
        self.j["images"].append({"bufferView": view, "mimeType": mime, "name": key})
        self.j["textures"].append({"sampler": 0, "source": len(self.j["images"]) - 1})
        self.images[key] = len(self.j["textures"]) - 1
        return self.images[key]

    def material(self, name, color=(1, 1, 1, 1), base=None, mr=None, normal=None, metallic=0.0, roughness=1.0, alpha_cutoff=None, double=False, normal_scale=1.0):
        m = {"name": name, "pbrMetallicRoughness": {"baseColorFactor": list(color), "metallicFactor": metallic, "roughnessFactor": roughness}}
        if base is not None:
            m["pbrMetallicRoughness"]["baseColorTexture"] = {"index": self.image(name + ".base", base) if not isinstance(base, int) else base}
        if mr is not None:
            m["pbrMetallicRoughness"]["metallicRoughnessTexture"] = {"index": self.image(name + ".mr", mr) if not isinstance(mr, int) else mr}
        if normal is not None:
            m["normalTexture"] = {"index": self.image(name + ".normal", normal) if not isinstance(normal, int) else normal, "scale": normal_scale}
        if alpha_cutoff is not None:
            m["alphaMode"] = "MASK"
            m["alphaCutoff"] = alpha_cutoff
        if double:
            m["doubleSided"] = True
        self.j["materials"].append(m)
        return len(self.j["materials"]) - 1

    def mesh(self, name, prims, skin=None):
        """prims: список dict(v, n, uv, tris, material, joints?, weights?, tangents?)."""
        out = []
        for p in prims:
            attrs = {
                "POSITION": self.accessor(p["v"].astype(np.float32), 5126, "VEC3", 34962, True),
                "NORMAL": self.accessor(p["n"].astype(np.float32), 5126, "VEC3", 34962),
            }
            if p.get("uv") is not None:
                uv = p["uv"].astype(np.float32).copy()
                uv[:, 1] = 1.0 - uv[:, 1]
                attrs["TEXCOORD_0"] = self.accessor(uv, 5126, "VEC2", 34962)
                tg = tangents(p["v"], p["n"], p["uv"], p["tris"])
                attrs["TANGENT"] = self.accessor(tg.astype(np.float32), 5126, "VEC4", 34962)
            if p.get("joints") is not None:
                attrs["JOINTS_0"] = self.accessor(p["joints"].astype(np.uint16), 5123, "VEC4", 34962)
                attrs["WEIGHTS_0"] = self.accessor(p["weights"].astype(np.float32), 5126, "VEC4", 34962)
            idx = self.accessor(p["tris"].astype(np.uint32).ravel(), 5125, "SCALAR", 34963)
            out.append({"attributes": attrs, "indices": idx, "material": p["material"]})
        self.j["meshes"].append({"name": name, "primitives": out})
        node = {"name": name, "mesh": len(self.j["meshes"]) - 1}
        if skin is not None:
            node["skin"] = skin
        self.j["nodes"].append(node)
        self.j["scenes"][0]["nodes"].append(len(self.j["nodes"]) - 1)

    def skeleton(self, names, parents, offsets, inverse_binds):
        """Узлы костей (локальные смещения) и скин с обратными матрицами привязки."""
        base = len(self.j["nodes"])
        for i, n in enumerate(names):
            self.j["nodes"].append({"name": n, "translation": [float(x) for x in offsets[i]]})
        for i, p in enumerate(parents):
            if p is None:
                self.j["scenes"][0]["nodes"].append(base + i)
            else:
                self.j["nodes"][base + p].setdefault("children", []).append(base + i)
        ibm = np.array([m.T.ravel() for m in inverse_binds], dtype=np.float32)  # столбцами
        acc = self.accessor(ibm, 5126, "MAT4")
        self.j.setdefault("skins", []).append({"name": "human", "joints": [base + i for i in range(len(names))], "inverseBindMatrices": acc, "skeleton": base})
        return len(self.j["skins"]) - 1

    def save(self, path):
        while len(self.bin) % 4:
            self.bin.append(0)
        self.j["buffers"] = [{"byteLength": len(self.bin)}]
        js = json.dumps(self.j, separators=(",", ":")).encode()
        while len(js) % 4:
            js += b" "
        total = 12 + 8 + len(js) + 8 + len(self.bin)
        with open(path, "wb") as f:
            f.write(struct.pack("<III", 0x46546C67, 2, total))
            f.write(struct.pack("<II", len(js), 0x4E4F534A))
            f.write(js)
            f.write(struct.pack("<II", len(self.bin), 0x004E4942))
            f.write(self.bin)


def to_image(arr, mode="RGB"):
    a = np.clip(arr, 0.0, 1.0)
    return Image.fromarray((a * 255 + 0.5).astype(np.uint8), mode)
