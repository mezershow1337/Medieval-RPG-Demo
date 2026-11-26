"""Монеты: золотая, серебряная и медная (свои рисунки чекана, всё процедурное).

Рисунок чекана — карта высот 1024×1024, которую рисует numpy (`relief_map`), без чужих картинок:
  * золотая — крест на трёх ступенях, точечный ободок и «надпись» по кругу;
  * серебряная — восьмилучевая звезда в венке;
  * медная — крупная, с городскими воротами о трёх башнях (Херсон).
Каждая монета получает свою развёртку «relief» (лицо монеты → квадрат карты). Через неё
материал берёт рельеф (bump) и грязь в углублениях; при запекании это уходит в карту нормалей
и цвет модели на земле, а геометрия остаётся лёгкой.
"""
import math
import random

import numpy as np

import itemkit as K
from itemkit import item

import bpy

RELIEF_PX = 1024


# =========================================================================== рисунок чекана


def _grid(n=RELIEF_PX):
    """Координаты лица монеты: u вправо, v вверх, обе от −1 до 1 (радиус монеты — 1)."""
    t = (np.arange(n, dtype=np.float32) + 0.5) / n * 2.0 - 1.0
    u, v = np.meshgrid(t, t)
    return u, v


def _blur(a, r):
    """Размытие квадратным окном, три прохода (почти гаусс), r — в пикселях."""
    if r < 1:
        return a
    k = 2 * r + 1
    out = a
    for _ in range(3):
        c = np.cumsum(np.pad(out, ((r + 1, r), (0, 0)), mode="edge"), axis=0)
        out = (c[k:] - c[:-k]) / k
        c = np.cumsum(np.pad(out, ((0, 0), (r + 1, r)), mode="edge"), axis=1)
        out = (c[:, k:] - c[:, :-k]) / k
    return out


def _rect(u, v, x0, x1, y0, y1):
    return (u >= x0) & (u <= x1) & (v >= y0) & (v <= y1)


def _border(u, v, beads=56):
    """Общая часть: высокий бортик по краю и кольцо точек-«жемчужин» внутри."""
    rho = np.hypot(u, v)
    th = np.arctan2(v, u)
    rim = (rho > 0.9) & (rho <= 1.0)
    a = np.round(th / (2 * math.pi) * beads) * (2 * math.pi) / beads
    bead = np.hypot(u - 0.8 * np.cos(a), v - 0.8 * np.sin(a)) < 0.032
    return rim | bead


def _legend(u, v, seed, r0=0.62, r1=0.72, a0=200.0, a1=-20.0, letters=16):
    """«Надпись» по кругу: короткие штрихи разной формы, как буквы легенды."""
    rnd = random.Random(seed)
    rho = np.hypot(u, v)
    th = np.degrees(np.arctan2(v, u))
    out = np.zeros_like(u, dtype=bool)
    band = (rho > r0) & (rho < r1)
    span = (a0 - a1) / letters
    for i in range(letters):
        c = a0 - (i + 0.5) * span
        d = (th - c + 540.0) % 360.0 - 180.0  # угол от центра буквы
        x = -d / (span * 0.5)  # −1..1 по ширине буквы, слева направо при чтении по часовой
        y = (rho - r0) / (r1 - r0) * 2 - 1  # −1..1 по высоте
        cell = band & (np.abs(x) < 0.8)
        kind = rnd.randrange(5)
        if kind == 0:  # «I»
            m = np.abs(x) < 0.22
        elif kind == 1:  # «Λ»
            m = np.abs(np.abs(x) - (1 - (y + 1) / 2) * 0.6) < 0.22
        elif kind == 2:  # «Ο»
            m = np.abs(np.hypot(x * 1.1, y) - 0.7) < 0.24
        elif kind == 3:  # «Π»
            m = (np.abs(np.abs(x) - 0.55) < 0.2) | (y > 0.6)
        else:  # «Ε»
            m = (x < -0.35) | (np.abs(y) > 0.7) | ((np.abs(y) < 0.18) & (x < 0.3))
        out |= cell & m
    return out


