//! Товар на прилавках: что лежит на полке торговца (`MerchantState::shelf`), то и выложено
//! в мире настоящими предметами со своей физикой.
//!
//! * «E» на товаре — купить одну штуку (обычная сделка `deal`, с ценой по отношению к игроку);
//! * «G» — стащить в сумку (`steal`, как кнопка «Стащить» в окне торговца);
//! * взять в руки (ЛКМ) можно открыто: покрутить, переставить. Но если унести дальше
//!   [`CARRY_STEAL`] от места, это кража на глазах у торговца — шанс попасться выше, чем
//!   у ловкой руки, а большую вещь вроде амфоры видно всегда;
//! * сдвинутое или уроненное торговец через [`TIDY_AFTER`] секунд кладёт на место.

use super::*;
use crate::loot_server::{GroundItem, ShelfRequest, ShelfTag, ground_message};
use crate::loot_net::LootId;
use crate::props::PropBody;

/// Унёс дальше этого от места на прилавке (м) — кража.
pub const CARRY_STEAL: f32 = 1.6;
/// Через сколько секунд торговец возвращает сдвинутый товар на место.
const TIDY_AFTER: f32 = 12.0;
/// Как часто сверять выложенное с полкой.
const REFRESH_SECS: f64 = 1.0;
const GAP: f32 = 0.035;

/// Раскладка полки по столешницам прилавка: (номер стопки, центр тела, поворот).
pub fn layout(m: &MerchantState) -> Vec<(ItemUid, Vec3, Quat)> {
    let tables = stall_obbs(m.pos, m.yaw, &m.stall);
    // Ряды: (z ряда, x курсора, глубина ряда) на каждой столешнице.
    let mut rows: Vec<(f32, f32, f32)> = tables.iter().map(|t| (-t.half.z + 0.05, -t.half.x + 0.05, 0.0)).collect();
    // Крупное — первым, чтобы ряды были ровнее.
    let mut items: Vec<&Item> = m.shelf.iter().collect();
    items.sort_by(|a, b| {
        let fa = crate::icons::model_half_extents(a.base);
        let fb = crate::icons::model_half_extents(b.base);
        (fb[0].max(fb[2]) * fb[0].min(fb[2])).total_cmp(&(fa[0].max(fa[2]) * fa[0].min(fa[2])))
    });
    let mut out = Vec::new();
    for item in items {
        let h = crate::icons::model_half_extents(item.base);
        let half = Vec3::from_array(h).max(Vec3::splat(0.008));
        // Длинной стороной вдоль стола.
        let turned = half.z > half.x;
        let (fx, fz) = if turned { (half.z * 2.0, half.x * 2.0) } else { (half.x * 2.0, half.z * 2.0) };
        for (t, table) in tables.iter().enumerate() {
            let (min_x, max_x) = (-table.half.x + 0.05, table.half.x - 0.05);
            let max_z = table.half.z - 0.03;
            let (mut rz, mut rx, mut depth) = rows[t];
            let wide = fx > max_x - min_x;
            if wide && fx > max_x - min_x + 0.5 {
                continue;
            }
            if rx + fx > max_x || (wide && rx > min_x) {
                rz += depth + GAP;
                rx = min_x;
                depth = 0.0;
            }
            if rz + fz > max_z {
                continue;
            }
            // Слишком длинное (копьё) — по центру, со свесом.
            let cx = if wide { 0.0 } else { rx + fx * 0.5 };
            let local = Vec3::new(cx, table.half.y + half.y + 0.002, rz + fz * 0.5);
            let rot = if turned { table.rot * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) } else { table.rot };
            out.push((item.uid, table.center + table.rot * local, rot));
            rx = if wide { max_x } else { rx + fx + GAP };
            rows[t] = (rz, rx, depth.max(fz));
            break;
        }
    }
    out
}

fn shelf_props(loot: &LootWorld, id: MerchantId) -> Vec<(LootId, ItemUid, Option<u64>)> {
    loot.ground
        .iter()
        .filter_map(|(lid, g)| g.shelf.as_ref().filter(|t| t.merchant == id).map(|t| (*lid, t.shelf_uid, g.holder)))
        .collect()
}

