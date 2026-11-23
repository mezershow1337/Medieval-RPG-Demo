"""Византийская черепица Херсонеса: тегулы (плоские, с бортиками) и калиптеры (полукруглые
над стыками) — моделируется и запекается в Blender (проверено на 5.3).

Запуск: в Blender (Scripting → Run Script, или через BlenderMCP), либо
    blender --background --python tools/textures/blender_byz_roof.py -- <папка_вывода>
Пишет MrpgByzRoof_Color.png, _NormalGL.png, _Roughness.png, _AmbientOcclusion.png (1024²),
дальше их забирает tools/textures/build_surface_textures.py (слой "roof").

Тайл — 1.5×1.5 м: 3 ряда по скату × 3 колонны. Геометрия выходит за края тайла и повторяется
с тем же «случайным» разбросом по модулю 3, поэтому текстура бесшовная. Верх картинки — конёк,
низ — карниз (в шейдере v растёт вниз по скату). Всё своё, без чужих ассетов.
"""
import bpy, bmesh, math, os, random, sys, time

OUT = globals().get("MRPG_OUT") or (sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else os.path.join(os.path.expanduser("~"), "mrpg_byz"))
os.makedirs(OUT, exist_ok=True)
SIZE = 1024
PERIOD = 1.5
PITCH = 0.5   # шаг колонн поперёк ската и рядов по скату (тегула 0.58 м, нахлёст 0.08)
N = 3

sc = bpy.data.scenes.get("mrpg_roof") or bpy.data.scenes.new("mrpg_roof")
for o in list(sc.collection.all_objects):
    bpy.data.objects.remove(o, do_unlink=True)
if bpy.context.window:
    bpy.context.window.scene = sc


def rnd(i, j, k):
    return random.Random((i % N) * 7919 + (j % N) * 104729 + k * 31)


def box(bm, x0, x1, y0, y1, z0, z1):
    vs = [bm.verts.new((x, y, z)) for z in (z0, z1) for y in (y0, y1) for x in (x0, x1)]
    for f in [(0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)]:
        bm.faces.new([vs[i] for i in f])


def tegula():
    bm = bmesh.new()
    w, L, t = 0.43, 0.58, 0.022
    box(bm, -w / 2, w / 2, -L / 2, L / 2, 0, t)
    for s in (-1, 1):  # бортики вдоль длинных сторон
        box(bm, s * w / 2 - (0.035 if s > 0 else 0), s * w / 2 + (0.0 if s > 0 else 0.035), -L / 2 + 0.04, L / 2, t, t + 0.045)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new("tegula")
    bm.to_mesh(me)
    bm.free()
    return me


def imbrex(r0=0.085, r1=0.1, L=0.56, th=0.016, seg=24):
    """Полутруба, к карнизу (−Y) шире — верхний калиптер входит в нижний."""
    bm = bmesh.new()
    rings = []
    for k, y in enumerate((-L / 2, L / 2)):
        r = r1 if k == 0 else r0
        ring = []
        for a in range(seg + 1):
            ang = math.pi * a / seg
            for rr in (r, r - th):
                ring.append(bm.verts.new((math.cos(ang) * rr, y, math.sin(ang) * rr * 0.95)))
        rings.append(ring)
    A, B = rings
    for a in range(seg):
        o0, i0, o1, i1 = 2 * a, 2 * a + 1, 2 * a + 2, 2 * a + 3
        bm.faces.new([A[o0], A[o1], B[o1], B[o0]])
        bm.faces.new([A[i0], B[i0], B[i1], A[i1]])
    for R in (A, B):
        for a in range(seg):
            bm.faces.new([R[2 * a], R[2 * a + 1], R[2 * a + 3], R[2 * a + 2]])
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    me = bpy.data.meshes.new("imbrex")
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = True
    return me


PHOTO = globals().get("MRPG_PHOTO") or os.path.join(os.path.dirname(OUT.rstrip("/\\")), "ph", "maps")


