//! Ненавязчивые подсказки новичку: маленькая кожаная плашка справа, по одной, каждая — один раз.
//!
//! Подсказка говорит, куда смотреть и к кому идти, но не даёт ответов (какую траву куда
//! прикладывать — игрок узнаёт сам). Выключаются в журнале (F1). Что уже показано, хранится
//! в `mrpg_hints.txt` рядом с игрой (`MRPG_HINTS_FILE` — другой путь).

use crate::fight::LocalFighter;
use crate::loot_client::LootState;
use crate::lore::{Book, Language, Lore, Substance};
use crate::lore_client::{LoreTarget, LoreView, MyKnowledge};
use crate::skill_tree;
use crate::ui_theme::{Skin, UiTheme, ink};
use bevy::prelude::*;
use std::collections::{HashSet, VecDeque};

pub struct HintsPlugin;

impl Plugin for HintsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Hints::load())
            .add_systems(Startup, setup_hint_ui.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (watch_situations, show_hints).chain());
    }
}

/// Сколько секунд висит подсказка и пауза между ними.
const SHOW_SECS: f32 = 11.0;
const GAP_SECS: f32 = 2.0;

#[derive(Resource)]
pub struct Hints {
    enabled: bool,
    seen: HashSet<String>,
    queue: VecDeque<(String, String)>,
    current: Option<(String, f32)>,
    /// Когда можно показать следующую.
    next_at: f32,
    persist: bool,
}

impl Hints {
    fn path() -> std::path::PathBuf {
        std::env::var("MRPG_HINTS_FILE").map(Into::into).unwrap_or_else(|_| "mrpg_hints.txt".into())
    }

    fn load() -> Self {
        let persist = std::env::var("MRPG_SHOT").is_err();
        let mut h = Hints { enabled: true, seen: HashSet::new(), queue: VecDeque::new(), current: None, next_at: 0.0, persist };
        if persist && let Ok(text) = std::fs::read_to_string(Self::path()) {
            for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                match line {
                    "off" => h.enabled = false,
                    "on" => h.enabled = true,
                    id => {
                        h.seen.insert(id.to_string());
                    }
                }
            }
        }
        h
    }

    fn save(&self) {
        if !self.persist {
            return;
        }
        let mut text = String::from(if self.enabled { "on\n" } else { "off\n" });
        let mut ids: Vec<&String> = self.seen.iter().collect();
        ids.sort();
        for id in ids {
            text.push_str(id);
            text.push('\n');
        }
        let _ = std::fs::write(Self::path(), text);
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.queue.clear();
            self.current = None;
        }
        self.save();
    }

    /// Показать подсказку `id` (если ещё не показывали).
    pub fn offer(&mut self, id: &str, text: impl Into<String>) {
        if !self.enabled || self.seen.contains(id) || self.queue.iter().any(|(q, _)| q == id) {
            return;
        }
        self.queue.push_back((id.to_string(), text.into()));
    }
}

#[derive(Component)]
struct HintBox;
#[derive(Component)]
struct HintText;

fn setup_hint_ui(mut commands: Commands, theme: Res<UiTheme>) {
    commands
        .spawn((
            HintBox,
            Node {
                position_type: PositionType::Absolute,
                right: px(18),
                top: percent(20),
                max_width: px(390),
                padding: Skin::Leather.padding(),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                display: Display::None,
                ..default()
            },
            theme.skin(Skin::Leather),
            GlobalZIndex(15),
        ))
        .with_children(|p| {
            p.spawn(theme.bold("Совет", 14.0, ink::GOLD));
            p.spawn((HintText, theme.text("", 15.0, ink::TEXT)));
            p.spawn(theme.text("F1 — журнал · подсказки выключаются там же", 11.0, ink::DIM));
        });
}

