//! Жители: горожане в домах, мастерских, лавках, церквях, банях и казармах.
//!
//! Где кто живёт, задаёт уровень (`level::Layout::residents`, одинаково у сервера и клиента);
//! тело — невидимая коробка в коллизиях уровня. Днём жители живут по распорядку
//! (`daily_life.rs`, рисует и двигает их `life_client.rs`) и отвечают парой слов на «E».

use crate::level::{Resident, Role};
use crate::appearance::{Folk, Look, Shape};
use crate::rig::{Beard, FootGear, Hair, HeadGear, LegGear, NeedsRig, Outfit, Tunic};
use bevy::prelude::*;

pub struct ResidentsPlugin;

impl Plugin for ResidentsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ResidentAim::default())
            .insert_resource(Speech::default())
            .add_systems(Startup, setup_speech.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (aim_resident, talk, update_speech).chain().after(crate::life_client::LifeSystems).before(crate::doors::DoorSystems));
    }
}

#[derive(Component)]
pub struct ResidentView {
    pub role: Role,
    pub index: usize,
}

/// На кого из жителей смотрит игрок (номер в списке уровня).
#[derive(Resource, Default)]
pub struct ResidentAim(pub Option<(usize, Role)>);

#[derive(Resource, Default)]
struct Speech {
    line: String,
    until: f32,
}

pub fn title(role: Role) -> &'static str {
    match role {
        Role::Householder => "Хозяин дома",
        Role::Housewife => "Хозяйка",
        Role::Servant => "Слуга",
        Role::Child => "Мальчишка",
        Role::Weaver => "Ткачиха",
        Role::Cook => "Кухарка",
        Role::Craftsman => "Ремесленник",
        Role::Shopkeeper => "Лавочник",
        Role::Priest => "Священник",
        Role::Monk => "Монах",
        Role::Physician => "Врач",
        Role::Patient => "Больной",
        Role::Scholar => "Грамматик",
        Role::Student => "Ученик",
        Role::Soldier => "Стратиот",
        Role::Officer => "Комит",
        Role::Bather => "Банщик",
        Role::Sailor => "Моряк",
        Role::Noble => "Знатный ссыльный",
        Role::Farmer => "Земледелец",
    }
}

/// Что житель говорит (по очереди, чтобы не повторяться подряд).
fn lines(role: Role) -> &'static [&'static str] {
    match role {
        Role::Householder => &[
            "Входи, раз пришёл, только грязь с улицы не неси.",
            "Цистерна у нас своя, а за водой из водопровода очередь с рассвета.",
            "Опять пошлину на рыбу подняли. Чем кормить семью?",
            "Говорят, гунны снова у Боспора. Слава Богу, стены Зенон починил.",
        ],
        Role::Housewife | Role::Weaver => &[
            "Шерсть нынче дорогая, а лён привозной ещё дороже.",
            "Не трогай станок, нить порвёшь.",
            "Хлеб в печи, к вечеру будет.",
            "Мой опять в порту до ночи, амфоры таскает.",
        ],
        Role::Cook => &["Рыбный соус готовится месяц, а съедают за вечер.", "Отойди от очага, обожжёшься.", "Соли бы ещё мешок…"],
        Role::Servant => &["Хозяина нет, приходи позже.", "Мне не велено пускать чужих в кладовую."],
        Role::Child => &["А правда, что у готов мечи длиной с меня?", "Мама не велит со стражниками говорить."],
        Role::Craftsman => &[
            "Заказы беру, но плата вперёд.",
            "Хороший инструмент дороже серебра.",
            "Работы много, рук мало. Ученика бы толкового.",
        ],
        Role::Shopkeeper => &["Смотри, но руками не трогай.", "Цена твёрдая, торговаться иди к готам."],
        Role::Priest => &[
            "Мир тебе. Литургия на рассвете, не опаздывай.",
            "Епископ велел молиться за императора Анастасия.",
            "Болящих неси в ксенон, там помогут.",
        ],
        Role::Monk => &["Молчание — тоже молитва.", "Свечи и ладан даёт ризничий, не я."],
        Role::Physician => &[
            "Гален учит: прежде чем резать, подумай дважды.",
            "Рану промыть вином, засыпать квасцами, перевязать чистым льном.",
            "Хочешь учиться врачеванию — плати, и приходи с рассветом.",
        ],
        Role::Patient => &["Лихорадка третий день…", "Воды бы…"],
        Role::Scholar => &["Без грамоты и Диоскорида не прочесть.", "Свитки руками не мять!"],
        Role::Student => &["Учитель строгий, розгу не жалеет.", "Сегодня зубрим Гомера."],
        Role::Soldier => &["Проходи, не задерживайся.", "Жалованье опять задерживают.", "У ворот спокойно. Пока."],
        Role::Officer => &["Говори коротко, у меня смотр.", "Без пропуска в претории делать нечего."],
        Role::Bather => &["Кальдарий натоплен, но дрова на исходе.", "Плата — фоллис, полотенце своё."],
        Role::Sailor => &["Из Синопы шли, штормило так, что половину амфор побили.", "Ветер встанет — уходим в Константинополь."],
        Role::Noble => &[
            "В Константинополе меня помнят. Пока.",
            "Сюда ссылают тех, кого нельзя казнить.",
            "Местное вино пить можно, если разбавить морем.",
        ],
        Role::Farmer => &["Виноград в этом году хороший, лишь бы степняки не пожгли.", "За стенами страшно, а кормиться надо."],
    }
}

