"""Общие кирпичики синтеза звука для игры (numpy + scipy).

Все звуки игры рождаются здесь из шума, резонаторов и формант — никаких чужих записей,
поэтому лицензия у них та же, что у кода игры.
"""

import numpy as np
from scipy import signal

SR = 44100


class Rng:
    def __init__(self, seed):
        self.g = np.random.default_rng(seed)

    def u(self, a=0.0, b=1.0):
        return float(self.g.uniform(a, b))

    def i(self, a, b):
        return int(self.g.integers(a, b + 1))

    def choice(self, xs):
        return xs[int(self.g.integers(0, len(xs)))]

    def chance(self, p):
        return self.g.uniform() < p

    def noise(self, n):
        return self.g.standard_normal(n)


def secs(n):
    return int(round(n * SR))


def t_axis(n):
    return np.arange(n) / SR


# ---------------------------------------------------------------- шумы

def pink(rng, n):
    """Розовый шум (1/f) через спектр."""
    w = rng.noise(n)
    f = np.fft.rfft(w)
    k = np.arange(len(f))
    k[0] = 1
    f /= np.sqrt(k)
    x = np.fft.irfft(f, n)
    return x / (np.std(x) + 1e-9)


def brown(rng, n):
    w = rng.noise(n)
    f = np.fft.rfft(w)
    k = np.arange(len(f)).astype(float)
    k[0] = 1
    f /= k
    x = np.fft.irfft(f, n)
    return x / (np.std(x) + 1e-9)


# ---------------------------------------------------------------- фильтры

def _sos(kind, f, order=2):
    nyq = SR / 2
    if kind == "bp":
        lo, hi = f
        lo = max(lo, 10) / nyq
        hi = min(hi, nyq * 0.98) / nyq
        return signal.butter(order, [lo, hi], btype="band", output="sos")
    f = min(max(f, 10), nyq * 0.98) / nyq
    return signal.butter(order, f, btype="low" if kind == "lp" else "high", output="sos")


def lp(x, f, order=2):
    return signal.sosfilt(_sos("lp", f, order), x, axis=-1)


def hp(x, f, order=2):
    return signal.sosfilt(_sos("hp", f, order), x, axis=-1)


def bp(x, lo, hi, order=2):
    return signal.sosfilt(_sos("bp", (lo, hi), order), x, axis=-1)


def peak(x, f, q):
    """Резонансный полосовой фильтр (форманта): пик на f с добротностью q, усиление 1 на f."""
    b, a = signal.iirpeak(min(f, SR / 2 * 0.95), q, fs=SR)
    return signal.lfilter(b, a, x)


def sweep_filter(x, kind, f_curve, order=2, block=256, q=None):
    """Фильтр с меняющейся частотой (f_curve — массив той же длины, Гц)."""
    out = np.zeros_like(x)
    zi = None
    for s in range(0, len(x), block):
        e = min(s + block, len(x))
        f = float(np.mean(f_curve[s:e]))
        if kind == "bpq":
            bw = f / q
            sos = _sos("bp", (max(20, f - bw / 2), f + bw / 2), order)
        else:
            sos = _sos(kind, f, order)
        if zi is None:
            zi = np.zeros((sos.shape[0], 2))
        out[s:e], zi = signal.sosfilt(sos, x[s:e], zi=zi)
    return out


# ---------------------------------------------------------------- огибающие

def env_exp(n, tau, attack=0.002):
    t = t_axis(n)
    a = np.clip(t / max(attack, 1e-5), 0, 1)
    return a * np.exp(-t / tau)


def env_ad(n, attack, decay_shape=1.0):
    """Подъём за attack секунд, затем спад до нуля к концу (степень decay_shape)."""
    t = t_axis(n)
    na = max(1, secs(attack))
    e = np.ones(n)
    e[:na] = np.linspace(0, 1, na) if na < n else np.linspace(0, 1, n)[:na]
    if na < n:
        e[na:] = np.linspace(1, 0, n - na) ** decay_shape
    return e


