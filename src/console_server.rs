//! Консоль разработчика на сервере: проверяет права и выполняет читы.

use crate::collision::{Body, LevelColliders};
use crate::console_net::*;
use crate::items::{ITEM_BASES, ItemBaseId, Quality, roll_item};
use crate::loot_net::{CH_LOOT, LootServerMessage};
use crate::loot_server::LootWorld;
use crate::npc::NpcSpawn;
use crate::protocol::*;
use crate::server::{self, GameWorld};
use bevy::prelude::*;
use bevy_renet::RenetServer;

pub struct ConsoleServerPlugin;

impl Plugin for ConsoleServerPlugin {
    fn build(&self, app: &mut App) {
        let all = std::env::var("MEDIEVAL_CHEATS").is_ok_and(|v| v != "0");
        app.insert_resource(ConsoleRules { cheats_for_all: all }).add_systems(Update, receive_commands);
    }
}

/// Читы для всех игроков (`sv_cheats 1` хозяина или `MEDIEVAL_CHEATS=1`).
#[derive(Resource, Default)]
pub struct ConsoleRules {
    pub cheats_for_all: bool,
}

/// Что получилось у команды.
struct Out {
    lines: Vec<(String, bool)>,
    cheats_changed: bool,
    teleport: Option<Vec3>,
    look: Option<(f32, f32)>,
}

impl Out {
    fn ok(&mut self, s: impl Into<String>) {
        self.lines.push((s.into(), false));
    }
    fn err(&mut self, s: impl Into<String>) {
        self.lines.push((s.into(), true));
    }
}

/// Контекст одной команды.
struct Ctx<'a> {
    server: &'a mut RenetServer,
    world: &'a mut GameWorld,
    level: &'a LevelColliders,
    loot: Option<&'a mut LootWorld>,
    bodies: Option<&'a mut crate::medicine_server::Bodies>,
    rules: &'a mut ConsoleRules,
    now: f64,
    pid: u64,
    host: bool,
}

#[allow(clippy::too_many_arguments)]
fn receive_commands(
    mut server: ResMut<RenetServer>,
    mut world: ResMut<GameWorld>,
    level: Res<LevelColliders>,
    mut loot: Option<ResMut<LootWorld>>,
    mut bodies: Option<ResMut<crate::medicine_server::Bodies>>,
    mut rules: ResMut<ConsoleRules>,
    time: Res<Time>,
) {
    for pid in server.clients_id() {
        while let Some(bytes) = server.receive_message(pid, CH_CONSOLE) {
            let Some(ConsoleClientMessage::Command { line, token }) = decode::<ConsoleClientMessage>(&bytes) else { continue };
            let host = token != 0 && token == host_token();
            let mut ctx = Ctx {
                server: &mut server,
                world: &mut world,
                level: &level,
                loot: loot.as_deref_mut(),
                bodies: bodies.as_deref_mut(),
                rules: &mut rules,
                now: time.elapsed_secs_f64(),
                pid,
                host,
            };
            let mut out = Out { lines: Vec::new(), cheats_changed: false, teleport: None, look: None };
            run_command(&mut ctx, &line, &mut out);
            info!("Консоль игрока {pid}: {line}");
            let world = &mut *world;
            if let Some(pos) = out.teleport
                && let Some(a) = world.actors.get_mut(&pid)
            {
                a.pose.feet = pos;
                a.last_update = time.elapsed_secs_f64();
                server.send_message(pid, CH_RELIABLE, encode(&ServerMessage::Correction { pos: pos.into() }));
            }
            if let Some((yaw, pitch)) = out.look {
                server.send_message(pid, CH_CONSOLE, encode(&ConsoleServerMessage::Look { yaw, pitch }));
            }
            if out.cheats_changed
                && let Some(a) = world.actors.get(&pid)
            {
                server.send_message(pid, CH_CONSOLE, encode(&ConsoleServerMessage::Cheats(a.cheats)));
            }
            for (text, error) in out.lines {
                server.send_message(pid, CH_CONSOLE, encode(&ConsoleServerMessage::Reply { text, error }));
            }
        }
    }
}

