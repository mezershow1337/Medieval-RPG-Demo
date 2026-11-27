"""Генератор деревьев и кустов Таврики для игры (запускается в Blender на ПК через BlenderMCP).

Дерево = ствол и ветви (трубки с корой Bark014, ambientCG CC0) + листва-карточки с атласом
веточек (атлас отрендерен из фотоскана Poly Haven island_tree_02 (CC0) и из процедурных
пучков хвои/чешуйчатых веточек, см. build_atlases.py) + дальний «импостор» (две скрещённые
плоскости с видом дерева сбоку).

В каждом .glb три меша на LOD: `<id>_lod0_bark`, `<id>_lod0_leaf`, `<id>_lod1_bark`,
`<id>_lod1_leaf`, `<id>_imp`. Второй UV-канал (TEXCOORD_1): x — сила качания на ветру
(0 у корня, 1 на концах ветвей), y — фаза. Материалы в игре свои (src/vegetation.rs),
из glb берутся только меши.

Запуск: exec этого файла в Blender, затем `build_all(OUT_DIR)`.
"""
import bmesh
import bpy
import math
import os
import random
from mathutils import Matrix, Quaternion, Vector

ATLAS_DIR = os.environ.get("MRPG_VEG_ATLAS", r"E:\Downloads\mrpg_veg\atlas")
BARK_DIR = os.environ.get("MRPG_VEG_BARK", r"E:\Downloads\mrpg_veg\bark")
# Размер картинки импостора: по правилу проекта текстуры 1024×1024.
IMP_SIZE = 1024


# Множитель цвета коры (линейный); тот же стоит в src/vegetation.rs (BARK_TINT).
BARK_TINT = (0.42, 0.36, 0.31)


class Species:
    def __init__(self, **kw):
        self.__dict__.update(kw)


# Высоты — в метрах; в игре модель масштабируется под нужную высоту.
SPECIES = {
    # Сосна крымская (Pinus nigra subsp. pallasiana): прямой ствол, мутовки ветвей,
    # к старости зонтичная крона; хвоя пучками на концах побегов.
    "pine": Species(height=11.0, trunk_r=0.26, levels=3, children=(10, 5, 3), crown_start=0.45,
                    angle=(78, 50, 40), length=(0.36, 0.45, 0.5), up=0.12, gravity=0.05, wobble=0.12,
                    atlas="pine", card=1.3, cards_per_tip=5, whorl=True, taper=0.85, multi_stem=1),
    # Дуб пушистый (Quercus pubescens): узловатый ствол, широкая раскидистая крона.
    "oak": Species(height=8.0, trunk_r=0.34, levels=3, children=(6, 4, 3), crown_start=0.25,
                   angle=(55, 45, 40), length=(0.6, 0.5, 0.45), up=0.25, gravity=0.06, wobble=0.5,
                   atlas="broadleaf", card=1.5, cards_per_tip=3, whorl=False, taper=0.75, multi_stem=1),
    # Можжевельник высокий (Juniperus excelsa): многоствольный, плотный, неправильный конус.
    "juniper": Species(height=4.0, trunk_r=0.12, levels=2, children=(9, 5), crown_start=0.08,
                       angle=(35, 38), length=(0.6, 0.45), up=0.5, gravity=0.02, wobble=0.25,
                       atlas="scale", card=1.0, cards_per_tip=3, whorl=False, taper=0.7, multi_stem=3),
    # Кипарис вечнозелёный: узкая колонна.
    "cypress": Species(height=9.0, trunk_r=0.18, levels=2, children=(26, 4), crown_start=0.06,
                       angle=(22, 25), length=(0.22, 0.35), up=0.85, gravity=0.0, wobble=0.05,
                       atlas="scale", card=1.0, cards_per_tip=3, whorl=False, taper=0.9, multi_stem=1, narrow=True),
    # Смоковница (инжир): низкая, многоствольная, широкая крона.
    "fig": Species(height=4.5, trunk_r=0.14, levels=3, children=(4, 4, 3), crown_start=0.2,
                   angle=(45, 45, 40), length=(0.6, 0.5, 0.45), up=0.35, gravity=0.04, wobble=0.3,
                   atlas="broadleaf", card=1.3, cards_per_tip=3, whorl=False, taper=0.7, multi_stem=3),
    # Кустарник шибляка (шиповник, боярышник, держидерево): 1–2 м.
    "shrub": Species(height=1.7, trunk_r=0.04, levels=2, children=(7, 4), crown_start=0.0,
                     angle=(45, 40), length=(0.7, 0.45), up=0.4, gravity=0.03, wobble=0.4,
                     atlas="broadleaf", card=0.85, cards_per_tip=3, whorl=False, taper=0.6, multi_stem=5),
}