/// Следим за положением дел и предлагаем подсказки.
#[allow(clippy::too_many_arguments)]
fn watch_situations(
    time: Res<Time>,
    mut hints: ResMut<Hints>,
    knowledge: Res<MyKnowledge>,
    loot: Res<LootState>,
    lore: Res<LoreView>,
    trade: Option<Res<crate::trade_client::TradeState>>,
    body: Option<Res<crate::medicine_client::MyBody>>,
    fighter: Query<&LocalFighter>,
    players: Query<(), With<crate::player::LocalPlayer>>,
    mut alive_for: Local<f32>,
    mut forced: Local<bool>,
) {
    if players.is_empty() {
        *alive_for = 0.0;
        return;
    }
    *alive_for += time.delta_secs();
    if !*forced && std::env::var("MRPG_SHOT_UI").as_deref() == Ok("hint") {
        hints.seen.clear();
        *forced = true;
    }
    if !hints.enabled || *alive_for < 3.0 {
        return;
    }
    let k = &knowledge.0;
    hints.offer(
        "welcome",
        "Херсон, 510 год. Вы приезжий: ни грамоты, ни ремесла. F1 — журнал «С чего начать», K — книга навыков: что вы умеете и куда расти.",
    );
    if let Some(inv) = loot.inventory.as_ref() {
        let items: Vec<_> = inv.all_items().collect();
        if items.iter().any(|i| Substance::from_item(i.base).is_some_and(|s| !k.identifies(s))) {
            hints.offer(
                "unknown_herb",
                "Вы не знаете, что за трава у вас в сумке. Узнавать травы учат Харитон-ризотом у главных ворот и знахарка в готском селении, а ещё — травники с рисунками.",
            );
        }
        if items.iter().any(|i| Book::from_item(i.base).is_some()) && !k.knows(Lore::Read(Language::Greek)) {
            hints.offer(
                "illiterate_book",
                "Без грамоты книга — закорючки. Грамоте учат церковный писец в западной церкви и Иоанн-книжник на агоре.",
            );
        }
        let strength = inv.equipped_stats().get(crate::items::Stat::Strength);
        let cap = crate::weight::carry_capacity(strength) + skill_tree::Mods::of(k).carry;
        if crate::weight::inventory_weight(inv) > cap {
            hints.offer("heavy", "Ноша тяжела — идёте медленнее. Сила растёт от тяжёлого труда, а «Спина грузчика» прибавит ноши.");
        }
    }
    if matches!(lore.target, Some(LoreTarget::Master(_))) {
        hints.offer(
            "teacher",
            "У мастера можно учиться: урок за плату (E) — быстрее всего, подмастерьем (R) — медленнее, зато платят и рука набивается.",
        );
    }
    if let Some(t) = trade.as_ref()
        && matches!(t.aim, Some(crate::trade_client::Aim::Merchant(_)))
    {
        hints.offer(
            "merchant",
            "Торговец: E — поговорить. Цены зависят от того, как к вам относится его народ, и от умения торговаться. Некоторые и учат.",
        );
    }
    if body.as_ref().is_some_and(|b| !b.0.wounds.is_empty()) {
        hints.offer(
            "wounded",
            "Вы ранены. Откройте сумку (Tab): в окне «Тело» видно, что и где. Чем лечить — узнаёте у лекарей и из книг, или купите готовое у аптекаря.",
        );
    }
    if let Ok(f) = fighter.single()
        && f.fighter.max_stamina > 0.0
        && f.fighter.stamina < f.fighter.max_stamina * 0.2
    {
        hints.offer("winded", "Силы на исходе: без выносливости защита ломается. Отступите и отдышитесь. Бег и бой понемногу развивают выносливость.");
    }
    if k.skills.iter().any(|s| *s >= 1.0) {
        hints.offer("first_skill", "Навык растёт от дела — и тянет за собой связанные: кузнец крепнет, боец становится выносливее. Книга навыков — K.");
    }
    if !skill_tree::earned(k).is_empty() {
        hints.offer("first_perk", "Первое умение! Умения приходят сами, когда набран навык и выучено нужное. В книге навыков (K) видно, чего не хватает до следующих.");
    }
}

fn show_hints(
    time: Res<Time>,
    mut hints: ResMut<Hints>,
    loot: Res<LootState>,
    book: Res<crate::skill_book::BookUi>,
    mut ui: Single<&mut Node, With<HintBox>>,
    mut text: Single<&mut Text, With<HintText>>,
) {
    let now = time.elapsed_secs();
    // Пока открыто большое окно, подсказки ждут.
    let busy = loot.ui_open || book.open;
    if let Some((_, until)) = hints.current.as_ref()
        && (now > *until || busy || !hints.enabled)
    {
        hints.current = None;
        hints.next_at = now + GAP_SECS;
    }
    if hints.current.is_none()
        && !busy
        && now >= hints.next_at
        && let Some((id, line)) = hints.queue.pop_front()
    {
        hints.seen.insert(id);
        hints.save();
        hints.current = Some((line, now + SHOW_SECS));
    }
    let want = if hints.current.is_some() { Display::Flex } else { Display::None };
    if ui.display != want {
        ui.display = want;
    }
    if let Some((line, _)) = hints.current.as_ref()
        && text.0 != *line
    {
        text.0 = line.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_hint_once_and_off_switch() {
        let mut h = Hints { enabled: true, seen: HashSet::new(), queue: VecDeque::new(), current: None, next_at: 0.0, persist: false };
        h.offer("a", "x");
        h.offer("a", "x");
        assert_eq!(h.queue.len(), 1);
        h.seen.insert("b".into());
        h.offer("b", "y");
        assert_eq!(h.queue.len(), 1);
        h.set_enabled(false);
        h.offer("c", "z");
        assert!(h.queue.is_empty());
    }
}