def _gold_design(u, v):
    """Крест с расширенными концами на трёх ступенях; по бокам — точки, вверху — легенда."""
    m = np.zeros_like(u, dtype=bool)
    m |= _rect(u, v, -0.07, 0.07, -0.30, 0.54)  # стойка креста
    m |= _rect(u, v, -0.30, 0.30, 0.10, 0.22)  # перекладина
    # Расширенные концы.
    m |= _rect(u, v, -0.13, 0.13, 0.48, 0.56)
    m |= _rect(u, v, -0.36, -0.28, 0.04, 0.28)
    m |= _rect(u, v, 0.28, 0.36, 0.04, 0.28)
    # Три ступени.
    for i, w in enumerate((0.2, 0.3, 0.4)):
        y1 = -0.30 - i * 0.085
        m |= _rect(u, v, -w, w, y1 - 0.07, y1)
    # Линия обреза и точки по сторонам.
    m |= _rect(u, v, -0.5, 0.5, -0.64, -0.6)
    for x, y in ((-0.42, -0.08), (0.42, -0.08)):
        m |= np.hypot(u - x, v - y) < 0.06
    m |= _legend(u, v, seed=11)
    return m


def _silver_design(u, v):
    """Восьмилучевая звезда с шариком в середине, вокруг — венок из листьев."""
    rho = np.hypot(u, v)
    th = np.arctan2(v, u)
    ray = 0.46 * (0.32 + 0.68 * np.abs(np.cos(4 * th)) ** 6)
    m = rho < ray
    m &= ~((rho > 0.1) & (rho < 0.14))  # бороздка вокруг шарика
    # Венок: листья-эллипсы по кругу, наклонённые вдоль окружности.
    for i in range(22):
        a = 2 * math.pi * i / 22
        if abs(math.sin(a) + 1) < 0.08:  # внизу венок завязан
            continue
        cx, cy = 0.64 * math.cos(a), 0.64 * math.sin(a)
        tang = a + math.pi / 2 + 0.5
        du, dv = u - cx, v - cy
        x = du * math.cos(tang) + dv * math.sin(tang)
        y = -du * math.sin(tang) + dv * math.cos(tang)
        m |= (x / 0.085) ** 2 + (y / 0.035) ** 2 < 1
    m |= np.abs(rho - 0.64) < 0.012  # стебель венка
    m |= np.hypot(u, v + 0.64) < 0.05  # узел внизу
    return m


def _copper_design(u, v):
    """Городские ворота: стена, три башни с зубцами, арка ворот; внизу — черта и точки."""
    m = np.zeros_like(u, dtype=bool)
    m |= _rect(u, v, -0.5, 0.5, -0.38, 0.05)  # стена
    for x, h in ((-0.42, 0.32), (0.0, 0.46), (0.42, 0.32)):
        m |= _rect(u, v, x - 0.12, x + 0.12, -0.38, h)  # башни
        for k in (-1, 0, 1):  # зубцы
            m |= _rect(u, v, x + k * 0.09 - 0.035, x + k * 0.09 + 0.035, h, h + 0.07)
        m &= ~_rect(u, v, x - 0.03, x + 0.03, h - 0.16, h - 0.06)  # бойницы
    # Арка ворот в средней башне.
    gate = _rect(u, v, -0.085, 0.085, -0.38, -0.12) | (np.hypot(u, v + 0.12) < 0.085)
    m &= ~gate
    # Кладка: тонкие швы на стене.
    for y in (-0.24, -0.1):
        m &= ~(_rect(u, v, -0.5, 0.5, y - 0.008, y + 0.008) & ~_rect(u, v, -0.13, 0.13, -1, 1))
    m |= _rect(u, v, -0.56, 0.56, -0.5, -0.45)  # черта
    for x in (-0.2, 0.0, 0.2):
        m |= np.hypot(u - x, v + 0.6) < 0.045
    return m


DESIGNS = {"gold": _gold_design, "silver": _silver_design, "copper": _copper_design}