def bell(n, center=0.5, width=0.25):
    t = np.linspace(0, 1, n)
    return np.exp(-0.5 * ((t - center) / width) ** 2)


def fade(x, fin=0.003, fout=0.01):
    x = x.copy()
    a, b = secs(fin), secs(fout)
    if a > 0:
        x[..., :a] *= np.linspace(0, 1, a)
    if b > 0:
        x[..., -b:] *= np.linspace(1, 0, b)
    return x


def smooth_random(rng, n, rate_hz, lo=0.0, hi=1.0):
    """Плавная случайная кривая (интерполяция случайных точек с частотой rate_hz)."""
    pts = max(4, int(n / SR * rate_hz) + 4)
    v = np.array([rng.u(lo, hi) for _ in range(pts)])
    xp = np.linspace(0, n, pts)
    # косинусная интерполяция
    idx = np.arange(n)
    k = np.searchsorted(xp, idx, side="right") - 1
    k = np.clip(k, 0, pts - 2)
    f = (idx - xp[k]) / (xp[k + 1] - xp[k])
    f = (1 - np.cos(f * np.pi)) / 2
    return v[k] * (1 - f) + v[k + 1] * f


# ---------------------------------------------------------------- резонаторы

def modes(n, freqs, decays, amps, rng=None, phase_rand=True):
    """Сумма затухающих синусоид (модальный синтез ударов по телам)."""
    t = t_axis(n)
    out = np.zeros(n)
    for f, d, a in zip(freqs, decays, amps):
        if f >= SR / 2 * 0.95:
            continue
        ph = rng.u(0, 2 * np.pi) if (rng and phase_rand) else 0.0
        out += a * np.sin(2 * np.pi * f * t + ph) * np.exp(-t / d)
    return out


def strike(rng, n, freqs, decays, amps, click=0.3, click_lp=6000, attack=0.0008):
    x = modes(n, freqs, decays, amps, rng)
    x *= np.clip(t_axis(n) / attack, 0, 1)
    if click > 0:
        c = rng.noise(n) * env_exp(n, 0.004, 0.0003)
        x += click * lp(c, click_lp) * np.max(np.abs(amps))
    return x


def place(buf, x, at, gain=1.0):
    """Вставить x в buf начиная с отсчёта at (с обрезкой)."""
    if at >= buf.shape[-1]:
        return buf
    if at < 0:
        x = x[..., -at:]
        at = 0
    m = min(x.shape[-1], buf.shape[-1] - at)
    buf[..., at:at + m] += gain * x[..., :m]
    return buf


# ---------------------------------------------------------------- голос (формантный синтез)

VOWELS = {
    # F1, F2, F3, F4 (мужской голос), ширины полос берём от частоты
    "a": (730, 1090, 2440, 3400),
    "o": (500, 850, 2450, 3300),
    "u": (330, 820, 2300, 3200),
    "e": (480, 1800, 2550, 3500),
    "i": (300, 2250, 2950, 3600),
    "ae": (660, 1700, 2400, 3400),
    "uh": (560, 1150, 2400, 3300),
    "schwa": (500, 1400, 2400, 3300),
}


def glottal(rng, f0_curve, jitter=0.004, shimmer=0.03, tilt=1.6, nharm=60):
    """Источник голоса: гармоники с наклоном спектра, дрожание высоты и громкости."""
    n = len(f0_curve)
    jit = 1 + jitter * smooth_random(rng, n, 30, -1, 1)
    f0 = f0_curve * jit
    phase = 2 * np.pi * np.cumsum(f0) / SR
    out = np.zeros(n)
    for k in range(1, nharm + 1):
        fk = f0 * k
        mask = fk < SR / 2 * 0.9
        if not mask.any():
            break
        out += np.where(mask, np.sin(k * phase) / k ** tilt, 0.0)
    sh = 1 + shimmer * smooth_random(rng, n, 40, -1, 1)
    return out * sh


