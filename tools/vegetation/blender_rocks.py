"""Камни-фотосканы Poly Haven (CC0) -> игровые glb с двумя LOD (Blender, на ПК или `pip install bpy`).

    python3 tools/vegetation/blender_rocks.py <папка с распакованными glTF 1k> <папка вывода>

Из каждого скана: меш `<id>_lod0` (~3000 треугольников) и `<id>_lod1` (~500), основание на y = 0,
центр по XZ в нуле; материал и текстуры (цвет, нормали, ORM) — из скана.
"""
import os
import sys

import bpy

ROCKS = ["rock_07", "rock_09", "stone_01", "namaqualand_boulder_02", "rock_face_01"]
TARGETS = [("lod0", 3000), ("lod1", 500)]


def process(src_dir, out_dir, rid):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=os.path.join(src_dir, rid, rid + ".gltf"))
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    for o in bpy.context.scene.objects:
        o.select_set(o in meshes)
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    ob = bpy.context.view_layer.objects.active
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    vs = [v.co for v in ob.data.vertices]
    cx = (min(v.x for v in vs) + max(v.x for v in vs)) / 2
    cy = (min(v.y for v in vs) + max(v.y for v in vs)) / 2
    z0 = min(v.z for v in vs)
    for v in ob.data.vertices:
        v.co.x -= cx
        v.co.y -= cy
        v.co.z -= z0
    full = len(ob.data.polygons)
    report = {}
    for name, target in TARGETS:
        lod = ob.copy()
        lod.data = ob.data.copy()
        lod.name = lod.data.name = f"{rid}_{name}"
        bpy.context.scene.collection.objects.link(lod)
        m = lod.modifiers.new("dec", "DECIMATE")
        m.ratio = min(1.0, target / max(1, full))
        bpy.context.view_layer.objects.active = lod
        bpy.ops.object.modifier_apply(modifier="dec")
        report[name] = len(lod.data.polygons)
    bpy.data.objects.remove(ob, do_unlink=True)
    path = os.path.join(out_dir, rid + ".glb")
    bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", export_yup=True, export_animations=False, use_active_scene=True)
    return report


if __name__ == "__main__":
    src, out = sys.argv[-2], sys.argv[-1]
    os.makedirs(out, exist_ok=True)
    for r in ROCKS:
        print(r, process(src, out, r), flush=True)
