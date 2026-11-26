"""Разовые звуки: шаги, бой, голоса, предметы, двери, алхимия, ремесло.

Каждая функция получает Rng и возвращает моно-массив (float, SR).
`SFX` — реестр: имя -> (функция, число вариантов).
"""

import numpy as np
from dsp import *

# ============================================================ шаги
# Обувь VI века — кожаные башмаки и калиги без гвоздей: удар пятки + перекат на носок.


def _heel_toe(rng, heel, toe, gap):
    n = heel.size + secs(gap) + toe.size
    out = np.zeros(n)
    place(out, heel, 0)
    place(out, toe, secs(gap), rng.u(0.35, 0.6))
    return out


def step_stone(rng):
    def hit(dur, bright):
        n = secs(dur)
        thump = np.sin(2 * np.pi * rng.u(70, 110) * t_axis(n)) * env_exp(n, 0.018, 0.001)
        click = bp(rng.noise(n), 900, bright) * env_exp(n, 0.006, 0.0005)
        grit = hp(rng.noise(n), 2500) * env_exp(n, 0.03, 0.004) * 0.12
        return thump * 0.8 + click * 1.0 + grit
    x = _heel_toe(rng, hit(0.09, rng.u(4000, 6500)), hit(0.07, rng.u(3000, 5000)), rng.u(0.05, 0.09))
    # лёгкое шарканье подошвы по камню
    n = x.size
    scuff = bp(rng.noise(n), 1500, 7000) * bell(n, 0.55, 0.2) * rng.u(0.03, 0.08)
    return lp(x + scuff, 9000)


def step_wood(rng):
    def hit(dur):
        n = secs(dur)
        f = rng.u(110, 160)
        body = modes(n, [f, f * 2.3, f * 3.9, f * 6.1], [0.05, 0.03, 0.02, 0.012], [1, 0.6, 0.35, 0.2], rng)
        body *= np.clip(t_axis(n) / 0.001, 0, 1)
        click = lp(rng.noise(n), 4000) * env_exp(n, 0.004)
        return body + click * 0.5
    x = _heel_toe(rng, hit(0.16), hit(0.12), rng.u(0.05, 0.09))
    if rng.chance(0.25):  # скрип доски
        x += _creak(rng, x.size, rng.u(0.15, 0.35), rng.u(300, 600), 0.15)
    return x


def step_dirt(rng):
    def hit(dur):
        n = secs(dur)
        thud = lp(rng.noise(n), 600) * env_exp(n, 0.025, 0.002) * 1.5
        thud += np.sin(2 * np.pi * rng.u(55, 80) * t_axis(n)) * env_exp(n, 0.02) * 0.6
        return thud + _gravel(rng, n, rng.u(6, 14), 0.06) * 0.6
    return lp(_heel_toe(rng, hit(0.12), hit(0.1), rng.u(0.05, 0.09)), 7000)


def step_grass(rng):
    n = secs(0.28)
    rustle = bp(rng.noise(n), 1500, 9000) * (smooth_random(rng, n, 60, 0.2, 1.0) ** 2) * env_ad(n, 0.03, 1.5)
    thud = lp(rng.noise(n), 400) * env_exp(n, 0.03, 0.003) * 1.2
    return rustle * 0.5 + thud + _gravel(rng, n, 3, 0.1) * 0.2


def step_cloth(rng):
    n = secs(0.16)
    thud = lp(rng.noise(n), 300) * env_exp(n, 0.03, 0.004)
    fuzz = bp(rng.noise(n), 800, 3000) * env_exp(n, 0.04, 0.01) * 0.1
    return thud + fuzz


def step_tile(rng):
    # керамическая черепица: глухой удар + звонкое цоканье черепков
    n = secs(0.2)
    x = np.zeros(n)
    place(x, lp(rng.noise(secs(0.05)), 900) * env_exp(secs(0.05), 0.015), 0)
    for _ in range(rng.i(1, 3)):
        m = secs(0.08)
        f = rng.u(1800, 3200)
        c = strike(rng, m, [f, f * 1.7, f * 2.6], [0.02, 0.012, 0.008], [1, 0.5, 0.3], click=0.2)
        place(x, c, secs(rng.u(0.0, 0.06)), rng.u(0.15, 0.35))
    return x


def step_water(rng):
    n = secs(0.4)
    splash = bp(rng.noise(n), 600, 6000) * env_exp(n, 0.07, 0.005)
    x = splash
    for _ in range(rng.i(4, 9)):
        b = _bubble(rng, rng.u(0.6, 2.5))
        place(x, b, secs(rng.u(0.0, 0.2)), rng.u(0.1, 0.3))
    return x


def land(rng):
    """Приземление после прыжка: тяжёлый удар ногами + звяканье снаряжения."""
    n = secs(0.35)
    t = t_axis(n)
    x = np.sin(2 * np.pi * (60 + 30 * np.exp(-t / 0.02)) * t) * env_exp(n, 0.06, 0.002)
    x += lp(rng.noise(n), 900) * env_exp(n, 0.04, 0.002) * 0.8
    x += _gear(rng, n, 0.25)
    return x