def photo(name):
    """Фотоскан Poly Haven (CC0) 2K; если его нет рядом — скачивает через API polyhaven.com.
    mrpg_clay_surface.jpg кладётся туда же заранее (tools/textures/quilt_clay.py)."""
    path = os.path.join(PHOTO, name)
    if not os.path.exists(path):
        import json, urllib.request
        asset, kind = name.rsplit("_", 1)
        kind = kind.split(".")[0]
        key = {"Color": "Diffuse", "Displacement": "Displacement", "Roughness": "Rough"}[kind]
        get = lambda u: urllib.request.urlopen(urllib.request.Request(u, headers={"User-Agent": "mrpg-texture-tool"}), timeout=120).read()
        files = json.loads(get("https://api.polyhaven.com/files/" + asset))[key]["2k"]
        fmt = "png" if "png" in files else "jpg"
        os.makedirs(PHOTO, exist_ok=True)
        path = os.path.join(PHOTO, "%s_%s.%s" % (asset, kind, fmt))
        open(path, "wb").write(get(files[fmt]["url"]))
    im = bpy.data.images.load(path, check_existing=True)
    im.colorspace_settings.name = 'Non-Color' if any(k in name for k in ("Displacement", "Roughness", "Normal")) else 'sRGB'
    return im


def terracotta():
    """Обожжённая глина. Цвета обжига сняты с фотоскана старой черепицы (clay_roof_tiles_02),
    рельеф пор и трещин — карта высот фотоскана глиняной штукатурки (clay_plaster), потёки и
    пятна грязи — маска с фотоскана старой стены (worn_mossy_plasterwall). Плюс грязь в щелях
    (AO), стёртые светлые кромки, накипной лишайник: серый и жёлто-оранжевый (ксантория)."""
    mat = bpy.data.materials.new("terracotta")
    mat.use_nodes = True
    nt = mat.node_tree
    nt.nodes.clear()
    Nn, L = nt.nodes, nt.links
    out = Nn.new("ShaderNodeOutputMaterial")
    bsdf = Nn.new("ShaderNodeBsdfPrincipled")
    L.new(bsdf.outputs[0], out.inputs[0])
    tc = Nn.new("ShaderNodeTexCoord")
    seed = Nn.new("ShaderNodeAttribute")
    seed.attribute_type = 'OBJECT'
    seed.attribute_name = "seed"
    mul = Nn.new("ShaderNodeMath"); mul.operation = 'MULTIPLY'; mul.inputs[1].default_value = 37.0
    L.new(seed.outputs["Fac"], mul.inputs[0])
    comb = Nn.new("ShaderNodeCombineXYZ")
    L.new(mul.outputs[0], comb.inputs[0]); L.new(mul.outputs[0], comb.inputs[1])
    add = Nn.new("ShaderNodeVectorMath"); add.operation = 'ADD'
    L.new(tc.outputs["Object"], add.inputs[0]); L.new(comb.outputs[0], add.inputs[1])
    P = add.outputs[0]

    def math(op, a, b=None):
        m = Nn.new("ShaderNodeMath"); m.operation = op
        for k, v in enumerate((a, b)):
            if v is None:
                continue
            if isinstance(v, (int, float)):
                m.inputs[k].default_value = v
            else:
                L.new(v, m.inputs[k])
        return m.outputs[0]

    def noise(scale, detail=6, rough=0.55, dist=0.0):
        n = Nn.new("ShaderNodeTexNoise")
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = detail
        n.inputs["Roughness"].default_value = rough
        n.inputs["Distortion"].default_value = dist
        L.new(P, n.inputs["Vector"])
        return n

    def map_range(src, a, b, c, d):
        m = Nn.new("ShaderNodeMapRange")
        m.inputs[1].default_value, m.inputs[2].default_value = a, b
        m.inputs[3].default_value, m.inputs[4].default_value = c, d
        L.new(src, m.inputs[0])
        return m.outputs[0]

    def mix(fac, a, b, blend='MIX'):
        m = Nn.new("ShaderNodeMix"); m.data_type = 'RGBA'; m.blend_type = blend
        if isinstance(fac, float):
            m.inputs["Factor"].default_value = fac
        else:
            L.new(fac, m.inputs["Factor"])
        for slot, v in ((6, a), (7, b)):
            if isinstance(v, tuple):
                m.inputs[slot].default_value = (*v, 1)
            else:
                L.new(v, m.inputs[slot])
        return m.outputs[2]

    def image(name, meters):
        """Фото в кубической проекции по координатам плитки (у каждой плитки свой сдвиг)."""
        mp = Nn.new("ShaderNodeMapping")
        mp.inputs["Scale"].default_value = (1 / meters,) * 3
        L.new(P, mp.inputs["Vector"])
        t = Nn.new("ShaderNodeTexImage")
        t.image = photo(name)
        t.projection = 'BOX'
        t.projection_blend = 0.25
        L.new(mp.outputs[0], t.inputs["Vector"])
        return t

    # Сама глина — фото: tools/textures/quilt_clay.py собирает из фотоскана старой черепицы
    # (clay_roof_tiles_02) поверхность без её рисунка; каждая плитка берёт свой участок.
    # Сверху — тон обжига по плитке: темнее/краснее или светлее/палевее (местная глина).
    clay = image("mrpg_clay_surface.jpg", 1.25)
    ramp = Nn.new("ShaderNodeValToRGB")
    cr = ramp.color_ramp
    stops = [(0.0, (0.6, 0.55, 0.55)), (0.3, (0.82, 0.78, 0.76)), (0.6, (1.0, 1.0, 1.0)),
             (0.85, (1.12, 1.06, 1.0)), (1.0, (1.2, 1.3, 1.4))]
    cr.elements[0].position, cr.elements[0].color = stops[0][0], (*stops[0][1], 1)
    cr.elements[1].position, cr.elements[1].color = stops[-1][0], (*stops[-1][1], 1)
    for p, c in stops[1:-1]:
        cr.elements.new(p).color = (*c, 1)
    L.new(seed.outputs["Fac"], ramp.inputs[0])

    height = image("clay_plaster_Displacement.png", 1.0)
    stain = image("worn_mossy_plasterwall_Color.jpg", 1.6)
    st = Nn.new("ShaderNodeRGBToBW"); L.new(stain.outputs["Color"], st.inputs[0])
    grain = noise(160.0, 3, 0.5)
    c = mix(1.0, clay.outputs["Color"], ramp.outputs[0], 'MULTIPLY')
    # Выцветание сверху (солнце и дождь) и потёки грязи по фотомаске.
    geo = Nn.new("ShaderNodeNewGeometry")
    sep = Nn.new("ShaderNodeSeparateXYZ"); L.new(geo.outputs["Normal"], sep.inputs[0])
    up = map_range(sep.outputs[2], 0.5, 0.95, 0.0, 1.0)
    c = mix(math('MULTIPLY', up, 0.15), c, (0.42, 0.24, 0.16))
    grime = map_range(st.outputs[0], 0.5, 0.3, 0.0, 0.3)
    c = mix(grime, c, (0.10, 0.075, 0.055))
    # Грязь и пыль в щелях и под нахлёстом.
    ao = Nn.new("ShaderNodeAmbientOcclusion"); ao.samples = 16; ao.inputs["Distance"].default_value = 0.06
    c = mix(map_range(ao.outputs["AO"], 0.85, 0.35, 0.0, 0.8), c, (0.075, 0.055, 0.04))
    # Стёртые кромки светлее.
    pt = Nn.new("ShaderNodeNewGeometry")
    edge = map_range(pt.outputs["Pointiness"], 0.52, 0.6, 0.0, 0.45)
    c = mix(edge, c, (0.52, 0.27, 0.15))

    # Лишайник — редкие круглые колонии с рваным краем, только на обращённом вверх.
    vor = Nn.new("ShaderNodeTexVoronoi"); vor.inputs["Scale"].default_value = 7.0
    L.new(P, vor.inputs["Vector"])
    vc = Nn.new("ShaderNodeSeparateColor"); L.new(vor.outputs["Color"], vc.inputs[0])
    ln = noise(26.0, 6, 0.7, 0.3)
    rag = math('ADD', vor.outputs["Distance"], math('MULTIPLY', math('SUBTRACT', ln.outputs["Fac"], 0.5), 0.35))
    radius = map_range(vc.outputs[1], 0.0, 1.0, 0.12, 0.3)
    spot = math('LESS_THAN', rag, radius)
    sparse = map_range(vc.outputs[0], 0.1, 0.06, 0.0, 1.0)
    lm = math('MULTIPLY', math('MULTIPLY', spot, sparse), up)
    lichen = mix(map_range(vc.outputs[2], 0.55, 0.6, 0.0, 1.0), (0.29, 0.30, 0.25), (0.52, 0.31, 0.05))
    lichen = mix(1.0, lichen, map_range(ln.outputs["Fac"], 0.3, 0.8, 0.7, 1.15), 'MULTIPLY')
    L.new(mix(lm, c, lichen), bsdf.inputs["Base Color"])

    rough = map_range(grain.outputs["Fac"], 0.0, 1.0, 0.7, 0.9)
    rough = math('MAXIMUM', rough, lm)
    rough = math('MAXIMUM', rough, math('MULTIPLY', grime, 1.3))
    L.new(rough, bsdf.inputs["Roughness"])

    # Рельеф: поры и трещинки с фото, мелкие выбоины, корочка лишайника.
    b0 = Nn.new("ShaderNodeBump"); b0.inputs["Strength"].default_value = 0.6; b0.inputs["Distance"].default_value = 0.004
    L.new(height.outputs["Color"], b0.inputs["Height"])
    pits = noise(220.0, 2, 0.5)
    b1 = Nn.new("ShaderNodeBump"); b1.inputs["Strength"].default_value = 0.2; b1.inputs["Distance"].default_value = 0.002
    L.new(pits.outputs["Fac"], b1.inputs["Height"]); L.new(b0.outputs[0], b1.inputs["Normal"])
    b2 = Nn.new("ShaderNodeBump"); b2.inputs["Strength"].default_value = 0.6; b2.inputs["Distance"].default_value = 0.003
    L.new(lm, b2.inputs["Height"]); L.new(b1.outputs[0], b2.inputs["Normal"])
    L.new(b2.outputs[0], bsdf.inputs["Normal"])
    return mat


