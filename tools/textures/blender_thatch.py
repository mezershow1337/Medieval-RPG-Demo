"""Соломенная (камышовая) кровля: ряды пучков стеблей внахлёст — моделируется и запекается
в Blender (проверено на 4.5).

Запуск: в Blender (Scripting → Run Script, или через BlenderMCP), либо как модуль Python
(`pip install bpy==4.5.*` под Python 3.11), либо
    blender --background --python tools/textures/blender_thatch.py -- <папка_вывода>
Пишет MrpgThatch_Color.png, _NormalGL.png, _Roughness.png, _AmbientOcclusion.png (1024²),
дальше их забирает tools/textures/build_surface_textures.py (слой "thatch").

Тайл — 1.5×1.5 м: 5 рядов по 0.3 м по скату. Ряд — пучки шириной 10–18 см, в пучке
несколько сотен стеблей толщиной 3–6 мм; нижние концы стеблей (комли) торчат вразнобой,
поэтому край ряда рваный, а каждый ряд выступает над нижним на ~4 см — отсюда тень и объём.
Геометрия выходит за края тайла и повторяется по модулю периода, текстура бесшовная.
Верх картинки — конёк, низ — карниз (в шейдере v растёт вниз по скату). Всё своё.
"""
import bpy, math, os, random, sys, time

OUT = globals().get("MRPG_OUT") or (sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else os.path.join(os.path.expanduser("~"), "mrpg_thatch"))
os.makedirs(OUT, exist_ok=True)
SIZE = 1024
PERIOD = 1.5
COURSE = 0.3          # шаг рядов по скату
N_COURSES = 5         # PERIOD / COURSE
STEP = 0.04           # насколько комли ряда выступают над нижним рядом
SLOPE = STEP / COURSE  # стебли уходят под верхний ряд
SIDES = 5
SEGS = 3

sc = bpy.data.scenes.get("mrpg_thatch") or bpy.data.scenes.new("mrpg_thatch")
for o in list(sc.collection.all_objects):
    bpy.data.objects.remove(o, do_unlink=True)
if bpy.context.window:
    bpy.context.window.scene = sc


def lin(c):
    return tuple(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c)


# Оттенки пучков (sRGB): золотистая, светлая, бурая, выгоревшая серая и тёмная, подгнившая.
TONES = [((0.78, 0.64, 0.38), 0.34), ((0.86, 0.76, 0.52), 0.2), ((0.66, 0.51, 0.29), 0.2), ((0.64, 0.58, 0.47), 0.16), ((0.46, 0.37, 0.23), 0.1)]


def pick_tone(r):
    x = r.random()
    for c, w in TONES:
        x -= w
        if x <= 0:
            return c
    return TONES[0][0]


verts, faces, cols = [], [], []


def straw(x0, y0, z0, length, lean, r0, col, cap_col, r):
    """Стебель: трубка из SIDES граней и SEGS отрезков от комля (x0, y0, z0) вверх по скату."""
    dx, dy = math.sin(lean), math.cos(lean)
    bow = r.uniform(-0.004, 0.006)
    roll = r.uniform(0, math.pi)
    flat = r.uniform(0.55, 1.0)  # примятые стебли — сплюснутые
    base = len(verts)
    for s in range(SEGS + 1):
        t = s / SEGS
        cx = x0 + dx * length * t
        cy = y0 + dy * length * t
        cz = z0 - SLOPE * length * t + bow * math.sin(math.pi * t)
        rr = r0 * (1.0 - 0.25 * t)
        # Поперечное сечение: оси «поперёк стебля в плоскости крыши» и «вверх».
        ax, ay = dy, -dx
        for k in range(SIDES):
            a = 2 * math.pi * k / SIDES + roll
            u, w = math.cos(a) * rr, math.sin(a) * rr * flat
            verts.append((cx + ax * u, cy + ay * u, cz + w))
            cols.append(cap_col if s == 0 else col)
    # Кольцо обходит стебель по часовой стрелке, если смотреть от комля вверх по стеблю, —
    # грани и срез обходим так, чтобы нормали смотрели наружу (иначе AO запекается чёрным).
    for s in range(SEGS):
        for k in range(SIDES):
            a = base + s * SIDES + k
            b = base + s * SIDES + (k + 1) % SIDES
            faces.append((a, a + SIDES, b + SIDES, b))
    # Срез комля.
    faces.append(tuple(base + k for k in range(SIDES)))