def gear(rng):
    """Шорох снаряжения на шаге воина: кольчуга, ремни, ножны."""
    return _gear(rng, secs(0.3), 1.0)


def _gear(rng, n, amt):
    x = np.zeros(n)
    for _ in range(rng.i(5, 12)):
        m = secs(0.05)
        f = rng.u(3000, 8000)
        ring = strike(rng, m, [f, f * 1.41, f * 1.93], [0.012, 0.008, 0.006], [1, 0.6, 0.4], click=0.4, click_lp=12000)
        place(x, ring, secs(rng.u(0, 0.15)), rng.u(0.05, 0.25) * amt)
    x += bp(rng.noise(n), 800, 4000) * env_ad(n, 0.02, 2) * 0.04 * amt
    return x


def _gravel(rng, n, count, spread):
    x = np.zeros(n)
    for _ in range(int(count)):
        m = secs(0.012)
        g = hp(rng.noise(m), rng.u(1500, 4000)) * env_exp(m, 0.002, 0.0002)
        place(x, g, secs(rng.u(0, spread)), rng.u(0.2, 0.7))
    return x


def _bubble(rng, scale=1.0):
    """Пузырёк: синус, высота которого растёт (как у всплывающего пузыря)."""
    r = rng.u(1.5, 6.0) / scale  # радиус, мм
    f0 = 3260.0 / r
    n = secs(rng.u(0.02, 0.07))
    t = t_axis(n)
    f = f0 * (1 + 0.1 * t / t[-1] * rng.u(2, 6))
    ph = 2 * np.pi * np.cumsum(f) / SR
    return np.sin(ph) * env_exp(n, n / SR / 3, 0.001)


def _creak(rng, n, dur, base_rate, amp):
    """Скрип дерева: прерывистое трение (серия щелчков меняющейся частоты) через резонансы доски."""
    m = min(n, secs(dur))
    rate = base_rate * (1 + 0.5 * smooth_random(rng, m, 8, -1, 1))
    ph = np.cumsum(rate) / SR
    pulses = np.diff(np.floor(ph), prepend=0) > 0
    exc = pulses.astype(float) * (0.6 + 0.4 * rng.noise(m))
    body = np.zeros(m)
    for f, q in [(rng.u(350, 500), 12), (rng.u(800, 1100), 10), (rng.u(1500, 2100), 8), (rng.u(2600, 3200), 6)]:
        body += peak(exc, f, q)
    out = np.zeros(n)
    out[:m] = body * env_ad(m, 0.03, 0.7) * amp
    return out


# ============================================================ бой

def swing(rng, heavy=False):
    """Свист клинка: шум сквозь полосу, центр которой взлетает и падает с движением."""
    dur = rng.u(0.32, 0.42) if heavy else rng.u(0.22, 0.3)
    n = secs(dur + 0.05)
    shape = bell(n, rng.u(0.45, 0.6), rng.u(0.13, 0.18))
    lo, hi = (250, 1400) if heavy else (500, 2800)
    fc = lo + (hi - lo) * shape
    x = sweep_filter(rng.noise(n), "bpq", fc, q=2.5 if heavy else 3.5)
    x *= shape ** 1.5
    if not heavy:
        x += sweep_filter(rng.noise(n), "bpq", fc * 2.2, q=6) * shape ** 3 * 0.3
    return x


def swing_heavy(rng):
    return swing(rng, True)


def _steel(rng, n, base, decay):
    """Клинок как стальная полоса: негармонические моды поперечных колебаний."""
    ratios = [1.0, 2.76, 5.40, 8.93, 13.34, 18.6]
    fr = [base * r * rng.u(0.98, 1.02) for r in ratios]
    dec = [decay * (1.0 / (1 + i * 0.6)) for i in range(len(fr))]
    amp = [1.0, 0.8, 0.6, 0.4, 0.25, 0.15]
    return strike(rng, n, fr, dec, amp, click=0.5, click_lp=12000)


def parry(rng):
    """Сталь о сталь: звон двух клинков + короткий скрежет скольжения."""
    n = secs(1.1)
    x = _steel(rng, n, rng.u(480, 620), rng.u(0.25, 0.4))
    x += _steel(rng, n, rng.u(700, 900), rng.u(0.15, 0.25)) * 0.7
    m = secs(rng.u(0.08, 0.16))
    scrape = bp(rng.noise(m), 2500, 9000) * env_ad(m, 0.005, 1.0) * (0.5 + 0.5 * np.sin(2 * np.pi * rng.u(80, 160) * t_axis(m)))
    place(x, scrape, secs(0.004), 0.4)
    return x


