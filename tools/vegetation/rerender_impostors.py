"""Перерисовывает импостеры деревьев по уже готовым мешам из assets/models/vegetation/*.glb.

Сами деревья не пересобираются: из glb берутся меши `<id>_lod0_bark` и `<id>_lod0_leaf`,
а дальше работает та же функция `impostor()`, что и в blender_trees.py (размер — IMP_SIZE).
После рендера яркость выравнивается через fix_impostors.py.

Запуск (Blender как модуль Python, `pip install bpy==4.5.*` под Python 3.11, или в Blender):
    MRPG_VEG_BARK=<папка с Bark014_1K-JPG_Color.jpg и _NormalGL.jpg> \\
    python3.11 tools/vegetation/rerender_impostors.py <папка_вывода> [id ...]
Атласы листвы берутся из assets/textures/vegetation. Готовые <id>_imp.png из папки вывода
копируются в assets/textures/vegetation вручную, после проверки на глаз.
"""
import os
import shutil
import subprocess
import sys

import bpy

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
VEG = os.path.join(ROOT, "assets", "textures", "vegetation")
MODELS = os.path.join(ROOT, "assets", "models", "vegetation")
os.environ.setdefault("MRPG_VEG_ATLAS", VEG)

trees = {"__name__": "blender_trees"}
exec(compile(open(os.path.join(HERE, "blender_trees.py"), encoding="utf-8").read(), "blender_trees.py", "exec"), trees)


def load_meshes(tid):
    """Меши ближнего LOD из готового glb (ось Y glTF → Z Blender делает импортёр)."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=os.path.join(MODELS, f"{tid}.glb"))
    meshes = {}
    for ob in set(bpy.data.objects) - before:
        if ob.type == "MESH" and ob.name.split(".")[0] in (f"{tid}_lod0_bark", f"{tid}_lod0_leaf"):
            me = ob.data
            me.transform(ob.matrix_world)
            meshes[ob.name.split(".")[0]] = me
        bpy.data.objects.remove(ob, do_unlink=True)
    return meshes


def main(out_dir, only):
    os.makedirs(out_dir, exist_ok=True)
    scene = bpy.data.scenes.get("mrpg_trees") or bpy.data.scenes.new("mrpg_trees")
    mats = trees["materials"]()
    for key, sp in trees["SPECIES"].items():
        for v in range(trees["VARIANTS"][key]):
            tid = f"{key}_{v}"
            if only and tid not in only:
                continue
            meshes = load_meshes(tid)
            trees["impostor"](tid, meshes, mats, sp, out_dir, scene)
            print("импостер", tid, "готов", flush=True)
    # fix_impostors берёт средний цвет атласов из той же папки.
    for a in ("broadleaf", "pine", "scale"):
        shutil.copy(os.path.join(VEG, f"{a}_atlas.png"), out_dir)
    subprocess.run([sys.executable, os.path.join(HERE, "fix_impostors.py"), out_dir], check=True)


if __name__ == "__main__":
    args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else sys.argv[1:]
    main(args[0], set(args[1:]))