def course(k):
    """Ряд с комлями на y ≈ k·COURSE; случайность — по номеру ряда по модулю периода."""
    kk = k % N_COURSES
    r = random.Random(7919 * kk + 17)
    y_b = k * COURSE
    x = 0.0
    bundles = []
    while x < PERIOD - 0.05:
        w = r.uniform(0.1, 0.18)
        if PERIOD - (x + w) < 0.08:
            w = PERIOD - x
        bundles.append((x, w))
        x += w
    for bx, bw in bundles:
        tone = pick_tone(r)
        dy_b = r.uniform(-0.03, 0.02)     # пучок уложен чуть выше или ниже соседей
        dz_b = r.uniform(0.0, 0.012)      # и чуть толще
        lean_b = math.radians(r.uniform(-5, 5))
        n = int(bw * r.uniform(520, 680))
        for _ in range(n):
            layer = r.random()
            x0 = bx + r.uniform(-0.01, bw + 0.01)
            # Комли вразнобой: в основном на 0–3 см, отдельные свисают до 6 см.
            hang = abs(r.gauss(0, 0.014)) + (r.uniform(0.02, 0.05) if r.random() < 0.06 else 0.0)
            y0 = y_b + dy_b - hang + layer * 0.02
            z0 = STEP + 0.01 + dz_b + layer * 0.012 - hang * 0.15
            length = r.uniform(0.42, 0.6)
            lean = lean_b + math.radians(r.gauss(0, 3.0))
            r0 = r.uniform(0.0016, 0.0029)
            shade = r.uniform(0.72, 1.15) * (0.5 if r.random() < 0.07 else 1.0)
            # Пучок вяжут из разной соломы: свой оттенок у большинства стеблей, но не у всех.
            c = tuple(min(1.0, v * shade) for v in (tone if r.random() < 0.6 else pick_tone(r)))
            cap = tuple(v * 0.55 for v in c)
            for off in (-PERIOD, 0.0, PERIOD):
                xa = x0 + off
                xb = xa + math.sin(lean) * length
                if max(xa, xb) < -0.03 or min(xa, xb) > PERIOD + 0.03:
                    continue
                straw(xa, y0, z0, length, lean, r0, lin(c) + (1.0,), lin(cap) + (1.0,), random.Random(r.random()))
    # Редкие сбившиеся стебли поперёк — чтобы поверхность не была слишком причёсанной.
    for _ in range(int(PERIOD * 14)):
        x0 = r.uniform(0, PERIOD)
        y0 = y_b + r.uniform(0.02, COURSE - 0.02)
        z0 = STEP + 0.02 - SLOPE * (y0 - y_b)
        lean = math.radians(r.uniform(25, 70) * r.choice((-1, 1)))
        c = tuple(min(1.0, v * r.uniform(0.8, 1.15)) for v in pick_tone(r))
        for off in (-PERIOD, 0.0, PERIOD):
            straw(x0 + off, y0, z0, r.uniform(0.12, 0.3), lean, r.uniform(0.0015, 0.0025), lin(c) + (1.0,), lin(c) + (1.0,), random.Random(r.random()))


t0 = time.time()
for k in range(-1, N_COURSES + 1):
    course(k)