def block_shield(rng):
    """Удар в деревянный щит, обтянутый кожей: гулкий удар доски + щелчок умбона."""
    n = secs(0.5)
    f = rng.u(95, 130)
    x = strike(rng, n, [f, f * 1.8, f * 2.9, f * 4.3, f * 5.7], [0.09, 0.06, 0.04, 0.025, 0.015], [1, 0.7, 0.5, 0.3, 0.2], click=0.8, click_lp=3500)
    x += lp(rng.noise(n), 250) * env_exp(n, 0.05, 0.001) * 0.8
    if rng.chance(0.4):
        x += _steel(rng, n, rng.u(900, 1300), 0.08) * 0.25
    return x


def guard_break(rng):
    """Пробитая защита: тяжёлый удар, звон и дребезг."""
    n = secs(0.9)
    x = block_shield(rng)
    x = np.pad(x, (0, max(0, n - x.size)))[:n]
    x += _steel(rng, n, rng.u(350, 450), 0.3) * 0.8
    x += lp(rng.noise(n), 180) * env_exp(n, 0.08) * 1.2
    return x


def hit_flesh(rng):
    """Попадание по телу: рассекающий шлепок и глухой удар (без смакования)."""
    n = secs(0.35)
    t = t_axis(n)
    slap = bp(rng.noise(n), 600, 5000) * env_exp(n, 0.012, 0.0005)
    thud = np.sin(2 * np.pi * (70 + 60 * np.exp(-t / 0.015)) * t) * env_exp(n, 0.05, 0.001)
    cloth = bp(rng.noise(n), 1500, 6000) * env_exp(n, 0.05, 0.003) * 0.25
    wet = bp(rng.noise(n), 300, 1500) * env_exp(n, 0.08, 0.005) * (0.5 + 0.5 * np.sin(2 * np.pi * 37 * t)) * 0.35
    return slap * 0.9 + thud + cloth + wet


def hit_armor(rng):
    """Удар по кольчуге или ламеллярному доспеху: дребезг колец и пластин + удар."""
    n = secs(0.5)
    x = hit_flesh(rng)[:n] * 0.5
    x = np.pad(x, (0, n - x.size))
    x += _gear(rng, n, 2.0)
    for _ in range(rng.i(2, 4)):
        m = secs(0.15)
        f = rng.u(1800, 3500)
        place(x, strike(rng, m, [f, f * 2.1, f * 3.3], [0.03, 0.02, 0.012], [1, 0.5, 0.3], click=0.3), secs(rng.u(0, 0.02)), 0.35)
    return x


def body_fall(rng):
    n = secs(0.9)
    t = t_axis(n)
    x = np.zeros(n)
    for k, at in enumerate([0.0, rng.u(0.08, 0.15), rng.u(0.25, 0.35)]):
        m = secs(0.3)
        tt = t_axis(m)
        hit = np.sin(2 * np.pi * (50 + 30 * np.exp(-tt / 0.03)) * tt) * env_exp(m, 0.07, 0.003)
        hit += lp(rng.noise(m), 700) * env_exp(m, 0.04, 0.002) * 0.7
        place(x, hit, secs(at), [1.0, 0.7, 0.4][k])
    x += _gear(rng, n, 0.8)
    x += bp(rng.noise(n), 700, 4000) * env_ad(n, 0.05, 2) * 0.06
    return x


def bow_release(rng):
    """Спуск тетивы составного лука: щелчок, низкое «тванг» тетивы и удар о накладки."""
    n = secs(0.5)
    t = t_axis(n)
    f = rng.u(110, 150)
    string = np.zeros(n)
    for k in range(1, 12):
        string += np.sin(2 * np.pi * f * k * (1 + 0.0004 * k * k) * t) / k ** 1.3 * np.exp(-t * (18 + 9 * k))
    thwack = lp(rng.noise(n), 2500) * env_exp(n, 0.01, 0.0005)
    whoosh = bp(rng.noise(n), 1500, 5000) * env_exp(n, 0.08, 0.01) * 0.2
    return string * 0.9 + thwack * 0.8 + whoosh


def bow_draw(rng):
    n = secs(0.8)
    x = _creak(rng, n, 0.7, rng.u(60, 110), 0.25)
    x += bp(rng.noise(n), 1500, 6000) * env_ad(n, 0.1, 1) * 0.04
    return x


def arrow_whiz(rng):
    n = secs(0.45)
    sh = bell(n, 0.5, 0.18)
    fc = 1500 + 2500 * sh * np.linspace(1.1, 0.8, n)
    return sweep_filter(rng.noise(n), "bpq", fc, q=5) * sh ** 2


def arrow_hit(rng):
    """Стрела вонзается в дерево/землю: тупой стук и дрожь древка."""
    n = secs(0.5)
    t = t_axis(n)
    thunk = lp(rng.noise(n), 1800) * env_exp(n, 0.012, 0.0005)
    f = rng.u(28, 45)
    wobble = bp(rng.noise(n), 200, 1200) * (0.5 + 0.5 * np.sin(2 * np.pi * f * t)) * env_exp(n, 0.12, 0.005) * 0.4
    return thunk + wobble + np.sin(2 * np.pi * 180 * t) * env_exp(n, 0.02) * 0.4


