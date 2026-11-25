//! Хотбар внизу экрана (как панель быстрого доступа в Dark and Darker, только механика).
//!
//! - Слот 1 — «Руки»: убрать оружие и драться кулаками.
//! - Слоты 2–9 — вещи из сумки: оружие берётся в правую руку, щит — в левую (повторное
//!   нажатие снимает щит), доспех надевается, бинт и зелье применяются, книги читаются.
//! - Цифры 1–9 выбирают слот, колесо мыши листает руки и оружие на панели.
//! - Привязать: в открытом инвентаре навести на вещь и нажать цифру 2–9, или взять вещь
//!   мышью и щёлкнуть по слоту. ПКМ по слоту в открытом инвентаре — очистить слот.
//!
//! Расходники привязываются по виду (бинт — любой бинт из сумки), остальное — по самому
//! предмету. Панель живёт на клиенте; на сервер уходят только «надеть», «применить» и
//! «убрать оружие» (`ClientMessage::Holster`).

#![allow(deprecated)] // Interaction: как в inventory_ui.rs

use crate::inventory::{Inventory, Loc, Target};
use crate::items::{EquipSlot, Item, ItemBaseId, ItemUid};
use crate::loot_client::{LootState, send};
use crate::loot_net::{LootClientMessage, Owner};
use crate::protocol::{CH_RELIABLE, ClientMessage, encode};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::RenetClient;

/// Сколько слотов (первый — «Руки»).
pub const SLOTS: usize = 9;
const SIZE: f32 = 54.0;

pub const KEYS: [KeyCode; SLOTS] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// Что лежит в слоте.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bind {
    /// Конкретный предмет (оружие, щит, доспех). `base` — чтобы рисовать, когда его нет в сумке.
    Item { uid: ItemUid, base: ItemBaseId },
    /// Любой предмет этого вида (расходники, стрелы, лекарства).
    Kind(ItemBaseId),
}

impl Bind {
    pub fn for_item(item: &Item) -> Self {
        if item.info().max_stack > 1 || item.base.consumable().is_some() {
            Bind::Kind(item.base)
        } else {
            Bind::Item { uid: item.uid, base: item.base }
        }
    }
    pub fn base(self) -> ItemBaseId {
        match self {
            Bind::Item { base, .. } | Bind::Kind(base) => base,
        }
    }
    /// Найти привязанный предмет в инвентаре.
    pub fn resolve(self, inv: &Inventory) -> Option<&Item> {
        match self {
            Bind::Item { uid, .. } => inv.find(uid).map(|(_, i)| i),
            Bind::Kind(base) => inv.all_items().find(|i| i.base == base),
        }
    }
    /// Сколько таких предметов в сумке (для подписи на слоте).
    fn count(self, inv: &Inventory) -> u32 {
        match self {
            Bind::Item { uid, .. } => inv.find(uid).map_or(0, |_| 1),
            Bind::Kind(base) => inv.all_items().filter(|i| i.base == base).map(|i| i.count as u32).sum(),
        }
    }
}

/// Что делать при выборе слота.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    /// Убрать оружие / взять снова (флаг «убрано»).
    Holster(bool),
    /// Переложить предмет (надеть или снять).
    Move { uid: ItemUid, target: Target, unholster: bool },
    /// Применить расходник.
    Use(ItemUid),
    /// Читать, лечить, поставить (lore_client::try_use).
    Lore(ItemUid),
    Nothing(String),
}

#[derive(Resource)]
pub struct Hotbar {
    pub slots: [Option<Bind>; SLOTS],
    /// Оружие убрано (дерёмся кулаками), хотя в правой руке что-то есть.
    pub holstered: bool,
    sent: Option<bool>,
    seeded: bool,
    /// Последнее оружие в правой руке (чтобы привязать новое).
    last_main: Option<ItemUid>,
}

