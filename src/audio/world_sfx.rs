//! Звуки мира и рук: двери, вещи (поднять, бросить, упало), монеты, сумка, работа руками
//! (перевязка, чтение, сбор трав, ступка, питьё), алхимия, голоса жителей.

use super::{AudioSet, Bank, Ears, Emitter, Sfx, SfxRng};
use crate::alchemy_fx::FxQueue;
use crate::brewing::{Heat, Vessel};
use crate::collision::LevelColliders;
use crate::doors::DoorLeaf;
use crate::items::ItemShape;
use crate::level::Role;
use crate::loot_client::{GroundView, LootState};
use crate::lore_net::BurstFx;
use crate::props_client::PropMotion;
use crate::residents::{ResidentAim, ResidentView};
use bevy::prelude::*;
use std::collections::HashMap;

pub struct WorldSfxPlugin;

impl Plugin for WorldSfxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AlchemyLoops>().add_systems(
            Update,
            (
                doors,
                (ground_items, picked_up),
                (activities, purse_and_bag, windows),
                (residents_talk, civilians_hurt),
                alchemy.before(crate::alchemy_fx::AlchemyFxSet),
            )
                .before(AudioSet),
        );
    }
}

// ---------------------------------------------------------------- двери

#[derive(Default)]
struct DoorMemory {
    open: HashMap<u32, bool>,
    since: Option<f32>,
}

fn doors(time: Res<Time>, level: Option<Res<LevelColliders>>, ears: Res<Ears>, leaves: Query<(&DoorLeaf, &GlobalTransform)>, mut mem: Local<DoorMemory>, mut out: MessageWriter<Sfx>) {
    let Some(level) = level else { return };
    let now = time.elapsed_secs();
    let since = *mem.since.get_or_insert(now);
    // Первые секунды приходит состояние всех дверей сразу — это не звуки.
    let quiet = now - since < 4.0;
    let mut played = 0;
    for (leaf, gt) in &leaves {
        let open = level.door_open(leaf.id as usize);
        let prev = mem.open.insert(leaf.id, open);
        if quiet || prev.is_none() || prev == Some(open) || played >= 3 {
            continue;
        }
        let p = gt.translation() + Vec3::Y * 1.0;
        if ears.valid && p.distance(ears.pos) > 30.0 {
            continue;
        }
        played += 1;
        out.write(Sfx::at(if open { "door_open" } else { "door_close" }, p).gain(0.8).range(2.0, 30.0));
    }
}

// ---------------------------------------------------------------- вещи

/// Каким звуком падает вещь.
pub fn drop_sound(shape: ItemShape, name: &str) -> &'static str {
    let lower = name.to_lowercase();
    let soft = ["кожан", "льнян", "шерст", "бинт", "ткан", "плащ", "туник", "трав", "лист", "корен", "цвет", "войлок", "сумк", "мешоч"];
    if soft.iter().any(|s| lower.contains(s)) {
        return "drop_soft";
    }
    match shape {
        ItemShape::Blade | ItemShape::Blunt | ItemShape::Helmet => "drop_metal",
        ItemShape::Armor => {
            if lower.contains("стёган") || lower.contains("стеган") {
                "drop_soft"
            } else {
                "drop_metal"
            }
        }
        ItemShape::Polearm | ItemShape::Shield => "drop_wood",
        ItemShape::Coins => "coin",
        ItemShape::Flask => "drop_ceramic",
        ItemShape::Small => {
            if ["слит", "крица", "желез", "медн", "бронз", "клещ", "молот", "нож"].iter().any(|s| lower.contains(s)) {
                "drop_metal"
            } else if ["горш", "амфор", "кувшин", "чаш", "стекл", "реторт", "алемб", "тигел", "склян"].iter().any(|s| lower.contains(s)) {
                "drop_ceramic"
            } else if ["книг", "свит", "кодекс", "пергамен"].iter().any(|s| lower.contains(s)) {
                "drop_soft"
            } else {
                "drop_wood"
            }
        }
    }
}

#[derive(Default)]
struct GroundMemory {
    asleep: HashMap<Entity, (bool, Vec3, f32)>,
}