/// Облик по роли. Детей потом уменьшаем масштабом.
pub fn outfit(role: Role, seed: u64) -> Outfit {
    let h = |k: u64| ((seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(k * 7919) >> 40) % 1000) as f32 / 1000.0;
    // Город многолюдный: кроме ромеев — готы из предместий и заморские моряки и купцы.
    let folk = match role {
        Role::Sailor if h(5) < 0.55 => Folk::Overseas,
        Role::Farmer | Role::Servant if h(5) < 0.4 => Folk::Goths,
        Role::Householder | Role::Craftsman | Role::Shopkeeper if h(5) < 0.12 => Folk::Overseas,
        Role::Householder | Role::Craftsman if h(5) < 0.22 => Folk::Goths,
        _ => Folk::Romaioi,
    };
    let look_seed = seed.wrapping_mul(2_654_435_761) ^ 0x51;
    let base = Outfit::for_actor(crate::protocol::ActorKind::Player, look_seed).with_look(Look::random(look_seed, folk));
    // Некрашеная шерсть и лён, крашеное — у богатых и клира.
    let wool = Color::srgb(0.55 + 0.2 * h(1), 0.48 + 0.17 * h(1), 0.38 + 0.12 * h(1));
    let dyed = [Color::srgb(0.45, 0.15, 0.12), Color::srgb(0.2, 0.28, 0.45), Color::srgb(0.32, 0.4, 0.22), Color::srgb(0.6, 0.48, 0.2)][(h(2) * 4.0) as usize % 4];
    let plain = Outfit { head: HeadGear::Bare, tunic: Tunic::Byzantine, legs: LegGear::Wrapped, feet: FootGear::Shoes, tunic_color: wool, ..base };
    match role {
        // Риг пока один на всех: женщин отличают длинные волосы, бритое лицо и крашеная туника.
        Role::Housewife | Role::Weaver | Role::Cook => Outfit {
            hair: Hair::Long,
            beard: Beard::None,
            tunic_color: dyed,
            // Мягкие черты, тоньше кость.
            shape: Shape { rugged: -0.8, longnose: plain.shape.longnose.min(0.3) - 0.3, square: plain.shape.square.min(0.0) - 0.4, brow: 0.0, heavy: plain.shape.heavy * 0.5, ..plain.shape },
            ..plain
        },
        Role::Child => Outfit { beard: Beard::None, hair: if plain.hair == Hair::Balding { Hair::Short } else { plain.hair }, shape: Shape { heavy: 0.0, aged: 0.0, brow: plain.shape.brow * 0.3, ..plain.shape }, ..plain },
        // Ученики — юноши: в лучшем случае пушок над губой.
        Role::Student => Outfit { beard: if h(6) < 0.3 { Beard::Moustache } else { Beard::None }, hair: Hair::Short, ..plain },
        // Клир по обычаю носит бороду и длинные волосы.
        Role::Priest => Outfit { beard: Beard::Full, hair: Hair::Long, tunic_color: Color::srgb(0.85, 0.82, 0.74), ..plain },
        // Монахи: борода и тонзура (выбритая макушка).
        Role::Monk => Outfit { beard: Beard::Full, hair: Hair::Balding, tunic_color: Color::srgb(0.16, 0.13, 0.12), ..plain },
        // Учёные и врачи — люди в летах, по-философски бородатые.
        Role::Physician | Role::Scholar => Outfit { beard: if h(6) < 0.7 { Beard::Full } else { Beard::Short }, tunic_color: Color::srgb(0.78, 0.74, 0.62), ..plain },
        // Знать бреется по столичной моде (как Анастасий на солидах) или носит холёную бородку.
        Role::Noble => Outfit { beard: if h(6) < 0.6 { Beard::None } else { Beard::Short }, tunic_color: dyed, feet: FootGear::Boots, legs: LegGear::Trousers, ..plain },
        Role::Soldier => Outfit { head: HeadGear::Spangenhelm, legs: LegGear::Trousers, feet: FootGear::Boots, tunic_color: Color::srgb(0.6, 0.55, 0.45), ..plain },
        Role::Officer => Outfit { legs: LegGear::Trousers, feet: FootGear::Boots, tunic_color: Color::srgb(0.55, 0.12, 0.1), ..plain },
        Role::Sailor => Outfit { head: HeadGear::FeltCap, legs: LegGear::Trousers, ..plain },
        Role::Farmer => Outfit { head: HeadGear::FeltCap, ..plain },
        _ => plain,
    }
}

