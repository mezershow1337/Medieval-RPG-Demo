"""Музыка в духе ранней Византии (VI век): кифара/лира, авлос (двойная свирель с бурдоном),
тимпан (рамочный бубен), кимвалы и византийское пение с исоном.

Лады — по византийской теории (октава = 72 мориям): мягкий диатонический первый глас
и твёрдый хроматический плагальный второй — отсюда «восточные» интервалы, которых нет
в равномерной темперации. Мелодии сочиняет генератор: попевки движутся поступенно,
поднимаются к господствующей ступени и опускаются к финалису, как в распевах.
"""

import numpy as np
from dsp import *

# --------------------------------------------------------------------- лады (в мориях от финалиса)
# первый глас от Па (Ре): Па–Ву 10, Ву–Га 8, Га–Ди 12, Ди–Ке 12, Ке–Зо 10, Зо–Ни 8, Ни–Па 12
PROTOS = [0, 10, 18, 30, 42, 52, 60, 72]
# плагальный второй (твёрдая хроматика) от Па: 6, 20, 4, 12, 6, 20, 4
PLAGAL2 = [0, 6, 26, 30, 42, 48, 68, 72]
# четвёртый глас (легетос) от Ву: 8, 12, 12, 10, 8, 12, 10
LEGETOS = [0, 8, 20, 32, 42, 50, 62, 72]


def degree_freq(final_hz, scale, deg):
    """Частота ступени (deg может быть отрицательной или выше октавы)."""
    o, d = divmod(deg, 7)
    moria = scale[d] + 72 * o
    return final_hz * 2 ** (moria / 72)


# --------------------------------------------------------------------- инструменты

def lyre(rng, f, dur, vel=1.0, bright=0.5, decay=2.2):
    """Жильная струна кифары: модальная модель (гармоники с местом защипа и затуханием) +
    резонансы деревянного корпуса."""
    ring = min(dur + decay * 1.2, decay * 2.5)
    n = secs(ring)
    t = t_axis(n)
    p = rng.u(0.12, 0.22)
    x = np.zeros(n)
    for k in range(1, 24):
        fk = f * k * (1 + 0.00015 * k * k)
        if fk > 9000:
            break
        a = abs(np.sin(np.pi * k * p)) / k ** (1.4 - 0.5 * bright)
        tau = decay / (1 + 0.35 * (k - 1) ** 1.3)
        x += a * np.sin(2 * np.pi * fk * t + rng.u(0, 6.28)) * np.exp(-t / tau)
    x *= np.clip(t / 0.002, 0, 1)
    # глушение струны рукой после длительности ноты
    damp_at = secs(dur + 0.6)
    if damp_at < n:
        x[damp_at:] *= np.exp(-t[: n - damp_at] / 0.15)
    body = peak(x, 240, 4) * 0.6 + peak(x, 520, 5) * 0.4 + peak(x, 1150, 6) * 0.25
    pluck = lp(rng.noise(n), 3000) * env_exp(n, 0.004) * 0.15
    return (x * 0.7 + body + pluck) * vel


def aulos(rng, fcurve, amp, reed=0.5):
    """Авлос: цилиндрическая трубка с двойной тростью — нечётные гармоники сильнее,
    носовой «гнусавый» тембр, дыхание и вибрато."""
    n = len(fcurve)
    t = t_axis(n)
    vib = 1 + 0.006 * np.sin(2 * np.pi * rng.u(4.8, 5.6) * t + rng.u(0, 6)) * np.clip(t / 0.4, 0, 1)
    f = fcurve * vib * (1 + 0.002 * smooth_random(rng, n, 6, -1, 1))
    ph = 2 * np.pi * np.cumsum(f) / SR
    x = np.zeros(n)
    for k in range(1, 30):
        if np.max(f) * k > 11000:
            break
        a = (1.0 if k % 2 else 0.45 + 0.2 * reed) / k ** (1.1 - 0.4 * reed)
        x += a * np.sin(k * ph)
    x = x * 0.4 + peak(x, 1400, 2.5) * 0.7 + peak(x, 3000, 3) * 0.35 * reed
    breath = bp(rng.noise(n), 1500, 6000) * 0.04
    return (x + breath) * amp