fn run_command(ctx: &mut Ctx, line: &str, out: &mut Out) {
    let args = split_args(line);
    let Some(cmd) = args.first().map(|s| s.to_lowercase()) else { return };
    let arg = |i: usize| args.get(i).map(String::as_str);

    // Без читов: список игроков.
    if cmd == "who" || cmd == "players" {
        let mut ids: Vec<_> = ctx.world.actors.iter().filter(|(_, a)| a.kind == ActorKind::Player).map(|(id, a)| (*id, a.pose.feet, a.alive)).collect();
        ids.sort_by_key(|(id, _, _)| *id);
        for (id, p, alive) in ids {
            let me = if id == ctx.pid { " (вы)" } else { "" };
            let dead = if alive { "" } else { ", мёртв" };
            out.ok(format!("  {id}{me}: {:.1} {:.1} {:.1}{dead}", p.x, p.y, p.z));
        }
        return;
    }

    let allowed = ctx.host || ctx.rules.cheats_for_all;
    if !allowed {
        out.err("Читы может включить только хозяин игры (у него в консоли: sv_cheats 1).");
        return;
    }
    if cmd == "sv_cheats" {
        if !ctx.host {
            out.err("sv_cheats меняет только хозяин игры.");
            return;
        }
        match parse_toggle(arg(1), ctx.rules.cheats_for_all) {
            Some(v) => {
                ctx.rules.cheats_for_all = v;
                out.ok(if v { "Читы разрешены всем игрокам." } else { "Читы снова только у хозяина." });
            }
            None => out.err("sv_cheats 0 | 1"),
        }
        return;
    }

    let pid = ctx.pid;
    let Some(me) = ctx.world.actors.get_mut(&pid) else {
        out.err("Вас ещё нет в мире.");
        return;
    };
    let feet = me.pose.feet;
    let yaw = me.pose.yaw;
    match cmd.as_str() {
        "fly" | "noclip" | "god" | "notarget" => {
            let c = &mut me.cheats;
            let field = match cmd.as_str() {
                "fly" => &mut c.fly,
                "noclip" => &mut c.noclip,
                "god" => &mut c.god,
                _ => &mut c.notarget,
            };
            let Some(v) = parse_toggle(arg(1), *field) else {
                out.err(format!("{cmd} [вкл|выкл]"));
                return;
            };
            *field = v;
            // Сквозь стены — значит и летать.
            if cmd == "noclip" {
                c.fly = v;
            }
            if cmd == "fly" && !v {
                c.noclip = false;
            }
            out.cheats_changed = true;
            let what = match cmd.as_str() {
                "fly" => "Полёт (Пробел — вверх, Ctrl — вниз, Shift — быстрее)",
                "noclip" => "Полёт сквозь стены",
                "god" => "Бессмертие",
                _ => "Невидимость для врагов",
            };
            out.ok(format!("{what}: {}", if v { "вкл" } else { "выкл" }));
        }
        "speed" => {
            let Some(v) = arg(1).and_then(|s| s.replace(',', ".").parse::<f32>().ok()).filter(|v| v.is_finite() && *v > 0.0) else {
                out.err(format!("speed <множитель>, сейчас {}", me.cheats.speed));
                return;
            };
            me.cheats.speed = v.clamp(0.1, 20.0);
            out.cheats_changed = true;
            out.ok(format!("Скорость ×{}", me.cheats.speed));
        }
        "heal" => {
            me.fighter.hp = me.fighter.max_hp;
            me.fighter.stamina = me.fighter.max_stamina;
            if let Some(b) = ctx.bodies.as_deref_mut() {
                *b.body(pid) = Default::default();
                b.touch(pid);
            }
            out.ok("Здоровье, силы и раны восстановлены.");
        }
        "kill" => {
            if !me.alive {
                out.err("Вы и так мертвы.");
                return;
            }
            me.cheats.god = false;
            out.cheats_changed = true;
            me.fighter.hp = 0.0;
            server::kill(ctx.server, ctx.world, pid, None, ctx.now);
            out.ok("Вы погибли. Возрождение через несколько секунд.");
        }
        "killall" => {
            let r: f32 = arg(1).and_then(|s| s.parse().ok()).unwrap_or(30.0);
            let victims: Vec<u64> = ctx.world.actors.iter().filter(|(_, a)| a.alive && a.kind.is_npc() && a.pose.feet.distance(feet) <= r).map(|(id, _)| *id).collect();
            for id in &victims {
                server::kill(ctx.server, ctx.world, *id, Some(pid), ctx.now);
            }
            out.ok(format!("Убито людей в радиусе {r} м: {}", victims.len()));
        }
        "spawn" => {
            let Some(kind) = arg(1).and_then(parse_kind) else {
                out.err("spawn <stratiot|bucellarius|goth|alan|hun> [сколько] — враг-разбойник перед вами");
                return;
            };
            let n: usize = arg(2).and_then(|s| s.parse().ok()).unwrap_or(1).clamp(1, 10);
            let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
            let side = Vec3::new(fwd.z, 0.0, -fwd.x);
            for i in 0..n {
                let pos = feet + fwd * 4.0 + side * ((i as f32 - (n as f32 - 1.0) / 2.0) * 1.5);
                // Лицом к игроку.
                let face = yaw + std::f32::consts::PI;
                let id = server::spawn_npc(ctx.world, ctx.level, NpcSpawn::brigand(kind, pos, face), ctx.now);
                // Вызванные консолью не возрождаются.
                if let Some(a) = ctx.world.actors.get_mut(&id) {
                    a.forget_spawn();
                }
            }
            out.ok(format!("{} ×{n}", kind.name_ru()));
        }
        "tp" | "goto" => teleport(ctx, &args, out),
        "give" => give(ctx, &args, out),
        _ => out.err(format!("Неизвестная команда «{cmd}». Список — help.")),
    }
}