/// Кем служит житель с этим номером (по раскладке уровня).
pub fn role_of(i: usize) -> Option<Role> {
    crate::level::shared().residents.get(i).map(|r| r.role)
}

/// Клиент: ставит жителей (вызывается при постройке уровня).
pub fn spawn_residents(commands: &mut Commands, residents: &[Resident]) {
    for (i, r) in residents.iter().enumerate() {
        let scale = if r.role == Role::Child { 0.72 } else { 1.0 };
        commands
            .spawn((
                ResidentView { role: r.role, index: i },
                Transform::from_translation(r.pos).with_rotation(Quat::from_rotation_y(r.yaw)).with_scale(Vec3::splat(scale)),
                Visibility::default(),
            ))
            .with_children(|p| {
                p.spawn((NeedsRig(outfit(r.role, i as u64 + 17)), Transform::default(), Visibility::default()));
            });
    }
}

fn aim_resident(
    mut aim: ResMut<ResidentAim>,
    loot: Res<crate::loot_client::LootState>,
    trade: Res<crate::trade_client::TradeState>,
    camera: Query<&GlobalTransform, With<crate::player::PlayerCamera>>,
    people: Query<(&ResidentView, &Transform), Without<crate::civil_client::Fallen>>,
) {
    aim.0 = None;
    if loot.ui_open || loot.target.is_some() || trade.window_open() || trade.aim.is_some() {
        return;
    }
    let Ok(cam) = camera.single() else { return };
    let (origin, dir) = (cam.translation(), cam.forward().as_vec3());
    let mut best: Option<(f32, usize, Role)> = None;
    for (r, t) in &people {
        let to = t.translation + Vec3::Y * 1.1 * t.scale.y - origin;
        let along = to.dot(dir);
        if !(0.0..=3.0).contains(&along) || (to - dir * along).length() > 0.45 {
            continue;
        }
        if best.is_none_or(|(d, _, _)| along < d) {
            best = Some((along, r.index, r.role));
        }
    }
    aim.0 = best.map(|(_, i, r)| (i, r));
}

#[allow(clippy::too_many_arguments)]
fn talk(
    keys: Res<ButtonInput<KeyCode>>,
    aim: Res<ResidentAim>,
    loot: Res<crate::loot_client::LootState>,
    time: Res<Time>,
    acts: Res<crate::life_client::LiveActs>,
    clock: Res<crate::world_clock::WorldClock>,
    mut speech: ResMut<Speech>,
    mut said: Local<u32>,
    lore: Option<Res<crate::lore_client::LoreView>>,
    mut school: Option<ResMut<crate::school_client::SchoolUi>>,
) {
    if loot.ui_open || !keys.just_pressed(KeyCode::KeyE) || school.as_ref().is_some_and(|s| s.open()) {
        return;
    }
    let Some((i, role)) = aim.0 else { return };
    // Учитель школы: окно с предметами (там же он скажет, если сейчас не учит).
    if let (Some(id), Some(school)) = (lore.as_ref().and_then(|l| l.master_of_resident(i)), school.as_mut()) {
        school.show(id);
        return;
    }
    *said += 1;
    let act = acts.0.get(i).copied().unwrap_or(crate::daily_life::Act::Idle);
    speech.line = match busy_line(act, clock.night(), *said as usize + i) {
        Some(l) if l.starts_with('(') => format!("{}: {l}", title(role)),
        Some(l) => format!("{}: «{l}»", title(role)),
        None => {
            let l = lines(role);
            format!("{}: «{}»", title(role), l[(i + *said as usize) % l.len()])
        }
    };
    speech.until = time.elapsed_secs() + 5.0;
}

