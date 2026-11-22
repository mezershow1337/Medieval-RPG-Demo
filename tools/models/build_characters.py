"""Модели персонажей: тело из жёстких частей, броня по слотам, оружие и щит.

Всё строится кодом (Blender 5.x, можно как модуль `pip install bpy`):
    python3 tools/models/textures.py
    python3 tools/models/build_characters.py          # -> assets/models/characters.glb
    python3 tools/models/build_characters.py --preview out.png

Соглашения (важно для src/rig.rs):
* Каждая деталь — отдельный меш с именем "<кость>.<слой>", например "torso.cuirass".
* Начало координат меша — сустав, к которому деталь крепится. В покое конечности висят вниз.
* В Blender персонаж смотрит в +Y (в Bevy это -Z), правая сторона — +X.
* Левые конечности — зеркальные копии с суффиксом _l, правые — _r.
* Оружие: начало в центре гарды, клинок вдоль +Z (в Bevy +Y), рукоять вниз.
* Щит: начало в центре, лицевая сторона смотрит в +Y (в Bevy -Z).
"""
import math
import os
import sys

import bpy  # noqa: E402  (bpy должен импортироваться раньше bmesh)
import bmesh
from mathutils import Euler, Matrix, Vector

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
TEX = os.path.join(ROOT, "assets", "models", "textures")
OUT = os.path.join(ROOT, "assets", "models", "characters.glb")

# Суставы в покое (Blender, метры). Те же числа — в src/rig.rs.
JOINTS = {
    "pelvis": (0.0, 0.0, 0.98),
    "torso": (0.0, 0.0, 1.04),
    "head": (0.0, 0.0, 1.50),
    "upper_arm": (0.2, 0.0, 1.42),
    "forearm": (0.2, 0.0, 1.13),
    "hand": (0.2, 0.0, 0.87),
    "thigh": (0.1, 0.0, 0.94),
    "shin": (0.1, 0.0, 0.50),
    "foot": (0.1, 0.0, 0.08),
}
LIMBS = {"upper_arm", "forearm", "hand", "thigh", "shin", "foot"}

# --------------------------------------------------------------------------- материалы


def reset_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def image(name, non_color=False):
    path = os.path.join(TEX, name + ".png")
    img = bpy.data.images.get(name) or bpy.data.images.load(path)
    img.name = name
    if non_color:
        img.colorspace_settings.name = "Non-Color"
    return img


MATS = {}


def material(name, color=None, tex=None, mr=None, normal=None, metallic=0.0, rough=0.6, normal_strength=1.0, emissive=None):
    m = bpy.data.materials.new(name)
    try:
        m.use_nodes = True
    except Exception:
        pass
    nt = m.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = rough
    if color:
        bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    if tex:
        t = nt.nodes.new("ShaderNodeTexImage")
        t.image = image(tex)
        nt.links.new(t.outputs["Color"], bsdf.inputs["Base Color"])
    if mr:
        t = nt.nodes.new("ShaderNodeTexImage")
        t.image = image(mr, True)
        sep = nt.nodes.new("ShaderNodeSeparateColor")
        nt.links.new(t.outputs["Color"], sep.inputs["Color"])
        nt.links.new(sep.outputs["Green"], bsdf.inputs["Roughness"])
        nt.links.new(sep.outputs["Blue"], bsdf.inputs["Metallic"])
    if normal:
        t = nt.nodes.new("ShaderNodeTexImage")
        t.image = image(normal, True)
        nm = nt.nodes.new("ShaderNodeNormalMap")
        nm.inputs["Strength"].default_value = normal_strength
        nt.links.new(t.outputs["Color"], nm.inputs["Color"])
        nt.links.new(nm.outputs["Normal"], bsdf.inputs["Normal"])
    if emissive:
        bsdf.inputs["Emission Color"].default_value = (*emissive, 1.0)
        bsdf.inputs["Emission Strength"].default_value = 1.0
    MATS[name] = m
    return m


def make_materials():
    material("steel", tex="steel_color", mr="steel_mr", metallic=0.95, rough=0.35)
    material("darksteel", tex="darksteel_color", mr="darksteel_mr", metallic=0.9, rough=0.45)
    material("mail", tex="mail_color", mr="mail_mr", normal="mail_normal", metallic=0.9, rough=0.5)
    # Ткань и стёганка почти белые: цвет (цвет игрока, герб рода) задаёт игра.
    material("gambeson", tex="quilted_color", normal="quilted_normal", rough=0.9)
    material("cloth", tex="cloth_color", normal="cloth_normal", rough=0.95)
    material("tabard", tex="cloth_color", normal="cloth_normal", rough=0.95)
    material("leather", tex="leather_color", normal="leather_normal", rough=0.7)
    material("skin", tex="skin_color", rough=0.65)
    material("wood", tex="wood_color", rough=0.8)
    material("heraldry", tex="heraldry_color", rough=0.6)
    material("brass", color=(0.75, 0.55, 0.25), metallic=1.0, rough=0.35)
    material("hair", color=(0.12, 0.07, 0.04), rough=0.85)
    material("dark", color=(0.02, 0.02, 0.02), rough=0.9)
    material("blade", tex="steel_color", mr="steel_mr", metallic=1.0, rough=0.25)