teg_me, imb_me, mat = tegula(), imbrex(), terracotta()
teg_me.materials.append(mat)
imb_me.materials.append(mat)
objs = []
for j in range(-2, N + 2):
    for i in range(-2, N + 2):
        r = rnd(i, j, 1)
        x, y = i * PITCH + PITCH / 2, j * PITCH + PITCH / 2
        seed = r.random()
        o = bpy.data.objects.new("teg_%d_%d" % (i, j), teg_me)
        # Нижний край тегулы лежит на тегуле ниже — наклон, виден уступ ряда.
        o.location = (x + r.uniform(-0.01, 0.01), y + r.uniform(-0.012, 0.012), 0.0)
        o.rotation_euler = (-math.atan2(0.05, 0.58) + (seed - 0.5) * 0.02, 0.0, (seed - 0.5) * 0.012)
        o["seed"] = seed
        objs.append(o)
        r2 = rnd(i, j, 2)
        seed2 = r2.random()
        o = bpy.data.objects.new("imb_%d_%d" % (i, j), imb_me)
        o.location = (x + PITCH / 2 + r2.uniform(-0.012, 0.012), y + 0.02, 0.045)
        o.rotation_euler = (-math.atan2(0.07, 0.58) + (seed2 - 0.5) * 0.03, (seed2 - 0.5) * 0.02, (seed2 - 0.5) * 0.016)
        o["seed"] = seed2
        objs.append(o)