/// Что отвечает занятый человек (спящий, молящийся, испуганный…); `None` — обычная речь.
fn busy_line(act: crate::daily_life::Act, night: bool, k: usize) -> Option<&'static str> {
    use crate::daily_life::Act::*;
    let pick = |l: &'static [&'static str]| Some(l[k % l.len()]);
    match act {
        Sleep => pick(&["(спит и что-то бормочет во сне)", "(храпит)", "Ммм… кто здесь? Ночь же… (переворачивается на другой бок)"]),
        Lie => pick(&["(тяжело дышит) Воды бы…", "Лекарь сказал лежать…"]),
        Pray | Preach => pick(&["(шепчет молитву и не отвечает)", "Не мешай, сын мой, идёт служба."]),
        Cower => pick(&["Не трогай меня! Я ничего не видел!", "Стража! Помогите!"]),
        Hammer => pick(&["Отойди, искры летят!", "Железо остывает, говори быстрей."]),
        Fish => pick(&["Тише, рыбу распугаешь.", "Клюёт сегодня плохо."]),
        Carry => pick(&["Посторонись, амфора тяжёлая!", "Некогда, груз ждут в хорреях."]),
        Drink => pick(&["Выпей с нами! Вино с Боспора, разбавленное как положено.", "Хозяин, ещё кувшин!"]),
        Eat => pick(&["Садись, раздели хлеб.", "Не мешай человеку обедать."]),
        Guard if night => pick(&["Ночью по городу не шатайся.", "Проходи. Ворота до рассвета закрыты."]),
        Talk => pick(&["Мы тут о своём… Тебе чего?", "А ты что думаешь, путник?"]),
        _ => None,
    }
}

/// Кто учит (учитель-житель).
fn l_master(lore: Option<&crate::lore_client::LoreView>, id: u32) -> Option<crate::lore::Teacher> {
    lore?.master(id).map(|m| m.teacher)
}

#[derive(Component)]
struct SpeechText;

fn setup_speech(mut commands: Commands, fonts: Res<crate::inventory_ui::UiFonts>) {
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100), top: percent(60), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            crate::ui_theme::theme().plaque(p).with_children(|p| {
                p.spawn((
                    SpeechText,
                    Text::new(""),
                    TextFont {
                        font: fonts.regular.clone().into(),
                        font_size: bevy::text::FontSize::Px(19.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.95, 0.9, 0.78)),
                    TextLayout::justify(Justify::Center),
                    TextShadow::default(),
                    crate::ui_theme::Plaque,
                ));
            });
        });
}

fn update_speech(
    time: Res<Time>,
    aim: Res<ResidentAim>,
    speech: Res<Speech>,
    acts: Res<crate::life_client::LiveActs>,
    lore: Option<Res<crate::lore_client::LoreView>>,
    school: Option<Res<crate::school_client::SchoolUi>>,
    mut text: Single<&mut Text, With<SpeechText>>,
) {
    let teacher = aim.0.and_then(|(i, _)| lore.as_ref().and_then(|l| l.master_of_resident(i)).and_then(|id| l_master(lore.as_deref(), id)));
    let line = if time.elapsed_secs() < speech.until {
        speech.line.clone()
    } else if school.is_some_and(|s| s.open()) {
        String::new()
    } else if let (Some((i, _)), Some(t)) = (aim.0, teacher) {
        let busy = acts.0.get(i).filter(|a| **a != crate::daily_life::Act::Idle).map(|a| format!(" ({})", a.name())).unwrap_or_default();
        format!("{}{busy}\n[E] Учиться: азбука, чтение, письмо, счёт", crate::school::tutor_title(t))
    } else if let Some((i, role)) = aim.0 {
        // Видно, чем человек занят: «Ремесленник (куёт)».
        match acts.0.get(i) {
            Some(a) if *a != crate::daily_life::Act::Idle => format!("[E] Поговорить: {} ({})", title(role), a.name()),
            _ => format!("[E] Поговорить: {}", title(role)),
        }
    } else {
        String::new()
    };
    if text.0 != line {
        text.0 = line;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_speaks() {
        for r in crate::level::layout().residents {
            assert!(!lines(r.role).is_empty());
        }
    }
}
