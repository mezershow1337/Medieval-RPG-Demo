"""Превью assets/models/humans.glb: ряд персонажей в разных нарядах (Blender, Cycles).

    python3 tools/models/preview_humans.py out.png [крупно]
Переменные окружения: SAMPLES, W, H, YAW (поворот камеры), CLOSE=1 (лица крупно).
"""

import math
import os
import sys

import bpy
from mathutils import Euler, Matrix, Vector

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import preview  # noqa: E402

OUTFITS = {
    "игрок": ["body.head", "body.hands", "eyes", "hair.short", "beard.short", "tunic.byz", "trousers", "wraps", "shoes", "belt"],
    "стратиот": ["body.head", "body.hands", "eyes", "beard.short", "tunic.byz", "trousers", "boots", "mail", "belt", "helm.spangen"],
    "букелларий": ["body.head", "body.hands", "eyes", "beard.full", "tunic.byz", "trousers", "boots", "lamellar", "belt", "helm.conical"],
    "гот": ["body.head", "body.hands", "eyes", "hair.long", "beard.full", "tunic.goth", "trousers", "wraps", "shoes", "belt"],
    "алан": ["body.head", "body.hands", "eyes", "beard.full", "tunic.kaftan", "trousers", "boots", "lamellar", "belt", "helm.conical"],
    "гунн": ["body.head", "body.hands", "eyes", "beard.short", "tunic.kaftan", "trousers", "boots", "belt", "cap.felt"],
}
# Цвета, как их задаёт игра (Outfit::for_actor в src/rig.rs).
HAIR_DARK = (0.22, 0.15, 0.09)
TINTS = {
    "игрок": {"tunic_byz": (0.86, 0.84, 0.78), "trousers": (0.45, 0.4, 0.34), "hair": HAIR_DARK, "beard": HAIR_DARK},
    "стратиот": {"tunic_byz": (0.75, 0.25, 0.2), "trousers": (0.45, 0.4, 0.34), "beard": HAIR_DARK},
    "букелларий": {"tunic_byz": (0.35, 0.42, 0.6), "trousers": (0.3, 0.27, 0.24), "beard": (0.3, 0.2, 0.12)},
    "гот": {"tunic_goth": (0.82, 0.76, 0.55), "trousers": (0.5, 0.45, 0.36), "hair": (0.7, 0.55, 0.32), "beard": (0.7, 0.55, 0.32), "skin": (1.0, 0.96, 0.95)},
    "алан": {"tunic_kaftan": (0.6, 0.45, 0.3), "trousers": (0.35, 0.3, 0.25), "beard": (0.18, 0.12, 0.08)},
    "гунн": {"tunic_kaftan": (0.65, 0.58, 0.42), "trousers": (0.32, 0.27, 0.22), "beard": (0.08, 0.06, 0.05), "skin": (0.93, 0.86, 0.78)},
}


def tint(obj, colors):
    """Умножает базовый цвет материалов детали на цвет наряда (как base_color в игре)."""
    for slot in obj.material_slots:
        m = slot.material
        if m is None or not m.use_nodes:
            continue
        key = next((k for k in colors if m.name.startswith(k)), None)
        if key is None:
            continue
        nt = m.node_tree
        bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
        if bsdf is None or not bsdf.inputs["Base Color"].links:
            continue
        src = bsdf.inputs["Base Color"].links[0].from_socket
        mix = nt.nodes.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        mix.blend_type = "MULTIPLY"
        mix.inputs[0].default_value = 1.0
        mix.inputs[7].default_value = (*colors[key], 1.0)
        nt.links.new(src, mix.inputs[6])
        nt.links.new(mix.outputs[2], bsdf.inputs["Base Color"])


WEAPONS = {"игрок": "spatha", "стратиот": "spatha", "букелларий": "spatha", "гот": "axe", "алан": "spear", "гунн": "bow"}
SHIELDS = {"стратиот": "shield_oval", "букелларий": "shield_oval", "гот": "shield_round"}


def import_glb():
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=os.path.join(ROOT, "assets", "models", "humans.glb"))
    return [o for o in bpy.data.objects if o not in before]


def main():
    out = sys.argv[-1] if sys.argv[-1].endswith(".png") else "/tmp/humans.png"
    preview.reset()
    preview.floor()
    names = list(OUTFITS)
    only = os.environ.get("ONLY")
    if only:
        names = [n for n in names if n in only.split(",")]
    spacing = 0.9
    for i, who in enumerate(names):
        objs = import_glb()
        arm = next(o for o in objs if o.type == "ARMATURE")
        x = (i - (len(names) - 1) / 2) * spacing
        keep = set(OUTFITS[who])
        wanted = {WEAPONS.get(who, "") + ".blade", WEAPONS.get(who, "") + ".hilt", SHIELDS.get(who, "-") + ".body", SHIELDS.get(who, "-") + ".boss"}
        for o in objs:
            if o.type != "MESH":
                continue
            if o.name.split(".001")[0] in keep or o.data.name.split(".0")[0] in keep:
                tint(o, TINTS.get(who, {}))
                continue
            base = o.name.rsplit(".", 1)[0] if o.name[-4:-3] == "." and o.name[-3:].isdigit() else o.name
            if base in wanted:
                if "shield" in base:
                    o.location = (x - 0.38, 0.25, 1.0)
                    o.rotation_euler = (0, 0, 0.9)
                else:
                    o.location = (x + 0.2, -0.05, 0.84)
                    o.rotation_euler = (math.radians(-80), 0, 0)
                    if "spear" in base:
                        o.location = (x + 0.25, -0.05, 0.8)
                        o.rotation_euler = (0, 0, 0)
                    if "bow" in base:
                        o.location = (x - 0.3, 0.0, 0.95)
                        o.rotation_euler = (0, 0, 0)
                continue
            bpy.data.objects.remove(o)
        arm.location = (x, 0, 0)
    close = os.environ.get("CLOSE")
    yaw = float(os.environ.get("YAW", "0"))
    if close:
        preview.camera_and_lights(target=(0, 0, 1.6), dist=1.0 + 0.45 * len(names), yaw=yaw, height=1.62, lens=70)
    else:
        preview.camera_and_lights(target=(0, 0, 0.95), dist=2.2 + 0.75 * len(names), yaw=yaw, height=1.1, lens=45)
    preview.render(out, int(os.environ.get("W", "1400")), int(os.environ.get("H", "800")))


main()
