//! Клиент: смерть мирных людей (тело на месте жителя или торговца) и разговор со стражей.
#![allow(deprecated)] // Interaction: в Bevy 0.20 помечен устаревшим, но работает и проще.

use crate::civilians::{Civ, LawAnswer, LawMessage};
use crate::client::CorpseIndex;
use crate::economy::format_money;
use crate::protocol::{CH_RELIABLE, ClientMessage, encode};
use crate::residents::ResidentView;
use crate::rig::RigPose;
use crate::trade_client::{MerchantNpc, TradeState};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_renet::RenetClient;

pub struct CivilClientPlugin;

impl Plugin for CivilClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CivilNet>()
            .init_resource::<LawUi>()
            .add_systems(Startup, setup_law_ui.after(crate::inventory_ui::setup_fonts))
            .add_systems(Update, (apply_civil, law_input, draw_law_ui).chain())
            .add_systems(Update, law_demo.before(apply_civil).run_if(|| std::env::var_os("MRPG_LAW_DEMO").is_some()));
    }
}

/// Сообщения сервера о мирных людях (пересылает `client::receive_reliable`).
#[derive(Message, Clone, Debug)]
pub enum CivilNet {
    Hit,
    Dead { civ: Civ, corpse: Option<(u64, Vec3, f32)>, fresh: bool },
    Back(Civ),
    Law(LawMessage),
}

/// Погибший житель или торговец (его фигуру не рисуем: вместо неё лежит тело).
#[derive(Component)]
pub struct Fallen;

/// Разговор со стражником, карцер и короткие сообщения.
#[derive(Resource, Default)]
pub struct LawUi {
    /// Стражник остановил: пеня, за что, сколько денег при себе и шанс уговорить (%).
    pub halt: Option<(u32, Vec<String>, u32, Option<u8>)>,
    /// Вас видели: сколько свидетелей и когда первый добежит до стражи.
    witnessed: Option<(u8, f32)>,
    pub jailed_until: Option<f32>,
    notice: String,
    notice_until: f32,
}

impl LawUi {
    /// Окно разговора открыто (курсор принадлежит ему, а не бою).
    pub fn open(&self) -> bool {
        self.halt.is_some()
    }
}

#[derive(Component)]
struct LawRoot;
/// Окна закона: их не прячут на снимке окна стражи (`MRPG_LAW_DEMO`).
#[derive(Component)]
pub struct ShotKeep;
#[derive(Component)]
struct LawText;
#[derive(Component)]
struct NoticeText;
#[derive(Component)]
struct LawButton(LawAnswer);
/// Кнопка «Уговорить»: видна, если есть надежда, и показывает шанс.
#[derive(Component)]
struct PersuadeButton;
#[derive(Component)]
struct WitnessText;

const SCREAMS: [&str; 4] = ["«Помогите! Убивают!»", "«Стража! Стража!»", "«Не надо! Пощади!»", "«Люди добрые, разбой!»"];