impl Default for Hotbar {
    fn default() -> Self {
        Self { slots: [None; SLOTS], holstered: false, sent: None, seeded: false, last_main: None }
    }
}

impl Hotbar {
    /// Привязать предмет к слоту `i` (1..SLOTS). Если он уже был в другом слоте — переезжает.
    pub fn bind(&mut self, i: usize, item: &Item) {
        if i == 0 || i >= SLOTS {
            return;
        }
        let b = Bind::for_item(item);
        for s in self.slots.iter_mut() {
            if *s == Some(b) {
                *s = None;
            }
        }
        self.slots[i] = Some(b);
    }

    pub fn clear(&mut self, i: usize) {
        if i > 0 && i < SLOTS {
            self.slots[i] = None;
        }
    }

    /// Начальная раскладка и автопривязка нового оружия в правой руке.
    fn update_from_inventory(&mut self, inv: &Inventory) {
        if !self.seeded {
            self.seeded = true;
            if let Some(w) = inv.equipped(EquipSlot::MainHand) {
                self.slots[1] = Some(Bind::for_item(w));
            }
            if let Some(s) = inv.equipped(EquipSlot::OffHand) {
                self.slots[2] = Some(Bind::for_item(s));
            }
            // Привычные клавиши быстрого лечения: 4 — бинт, 5 — зелье.
            for (i, name) in [(3, "Бинт"), (4, "Целебное зелье")] {
                if let Some(base) = ItemBaseId::find(name) {
                    self.slots[i] = Some(Bind::Kind(base));
                }
            }
        }
        let main = inv.equipped(EquipSlot::MainHand);
        let main_uid = main.map(|i| i.uid);
        if main_uid != self.last_main {
            self.last_main = main_uid;
            if let Some(item) = main {
                let b = Bind::for_item(item);
                if !self.slots.contains(&Some(b))
                    && let Some(free) = (1..SLOTS).find(|i| self.slots[*i].is_none())
                {
                    self.slots[free] = Some(b);
                }
            }
        }
    }

    /// Какой слот сейчас «в руках»: 0 — руки, иначе слот с оружием из правой руки.
    pub fn active(&self, inv: &Inventory) -> usize {
        let Some(main) = inv.equipped(EquipSlot::MainHand).filter(|_| !self.holstered) else { return 0 };
        (1..SLOTS).find(|i| self.slots[*i].is_some_and(|b| b.resolve(inv).is_some_and(|it| it.uid == main.uid))).unwrap_or(usize::MAX)
    }

    /// Решить, что делает выбор слота `i`.
    pub fn plan(&self, i: usize, inv: &Inventory) -> Act {
        if i == 0 {
            return Act::Holster(true);
        }
        let Some(bind) = self.slots.get(i).copied().flatten() else { return Act::Nothing(format!("Слот {} пуст: наведите на вещь в инвентаре и нажмите {}", i + 1, i + 1)) };
        let Some(item) = bind.resolve(inv) else { return Act::Nothing(format!("Нет предмета: {}", bind.base().base().name)) };
        let loc = inv.find(item.uid).map(|(l, _)| l);
        match item.info().slot {
            Some(EquipSlot::MainHand) => {
                if loc == Some(Loc::Equip(EquipSlot::MainHand)) {
                    // Уже в руке: повторное нажатие убирает оружие и обратно.
                    Act::Holster(!self.holstered)
                } else {
                    Act::Move { uid: item.uid, target: Target::Equip(EquipSlot::MainHand), unholster: true }
                }
            }
            Some(EquipSlot::OffHand) if loc == Some(Loc::Equip(EquipSlot::OffHand)) => Act::Move { uid: item.uid, target: Target::Auto, unholster: false },
            Some(slot) if loc != Some(Loc::Equip(slot)) => Act::Move { uid: item.uid, target: Target::Equip(slot), unholster: false },
            Some(_) => Act::Nothing(format!("{} уже надет", item.info().name)),
            None if item.base.consumable().is_some() => Act::Use(item.uid),
            None if crate::lore_client::usable(item.base) => Act::Lore(item.uid),
            None => Act::Nothing(format!("{} нельзя взять в руки", item.info().name)),
        }
    }

