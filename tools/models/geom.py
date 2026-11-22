"""Простые генераторы геометрии на numpy: тела вращения, протяжки сечений, коробки.

Каждая функция возвращает (v, tris, uv): вершины (N,3), треугольники (T,3), UV (N,2) в [0,1].
Оси — как в игре: Y вверх, перёд -Z.
"""

import numpy as np


def merge(*parts):
    vs, ts, us = [], [], []
    off = 0
    for v, t, u in parts:
        vs.append(v)
        ts.append(t + off)
        us.append(u)
        off += len(v)
    return np.concatenate(vs), np.concatenate(ts), np.concatenate(us)


def transform(part, R=None, t=(0, 0, 0)):
    v, tr, u = part
    if R is not None:
        v = v @ np.asarray(R).T
    return v + np.asarray(t), tr, u


def grid_tris(rows, cols, closed=False):
    """Треугольники для сетки rows x cols (вершины построчно). closed — замкнуть столбцы."""
    tris = []
    cc = cols if closed else cols - 1
    for i in range(rows - 1):
        for j in range(cc):
            a = i * cols + j
            b = i * cols + (j + 1) % cols
            c = (i + 1) * cols + (j + 1) % cols
            d = (i + 1) * cols + j
            tris += [(a, d, c), (a, c, b)]
    return np.array(tris, dtype=np.int64)


def sweep(sections, ys=None, path=None, closed=True, cap=True, u_scale=1.0):
    """Протяжка: sections — список массивов (M,2) точек сечения в плоскости XZ (одинаковое M).
    Сечение i стоит на высоте ys[i] (или в точке path[i]). Возвращает (v, tris, uv)."""
    n = len(sections)
    m = len(sections[0])
    v = []
    for i, s in enumerate(sections):
        s = np.asarray(s, float)
        base = np.array(path[i], float) if path is not None else np.array([0.0, ys[i], 0.0])
        v.append(np.c_[s[:, 0], np.zeros(m), s[:, 1]] + base)
    v = np.concatenate(v)
    # UV: u — по периметру, v — по длине.
    per = []
    for s in sections:
        s = np.asarray(s, float)
        d = np.linalg.norm(np.roll(s, -1, 0) - s, axis=1)
        c = np.r_[0, np.cumsum(d)[:-1]]
        per.append(c / max(d.sum(), 1e-9))
    lens = np.r_[0, np.cumsum(np.linalg.norm(np.diff(v.reshape(n, m, 3).mean(1), axis=0), axis=1))]
    lens = lens / max(lens[-1], 1e-9)
    # Бок занимает низ развёртки (v до 0.8), торцы — полосу сверху, чтобы острова UV не пересекались.
    uv = np.concatenate([np.c_[per[i] * u_scale, np.full(m, lens[i] * 0.8)] for i in range(n)])
    tris = grid_tris(n, m, closed)
    parts = [(v, tris, uv)]
    if cap:
        for i, flip in ((0, True), (n - 1, False)):
            ring = v[i * m:(i + 1) * m]
            c = ring.mean(0)
            cv = np.r_[ring, [c]]
            ct = np.array([(j, (j + 1) % m, m) for j in range(m)])
            if flip:
                ct = ct[:, [1, 0, 2]]
            ang = np.linspace(0, 2 * np.pi, m, endpoint=False)
            cu = 0.12 + (0.0 if flip else 0.3)
            cuv = np.r_[np.c_[cu + 0.08 * np.cos(ang), 0.9 + 0.08 * np.sin(ang)], [[cu, 0.9]]]
            parts.append((cv, ct, cuv))
    return merge(*parts)


def ellipse(rx, rz, m=16, cz=0.0, cx=0.0):
    a = np.linspace(0, 2 * np.pi, m, endpoint=False)
    return np.c_[cx + rx * np.cos(a), cz + rz * np.sin(a)]


def lens_section(w, t, m=16, edge=0.0015):
    """Сечение клинка: «линза» шириной w и толщиной t."""
    a = np.linspace(0, 2 * np.pi, m, endpoint=False)
    x = 0.5 * w * np.cos(a)
    z = (0.5 * t - edge) * np.sin(a) * np.abs(np.sin(a)) ** 0.3 + edge * np.sin(a)
    return np.c_[x, z]


def lathe(profile, m=32, rx=1.0, rz=1.0, center=(0, 0, 0), a0=0.0, a1=2 * np.pi):
    """Тело вращения вокруг вертикали: profile — список (r, y) снизу вверх."""
    full = abs(a1 - a0 - 2 * np.pi) < 1e-6
    cols = m if full else m + 1
    ang = np.linspace(a0, a1, cols, endpoint=not full)
    v, uv = [], []
    prof = np.asarray(profile, float)
    L = np.r_[0, np.cumsum(np.linalg.norm(np.diff(prof, axis=0), axis=1))]
    L /= max(L[-1], 1e-9)
    for i, (r, y) in enumerate(prof):
        for j, a in enumerate(ang):
            v.append((center[0] + r * rx * np.sin(a), center[1] + y, center[2] - r * rz * np.cos(a)))
            uv.append(((a - a0) / (a1 - a0), L[i]))
    return np.array(v), grid_tris(len(prof), cols, closed=full), np.array(uv)


def box(sx, sy, sz, c=(0, 0, 0)):
    x, y, z = sx / 2, sy / 2, sz / 2
    faces = []
    for axis in range(3):
        for sgn in (-1, 1):
            q = []
            for a, b in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
                p = [0, 0, 0]
                p[axis] = sgn
                p[(axis + 1) % 3] = a
                p[(axis + 2) % 3] = b
                q.append(p)
            if sgn < 0:
                q = q[::-1]
            faces.append(q)
    v = np.array([p for f in faces for p in f], float) * np.array([x, y, z]) + np.asarray(c)
    t = np.array([(i * 4, i * 4 + 1, i * 4 + 2) for i in range(6)] + [(i * 4, i * 4 + 2, i * 4 + 3) for i in range(6)])
    quad = np.array([[0, 0], [1, 0], [1, 1], [0, 1]], float) * 0.31
    uv = np.concatenate([quad + [0.01 + 0.33 * (i % 3), 0.01 + 0.5 * (i // 3)] for i in range(6)])
    return v, t, uv


def sphere(r, m=24, n=16, c=(0, 0, 0)):
    prof = [(r * np.sin(t), -r * np.cos(t)) for t in np.linspace(0, np.pi, n)]
    return lathe(prof, m, center=c)