def formant_filter(x, vowel, scale=1.0, bws=(80, 90, 120, 150), gains=(1.0, 0.6, 0.3, 0.15)):
    fs = VOWELS[vowel] if isinstance(vowel, str) else vowel
    out = np.zeros_like(x)
    for f, bw, g in zip(fs, bws, gains):
        f *= scale
        out += g * peak(x, f, f / (bw * scale))
    return out


def vowel_morph(x, vowels, scale=1.0):
    """Плавный переход между гласными (вдоль звука): фильтруем каждой и смешиваем."""
    n = len(x)
    k = len(vowels)
    if k == 1:
        return formant_filter(x, vowels[0], scale)
    outs = [formant_filter(x, v, scale) for v in vowels]
    pos = np.linspace(0, k - 1, n)
    res = np.zeros(n)
    for i in range(k):
        w = np.clip(1 - np.abs(pos - i), 0, 1)
        res += outs[i] * w
    return res


# ---------------------------------------------------------------- реверберация

def reverb_ir(rng, rt60=1.5, length=None, predelay=0.012, bright=0.5, stereo=True, early=8):
    """Синтетическая импульсная характеристика зала: ранние отражения + шумовой хвост,
    высокие затухают быстрее низких."""
    length = length or rt60 * 1.2
    n = secs(length)
    chans = []
    for c in range(2 if stereo else 1):
        t = t_axis(n)
        bands = [(20, 400, 1.0), (400, 2000, 0.8), (2000, 6000, 0.55 + 0.3 * bright), (6000, 16000, 0.3 + 0.4 * bright)]
        ir = np.zeros(n)
        for lo, hi, k in bands:
            w = bp(rng.noise(n), lo, hi)
            ir += w * np.exp(-6.9 * t / (rt60 * k))
        pd = secs(predelay)
        ir = np.concatenate([np.zeros(pd), ir])[:n]
        for _ in range(early):
            at = pd + secs(rng.u(0.002, 0.06))
            if at < n:
                ir[at] += rng.u(-1, 1) * 3.0
        ir *= np.clip(t / 0.003, 0, 1)
        chans.append(ir / np.sqrt(np.sum(ir ** 2)))
    return np.array(chans)


def apply_reverb(x, ir, wet=0.3, dry=1.0):
    """x — моно (n) или стерео (2, n). Возвращает стерео той же длины (+ хвост)."""
    if x.ndim == 1:
        x = np.array([x, x])
    n = x.shape[1] + ir.shape[1] - 1
    out = np.zeros((2, n))
    for c in range(2):
        out[c] = signal.fftconvolve(x[c], ir[c % ir.shape[0]])
    res = np.zeros((2, n))
    res[:, :x.shape[1]] += dry * x
    res += wet * out
    return res


# ---------------------------------------------------------------- сведение

def normalize(x, peak_db=-1.0):
    m = np.max(np.abs(x)) + 1e-12
    return x * (10 ** (peak_db / 20) / m)


def rms_db(x):
    return 20 * np.log10(np.sqrt(np.mean(x ** 2)) + 1e-12)


def loop_seam(x, xfade):
    """Сделать звук бесшовно зацикленным: хвост длиной xfade накладывается на начало.
    x — (…, n), возвращает (…, n - xfade)."""
    k = secs(xfade)
    body = x[..., :-k].copy()
    tail = x[..., -k:]
    r = np.linspace(0, 1, k)
    fin, fout = np.sqrt(r), np.sqrt(1 - r)
    body[..., :k] = body[..., :k] * fin + tail * fout
    return body


def pan(x, p):
    """Моно -> стерео, p в -1..1 (равная мощность)."""
    a = (p + 1) * np.pi / 4
    return np.array([x * np.cos(a), x * np.sin(a)])


def soft_clip(x, drive=1.0):
    return np.tanh(x * drive) / np.tanh(drive)
