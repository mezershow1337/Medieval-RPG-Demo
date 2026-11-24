#!/usr/bin/env python3
"""Описание релиза «Сборка для Windows №N»: что нового с прошлой сборки.

Берёт PR, влитые в main между прошлым тегом build-* и этой сборкой, и пишет
по-русски: заголовок PR со ссылкой и короткие пункты «для игрока».

Откуда берутся пункты (первое, что найдётся):
  1. файл docs/release-notes/pr-<номер>.md в репозитории;
  2. раздел «## Для игрока» (или «## Что нового для игрока») в описании PR;
  3. ничего — тогда только заголовок PR.

Ещё отмечает, если сменился PROTOCOL_ID (src/protocol.rs): тогда хосту и
друзьям нужна именно эта сборка.

Запуск:
  python tools/release_notes.py --tag build-87 --sha <commit> --out notes.md
  python tools/release_notes.py --backfill 10          # дописать старые релизы
  python tools/release_notes.py --backfill 10 --dry-run

Нужны переменные GITHUB_REPOSITORY и GITHUB_TOKEN (в Actions есть сами;
без токена работает только чтение публичного репозитория).
Только стандартная библиотека Python.
"""

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

MARKER = "<!-- release-notes v1 -->"
ROOT = Path(__file__).resolve().parent.parent
NOTES_DIR = ROOT / "docs" / "release-notes"
REPO = os.environ.get("GITHUB_REPOSITORY", "mezershow1337/Medieval-RPG-Demo")
TOKEN = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
API = "https://api.github.com"

FOOTER = """---
**Как запустить:** распакуй архив целиком и запусти `MedievalRPG.exe` — откроется меню «Создать игру / Подключиться».
Подробности в `README-Как-играть.txt` внутри архива."""

PLAYER_HEADINGS = ("для игрока", "что нового для игрока", "что нового")


def api(path, method="GET", body=None, raw=False):
    url = path if path.startswith("http") else f"{API}{path}"
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Accept", "application/vnd.github.raw" if raw else "application/vnd.github+json")
    req.add_header("X-GitHub-Api-Version", "2022-11-28")
    req.add_header("User-Agent", "release-notes-script")
    if TOKEN:
        req.add_header("Authorization", f"Bearer {TOKEN}")
    if data is not None:
        req.add_header("Content-Type", "application/json")
    with urllib.request.urlopen(req, timeout=30) as r:
        payload = r.read()
    return payload.decode("utf-8") if raw else json.loads(payload or b"null")


def build_number(tag):
    m = re.fullmatch(r"build-(\d+)", tag or "")
    return int(m.group(1)) if m else None


def list_build_releases():
    out, page = [], 1
    while True:
        chunk = api(f"/repos/{REPO}/releases?per_page=100&page={page}")
        out += [r for r in chunk if build_number(r["tag_name"]) is not None]
        if len(chunk) < 100:
            break
        page += 1
    out.sort(key=lambda r: build_number(r["tag_name"]))
    return out


def previous_tag(releases, number):
    older = [r["tag_name"] for r in releases if build_number(r["tag_name"]) < number]
    return older[-1] if older else None


def commits_between(base, head):
    """Коммиты base..head (новые в конце). Пагинация compare — до 250+ коммитов."""
    commits, page = [], 1
    while True:
        cmp = api(f"/repos/{REPO}/compare/{base}...{head}?per_page=100&page={page}")
        chunk = cmp.get("commits", [])
        commits += chunk
        if len(chunk) < 100:
            break
        page += 1
    return commits


def prs_in_range(base, head):
    """Номера PR, влитых в этот промежуток (по порядку), и прямые коммиты в main."""
    if base is None:
        commits = [api(f"/repos/{REPO}/commits/{head}")]
    else:
        commits = commits_between(base, head)
    pr_numbers, direct = [], []
    pr_commit_shas = set()
    for c in commits:
        msg = c["commit"]["message"]
        first = msg.splitlines()[0] if msg else ""
        m = re.match(r"Merge pull request #(\d+)", first) or re.search(r"\(#(\d+)\)\s*$", first)
        if m:
            n = int(m.group(1))
            if n not in pr_numbers:
                pr_numbers.append(n)
            continue
        if len(c.get("parents", [])) == 1:
            direct.append(c)
    # Коммиты, пришедшие внутри PR, — не «прямые»: убираем их.
    for n in pr_numbers:
        try:
            for pc in api(f"/repos/{REPO}/pulls/{n}/commits?per_page=100"):
                pr_commit_shas.add(pc["sha"])
        except urllib.error.HTTPError:
            pass
    direct = [c for c in direct if c["sha"] not in pr_commit_shas]
    return pr_numbers, direct