#[allow(clippy::too_many_arguments)]
fn apply_civil(
    mut commands: Commands,
    mut events: MessageReader<CivilNet>,
    mut ui: ResMut<LawUi>,
    mut corpses: ResMut<CorpseIndex>,
    time: Res<Time>,
    trade: Option<Res<TradeState>>,
    residents: Query<(Entity, &ResidentView, &Transform)>,
    merchants: Query<(Entity, &MerchantNpc, &Children)>,
    rigs: Query<(), With<RigPose>>,
    mut vis: Query<&mut Visibility>,
    mut screams: Local<usize>,
) {
    let now = time.elapsed_secs();
    for ev in events.read() {
        match ev.clone() {
            CivilNet::Hit => {
                ui.notice = SCREAMS[*screams % SCREAMS.len()].into();
                ui.notice_until = now + 2.5;
                *screams += 1;
            }
            CivilNet::Dead { civ, corpse, fresh } => match civ {
                Civ::Resident(i) => {
                    let Some((e, r, t)) = residents.iter().find(|(_, r, _)| r.index == i as usize) else { continue };
                    commands.entity(e).insert(Fallen);
                    if let Ok(mut v) = vis.get_mut(e) {
                        *v = Visibility::Hidden;
                    }
                    if let Some((id, pos, yaw)) = corpse
                        && !corpses.0.contains_key(&id)
                    {
                        let outfit = crate::residents::outfit(r.role, i as u64 + 17);
                        crate::client::spawn_civ_corpse(&mut commands, &mut corpses, id, outfit, t.scale.y, pos, yaw, fresh);
                    }
                }
                Civ::Merchant(id) => {
                    let Some((e, _, children)) = merchants.iter().find(|(_, m, _)| m.id.0 == id) else { continue };
                    commands.entity(e).insert(Fallen);
                    // Прилавок остаётся, прячем самого торговца.
                    for c in children.iter().filter(|c| rigs.contains(*c)) {
                        if let Ok(mut v) = vis.get_mut(c) {
                            *v = Visibility::Hidden;
                        }
                    }
                    let info = trade.as_ref().and_then(|t| t.merchants.iter().find(|m| m.id.0 == id));
                    if let (Some((cid, pos, yaw)), Some(info)) = (corpse, info)
                        && !corpses.0.contains_key(&cid)
                    {
                        crate::client::spawn_civ_corpse(&mut commands, &mut corpses, cid, crate::trade_client::outfit_for(info), 1.0, pos, yaw, fresh);
                    }
                }
            },
            CivilNet::Back(civ) => match civ {
                Civ::Resident(i) => {
                    if let Some((e, _, _)) = residents.iter().find(|(_, r, _)| r.index == i as usize) {
                        commands.entity(e).remove::<Fallen>();
                        if let Ok(mut v) = vis.get_mut(e) {
                            *v = Visibility::Inherited;
                        }
                    }
                }
                Civ::Merchant(id) => {
                    if let Some((e, _, children)) = merchants.iter().find(|(_, m, _)| m.id.0 == id) {
                        commands.entity(e).remove::<Fallen>();
                        for c in children.iter().filter(|c| rigs.contains(*c)) {
                            if let Ok(mut v) = vis.get_mut(c) {
                                *v = Visibility::Inherited;
                            }
                        }
                    }
                }
            },
            CivilNet::Law(msg) => match msg {
                LawMessage::Notice(text) => {
                    ui.notice = text;
                    ui.notice_until = now + 6.0;
                }
                LawMessage::Halt { fine, crimes, purse, persuade } => ui.halt = Some((fine, crimes, purse, persuade)),
                LawMessage::Witnessed { count, secs } => ui.witnessed = (count > 0).then_some((count, now + secs)),
                LawMessage::HaltEnd => ui.halt = None,
                LawMessage::Jailed { secs } => {
                    ui.halt = None;
                    ui.jailed_until = Some(now + secs);
                }
                LawMessage::Released => {
                    ui.jailed_until = None;
                    ui.notice = "Вас выпустили из карцера. Пеня списана.".into();
                    ui.notice_until = now + 6.0;
                }
            },
        }
    }
}

/// Снимок окна стражи (`MRPG_LAW_DEMO=1` вместе с `MRPG_CAM`): стражник остановил,
/// свидетели бегут доносить.
fn law_demo(mut out: MessageWriter<CivilNet>, mut sent: Local<bool>) {
    // Отсчёт на снимке не убегает (кадры в облаке идут медленно).
    out.write(CivilNet::Law(LawMessage::Witnessed { count: 2, secs: 7.0 }));
    if *sent {
        return;
    }
    *sent = true;
    out.write(CivilNet::Law(LawMessage::Halt { fine: 680, crimes: vec!["кража".into(), "побои".into()], purse: 412, persuade: Some(38) }));
    out.write(CivilNet::Law(LawMessage::Notice("«Держи вора!» — вас заметили: «Серебряная фибула».".into())));
}

fn setup_law_ui(mut commands: Commands, fonts: Res<crate::inventory_ui::UiFonts>) {
    let font = |size: f32, bold: bool| TextFont {
        font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(),
        font_size: bevy::text::FontSize::Px(size),
        ..default()
    };
    // Сообщения закона — над хотбаром.
    commands
        .spawn((ShotKeep, Node { position_type: PositionType::Absolute, width: percent(100), bottom: percent(17), justify_content: JustifyContent::Center, ..default() }))
        .with_children(|p| {
            p.spawn((NoticeText, Text::new(""), font(18.0, false), TextColor(Color::srgb(0.98, 0.86, 0.6)), TextLayout::justify(Justify::Center), TextShadow::default()));
        });
    // Свидетели бегут к страже — сверху по центру, на коже.
    commands
        .spawn((ShotKeep, Node { position_type: PositionType::Absolute, width: percent(100), top: px(64), justify_content: JustifyContent::Center, ..default() }))
        .with_children(|p| {
            p.spawn((
                WitnessText,
                Node { padding: crate::ui_theme::Skin::Leather.padding(), ..default() },
                crate::ui_theme::theme().skin(crate::ui_theme::Skin::Leather),
                Visibility::Hidden,
            ))
            .with_children(|b| {
                b.spawn((Text::new(""), font(19.0, true), TextColor(Color::srgb(1.0, 0.72, 0.45)), TextShadow::default()));
            });
        });
    // Разговор со стражником.
    commands
        .spawn((
            LawRoot,
            ShotKeep,
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    width: px(560),
                    padding: crate::ui_theme::Skin::Wood.padding(),
                    row_gap: px(10),
                    ..default()
                },
                crate::ui_theme::theme().skin(crate::ui_theme::Skin::Wood),
            ))
            .with_children(|w| {
                w.spawn(crate::ui_theme::theme().title("Стражник гарнизона", 28.0));
                crate::ui_theme::theme().divider(w);
                w.spawn((LawText, Text::new(""), font(17.0, false), TextColor(Color::srgb(0.95, 0.9, 0.8))));
                for (answer, label) in [
                    (LawAnswer::Pay, "1. Уплатить пеню"),
                    (LawAnswer::Jail, "2. Сдаться: карцер претории, пеня спишется"),
                    (LawAnswer::Resist, "3. Сопротивляться"),
                ] {
                    crate::ui_theme::theme().button_sized(w, label, 17.0, LawButton(answer), true);
                }
                crate::ui_theme::theme().button_sized(w, "4. Уговорить", 17.0, (LawButton(LawAnswer::Persuade), PersuadeButton), true);
            });
        });
}

