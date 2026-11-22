"""Проверка результата build_items.py:
  * лист иконок на фоне клеток инвентаря (docs/items_icons.png);
  * все .glb разложены на каменном полу, как в игре (docs/items_ground.jpg).

    python3 tools/models/preview_items.py
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import itemkit as K  # noqa: E402
from PIL import Image  # noqa: E402

import build_items  # noqa: E402

DOCS = os.path.join(K.ROOT, "docs")
CELL = 64  # клетка на листе иконок


def icon_sheet(keys, path):
    pad = 10
    cols = 8
    rows = []
    row, row_w = [], 0
    for k in keys:
        im = Image.open(os.path.join(K.OUT_ICONS, k + ".png")).convert("RGBA")
        w, h = build_items.ITEMS[k]["cells"]
        im = im.resize((w * CELL, h * CELL), Image.LANCZOS)
        if len(row) >= cols:
            rows.append(row)
            row = []
        row.append(im)
    rows.append(row)
    width = max(sum(i.width + pad for i in r) for r in rows) + pad
    height = sum(max(i.height for i in r) + pad for r in rows) + pad
    sheet = Image.new("RGBA", (width, height), (30, 26, 22, 255))
    y = pad
    for r in rows:
        x = pad
        for im in r:
            cells = Image.new("RGBA", im.size, (0, 0, 0, 0))
            for cy in range(0, im.height, CELL):
                for cx in range(0, im.width, CELL):
                    tile = Image.new("RGBA", (CELL - 2, CELL - 2), (14, 12, 10, 200))
                    cells.alpha_composite(tile, (cx + 1, cy + 1))
            sheet.alpha_composite(cells, (x, y))
            sheet.alpha_composite(im, (x, y))
            x += im.width + pad
        y += max(i.height for i in r) + pad
    sheet.convert("RGB").save(path)


def ground_render(keys, path):
    K.reset_scene()
    bpy = K.bpy
    scene = bpy.context.scene
    n = len(keys)
    cols = 6
    spacing = 1.0
    for i, k in enumerate(keys):
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=os.path.join(K.OUT_MODELS, k + ".glb"))
        new = [o for o in bpy.data.objects if o not in before]
        x = (i % cols - (cols - 1) / 2) * spacing
        y = (i // cols) * spacing * 0.9
        for o in new:
            if o.parent is None:
                o.location.x += x
                o.location.y += y
                o.rotation_euler.z += math.radians((i * 37) % 50 - 25)
    # Пол.
    bpy.ops.mesh.primitive_plane_add(size=40, location=(0, 1.5, 0))
    floor = bpy.context.object
    m, nb, bsdf = K.new_material("floor")
    v = nb.coord((1, 1, 1))
    tiles = nb.voronoi(1.6, v, "F1", "Distance")
    col = nb.mix(nb.noise(3.0, 6, 0.6, v), (0.09, 0.085, 0.075), (0.18, 0.16, 0.14))
    nb.val(bsdf.inputs["Base Color"], nb.mix(nb.ramp(tiles, [(0.0, 1.0), (0.05, 0.0)]), col, (0.03, 0.03, 0.03)))
    bsdf.inputs["Roughness"].default_value = 0.85
    floor.data.materials.append(m)
    # Свет: «вечернее солнце» и небо.
    sun = bpy.data.lights.new("sun", "SUN")
    sun.energy = 3.0
    sun.angle = math.radians(3)
    so = K.link(bpy.data.objects.new("sun", sun))
    so.rotation_euler = (math.radians(50), 0, math.radians(35))
    world = bpy.data.worlds.new("w")
    scene.world = world
    try:
        world.use_nodes = True
    except Exception:
        pass
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.35, 0.42, 0.55, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.6
    cam_d = bpy.data.cameras.new("cam")
    cam_d.lens = 32
    cam = K.link(bpy.data.objects.new("cam", cam_d))
    rows = (n + cols - 1) // cols
    cam.location = (0, -2.6, 3.2)
    target = K.Vector((0, rows * spacing * 0.9 / 2 - 0.3, 0))
    cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
    scene.camera = cam
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 96
    scene.cycles.use_denoising = True
    scene.render.resolution_x = 1600
    scene.render.resolution_y = 1000
    scene.view_settings.view_transform = "AgX"
    scene.render.image_settings.file_format = "JPEG"
    scene.render.image_settings.quality = 88
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)


def main():
    keys = [a for a in sys.argv[1:] if not a.startswith("-")] or list(build_items.ITEMS)
    os.makedirs(DOCS, exist_ok=True)
    icon_sheet(keys, os.path.join(DOCS, "items_icons.png"))
    ground_render(keys, os.path.join(DOCS, "items_ground.jpg"))


if __name__ == "__main__":
    main()
