#!/usr/bin/env python3
"""Фотоматериалы уровня: камень, дерево, земля, черепица, ткань, металл.

Источники — ambientCG (https://ambientcg.com) и Poly Haven (https://polyhaven.com, id в нижнем
регистре), все материалы там под лицензией CC0 (общественное достояние); слой roof — свой
(Blender). Скрипт:
  1. берёт (или скачивает) архивы <Id>_1K-JPG.zip, либо карты <id>_Color/NormalGL/Roughness/
     AmbientOcclusion.(jpg|png) из папки;
  2. для каждого слоя собирает две картинки 1024×1024:
       <layer>_albedo.jpg — цвет × мягкая AO (sRGB);
       <layer>_nrm.jpg    — R,G = нормаль (OpenGL, +Y вверх), B = шероховатость;
  3. пишет assets/textures/surface/*.jpg и src/surface_layers.rs
     (номера слоёв, средний линейный цвет слоя — по нему шейдер перекрашивает фото
     в цвет материала, и масштаб тайла в метрах).

Запуск:  python3 tools/textures/build_surface_textures.py [папка_с_zip_или_jpg]
Без папки архивы качаются с ambientCG в ~/.cache/mrpg-ambientcg.
Нужны только Pillow и numpy.
"""
import io, os, sys, zipfile, urllib.request
import numpy as np
from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "assets", "textures", "surface")
RS = os.path.join(ROOT, "src", "surface_layers.rs")
SIZE = 1024

# (слой, id ambientCG, метров на тайл, что это)
LAYERS = [
    ("ashlar", "large_sandstone_blocks_01", 3.0, "крупные тёсаные блоки известняка, выветренные, с лишайником (фотоскан Poly Haven)"),
    ("rubble", "medieval_blocks_05", 2.0, "бутовая кладка из рваного известняка на растворе (фотоскан Poly Haven)"),
    ("plaster", "worn_mossy_plasterwall", 1.8, "старая известковая штукатурка в потёках (фотоскан Poly Haven)"),
    ("roof", "MrpgByzRoof", 1.5, "черепица VI века: тегулы и калиптеры (своя, Blender: tools/textures/blender_byz_roof.py, глина — из фотоскана)"),
    ("wood", "Planks039", 2.4, "грубые старые доски"),
    ("beam", "Wood060", 1.6, "тёмное дерево без швов"),
    ("earth", "Ground109", 3.0, "утоптанная сухая земля"),
    ("grass", "Grass004", 2.5, "трава"),
    ("soil", "Ground102", 3.5, "сухая степная почва"),
    ("rock", "Rock026", 4.0, "слоистая скала"),
    ("cobble", "PavingStones138", 2.4, "булыжная мостовая"),
    ("flagstone", "Tiles143", 3.4, "известняковые плиты пола"),
    ("marble", "Marble014", 2.0, "мрамор"),
    ("brick", "Bricks097", 1.6, "старый кирпич (для плинфы)"),
    ("wool", "Fabric045", 0.6, "грубая шерстяная ткань"),
    ("iron", "Metal052C", 1.2, "кованое железо"),
    ("bronze", "Metal047B", 1.2, "бронза/медь с патиной"),
    ("gold", "Metal048C", 1.0, "золото"),
    ("gravel", "Gravel041", 2.0, "береговая галька"),
]


# Крупные пятна на фото (тёмный лишайник на одном блоке) при повторе тайла бросаются в глаза:
# у этих слоёв гасим низкие частоты яркости (доля 0..1), мелкая фактура остаётся.
FLATTEN = {"ashlar": 0.6, "plaster": 0.4}


def flatten(lin, k):
    lum = lin.mean(-1)
    n = SIZE // 16
    small = lum.reshape(n, 16, n, 16).mean((1, 3))
    for _ in range(3):  # сглаживание с заворотом краёв — тайл остаётся бесшовным
        small = (small + np.roll(small, 1, 0) + np.roll(small, -1, 0) + np.roll(small, 1, 1) + np.roll(small, -1, 1)) / 5
    pad = 2
    big = Image.fromarray(np.pad(small, pad, mode="wrap").astype(np.float32), "F")
    big = big.resize(((n + 2 * pad) * 16,) * 2, Image.BILINEAR)
    low = np.asarray(big)[pad * 16 + 8 - 8:pad * 16 + SIZE, pad * 16:pad * 16 + SIZE][:SIZE, :SIZE]
    gain = (lum.mean() / np.maximum(low, 1e-4)) ** k
    return lin * gain[..., None]