class Branch:
    def __init__(self, pts, radii, level, parent_t=0.0):
        self.pts = pts
        self.radii = radii
        self.level = level
        self.children = []


def _rot_towards(v, target, amount):
    v = v.normalized()
    t = target.normalized()
    if amount <= 0:
        return v
    return (v.lerp(t, amount)).normalized()


def grow(sp, rnd):
    """Скелет дерева: список ветвей (полилиния, радиусы, уровень)."""
    branches = []
    H = sp.height

    def make_branch(origin, direction, length, radius, level, sway0):
        segs = max(3, int(length / (0.35 if level == 0 else 0.25)))
        segs = min(segs, 14)
        pts = [origin]
        radii = [radius]
        d = direction.normalized()
        for i in range(1, segs + 1):
            t = i / segs
            jitter = Vector((rnd.uniform(-1, 1), rnd.uniform(-1, 1), rnd.uniform(-1, 1))) * sp.wobble * 0.6
            d = (d + jitter * (1.0 / segs) * 3.0).normalized()
            d = _rot_towards(d, Vector((0, 0, 1)), sp.up * 0.12 if level > 0 else 0.05)
            d = (d + Vector((0, 0, -sp.gravity * t))).normalized()
            pts.append(pts[-1] + d * (length / segs))
            radii.append(max(radius * (1.0 - t * sp.taper), 0.006))
        b = Branch(pts, radii, level)
        b.sway = [sway0 + (1.0 - sway0) * (i / segs) ** 1.5 * (0.35 if level == 0 else 1.0) for i in range(segs + 1)]
        b.length = length
        branches.append(b)
        return b

    def spawn_children(b, level):
        if level >= sp.levels:
            b.tip = True
            return
        b.tip = False
        n = sp.children[level]
        if getattr(sp, "narrow", False) and level == 0:
            n = sp.children[0]
        start = sp.crown_start if level == 0 else 0.2
        whorl_phase = rnd.uniform(0, math.tau)
        for i in range(n):
            if sp.whorl and level == 0:
                # Мутовки: по 3–4 ветви на одной высоте.
                tier = i // 3
                tiers = max(1, (n + 2) // 3)
                t = start + (1 - start) * (tier + rnd.uniform(0.0, 0.3)) / tiers
                az = whorl_phase + (i % 3) * math.tau / 3 + tier * 0.9
            else:
                t = start + (1 - start) * (i + rnd.uniform(0.2, 0.8)) / n
                az = whorl_phase + i * 2.39996 + rnd.uniform(-0.3, 0.3)
            t = min(t, 0.97)
            k = t * (len(b.pts) - 1)
            i0 = int(k)
            i1 = min(i0 + 1, len(b.pts) - 1)
            f = k - i0
            p = b.pts[i0].lerp(b.pts[i1], f)
            r_par = b.radii[i0] * (1 - f) + b.radii[i1] * f
            axis = (b.pts[i1] - b.pts[i0]).normalized()
            side = axis.orthogonal().normalized()
            side = Quaternion(axis, az) @ side
            ang = math.radians(sp.angle[level] + rnd.uniform(-10, 10))
            d = (axis * math.cos(ang) + side * math.sin(ang)).normalized()
            # Ветви выше по стволу короче (конус кроны) — у сосны сильнее.
            shape = 1.0 - (t - start) / max(1e-3, 1 - start) * (0.55 if level == 0 else 0.3)
            if getattr(sp, "narrow", False) and level == 0:
                shape = 0.5 + 0.5 * math.sin(min(1.0, (t - start) / (1 - start)) * math.pi) ** 0.7
            L = b.length * sp.length[level] * shape * rnd.uniform(0.8, 1.15)
            r = min(r_par * 0.7, max(0.008, r_par * (0.55 if level == 0 else 0.6)))
            sway0 = b.sway[i0]
            c = make_branch(p - axis * r_par * 0.2, d, max(L, 0.25), r, level + 1, sway0)
            b.children.append(c)
            spawn_children(c, level + 1)

    stems = sp.multi_stem
    for s in range(stems):
        if stems == 1:
            d = Vector((rnd.uniform(-0.04, 0.04), rnd.uniform(-0.04, 0.04), 1))
            o = Vector((0, 0, -0.15))
            h = H * 0.95
            r = sp.trunk_r
        else:
            az = s * math.tau / stems + rnd.uniform(-0.4, 0.4)
            lean = rnd.uniform(0.15, 0.4)
            d = Vector((math.cos(az) * lean, math.sin(az) * lean, 1))
            o = Vector((math.cos(az) * sp.trunk_r * 0.6, math.sin(az) * sp.trunk_r * 0.6, -0.1))
            h = H * rnd.uniform(0.75, 0.95)
            r = sp.trunk_r * rnd.uniform(0.7, 1.0)
        trunk = make_branch(o, d, h, r, 0, 0.0)
        spawn_children(trunk, 0)
    return branches


def tube(bm, uv0, uv1, b, sides, cut_level):
    """Трубка вдоль полилинии ветви. UV0 — кора (метры), UV1 — (качание, фаза)."""
    if b.level > cut_level:
        return
    rings = []
    vacc = 0.0
    phase = (hash(tuple(round(c, 2) for c in b.pts[0])) % 1000) / 1000.0
    for i, p in enumerate(b.pts):
        if i == 0:
            d = b.pts[1] - b.pts[0]
        elif i == len(b.pts) - 1:
            d = b.pts[i] - b.pts[i - 1]
        else:
            d = b.pts[i + 1] - b.pts[i - 1]
        d.normalize()
        side = d.orthogonal().normalized() if i == 0 else (rings[-1][1] - (rings[-1][1].dot(d)) * d).normalized()
        if i > 0:
            vacc += (b.pts[i] - b.pts[i - 1]).length
        ring = []
        for k in range(sides):
            a = k / sides * math.tau
            n = (Quaternion(d, a) @ side).normalized()
            ring.append(bm.verts.new(p + n * b.radii[i]))
        rings.append((ring, side, vacc, b.sway[i]))
    circ = max(0.15, 2 * math.pi * b.radii[0])
    for i in range(len(rings) - 1):
        ra, _, va, sa = rings[i]
        rb, _, vb, sb = rings[i + 1]
        for k in range(sides):
            k2 = (k + 1) % sides
            f = bm.faces.new((ra[k], ra[k2], rb[k2], rb[k]))
            us = [k / sides, (k + 1) / sides, (k + 1) / sides, k / sides]
            vs = [va, va, vb, vb]
            ss = [sa, sa, sb, sb]
            for l, u, v, s in zip(f.loops, us, vs, ss):
                l[uv0].uv = (u * circ / 0.8, v / 0.8)
                l[uv1].uv = (s, phase)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])