# UV: метров на один повтор текстуры для каждого материала.
UV_TILE = {"mail": 0.16, "gambeson": 0.36, "cloth": 0.5, "tabard": 0.5, "leather": 0.4, "skin": 0.5, "wood": 0.5, "steel": 0.6, "darksteel": 0.6, "blade": 0.6}

# --------------------------------------------------------------------------- геометрия


def lathe(rings, segs=16, cap_top=True, cap_bot=True):
    """Тело вращения вокруг Z. rings: сверху вниз (z, rx[, ry[, cx, cy]]). r=0 — полюс."""
    bm = bmesh.new()
    loops = []
    for ring in rings:
        z, rx = ring[0], ring[1]
        ry = ring[2] if len(ring) > 2 else rx
        cx = ring[3] if len(ring) > 3 else 0.0
        cy = ring[4] if len(ring) > 4 else 0.0
        if rx == 0:
            loops.append([bm.verts.new((cx, cy, z))])
        else:
            loops.append([bm.verts.new((cx + rx * math.cos(2 * math.pi * i / segs), cy + ry * math.sin(2 * math.pi * i / segs), z)) for i in range(segs)])
    for a, b in zip(loops, loops[1:]):
        for i in range(segs):
            j = (i + 1) % segs
            if len(a) == 1 and len(b) == 1:
                continue
            if len(a) == 1:
                bm.faces.new((a[0], b[j], b[i]))
            elif len(b) == 1:
                bm.faces.new((a[i], a[j], b[0]))
            else:
                bm.faces.new((a[i], a[j], b[j], b[i]))
    if cap_top and len(loops[0]) > 1:
        bm.faces.new(list(reversed(loops[0])))
    if cap_bot and len(loops[-1]) > 1:
        bm.faces.new(loops[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    return bm


def box(x0, x1, y0, y1, z0, z1):
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co = Vector((x0 + (v.co.x + 0.5) * (x1 - x0), y0 + (v.co.y + 0.5) * (y1 - y0), z0 + (v.co.z + 0.5) * (z1 - z0)))
    return bm


def merge(*bms):
    out = bmesh.new()
    for b in bms:
        me = bpy.data.meshes.new("tmp")
        b.to_mesh(me)
        out.from_mesh(me)
        bpy.data.meshes.remove(me)
        b.free()
    return out


def deform(bm, fn):
    for v in bm.verts:
        v.co = Vector(fn(v.co.copy()))
    return bm


def delete_faces(bm, pred):
    faces = [f for f in bm.faces if pred(f.calc_center_median())]
    bmesh.ops.delete(bm, geom=faces, context="FACES")
    return bm


def set_mat(bm, index, pred):
    for f in bm.faces:
        if pred(f.calc_center_median()):
            f.material_index = index
    return bm


def box_uv(me, mats):
    uv = me.uv_layers.new(name="UVMap")
    for poly in me.polygons:
        tile = UV_TILE.get(mats[poly.material_index].name, 0.5)
        n = poly.normal
        ax = max(range(3), key=lambda i: abs(n[i]))
        a, b = [(1, 2), (0, 2), (0, 1)][ax]
        for li in poly.loop_indices:
            co = me.vertices[me.loops[li].vertex_index].co
            uv.data[li].uv = (co[a] / tile, co[b] / tile)


def finish(name, bm, mats, subsurf=1, solidify=0.0, smooth=True, location=(0, 0, 0)):
    """Меш из bmesh: модификаторы применяются, UV — кубическая проекция."""
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    for m in mats:
        me.materials.append(MATS[m])
    if solidify:
        mod = obj.modifiers.new("solid", "SOLIDIFY")
        mod.thickness = solidify
        mod.offset = 1.0
    if subsurf:
        mod = obj.modifiers.new("sub", "SUBSURF")
        mod.levels = subsurf
        mod.render_levels = subsurf
    dg = bpy.context.evaluated_depsgraph_get()
    ev = obj.evaluated_get(dg)
    new = bpy.data.meshes.new_from_object(ev)
    obj.modifiers.clear()
    obj.data = new
    bpy.data.meshes.remove(me)
    new.name = name
    for p in new.polygons:
        p.use_smooth = smooth
    box_uv(new, [MATS[m] for m in mats])
    obj.location = location
    return obj


PARTS = []  # (имя, объект)


def part(bone, layer, bm, mats, mirror=True, **kw):
    """Регистрирует деталь кости; для конечностей делает правую и зеркальную левую."""
    if bone in LIMBS:
        right = finish(f"{bone}_r.{layer}", bm, mats, location=JOINTS[bone], **kw)
        PARTS.append(right)
        if mirror:
            me = right.data.copy()
            me.name = f"{bone}_l.{layer}"
            for v in me.vertices:
                v.co.x = -v.co.x
            me.flip_normals()
            left = bpy.data.objects.new(me.name, me)
            bpy.context.scene.collection.objects.link(left)
            x, y, z = JOINTS[bone]
            left.location = (-x, y, z)
            PARTS.append(left)
        return right
    obj = finish(f"{bone}.{layer}", bm, mats, location=JOINTS.get(bone, (0, 0, 0)), **kw)
    PARTS.append(obj)
    return obj


def gauss(x, w):
    return math.exp(-((x / w) ** 2))


# --------------------------------------------------------------------------- тело


def body():
    part("pelvis", "body", lathe([(0.09, 0.145, 0.10), (0.04, 0.16, 0.11), (-0.04, 0.17, 0.115, 0, -0.005), (-0.10, 0.15, 0.105), (-0.15, 0.09, 0.07), (-0.16, 0)]), ["cloth"])
    torso = lathe([(-0.04, 0.15, 0.105), (0.08, 0.16, 0.11, 0, 0.005), (0.2, 0.18, 0.125, 0, 0.012), (0.3, 0.195, 0.125, 0, 0.01), (0.37, 0.2, 0.115), (0.42, 0.15, 0.09), (0.46, 0.07, 0.06), (0.49, 0.06, 0.055)], segs=20)
    part("torso", "body", torso, ["gambeson"])
    neck = lathe([(0.12, 0.05, 0.05, 0, -0.005), (-0.02, 0.06, 0.06, 0, -0.005)])
    head = lathe([(0.17 + 0.118 * math.cos(a), 0.082 * math.sin(a) + 1e-9 if 0 < a < math.pi else 0, 0.1 * math.sin(a) if 0 < a < math.pi else 0, 0, 0.01) for a in [i * math.pi / 18 for i in range(19)]], segs=36)

    def face(c):
        x, y, z = c
        if y > 0.05:
            y += 0.026 * gauss(x, 0.016) * gauss(z - 0.155, 0.028) * (1 if z < 0.19 else 0.5)  # нос
            y += 0.012 * gauss(z - 0.205, 0.012) * gauss(abs(x) - 0.035, 0.03)  # надбровья
            y -= 0.012 * gauss(z - 0.185, 0.015) * gauss(abs(x) - 0.035, 0.015)  # глазницы
        if z < 0.12:
            x *= 0.92  # подбородок уже
        return x, y, z

    eyes = []
    for sx in (-1, 1):
        e = lathe([(0.011, 0), (0.0, 0.012), (-0.011, 0)], segs=10)
        eyes.append(set_mat(deform(e, lambda c, sx=sx: (c.x + sx * 0.032, c.y + 0.104, c.z + 0.183)), 1, lambda c: True))
        ear = lathe([(0.025, 0), (0.0, 0.012, 0.022), (-0.025, 0)], segs=10)
        eyes.append(deform(ear, lambda c, sx=sx: (c.x + sx * 0.082, c.y - 0.0, c.z + 0.165)))
    hair = lathe([(0.172 + 0.126 * math.cos(a), 0.09 * math.sin(a) + 1e-9 if 0 < a < math.pi else 0, 0.108 * math.sin(a) if 0 < a < math.pi else 0, 0, 0.006) for a in [i * math.pi / 18 for i in range(13)]], segs=36, cap_bot=False)
    hair = delete_faces(hair, lambda c: (c.y > 0.02 and c.z < 0.235) or (abs(c.x) > 0.06 and c.z < 0.2 and c.y > -0.04))
    set_mat(hair, 2, lambda c: True)
    head_bm = merge(neck, deform(head, face), *eyes, hair)
    part("head", "body", head_bm, ["skin", "dark", "hair"])
    # Пояс с пряжкой — у всех.
    belt = tube(0.035, -0.01, 0.168, 0.168, segs=24, ry_scale=0.69, steps=1)
    buckle = box(-0.022, 0.022, 0.112, 0.124, -0.012, 0.037)
    belt_bm = merge(belt, buckle)
    set_mat(belt_bm, 1, lambda c: c.y > 0.11 and abs(c.x) < 0.024)
    part("pelvis", "belt", belt_bm, ["leather", "brass"], solidify=0.006, subsurf=0)
    # Подол стёганки (для лёгкого наряда).
    hem = tube(0.06, -0.24, 0.168, 0.2, segs=24, ry_scale=0.72, steps=4)
    hem = delete_faces(hem, lambda c: abs(c.x) > 0.14 and c.z < -0.05)
    part("pelvis", "gambeson", hem, ["gambeson"], solidify=0.012)
    part("upper_arm", "body", lathe([(0.065, 0), (0.05, 0.045), (0.0, 0.06), (-0.1, 0.058), (-0.2, 0.05), (-0.29, 0.045), (-0.32, 0)]), ["gambeson"])
    part("forearm", "body", lathe([(0.03, 0), (0.0, 0.045, 0.04), (-0.07, 0.05, 0.043), (-0.18, 0.04, 0.034), (-0.26, 0.032, 0.028), (-0.28, 0)]), ["gambeson"])
    fist = box(-0.038, 0.038, -0.03, 0.035, -0.105, -0.005)
    thumb = box(-0.052, -0.025, 0.0, 0.03, -0.07, -0.02)
    part("hand", "body", merge(fist, thumb), ["skin"], subsurf=2)
    part("thigh", "body", lathe([(0.06, 0), (0.03, 0.075), (-0.05, 0.085), (-0.25, 0.07), (-0.42, 0.055), (-0.47, 0)]), ["cloth"])
    part("shin", "body", lathe([(0.03, 0), (0.0, 0.055), (-0.1, 0.058, 0.058, 0, -0.01), (-0.25, 0.045), (-0.4, 0.035), (-0.43, 0)]), ["cloth"])


# --------------------------------------------------------------------------- снаряжение


def tube(z0, z1, r0, r1, segs=16, ry_scale=1.0, cy=0.0, steps=4):
    rings = []
    for i in range(steps + 1):
        t = i / steps
        r = r0 + (r1 - r0) * t
        rings.append((z0 + (z1 - z0) * t, r, r * ry_scale, 0, cy))
    return lathe(rings, segs, cap_top=False, cap_bot=False)


def gear():
    # ---- Голова
    part("head", "coif", delete_faces(
        lathe([(0.31, 0), (0.295, 0.06, 0.07), (0.25, 0.095, 0.11), (0.17, 0.1, 0.12, 0, 0.005), (0.08, 0.09, 0.105, 0, 0.0), (0.02, 0.1, 0.1), (-0.04, 0.15, 0.13)], segs=20, cap_bot=False),
        lambda c: c.y > 0.05 and abs(c.x) < 0.065 and 0.09 < c.z < 0.235), ["mail"], solidify=0.006)

    nasal = lathe([(0.335, 0), (0.31, 0.05, 0.06), (0.26, 0.09, 0.108), (0.2, 0.1, 0.12), (0.17, 0.102, 0.122), (0.155, 0.103, 0.123)], segs=24, cap_bot=False)
    nasal = deform(nasal, lambda c: (c.x, c.y + 0.004 * gauss(c.x, 0.01) * (c.z > 0.17), c.z))
    band = tube(0.17, 0.145, 0.106, 0.106, segs=24, ry_scale=1.17, steps=1)
    bar = box(-0.008, 0.008, 0.118, 0.13, 0.07, 0.165)
    # Шлем сидит на лбу: поднят так, чтобы глаза были под ободом.
    part("head", "nasal_helm", deform(merge(nasal, band, bar), lambda c: (c.x, c.y, c.z + 0.045)), ["steel"], subsurf=1, solidify=0.0)

    gh = lathe([(0.335, 0), (0.33, 0.05, 0.06), (0.315, 0.1, 0.115), (0.29, 0.108, 0.125), (0.2, 0.108, 0.125, 0, 0.005), (0.1, 0.106, 0.123, 0, 0.005), (0.02, 0.104, 0.12, 0, 0.0), (-0.03, 0.105, 0.118)], segs=28, cap_bot=False)
    for v in gh.verts:  # подразбиваем по высоте нет — делаем прорезь вдавливанием
        pass
    gh = bmesh_subdivide(gh, 2)

    def slit(c):
        x, y, z = c
        if y > 0.04 and 0.168 < z < 0.19 and abs(x) > 0.008:
            y -= 0.018
        if y > 0.06:
            y += 0.008 * gauss(x, 0.012)  # рёбро
        return x, y, z

    gh = deform(gh, slit)
    set_mat(gh, 1, lambda c: c.y > 0.04 and 0.168 < c.z < 0.19 and abs(c.x) > 0.008)
    set_mat(gh, 1, lambda c: c.y > 0.04 and 0.04 < c.z < 0.13 and 0.03 < c.x < 0.09 and int(c.z * 120) % 2 == 0 and int(c.x * 120) % 2 == 0)
    set_mat(gh, 2, lambda c: c.z < 0.0 or (c.y > 0.08 and abs(c.x) < 0.008 and c.z < 0.165))
    part("head", "great_helm", gh, ["steel", "dark", "brass"], subsurf=0, solidify=0.006)

    kettle = lathe([(0.315, 0), (0.3, 0.05, 0.055), (0.26, 0.095, 0.105), (0.2, 0.108, 0.118), (0.178, 0.11, 0.12), (0.168, 0.16, 0.17), (0.15, 0.19, 0.2)], segs=24, cap_bot=False)
    part("head", "kettle_hat", deform(kettle, lambda c: (c.x, c.y, c.z + 0.05)), ["steel"], solidify=0.005)

    # ---- Торс
    def shell(scale, z_top, open_neck=True):
        rings = [(-0.06, 0.155 * scale, 0.11 * scale), (0.08, 0.165 * scale, 0.118 * scale, 0, 0.006), (0.2, 0.185 * scale, 0.13 * scale, 0, 0.014), (0.3, 0.2 * scale, 0.13 * scale, 0, 0.01), (0.37, 0.205 * scale, 0.12 * scale), (z_top, 0.16 * scale, 0.095 * scale)]
        return lathe(rings, segs=24, cap_top=not open_neck, cap_bot=False)

    part("torso", "mail", lathe([(-0.06, 0.16, 0.115), (0.08, 0.17, 0.12, 0, 0.006), (0.2, 0.19, 0.133, 0, 0.014), (0.3, 0.205, 0.133, 0, 0.01), (0.37, 0.21, 0.123), (0.42, 0.16, 0.098), (0.45, 0.08, 0.07)], segs=24, cap_bot=False), ["mail"], solidify=0.004)
    part("pelvis", "mail", tube(0.1, -0.3, 0.168, 0.205, segs=24, ry_scale=0.74, steps=4), ["mail"], solidify=0.005)
    part("upper_arm", "mail", lathe([(0.075, 0), (0.05, 0.05), (0.0, 0.068), (-0.1, 0.066), (-0.2, 0.06)], cap_bot=False), ["mail"], solidify=0.004)

    tabard = shell(1.1, 0.42)
    tabard = delete_faces(tabard, lambda c: c.z > 0.3 and abs(c.x) > 0.12)  # без рукавов
    part("torso", "tabard", tabard, ["tabard"], solidify=0.004)
    skirt = tube(0.06, -0.44, 0.185, 0.24, segs=24, ry_scale=0.72, steps=6)
    skirt = delete_faces(skirt, lambda c: abs(c.x) > 0.13 and c.z < -0.08)  # разрезы по бокам
    part("pelvis", "tabard", skirt, ["tabard"], solidify=0.004)

    # Сюрко поверх лат (тяжёлый рыцарь): длинное, с разрезами спереди и сзади.
    surcoat = shell(1.22, 0.4)
    surcoat = delete_faces(surcoat, lambda c: c.z > 0.28 and abs(c.x) > 0.13)
    part("torso", "surcoat", surcoat, ["tabard"], solidify=0.005)
    skirt = tube(0.0, -0.5, 0.22, 0.27, segs=24, ry_scale=0.72, steps=6)
    skirt = delete_faces(skirt, lambda c: (abs(c.x) < 0.035 and c.z < -0.15) or (abs(c.x) > 0.17 and c.z < -0.12))
    part("pelvis", "surcoat", skirt, ["tabard"], solidify=0.005)

    cuirass = shell(1.13, 0.43)
    cuirass = deform(cuirass, lambda c: (c.x, c.y + (0.022 * gauss(c.x, 0.05) if c.y > 0 else 0), c.z))
    part("torso", "cuirass", cuirass, ["steel"], solidify=0.006)
    lames = []
    for i, (z, r) in enumerate([(0.08, 0.18), (0.02, 0.19), (-0.04, 0.2), (-0.1, 0.21)]):
        lames.append(tube(z, z - 0.07, r, r + 0.012, segs=24, ry_scale=0.72, steps=1))
    part("pelvis", "faulds", merge(*lames), ["steel"], solidify=0.004)
    pauldron = []
    for i, (z, r) in enumerate([(0.0, 0.09), (-0.05, 0.085), (-0.1, 0.08)]):
        pauldron.append(tube(z, z - 0.06, r, r + 0.005, cy=0.0, steps=1))
    dome = lathe([(0.1, 0), (0.08, 0.06), (0.04, 0.085), (0.0, 0.095)], cap_bot=False)
    part("upper_arm", "pauldron", deform(merge(dome, *pauldron), lambda c: (c.x + 0.02, c.y, c.z)), ["steel"], solidify=0.005)
    part("upper_arm", "rerebrace", tube(-0.1, -0.27, 0.066, 0.058), ["steel"], solidify=0.004)

    # ---- Руки
    part("forearm", "vambrace", lathe([(0.065, 0, 0, 0, -0.02), (0.045, 0.05, 0.05, 0, -0.015), (0.0, 0.066, 0.062), (-0.05, 0.058, 0.052), (-0.2, 0.049, 0.043), (-0.24, 0.051, 0.045)], cap_bot=False), ["steel"], solidify=0.004)
    g_fist = box(-0.046, 0.046, -0.038, 0.043, -0.118, 0.0)
    g_fist = bmesh_subdivide(g_fist, 2)
    g_fist = deform(g_fist, lambda c: (c.x, c.y + 0.004 * math.sin(c.z * 160) * (c.y > 0.03), c.z))
    g_thumb = box(-0.06, -0.03, 0.0, 0.035, -0.075, -0.015)
    part("hand", "gauntlet", merge(g_fist, g_thumb), ["steel"], subsurf=1)
    part("forearm", "gauntlet_cuff", tube(-0.17, -0.28, 0.05, 0.068, steps=2), ["steel"], solidify=0.004)
    glove = merge(box(-0.042, 0.042, -0.034, 0.039, -0.11, -0.005), box(-0.056, -0.027, 0.0, 0.032, -0.072, -0.018))
    part("hand", "glove", glove, ["leather"], subsurf=2)
    part("forearm", "glove_cuff", tube(-0.2, -0.27, 0.042, 0.055, steps=2), ["leather"], solidify=0.004)

    # ---- Ноги
    cuisse = tube(0.02, -0.38, 0.095, 0.072, segs=20, steps=4)
    cuisse = delete_faces(cuisse, lambda c: c.y < -0.03)
    part("thigh", "cuisse", cuisse, ["steel"], solidify=0.005)
    greave = tube(-0.04, -0.4, 0.068, 0.048, segs=20, cy=-0.004, steps=4)
    poleyn = lathe([(0.06, 0, 0, 0, 0.03), (0.04, 0.05, 0.035, 0, 0.035), (0.0, 0.07, 0.05, 0, 0.03), (-0.05, 0.065, 0.05, 0, 0.02)], cap_bot=False)
    part("shin", "greave", merge(greave, poleyn), ["steel"], solidify=0.005)
    part("shin", "boot_cuff", tube(-0.18, -0.44, 0.058, 0.048, steps=3), ["leather"], solidify=0.006)

    boot = box(-0.046, 0.046, -0.055, 0.175, -0.08, 0.02)
    boot = bmesh_subdivide(boot, 2)
    boot = deform(boot, lambda c: (c.x * (1 - 0.25 * max(0, c.y - 0.08) / 0.1), c.y, c.z - 0.02 * max(0, c.y - 0.1) / 0.08 * (c.z > -0.03)))
    ankle = lathe([(0.08, 0.045), (0.0, 0.05), (-0.05, 0.055)], cap_top=True, cap_bot=False)
    part("foot", "boot", merge(boot, ankle), ["leather"], subsurf=1)
    sab = box(-0.052, 0.052, -0.06, 0.2, -0.08, 0.03)
    sab = bmesh_subdivide(sab, 3)
    sab = deform(sab, lambda c: (c.x * (1 - 0.45 * max(0, c.y - 0.08) / 0.12), c.y, c.z - 0.03 * max(0, c.y - 0.08) / 0.12 * (c.z > -0.04) + (0.004 * (int(c.y * 35) % 2) if c.z > 0 and c.y > 0 else 0)))
    ankle2 = lathe([(0.09, 0.052), (0.0, 0.058), (-0.05, 0.062)], cap_top=True, cap_bot=False)
    part("foot", "sabaton", merge(sab, ankle2), ["steel"], subsurf=1)


def bmesh_subdivide(bm, cuts):
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=cuts, use_grid_fill=True)
    return bm


# --------------------------------------------------------------------------- оружие


def blade_mesh(length, width, base_z=0.02, thick=0.008, tip=0.12, ricasso=0.0):
    """Ромбовидное сечение, сужение к острию."""
    bm = bmesh.new()
    steps = 10
    rings = []
    for i in range(steps + 1):
        t = i / steps
        z = base_z + (length - tip) * t
        w = width * (1 - 0.35 * t)
        if z < base_z + ricasso:
            w = width * 0.75
        rings.append((z, w, thick))
    rings.append((base_z + length, 0.001, 0.001))
    loops = []
    for z, w, th in rings:
        loops.append([bm.verts.new(p) for p in [(w / 2, 0, z), (0, th / 2, z), (-w / 2, 0, z), (0, -th / 2, z)]])
    for a, b in zip(loops, loops[1:]):
        for i in range(4):
            j = (i + 1) % 4
            bm.faces.new((a[i], a[j], b[j], b[i]))
    bm.faces.new(list(reversed(loops[0])))
    bm.faces.new(loops[-1])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    return bm


def weapons():
    # Одноручный меч: длина досягаемости в игре 1.0 (клинок от 0.02 до 1.0).
    PARTS.append(finish("arming_sword.blade", blade_mesh(0.98, 0.05), ["blade"], subsurf=0, smooth=False))
    guard = box(-0.1, 0.1, -0.015, 0.015, -0.012, 0.018)
    guard = deform(bmesh_subdivide(guard, 3), lambda c: (c.x, c.y, c.z - 0.6 * c.x * c.x))
    grip = lathe([(-0.012, 0.016), (-0.07, 0.018), (-0.12, 0.016)], segs=10)
    pommel = lathe([(-0.12, 0.012), (-0.128, 0.03, 0.012), (-0.152, 0.03, 0.012), (-0.16, 0.012)], segs=16)
    hilt = merge(guard, grip, pommel)
    set_mat(hilt, 1, lambda c: -0.12 < c.z < -0.012)
    PARTS.append(finish("arming_sword.hilt", hilt, ["steel", "leather"], subsurf=1))

    # Двуручный меч: досягаемость 1.5.
    PARTS.append(finish("greatsword.blade", blade_mesh(1.48, 0.062, thick=0.01, tip=0.16, ricasso=0.14), ["blade"], subsurf=0, smooth=False))
    guard = box(-0.16, 0.16, -0.018, 0.018, -0.015, 0.022)
    guard = deform(bmesh_subdivide(guard, 4), lambda c: (c.x, c.y, c.z + 0.5 * c.x * c.x))
    lugs = box(-0.05, 0.05, -0.006, 0.006, 0.1, 0.13)
    grip = lathe([(-0.015, 0.019), (-0.14, 0.021), (-0.27, 0.019)], segs=10)
    pommel = lathe([(-0.27, 0.014), (-0.285, 0.034), (-0.315, 0.032), (-0.33, 0)], segs=16)
    hilt = merge(guard, lugs, grip, pommel)
    set_mat(hilt, 1, lambda c: -0.27 < c.z < -0.015)
    PARTS.append(finish("greatsword.hilt", hilt, ["darksteel", "leather"], subsurf=1))

    # Щит-«утюг»: плоскость XZ, лицо в +Y, выгнут к лицу.
    pts = []
    for i in range(13):
        t = i / 12
        x = -0.28 + 0.56 * t
        pts.append((x, 0.36))
    for i in range(1, 16):
        a = i / 16 * math.pi / 2
        pts.append((0.28 * math.cos(a) ** 0.85, 0.36 - 0.06 - 0.36 * math.sin(a) ** 1.3 * 1.15))
    pts.append((0.0, -0.44))
    for x, z in reversed(pts[13:-1]):
        pts.append((-x, z))
    bm = bmesh.new()
    front = [bm.verts.new((x, 0.0, z)) for x, z in pts]
    bm.faces.new(front)
    bmesh.ops.triangulate(bm, faces=bm.faces[:])
    bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=2, use_grid_fill=True)
    ext = bmesh.ops.extrude_face_region(bm, geom=bm.faces[:])
    for v in ext["geom"]:
        if isinstance(v, bmesh.types.BMVert):
            v.co.y -= 0.03
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    deform(bm, lambda c: (c.x, c.y - 0.9 * c.x * c.x, c.z))
    for f in bm.faces:
        n = f.normal
        f.material_index = 0 if n.y > 0.5 else (1 if n.y < -0.5 else 2)
    obj = finish("shield.body", bm, ["heraldry", "wood", "steel"], subsurf=0)
    # Лицо щита: UV по контуру, чтобы герб лёг целиком.
    me = obj.data
    uv = me.uv_layers.active
    for poly in me.polygons:
        if poly.material_index == 0:
            for li in poly.loop_indices:
                co = me.vertices[me.loops[li].vertex_index].co
                uv.data[li].uv = (co.x / 0.56 + 0.5, (co.z + 0.44) / 0.8)
    PARTS.append(obj)
    boss = lathe([(0.0, 0.05), (0.015, 0.045), (0.028, 0.02), (0.032, 0)], segs=16)
    boss = deform(boss, lambda c: (c.x, c.z + 0.002, c.y + 0.05))
    PARTS.append(finish("shield.boss", boss, ["steel"], subsurf=1))