    /// Слоты, которые листает колесо: руки и оружие.
    fn wheel_slots(&self, inv: &Inventory) -> Vec<usize> {
        let mut v = vec![0];
        v.extend((1..SLOTS).filter(|i| self.slots[*i].and_then(|b| b.resolve(inv)).is_some_and(|it| it.info().slot == Some(EquipSlot::MainHand))));
        v
    }
}

pub struct HotbarPlugin;

impl Plugin for HotbarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hotbar>()
            .add_systems(Startup, setup_hotbar.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (track_inventory, hotbar_input, sync_holster, draw_hotbar).chain());
    }
}

fn track_inventory(state: Res<LootState>, mut bar: ResMut<Hotbar>) {
    if let Some(inv) = &state.inventory {
        bar.update_from_inventory(inv);
    }
}

/// Выполнить выбор слота.
pub fn activate(i: usize, bar: &mut Hotbar, state: &mut LootState, client: &mut RenetClient, body: Option<&crate::medicine::Body>, now: f32) {
    let Some(inv) = state.inventory.as_ref() else { return };
    match bar.plan(i, inv) {
        Act::Holster(on) => bar.holstered = on,
        Act::Move { uid, target, unholster } => {
            send(client, &LootClientMessage::Move { uid, from: Owner::Me, to: Owner::Me, target });
            if unholster {
                bar.holstered = false;
            }
        }
        Act::Use(uid) => send(client, &LootClientMessage::Use { uid }),
        Act::Lore(uid) => {
            if let Some(item) = inv.find(uid).map(|(_, i)| i.clone()) {
                crate::lore_client::try_use(client, &item, body);
            }
        }
        Act::Nothing(text) => state.notify(text, now),
    }
}

#[allow(clippy::too_many_arguments)]
fn hotbar_input(
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    cursor: Single<&CursorOptions, With<PrimaryWindow>>,
    mut bar: ResMut<Hotbar>,
    mut state: ResMut<LootState>,
    client: Option<ResMut<RenetClient>>,
    grab: Option<Res<crate::props_client::PropGrab>>,
    body: Option<Res<crate::medicine_client::MyBody>>,
    time: Res<Time>,
) {
    let Some(mut client) = client else { return };
    // В открытом инвентаре цифры привязывают вещи (inventory_ui), во время применения
    // (перевязка, огонь под сосудом: 1–3) — заняты.
    if state.ui_open || state.using.is_some() || state.inventory.is_none() {
        return;
    }
    let now = time.elapsed_secs();
    let body = body.as_deref().map(|b| &b.0);
    if let Some(i) = KEYS.iter().position(|k| keys.just_pressed(*k)) {
        activate(i, &mut bar, &mut state, &mut client, body, now);
        return;
    }
    // Колесо: только когда мышь захвачена и в руках нет предмета (им колесо двигает предмет).
    let locked = cursor.grab_mode != CursorGrabMode::None;
    let holding = grab.as_ref().is_some_and(|g| g.holding.is_some());
    if !locked || holding || scroll.delta.y.abs() < 0.01 {
        return;
    }
    let inv = state.inventory.as_ref().unwrap();
    let order = bar.wheel_slots(inv);
    if order.len() < 2 {
        return;
    }
    let cur = bar.active(inv);
    let pos = order.iter().position(|s| *s == cur).unwrap_or(0);
    let step = if scroll.delta.y < 0.0 { 1 } else { order.len() - 1 };
    let next = order[(pos + step) % order.len()];
    activate(next, &mut bar, &mut state, &mut client, body, now);
}