def fetch(asset, src):
    """Возвращает словарь карта -> PIL.Image."""
    maps = {}
    names = {"Color": "color", "NormalGL": "normal", "Roughness": "rough", "AmbientOcclusion": "ao"}
    if src and os.path.isdir(src):
        for f in os.listdir(src):
            if f.startswith(asset + "_"):
                for k, v in names.items():
                    if f.endswith("_" + k + ".jpg") or f.endswith("_" + k + ".png"):
                        maps[v] = Image.open(os.path.join(src, f))
        zp = os.path.join(src, asset + "_1K-JPG.zip")
        if not maps and os.path.exists(zp):
            maps = from_zip(open(zp, "rb").read(), names)
    if not maps and asset.islower():
        maps = from_polyhaven(asset, names)
    if not maps:
        cache = os.path.expanduser("~/.cache/mrpg-ambientcg")
        os.makedirs(cache, exist_ok=True)
        zp = os.path.join(cache, asset + "_1K-JPG.zip")
        if not os.path.exists(zp):
            url = "https://ambientcg.com/get?file=%s_1K-JPG.zip" % asset
            print("скачиваю", url)
            req = urllib.request.Request(url, headers={"User-Agent": "mrpg-texture-builder"})
            open(zp, "wb").write(urllib.request.urlopen(req, timeout=120).read())
        maps = from_zip(open(zp, "rb").read(), names)
    return maps


def from_polyhaven(asset, names):
    """Фотоскан Poly Haven (CC0, https://polyhaven.com/license), карты 2K."""
    import json
    get = lambda u: urllib.request.urlopen(urllib.request.Request(u, headers={"User-Agent": "mrpg-texture-builder"}), timeout=120).read()
    files = json.loads(get("https://api.polyhaven.com/files/" + asset))
    keys = {"Diffuse": "color", "nor_gl": "normal", "Rough": "rough", "AO": "ao"}
    maps = {}
    for k, v in keys.items():
        if k in files:
            d = files[k]["2k"]
            fmt = "jpg" if "jpg" in d else "png"
            print("скачиваю", d[fmt]["url"])
            maps[v] = Image.open(io.BytesIO(get(d[fmt]["url"])))
    return maps


def from_zip(data, names):
    z = zipfile.ZipFile(io.BytesIO(data))
    maps = {}
    for m in z.namelist():
        for k, v in names.items():
            if m.endswith("_" + k + ".jpg") or m.endswith("_" + k + ".png"):
                maps[v] = Image.open(io.BytesIO(z.read(m)))
    return maps


def arr(im, mode):
    im = im.convert(mode)
    if im.size != (SIZE, SIZE):
        im = im.resize((SIZE, SIZE), Image.LANCZOS)
    return np.asarray(im).astype(np.float32) / 255.0


def to_lin(c):
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def to_srgb(l):
    return np.where(l <= 0.0031308, l * 12.92, 1.055 * np.power(np.clip(l, 0, None), 1 / 2.4) - 0.055)


def main():
    src = sys.argv[1] if len(sys.argv) > 1 else None
    os.makedirs(OUT, exist_ok=True)
    rows = []
    for i, (layer, asset, meters, what) in enumerate(LAYERS):
        m = fetch(asset, src)
        color = arr(m["color"], "RGB")
        lin = to_lin(color)
        if "ao" in m:
            # AO вносим в цвет мягко: прямой свет её не знает, а SSAO крупнее масштабом.
            ao = arr(m["ao"], "L")[..., None]
            lin = lin * (0.35 + 0.65 * ao)
        if layer in FLATTEN:
            lin = flatten(lin, FLATTEN[layer])
        alb = (np.clip(to_srgb(lin), 0, 1) * 255 + 0.5).astype(np.uint8)
        Image.fromarray(alb, "RGB").save(os.path.join(OUT, layer + "_albedo.jpg"), quality=93, subsampling=0)
        nrm = arr(m["normal"], "RGB")
        rough = arr(m["rough"], "L") if "rough" in m else np.full((SIZE, SIZE), 0.8, np.float32)
        pack = np.stack([nrm[..., 0], nrm[..., 1], rough], -1)
        Image.fromarray((pack * 255 + 0.5).astype(np.uint8), "RGB").save(os.path.join(OUT, layer + "_nrm.jpg"), quality=95, subsampling=0)
        mean = lin.reshape(-1, 3).mean(0)
        rows.append((i, layer, asset, meters, mean, what))
        print("%-10s %-16s %.1f м  средний цвет %s" % (layer, asset, meters, np.round(mean, 3)))

    with open(RS, "w", encoding="utf-8") as f:
        f.write("//! Сгенерировано tools/textures/build_surface_textures.py — не править руками.\n")
        f.write("//! Фотоматериалы ambientCG и Poly Haven (CC0), своя черепица из Blender: assets/textures/surface/<слой>_albedo.jpg и _nrm.jpg.\n\n")
        f.write("/// Слой фотоматериала: имя файла, id на ambientCG или Poly Haven, метров на тайл, средний линейный цвет.\n")
        f.write("pub struct Layer {\n    pub name: &'static str,\n    pub source: &'static str,\n    pub meters: f32,\n    pub mean: [f32; 3],\n}\n\n")
        f.write("pub const LAYERS: &[Layer] = &[\n")
        for i, layer, asset, meters, mean, what in rows:
            f.write("    // %d — %s\n" % (i, what))
            f.write('    Layer { name: "%s", source: "%s", meters: %.2f, mean: [%.4f, %.4f, %.4f] },\n' % (layer, asset, meters, *mean))
        f.write("];\n\n")
        for i, layer, *_ in rows:
            f.write("pub const L_%s: u32 = %d;\n" % (layer.upper(), i))
    print("готово:", OUT, RS)


if __name__ == "__main__":
    main()