def leaf_cards(bm, uv0, uv1, branches, sp, rnd, density, size_mul, crown_c):
    """Карточки листвы на концевых ветвях. Нормали — «сферические» от центра кроны (мягкий свет)."""
    nrm = {}
    tips = [b for b in branches if getattr(b, "tip", False)]
    # Верхушка ствола тоже в хвое/листве — иначе из кроны торчит голый шест.
    tips += [b for b in branches if b.level == 0 and not getattr(b, "tip", False)]
    for b in tips:
        trunk = b.level == 0
        n_cards = max(1, int(round(sp.cards_per_tip * density * rnd.uniform(0.8, 1.2) * (2.0 if trunk else 1 + b.length / 2.0))))
        for c in range(n_cards):
            if trunk:
                t = 1.0 - rnd.random() * 0.12
            else:
                t = 1.0 - (c / max(1, n_cards)) * 0.75 * rnd.uniform(0.7, 1.0)
            k = t * (len(b.pts) - 1)
            i0 = int(k)
            i1 = min(i0 + 1, len(b.pts) - 1)
            p = b.pts[i0].lerp(b.pts[i1], k - i0)
            axis = (b.pts[i1] - b.pts[i0]).normalized()
            s = sp.card * size_mul * rnd.uniform(0.8, 1.15)
            # Карточка вытянута вдоль ветви, повернута вокруг неё случайно.
            out = (p - crown_c)
            out.z *= 0.4
            out = out.normalized() if out.length > 1e-3 else Vector((1, 0, 0))
            up_dir = (axis * 0.6 + out * 0.3 + Vector((0, 0, 0.35))).normalized()
            side = up_dir.cross(Vector((0, 0, 1)))
            if side.length < 1e-3:
                side = Vector((1, 0, 0))
            side = Quaternion(up_dir, rnd.uniform(-1.2, 1.2)) @ side.normalized()
            base = p - up_dir * s * 0.15
            corners = [base - side * s / 2, base + side * s / 2, base + side * s / 2 + up_dir * s, base - side * s / 2 + up_dir * s]
            verts = [bm.verts.new(v) for v in corners]
            f = bm.faces.new(verts)
            cell = rnd.randrange(4)
            cx, cy = (cell % 2) * 0.5, (1 - cell // 2) * 0.5
            flip = rnd.random() < 0.5
            uvs = [(0, 0), (1, 0), (1, 1), (0, 1)]
            if flip:
                uvs = [(1, 0), (0, 0), (0, 1), (1, 1)]
            sway = b.sway[-1]
            phase = rnd.random()
            for l, (u, v) in zip(f.loops, uvs):
                l[uv0].uv = (cx + u * 0.5, cy + v * 0.5)
                l[uv1].uv = (sway, phase)
            for v in verts:
                sn = (v.co - crown_c)
                sn.z = sn.z * 0.7 + 0.25 * sn.length
                nrm[v.index if v.index >= 0 else id(v)] = sn.normalized()
    return nrm


def build_mesh(name, bm, spherical=None, crown_c=None):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    if spherical is not None:
        # Сферические нормали для листвы: от центра кроны.
        normals = []
        for lp in me.loops:
            v = me.vertices[lp.vertex_index].co
            sn = (v - crown_c)
            sn.z = sn.z * 0.7 + 0.25 * sn.length
            normals.append(sn.normalized())
        me.normals_split_custom_set(normals)
    return me


def make_tree(tid, sp, seed):
    rnd = random.Random(seed)
    branches = grow(sp, rnd)
    allpts = [p for b in branches if b.level > 0 for p in b.pts] or [p for b in branches for p in b.pts]
    crown_c = sum(allpts, Vector()) / len(allpts)
    meshes = {}
    for lod, (sides0, cut, dens, size) in enumerate([(8, 99, 1.0, 1.0), (5, sp.levels - 1, 0.45, 1.45)]):
        bm = bmesh.new()
        uv0 = bm.loops.layers.uv.new("UVMap")
        uv1 = bm.loops.layers.uv.new("Wind")
        for b in branches:
            sides = max(3, sides0 - b.level * 2)
            if b.radii[0] < 0.02:
                sides = 3
            tube(bm, uv0, uv1, b, sides, cut)
        meshes[f"{tid}_lod{lod}_bark"] = build_mesh(f"{tid}_lod{lod}_bark", bm)
        bm = bmesh.new()
        uv0 = bm.loops.layers.uv.new("UVMap")
        uv1 = bm.loops.layers.uv.new("Wind")
        rnd_cards = random.Random(seed * 7 + lod)
        leaf_cards(bm, uv0, uv1, branches, sp, rnd_cards, dens, size, crown_c)
        meshes[f"{tid}_lod{lod}_leaf"] = build_mesh(f"{tid}_lod{lod}_leaf", bm, spherical=True, crown_c=crown_c)
    return meshes, crown_c


def materials():
    """Материалы для превью/рендера импостора (в игре материалы свои)."""
    def img(path, srgb=True):
        im = bpy.data.images.load(path, check_existing=True)
        if not srgb:
            im.colorspace_settings.name = "Non-Color"
        return im

    out = {}
    bark = bpy.data.materials.get("mrpg_bark") or bpy.data.materials.new("mrpg_bark")
    bark.use_nodes = True
    nt = bark.node_tree
    nt.nodes.clear()
    o = nt.nodes.new("ShaderNodeOutputMaterial")
    p = nt.nodes.new("ShaderNodeBsdfPrincipled")
    t = nt.nodes.new("ShaderNodeTexImage")
    t.image = img(os.path.join(BARK_DIR, "Bark014_1K-JPG_Color.jpg"))
    nm = nt.nodes.new("ShaderNodeNormalMap")
    tn = nt.nodes.new("ShaderNodeTexImage")
    tn.image = img(os.path.join(BARK_DIR, "Bark014_1K-JPG_NormalGL.jpg"), False)
    # Bark014 светлый: приглушаем до серо-бурой коры (в игре — тот же множитель).
    mul = nt.nodes.new("ShaderNodeMix")
    mul.data_type = "RGBA"
    mul.blend_type = "MULTIPLY"
    mul.inputs[0].default_value = 1.0
    mul.inputs[7].default_value = (*BARK_TINT, 1.0)
    nt.links.new(t.outputs[0], mul.inputs[6])
    nt.links.new(mul.outputs[2], p.inputs["Base Color"])
    nt.links.new(tn.outputs[0], nm.inputs["Color"])
    nt.links.new(nm.outputs[0], p.inputs["Normal"])
    p.inputs["Roughness"].default_value = 0.9
    nt.links.new(p.outputs[0], o.inputs[0])
    out["bark"] = bark
    for a in ["broadleaf", "pine", "scale"]:
        m = bpy.data.materials.get(f"mrpg_leaf_{a}") or bpy.data.materials.new(f"mrpg_leaf_{a}")
        m.use_nodes = True
        nt = m.node_tree
        nt.nodes.clear()
        o = nt.nodes.new("ShaderNodeOutputMaterial")
        p = nt.nodes.new("ShaderNodeBsdfPrincipled")
        t = nt.nodes.new("ShaderNodeTexImage")
        t.image = img(os.path.join(ATLAS_DIR, f"{a}_atlas.png"))
        nt.links.new(t.outputs[0], p.inputs["Base Color"])
        nt.links.new(t.outputs[1], p.inputs["Alpha"])
        p.inputs["Roughness"].default_value = 0.8
        nt.links.new(p.outputs[0], o.inputs[0])
        try:
            m.surface_render_method = "DITHERED"
        except Exception:
            pass
        out[a] = m
    return out


def impostor(tid, meshes, mats, sp, out_dir, scene):
    """Вид дерева сбоку (цвет + альфа) для дальнего LOD: две скрещённые плоскости."""
    for ob in list(scene.objects):
        bpy.data.objects.remove(ob, do_unlink=True)
    objs = []
    for name in (f"{tid}_lod0_bark", f"{tid}_lod0_leaf"):
        me = meshes[name]
        me.materials.clear()
        me.materials.append(mats["bark"] if name.endswith("bark") else mats[sp.atlas])
        ob = bpy.data.objects.new(name, me)
        scene.collection.objects.link(ob)
        objs.append(ob)
    pts = [ob.matrix_world @ v.co for ob in objs for v in ob.data.vertices]
    mn = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    mx = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    w = max(mx.x - mn.x, mx.y - mn.y)
    h = mx.z - max(mn.z, 0.0)
    size = max(w, h)
    cam = bpy.data.objects.new("imp_cam", bpy.data.cameras.new("imp_cam"))
    scene.collection.objects.link(cam)
    cam.data.type = "ORTHO"
    cam.data.ortho_scale = size
    cz = max(mn.z, 0.0) + size / 2
    cam.location = Vector((0, -50, cz))
    cam.rotation_euler = (math.pi / 2, 0, 0)
    scene.camera = cam
    sun = bpy.data.objects.new("imp_sun", bpy.data.lights.new("imp_sun", "SUN"))
    scene.collection.objects.link(sun)
    sun.data.energy = 2.5
    sun.rotation_euler = (0.9, 0.0, 0.5)
    if not scene.world:
        scene.world = bpy.data.worlds.new("imp_world")
    scene.world.use_nodes = True
    scene.world.node_tree.nodes["Background"].inputs[0].default_value = (0.6, 0.65, 0.75, 1)
    scene.world.node_tree.nodes["Background"].inputs[1].default_value = 1.2
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 32
    scene.render.film_transparent = True
    scene.render.resolution_x = IMP_SIZE
    scene.render.resolution_y = IMP_SIZE
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.view_settings.view_transform = "Standard"
    path = os.path.join(out_dir, f"{tid}_imp.png")
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True, scene=scene.name)
    # Две скрещённые плоскости размером size × size, низ на земле (z = cz - size/2).
    bm = bmesh.new()
    uv0 = bm.loops.layers.uv.new("UVMap")
    uv1 = bm.loops.layers.uv.new("Wind")
    z0 = cz - size / 2
    for ang in (0.0, math.pi / 2):
        d = Vector((math.cos(ang), math.sin(ang), 0)) * size / 2
        vs = [bm.verts.new(c) for c in (-d + Vector((0, 0, z0)), d + Vector((0, 0, z0)), d + Vector((0, 0, z0 + size)), -d + Vector((0, 0, z0 + size)))]
        f = bm.faces.new(vs)
        for l, uv in zip(f.loops, [(0, 0), (1, 0), (1, 1), (0, 1)]):
            l[uv0].uv = uv
            l[uv1].uv = (uv[1] * 0.3, 0.0)
    me = bpy.data.meshes.new(f"{tid}_imp")
    bm.to_mesh(me)
    bm.free()
    # Нормали плоскостей — вверх-наружу, чтобы освещение не мигало при повороте.
    me.normals_split_custom_set([Vector((0, 0, 1)) for _ in me.loops])
    # Сначала отвязываем камеру от сцены: иначе экспорт glTF падает на висячей ссылке.
    scene.camera = None
    for ob in list(scene.objects):
        bpy.data.objects.remove(ob, do_unlink=True)
    return me, path