fn parse_kind(s: &str) -> Option<ActorKind> {
    let s = s.to_lowercase();
    let pairs: [(&[&str], ActorKind); 5] = [
        (&["stratiot", "stratiotes", "стратиот", "soldier"], ActorKind::Stratiotes),
        (&["bucellarius", "букелларий", "heavy"], ActorKind::Bucellarius),
        (&["goth", "гот"], ActorKind::GothWarrior),
        (&["alan", "алан", "lancer"], ActorKind::AlanLancer),
        (&["hun", "гунн", "archer", "лучник", "кочевник"], ActorKind::HunArcher),
    ];
    pairs.iter().find(|(names, _)| names.iter().any(|n| n.starts_with(&s) && s.len() >= 3)).map(|(_, k)| *k)
}

fn parse_f(s: &str) -> Option<f32> {
    s.replace(',', ".").parse::<f32>().ok().filter(|v| v.is_finite())
}

fn teleport(ctx: &mut Ctx, args: &[String], out: &mut Out) {
    let nums: Vec<f32> = args[1..].iter().filter_map(|s| parse_f(s)).collect();
    let target = if args.len() >= 4 && nums.len() == args.len() - 1 {
        // tp x y z [yaw pitch]
        if nums.len() >= 5 {
            out.look = Some((nums[3], nums[4].clamp(-1.5, 1.5)));
        }
        Vec3::new(nums[0], nums[1], nums[2])
    } else if let Some(name) = args.get(1) {
        let name = name.to_lowercase();
        if let Ok(id) = name.parse::<u64>() {
            // tp <id игрока>
            match ctx.world.actors.get(&id).filter(|a| a.kind == ActorKind::Player) {
                Some(a) => a.pose.feet + Vec3::X * 1.0,
                None => {
                    out.err(format!("Нет игрока {id} (список — who)."));
                    return;
                }
            }
        } else if let Some(p) = crate::level::place(&name) {
            p.pos
        } else {
            let found: Vec<_> = crate::level::places().iter().filter(|p| p.id.contains(&name) || p.name.to_lowercase().contains(&name)).collect();
            match found.as_slice() {
                [p] => p.pos,
                [] => {
                    out.err(format!("Нет места «{name}». Список мест — places."));
                    return;
                }
                many => {
                    out.err(format!("Подходит несколько мест: {}", many.iter().map(|p| p.id).collect::<Vec<_>>().join(", ")));
                    return;
                }
            }
        }
    } else {
        out.err("tp x y z [yaw pitch] | tp <место> | tp <id игрока>");
        return;
    };
    // Чтобы не застрять в стене, ищем рядом свободное место (кроме полёта сквозь стены).
    let noclip = ctx.world.actors.get(&ctx.pid).is_some_and(|a| a.cheats.noclip);
    let pos = if noclip || ctx.level.body_is_free(&Body::STANDING, target) { target } else { crate::npc::find_free_spot(ctx.level, target) };
    out.teleport = Some(pos);
    out.ok(format!("Перенос в {:.1} {:.1} {:.1}", pos.x, pos.y, pos.z));
}

