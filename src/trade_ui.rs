//! Окна торговли: прилавок торговца, сделка (корзина), своя сумка; заказы, уроки, штрафы,
//! пожертвования; обмен между игроками; репутация у фракций (J).
//!
//! Мышь: ЛКМ по кнопкам. Shift+ЛКМ по «Взять»/«Отдать» — всю стопку сразу.
//! Колесо мыши прокручивает длинные списки.

#![allow(deprecated)] // Interaction: в Bevy 0.20 помечен устаревшим, но работает и проще.

use crate::economy::{format_money, format_money_short, purse};
use crate::factions::{Faction, Standing};
use crate::inventory_ui::UiFonts;
use crate::items::{Item, ItemUid, Quality};
use crate::loot_client::LootState;
use crate::trade::{Attention, MoneyPolicy};
use crate::trade_client::{TradeState, send};
use crate::trade_net::*;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy_renet::RenetClient;

use crate::ui_theme::{AppearChildren, Skin, ink, theme};

const TITLE: Color = ink::TITLE;
const TEXT: Color = ink::TEXT;
const DIM: Color = ink::DIM;
const GOOD: Color = ink::GOOD;
const BAD: Color = ink::BAD;
const GOLD: Color = ink::GOLD;

pub struct TradeUiPlugin;

impl Plugin for TradeUiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(BuiltRev::default()).add_systems(
            Update,
            (rebuild, click_buttons, scroll_lists, tick_order_timers).chain(),
        );
    }
}

/// Что делает кнопка.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum Act {
    Buy(ItemUid),
    Sell(ItemUid),
    Unbuy(ItemUid),
    Unsell(ItemUid),
    Steal(ItemUid),
    Deal,
    Clear,
    Order(u8),
    Collect(u64),
    Lesson(u8),
    PayFine,
    Donate(u32),
    Repair(ItemUid),
    Close,
    P2pAdd(ItemUid),
    P2pRemove(ItemUid),
    P2pCoins(i64),
    P2pConfirm(bool),
    P2pCancel,
}

#[derive(Component)]
struct Enabled(bool);

#[derive(Component)]
struct TradeRoot;

#[derive(Component)]
struct Scroll;

/// Текст отсчёта готовности заказа.
#[derive(Component)]
struct OrderTimer {
    remaining: f32,
    due: u32,
}

#[derive(Resource, Default, PartialEq)]
struct BuiltRev(u64, u64, bool);

fn text(fonts: &UiFonts, s: impl Into<String>, size: f32, color: Color, bold: bool) -> impl Bundle {
    (
        Text::new(s),
        TextFont {
            font: if bold {
                fonts.bold.clone()
            } else {
                fonts.regular.clone()
            }
            .into(),
            font_size: bevy::text::FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

fn quality_color(q: Quality) -> Color {
    let [r, g, b] = q.color();
    Color::srgb(r, g, b)
}

fn standing_color(s: Standing) -> Color {
    let [r, g, b] = s.color();
    Color::srgb(r, g, b)
}

fn panel<'a>(root: &'a mut ChildSpawnerCommands, width: f32) -> EntityCommands<'a> {
    root.spawn((
        Node {
            flex_direction: FlexDirection::Column,
            padding: Skin::Wood.padding(),
            row_gap: px(6),
            // Рама толще прежней рамки — расширяем окно, чтобы содержимому хватило места.
            width: px(width + 36.0),
            max_height: percent(94),
            ..default()
        },
        theme().skin(Skin::Wood),
        Interaction::default(),
        FocusPolicy::Pass,
    ))
}

/// Прокручиваемый список (колесо мыши).
fn scroll_list<'a>(p: &'a mut ChildSpawnerCommands, max_h: f32) -> EntityCommands<'a> {
    p.spawn((
        Scroll,
        Interaction::default(),
        FocusPolicy::Pass,
        ScrollPosition::default(),
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(3),
            max_height: px(max_h),
            overflow: Overflow::scroll_y(),
            ..default()
        },
    ))
}

fn button(p: &mut ChildSpawnerCommands, fonts: &UiFonts, label: &str, act: Act, enabled: bool) {
    let _ = fonts;
    theme().small_button(
        p,
        label,
        (act, Enabled(enabled), FocusPolicy::Pass),
        enabled,
    );
}