me = bpy.data.meshes.new("thatch")
me.from_pydata(verts, [], faces)
attr = me.color_attributes.new("Col", "FLOAT_COLOR", "POINT")
attr.data.foreach_set("color", [v for c in cols for v in c])
me.update()
print("стеблей: %d граней, %.1f с" % (len(faces), time.time() - t0))

mat = bpy.data.materials.new("straw")
mat.use_nodes = True
nt = mat.node_tree
bsdf = nt.nodes["Principled BSDF"]
at = nt.nodes.new("ShaderNodeAttribute")
at.attribute_name = "Col"
nt.links.new(at.outputs["Color"], bsdf.inputs["Base Color"])
# Солома матовая, у светлых свежих стеблей лёгкий восковой блеск.
sep = nt.nodes.new("ShaderNodeRGBToBW")
nt.links.new(at.outputs["Color"], sep.inputs[0])
mr = nt.nodes.new("ShaderNodeMapRange")
mr.inputs["From Min"].default_value = 0.1
mr.inputs["From Max"].default_value = 0.6
mr.inputs["To Min"].default_value = 0.92
mr.inputs["To Max"].default_value = 0.72
nt.links.new(sep.outputs[0], mr.inputs["Value"])
nt.links.new(mr.outputs["Result"], bsdf.inputs["Roughness"])
me.materials.append(mat)
sc.collection.objects.link(bpy.data.objects.new("thatch", me))

# Подложка: в щелях между стеблями — глубокая тень старой соломы.
under = bpy.data.materials.new("under"); under.use_nodes = True
under.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.035, 0.026, 0.016, 1)
um = bpy.data.meshes.new("underlay")
um.from_pydata([(-2, -2, -0.04), (4, -2, -0.04), (4, 4, -0.04), (-2, 4, -0.04)], [], [(0, 1, 2, 3)])
um.materials.append(under)
sc.collection.objects.link(bpy.data.objects.new("underlay", um))

pm_me = bpy.data.meshes.new("bake_plane")
pm_me.from_pydata([(0, 0, 0.35), (PERIOD, 0, 0.35), (PERIOD, PERIOD, 0.35), (0, PERIOD, 0.35)], [], [(0, 1, 2, 3)])
uv = pm_me.uv_layers.new(name="UVMap")
for li, (u, v) in enumerate([(0, 0), (1, 0), (1, 1), (0, 1)]):
    uv.data[li].uv = (u, v)
pm = bpy.data.materials.new("bake_target"); pm.use_nodes = True
tex = pm.node_tree.nodes.new("ShaderNodeTexImage")
pm.node_tree.nodes.active = tex
pm_me.materials.append(pm)
plane = bpy.data.objects.new("bake_plane", pm_me)
sc.collection.objects.link(plane)

if not sc.world:
    sc.world = bpy.data.worlds.new("thatch_world")
# AO бака — на масштабе ступеньки ряда и щелей между стеблями.
sc.world.light_settings.distance = 0.06
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
for kind, btype, samples in (("Color", "DIFFUSE", 8), ("NormalGL", "NORMAL", 8), ("Roughness", "ROUGHNESS", 8), ("AmbientOcclusion", "AO", 64)):
    im = bpy.data.images.new("thatch_" + kind, SIZE, SIZE, alpha=False, float_buffer=(kind == "NormalGL"))
    if kind != "Color":
        im.colorspace_settings.name = 'Non-Color'
    tex.image = im
    sc.cycles.samples = samples
    t = time.time()
    kw = dict(type=btype)
    if btype == "DIFFUSE":
        kw["pass_filter"] = {'COLOR'}
    with bpy.context.temp_override(scene=sc, view_layer=view_layer):
        bpy.ops.object.bake(**kw)
    path = os.path.join(OUT, "MrpgThatch_%s.png" % kind)
    im.filepath_raw = path
    im.file_format = 'PNG'
    im.save(filepath=path)
    print("%s: %.1f с -> %s" % (kind, time.time() - t, path), flush=True)
