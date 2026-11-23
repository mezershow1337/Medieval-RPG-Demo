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
            .add_systems(Update, (apply_civil, law_input, draw_law_ui).chain());
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
    /// Стражник остановил: пеня, за что и сколько денег при себе.
    pub halt: Option<(u32, Vec<String>, u32)>,
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
#[derive(Component)]
struct LawText;
#[derive(Component)]
struct NoticeText;
#[derive(Component)]
struct LawButton(LawAnswer);

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
                LawMessage::Halt { fine, crimes, purse } => ui.halt = Some((fine, crimes, purse)),
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

fn setup_law_ui(mut commands: Commands, fonts: Res<crate::inventory_ui::UiFonts>) {
    let font = |size: f32, bold: bool| TextFont {
        font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(),
        font_size: bevy::text::FontSize::Px(size),
        ..default()
    };
    // Сообщения закона — над хотбаром.
    commands
        .spawn(Node { position_type: PositionType::Absolute, width: percent(100), bottom: percent(17), justify_content: JustifyContent::Center, ..default() })
        .with_children(|p| {
            p.spawn((NoticeText, Text::new(""), font(18.0, false), TextColor(Color::srgb(0.98, 0.86, 0.6)), TextLayout::justify(Justify::Center), TextShadow::default()));
        });
    // Разговор со стражником.
    commands
        .spawn((
            LawRoot,
            Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    width: px(520),
                    padding: UiRect::all(px(18)),
                    row_gap: px(10),
                    border: UiRect::all(px(3)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.16, 0.11, 0.07, 0.94)),
                BorderColor::all(Color::srgb(0.55, 0.42, 0.2)),
            ))
            .with_children(|w| {
                w.spawn((Text::new("Стражник гарнизона"), font(24.0, true), TextColor(Color::srgb(0.93, 0.8, 0.5))));
                w.spawn((LawText, Text::new(""), font(17.0, false), TextColor(Color::srgb(0.95, 0.9, 0.8))));
                for (answer, label) in [
                    (LawAnswer::Pay, "1. Уплатить пеню"),
                    (LawAnswer::Jail, "2. Сдаться: карцер претории, пеня спишется"),
                    (LawAnswer::Resist, "3. Сопротивляться"),
                ] {
                    w.spawn((
                        LawButton(answer),
                        Interaction::default(),
                        Node { padding: UiRect::axes(px(12), px(7)), border: UiRect::all(px(1)), ..default() },
                        BackgroundColor(Color::srgb(0.3, 0.21, 0.12)),
                        BorderColor::all(Color::srgb(0.6, 0.47, 0.25)),
                    ))
                    .with_children(|b| {
                        b.spawn((Text::new(label), font(17.0, false), TextColor(Color::srgb(0.97, 0.92, 0.82))));
                    });
                }
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
    mut buttons: Query<(&Interaction, &LawButton, &mut BackgroundColor), Changed<Interaction>>,
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
    for (key, answer) in [(KeyCode::Digit1, LawAnswer::Pay), (KeyCode::Digit2, LawAnswer::Jail), (KeyCode::Digit3, LawAnswer::Resist)] {
        if keys.just_pressed(key) {
            send_answer(&mut client, answer);
        }
    }
    for (interaction, button, mut bg) in &mut buttons {
        match interaction {
            Interaction::Pressed => send_answer(&mut client, button.0),
            Interaction::Hovered => bg.0 = Color::srgb(0.42, 0.3, 0.16),
            Interaction::None => bg.0 = Color::srgb(0.3, 0.21, 0.12),
        }
    }
}

fn draw_law_ui(
    time: Res<Time>,
    ui: Res<LawUi>,
    mut root: Single<&mut Visibility, With<LawRoot>>,
    mut text: Single<&mut Text, (With<LawText>, Without<NoticeText>)>,
    mut notice: Single<&mut Text, (With<NoticeText>, Without<LawText>)>,
) {
    let now = time.elapsed_secs();
    let want = if ui.open() { Visibility::Inherited } else { Visibility::Hidden };
    if **root != want {
        **root = want;
    }
    if let Some((fine, crimes, purse)) = &ui.halt {
        let why = if crimes.is_empty() { "кража или сбыт краденого".to_string() } else { crimes.join(", ") };
        let line = format!(
            "«Стой! Ты нарушил закон: {why}.\nПеня — {}. Плати сейчас, иди в карцер или пеняй на себя».\n\nПри вас: {}.",
            format_money(*fine),
            format_money(*purse)
        );
        if text.0 != line {
            text.0 = line;
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