# ============================================================ голоса (без слов)

def _voice(rng, dur, f0s, vowels, breath=0.15, scale=1.0, tilt=1.5, attack=0.02, release_shape=1.3):
    n = secs(dur)
    pts = np.linspace(0, n - 1, len(f0s))
    f0 = np.interp(np.arange(n), pts, f0s)
    src = glottal(rng, f0, jitter=0.008, shimmer=0.06, tilt=tilt)
    src += hp(rng.noise(n), 800) * breath
    v = vowel_morph(src, vowels, scale)
    return v * env_ad(n, attack, release_shape)


def grunt(rng):
    """Выдох с усилием при ударе."""
    f = rng.u(95, 135)
    return _voice(rng, rng.u(0.18, 0.3), [f * 1.15, f, f * 0.85], [rng.choice(["uh", "a", "schwa"])], breath=0.35, attack=0.01, release_shape=2.0)


def pain(rng):
    """Вскрик от раны."""
    f = rng.u(160, 230)
    return _voice(rng, rng.u(0.3, 0.5), [f * 0.9, f * 1.25, f * 1.1, f * 0.7], [rng.choice(["a", "ae"]), "uh"], breath=0.25, attack=0.015, tilt=1.2)


def death(rng):
    f = rng.u(120, 170)
    x = _voice(rng, rng.u(0.8, 1.2), [f * 1.2, f, f * 0.8, f * 0.6, f * 0.5], ["a", "o", "uh"], breath=0.4, attack=0.03, release_shape=1.0)
    return x


def breath_tired(rng):
    """Тяжёлое дыхание уставшего: вдох — выдох (воздух сквозь глотку, без голоса)."""
    n = secs(rng.u(1.0, 1.3))
    x = np.zeros(n)
    inh = secs(0.42)
    exh = secs(0.55)
    a = formant_filter(rng.noise(inh), "e", 1.0) * env_ad(inh, 0.15, 1.2) * 0.6
    b = formant_filter(rng.noise(exh), "a", 1.0) * env_ad(exh, 0.04, 1.6)
    place(x, hp(a, 300), 0)
    place(x, hp(b, 200), secs(0.47))
    return x


# ---- речь горожан: неразборчивое бормотание с греческим ритмом слогов

_SYL_V = ["a", "o", "e", "i", "u", "a", "o", "e"]


def murmur(rng, female=False, child=False, dur=None):
    scale = 1.0 + (0.17 if female else 0) + (0.3 if child else 0)
    base = 115 * (1.75 if female else 1) * (2.0 if child else 1) * rng.u(0.9, 1.12)
    nsyl = rng.i(3, 7)
    out = []
    contour = 1.0 + 0.12 * rng.u(-1, 1)
    for s in range(nsyl):
        d = rng.u(0.11, 0.2)
        stressed = rng.chance(0.3)
        f = base * contour * (1.2 if stressed else 1.0)
        contour *= rng.u(0.94, 1.03)
        v = [rng.choice(_SYL_V), rng.choice(_SYL_V)]
        syl = _voice(rng, d, [f * 1.03, f, f * 0.97], v, breath=0.1, scale=scale, attack=0.015, release_shape=0.7)
        # согласная перед слогом: шипящая, взрывная или сонорная
        c = rng.choice(["s", "t", "k", "p", "n", "l", "r", "th", ""])
        m = secs(0.05)
        if c in ("s", "th"):
            cons = hp(rng.noise(m), 3500 if c == "s" else 1800) * env_ad(m, 0.015, 1) * 0.12
        elif c in ("t", "k", "p"):
            cons = bp(rng.noise(m), {"t": 3000, "k": 1500, "p": 600}[c], 7000) * env_exp(m, 0.006) * 0.25
        else:
            cons = np.zeros(m)
        out.append(cons)
        out.append(syl * (1.3 if stressed else 1.0))
        if rng.chance(0.15):
            out.append(np.zeros(secs(rng.u(0.05, 0.15))))
    x = np.concatenate(out)
    return x


def murmur_m(rng):
    return murmur(rng)


def murmur_f(rng):
    return murmur(rng, female=True)


def murmur_child(rng):
    return murmur(rng, child=True)


def hail(rng):
    """Окрик стражи/торговца: громкое протяжное «э-эй!» (без слов)."""
    f = rng.u(140, 190)
    return _voice(rng, rng.u(0.5, 0.75), [f, f * 1.3, f * 1.35, f * 1.1], ["e", "e", "i"], breath=0.12, attack=0.04, tilt=1.1)


# ============================================================ предметы и торговля