/// Строка товара: иконка, название (цвет качества), правее — подпись и кнопки.
fn item_row(
    p: &mut ChildSpawnerCommands,
    fonts: &UiFonts,
    icons: &crate::icons::ItemIcons,
    item: &Item,
    count: u16,
    right: impl FnOnce(&mut ChildSpawnerCommands),
) {
    p.spawn(Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: px(6),
        min_height: px(30),
        ..default()
    })
    .with_children(|r| {
        let (w, h) = item.size();
        let k = 26.0 / w.max(h) as f32;
        r.spawn((
            Node {
                width: px(30),
                height: px(30),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            theme().slot_image(false),
        ))
        .with_children(|i| {
            i.spawn((
                ImageNode::new(icons.get(item.base)),
                Node {
                    width: px(w as f32 * k),
                    height: px(h as f32 * k),
                    ..default()
                },
            ));
        });
        let name = if count > 1 {
            format!("{} ×{count}", item.info().name)
        } else {
            item.info().name.to_string()
        };
        let color = if item.info().has_quality && item.quality != Quality::Common {
            quality_color(item.quality)
        } else {
            TEXT
        };
        r.spawn((
            text(fonts, name, 14.0, color, false),
            Node {
                flex_grow: 1.0,
                ..default()
            },
        ));
        right(r);
    });
}

fn section(p: &mut ChildSpawnerCommands, fonts: &UiFonts, title: &str) {
    let _ = fonts;
    p.spawn((
        theme().title(title, 19.0),
        Node {
            margin: UiRect::top(px(6)),
            ..default()
        },
    ));
}