fn send_answer(client: &mut Option<ResMut<RenetClient>>, answer: LawAnswer) {
    if let Some(c) = client.as_deref_mut() {
        c.send_message(CH_RELIABLE, encode(&ClientMessage::Law(answer)));
    }
}

fn law_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut client: Option<ResMut<RenetClient>>,
    ui: Res<LawUi>,
    buttons: Query<(&Interaction, &LawButton), Changed<Interaction>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if !ui.open() {
        return;
    }
    // Пока стражник говорит, курсор свободен: отвечаем мышью или цифрами.
    if cursor.grab_mode != CursorGrabMode::None || !cursor.visible {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
    let can_persuade = ui.halt.as_ref().is_some_and(|h| h.3.is_some());
    for (key, answer) in [(KeyCode::Digit1, LawAnswer::Pay), (KeyCode::Digit2, LawAnswer::Jail), (KeyCode::Digit3, LawAnswer::Resist), (KeyCode::Digit4, LawAnswer::Persuade)] {
        if keys.just_pressed(key) && (answer != LawAnswer::Persuade || can_persuade) {
            send_answer(&mut client, answer);
        }
    }
    // Подсветку кнопок делает тема (ThemeButton).
    for (interaction, button) in &buttons {
        if *interaction == Interaction::Pressed {
            send_answer(&mut client, button.0);
        }
    }
}

#[allow(clippy::type_complexity)]
fn draw_law_ui(
    time: Res<Time>,
    ui: Res<LawUi>,
    mut root: Single<&mut Visibility, (With<LawRoot>, Without<WitnessText>)>,
    mut text: Single<&mut Text, (With<LawText>, Without<NoticeText>)>,
    mut notice: Single<&mut Text, (With<NoticeText>, Without<LawText>)>,
    mut persuade: Single<(&mut Node, &Children), With<PersuadeButton>>,
    mut witness: Single<(&mut Visibility, &Children), (With<WitnessText>, Without<LawRoot>)>,
    mut texts: Query<&mut Text, (Without<LawText>, Without<NoticeText>)>,
) {
    let now = time.elapsed_secs();
    let want = if ui.open() { Visibility::Inherited } else { Visibility::Hidden };
    if **root != want {
        **root = want;
    }
    if let Some((fine, crimes, purse, chance)) = &ui.halt {
        let why = if crimes.is_empty() { "кража или сбыт краденого".to_string() } else { crimes.join(", ") };
        let line = format!(
            "«Стой! Ты нарушил закон: {why}.\nПеня — {}. Плати сейчас, иди в карцер или пеняй на себя».\n\nПри вас: {}.",
            format_money(*fine),
            format_money(*purse)
        );
        if text.0 != line {
            text.0 = line;
        }
        let (node, children) = &mut *persuade;
        let display = if chance.is_some() { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if let Some(c) = chance {
            let label = format!("4. Уговорить (красноречие, шанс {c}%)");
            for child in children.iter() {
                if let Ok(mut t) = texts.get_mut(child)
                    && t.0 != label
                {
                    t.0 = label.clone();
                }
            }
        }
    }
    // Свидетели: сколько их и сколько секунд, пока первый не добежит до стражи.
    let (vis, children) = &mut *witness;
    let seen = ui.witnessed.filter(|_| ui.jailed_until.is_none_or(|u| u < now));
    let want = if seen.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    if **vis != want {
        **vis = want;
    }
    if let Some((count, until)) = seen {
        let left = (until - now).ceil().max(0.0) as u32;
        let who = match count {
            1 => "1 свидетель".to_string(),
            2..=4 => format!("{count} свидетеля"),
            _ => format!("{count} свидетелей"),
        };
        let label = format!("Вас видели: {who} — до стражи {left} с");
        for child in children.iter() {
            if let Ok(mut t) = texts.get_mut(child)
                && t.0 != label
            {
                t.0 = label.clone();
            }
        }
    }
    let line = if let Some(until) = ui.jailed_until.filter(|u| *u > now) {
        format!("Вы в карцере претории. Осталось {} с.", (until - now).ceil() as u32)
    } else if now < ui.notice_until {
        ui.notice.clone()
    } else {
        String::new()
    };
    if notice.0 != line {
        notice.0 = line;
    }
}