def note_curve(n, f, glide_from=None, glide_t=0.06):
    c = np.full(n, f)
    if glide_from:
        g = min(n, secs(glide_t))
        c[:g] = np.linspace(glide_from, f, g)
    return c


def drum(rng, kind, vel=1.0, base=90.0):
    """Тимпан: мембрана рамочного бубна. dum — удар в центр (низкий, долгий),
    tek — ребро (звонкий, короткий), ka — приглушённый пальцами."""
    ratios = [1.0, 1.59, 2.14, 2.30, 2.65, 2.92, 3.16, 3.5]
    if kind == "dum":
        n = secs(0.9)
        amps = [1.0, 0.5, 0.25, 0.2, 0.12, 0.1, 0.06, 0.05]
        decs = [0.35, 0.18, 0.12, 0.1, 0.08, 0.06, 0.05, 0.04]
        click = 0.15
        f = base
    elif kind == "tek":
        n = secs(0.35)
        amps = [0.2, 0.5, 0.7, 0.6, 0.6, 0.5, 0.4, 0.4]
        decs = [0.08, 0.06, 0.05, 0.05, 0.04, 0.035, 0.03, 0.03]
        click = 0.5
        f = base * 1.9
    else:  # ka
        n = secs(0.2)
        amps = [0.3, 0.5, 0.4, 0.3, 0.2, 0.2, 0.1, 0.1]
        decs = [0.03, 0.025, 0.02, 0.02, 0.015, 0.015, 0.01, 0.01]
        click = 0.6
        f = base * 1.6
    t = t_axis(n)
    pitch_drop = 1 + 0.08 * np.exp(-t / 0.03) * vel
    x = np.zeros(n)
    for r, a, d in zip(ratios, amps, decs):
        ph = 2 * np.pi * np.cumsum(f * r * pitch_drop) / SR
        x += a * np.sin(ph + rng.u(0, 6)) * np.exp(-t / d)
    x += lp(rng.noise(n), 5000 if kind != "dum" else 1500) * env_exp(n, 0.006, 0.0003) * click
    x *= np.clip(t / 0.0008, 0, 1)
    return x * vel


def cymbal(rng, vel=1.0):
    """Кимвалы: два маленьких бронзовых тарелочки."""
    n = secs(1.5)
    base = rng.u(1900, 2300)
    fr = [base * r for r in [1.0, 1.47, 2.09, 2.56, 3.39, 4.1, 5.2]]
    x = strike(rng, n, fr, [0.6, 0.5, 0.4, 0.35, 0.25, 0.2, 0.15], [1, 0.8, 0.7, 0.5, 0.4, 0.3, 0.2], click=0.3, click_lp=12000)
    return hp(x, 1200) * vel