/// Брошенная или упавшая вещь легла на землю — стук по её материалу.
fn ground_items(time: Res<Time>, ears: Res<Ears>, q: Query<(Entity, &GroundView, &PropMotion)>, mut mem: Local<GroundMemory>, mut out: MessageWriter<Sfx>) {
    let now = time.elapsed_secs();
    let mut alive = Vec::with_capacity(mem.asleep.len());
    for (e, g, m) in &q {
        alive.push(e);
        let speed = m.vel.length();
        let peak = speed.max(mem.asleep.get(&e).map(|p| p.2).unwrap_or(0.0));
        let prev = mem.asleep.insert(e, (m.asleep, m.pos, peak));
        let Some((was_asleep, _, peak_speed)) = prev else { continue };
        if !was_asleep && m.asleep && now - m.at < 0.5 {
            if ears.valid && m.pos.distance(ears.pos) > 25.0 {
                continue;
            }
            let base = g.base.base();
            let name = drop_sound(base.shape, base.name);
            let kg = crate::weight::unit_weight(g.base) * g.count.max(1) as f32;
            let gain = (0.35 + 0.25 * kg.sqrt()).min(1.0) * (0.5 + 0.1 * peak_speed).min(1.0);
            out.write(Sfx::at(name, m.pos).gain(gain).range(1.5, 25.0).pitch((1.15 - 0.05 * kg).clamp(0.8, 1.2)));
            if let Some(v) = mem.asleep.get_mut(&e) {
                v.2 = 0.0;
            }
        }
    }
    mem.asleep.retain(|e, _| alive.contains(e));
}

/// Вещь с земли исчезла рядом с нами — подобрали (шорох сумки).
fn picked_up(ears: Res<Ears>, q: Query<(Entity, &Transform), With<GroundView>>, mut gone: RemovedComponents<GroundView>, mut last: Local<HashMap<Entity, Vec3>>, mut out: MessageWriter<Sfx>) {
    for (e, t) in &q {
        last.insert(e, t.translation);
    }
    for e in gone.read() {
        if let Some(p) = last.remove(&e)
            && ears.valid
            && p.distance(ears.pos) < 4.0
        {
            out.write(Sfx::at("rustle", p).gain(0.5).range(1.5, 12.0));
        }
    }
}

// ---------------------------------------------------------------- работа руками

/// Что звучит, пока идёт дело с такой подписью: звук и как часто (с).
pub fn activity_sound(verb: &str) -> Option<(&'static str, f32)> {
    let v = verb.to_lowercase();
    let has = |keys: &[&str]| keys.iter().any(|k| v.contains(k));
    Some(if has(&["перевяз", "бинт"]) {
        ("cloth_tear", 1.4)
    } else if has(&["пьёт", "пьет", "зелье", "отвар", "выпива"]) {
        ("gulp", 1.6)
    } else if has(&["чтени", "чита", "грамот", "книг", "свит"]) {
        ("page", 4.5)
    } else if has(&["собира", "срыва", "выкапыва", "корен"]) {
        ("herb_pick", 1.1)
    } else if has(&["ступк", "растира", "толч", "толоч"]) {
        ("mortar", 1.2)
    } else if has(&["прикладыва", "присыпа", "кладёт", "кладет", "мазь", "компресс"]) {
        ("rustle", 1.0)
    } else if has(&["ест", "съеда", "жуёт"]) {
        ("eat", 1.0)
    } else if has(&["урок", "учит", "работ"]) {
        ("page", 6.0)
    } else {
        return None;
    })
}

#[derive(Default)]
struct ActivityState {
    current: Option<(String, f32)>,
    next: f32,
}

fn activities(time: Res<Time>, loot: Option<Res<LootState>>, mut st: Local<ActivityState>, mut out: MessageWriter<Sfx>, mut rng: ResMut<SfxRng>) {
    let Some(loot) = loot else { return };
    let now = time.elapsed_secs();
    let cur = loot.using.as_ref().map(|(v, start, _)| (v.clone(), *start));
    if cur != st.current {
        st.current = cur.clone();
        st.next = now;
    }
    let Some((verb, _)) = cur else { return };
    let Some((name, every)) = activity_sound(&verb) else { return };
    if now >= st.next {
        out.write(Sfx::ui(name).gain(0.55).spread(0.08));
        st.next = now + every * rng.range(0.85, 1.2);
    }
}

#[derive(Default)]
struct BagMemory {
    purse: Option<u32>,
    items: Option<u32>,
}

/// Монеты в кошеле и вещи в сумке: звон при оплате/получении, шорох при перекладывании.
fn purse_and_bag(loot: Option<Res<LootState>>, trade: Option<Res<crate::trade_client::TradeState>>, mut mem: Local<BagMemory>, mut out: MessageWriter<Sfx>) {
    let Some(loot) = loot else { return };
    if !loot.is_changed() {
        return;
    }
    let Some(inv) = loot.inventory.as_ref() else { return };
    let purse = crate::economy::purse(inv);
    let items: u32 = inv.all_items().map(|i| i.count as u32).sum();
    let (old_purse, old_items) = (mem.purse.replace(purse), mem.items.replace(items));
    let (Some(op), Some(oi)) = (old_purse, old_items) else { return };
    let trading = trade.is_some_and(|t| t.window_open());
    if purse != op {
        let big = purse.abs_diff(op) >= crate::economy::NUMMI_PER_SOLIDUS / 4;
        out.write(Sfx::ui(if big || trading { "coins_pour" } else { "coin" }).gain(0.5));
    } else if items != oi {
        out.write(Sfx::ui("rustle").gain(0.4));
    }
}