/// Сообщить серверу, убрано ли оружие (при изменении и после подключения).
fn sync_holster(mut bar: ResMut<Hotbar>, client: Option<ResMut<RenetClient>>, net: Option<Res<crate::client::NetState>>) {
    let Some(mut client) = client else { return };
    if !client.is_connected() || net.as_ref().is_none_or(|n| n.my_id.is_none()) {
        bar.sent = None;
        return;
    }
    if bar.sent != Some(bar.holstered) {
        client.send_message(CH_RELIABLE, encode(&ClientMessage::Holster { on: bar.holstered }));
        bar.sent = Some(bar.holstered);
    }
}

// ---------------------------------------------------------------------------
// Отрисовка
// ---------------------------------------------------------------------------

#[derive(Component)]
struct HotbarRoot;

fn setup_hotbar(mut commands: Commands) {
    commands.spawn((
        HotbarRoot,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            bottom: px(10),
            justify_content: JustifyContent::Center,
            column_gap: px(4),
            ..default()
        },
        // Выше окна инвентаря, чтобы на слоты можно было положить вещь.
        GlobalZIndex(15),
        FocusPolicy::Pass,
    ));
}

#[derive(PartialEq, Clone, Default)]
struct Drawn {
    slots: Vec<(Option<ItemBaseId>, u32, bool, u16)>,
    active: usize,
    visible: bool,
}

