"""Собирает все звуки игры: python3 tools/audio/build.py [--only sfx|ambient|music] [имя ...]

Пишет OGG в assets/audio/{sfx,ambient,music}/ и таблицу src/audio/bank_gen.rs (сколько вариантов
у каждого звука). Нужны numpy, scipy и ffmpeg с libvorbis.
"""

import os
import zlib
import subprocess
import sys
import time

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from dsp import SR, Rng, normalize, rms_db  # noqa: E402
import sfx  # noqa: E402
import ambient  # noqa: E402
import music  # noqa: E402

ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
OUT = os.path.join(ROOT, "assets", "audio")


def write_ogg(path, x, quality=4):
    x = np.asarray(x, dtype=np.float64)
    if not np.all(np.isfinite(x)):
        raise ValueError(f"{path}: NaN/inf в звуке")
    ch = 1 if x.ndim == 1 else x.shape[0]
    data = (x if x.ndim == 1 else x.T).astype(np.float32).tobytes()
    os.makedirs(os.path.dirname(path), exist_ok=True)
    cmd = ["ffmpeg", "-y", "-loglevel", "error", "-f", "f32le", "-ar", str(SR), "-ac", str(ch), "-i", "-",
           "-c:a", "libvorbis", "-q:a", str(quality), path]
    subprocess.run(cmd, input=data, check=True)


def target_rms(x, db):
    cur = rms_db(x)
    y = x * 10 ** ((db - cur) / 20)
    m = np.max(np.abs(y))
    if m > 0.95:
        # мягкий лимитер вместо жёсткого обреза
        y = np.tanh(y / 0.95 * 1.2) * 0.95 / np.tanh(1.2)
    return y


def build_sfx(names):
    for name, (fn, count) in sfx.SFX.items():
        if names and name not in names:
            continue
        for i in range(count):
            rng = Rng(zlib.crc32(name.encode()) % 100000 * 31 + i * 7919 + 17)
            x = fn(rng)
            x = normalize(x, -1.0)
            write_ogg(os.path.join(OUT, "sfx", f"{name}_{i + 1}.ogg"), x, quality=4)
        print(f"  sfx {name} ×{count}")


def build_ambient(names):
    for name, fn in ambient.AMBIENT.items():
        if names and name not in names:
            continue
        rng = Rng(sum(map(ord, name)) * 131 + 5)
        x = fn(rng)
        x = target_rms(x, -20.0)
        write_ogg(os.path.join(OUT, "ambient", f"{name}.ogg"), x, quality=3)
        print(f"  ambient {name}: {x.shape[1] / SR:.1f} с")


def build_music(names):
    for name, fn in music.MUSIC.items():
        if names and name not in names:
            continue
        t0 = time.time()
        x = fn()
        x = target_rms(x, -19.0)
        write_ogg(os.path.join(OUT, "music", f"{name}.ogg"), x, quality=4)
        print(f"  music {name}: {x.shape[1] / SR:.1f} с ({time.time() - t0:.0f} с счёта)")


def duration(path):
    r = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path], capture_output=True, text=True, check=True)
    return float(r.stdout.strip())


def write_bank():
    lines = [
        "// Создано tools/audio/build.py — не править руками.",
        "/// Звук -> сколько у него вариантов (файлы assets/audio/sfx/<имя>_<n>.ogg).",
        "pub const SFX_VARIANTS: &[(&str, u8)] = &[",
    ]
    for name, (_fn, count) in sfx.SFX.items():
        lines.append(f'    ("{name}", {count}),')
    lines.append("];")
    lines.append("/// Фоновые петли (assets/audio/ambient/<имя>.ogg).")
    lines.append("pub const AMBIENT: &[&str] = &[" + ", ".join(f'"{n}"' for n in ambient.AMBIENT) + "];")
    lines.append("/// Музыка (assets/audio/music/<имя>.ogg) и длительность пьесы в секундах.")
    lines.append("pub const MUSIC: &[(&str, f32)] = &[" + ", ".join(f'("{n}", {duration(os.path.join(OUT, "music", n + ".ogg")):.1f})' for n in music.MUSIC) + "];")
    path = os.path.join(ROOT, "src", "audio", "bank_gen.rs")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    os.environ.setdefault("PYTHONHASHSEED", "0")
    args = sys.argv[1:]
    only = None
    if args[:1] == ["--only"]:
        only = args[1]
        args = args[2:]
    names = set(args)
    if only in (None, "sfx"):
        build_sfx(names)
    if only in (None, "ambient"):
        build_ambient(names)
    if only in (None, "music"):
        build_music(names)
    write_bank()