def relief_map(kind, n=RELIEF_PX):
    """(высота, углубления) — два массива n×n от 0 до 1 для чекана `kind`."""
    u, v = _grid(n)
    rho = np.hypot(u, v)
    raised = DESIGNS[kind](u, v) | _border(u, v)
    h = _blur(raised.astype(np.float32), max(1, n // 400))
    # Поле чуть выпуклое к середине, как у чеканной монеты.
    h = np.clip(h * 0.85 + 0.12 * (1 - rho**2), 0, 1)
    # Грязь и патина держатся у подножия рельефа: где вокруг выше, чем в точке.
    cav = np.clip((_blur(h, max(2, n // 90)) - h) * 6.0, 0, 1)
    # Стёртость: самые высокие места чуть сглажены (монета ходила по рукам).
    h = h - 0.06 * _blur(h, max(2, n // 60)) * (h > 0.6)
    return h.astype(np.float32), cav.astype(np.float32)


def relief_image(kind):
    """Картинка Blender (Non-Color): R — высота рельефа, G — углубления."""
    name = f"relief_{kind}"
    img = bpy.data.images.get(name)
    if img:
        return img
    h, cav = relief_map(kind)
    n = h.shape[0]
    img = bpy.data.images.new(name, n, n, alpha=False, float_buffer=True)
    img.colorspace_settings.name = "Non-Color"
    # У картинки Blender первая строка — нижняя, как и у нашего v = −1.
    px = np.stack([h, cav, np.zeros_like(h), np.ones_like(h)], -1)
    img.pixels.foreach_set(px.ravel())
    img.pack()
    return img


# =========================================================================== материал


def coin_mat(name, kind, color, color2, rough, recess, bump_mm=0.16, polish=0.25, tarnish=None):
    """Металл монеты + чекан по развёртке «relief»: рельеф, тёмные углубления, натёртые выпуклости."""
    m = K.metal_mat(name, color, color2, rough=rough, scratches=0.25, dirt=0.35, bump=0.12, tarnish=tarnish)
    nb = K.Nodes(m)
    bsdf = next(n for n in nb.n if n.type == "BSDF_PRINCIPLED")
    uv = nb.new("ShaderNodeUVMap")
    uv.uv_map = "relief"
    tex = nb.new("ShaderNodeTexImage", interpolation="Cubic", extension="EXTEND")
    tex.image = relief_image(kind)
    nb.link(uv.outputs["UV"], tex.inputs["Vector"])
    sep = nb.new("ShaderNodeSeparateColor")
    nb.link(tex.outputs["Color"], sep.inputs["Color"])
    height, cav = sep.outputs["Red"], sep.outputs["Green"]

    def source(sock):
        return next((lk.from_socket for lk in nb.nt.links if lk.to_socket == sock), None)

    # Цвет: углубления темнее (грязь, патина), выпуклости светлее (натёрты пальцами).
    col = source(bsdf.inputs["Base Color"])
    col = nb.mix(nb.math("MULTIPLY", cav, 0.85, clamp=True), col, K.lin(recess))
    shine = nb.math("MULTIPLY", nb.ramp(height, [(0.55, 0.0), (0.95, 1.0)]), polish)
    col = nb.mix(shine, col, tuple(min(1.0, c * 1.25) for c in K.lin(color)))
    nb.val(bsdf.inputs["Base Color"], col)
    r = source(bsdf.inputs["Roughness"])
    r = nb.math("ADD", r, nb.math("SUBTRACT", nb.math("MULTIPLY", cav, 0.35), nb.math("MULTIPLY", shine, 0.4)), clamp=True)
    nb.val(bsdf.inputs["Roughness"], r)
    # Рельеф поверх мелкой фактуры металла.
    old_n = source(bsdf.inputs["Normal"])
    nb.val(bsdf.inputs["Normal"], nb.bump(height, 1.0, bump_mm / 1000.0, normal=old_n))
    return m


def coin_materials():
    K.common_materials()
    coin_mat("coin_gold", "gold", (250, 204, 112), (206, 150, 64), rough=(0.12, 0.24), recess=(120, 78, 30), polish=0.3)
    coin_mat("coin_silver", "silver", (226, 228, 232), (172, 172, 178), rough=(0.16, 0.3), recess=(58, 54, 52), polish=0.35, tarnish=((96, 92, 88), 0.35))
    coin_mat("coin_copper", "copper", (196, 108, 64), (138, 70, 40), rough=(0.32, 0.52), recess=(52, 72, 56), polish=0.4, bump_mm=0.22, tarnish=((70, 104, 82), 0.55))


# =========================================================================== монета и горстка


def coin(mat, r, t, x, y, z, tx=0.0, ty=0.0, rz=0.0, segs=64):
    """Одна монета: ребро с фаской, бортик и поле; развёртка «relief» — лицо монеты в квадрат карты."""
    prof = [(0, 0.0), (0, r * 0.86), (0.00015, r * 0.9), (0.0, r * 0.97), (0.00035, r), (t - 0.00035, r), (t, r * 0.97), (t - 0.00015, r * 0.9), (t, r * 0.86), (t, 0.0)]
    bm = K.lathe(prof, segs, cap_top=False, cap_bot=False)
    lay = bm.loops.layers.uv.new("relief")
    for f in bm.faces:
        for lp in f.loops:
            co = lp.vert.co
            # Снизу монета видна зеркально: переворачиваем u, чтобы рисунок не читался наоборот.
            s = 1.0 if co.z > t * 0.5 else -1.0
            lp[lay].uv = (0.5 + 0.5 * s * co.x / r, 0.5 + 0.5 * co.y / r)
    K.rot(bm, "Z", rz)
    K.rot(bm, "X", tx)
    K.rot(bm, "Y", ty)
    K.move(bm, x, y, z)
    return K.mesh_obj("coin", bm, mat, smooth_angle=50)


def pile(mat, r, t, stack, loose, leaning, seed):
    """Стопка из `stack` монет, рядом `loose` лежат россыпью (смещения в радиусах), одна прислонена."""
    rnd = random.Random(seed)
    z = 0.0
    for _ in range(stack):
        coin(mat, r, t, rnd.uniform(-r * 0.1, r * 0.1), rnd.uniform(-r * 0.1, r * 0.1), z, rnd.uniform(-1.5, 1.5), rnd.uniform(-1.5, 1.5), rnd.uniform(0, 360))
        z += t * 1.02
    # Россыпь. Монета, что легла краем на соседнюю (или на стопку), чуть наклонена: дальний край
    # на земле, ближний — на соседке. Так монеты не проходят друг сквозь друга.
    ground = [(0.0, 0.0)]
    for dx, dy in loose:
        x, y = dx * r, dy * r
        under = [(gx, gy) for gx, gy in ground if math.hypot(x - gx, y - gy) < 2 * r * 1.02]
        tx = ty = 0.0
        z = 0.0
        if under:
            gx, gy = min(under, key=lambda g: math.hypot(x - g[0], y - g[1]))
            d = math.hypot(x - gx, y - gy)
            nx, ny = (x - gx) / d, (y - gy) / d
            # Опора — край нижней монеты: на расстоянии (r − (d − r)) от нашего центра в её сторону.
            lever = max(r * 0.2, 2 * r - d)
            a = math.degrees(math.atan2(t * 1.05, r + lever * 0.5))
            tx, ty = -a * ny, a * nx
            z = r * math.sin(math.radians(a))
        else:
            ground.append((x, y))
        coin(mat, r, t, x, y, z, tx, ty, rnd.uniform(-25, 25))
    if leaning:
        dx, dy, tilt = leaning
        coin(mat, r, t, dx * r, dy * r, t * 2.2, tilt, -10, 15)


@item("gold_coin", (1, 1), name="Золотая монета", pose="upright", azimuth=24, elevation=34, margin=0.88)
def gold_coin():
    """Горстка золотых (≈21 мм, тонкие, ярко блестят)."""
    coin_materials()
    pile("coin_gold", 0.0105, 0.0013, 6, ((1.8, -0.6), (-1.6, -0.95), (0.25, -1.85)), (-1.1, 1.15, 24), seed=21)


@item("silver_coin", (1, 1), name="Серебряная монета", pose="upright", azimuth=24, elevation=34, margin=0.88)
def silver_coin():
    """Серебряные (≈22 мм, тоньше золотых): невысокая стопка и веер рядом."""
    coin_materials()
    pile("coin_silver", 0.011, 0.0011, 4, ((1.75, -0.4), (-1.7, -0.7), (-0.2, -1.8), (1.2, -1.75)), (-1.05, 1.2, 22), seed=22)


@item("copper_coin", (1, 1), name="Медная монета", pose="upright", azimuth=24, elevation=34, margin=0.88)
def copper_coin():
    """Медные (≈27 мм, толстые): пара в стопке, остальные россыпью."""
    coin_materials()
    pile("coin_copper", 0.0135, 0.0021, 3, ((1.75, -0.55), (-1.6, -0.8)), (-0.9, 1.25, 20), seed=23)