#[allow(clippy::too_many_arguments)]
fn draw_hotbar(
    mut commands: Commands,
    bar: Res<Hotbar>,
    state: Res<LootState>,
    fonts: Option<Res<crate::inventory_ui::UiFonts>>,
    icons: Option<Res<crate::icons::ItemIcons>>,
    root: Single<(Entity, &mut Node), With<HotbarRoot>>,
    mut drawn: Local<Drawn>,
) {
    let (Some(fonts), Some(icons)) = (fonts, icons) else { return };
    let (root, mut node) = root.into_inner();
    let Some(inv) = state.inventory.as_ref() else {
        if node.display != Display::None {
            node.display = Display::None;
        }
        return;
    };
    let active = bar.active(inv);
    let want = Drawn {
        slots: bar
            .slots
            .iter()
            .map(|s| match s {
                Some(b) => {
                    let item = b.resolve(inv);
                    (Some(b.base()), b.count(inv), item.is_some(), item.map_or(0, |i| i.quality as u16))
                }
                None => (None, 0, false, 0),
            })
            .collect(),
        active,
        visible: true,
    };
    // Правка узла — новая раскладка всего интерфейса: пишем, только если что-то сменилось.
    if node.display != Display::Flex {
        node.display = Display::Flex;
    }
    if *drawn == want {
        return;
    }
    *drawn = want.clone();
    commands.entity(root).despawn_related::<Children>();
    commands.entity(root).with_children(|row| {
        for (i, (base, count, present, _)) in want.slots.iter().enumerate() {
            let on = i == active;
            row.spawn((
                crate::inventory_ui::hotbar_hit(i),
                Interaction::default(),
                FocusPolicy::Block,
                Node {
                    width: px(SIZE),
                    height: px(SIZE),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(5)),
                    ..default()
                },
                // Ячейка темы: бронзовый ободок, у выбранной — золотое свечение.
                crate::ui_theme::theme().slot(on),
            ))
            .with_children(|s| {
                let label = |t: String, size: f32, color: Color| {
                    (
                        Text::new(t),
                        TextFont { font: fonts.bold.clone().into(), font_size: bevy::text::FontSize::Px(size), ..default() },
                        TextColor(color),
                        TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.9) },
                        FocusPolicy::Pass,
                    )
                };
                if i == 0 {
                    s.spawn((label("Руки".into(), 14.0, Color::srgb(0.92, 0.86, 0.72)), TextLayout::justify(Justify::Center)));
                } else if let Some(base) = base {
                    let alpha = if *present { 1.0 } else { 0.3 };
                    let (w, h) = base.base().size;
                    s.spawn((
                        ImageNode::new(icons.get(*base)).with_color(Color::WHITE.with_alpha(alpha)),
                        Node { height: percent(100), aspect_ratio: Some(w as f32 / h as f32), max_width: percent(100), ..default() },
                        FocusPolicy::Pass,
                    ));
                    if matches!(bar.slots[i], Some(Bind::Kind(_))) {
                        s.spawn((label(count.to_string(), 12.0, Color::WHITE), Node { position_type: PositionType::Absolute, right: px(3), bottom: px(1), ..default() }));
                    }
                }
                s.spawn((label((i + 1).to_string(), 12.0, Color::srgba(1.0, 0.95, 0.8, 0.8)), Node { position_type: PositionType::Absolute, left: px(3), top: px(1), ..default() }));
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::*;

    fn make(name: &str, uids: &mut UidGen) -> Item {
        roll_item(ItemBaseId::by_name(name), Quality::Common, 1, &mut Rng::new(3), uids)
    }

    #[test]
    fn hands_slot_holsters_and_weapon_slot_toggles() {
        let mut uids = UidGen::default();
        let mut inv = Inventory::new((6, 6));
        let sword = make("Спата", &mut uids);
        let axe = make("Боевой топор", &mut uids);
        let (sid, aid) = (sword.uid, axe.uid);
        inv.place(sword, Target::Equip(EquipSlot::MainHand)).unwrap();
        inv.place(axe, Target::Auto).unwrap();
        let mut bar = Hotbar::default();
        bar.update_from_inventory(&inv);
        assert_eq!(bar.slots[1], Some(Bind::Item { uid: sid, base: ItemBaseId::by_name("Спата") }));
        assert_eq!(bar.active(&inv), 1);
        assert_eq!(bar.plan(0, &inv), Act::Holster(true));
        bar.holstered = true;
        assert_eq!(bar.active(&inv), 0);
        // Снова слот меча — достать.
        assert_eq!(bar.plan(1, &inv), Act::Holster(false));
        // Топор из сумки — в правую руку.
        let axe_item = inv.find(aid).unwrap().1.clone();
        bar.bind(2, &axe_item);
        assert_eq!(bar.plan(2, &inv), Act::Move { uid: aid, target: Target::Equip(EquipSlot::MainHand), unholster: true });
        assert_eq!(bar.wheel_slots(&inv), vec![0, 1, 2]);
    }

    #[test]
    fn consumables_bind_by_kind_and_shield_toggles() {
        let mut uids = UidGen::default();
        let mut inv = Inventory::new((6, 6));
        let bandage = make("Бинт", &mut uids);
        let shield = make("Круглый щит", &mut uids);
        let (bid, shid) = (bandage.uid, shield.uid);
        inv.place(bandage, Target::Auto).unwrap();
        inv.place(shield, Target::Auto).unwrap();
        let mut bar = Hotbar::default();
        bar.update_from_inventory(&inv);
        assert_eq!(bar.plan(3, &inv), Act::Use(bid));
        assert!(matches!(bar.plan(4, &inv), Act::Nothing(_)));
        let s = inv.find(shid).unwrap().1.clone();
        bar.bind(5, &s);
        assert_eq!(bar.plan(5, &inv), Act::Move { uid: shid, target: Target::Equip(EquipSlot::OffHand), unholster: false });
        inv = {
            let mut v = vec![inv];
            crate::inventory::transfer(&mut v, 0, 0, shid, Target::Equip(EquipSlot::OffHand)).unwrap();
            v.pop().unwrap()
        };
        assert_eq!(bar.plan(5, &inv), Act::Move { uid: shid, target: Target::Auto, unholster: false });
        // Перепривязка: тот же щит в другой слот — старый слот освобождается.
        bar.bind(6, &s);
        assert_eq!(bar.slots[5], None);
    }
}