# --------------------------------------------------------------------------- экспорт и превью


def export():
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    bpy.ops.export_scene.gltf(filepath=OUT, export_format="GLB", use_selection=False, export_yup=True, export_apply=False, export_animations=False, export_cameras=False, export_lights=False)
    print("exported", OUT, os.path.getsize(OUT) // 1024, "KB")


# Наряды: какие слои надеты на каждую кость (как в src/rig.rs).
OUTFITS = {
    "player": {"pelvis": ["body", "gambeson", "belt"], "torso": ["body"], "head": ["body", "kettle_hat"], "upper_arm": ["body"], "forearm": ["body", "glove_cuff"], "hand": ["glove"], "thigh": ["body"], "shin": ["body", "boot_cuff"], "foot": ["boot"]},
    "swordsman": {"pelvis": ["body", "mail", "tabard", "belt"], "torso": ["body", "mail", "tabard"], "head": ["body", "coif", "nasal_helm"], "upper_arm": ["body", "mail"], "forearm": ["body", "gauntlet_cuff"], "hand": ["gauntlet"], "thigh": ["body"], "shin": ["body", "greave"], "foot": ["boot"]},
    "heavy": {"pelvis": ["body", "mail", "faulds", "surcoat"], "torso": ["body", "cuirass", "surcoat"], "head": ["great_helm"], "upper_arm": ["body", "pauldron", "rerebrace"], "forearm": ["body", "vambrace"], "hand": ["gauntlet"], "thigh": ["body", "cuisse"], "shin": ["body", "greave"], "foot": ["sabaton"]},
}
TINTS = {"player": {"gambeson": (0.55, 0.45, 0.32), "cloth": (0.3, 0.28, 0.24)}, "swordsman": {"tabard": (0.12, 0.2, 0.5), "cloth": (0.25, 0.22, 0.2)}, "heavy": {"tabard": (0.45, 0.06, 0.05), "cloth": (0.2, 0.18, 0.16)}}


def tinted(mat_name, tint):
    key = f"{mat_name}_{tint}"
    if key in bpy.data.materials:
        return bpy.data.materials[key]
    m = MATS[mat_name].copy()
    m.name = key
    nt = m.node_tree
    bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
    tex = next(n for n in nt.nodes if n.type == "TEX_IMAGE" and n.outputs["Color"].links and n.outputs["Color"].links[0].to_socket == bsdf.inputs["Base Color"])
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs["Factor"].default_value = 1.0
    nt.links.new(tex.outputs["Color"], mix.inputs[6])
    mix.inputs[7].default_value = (*tint, 1)
    nt.links.new(mix.outputs[2], bsdf.inputs["Base Color"])
    return m


def preview(path):
    """Расставляет три персонажа в позах и рендерит картинку (Cycles, CPU)."""
    by_name = {o.name: o for o in PARTS}
    scene = bpy.context.scene
    for o in PARTS:
        o.hide_render = True
    poses = {
        "player": {"upper_arm_r": (0.9, 0, 0.2), "forearm_r": (1.2, 0, 0), "upper_arm_l": (0.1, 0, -0.15), "forearm_l": (0.3, 0, 0), "thigh_l": (0.3, 0, 0), "shin_l": (-0.4, 0, 0), "thigh_r": (-0.15, 0, 0)},
        "swordsman": {"upper_arm_r": (0.4, 0, 0.25), "forearm_r": (1.4, 0, 0), "upper_arm_l": (0.6, 0, -0.5), "forearm_l": (1.5, 0, 0), "thigh_r": (0.2, 0, 0), "thigh_l": (-0.2, 0, 0)},
        "heavy": {"upper_arm_r": (1.0, 0, 0.3), "forearm_r": (0.9, 0, 0), "upper_arm_l": (1.0, 0, -0.4), "forearm_l": (1.0, 0, 0), "torso": (0, 0, 0.2), "thigh_l": (0.25, 0, 0), "shin_l": (-0.3, 0, 0)},
    }
    hierarchy = {"pelvis": None, "torso": "pelvis", "head": "torso", "upper_arm_r": "torso", "upper_arm_l": "torso", "forearm_r": "upper_arm_r", "forearm_l": "upper_arm_l", "hand_r": "forearm_r", "hand_l": "forearm_l", "thigh_r": "pelvis", "thigh_l": "pelvis", "shin_r": "thigh_r", "shin_l": "thigh_l", "foot_r": "shin_r", "foot_l": "shin_l"}

    def joint(bone):
        base = bone[:-2] if bone.endswith(("_r", "_l")) else bone
        x, y, z = JOINTS[base]
        return base, Vector((-x if bone.endswith("_l") else x, y, z))

    def place(name, matrix, tint):
        src = by_name.get(name)
        if src is None:
            return
        o = src.copy()
        o.data = src.data.copy()
        o.hide_render = False
        scene.collection.objects.link(o)
        for i, m in enumerate(o.data.materials):
            if m.name in tint:
                o.data.materials[i] = tinted(m.name, tint[m.name])
        o.matrix_world = matrix

    for ci, (who, outfit) in enumerate(OUTFITS.items()):
        offset = Vector(((ci - 1) * 1.1, 0, 0))
        world = {}
        for bone, parent in hierarchy.items():
            base, pos = joint(bone)
            rot = Euler(poses[who].get(bone, (0, 0, 0))).to_matrix().to_4x4()
            if parent is None:
                m = Matrix.Translation(pos + offset) @ Euler((0, 0, [float(os.environ.get("PYAW", "0.0")), 0.15, -0.4][ci])).to_matrix().to_4x4() @ rot
            else:
                m = world[parent] @ Matrix.Translation(pos - joint(parent)[1]) @ rot
            world[bone] = m
            for layer in outfit[base]:
                place(f"{bone}.{layer}", m, TINTS[who])
        weapon = "greatsword" if who == "heavy" else "arming_sword"
        grip = world["hand_r"] @ Matrix.Translation((0, 0.0, -0.06)) @ Euler((-1.4, 0, 0)).to_matrix().to_4x4()
        for piece in ("blade", "hilt"):
            place(f"{weapon}.{piece}", grip, {})
        if who == "swordsman":
            sh = world["forearm_l"] @ Matrix.Translation((0.0, 0.08, -0.14)) @ Euler((-1.5, 0, 0)).to_matrix().to_4x4()
            for piece in ("body", "boss"):
                place(f"shield.{piece}", sh, {})
    bpy.ops.mesh.primitive_plane_add(size=20)
    ground = bpy.context.active_object
    gm = bpy.data.materials.new("ground")
    gm.diffuse_color = (0.3, 0.28, 0.25, 1)
    bsdf = next(n for n in gm.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    bsdf.inputs["Base Color"].default_value = (0.25, 0.23, 0.2, 1)
    ground.data.materials.append(gm)
    cam = bpy.data.objects.new("cam", bpy.data.cameras.new("cam"))
    scene.collection.objects.link(cam)
    cam.location = (0.0, 5.2, 1.05)
    cam.rotation_euler = (math.radians(89), 0, math.radians(180))
    cam.data.lens = 42
    if os.environ.get("CLOSE"):
        back = os.environ.get("BACK") is not None
        cam.location = (-1.1 + float(os.environ["CLOSE"]), -1.2 if back else 1.2, 1.62)
        cam.rotation_euler = (math.radians(90), 0, 0 if back else math.radians(180))
        cam.data.lens = 60
    scene.camera = cam
    for loc, energy, size in [((2.5, 3.5, 4.0), 900, 2.0), ((-3.0, 2.0, 2.5), 300, 3.0), ((0.0, -3.0, 3.0), 600, 1.5)]:
        light = bpy.data.objects.new("l", bpy.data.lights.new("l", "AREA"))
        light.data.energy = energy
        light.data.size = size
        light.location = loc
        d = Vector((0, 0, 1.0)) - Vector(loc)
        light.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()
        scene.collection.objects.link(light)
    world = bpy.data.worlds.new("w")
    scene.world = world
    world.color = (0.25, 0.25, 0.28)
    scene.render.engine = "CYCLES"
    scene.cycles.samples = int(os.environ.get("SAMPLES", "48"))
    scene.cycles.device = "CPU"
    scene.render.resolution_x = int(os.environ.get("W", "1280"))
    scene.render.resolution_y = int(os.environ.get("H", "800"))
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    print("preview", path)


def main():
    reset_scene()
    make_materials()
    body()
    gear()
    weapons()
    export()
    if "--preview" in sys.argv:
        preview(os.path.abspath(sys.argv[sys.argv.index("--preview") + 1]))


if __name__ == "__main__":
    main()
