"""Фоновые петли окружения (стерео, бесшовные): море, ветер, город, рынок, ночь, огонь,
кипение, комната, базилика."""

import numpy as np
from dsp import *
import sfx


def _stereo_noise_bed(rng, n, lo, hi, width=0.6):
    a = bp(pink(rng, n), lo, hi)
    b = bp(pink(rng, n), lo, hi)
    m = (a + b) / 2
    return np.array([m + width * (a - m), m + width * (b - m)])


def sea(rng, dur=48.0):
    """Прибой о галечный берег и скалы Карантинной бухты: накат, разбег пены, шорох гальки."""
    n = secs(dur + 4)
    out = np.zeros((2, n))
    # ровный далёкий гул моря
    out += _stereo_noise_bed(rng, n, 40, 500) * 0.12
    t = 0.0
    while t < dur + 2:
        period = rng.u(6.0, 11.0)
        m = secs(period + 3)
        tt = np.linspace(0, 1, m)
        crest = rng.u(0.25, 0.4)
        swell = np.where(tt < crest, (tt / crest) ** 2, np.exp(-(tt - crest) * rng.u(4, 7)))
        fc = 200 + 2200 * swell
        p = rng.u(-0.6, 0.6)
        body = sweep_filter(pink(rng, m), "lp", fc, order=2) * swell * rng.u(0.5, 0.9)
        # пена откатывается: шипение с пузырьками и галькой
        foam_env = np.clip((tt - crest) * 3, 0, 1) * np.exp(-(tt - crest).clip(0) * 3)
        foam = hp(rng.noise(m), 2500) * foam_env * 0.12
        pebbles = np.zeros(m)
        for _ in range(int(80 * period / 8)):
            at = int(m * rng.u(crest, 0.95))
            g = hp(rng.noise(secs(0.006)), 3000) * env_exp(secs(0.006), 0.0012)
            place(pebbles, g, at, rng.u(0.02, 0.12) * (1 - (at / m)))
        st = pan(body + foam + pebbles, p)
        place(out, st, secs(t))
        t += period * rng.u(0.6, 0.9)
    out = lp(out, 9000)
    return loop_seam(out, 4.0)


def wind(rng, dur=40.0, strong=False):
    """Ветер на открытом плато и стенах: порывы, свист по краям камня."""
    n = secs(dur + 4)
    gust = smooth_random(rng, n, 0.25, 0.15, 1.0) ** (1.5 if strong else 2.0)
    fc = 250 + (900 if strong else 500) * gust
    chans = []
    for c in range(2):
        g = gust * (0.85 + 0.15 * smooth_random(rng, n, 0.5, 0, 1))
        x = sweep_filter(pink(rng, n), "bpq", fc * (1 + 0.1 * c), q=0.8) * g
        x += lp(brown(rng, n), 120) * 0.4 * g
        if strong:
            whistle = sweep_filter(rng.noise(n), "bpq", 900 + 700 * gust, q=25) * gust ** 3 * 0.5
            x += whistle
        chans.append(x)
    return loop_seam(np.array(chans), 4.0)


def _babble(rng, n, voices, dist_lp, density):
    out = np.zeros((2, n))
    for v in range(voices):
        t = rng.u(0, 1.0)
        p = rng.u(-0.9, 0.9)
        female = rng.chance(0.4)
        child = rng.chance(0.08)
        while t < n / SR:
            s = sfx.murmur(rng, female=female and not child, child=child)
            place(out, pan(s, p), secs(t), rng.u(0.4, 1.0))
            t += s.size / SR + rng.u(0.2, 2.5) / density
    return lp(out, dist_lp)


def market(rng, dur=45.0):
    """Агора: многоголосый торг рядом, стук, звон монет, ослик и куры где-то."""
    n = secs(dur + 4)
    out = _babble(rng, n, 14, 3500, 1.0) * 0.5
    far = _babble(rng, n, 16, 1200, 1.5) * 0.35
    out += far
    for _ in range(int(dur / 3)):
        k = rng.choice(["coin", "coin", "drop_ceramic", "drop_wood", "hail", "rustle"])
        f, _cnt = sfx.SFX[k]
        s = f(rng)
        place(out, pan(lp(s, 5000), rng.u(-0.9, 0.9)), secs(rng.u(0, dur)), rng.u(0.1, 0.35))
    ir = reverb_ir(rng, 0.9, predelay=0.02, bright=0.3)
    out = apply_reverb(out, ir, wet=0.25)[:, :n]
    return loop_seam(out, 4.0)


def city(rng, dur=50.0):
    """Улицы Херсона днём: далёкий гул голосов, ремесло, телеги, ветер в проулках."""
    n = secs(dur + 4)
    out = _babble(rng, n, 18, 900, 1.2) * 0.35
    out += _stereo_noise_bed(rng, n, 60, 600) * 0.05
    for _ in range(int(dur / 6)):
        k = rng.choice(["far_hammer", "far_hammer", "cart", "dog_bark", "drop_wood"])
        s = sfx.SFX[k][0](rng)
        place(out, pan(lp(s, 1800), rng.u(-1, 1)), secs(rng.u(0, dur)), rng.u(0.08, 0.2))
    ir = reverb_ir(rng, 1.4, predelay=0.03, bright=0.2)
    out = apply_reverb(out, ir, wet=0.35)[:, :n]
    return loop_seam(out, 4.0)