# Черепица ручной формовки: каждая чуть покороблена при обжиге, кромки неровные, с выбоинами.
# Рельеф берётся в координатах «якоря», сдвинутого на seed, — у повторов по модулю 3 он
# одинаковый, и текстура остаётся бесшовной.
warp_tex = bpy.data.textures.new("warp", 'CLOUDS'); warp_tex.noise_scale = 0.25
chip_tex = bpy.data.textures.new("chip", 'VORONOI'); chip_tex.noise_scale = 0.035
for o in objs:
    sc.collection.objects.link(o)
    anchor = bpy.data.objects.new(o.name + "_anchor", None)
    anchor.location = (o.location.x - o["seed"] * 17.0, o.location.y - o["seed"] * 23.0, o.location.z)
    sc.collection.objects.link(anchor)
    m = o.modifiers.new("bevel", "BEVEL"); m.width = 0.005; m.segments = 2; m.limit_method = 'ANGLE'
    s = o.modifiers.new("dense", "SUBSURF"); s.subdivision_type = 'SIMPLE'; s.levels = s.render_levels = 3
    for tex, k in ((warp_tex, 0.006), (chip_tex, -0.003)):
        d = o.modifiers.new("disp", "DISPLACE"); d.texture = tex; d.strength = k
        d.texture_coords = 'OBJECT'; d.texture_coords_object = anchor; d.mid_level = 0.5
    s = o.modifiers.new("sub", "SUBSURF"); s.levels = 1; s.render_levels = 1