/// Найти предмет по части названия.
pub fn find_items(query: &str) -> Vec<ItemBaseId> {
    if let Some(id) = ItemBaseId::find(query) {
        return vec![id];
    }
    let q = query.to_lowercase();
    if let Some(i) = ITEM_BASES.iter().position(|b| b.name.to_lowercase() == q) {
        return vec![ItemBaseId(i as u16)];
    }
    ITEM_BASES.iter().enumerate().filter(|(_, b)| b.name.to_lowercase().contains(&q)).map(|(i, _)| ItemBaseId(i as u16)).collect()
}

fn parse_quality(s: &str) -> Option<Quality> {
    if let Ok(i) = s.parse::<usize>() {
        return Quality::ALL.get(i).copied();
    }
    let s = s.to_lowercase();
    let en = ["poor", "common", "uncommon", "rare", "epic", "legendary"];
    Quality::ALL.into_iter().zip(en).find(|(q, e)| q.name().to_lowercase().starts_with(&s) || e.starts_with(&s)).map(|(q, _)| q)
}

fn give(ctx: &mut Ctx, args: &[String], out: &mut Out) {
    // give <название> [количество] [качество]; название из нескольких слов — в кавычках или подряд.
    let mut words: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    let mut quality = Quality::Common;
    let mut count: u32 = 1;
    if let Some(q) = words.last().and_then(|w| if w.parse::<u32>().is_ok() { None } else { parse_quality(w) })
        && words.len() > 1
    {
        quality = q;
        words.pop();
    }
    if let Some(n) = words.last().and_then(|w| w.parse::<u32>().ok())
        && words.len() > 1
    {
        count = n.clamp(1, 999);
        words.pop();
    }
    let name = words.join(" ");
    if name.is_empty() {
        out.err("give <название> [количество] [качество: ветхое…легендарное / rare…]. Найти название — items <часть>.");
        return;
    }
    let found = find_items(&name);
    let base = match found.as_slice() {
        [one] => *one,
        [] => {
            out.err(format!("Нет предмета «{name}». Поиск — items <часть названия>."));
            return;
        }
        many => {
            let names: Vec<_> = many.iter().take(12).map(|b| b.base().name).collect();
            out.err(format!("Подходит {}: {}{}", many.len(), names.join(", "), if many.len() > 12 { "…" } else { "" }));
            return;
        }
    };
    let Some(loot) = ctx.loot.as_deref_mut() else {
        out.err("Инвентарей на сервере нет.");
        return;
    };
    let stack = base.base().max_stack.max(1) as u32;
    let mut items = Vec::new();
    let mut left = count;
    while left > 0 {
        let n = left.min(stack);
        items.push(roll_item(base, quality, n as u16, &mut loot.rng, &mut loot.uids));
        left -= n;
    }
    let pid = ctx.pid;
    let Some(inv) = loot.inventories.get_mut(&pid) else {
        out.err("У вас ещё нет инвентаря.");
        return;
    };
    let rest = inv.fill(items, false);
    let inv_msg = LootServerMessage::Inventory(inv.clone());
    ctx.server.send_message(pid, CH_LOOT, encode(&inv_msg));
    let dropped = rest.len();
    if let Some(a) = ctx.world.actors.get(&pid) {
        let fwd = Vec3::new(-a.pose.yaw.sin(), 0.0, -a.pose.yaw.cos());
        for item in rest {
            loot.spawn_ground(ctx.server, a.pose.feet + Vec3::Y * 1.2 + fwd * 0.6, fwd * 1.5, item);
        }
    }
    let q = if base.base().has_quality { format!(" ({})", quality.name().to_lowercase()) } else { String::new() };
    out.ok(format!("Выдано: {} ×{count}{q}", base.base().name));
    if dropped > 0 {
        out.ok(format!("Не влезло в сумку — {dropped} шт. брошено перед вами."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_items_by_part_of_name() {
        let coins = find_items("фоллис");
        assert_eq!(coins.len(), 1);
        assert_eq!(coins[0].base().name, "Фоллис");
        assert!(find_items("несуществующая штука").is_empty());
    }

    #[test]
    fn parses_enemy_kinds_and_qualities() {
        assert_eq!(parse_kind("goth"), Some(ActorKind::GothWarrior));
        assert_eq!(parse_kind("стратиот"), Some(ActorKind::Stratiotes));
        assert_eq!(parse_kind("x"), None);
        assert_eq!(parse_quality("легенд"), Some(Quality::Legendary));
        assert_eq!(parse_quality("5"), Some(Quality::Legendary));
        assert_eq!(parse_quality("rare"), Some(Quality::Rare));
    }
}