def night(rng, dur=40.0):
    """Крымская ночь: сверчки и цикады в сухой траве, редкая сплюшка."""
    n = secs(dur + 4)
    out = np.zeros((2, n))
    for c in range(9):
        f = rng.u(3800, 5200)
        rate = rng.u(1.2, 3.0)  # трелей в секунду
        chirp_pulses = rng.i(2, 5)
        p = rng.u(-0.9, 0.9)
        amp = rng.u(0.15, 0.5)
        tt = rng.u(0, 0.5)
        x = np.zeros(n)
        while tt < n / SR:
            for k in range(chirp_pulses):
                m = secs(0.018)
                s = np.sin(2 * np.pi * f * t_axis(m)) * np.sin(np.pi * np.linspace(0, 1, m)) ** 2
                place(x, s, secs(tt + k * 0.028))
            tt += 1 / rate * rng.u(0.9, 1.1)
        out += pan(x * amp * (0.7 + 0.3 * smooth_random(rng, n, 0.2, 0, 1)), p)
    out += _stereo_noise_bed(rng, n, 50, 400) * 0.06
    t = rng.u(2, 6)
    while t < dur:
        s = sfx.owl(rng)
        for r in range(rng.i(3, 6)):
            place(out, pan(s, -0.5), secs(t + r * 2.6), 0.25)
        t += rng.u(18, 30)
    return loop_seam(out, 4.0)


def fire(rng, dur=12.0):
    """Огонь в очаге/горне: гул пламени, треск поленьев."""
    n = secs(dur + 2)
    roar = lp(brown(rng, n), 300) * (0.6 + 0.4 * smooth_random(rng, n, 3, 0, 1)) * 0.5
    x = roar + bp(rng.noise(n), 1000, 4000) * smooth_random(rng, n, 5, 0, 1) ** 3 * 0.04
    for _ in range(int(dur * 9)):
        m = secs(rng.u(0.004, 0.02))
        c = hp(rng.noise(m), rng.u(800, 3000)) * env_exp(m, m / SR / 3, 0.0002)
        place(x, c, secs(rng.u(0, dur + 1.5)), rng.u(0.1, 0.7) ** 2)
    st = np.array([x, np.roll(x, secs(0.013))])
    return loop_seam(st, 2.0)


def boiling(rng, dur=8.0):
    """Отвар бурлит в горшке/кубе: петля для работающей алхимической установки."""
    n = secs(dur + 2)
    x = lp(rng.noise(n), 800) * 0.04
    for _ in range(int(dur * 45)):
        place(x, sfx._bubble(rng, rng.u(0.5, 1.4)), secs(rng.u(0, dur + 1.9)), rng.u(0.05, 0.35))
    return loop_seam(np.array([x, x]), 2.0)


def room(rng, dur=20.0):
    """Тишина в каменном доме: еле слышный гул улицы сквозь стены."""
    n = secs(dur + 3)
    x = _stereo_noise_bed(rng, n, 30, 250, 0.3) * 0.25
    x += lp(_babble(rng, n, 6, 400, 0.6), 400) * 0.2
    return loop_seam(x, 3.0)


def basilica(rng, dur=40.0):
    """Базилика: гулкий простор, шёпот молящихся, далёкое пение клира."""
    n = secs(dur + 4)
    x = _stereo_noise_bed(rng, n, 40, 300, 0.5) * 0.1
    x += lp(_babble(rng, n, 5, 1200, 0.3), 1200) * 0.15
    ir = reverb_ir(rng, 4.0, predelay=0.04, bright=0.35)
    x = apply_reverb(x, ir, wet=0.6)[:, :n]
    return loop_seam(x, 4.0)


def steppe(rng, dur=45.0):
    """Степь за стенами: ветер в ковыле, жаворонки и цикады днём."""
    n = secs(dur + 4)
    x = wind(rng, dur + 8)[:, :n] * 0.7
    grass = _stereo_noise_bed(rng, n, 2500, 9000) * 0.05 * smooth_random(rng, n, 0.3, 0.3, 1)
    x += grass
    # цикады (стрекот)
    for c in range(3):
        m = n
        tt = t_axis(m)
        buzz = np.sin(2 * np.pi * rng.u(5500, 7000) * tt) * (0.5 + 0.5 * np.sign(np.sin(2 * np.pi * rng.u(80, 140) * tt)))
        e = smooth_random(rng, m, 0.15, 0, 1) ** 4
        x += pan(buzz * e * 0.05, rng.u(-0.8, 0.8))
    # жаворонок: быстрые трели высоко в небе
    t = rng.u(1, 5)
    while t < dur:
        m = secs(rng.u(1.5, 3.0))
        ph = 0
        s = np.zeros(m)
        f = 3000 + 1500 * smooth_random(rng, m, 18, 0, 1)
        s = np.sin(2 * np.pi * np.cumsum(f) / SR) * (0.5 + 0.5 * np.sign(np.sin(2 * np.pi * 14 * t_axis(m))))
        place(x, pan(s * env_ad(m, 0.1, 0.5) * 0.04, rng.u(-0.6, 0.6)), secs(t))
        t += rng.u(6, 14)
    return loop_seam(x, 4.0)


AMBIENT = {
    "sea": sea,
    "wind": wind,
    "wind_strong": lambda rng: wind(rng, strong=True),
    "market": market,
    "city": city,
    "night": night,
    "fire": fire,
    "boiling": boiling,
    "room": room,
    "basilica": basilica,
    "steppe": steppe,
}