/// Выложить полку заново, если на ней что-то поменялось (купили, украли, подвезли).
fn refresh_display(server: &mut RenetServer, tw: &TradeWorld, loot: &mut LootWorld, id: MerchantId) {
    let m = &tw.merchants[id.0 as usize];
    let want = layout(m);
    let shown = shelf_props(loot, id);
    let mut a: Vec<ItemUid> = want.iter().map(|w| w.0).collect();
    let mut b: Vec<ItemUid> = shown.iter().map(|s| s.1).collect();
    a.sort_unstable();
    b.sort_unstable();
    if a == b {
        return;
    }
    let on_shelf = |uid: ItemUid| m.shelf.iter().any(|i| i.uid == uid);
    let mut kept = Vec::new();
    for (lid, uid, holder) in shown {
        // Что держат в руках, не трогаем (если оно ещё продаётся).
        if holder.is_some() && on_shelf(uid) {
            kept.push(uid);
            continue;
        }
        loot.remove_ground(server, lid);
    }
    for (uid, pos, rot) in want {
        if kept.contains(&uid) {
            continue;
        }
        let Some(item) = m.shelf.iter().find(|i| i.uid == uid) else { continue };
        let mut piece = item.clone();
        piece.count = 1;
        let mut body = PropBody::for_item(piece.base, 1, pos, rot);
        body.place(pos, rot);
        let shelf = Some(ShelfTag { merchant: id, shelf_uid: uid, home: (pos, rot), away: 0.0 });
        loot.insert_ground(server, GroundItem { body, item: piece, shelf, holder: None });
    }
}

fn loot_notice(server: &mut RenetServer, player: u64, text: impl Into<String>) {
    notice(server, player, text);
}

fn price_info(tw: &TradeWorld, player: u64, id: MerchantId, uid: ItemUid) -> Option<(u32, u16, bool)> {
    let def = id.def();
    let m = &tw.merchants[id.0 as usize];
    let item = m.shelf.iter().find(|i| i.uid == uid)?;
    let standing = tw.standing(player, def.faction);
    let normal = def.stock.iter().find(|st| st.item == item.info().name).map(|st| st.max).unwrap_or(1);
    let count: u16 = m.shelf.iter().filter(|i| i.base == item.base).map(|i| i.count).sum();
    let (w, h) = item.size();
    let price = ask_price(def, item.base, item.quality, standing, count, normal);
    // Умение торговаться — та же скидка, что в окне лавки.
    let price = ((price as f32 * tw.haggle(player).0).round() as u32).max(price.min(1));
    Some((price, item.count, w * h <= MAX_STEAL_CELLS))
}

/// Товар унесли с прилавка в руках: заметил ли торговец.
#[allow(clippy::too_many_arguments)]
fn carried_off(
    server: &mut RenetServer,
    tw: &mut TradeWorld,
    loot: &mut LootWorld,
    game: &GameWorld,
    player: u64,
    lid: LootId,
    now: f64,
    crimes: &mut Vec<CrimeCommitted>,
) {
    let Some(tag) = loot.ground.get(&lid).and_then(|g| g.shelf.clone()) else { return };
    let id = tag.merchant;
    let def = id.def();
    let Some(item) = tw.merchants[id.0 as usize].shelf.iter().find(|i| i.uid == tag.shelf_uid).cloned() else { return };
    let Some(actor) = game.actors.get(&player) else { return };
    let agility = loot.inventories.get(&player).map(|inv| inv.equipped_stats().get(Stat::Agility)).unwrap_or(0.0);
    let grudge = tw.merchants[id.0 as usize].grudges.get(&player).is_some_and(|t| *t > now);
    let m = &mut tw.merchants[id.0 as usize];
    m.thefts.retain(|t| now - t < 900.0);
    let (w, h) = item.size();
    let attempt = TheftAttempt {
        vigilance: def.vigilance,
        attention: m.attention,
        cells: w * h,
        crouching: actor.pose.crouching,
        agility,
        recent_thefts: m.thefts.len() as u32,
        guards_near: def.faction == Faction::Garrison,
    };
    // Уносить в руках у всех на виду труднее, чем спрятать в рукав; большое видно всегда.
    let big = if w * h > MAX_STEAL_CELLS { 0.25 } else { 0.0 };
    let chance = if grudge { 1.0 } else { (theft_detection(&attempt) * 1.3 + big).min(0.97) };
    m.thefts.push(now);
    if tw.rng.f32() < chance {
        let said = caught_stealing(tw, player, id, &item, actor.pose.feet, now, crimes);
        loot.release(server, player, None);
        if let Some(g) = loot.ground.get_mut(&lid) {
            g.body.place(tag.home.0, tag.home.1);
            server.broadcast_message(CH_LOOT, encode(&LootServerMessage::PropRest { id: lid, pos: tag.home.0.into(), rot: tag.home.1.to_array() }));
        }
        loot_notice(server, player, format!("{said} Товар вернули на прилавок."));
        return;
    }
    let mut uids = loot.uids.clone();
    let Some(piece) = split_from(&mut tw.merchant(id).shelf, tag.shelf_uid, 1, &mut uids) else { return };
    loot.uids = uids;
    tw.stolen.insert(piece.uid, Stolen { from: id });
    tw.change_rep(player, Faction::Underworld, 1.0);
    let name = piece.info().name;
    if let Some(g) = loot.ground.get_mut(&lid) {
        g.item = piece;
        g.shelf = None;
        let msg = ground_message(lid, g);
        server.broadcast_message(CH_LOOT, encode(&msg));
        server.broadcast_message(CH_LOOT, encode(&LootServerMessage::PropHeld { id: lid, holder: g.holder }));
    }
    loot_notice(server, player, format!("Вы унесли «{name}» — {} не заметил. Не продавайте это ему: узнает.", def.name));
}