def chant_voice(rng, notes, final, scale, base_vowels=("a", "o", "e"), voices=4, register=1.0, vib=0.004):
    """Хор клира: notes = [(старт с, длит с, ступень)], несколько слегка разных голосов."""
    end = max(s + d for s, d, _ in notes) + 0.5
    n = secs(end)
    out = np.zeros((2, n))
    for v in range(voices):
        f0 = np.zeros(n)
        voc_seq = []
        for (s, d, deg) in notes:
            a, b = secs(s), secs(s + d)
            f = degree_freq(final, scale, deg) * register
            f0[a:b] = f
            voc_seq.append(rng.choice(base_vowels))
        # плавные переходы высоты (портаменто) и вибрато
        f0 = np.where(f0 > 0, f0, np.nan)
        idx = np.arange(n)
        ok = ~np.isnan(f0)
        f0 = np.interp(idx, idx[ok], f0[ok])
        f0 = lp(f0, 12, order=1)
        tt = t_axis(n)
        f0 *= 1 + vib * np.sin(2 * np.pi * rng.u(4.5, 5.5) * tt + rng.u(0, 6)) + 0.003 * smooth_random(rng, n, 2, -1, 1)
        f0 *= rng.u(0.995, 1.005)
        src = glottal(rng, f0, jitter=0.003, shimmer=0.03, tilt=1.7, nharm=40)
        src += hp(rng.noise(n), 1500) * 0.02
        # гласная меняется на каждом слоге
        vowel = np.zeros(n)
        filt = {}
        for vw in set(voc_seq):
            filt[vw] = formant_filter(src, vw, 1.0 if register < 1.5 else 1.12)
        y = np.zeros(n)
        for (s, d, deg), vw in zip(notes, voc_seq):
            a, b = secs(s), min(n, secs(s + d + 0.08))
            w = np.ones(b - a)
            k = min(len(w) // 3, secs(0.06))
            if k > 0:
                w[:k] = np.linspace(0, 1, k)
                w[-k:] = np.linspace(1, 0, k)
            y[a:b] += filt[vw][a:b] * w
        out += pan(y, rng.u(-0.5, 0.5))
    return out / voices


# --------------------------------------------------------------------- сочинитель напевов

def compose_line(rng, length_beats, lo=-1, hi=5, final=0, peak=4, cadence_deg=0):
    """Мелодическая линия ступенями: список (длительность в долях, ступень или None — пауза).
    Поднимается к господствующей ступени и возвращается к финалису."""
    rhythms = [[1, 1], [2], [1, 0.5, 0.5], [0.5, 0.5, 1], [1.5, 0.5], [3, 1], [2, 1, 1], [0.5, 0.5, 0.5, 0.5]]
    out = []
    deg = final + rng.choice([0, 0, 1, 2])
    beats = 0.0
    half = length_beats * rng.u(0.4, 0.6)
    while beats < length_beats - 2:
        rh = rng.choice(rhythms)
        for d in rh:
            if beats >= length_beats - 2:
                break
            target = peak if beats < half else cadence_deg
            step = int(np.sign(target - deg)) if rng.chance(0.6) else rng.choice([-1, 1, 0, -1, 1, 2, -2])
            if rng.chance(0.08):
                step *= 2
            deg = int(np.clip(deg + step, lo, hi))
            out.append((d, deg))
            beats += d
    # каденция: подход сверху к финалису и долгая нота
    out.append((1, cadence_deg + 1))
    out.append((max(1, length_beats - beats - 1), cadence_deg))
    return out


def vary(rng, line, amount=0.3):
    """Вариант попевки: часть нот сдвинута на ступень, ритм местами раздроблен."""
    res = []
    for d, deg in line[:-2]:
        if rng.chance(amount):
            deg = deg + rng.choice([-1, 1])
        if d >= 1 and rng.chance(amount * 0.5):
            res.append((d / 2, deg))
            res.append((d / 2, deg + rng.choice([-1, 1])))
        else:
            res.append((d, deg))
    return res + line[-2:]


# --------------------------------------------------------------------- рендер партий

class Mix:
    def __init__(self, dur, tail=6.0):
        self.n = secs(dur + tail)
        self.dur = dur
        self.dry = np.zeros((2, self.n))
        self.wet = np.zeros((2, self.n))

    def add(self, x, at, gain=1.0, pan_=0.0, send=0.3):
        st = x if x.ndim == 2 else pan(x, pan_)
        place(self.dry, st, secs(at), gain)
        place(self.wet, st, secs(at), gain * send)

    def render(self, rng, rt60=2.2, bright=0.4, wrap=True):
        ir = reverb_ir(rng, rt60, predelay=0.025, bright=bright)
        w = np.zeros_like(self.dry)
        for c in range(2):
            w[c] = signal.fftconvolve(self.wet[c], ir[c])[: self.n]
        out = self.dry + w
        if wrap:
            # хвост после конца петли переносим в её начало — шов не слышен
            N = secs(self.dur)
            body = out[:, :N].copy()
            tail = out[:, N:]
            body[:, : tail.shape[1]] += tail
            out = body
        return out


from scipy import signal  # noqa: E402


def play_lyre_line(mix, rng, line, start, beat, final, scale, octave=0, gain=0.6, pan_=-0.25, send=0.35, bright=0.5):
    t = start
    for d, deg in line:
        if deg is not None:
            f = degree_freq(final, scale, deg + 7 * octave)
            mix.add(lyre(rng, f, d * beat, rng.u(0.8, 1.0), bright), t + rng.u(-0.008, 0.008), gain, pan_, send)
        t += d * beat
    return t


def play_aulos_line(mix, rng, line, start, beat, final, scale, octave=0, gain=0.35, pan_=0.2, send=0.4, ornament=0.3, legato=True):
    """Авлос играет связно: одна непрерывная кривая высоты с подъездами и форшлагами."""
    total = sum(d for d, _ in line) * beat + 0.3
    n = secs(total)
    fc = np.zeros(n)
    amp = np.zeros(n)
    t = 0.0
    prev = None
    for d, deg in line:
        a, b = secs(t), min(n, secs(t + d * beat))
        if deg is None:
            prev = None
            t += d * beat
            continue
        f = degree_freq(final, scale, deg + 7 * octave)
        seg = np.full(b - a, f)
        if prev and rng.chance(ornament):
            # форшлаг сверху (типичное украшение)
            g = min(len(seg), secs(0.06))
            seg[:g] = degree_freq(final, scale, deg + 1 + 7 * octave)
        elif prev:
            g = min(len(seg), secs(0.035))
            seg[:g] = np.linspace(prev, f, g)
        fc[a:b] = seg
        e = np.ones(b - a)
        att = min(len(e), secs(0.05 if prev is None else 0.015))
        e[:att] = np.linspace(0.2 if legato and prev else 0, 1, att)
        rel = min(len(e), secs(0.08))
        if not legato or d * beat > 1.2:
            e[-rel:] *= np.linspace(1, 0.55, rel)
        amp[a:b] = e * (1.0 if d * beat < 1.5 else np.linspace(1, 0.8, b - a))
        prev = f
        t += d * beat
    fc[fc == 0] = np.nan
    idx = np.arange(n)
    ok = ~np.isnan(fc)
    if not ok.any():
        return start + total
    fc = np.interp(idx, idx[ok], fc[ok])
    amp = lp(amp, 30, order=1)
    x = aulos(rng, fc, amp)
    mix.add(x, start, gain, pan_, send)
    return start + total


def drone_aulos(rng, f, dur, gain=1.0):
    """Вторая трубка авлоса — бурдон на финалисе."""
    n = secs(dur)
    amp = np.ones(n)
    a = secs(1.0)
    amp[:a] = np.linspace(0, 1, a)
    amp[-a:] = np.linspace(1, 0, a)
    return aulos(rng, np.full(n, f), amp * gain, reed=0.3)


def drum_pattern(mix, rng, pattern, start, beat, bars, gain=0.6, pan_=0.05, send=0.2, base=88.0, fill_every=4):
    """pattern: список (доля, вид). Каждый fill_every-й такт — сбивка."""
    t = start
    bar_len = max(p for p, _ in pattern) + 1 if pattern else 4
    for b in range(bars):
        pat = pattern
        if fill_every and b % fill_every == fill_every - 1:
            pat = pattern + [(bar_len - 0.5, "tek"), (bar_len - 0.25, "ka")]
        for pos, kind in pat:
            vel = rng.u(0.75, 1.0) * (1.1 if pos == 0 else 1.0)
            mix.add(drum(rng, kind, vel, base), t + pos * beat + rng.u(-0.006, 0.006), gain, pan_, send)
        t += bar_len * beat
    return t


# --------------------------------------------------------------------- пьесы

D3 = 146.83  # финалис первого гласа (Па) — Ре малой октавы


def piece_city(seed=101):
    """День в Херсоне: кифара перебирает бурдон, авлос поёт попевки первого гласа, тимпан."""
    rng = Rng(seed)
    beat = 60 / 96
    bars = 48
    dur = bars * 4 * beat
    mix = Mix(dur)
    final = D3
    sc = PROTOS
    # кифара: ровное остинато бурдон-квинта-октава
    arp = [(1, 0), (0.5, 4), (0.5, 7), (1, 4), (0.5, 7), (0.5, 4)]
    t = 0.0
    for b in range(bars):
        a = arp if b % 8 < 6 else [(1, 0), (1, 3), (1, 4), (1, 0)]
        t = play_lyre_line(mix, rng, a, t, beat, final, sc, octave=-1, gain=0.45, pan_=-0.35, send=0.3, bright=0.3)
    # мелодия авлоса: A A' B A'' и т.д., входит с 4-го такта
    A = compose_line(rng, 16, lo=-1, hi=6, peak=4)
    B = compose_line(rng, 16, lo=1, hi=8, peak=7, cadence_deg=4)
    form = [A, vary(rng, A), B, vary(rng, A, 0.2), vary(rng, B, 0.25), A, vary(rng, A, 0.4), vary(rng, B, 0.3), A, vary(rng, A, 0.3)]
    t = 4 * 4 * beat
    for i, line in enumerate(form):
        if t > dur - 16 * beat:
            break
        if i == 4:
            # лира отвечает мелодией, авлос молчит
            t = play_lyre_line(mix, rng, line, t, beat, final, sc, octave=0, gain=0.5, pan_=0.25, send=0.35, bright=0.6)
            continue
        t = play_aulos_line(mix, rng, line, t, beat, final, sc, octave=1, gain=0.22, pan_=0.15)
    # тимпан: «дум — тек-тек — дум — тек» с 8-го такта, пауза в середине
    pat = [(0, "dum"), (1, "tek"), (1.5, "ka"), (2, "dum"), (3, "tek")]
    drum_pattern(mix, rng, pat, 8 * 4 * beat, beat, 16, gain=0.4, base=92)
    drum_pattern(mix, rng, pat, 28 * 4 * beat, beat, bars - 28 - 2, gain=0.42, base=92)
    for b in range(8, bars - 2, 8):
        mix.add(cymbal(rng, 0.5), b * 4 * beat, 0.15, 0.4, 0.4)
    out = mix.render(rng, rt60=1.6)
    return out


def piece_evening(seed=202):
    """Золотой час: неторопливая кифара в четвёртом гласе, редкий авлос издалека."""
    rng = Rng(seed)
    beat = 60 / 66
    final = 164.8  # Ву — Ми
    sc = LEGETOS
    phrases = []
    for i in range(10):
        if i % 3 == 2:
            phrases.append(compose_line(rng, 12, lo=0, hi=8, peak=6, cadence_deg=4))
        else:
            phrases.append(compose_line(rng, 12, lo=-2, hi=5, peak=rng.choice([3, 4]), cadence_deg=0))
    total_beats = sum(sum(d for d, _ in p) + 2 for p in phrases)
    dur = total_beats * beat
    mix = Mix(dur, tail=8)
    t = 0.0
    for i, p in enumerate(phrases):
        start = t
        # бурдон: открытая струна финалиса и квинты в начале фразы
        mix.add(lyre(rng, final / 2, 3 * beat, 0.6, 0.2, decay=3.5), start, 0.4, -0.3, 0.4)
        if i % 2 == 1:
            mix.add(lyre(rng, degree_freq(final, sc, 4) / 2, 3 * beat, 0.5, 0.2, decay=3.5), start + beat, 0.35, -0.2, 0.4)
        t = play_lyre_line(mix, rng, p, t, beat, final, sc, gain=0.55, pan_=0.1, send=0.45, bright=0.55)
        if i in (5, 6, 8):
            play_aulos_line(mix, rng, vary(rng, p, 0.2), start + beat * 0.5, beat, final, sc, octave=1, gain=0.12, pan_=0.45, send=0.8)
        t += 2 * beat
    return mix.render(rng, rt60=2.6, bright=0.35)


def piece_night(seed=303):
    """Ночь: низкий авлос в плагальном втором (хроматика), долгий бурдон, редкие удары бубна."""
    rng = Rng(seed)
    beat = 60 / 54
    final = 130.81  # До
    sc = PLAGAL2
    lines = [compose_line(rng, 10, lo=-1, hi=5, peak=rng.choice([3, 4]), cadence_deg=0) for _ in range(8)]
    total = sum(sum(d for d, _ in l) + 3 for l in lines)
    dur = total * beat
    mix = Mix(dur, tail=8)
    # бурдон на всю пьесу
    mix.add(drone_aulos(rng, final / 2, dur + 2, 0.7), 0, 0.12, -0.2, 0.6)
    t = 2 * beat
    for i, l in enumerate(lines):
        if i % 2 == 0:
            t2 = play_aulos_line(mix, rng, l, t, beat, final, sc, octave=0, gain=0.2, pan_=0.15, send=0.7, ornament=0.4)
        else:
            t2 = play_lyre_line(mix, rng, l, t, beat, final, sc, octave=0, gain=0.4, pan_=-0.15, send=0.6, bright=0.35)
        if i % 3 == 1:
            mix.add(drum(rng, "dum", 0.6, 70), t, 0.35, 0, 0.6)
        t = t2 + 3 * beat
    return mix.render(rng, rt60=3.5, bright=0.25)


def piece_combat(seed=404):
    """Бой: несколько тимпанов в размере 7/8 (3+2+2), резкий авлос в хроматике, удары кифары."""
    rng = Rng(seed)
    beat = 60 / 300  # восьмая
    final = D3
    sc = PLAGAL2
    bar = 7
    bars = 64
    dur = bars * bar * beat
    mix = Mix(dur, tail=4)
    pat1 = [(0, "dum"), (2, "ka"), (3, "tek"), (5, "tek"), (6, "ka")]
    pat2 = [(0, "dum"), (1, "ka"), (2, "tek"), (3, "dum"), (5, "tek"), (6, "tek")]
    t = 0.0
    for b in range(bars):
        p = pat1 if (b // 4) % 2 == 0 else pat2
        for pos, kind in p:
            vel = rng.u(0.8, 1.0)
            mix.add(drum(rng, kind, vel, 80), t + pos * beat, 0.55, -0.15, 0.2)
            if kind == "dum" and b >= 8:
                mix.add(drum(rng, "dum", vel * 0.9, 62), t + pos * beat + 0.004, 0.5, 0.2, 0.2)
        if b >= 16 and b % 2 == 1:
            mix.add(drum(rng, "tek", 0.6, 110), t + 4 * beat, 0.3, 0.5, 0.2)
        # удар низкой струны кифары на сильную долю
        if b % 2 == 0:
            mix.add(lyre(rng, final / 2, 1.0, 1.0, 0.7, decay=1.2), t, 0.5, -0.4, 0.2)
        t += bar * beat
    # авлос: короткие тревожные попевки (в долях по 2 восьмые)
    q = beat * 2
    t = 8 * bar * beat
    k = 0
    while t < dur - 6 * bar * beat:
        line = compose_line(rng, 14, lo=0, hi=7, peak=rng.choice([4, 5, 7]), cadence_deg=rng.choice([0, 4]))
        line = [(d * rng.choice([1, 1, 0.5]), deg) for d, deg in line]
        t = play_aulos_line(mix, rng, line, t, q, final, sc, octave=1, gain=0.22, pan_=0.25, send=0.3, ornament=0.5, legato=False)
        t += rng.choice([2, 4]) * bar * beat
        k += 1
    for b in range(0, bars, 8):
        mix.add(cymbal(rng, 0.8), b * bar * beat, 0.2, 0.5, 0.3)
    return mix.render(rng, rt60=1.3)


def piece_sacred(seed=505):
    """Литургическое пение в базилике: исон (бурдон хора) и мелос первого гласа, долгая акустика."""
    rng = Rng(seed)
    beat = 60 / 58
    final = 110.0  # Ля большой октавы — низкие мужские голоса
    sc = PROTOS
    notes = []
    t = 1.0
    ison = []
    for k in range(9):
        line = compose_line(rng, 14, lo=-1, hi=5, peak=rng.choice([3, 4, 5]), cadence_deg=0 if k % 3 != 1 else 4)
        line = vary(rng, line, 0.2)
        start = t
        for d, deg in line:
            notes.append((t, d * beat * 0.98, deg + 7))
            t += d * beat
        ison.append((start - 0.3, t - start + 0.6, 0 if k % 3 != 1 else 4))
        t += 2.5 * beat
    dur = t + 1
    mix = Mix(dur, tail=10)
    mel = chant_voice(rng, notes, final, sc, ("a", "o", "e", "i", "o"), voices=5, register=1.0)
    mix.add(mel, 0, 0.9, 0, 0.9)
    ison_notes = [(s, d, deg) for s, d, deg in ison]
    iv = chant_voice(rng, ison_notes, final, sc, ("o",), voices=4, register=1.0, vib=0.002)
    mix.add(iv, 0, 0.55, 0, 0.9)
    return mix.render(rng, rt60=4.5, bright=0.3)


MUSIC = {
    "city": piece_city,
    "evening": piece_evening,
    "night": piece_night,
    "combat": piece_combat,
    "sacred": piece_sacred,
}
