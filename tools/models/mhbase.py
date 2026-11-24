"""Основа тела из MakeHuman (CC0): меш, скелет, веса.

Данные MakeHuman 1.1 (base.obj, default.mhskel, default_weights.mhw и цели формы) выпущены под CC0,
см. https://github.com/makehumancommunity/makehuman (LICENSE.md, раздел C). Скрипт сам скачивает
их в кэш (~/.cache/mrpg-makehuman) при первом запуске.

Координаты на выходе — как в игре (Bevy): метры, Y вверх, лицо смотрит в -Z, правая рука в +X.
"""

import json
import os
import urllib.request

import numpy as np

BASE_URL = "https://raw.githubusercontent.com/makehumancommunity/makehuman/master/makehuman/"
FILES = [
    "data/3dobjs/base.obj",
    "data/rigs/default.mhskel",
    "data/rigs/default_weights.mhw",
    "data/targets/macrodetails/universal-male-young-maxmuscle-averageweight.target",
    "data/targets/macrodetails/proportions/male-young-averagemuscle-averageweight-idealproportions.target",
    "data/targets/macrodetails/caucasian-male-young.target",
    "data/targets/macrodetails/asian-male-young.target",
    "data/targets/macrodetails/african-male-young.target",
]
CACHE = os.environ.get("MH_CACHE", os.path.expanduser("~/.cache/mrpg-makehuman"))


def fetch(rel):
    path = os.path.join(CACHE, rel)
    if not os.path.exists(path):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        print("скачиваю", rel)
        with urllib.request.urlopen(BASE_URL + rel) as r:
            data = r.read()
        if data.startswith(b"404"):
            raise RuntimeError(f"не найден {BASE_URL + rel}")
        with open(path, "wb") as f:
            f.write(data)
    return path


def load_obj(path):
    verts, uvs, faces, fuvs, fgroups = [], [], [], [], []
    group = None
    groups = {}
    for line in open(path):
        if line.startswith("v "):
            verts.append([float(x) for x in line.split()[1:4]])
        elif line.startswith("vt "):
            uvs.append([float(x) for x in line.split()[1:3]])
        elif line.startswith("g "):
            group = line.split()[1]
            groups.setdefault(group, len(groups))
        elif line.startswith("f "):
            vs, ts = [], []
            for tok in line.split()[1:]:
                p = tok.split("/")
                vs.append(int(p[0]) - 1)
                ts.append(int(p[1]) - 1 if len(p) > 1 and p[1] else -1)
            faces.append(vs)
            fuvs.append(ts)
            fgroups.append(group)
    return np.array(verts), np.array(uvs), faces, fuvs, fgroups


def load_target(path):
    idx, d = [], []
    for line in open(path):
        if line.startswith("#") or not line.strip():
            continue
        p = line.split()
        idx.append(int(p[0]))
        d.append([float(x) for x in p[1:4]])
    return np.array(idx, dtype=int), np.array(d)


class Base:
    """Меш MakeHuman после применения целей формы, в координатах игры."""

    def __init__(self, muscle=0.5, proportions=1.0, stature=1.75, ethnic=(0.85, 0.1, 0.05)):
        v, self.uv, self.faces, self.fuvs, self.fgroups = load_obj(fetch(FILES[0]))
        mix = [(FILES[3], muscle), (FILES[4], proportions)] + list(zip(FILES[5:8], ethnic))
        for rel, w in mix:
            i, d = load_target(fetch(rel))
            v[i] += d * w
        self.skel = json.load(open(fetch(FILES[1])))
        self.mh_weights = json.load(open(fetch(FILES[2])))["weights"]
        # MakeHuman: дециметры, Y вверх, лицо +Z, левая рука +X  ->  игра: метры, лицо -Z, правая +X.
        v = np.stack([-v[:, 0], v[:, 1], -v[:, 2]], axis=1) * 0.1
        body = self.group_verts("body", v)
        ground = v[body, 1].min()
        top = v[body, 1].max()
        v[:, 1] -= ground
        self.scale = stature / (top - ground)
        v *= self.scale
        self.v = v

    def target_delta(self, rel, w=1.0):
        """Сдвиги вершин цели формы MakeHuman (в координатах и масштабе игры), умноженные на w."""
        i, d = load_target(fetch(rel))
        out = np.zeros_like(self.v)
        out[i] += np.stack([-d[:, 0], d[:, 1], -d[:, 2]], axis=1) * 0.1 * self.scale * w
        return out

    def group_verts(self, name, v=None):
        s = set()
        for f, g in zip(self.faces, self.fgroups):
            if g == name:
                s.update(f)
        return np.array(sorted(s), dtype=int)

    def joint(self, name):
        """Положение сустава скелета MakeHuman (среднее его вершин)."""
        return self.v[self.skel["joints"][name]].mean(axis=0)

    def bone(self, name):
        b = self.skel["bones"][name]
        return self.joint(b["head"]), self.joint(b["tail"]), b.get("parent")