fn count_in(lines: &[Line], uid: ItemUid) -> u16 {
    lines
        .iter()
        .find(|(u, _)| *u == uid)
        .map(|(_, c)| *c)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Построение
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    state: Res<TradeState>,
    loot: Res<LootState>,
    fonts: Res<UiFonts>,
    icons: Res<crate::icons::ItemIcons>,
    time: Res<Time>,
    party: Option<Res<crate::map::Party>>,
    mut built: ResMut<BuiltRev>,
    roots: Query<Entity, With<TradeRoot>>,
) {
    let want = BuiltRev(state.rev, loot.rev, state.window_open());
    if *built == want {
        return;
    }
    let opening = !built.2 && want.2;
    *built = want;
    for e in &roots {
        commands.entity(e).despawn();
    }
    if !state.window_open() {
        return;
    }
    let elapsed = time.elapsed_secs() - state.view_at;
    commands
        .spawn((
            TradeRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(12),
                ..default()
            },
            theme().backdrop(),
            GlobalZIndex(12),
        ))
        .insert_if(AppearChildren, || opening)
        .with_children(|root| {
            if let Some(view) = &state.view {
                merchant_window(root, &fonts, &icons, &state, &loot, view, elapsed);
            } else if let Some(p2p) = &state.p2p {
                p2p_window(root, &fonts, &icons, &loot, p2p, &partner_name(party.as_deref(), p2p.partner));
            } else if state.rep_open {
                reputation_window(root, &fonts, &state);
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn merchant_window(
    root: &mut ChildSpawnerCommands,
    fonts: &UiFonts,
    icons: &crate::icons::ItemIcons,
    state: &TradeState,
    loot: &LootState,
    view: &MerchantView,
    elapsed: f32,
) {
    let standing = Standing::of(view.rep);
    let refused = view.refuse.is_some();
    let barter = matches!(view.money, MoneyPolicy::Barter { .. });
    let inv = loot.inventory.as_ref();

    // --- Прилавок ---
    panel(root, 430.0).with_children(|p| {
        p.spawn(theme().title(&view.name, 28.0));
        theme().divider(p);
        p.spawn(text(fonts, &view.title, 14.0, DIM, false));
        p.spawn(text(
            fonts,
            format!(
                "{} · {} ({:+.0})",
                view.faction.name(),
                standing.name(),
                view.rep
            ),
            13.0,
            standing_color(standing),
            false,
        ));
        p.spawn(text(
            fonts,
            format!("«{}»", view.greeting),
            13.0,
            DIM,
            false,
        ));
        if let Some(r) = &view.refuse {
            p.spawn(text(fonts, r, 14.0, BAD, true));
        }
        let watching = view.attention == Attention::Watching;
        p.spawn(text(
            fonts,
            format!("Торговец {}", view.attention.describe()),
            13.0,
            if watching { DIM } else { GOOD },
            false,
        ));
        section(
            p,
            fonts,
            if barter {
                "Товар (мена)"
            } else {
                "Прилавок"
            },
        );
        scroll_list(p, 300.0).with_children(|list| {
            if view.shelf.is_empty() {
                list.spawn(text(fonts, "Пусто — приходите позже", 13.0, DIM, false));
            }
            // Одинаковый товар из разных стопок показываем одной строкой.
            let mut groups: Vec<(&ShelfItem, u16, Option<ItemUid>)> = Vec::new();
            for s in &view.shelf {
                let left = s
                    .item
                    .count
                    .saturating_sub(count_in(&state.basket_buy, s.item.uid));
                let same = groups.iter_mut().find(|(g, _, _)| {
                    g.item.base == s.item.base
                        && g.item.quality == s.item.quality
                        && g.item.affixes.is_empty()
                        && s.item.affixes.is_empty()
                        && g.price == s.price
                });
                match same {
                    Some((_, total, next)) => {
                        *total += left;
                        if next.is_none() && left > 0 {
                            *next = Some(s.item.uid);
                        }
                    }
                    None => groups.push((s, left, (left > 0).then_some(s.item.uid))),
                }
            }
            for (s, left, next) in groups {
                let mut shown = s.item.clone();
                shown.count = left.max(1);
                item_row(list, fonts, icons, &shown, left, |r| {
                    r.spawn(text(fonts, format_money_short(s.price), 13.0, GOLD, false));
                    let uid = next.unwrap_or(s.item.uid);
                    button(r, fonts, "Взять", Act::Buy(uid), !refused && next.is_some());
                    if s.pocketable {
                        button(r, fonts, "Стянуть", Act::Steal(uid), next.is_some());
                    }
                });
            }
        });
        if !view.orders.is_empty() || !view.my_orders.is_empty() {
            section(p, fonts, "Работа на заказ (половина вперёд)");
            for o in &view.orders {
                p.spawn(Node {
                    column_gap: px(6),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|r| {
                    let what = if o.count > 1 {
                        format!("{} ×{}", o.item, o.count)
                    } else {
                        o.item.clone()
                    };
                    let line = format!(
                        "{what} — {}, ~{} мин",
                        format_money_short(o.price),
                        (o.secs / 60.0).ceil()
                    );
                    r.spawn((
                        text(fonts, line, 13.0, TEXT, false),
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                    ));
                    match &o.locked {
                        Some(why) => {
                            r.spawn(text(fonts, why, 12.0, DIM, false));
                        }
                        None => button(r, fonts, "Заказать", Act::Order(o.index), !refused),
                    }
                });
            }
            for o in &view.my_orders {
                p.spawn(Node {
                    column_gap: px(6),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|r| {
                    let what = if o.count > 1 {
                        format!("{} ×{}", o.item, o.count)
                    } else {
                        o.item.clone()
                    };
                    r.spawn((
                        text(fonts, format!("Ваш заказ: {what}"), 13.0, GOLD, false),
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                    ));
                    r.spawn((
                        text(fonts, "", 12.0, DIM, false),
                        OrderTimer {
                            remaining: o.remaining,
                            due: o.due,
                        },
                    ));
                    button(
                        r,
                        fonts,
                        "Забрать",
                        Act::Collect(o.id),
                        o.remaining - elapsed <= 0.0,
                    );
                });
            }
        }
        if !view.lessons.is_empty() {
            section(p, fonts, "Учёба и работа");
            for l in &view.lessons {
                p.spawn(Node {
                    column_gap: px(6),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|r| {
                    let money = if l.paid {
                        format!("плата {}", format_money_short(l.money))
                    } else {
                        format!("платят {}", format_money_short(l.money))
                    };
                    r.spawn((
                        text(
                            fonts,
                            format!("{} · {}, {:.0} с", l.title, money, l.secs),
                            13.0,
                            TEXT,
                            false,
                        ),
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                    ));
                    match &l.locked {
                        Some(why) => {
                            r.spawn(text(fonts, why, 12.0, DIM, false));
                        }
                        None => button(
                            r,
                            fonts,
                            if l.paid {
                                "Учиться"
                            } else {
                                "Работать"
                            },
                            Act::Lesson(l.index),
                            !refused && state.lesson.is_none(),
                        ),
                    }
                });
            }
        }
        if view.fines {
            section(p, fonts, "Штрафы");
            p.spawn(Node {
                column_gap: px(6),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|r| {
                let line = if state.fine > 0 {
                    format!("За вами штраф {}", format_money(state.fine))
                } else {
                    "За вами ничего нет".into()
                };
                r.spawn((
                    text(
                        fonts,
                        line,
                        13.0,
                        if state.fine > 0 { BAD } else { DIM },
                        false,
                    ),
                    Node {
                        flex_grow: 1.0,
                        ..default()
                    },
                ));
                button(r, fonts, "Уплатить", Act::PayFine, state.fine > 0);
            });
        }
        if !view.repairs.is_empty() {
            section(p, fonts, "Правка и заточка");
            for o in &view.repairs {
                p.spawn(Node {
                    column_gap: px(6),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|r| {
                    r.spawn((
                        text(fonts, format!("{} — {:.0}%", o.name, o.condition * 100.0), 15.0, TEXT, false),
                        Node {
                            flex_grow: 1.0,
                            ..default()
                        },
                    ));
                    button(r, fonts, &format_money_short(o.price), Act::Repair(o.uid), !refused);
                });
            }
        }
        if view.donations {
            section(p, fonts, "Милостыня на храм и ксенон");
            p.spawn(Node {
                column_gap: px(6),
                ..default()
            })
            .with_children(|r| {
                for (label, n) in [("1 фоллис", 40), ("10 фоллисов", 400), ("1 солид", 7200)]
                {
                    button(r, fonts, label, Act::Donate(n), !refused);
                }
            });
        }
    });

    // --- Сделка ---
    panel(root, 320.0).with_children(|p| {
        p.spawn(theme().title("Сделка", 28.0));
        theme().divider(p);
        let shelf_price = |uid: ItemUid| {
            view.shelf
                .iter()
                .find(|s| s.item.uid == uid)
                .map(|s| (s.item.clone(), s.price))
        };
        let bid_of = |uid: ItemUid| view.bids.iter().find(|b| b.uid == uid);
        let my_item = |uid: ItemUid| {
            inv.and_then(|i| i.grid.items.iter().find(|pl| pl.item.uid == uid))
                .map(|pl| pl.item.clone())
        };
        let mut buy_total = 0u32;
        let mut sell_total = 0u32;
        let mut contraband = false;
        section(p, fonts, "Вы берёте");
        if state.basket_buy.is_empty() {
            p.spawn(text(fonts, "—", 13.0, DIM, false));
        }
        for (uid, count) in &state.basket_buy {
            let Some((item, price)) = shelf_price(*uid) else {
                continue;
            };
            buy_total += price * *count as u32;
            item_row(p, fonts, icons, &item, *count, |r| {
                r.spawn(text(
                    fonts,
                    format_money_short(price * *count as u32),
                    12.0,
                    GOLD,
                    false,
                ));
                button(r, fonts, "×", Act::Unbuy(*uid), true);
            });
        }
        section(p, fonts, "Вы отдаёте");
        if state.basket_sell.is_empty() {
            p.spawn(text(fonts, "—", 13.0, DIM, false));
        }
        for (uid, count) in &state.basket_sell {
            let (Some(item), Some(bid)) = (my_item(*uid), bid_of(*uid)) else {
                continue;
            };
            let price = bid.price.unwrap_or(0);
            contraband |= bid.contraband;
            sell_total += price * *count as u32;
            item_row(p, fonts, icons, &item, *count, |r| {
                r.spawn(text(
                    fonts,
                    format_money_short(price * *count as u32),
                    12.0,
                    GOLD,
                    false,
                ));
                button(r, fonts, "×", Act::Unsell(*uid), true);
            });
        }
        p.spawn(Node {
            height: px(6),
            ..default()
        });
        let (verdict, color, ok) = if buy_total == 0 && sell_total == 0 {
            (
                "Выберите товар: «Взять» слева, «Отдать» справа".to_string(),
                DIM,
                false,
            )
        } else if barter {
            let need = buy_total.saturating_sub(sell_total);
            p.spawn(text(
                fonts,
                format!(
                    "Их товар: {}\nВаш по их мерке: {}",
                    format_money(buy_total),
                    format_money(sell_total)
                ),
                14.0,
                TEXT,
                false,
            ));
            if need > 0 {
                (
                    format!("Не хватает товара на {}", format_money(need)),
                    BAD,
                    false,
                )
            } else if sell_total > buy_total + buy_total / 5 + 200 {
                ("Хватает с лихвой — сдачи не будет".to_string(), GOLD, true)
            } else {
                ("Равная мена".to_string(), GOOD, true)
            }
        } else if buy_total >= sell_total {
            let pay = buy_total - sell_total;
            let have = inv.map(purse).unwrap_or(0);
            if pay > have {
                (
                    format!("Заплатить {} — не хватает", format_money(pay)),
                    BAD,
                    false,
                )
            } else {
                (format!("Заплатить {}", format_money(pay)), TEXT, true)
            }
        } else {
            let get = sell_total - buy_total;
            if get > view.cash {
                (
                    format!(
                        "Получить {} — в кассе только {}",
                        format_money(get),
                        format_money(view.cash)
                    ),
                    BAD,
                    false,
                )
            } else {
                (format!("Получить {}", format_money(get)), GOOD, true)
            }
        };
        p.spawn(text(fonts, verdict, 15.0, color, true));
        if contraband {
            p.spawn(text(
                fonts,
                "Продажа оружия и доспехов варварам запрещена законом. Если донесут — штраф.",
                12.0,
                BAD,
                false,
            ));
        }
        if barter {
            p.spawn(text(
                fonts,
                "За стенами платят товаром, а не монетой. Медь здесь не в чести, золото ценят.",
                12.0,
                DIM,
                false,
            ));
        }
        let any = !state.basket_buy.is_empty() || !state.basket_sell.is_empty();
        p.spawn(Node {
            column_gap: px(8),
            margin: UiRect::top(px(6)),
            ..default()
        })
        .with_children(|r| {
            button(
                r,
                fonts,
                "Ударить по рукам",
                Act::Deal,
                any && ok && !refused,
            );
            button(r, fonts, "Очистить", Act::Clear, any);
        });
        if let Some(said) = &state.said {
            p.spawn(text(fonts, said, 13.0, GOLD, false));
        }
        p.spawn(text(
            fonts,
            format!("Кошель: {}", format_money(inv.map(purse).unwrap_or(0))),
            14.0,
            GOLD,
            false,
        ));
        p.spawn(text(
            fonts,
            "Shift+ЛКМ — всю стопку · E или Esc — уйти",
            12.0,
            DIM,
            false,
        ));
        button(p, fonts, "Уйти", Act::Close, true);
    });

    // --- Своя сумка ---
    panel(root, 360.0).with_children(|p| {
        p.spawn(theme().title("Ваша сумка", 28.0));
        theme().divider(p);
        p.spawn(text(
            fonts,
            "Надетое не продаётся — сначала снимите в инвентаре (Tab).",
            12.0,
            DIM,
            false,
        ));
        scroll_list(p, 560.0).with_children(|list| {
            let Some(inv) = inv else { return };
            let mut items: Vec<&Item> = inv.grid.items.iter().map(|pl| &pl.item).collect();
            items.sort_by_key(|i| (i.base.0, i.uid));
            for item in items {
                let Some(bid) = view.bids.iter().find(|b| b.uid == item.uid) else {
                    continue;
                };
                let is_coin = crate::economy::coin_value(item.base).is_some();
                if is_coin && !barter {
                    continue;
                }
                let offered = count_in(&state.basket_sell, item.uid);
                let left = item.count.saturating_sub(offered);
                let mut shown = item.clone();
                shown.count = left;
                item_row(list, fonts, icons, &shown, left, |r| match bid.price {
                    Some(price) => {
                        let mark = if bid.contraband { "⚠ " } else { "" };
                        r.spawn(text(
                            fonts,
                            format!("{mark}{}", format_money_short(price)),
                            13.0,
                            if bid.contraband { BAD } else { GOLD },
                            false,
                        ));
                        button(
                            r,
                            fonts,
                            "Отдать",
                            Act::Sell(item.uid),
                            !refused && left > 0,
                        );
                    }
                    None => {
                        r.spawn(text(fonts, "не нужно", 12.0, DIM, false));
                    }
                });
            }
        });
    });
}

/// Имя другого игрока (его вводят в меню; сервер раздаёт их для карты).
pub fn partner_name(party: Option<&crate::map::Party>, id: u64) -> String {
    party.map_or_else(|| "Странник".to_string(), |p| p.name(id))
}

fn p2p_window(
    root: &mut ChildSpawnerCommands,
    fonts: &UiFonts,
    icons: &crate::icons::ItemIcons,
    loot: &LootState,
    p2p: &P2pState,
    partner: &str,
) {
    let inv = loot.inventory.as_ref();
    panel(root, 380.0).with_children(|p| {
        p.spawn(text(
            fonts,
            format!("Обмен: {partner}"),
            22.0,
            TITLE,
            true,
        ));
        section(p, fonts, "Вы отдаёте");
        for item in &p2p.mine {
            item_row(p, fonts, icons, item, item.count, |r| {
                button(r, fonts, "×", Act::P2pRemove(item.uid), true)
            });
        }
        p.spawn(Node {
            column_gap: px(6),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|r| {
            r.spawn((
                text(
                    fonts,
                    format!("Деньги: {}", format_money(p2p.my_nummi)),
                    14.0,
                    GOLD,
                    false,
                ),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
            ));
            for (label, d) in [
                ("−1 сол.", -7200i64),
                ("−1 ф.", -40),
                ("+1 ф.", 40),
                ("+10 ф.", 400),
                ("+1 сол.", 7200),
            ] {
                button(r, fonts, label, Act::P2pCoins(d), true);
            }
        });
        section(p, fonts, "Вы получаете");
        if p2p.theirs.is_empty() && p2p.their_nummi == 0 {
            p.spawn(text(fonts, "— пока ничего —", 13.0, DIM, false));
        }
        for item in &p2p.theirs {
            item_row(p, fonts, icons, item, item.count, |_| {});
        }
        if p2p.their_nummi > 0 {
            p.spawn(text(
                fonts,
                format!("Деньги: {}", format_money(p2p.their_nummi)),
                14.0,
                GOLD,
                false,
            ));
        }
        let status = match (p2p.i_confirmed, p2p.they_confirmed) {
            (true, true) => ("Оба согласны", GOOD),
            (true, false) => ("Ждём согласия партнёра", TEXT),
            (false, true) => ("Партнёр согласен — ваше слово", GOLD),
            (false, false) => ("Любое изменение снимает согласие обоих", DIM),
        };
        p.spawn(text(fonts, status.0, 14.0, status.1, true));
        p.spawn(Node {
            column_gap: px(8),
            ..default()
        })
        .with_children(|r| {
            if p2p.i_confirmed {
                button(r, fonts, "Передумать", Act::P2pConfirm(false), true);
            } else {
                button(r, fonts, "Согласен", Act::P2pConfirm(true), true);
            }
            button(r, fonts, "Отменить обмен", Act::P2pCancel, true);
        });
    });
    panel(root, 360.0).with_children(|p| {
        p.spawn(theme().title("Ваша сумка", 28.0));
        theme().divider(p);
        scroll_list(p, 560.0).with_children(|list| {
            let Some(inv) = inv else { return };
            for pl in &inv.grid.items {
                let item = &pl.item;
                if crate::economy::coin_value(item.base).is_some() {
                    continue;
                }
                let offered = p2p
                    .mine
                    .iter()
                    .find(|m| m.uid == item.uid)
                    .map(|m| m.count)
                    .unwrap_or(0);
                item_row(list, fonts, icons, item, item.count, |r| {
                    button(
                        r,
                        fonts,
                        "Предложить",
                        Act::P2pAdd(item.uid),
                        offered < item.count,
                    )
                });
            }
        });
    });
}

fn reputation_window(root: &mut ChildSpawnerCommands, fonts: &UiFonts, state: &TradeState) {
    panel(root, 620.0).with_children(|p| {
        p.spawn(theme().title("Херсон и округа, 510 год · Отношения", 28.0));
        theme().divider(p);
        if state.fine > 0 {
            p.spawn(text(fonts, format!("Вы в розыске: штраф {}. Уплатите казначею гарнизона, иначе горожане торговать не будут.", format_money(state.fine)), 14.0, BAD, true));
        }
        let Some(rep) = &state.rep else {
            p.spawn(text(fonts, "…", 14.0, DIM, false));
            return;
        };
        scroll_list(p, 620.0).with_children(|list| {
            for f in Faction::ALL {
                let v = rep.get(f);
                let s = Standing::of(v);
                list.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2), margin: UiRect::bottom(px(6)), ..default() }).with_children(|b| {
                    b.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|r| {
                        r.spawn((text(fonts, f.name(), 16.0, TITLE, true), Node { width: px(260), ..default() }));
                        r.spawn((text(fonts, format!("{} ({v:+.0})", s.name()), 14.0, standing_color(s), true), Node { width: px(170), ..default() }));
                        let where_ = if f.outside_walls() { "за стенами" } else { "в городе" };
                        r.spawn(text(fonts, where_, 12.0, DIM, false));
                    });
                    // Полоска −100..100.
                    theme().bar(b, Node { width: px(560), height: px(14), ..default() }, (v + 100.0) / 200.0, standing_color(s), ());
                    b.spawn(text(fonts, f.description(), 12.0, DIM, false));
                });
            }
        });
        p.spawn(text(fonts, "Лучше отношение — ниже цены, доступ к заказам и урокам. J или Esc — закрыть.", 12.0, DIM, false));
    });
}

// ---------------------------------------------------------------------------
// Кнопки
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn click_buttons(
    buttons: Query<(&Interaction, &Act, &Enabled), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<TradeState>,
    loot: Res<LootState>,
    mut client: Option<ResMut<RenetClient>>,
) {
    let Some(client) = client.as_mut() else {
        return;
    };
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    for (interaction, act, enabled) in &buttons {
        if *interaction != Interaction::Pressed || !enabled.0 {
            continue;
        }
        let state = &mut *state;
        match *act {
            Act::Buy(uid) => {
                let Some(view) = &state.view else { continue };
                let Some(s) = view.shelf.iter().find(|s| s.item.uid == uid) else {
                    continue;
                };
                let have = count_in(&state.basket_buy, uid);
                let add = if shift { s.item.count - have } else { 1 };
                bump(&mut state.basket_buy, uid, add as i32, s.item.count);
            }
            Act::Sell(uid) => {
                let Some(item) = loot
                    .inventory
                    .as_ref()
                    .and_then(|i| i.grid.items.iter().find(|p| p.item.uid == uid))
                    .map(|p| p.item.clone())
                else {
                    continue;
                };
                let have = count_in(&state.basket_sell, uid);
                // Монеты в мене кладём горстью.
                let step = if crate::economy::coin_value(item.base).is_some() {
                    10
                } else {
                    1
                };
                let add = if shift { item.count - have } else { step };
                bump(&mut state.basket_sell, uid, add as i32, item.count);
            }
            Act::Unbuy(uid) => bump(
                &mut state.basket_buy,
                uid,
                if shift { -9999 } else { -1 },
                u16::MAX,
            ),
            Act::Unsell(uid) => bump(
                &mut state.basket_sell,
                uid,
                if shift { -9999 } else { -1 },
                u16::MAX,
            ),
            Act::Steal(uid) => send(client, &TradeClientMessage::Steal { uid }),
            Act::Deal => send(
                client,
                &TradeClientMessage::Deal {
                    buy: state.basket_buy.clone(),
                    sell: state.basket_sell.clone(),
                },
            ),
            Act::Clear => {
                state.basket_buy.clear();
                state.basket_sell.clear();
            }
            Act::Order(i) => send(client, &TradeClientMessage::PlaceOrder { index: i }),
            Act::Collect(id) => send(client, &TradeClientMessage::CollectOrder { id }),
            Act::Lesson(i) => send(client, &TradeClientMessage::StartLesson { index: i }),
            Act::PayFine => send(client, &TradeClientMessage::PayFine),
            Act::Donate(n) => send(client, &TradeClientMessage::Donate { nummi: n }),
            Act::Repair(uid) => send(client, &TradeClientMessage::Repair { uid }),
            Act::Close => {
                if state.view.take().is_some() {
                    send(client, &TradeClientMessage::Close);
                }
            }
            Act::P2pAdd(uid) | Act::P2pRemove(uid) => {
                let Some(p2p) = &state.p2p else { continue };
                let mut lines: Vec<Line> = p2p.mine.iter().map(|i| (i.uid, i.count)).collect();
                let max = loot
                    .inventory
                    .as_ref()
                    .and_then(|i| i.grid.items.iter().find(|p| p.item.uid == uid))
                    .map(|p| p.item.count)
                    .unwrap_or(1);
                let delta = match *act {
                    Act::P2pAdd(_) if shift => max as i32,
                    Act::P2pAdd(_) => 1,
                    _ if shift => -9999,
                    _ => -1,
                };
                bump(&mut lines, uid, delta, max);
                send(
                    client,
                    &TradeClientMessage::P2pOffer {
                        items: lines,
                        nummi: p2p.my_nummi,
                    },
                );
            }
            Act::P2pCoins(d) => {
                let Some(p2p) = &state.p2p else { continue };
                let have = loot.inventory.as_ref().map(purse).unwrap_or(0) as i64;
                let nummi = (p2p.my_nummi as i64 + d).clamp(0, have) as u32;
                let lines: Vec<Line> = p2p.mine.iter().map(|i| (i.uid, i.count)).collect();
                send(
                    client,
                    &TradeClientMessage::P2pOffer {
                        items: lines,
                        nummi,
                    },
                );
            }
            Act::P2pConfirm(ok) => send(client, &TradeClientMessage::P2pConfirm(ok)),
            Act::P2pCancel => {
                send(client, &TradeClientMessage::P2pCancel);
                state.p2p = None;
            }
        }
        state.rev += 1;
    }
}

/// Изменить количество в корзине (0 — убрать строку).
fn bump(lines: &mut Vec<Line>, uid: ItemUid, delta: i32, max: u16) {
    let cur = count_in(lines, uid) as i32;
    let new = (cur + delta).clamp(0, max as i32) as u16;
    lines.retain(|(u, _)| *u != uid);
    if new > 0 {
        lines.push((uid, new));
    }
}

fn scroll_lists(
    scroll: Res<AccumulatedMouseScroll>,
    mut lists: Query<(&Interaction, &mut ScrollPosition, &ComputedNode), With<Scroll>>,
) {
    if scroll.delta.y == 0.0 {
        return;
    }
    let step = match scroll.unit {
        bevy::input::mouse::MouseScrollUnit::Line => 32.0,
        bevy::input::mouse::MouseScrollUnit::Pixel => 1.0,
    };
    for (i, mut pos, node) in &mut lists {
        if *i == Interaction::None {
            continue;
        }
        let max = (node.content_size().y - node.size().y).max(0.0) * node.inverse_scale_factor();
        pos.0.y = (pos.0.y - scroll.delta.y * step).clamp(0.0, max);
    }
}

fn tick_order_timers(
    time: Res<Time>,
    mut state: ResMut<TradeState>,
    mut timers: Query<(&OrderTimer, &mut Text)>,
) {
    let elapsed = time.elapsed_secs() - state.view_at;
    let mut became_ready = false;
    for (t, mut text) in &mut timers {
        let left = t.remaining - elapsed;
        let line = if left > 0.0 {
            format!("через {}:{:02}", (left / 60.0) as u32, (left % 60.0) as u32)
        } else {
            became_ready |= t.remaining > 0.0 && left > -time.delta_secs() * 1.5;
            format!("готово · доплатить {}", format_money_short(t.due))
        };
        if text.0 != line {
            text.0 = line;
        }
    }
    // Заказ дозрел — перестроить окно, чтобы кнопка «Забрать» ожила.
    if became_ready {
        state.rev += 1;
    }
}