/// Открыли сумку или окно торговца — кожаный клапан.
fn windows(loot: Option<Res<LootState>>, trade: Option<Res<crate::trade_client::TradeState>>, mut last: Local<(bool, bool)>, mut out: MessageWriter<Sfx>) {
    let bag = loot.is_some_and(|l| l.ui_open);
    let shop = trade.is_some_and(|t| t.window_open());
    if (bag && !last.0) || (shop && !last.1) {
        out.write(Sfx::ui("ui_open").gain(0.35));
    }
    *last = (bag, shop);
}

// ---------------------------------------------------------------- жители

pub fn voice_of(role: Role) -> &'static str {
    match role {
        Role::Housewife | Role::Weaver | Role::Cook => "murmur_f",
        Role::Child => "murmur_child",
        _ => "murmur_m",
    }
}

/// Жители отвечают на «E» и переговариваются между собой, когда рядом проходишь.
#[allow(clippy::too_many_arguments)]
fn residents_talk(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    aim: Option<Res<ResidentAim>>,
    loot: Option<Res<LootState>>,
    ears: Res<Ears>,
    people: Query<(&ResidentView, &Transform, &Visibility), Without<crate::civil_client::Fallen>>,
    mut next_chat: Local<f32>,
    mut rng: ResMut<SfxRng>,
    mut out: MessageWriter<Sfx>,
) {
    if !ears.valid {
        return;
    }
    let now = time.elapsed_secs();
    let ui = loot.is_some_and(|l| l.ui_open);
    if !ui
        && keys.just_pressed(KeyCode::KeyE)
        && let Some((_, role)) = aim.and_then(|a| a.0)
    {
        // Ближайший к взгляду житель этой роли.
        let best = people
            .iter()
            .filter(|(r, t, _)| r.role == role && t.translation.distance(ears.pos) < 4.0)
            .min_by(|a, b| a.1.translation.distance(ears.pos).total_cmp(&b.1.translation.distance(ears.pos)));
        if let Some((r, t, _)) = best {
            out.write(Sfx::at(voice_of(r.role), t.translation + Vec3::Y * 1.6 * t.scale.y).gain(0.7).range(1.5, 15.0).spread(0.05));
        }
    }
    if now < *next_chat {
        return;
    }
    *next_chat = now + rng.range(3.0, 8.0);
    let near: Vec<_> = people.iter().filter(|(_, t, v)| **v != Visibility::Hidden && t.translation.distance(ears.pos) < 14.0).collect();
    if near.is_empty() {
        return;
    }
    let (r, t, _) = near[rng.index(near.len())];
    out.write(Sfx::at(voice_of(r.role), t.translation + Vec3::Y * 1.6 * t.scale.y).gain(0.35).range(1.5, 14.0).spread(0.06));
}

