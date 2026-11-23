#!/usr/bin/env python3
"""Поверхность обожжённой глины для черепицы (tools/textures/blender_byz_roof.py).

Сшивание кусочков (image quilting, Efros & Freeman 2001) из фотоскана старой черепицы
Poly Haven clay_roof_tiles_02 (CC0, 2.5 м на 2K): берём только «середины» плиток — где AO
высокое и нормаль ровная, — поэтому рисунок чужой черепицы (полукруглой, сплошными рядами)
не переносится, остаются настоящие цвет, пятна обжига и налёт. Выход — 1024², 1.25 м.

Запуск:  python3 tools/textures/quilt_clay.py <Color.jpg> <AmbientOcclusion.jpg> <NormalGL.jpg> <выход.jpg>
Нужны только Pillow и numpy. Выход положить к картам фотосканов как mrpg_clay_surface.jpg.
"""
import sys, numpy as np
from PIL import Image
src_c, src_ao, src_n, out = sys.argv[1:5]
rng = np.random.default_rng(7)
C = np.asarray(Image.open(src_c).convert('RGB')).astype(np.float32) / 255
AO = np.asarray(Image.open(src_ao).convert('L')).astype(np.float32) / 255
NR = np.asarray(Image.open(src_n).convert('RGB')).astype(np.float32) / 255
H = C.shape[0]
P, OV, OUT = 40, 10, 1024
step = P - OV
# Допустимые патчи: целиком внутри освещённой глины.
flat = (np.abs(NR[..., 0] - 0.5) < 0.2) & (np.abs(NR[..., 1] - 0.5) < 0.2)
okpix = (AO > 0.72) & flat
I = np.pad(okpix.astype(np.int32).cumsum(0).cumsum(1), ((1, 0), (1, 0)))
cnt = I[P:, P:] - I[:-P, P:] - I[P:, :-P] + I[:-P, :-P]
ys, xs = np.nonzero(cnt[: H - P, : H - P] >= int(P * P * 0.92))
print("кандидатов", len(ys))
n = (OUT + OV) // step + 2
S = n * step + OV
res = np.zeros((S, S, 3), np.float32)
def cut_v(e):  # e: (P, OV) ошибка, минимальный путь сверху вниз
    E = e.copy()
    for i in range(1, E.shape[0]):
        l = np.r_[np.inf, E[i - 1, :-1]]; r = np.r_[E[i - 1, 1:], np.inf]
        E[i] += np.minimum(np.minimum(l, E[i - 1]), r)
    mask = np.zeros_like(e, bool)
    j = int(np.argmin(E[-1]))
    for i in range(E.shape[0] - 1, -1, -1):
        mask[i, j:] = True
        if i:
            lo, hi = max(j - 1, 0), min(j + 2, E.shape[1])
            j = lo + int(np.argmin(E[i - 1, lo:hi]))
    return mask  # True — берём новый патч
for by in range(n):
    for bx in range(n):
        y0, x0 = by * step, bx * step
        k = rng.choice(len(ys), 300, replace=False)
        best, berr = None, np.inf
        for idx in k:
            py, px = ys[idx], xs[idx]
            patch = C[py:py + P, px:px + P]
            err = 0.0
            if bx: err += ((patch[:, :OV] - res[y0:y0 + P, x0:x0 + OV]) ** 2).sum()
            if by: err += ((patch[:OV, :] - res[y0:y0 + OV, x0:x0 + P]) ** 2).sum()
            if err < berr: best, berr = patch, err
        if best is None or (bx == 0 and by == 0):
            best = C[ys[k[0]]:ys[k[0]] + P, xs[k[0]]:xs[k[0]] + P]
        m = np.ones((P, P), bool)
        if bx:
            e = ((best[:, :OV] - res[y0:y0 + P, x0:x0 + OV]) ** 2).sum(-1)
            m[:, :OV] &= cut_v(e)
        if by:
            e = ((best[:OV, :] - res[y0:y0 + OV, x0:x0 + P]) ** 2).sum(-1).T
            m[:OV, :] &= cut_v(e).T
        reg = res[y0:y0 + P, x0:x0 + P]
        reg[m] = best[m]
img = res[:OUT, :OUT]
# Бесшовность по краю: перекрёстное смешивание с тем, что за краем.
B = 64
ext = res[:OUT + B, :OUT + B]
w = np.linspace(0, 1, B)[None, :, None]
img = img.copy()
img[:, :B] = img[:, :B] * w + ext[:OUT, OUT:OUT + B] * (1 - w)
wy = np.linspace(0, 1, B)[:, None, None]
img[:B, :] = img[:B, :] * wy + np.vstack([res[OUT:OUT + B, :OUT]])[:, :OUT] * (1 - wy)
Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8)).save(out, quality=95)
print("ok", img.reshape(-1, 3).mean(0))
