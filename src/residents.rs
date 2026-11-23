//! Жители: горожане в домах, мастерских, лавках, церквях, банях и казармах.
//!
//! Где кто стоит, задаёт уровень (`level::Layout::residents`, одинаково у сервера и клиента);
//! тело — невидимая коробка в коллизиях уровня. Рисует их только клиент: жители не дерутся
//! и не ходят, а дышат, поглядывают по сторонам и отвечают парой слов на «E».

use crate::level::{Resident, Role};
use crate::rig::{Beard, FootGear, Hair, HeadGear, LegGear, NeedsRig, Outfit, RigPose, Tunic};
use bevy::prelude::*;

pub struct ResidentsPlugin;

impl Plugin for ResidentsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ResidentAim::default())
            .insert_resource(Speech::default())
            .add_systems(Startup, setup_speech.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (aim_resident, talk, update_speech, idle_residents, cull_residents).chain().before(crate::doors::DoorSystems));
    }
}

#[derive(Component)]
pub struct ResidentView {
    pub role: Role,
    phase: f32,
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
    let base = Outfit::for_actor(crate::protocol::ActorKind::Player, seed.wrapping_mul(2_654_435_761) ^ 0x51);
    let h = |k: u64| ((seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(k * 7919) >> 40) % 1000) as f32 / 1000.0;
    // Некрашеная шерсть и лён, крашеное — у богатых и клира.
    let wool = Color::srgb(0.55 + 0.2 * h(1), 0.48 + 0.17 * h(1), 0.38 + 0.12 * h(1));
    let dyed = [Color::srgb(0.45, 0.15, 0.12), Color::srgb(0.2, 0.28, 0.45), Color::srgb(0.32, 0.4, 0.22), Color::srgb(0.6, 0.48, 0.2)][(h(2) * 4.0) as usize % 4];
    let plain = Outfit { head: HeadGear::Bare, tunic: Tunic::Byzantine, legs: LegGear::Wrapped, feet: FootGear::Shoes, tunic_color: wool, ..base };
    match role {
        // Риг пока один на всех: женщин отличают длинные волосы, бритое лицо и крашеная туника.
        Role::Housewife | Role::Weaver | Role::Cook => Outfit { hair: Hair::Long, beard: Beard::None, tunic_color: dyed, ..plain },
        Role::Child | Role::Student => Outfit { beard: Beard::None, ..plain },
        Role::Priest => Outfit { beard: Beard::Full, hair: Hair::Long, tunic_color: Color::srgb(0.85, 0.82, 0.74), ..plain },
        Role::Monk => Outfit { beard: Beard::Full, hair: Hair::Shaved, tunic_color: Color::srgb(0.16, 0.13, 0.12), ..plain },
        Role::Physician | Role::Scholar => Outfit { beard: Beard::Full, tunic_color: Color::srgb(0.78, 0.74, 0.62), ..plain },
        Role::Noble => Outfit { beard: Beard::Short, tunic_color: dyed, feet: FootGear::Boots, legs: LegGear::Trousers, ..plain },
        Role::Soldier => Outfit { head: HeadGear::Spangenhelm, legs: LegGear::Trousers, feet: FootGear::Boots, tunic_color: Color::srgb(0.6, 0.55, 0.45), ..plain },
        Role::Officer => Outfit { legs: LegGear::Trousers, feet: FootGear::Boots, tunic_color: Color::srgb(0.55, 0.12, 0.1), ..plain },
        Role::Sailor => Outfit { head: HeadGear::FeltCap, legs: LegGear::Trousers, ..plain },
        Role::Farmer => Outfit { head: HeadGear::FeltCap, ..plain },
        _ => plain,
    }
}

/// Клиент: ставит жителей (вызывается при постройке уровня).
pub fn spawn_residents(commands: &mut Commands, residents: &[Resident]) {
    for (i, r) in residents.iter().enumerate() {
        let scale = if r.role == Role::Child { 0.72 } else { 1.0 };
        commands
            .spawn((
                ResidentView { role: r.role, phase: i as f32 * 2.31, index: i },
                Transform::from_translation(r.pos).with_rotation(Quat::from_rotation_y(r.yaw)).with_scale(Vec3::splat(scale)),
                Visibility::default(),
            ))
            .with_children(|p| {
                p.spawn((NeedsRig(outfit(r.role, i as u64 + 17)), Transform::default(), Visibility::default()));
            });
    }
}

/// Работающие склоняются над делом, остальные стоят прямо.
fn working(role: Role) -> bool {
    matches!(role, Role::Craftsman | Role::Weaver | Role::Cook | Role::Scholar | Role::Student)
}

fn idle_residents(time: Res<Time>, people: Query<(&ResidentView, &Children, &Visibility)>, mut poses: Query<&mut RigPose>) {
    let t = time.elapsed_secs();
    for (r, children, vis) in &people {
        if *vis == Visibility::Hidden {
            continue;
        }
        for child in children.iter() {
            let Ok(mut pose) = poses.get_mut(child) else { continue };
            let sway = (t * 0.35 + r.phase).sin();
            let lean = if working(r.role) { -0.28 + 0.06 * (t * 0.9 + r.phase).sin() } else { 0.0 };
            let solved = crate::rig::solve_pose(&crate::rig::PoseInput { twist: sway * 0.12, lean, pitch: if working(r.role) { -0.4 } else { 0.0 }, time: t + r.phase, ..default() });
            *pose = solved.pose;
        }
    }
}

/// Дальних жителей не рисуем (их сотни, а видно — десяток).
fn cull_residents(camera: Query<&GlobalTransform, With<crate::player::PlayerCamera>>, mut people: Query<(&Transform, &mut Visibility), (With<ResidentView>, Without<crate::civil_client::Fallen>)>) {
    let Ok(cam) = camera.single() else { return };
    let c = cam.translation();
    for (t, mut vis) in &mut people {
        let want = if t.translation.distance_squared(c) < 45.0 * 45.0 { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
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

fn talk(keys: Res<ButtonInput<KeyCode>>, aim: Res<ResidentAim>, loot: Res<crate::loot_client::LootState>, time: Res<Time>, mut speech: ResMut<Speech>, mut said: Local<u32>) {
    if loot.ui_open || !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    let Some((i, role)) = aim.0 else { return };
    let l = lines(role);
    *said += 1;
    let line = l[(i + *said as usize) % l.len()];
    speech.line = format!("{}: «{line}»", title(role));
    speech.until = time.elapsed_secs() + 5.0;
}

#[derive(Component)]
struct SpeechText;

fn setup_speech(mut commands: Commands, fonts: Res<crate::inventory_ui::UiFonts>) {
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100), top: percent(60), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            p.spawn((
                SpeechText,
                Text::new(""),
                TextFont { font: fonts.regular.clone().into(), font_size: bevy::text::FontSize::Px(19.0), ..default() },
                TextColor(Color::srgb(0.95, 0.9, 0.78)),
                TextLayout::justify(Justify::Center),
                TextShadow::default(),
            ));
        });
}

fn update_speech(time: Res<Time>, aim: Res<ResidentAim>, speech: Res<Speech>, mut text: Single<&mut Text, With<SpeechText>>) {
    let line = if time.elapsed_secs() < speech.until {
        speech.line.clone()
    } else if let Some((_, role)) = aim.0 {
        format!("[E] Поговорить: {}", title(role))
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