/// Мирного человека ударили или убили: вскрик, падение тела.
fn civilians_hurt(mut events: MessageReader<crate::civil_client::CivilNet>, ears: Res<Ears>, people: Query<(&ResidentView, &Transform)>, mut out: MessageWriter<Sfx>) {
    use crate::civil_client::CivilNet;
    for ev in events.read() {
        match ev {
            CivilNet::Hit => {
                // Кого ударили, сервер не говорит: ближайший к нам житель.
                let near = people.iter().filter(|(_, t)| t.translation.distance(ears.pos) < 5.0).min_by(|a, b| a.1.translation.distance(ears.pos).total_cmp(&b.1.translation.distance(ears.pos)));
                let (pos, pitch) = match near {
                    Some((r, t)) => (t.translation + Vec3::Y * 1.5 * t.scale.y, if voice_of(r.role) == "murmur_m" { 1.0 } else { 1.35 }),
                    None => (ears.pos + ears.forward, 1.0),
                };
                out.write(Sfx::at("pain", pos).gain(0.7).pitch(pitch).range(2.0, 35.0));
            }
            CivilNet::Dead { corpse: Some((_, pos, _)), fresh: true, .. } => {
                out.write(Sfx::at("death", *pos + Vec3::Y * 0.8).gain(0.6).range(2.0, 35.0));
                out.write(Sfx::at("body_fall", *pos + Vec3::Y * 0.3).gain(0.8).range(2.0, 30.0));
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- алхимия

/// Кипящие сосуды: петля бульканья у каждой работы на огне.
#[derive(Resource, Default)]
struct AlchemyLoops(HashMap<u64, Entity>);

#[allow(clippy::too_many_arguments)]
fn alchemy(
    mut commands: Commands,
    queue: Option<Res<FxQueue>>,
    bank: Option<Res<Bank>>,
    ears: Res<Ears>,
    mut loops: ResMut<AlchemyLoops>,
    mut out: MessageWriter<Sfx>,
    mut emitters: Query<&mut Emitter>,
    time: Res<Time>,
    mut mortar_next: Local<HashMap<u64, (Vec3, f32)>>,
) {
    let (Some(queue), Some(bank)) = (queue, bank) else { return };
    for (pos, fx) in &queue.bursts {
        let p = *pos;
        let s = match fx {
            BurstFx::Done => Sfx::at("hiss", p).gain(0.35),
            BurstFx::Spoiled => Sfx::at("hiss", p).gain(0.5).pitch(0.8),
            BurstFx::BoilOver => Sfx::at("boil", p).gain(0.9),
            BurstFx::Shatter { glass: true } => Sfx::at("glass_clink", p).gain(1.0).pitch(1.4),
            BurstFx::Shatter { glass: false } => Sfx::at("drop_ceramic", p).gain(1.0).pitch(0.9),
            BurstFx::Ignition => Sfx::at("flame_burst", p).gain(1.0).range(3.0, 45.0),
            BurstFx::Fumes => Sfx::at("hiss", p).gain(0.4).pitch(0.7),
            BurstFx::Gathered => Sfx::at("herb_pick", p).gain(0.6),
            BurstFx::Bellows => Sfx::at("flame_burst", p).gain(0.35).pitch(1.3),
            BurstFx::Hiss => Sfx::at("hiss", p).gain(0.8),
        };
        out.write(s.range(2.0, 30.0));
        if matches!(fx, BurstFx::BoilOver) {
            out.write(Sfx::at("hiss", p).gain(0.6).range(2.0, 25.0));
        }
    }
    for (player, work) in &queue.works {
        if let Some(e) = loops.0.remove(player) {
            commands.entity(e).despawn();
        }
        mortar_next.remove(player);
        let Some(w) = work else { continue };
        let pos: Vec3 = w.pos.into();
        match w.vessel {
            Vessel::Mortar => {
                mortar_next.insert(*player, (pos, 0.0));
            }
            Vessel::DryingTray => {}
            _ if w.heat != Heat::None => {
                let Some(h) = bank.ambient.get("boiling") else { continue };
                let gain = match w.heat {
                    Heat::Low => 0.25,
                    Heat::Medium => 0.45,
                    _ => 0.7,
                };
                let em = Emitter { at: Some(pos + Vec3::Y * 0.3), follow: None, gain, near: 1.0, range: 14.0, channel: super::Channel::Sfx, fade: 1.0 };
                loops.0.insert(*player, super::spawn_loop(&mut commands, h.clone(), em, &ears));
            }
            _ => {}
        }
    }
    // Ступка: стук и растирание, пока идёт работа.
    let now = time.elapsed_secs();
    for (pos, next) in mortar_next.values_mut() {
        if now >= *next {
            out.write(Sfx::at("mortar", *pos + Vec3::Y * 0.2).gain(0.6).range(1.5, 15.0));
            *next = now + 1.25;
        }
    }
    // Петли, источник которых пропал, не держим.
    loops.0.retain(|_, e| emitters.get_mut(*e).is_ok());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activities_have_sounds() {
        assert_eq!(activity_sound("Перевязываете рану").unwrap().0, "cloth_tear");
        assert_eq!(activity_sound("Чтение").unwrap().0, "page");
        assert_eq!(activity_sound("Собираете").unwrap().0, "herb_pick");
        assert_eq!(activity_sound("Ступка: растирать").unwrap().0, "mortar");
        assert_eq!(activity_sound("Пьёте зелье").unwrap().0, "gulp");
    }

    #[test]
    fn every_used_sound_exists() {
        let names: Vec<&str> = super::super::bank_gen::SFX_VARIANTS.iter().map(|(n, _)| *n).collect();
        for s in ["drop_metal", "drop_wood", "drop_ceramic", "drop_soft", "coin", "coins_pour", "rustle", "murmur_m", "murmur_f", "murmur_child", "ui_open", "door_open", "door_close", "hiss", "boil", "flame_burst", "glass_clink", "herb_pick", "mortar"] {
            assert!(names.contains(&s), "{s}");
        }
        for (shape, name) in [(ItemShape::Blade, "Спата"), (ItemShape::Flask, "Склянка"), (ItemShape::Small, "Подорожник"), (ItemShape::Coins, "Фоллис")] {
            assert!(names.contains(&drop_sound(shape, name)));
        }
    }
}