#[derive(Default)]
pub struct ShelfClock {
    next: f64,
}

#[allow(clippy::too_many_arguments)]
pub fn tick_shelves(
    mut commands: Commands,
    mut server: ResMut<RenetServer>,
    game: Res<GameWorld>,
    time: Res<Time>,
    mut loot: ResMut<LootWorld>,
    tw: Option<ResMut<TradeWorld>>,
    mut clock: Local<ShelfClock>,
) {
    let Some(mut tw) = tw else { return };
    let tw = &mut *tw;
    let loot = &mut *loot;
    let now = time.elapsed_secs_f64();
    let dt = time.delta_secs();
    let mut crimes = Vec::new();
    let mut dirty: Vec<MerchantId> = Vec::new();

    for req in std::mem::take(&mut loot.shelf_requests) {
        let (ShelfRequest::Buy { player, id } | ShelfRequest::Pocket { player, id } | ShelfRequest::Query { player, id }) = req;
        let Some(g) = loot.ground.get(&id) else { continue };
        let Some(tag) = g.shelf.clone() else { continue };
        let holder = g.holder;
        let merchant = tag.merchant;
        let at = g.pos();
        match req {
            ShelfRequest::Query { .. } => {
                let Some((price, stock, pocketable)) = price_info(tw, player, merchant, tag.shelf_uid) else { continue };
                let m = &tw.merchants[merchant.0 as usize];
                let msg = LootServerMessage::ShelfInfo {
                    id,
                    price,
                    stock,
                    merchant: merchant.def().name.to_string(),
                    attention: m.attention.describe(),
                    pocketable,
                    refuse: tw.refusal(player, merchant, now),
                };
                server.send_message(player, CH_LOOT, encode(&msg));
            }
            ShelfRequest::Buy { .. } => {
                if holder.is_some_and(|h| h != player) {
                    loot_notice(&mut server, player, "Это держит другой покупатель");
                    continue;
                }
                let said = match deal(tw, loot, player, merchant, &[(tag.shelf_uid, 1)], &[], at, now, &mut crimes) {
                    DealOutcome::Done { said } => format!("Куплено. {said}"),
                    DealOutcome::Caught { said } | DealOutcome::Failed(said) => said,
                };
                loot_notice(&mut server, player, said);
                send_inventory(&mut server, loot, player);
                refresh_merchant(&mut server, tw, loot, merchant, now);
                dirty.push(merchant);
            }
            ShelfRequest::Pocket { .. } => {
                if holder.is_some_and(|h| h != player) {
                    loot_notice(&mut server, player, "Это держит другой");
                    continue;
                }
                let said = match steal(tw, loot, &game, player, merchant, tag.shelf_uid, now, &mut crimes) {
                    Ok(s) | Err(s) => s,
                };
                if !said.is_empty() {
                    loot_notice(&mut server, player, said);
                }
                send_inventory(&mut server, loot, player);
                refresh_merchant(&mut server, tw, loot, merchant, now);
                dirty.push(merchant);
            }
        }
    }

    // Унесли в руках или сдвинули.
    let mut carried = Vec::new();
    for (lid, g) in loot.ground.iter_mut() {
        let Some(tag) = g.shelf.as_mut() else { continue };
        let off = g.body.pos.distance(tag.home.0);
        if let Some(p) = g.holder {
            tag.away = 0.0;
            if off > CARRY_STEAL {
                carried.push((p, *lid));
            }
        } else if off > 0.12 || g.body.rot.angle_between(tag.home.1) > 0.3 {
            tag.away += dt;
            if tag.away > TIDY_AFTER && (g.body.asleep || off > CARRY_STEAL) {
                tag.away = 0.0;
                g.body.place(tag.home.0, tag.home.1);
                server.broadcast_message(CH_LOOT, encode(&LootServerMessage::PropRest { id: *lid, pos: tag.home.0.into(), rot: tag.home.1.to_array() }));
            }
        } else {
            tag.away = 0.0;
        }
    }
    for (player, lid) in carried {
        if let Some(m) = loot.ground.get(&lid).and_then(|g| g.shelf.as_ref()).map(|t| t.merchant) {
            carried_off(&mut server, tw, loot, &game, player, lid, now, &mut crimes);
            refresh_merchant(&mut server, tw, loot, m, now);
            dirty.push(m);
        }
    }

    if now >= clock.next {
        clock.next = now + REFRESH_SECS;
        dirty = tw.merchants.iter().map(|m| m.id).collect();
    }
    dirty.sort_by_key(|m| m.0);
    dirty.dedup();
    for id in dirty {
        refresh_display(&mut server, tw, loot, id);
    }
    for c in crimes {
        commands.trigger(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stall_shows_goods_on_its_tables() {
        let mut rng = Rng::new(3);
        let mut uids = UidGen(1);
        for (i, def) in MERCHANTS.iter().enumerate() {
            let mut m = MerchantState {
                id: MerchantId(i as u16),
                pos: Vec3::new(10.0, 0.0, 5.0),
                yaw: 0.7,
                shelf: vec![],
                cash: def.cash,
                attention: Attention::Watching,
                attention_until: 0.0,
                next_restock: 0.0,
                grudges: HashMap::new(),
                thefts: vec![],
                stall: stall_layout(def.faction.outside_walls(), i % 2 == 0),
                dead: false,
            };
            restock(&mut m, &mut rng, &mut uids, true);
            let placed = layout(&m);
            assert!(!placed.is_empty() || m.shelf.is_empty(), "{}: пустой прилавок", def.name);
            let tables = stall_obbs(m.pos, m.yaw, &m.stall);
            for (uid, pos, _) in &placed {
                let item = m.shelf.iter().find(|it| it.uid == *uid).unwrap();
                // Лежит над какой-нибудь столешницей (центр внутри её прямоугольника сверху).
                let on = tables.iter().any(|t| {
                    let l = t.rot.inverse() * (*pos - t.center);
                    l.y > t.half.y && l.y < t.half.y + 0.4 && l.x.abs() <= t.half.x + 0.01 && l.z.abs() <= t.half.z + 0.01
                });
                assert!(on, "{}: «{}» не на прилавке", def.name, item.info().name);
            }
            // Ничего не лежит друг в друге.
            for a in 0..placed.len() {
                for b in a + 1..placed.len() {
                    let body = |k: usize| {
                        let it = m.shelf.iter().find(|it| it.uid == placed[k].0).unwrap();
                        PropBody::for_item(it.base, 1, placed[k].1, placed[k].2).obb()
                    };
                    let (x, y) = (body(a), body(b));
                    let overlap = x.aabb().overlaps(&y.aabb());
                    let flat = (x.center - y.center).with_y(0.0).length() < 0.02;
                    assert!(!(overlap && flat), "{}: товары лежат друг в друге", def.name);
                }
            }
        }
    }
}
