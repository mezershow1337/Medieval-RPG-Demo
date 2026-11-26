"""Пишет src/arsenal_table.rs (подключается из src/arsenal.rs) и блок видов предметов в src/items.rs из tools/arsenal_data.py.

    python3 tools/gen_arsenal.py
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "tools"))
import arsenal_data as D  # noqa: E402

FAM = {"sword": "Sword", "seax": "Seax", "axe": "Axe", "mace": "Mace", "spear": "Spear", "head": "Head"}
SHAPE = {"sword": "Blade", "seax": "Blade", "axe": "Blunt", "mace": "Blunt", "spear": "Polearm", "head": "Small"}


def rs(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def f(x):
    s = repr(float(x))
    return s


def arsenal_rs():
    out = []
    out.append("// Кованое оружие и наконечники стрел. СОЗДАНО tools/gen_arsenal.py из tools/arsenal_data.py —")
    out.append("// правьте данные там и запускайте генератор, а не этот файл.")
    out.append("")
    out.append("/// Семейство оружия: как его куют и как им бьются.")
    out.append("#[derive(Clone, Copy, Debug, PartialEq, Eq)]")
    out.append("pub enum Fam {")
    for v in FAM.values():
        out.append(f"    {v},")
    out.append("}")
    out.append("")
    out.append("/// Вид кованого оружия.")
    out.append("#[derive(Debug)]")
    out.append("pub struct Arm {")
    out.append("    /// Имя модели и иконки (assets/models/items/<key>.glb).")
    out.append("    #[allow(dead_code)]")
    out.append("    pub key: &'static str,")
    out.append("    pub name: &'static str,")
    out.append("    pub fam: Fam,")
    out.append("    /// Чья работа (образец).")
    out.append("    pub people: &'static str,")
    out.append("    /// Урон; у наконечников — прибавка урона стрелы, %.")
    out.append("    pub dmg: f32,")
    out.append("    /// Врождённая быстрота, %; у наконечников — пробивание доспеха, %.")
    out.append("    pub speed: f32,")
    out.append("    pub metal: u8,")
    out.append("    pub mass: f32,")
    out.append("    /// Лучший отпуск, °C (0 — не калят).")
    out.append("    pub temper: f32,")
    out.append("    pub weld: bool,")
    out.append("    pub needs: Option<Technique>,")
    out.append("    pub parts: &'static [(&'static str, u16)],")
    out.append("    /// Цена обычного качества, в долях (медная монета — 10).")
    out.append("    pub price: u32,")
    out.append("    pub kg: f32,")
    out.append("    pub desc: &'static str,")
    out.append("}")
    out.append("")
    out.append("pub const ARMS: &[Arm] = &[")
    for a in D.W:
        needs = f"Some(Technique::{a['needs']})" if a["needs"] else "None"
        parts = ", ".join(f"({rs(n)}, {c})" for n, c in a["parts"])
        out.append(
            f"    Arm {{ key: {rs(a['key'])}, name: {rs(a['name'])}, fam: Fam::{FAM[a['fam']]}, people: {rs(a['people'])}, dmg: {f(a['dmg'])}, speed: {f(a['speed'])}, "
            f"metal: {a['metal']}, mass: {f(a['mass'])}, temper: {f(a['temper'])}, weld: {str(a['weld']).lower()}, needs: {needs}, parts: &[{parts}], "
            f"price: {a['price']}, kg: {f(a['kg'])}, desc: {rs(a['desc'])} }},"
        )
    out.append("];")
    out.append("")
    return "\n".join(out)


def items_block():
    lines = ["    // <arsenal> — создано tools/gen_arsenal.py"]
    for a in D.W:
        if a["fam"] == "head":
            lines.append(f"    base!({rs(a['name'])}, {rs(a['short'])}, None, ({a['size'][0]}, {a['size'][1]}), None, 30, &[], Sh::Small, true),")
        else:
            lines.append(
                f"    base!({rs(a['name'])}, {rs(a['short'])}, Some(S::MainHand), ({a['size'][0]}, {a['size'][1]}), Some((Stat::WeaponDamage, {f(a['dmg'])})), 1, WEAPON_AFFIXES, Sh::{SHAPE[a['fam']]}, true),"
            )
    lines.append("    // </arsenal>")
    return "\n".join(lines)


def main():
    with open(os.path.join(ROOT, "src/arsenal_table.rs"), "w") as fh:
        fh.write(arsenal_rs())
    p = os.path.join(ROOT, "src/items.rs")
    s = open(p).read()
    block = items_block()
    if "// <arsenal>" in s:
        s = re.sub(r"    // <arsenal>.*?// </arsenal>", lambda _: block, s, count=1, flags=re.S)
    else:
        # В конец ITEM_BASES: номера прежних видов не сдвигаются.
        i = s.index("pub const ITEM_BASES: &[ItemBase] = &[")
        j = s.index("\n];", i)
        s = s[:j] + "\n" + block + s[j:]
    open(p, "w").write(s)
    print(f"{len(D.W)} видов: src/arsenal_table.rs и src/items.rs обновлены")


if __name__ == "__main__":
    main()