def coin(rng):
    """Медные и серебряные монеты в горсти: несколько звонких касаний монет."""
    n = secs(0.6)
    x = np.zeros(n)
    k = rng.i(3, 8)
    at = 0.0
    for i in range(k):
        f = rng.u(3800, 6500)
        m = secs(0.25)
        c = strike(rng, m, [f, f * 1.52, f * 2.27, f * 0.62], [0.09, 0.05, 0.03, 0.05], [1, 0.6, 0.35, 0.3], click=0.35, click_lp=14000)
        place(x, c, secs(at), rng.u(0.3, 1.0))
        at += rng.u(0.02, 0.08)
    return x


def coins_pour(rng):
    n = secs(1.0)
    x = np.zeros(n)
    for i in range(rng.i(14, 24)):
        f = rng.u(3500, 7000)
        m = secs(0.2)
        c = strike(rng, m, [f, f * 1.52, f * 2.27], [0.06, 0.04, 0.02], [1, 0.6, 0.35], click=0.3, click_lp=14000)
        t = rng.u(0, 0.7) ** 1.3
        place(x, c, secs(t), rng.u(0.2, 0.8) * (1 - t))
    return x


def drop_metal(rng):
    n = secs(0.9)
    f = rng.u(500, 900)
    x = strike(rng, n, [f, f * 2.4, f * 4.1, f * 6.3, f * 8.8], [0.25, 0.15, 0.1, 0.06, 0.04], [1, 0.7, 0.5, 0.3, 0.2], click=0.6)
    place(x, strike(rng, secs(0.4), [f * 1.1, f * 2.6], [0.1, 0.06], [0.5, 0.3]), secs(rng.u(0.08, 0.15)), 0.5)
    return x + lp(rng.noise(n), 500) * env_exp(n, 0.02) * 0.5


def drop_wood(rng):
    n = secs(0.4)
    f = rng.u(180, 320)
    x = strike(rng, n, [f, f * 2.1, f * 3.4, f * 5.2], [0.05, 0.035, 0.02, 0.012], [1, 0.6, 0.4, 0.25], click=0.6, click_lp=4000)
    place(x, strike(rng, secs(0.2), [f * 1.1, f * 2.2], [0.03, 0.02], [0.6, 0.3]), secs(rng.u(0.06, 0.12)), 0.4)
    return x


def drop_ceramic(rng):
    n = secs(0.5)
    f = rng.u(900, 1600)
    x = strike(rng, n, [f, f * 1.9, f * 2.8, f * 4.4], [0.08, 0.05, 0.03, 0.02], [1, 0.7, 0.4, 0.3], click=0.5, click_lp=8000)
    return x + lp(rng.noise(n), 700) * env_exp(n, 0.02) * 0.5


def drop_soft(rng):
    n = secs(0.3)
    return lp(rng.noise(n), 500) * env_exp(n, 0.035, 0.003) + bp(rng.noise(n), 1000, 5000) * env_exp(n, 0.05, 0.01) * 0.15


def rustle(rng):
    """Кожа и ткань: взять вещь, переложить в сумке."""
    n = secs(rng.u(0.18, 0.3))
    am = smooth_random(rng, n, 70, 0, 1) ** 3
    return bp(rng.noise(n), 900, 7000) * am * env_ad(n, 0.02, 1.2)


def page(rng):
    """Лист пергамента/папируса: шорох с хрустом."""
    n = secs(rng.u(0.3, 0.45))
    sh = bell(n, 0.45, 0.2)
    fc = 2000 + 3000 * sh
    x = sweep_filter(rng.noise(n), "bpq", fc, q=1.2) * sh
    x += _gravel(rng, n, rng.i(4, 10), n / SR * 0.8) * 0.2
    return x


def scroll(rng):
    n = secs(0.9)
    x = bp(rng.noise(n), 1200, 8000) * (smooth_random(rng, n, 25, 0.1, 1) ** 2) * env_ad(n, 0.1, 1)
    return x + _gravel(rng, n, 18, 0.8) * 0.15


def herb_pick(rng):
    """Сорвать траву: треск волокон и шелест листьев."""
    n = secs(0.5)
    x = bp(rng.noise(n), 1500, 8000) * (smooth_random(rng, n, 40, 0, 1) ** 2) * env_ad(n, 0.05, 1.5) * 0.6
    for _ in range(rng.i(3, 7)):
        m = secs(0.008)
        place(x, hp(rng.noise(m), 2000) * env_exp(m, 0.0015), secs(rng.u(0.1, 0.3)), rng.u(0.4, 1.0))
    return x


def cloth_tear(rng):
    """Рвём полотно на бинты."""
    n = secs(rng.u(0.5, 0.75))
    rate = 280 + 250 * smooth_random(rng, n, 6, 0, 1)
    ph = np.cumsum(rate) / SR
    pulses = (np.diff(np.floor(ph), prepend=0) > 0).astype(float) * (0.5 + 0.5 * rng.noise(n))
    x = bp(pulses, 1200, 7000) * 3 + bp(rng.noise(n), 2000, 8000) * 0.1
    return x * env_ad(n, 0.03, 0.8)