def player_section(body):
    """Пункты из раздела «## Для игрока» описания PR."""
    if not body:
        return None
    lines = body.replace("\r\n", "\n").split("\n")
    grab, out = False, []
    for line in lines:
        h = re.match(r"^#{2,3}\s+(.*?)\s*$", line)
        if h:
            if grab:
                break
            grab = h.group(1).strip().lower().rstrip(":") in PLAYER_HEADINGS
            continue
        if grab:
            if line.strip() == "---":
                break
            out.append(line)
    text = "\n".join(out).strip()
    return text or None


def player_notes(pr):
    f = NOTES_DIR / f"pr-{pr['number']}.md"
    if f.is_file():
        text = f.read_text(encoding="utf-8").strip()
        if text:
            return text
    return player_section(pr.get("body"))


def protocol_id(ref):
    try:
        src = api(f"/repos/{REPO}/contents/src/protocol.rs?ref={ref}", raw=True)
    except urllib.error.HTTPError:
        return None
    m = re.search(r"PROTOCOL_ID\s*:\s*u64\s*=\s*0x([0-9A-Fa-f_]+)", src)
    # Показываем только младшие 4 цифры: «0022» → «0023».
    return f"{int(m.group(1).replace('_', ''), 16) & 0xFFFF:04X}" if m else None


def render(tag, head, base):
    number = build_number(tag)
    pr_numbers, direct = prs_in_range(base, head)
    parts = [MARKER, f"## Что нового в сборке №{number}", ""]
    if base:
        parts.append(f"С прошлой сборки №{build_number(base)}:")
        parts.append("")

    if not pr_numbers and not direct:
        parts.append("Код не менялся — сборка пересобрана заново.")
        parts.append("")

    for n in pr_numbers:
        try:
            pr = api(f"/repos/{REPO}/pulls/{n}")
        except urllib.error.HTTPError:
            parts += [f"### #{n}", ""]
            continue
        parts.append(f"### {pr['title'].strip()} — [#{n}]({pr['html_url']})")
        notes = player_notes(pr)
        if notes:
            parts.append(notes)
        parts.append("")

    if direct:
        parts.append("### Прочие правки")
        for c in direct:
            first = c["commit"]["message"].splitlines()[0]
            parts.append(f"- {first} ([{c['sha'][:7]}]({c['html_url']}))")
        parts.append("")

    if base:
        old_p, new_p = protocol_id(base), protocol_id(head)
        if old_p and new_p and old_p != new_p:
            parts.append(
                f"> ⚠️ **Сменилась сетевая версия** ({old_p} → {new_p}). "
                "Хосту и всем друзьям нужна именно эта сборка, со старой подключиться не получится."
            )
        else:
            parts.append("> Сетевая версия та же — можно играть вместе с прошлой сборкой.")
        parts.append("")
        parts.append(f"Все изменения в коде: [{base}…{tag}](https://github.com/{REPO}/compare/{base}...{tag})")
        parts.append("")

    parts.append(FOOTER)
    return "\n".join(parts) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tag", help="тег новой сборки, например build-87")
    ap.add_argument("--sha", help="коммит сборки (по умолчанию — сам тег)")
    ap.add_argument("--out", help="куда записать описание")
    ap.add_argument("--backfill", type=int, default=0,
                    help="дописать описание последним N релизам, у которых его нет")
    ap.add_argument("--force", action="store_true", help="при --backfill переписать и готовые описания")
    ap.add_argument("--dry-run", action="store_true", help="только напечатать")
    a = ap.parse_args()
    # На Windows консоль Actions иначе не печатает кириллицу.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")

    releases = list_build_releases()

    if a.tag:
        number = build_number(a.tag)
        if number is None:
            sys.exit(f"не похоже на тег сборки: {a.tag}")
        text = render(a.tag, a.sha or a.tag, previous_tag(releases, number))
        if a.out:
            Path(a.out).write_text(text, encoding="utf-8")
        if a.dry_run or not a.out:
            print(text)

    if a.backfill:
        for r in releases[-a.backfill:]:
            if MARKER in (r.get("body") or "") and not a.force:
                continue
            tag = r["tag_name"]
            text = render(tag, tag, previous_tag(releases, build_number(tag)))
            if a.dry_run:
                print(f"===== {tag} =====\n{text}")
                continue
            api(f"/repos/{REPO}/releases/{r['id']}", method="PATCH", body={"body": text})
            print(f"описание {tag} обновлено")


if __name__ == "__main__":
    main()