def export(tid, meshes, out_dir, scene):
    for ob in list(scene.objects):
        bpy.data.objects.remove(ob, do_unlink=True)
    for name, me in meshes.items():
        me.materials.clear()
        ob = bpy.data.objects.new(name, me)
        scene.collection.objects.link(ob)
    path = os.path.join(out_dir, f"{tid}.glb")
    with bpy.context.temp_override(scene=scene, view_layer=scene.view_layers[0]):
        bpy.ops.export_scene.gltf(filepath=path, export_format="GLB", use_selection=False, export_materials="NONE",
                                  export_yup=True, export_animations=False, use_active_scene=True)
    for ob in list(scene.objects):
        bpy.data.objects.remove(ob, do_unlink=True)
    return path


VARIANTS = {"pine": 3, "oak": 3, "juniper": 3, "cypress": 2, "fig": 2, "shrub": 3}


def build_all(out_dir, only=None):
    os.makedirs(out_dir, exist_ok=True)
    scene = bpy.data.scenes.get("mrpg_trees") or bpy.data.scenes.new("mrpg_trees")
    mats = materials()
    report = {}
    for key, sp in SPECIES.items():
        if only and key not in only:
            continue
        for v in range(VARIANTS[key]):
            tid = f"{key}_{v}"
            meshes, _ = make_tree(tid, sp, seed=1000 + 97 * v + len(key) * 13)
            imp, _ = impostor(tid, meshes, mats, sp, out_dir, scene)
            meshes[f"{tid}_imp"] = imp
            export(tid, meshes, out_dir, scene)
            report[tid] = {n: len(m.polygons) for n, m in meshes.items()}
    return report