def gulp(rng):
    """Глотки из фляги или чаши."""
    n = secs(1.2)
    x = np.zeros(n)
    at = 0.05
    for _ in range(rng.i(2, 4)):
        m = secs(0.12)
        t = t_axis(m)
        f = 320 * np.exp(-t / 0.06) + 140
        g = np.sin(2 * np.pi * np.cumsum(f) / SR) * env_ad(m, 0.01, 1.5)
        g = formant_filter(g + lp(rng.noise(m), 800) * 0.2, "o")
        place(x, g, secs(at))
        at += rng.u(0.28, 0.38)
    return x


def eat(rng):
    n = secs(0.7)
    x = np.zeros(n)
    for i in range(rng.i(3, 5)):
        m = secs(0.08)
        c = lp(rng.noise(m), 3000) * env_exp(m, 0.015) + _gravel(rng, m, 5, 0.04) * 0.5
        place(x, c, secs(i * rng.u(0.12, 0.17)))
    return x


def glass_clink(rng):
    n = secs(0.8)
    f = rng.u(1800, 2800)
    return strike(rng, n, [f, f * 2.32, f * 4.25, f * 6.63], [0.4, 0.25, 0.15, 0.08], [1, 0.6, 0.3, 0.2], click=0.2, click_lp=12000)


def pour(rng):
    """Переливание жидкости: струя и бульканье."""
    n = secs(rng.u(1.2, 1.8))
    stream = bp(rng.noise(n), 400, 3000) * (0.6 + 0.4 * smooth_random(rng, n, 12, 0, 1)) * env_ad(n, 0.08, 0.4) * 0.3
    for _ in range(rng.i(25, 45)):
        place(stream, _bubble(rng, rng.u(0.7, 1.6)), secs(rng.u(0, n / SR - 0.1)), rng.u(0.05, 0.2))
    return stream


def mortar(rng):
    """Ступка: толчение (удар пестика) и растирание с хрустом сырья."""
    n = secs(rng.u(0.9, 1.2))
    x = np.zeros(n)
    m = secs(0.15)
    f = rng.u(500, 800)
    knock = strike(rng, m, [f, f * 2.2, f * 3.7], [0.04, 0.025, 0.015], [1, 0.5, 0.3], click=0.6, click_lp=5000)
    place(x, knock, 0)
    g = secs(0.75)
    grind = bp(rng.noise(g), 500, 5000) * (smooth_random(rng, g, 9, 0.2, 1)) * env_ad(g, 0.1, 0.8) * 0.25
    grind += _gravel(rng, g, 30, 0.7) * 0.4
    place(x, grind, secs(0.15))
    return x


def boil(rng):
    """Кипение в сосуде: короткий всплеск пузырей (петля кипения — в окружении)."""
    n = secs(1.5)
    x = lp(rng.noise(n), 1200) * 0.03
    for _ in range(rng.i(25, 40)):
        place(x, _bubble(rng, rng.u(0.5, 1.2)), secs(rng.u(0, 1.4)), rng.u(0.1, 0.4))
    return x


def hiss(rng):
    """Шипение пара/жидкости на огне."""
    n = secs(rng.u(0.8, 1.3))
    return hp(rng.noise(n), 2500) * env_ad(n, 0.01, 2.0) * (0.7 + 0.3 * smooth_random(rng, n, 30, 0, 1))


def flame_burst(rng):
    """Вспышка (горючая смесь, масло): глухой «вуф» и треск."""
    n = secs(1.0)
    t = t_axis(n)
    w = lp(rng.noise(n), 500) * env_ad(n, 0.05, 2.5) * 1.5
    x = w + bp(rng.noise(n), 1000, 6000) * env_ad(n, 0.03, 2) * 0.2
    for _ in range(rng.i(5, 12)):
        place(x, hp(rng.noise(secs(0.01)), 1500) * env_exp(secs(0.01), 0.002), secs(rng.u(0.05, 0.8)), rng.u(0.2, 0.6))
    return x


def door_open(rng):
    """Дверь на кованых петлях: щелчок засова и скрип."""
    n = secs(1.2)
    x = np.zeros(n)
    m = secs(0.12)
    f = rng.u(1400, 2000)
    latch = strike(rng, m, [f, f * 2.3, f * 3.8], [0.03, 0.02, 0.01], [1, 0.5, 0.3], click=0.6)
    place(x, latch, 0, 0.6)
    place(x, _creak(rng, secs(0.9), rng.u(0.6, 0.9), rng.u(25, 70), 0.6), secs(0.1))
    return x


def door_close(rng):
    n = secs(0.8)
    x = np.zeros(n)
    place(x, _creak(rng, secs(0.4), 0.35, rng.u(40, 90), 0.3), 0)
    m = secs(0.5)
    f = rng.u(70, 100)
    slam = strike(rng, m, [f, f * 2.1, f * 3.3, f * 5.0], [0.12, 0.08, 0.05, 0.03], [1, 0.6, 0.4, 0.2], click=0.8, click_lp=2500)
    place(x, slam, secs(0.3))
    f2 = rng.u(1400, 2000)
    place(x, strike(rng, secs(0.1), [f2, f2 * 2.3], [0.03, 0.02], [1, 0.5], click=0.5), secs(0.33), 0.4)
    return x