# Тёмная обмазка в щелях и плоскость запекания ровно на один тайл (UV 0..1).
under = bpy.data.materials.new("under"); under.use_nodes = True
under.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.05, 0.035, 0.025, 1)
me = bpy.data.meshes.new("underlay")
me.from_pydata([(-2, -2, -0.04), (4, -2, -0.04), (4, 4, -0.04), (-2, 4, -0.04)], [], [(0, 1, 2, 3)])
me.materials.append(under)
sc.collection.objects.link(bpy.data.objects.new("underlay", me))
me = bpy.data.meshes.new("bake_plane")
me.from_pydata([(0, 0, 0.35), (PERIOD, 0, 0.35), (PERIOD, PERIOD, 0.35), (0, PERIOD, 0.35)], [], [(0, 1, 2, 3)])
uv = me.uv_layers.new(name="UVMap")
for li, (u, v) in enumerate([(0, 0), (1, 0), (1, 1), (0, 1)]):
    uv.data[li].uv = (u, v)
pm = bpy.data.materials.new("bake_target"); pm.use_nodes = True
tex = pm.node_tree.nodes.new("ShaderNodeTexImage")
pm.node_tree.nodes.active = tex
me.materials.append(pm)
plane = bpy.data.objects.new("bake_plane", me)
sc.collection.objects.link(plane)

sc.render.engine = 'CYCLES'
b = sc.render.bake
b.use_selected_to_active = True
b.cage_extrusion = 0.0
b.max_ray_distance = 1.0
b.margin = 0
b.normal_space = 'TANGENT'
view_layer = sc.view_layers[0]
for o in sc.collection.all_objects:
    o.select_set(o.type == 'MESH')
view_layer.objects.active = plane
for kind, btype, samples in (("Color", "DIFFUSE", 4), ("NormalGL", "NORMAL", 4), ("Roughness", "ROUGHNESS", 4), ("AmbientOcclusion", "AO", 64)):
    im = bpy.data.images.new("roof_" + kind, SIZE, SIZE, alpha=False, float_buffer=(kind == "NormalGL"))
    if kind != "Color":
        im.colorspace_settings.name = 'Non-Color'
    tex.image = im
    sc.cycles.samples = samples
    t = time.time()
    kw = dict(type=btype)
    if btype == "DIFFUSE":
        kw["pass_filter"] = {'COLOR'}
    bpy.ops.object.bake(**kw)
    path = os.path.join(OUT, "MrpgByzRoof_%s.png" % kind)
    im.filepath_raw = path
    im.file_format = 'PNG'
    im.save(filepath=path)
    print("%s: %.1f с -> %s" % (kind, time.time() - t, path))