def ui_click(rng):
    n = secs(0.08)
    f = rng.u(900, 1200)
    return strike(rng, n, [f, f * 2.3], [0.012, 0.007], [1, 0.4], click=0.4, click_lp=4000)


def ui_open(rng):
    """Открыть сумку/окно: кожаный клапан."""
    n = secs(0.25)
    return bp(rng.noise(n), 400, 3000) * env_ad(n, 0.02, 2) * (smooth_random(rng, n, 50, 0.3, 1)) + lp(rng.noise(n), 300) * env_exp(n, 0.02) * 0.5


def learn(rng):
    """Озарение (выучил ремесло/знание): мягкий удар по струне кифары с призвуком."""
    n = secs(2.0)
    t = t_axis(n)
    x = np.zeros(n)
    for i, f in enumerate([293.7, 440.0, 587.3]):
        m = n - secs(i * 0.09)
        s = np.zeros(m)
        tt = t_axis(m)
        for k in range(1, 10):
            s += np.sin(2 * np.pi * f * k * tt) * np.sin(np.pi * k * 0.2) / k * np.exp(-tt * (1.2 + 0.8 * k))
        place(x, s, secs(i * 0.09), 0.6)
    return x


# ============================================================ звери и город (разовые звуки окружения)

def gull(rng):
    """Средиземноморская (желтоногая) чайка: серия «кья-кья» с понижением."""
    n = secs(rng.u(1.0, 2.0))
    x = np.zeros(n)
    at = 0.0
    calls = rng.i(1, 5)
    for c in range(calls):
        d = rng.u(0.18, 0.32) if c == 0 else rng.u(0.12, 0.2)
        m = secs(d)
        t = np.linspace(0, 1, m)
        f0 = rng.u(700, 950) * (1 - 0.06 * c)
        f = f0 * (0.85 + 0.35 * np.sin(np.pi * t ** 0.7)) * (1 - 0.15 * t)
        ph = 2 * np.pi * np.cumsum(f) / SR
        s = np.zeros(m)
        for k in range(1, 12):
            s += np.sin(k * ph) / k ** 0.9
        s = peak(s, rng.u(1800, 2400), 3) + peak(s, rng.u(3200, 4000), 4) * 0.5 + s * 0.05
        s += hp(rng.noise(m), 3000) * 0.04
        s *= env_ad(m, 0.02, 0.8)
        place(x, s, secs(at))
        at += d + rng.u(0.03, 0.12)
    return x


def dog_bark(rng):
    n = secs(1.4)
    x = np.zeros(n)
    at = 0.0
    big = rng.u(0.8, 1.2)
    for _ in range(rng.i(1, 3)):
        d = rng.u(0.12, 0.2)
        f = rng.u(380, 520) / big
        v = _voice(rng, d, [f * 0.9, f * 1.2, f * 0.8], ["a", "uh"], breath=0.5, scale=1.35 / big, attack=0.005, tilt=1.0, release_shape=1.5)
        place(x, v, secs(at))
        at += d + rng.u(0.15, 0.35)
    return x


def sheep(rng):
    d = rng.u(0.6, 0.9)
    n = secs(d)
    t = t_axis(n)
    f = rng.u(200, 280)
    trem = 1 + 0.06 * np.sin(2 * np.pi * rng.u(6, 8) * t)
    v = _voice(rng, d, [f, f * 1.05, f * 0.95], ["ae", "e"], breath=0.3, scale=1.3, attack=0.04)
    return v * trem * (0.75 + 0.25 * np.sin(2 * np.pi * rng.u(6, 8) * t))


def rooster(rng):
    d = 1.4
    n = secs(d)
    f = rng.u(520, 620)
    return _voice(rng, d, [f * 0.8, f * 1.3, f * 1.3, f * 1.35, f * 0.9], ["a", "o", "o", "u"], breath=0.25, scale=1.6, attack=0.03, tilt=1.0)


def owl(rng):
    """Сплюшка (Otus scops): короткое чистое «тью»."""
    n = secs(0.35)
    t = t_axis(n)
    f = rng.u(1050, 1200) * (1 + 0.03 * np.sin(np.pi * t / t[-1]))
    s = np.sin(2 * np.pi * np.cumsum(f) / SR) + 0.08 * np.sin(4 * np.pi * np.cumsum(f) / SR)
    return s * env_ad(n, 0.05, 1.6)


def donkey(rng):
    """Осёл у ворот: «и-а» вдох-выдох."""
    n = secs(1.8)
    x = np.zeros(n)
    at = 0.0
    for i in range(rng.i(2, 3)):
        hi = _voice(rng, 0.35, [700, 800, 650], ["i", "e"], breath=0.5, scale=1.2, attack=0.03, tilt=0.9)
        lo = _voice(rng, 0.4, [180, 160, 140], ["a", "o"], breath=0.6, attack=0.03, tilt=1.1)
        place(x, hi, secs(at), 0.6)
        place(x, lo, secs(at + 0.38))
        at += 0.8
    return x


def semantron(rng):
    """Семантрон: монах бьёт деревянной колотушкой по доске, сзывая на молитву (колоколов ещё нет)."""
    pattern = [0, 0.32, 0.64, 0.8, 0.96, 1.28, 1.44, 1.6, 1.76, 1.92]
    n = secs(pattern[-1] + 1.0)
    x = np.zeros(n)
    for i, at in enumerate(pattern):
        m = secs(0.5)
        f = rng.u(260, 300) * (1.12 if i % 3 == 2 else 1.0)
        knock = strike(rng, m, [f, f * 2.57, f * 4.1, f * 5.9], [0.12, 0.07, 0.04, 0.025], [1, 0.6, 0.4, 0.3], click=0.7, click_lp=5000)
        place(x, knock, secs(at + rng.u(-0.01, 0.01)), rng.u(0.8, 1.0))
    return x


def far_hammer(rng):
    """Далёкий стук ремесленника (плотник, медник)."""
    n = secs(2.5)
    x = np.zeros(n)
    f = rng.u(300, 700)
    at = 0.0
    for _ in range(rng.i(3, 7)):
        k = strike(rng, secs(0.3), [f, f * 2.3, f * 3.9], [0.06, 0.04, 0.02], [1, 0.5, 0.3], click=0.5, click_lp=3000)
        place(x, k, secs(at), rng.u(0.6, 1.0))
        at += rng.u(0.3, 0.45)
    return lp(x, 3000)


def cart(rng):
    """Телега по мостовой: стук колёс и скрип оси."""
    n = secs(3.0)
    x = np.zeros(n)
    for i in range(30):
        place(x, lp(rng.noise(secs(0.05)), 900) * env_exp(secs(0.05), 0.012), secs(i * 0.09 + rng.u(0, 0.03)), rng.u(0.3, 1.0))
    x += _creak(rng, n, 2.8, rng.u(30, 50), 0.15)
    return x * env_ad(n, 0.8, 1.0)


# ============================================================ реестр

SFX = {
    "step_stone": (step_stone, 8),
    "step_wood": (step_wood, 6),
    "step_dirt": (step_dirt, 6),
    "step_grass": (step_grass, 6),
    "step_cloth": (step_cloth, 4),
    "step_tile": (step_tile, 4),
    "step_water": (step_water, 4),
    "land": (land, 3),
    "gear": (gear, 4),
    "swing": (swing, 5),
    "swing_heavy": (swing_heavy, 4),
    "parry": (parry, 5),
    "block_shield": (block_shield, 5),
    "guard_break": (guard_break, 3),
    "hit_flesh": (hit_flesh, 5),
    "hit_armor": (hit_armor, 4),
    "body_fall": (body_fall, 3),
    "bow_release": (bow_release, 3),
    "bow_draw": (bow_draw, 2),
    "arrow_whiz": (arrow_whiz, 3),
    "arrow_hit": (arrow_hit, 3),
    "grunt": (grunt, 6),
    "pain": (pain, 6),
    "death": (death, 3),
    "breath_tired": (breath_tired, 3),
    "murmur_m": (murmur_m, 10),
    "murmur_f": (murmur_f, 8),
    "murmur_child": (murmur_child, 4),
    "hail": (hail, 3),
    "coin": (coin, 4),
    "coins_pour": (coins_pour, 2),
    "drop_metal": (drop_metal, 3),
    "drop_wood": (drop_wood, 3),
    "drop_ceramic": (drop_ceramic, 3),
    "drop_soft": (drop_soft, 3),
    "rustle": (rustle, 5),
    "page": (page, 4),
    "scroll": (scroll, 2),
    "herb_pick": (herb_pick, 4),
    "cloth_tear": (cloth_tear, 3),
    "gulp": (gulp, 3),
    "eat": (eat, 2),
    "glass_clink": (glass_clink, 3),
    "pour": (pour, 3),
    "mortar": (mortar, 4),
    "boil": (boil, 3),
    "hiss": (hiss, 3),
    "flame_burst": (flame_burst, 2),
    "door_open": (door_open, 3),
    "door_close": (door_close, 3),
    "ui_click": (ui_click, 3),
    "ui_open": (ui_open, 2),
    "learn": (learn, 1),
    "gull": (gull, 8),
    "dog_bark": (dog_bark, 5),
    "sheep": (sheep, 3),
    "rooster": (rooster, 2),
    "owl": (owl, 2),
    "donkey": (donkey, 2),
    "semantron": (semantron, 2),
    "far_hammer": (far_hammer, 4),
    "cart": (cart, 2),
}
