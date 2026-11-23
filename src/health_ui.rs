//! Окно здоровья: силуэт тела по частям, раны на каждой части и лечение прямо отсюда.
//!
//! Это часть окна инвентаря (Tab, I или H). Предмет берут из сумки и кладут на рану в списке,
//! на часть тела на силуэте или в рот. Окно ничего не советует: подсвечивается только то место,
//! над которым мышь, а не то, куда «правильно». Что видно о ране — зависит от знаний
//! (`medicine::wound_views`): неуч видит признаки, врач — диагноз.

#![allow(deprecated)] // Interaction — как в inventory_ui.rs, чтобы окна работали одинаково.

use crate::inventory_ui::{Hit, UiFonts};
use crate::lore::Knowledge;
use crate::medicine::{Alarm, Body, BodyPart, CareTarget, WoundView, general_symptoms, wound_views};
use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use std::hash::{Hash, Hasher};

const TITLE: Color = Color::srgb(0.93, 0.83, 0.6);
const DIM: Color = Color::srgb(0.6, 0.56, 0.5);
const TEXT: Color = Color::srgb(0.86, 0.8, 0.68);
/// Силуэт нарисован, как в анатомическом списке: сепия по пергаменту.
const PARCHMENT: Color = Color::srgb(0.76, 0.67, 0.5);
const SKIN: Color = Color::srgb(0.6, 0.5, 0.36);
const OUTLINE: Color = Color::srgb(0.27, 0.19, 0.11);
const HOVER: Color = Color::srgb(0.98, 0.86, 0.5);

/// Масштаб силуэта (фигура задана в пикселях при 1,0) и её размер.
const FIG_SCALE: f32 = 0.72;
const FIG_W: f32 = 176.0 * FIG_SCALE;
const FIG_H: f32 = 392.0 * FIG_SCALE;
/// Поля пергамента вокруг фигуры.
const PLATE_PAD: f32 = 10.0;

pub struct HealthUiPlugin;

impl Plugin for HealthUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, highlight_zones);
    }
}

/// Место, куда можно приложить средство (для подсветки под мышью).
#[derive(Component)]
pub struct CareZone {
    border: Color,
}

pub fn alarm_color(a: Alarm) -> Color {
    match a {
        Alarm::Mild => Color::srgb(0.92, 0.82, 0.55),
        Alarm::Serious => Color::srgb(1.0, 0.58, 0.28),
        Alarm::Deadly => Color::srgb(1.0, 0.3, 0.25),
    }
}

/// Заливка части тела по самой тревожной ране на ней.
fn part_fill(worst: Option<Alarm>) -> Color {
    match worst {
        None => SKIN,
        Some(Alarm::Mild) => Color::srgb(0.72, 0.5, 0.24),
        Some(Alarm::Serious) => Color::srgb(0.72, 0.3, 0.13),
        Some(Alarm::Deadly) => Color::srgb(0.58, 0.08, 0.06),
    }
}

/// Отпечаток того, что покажет окно: перестраиваем его, только когда видимое изменилось
/// (тело приходит с сервера дважды в секунду).
pub fn signature(body: &Body, k: &Knowledge) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in wound_views(body, k) {
        v.id.hash(&mut h);
        v.part.hash(&mut h);
        v.title.hash(&mut h);
        v.details.hash(&mut h);
        (v.alarm as u8).hash(&mut h);
    }
    for (line, a) in general_symptoms(body, k) {
        line.hash(&mut h);
        (a as u8).hash(&mut h);
    }
    for line in effects(body) {
        line.hash(&mut h);
    }
    h.finish()
}

/// Что недуги делают с бойцом, словами.
fn effects(body: &Body) -> Vec<String> {
    let m = body.mods();
    let mut out = Vec::new();
    let pct = |x: f32| ((1.0 - x) * 100.0).round();
    if m.move_mult < 0.97 {
        out.push(format!("Шаг медленнее на {:.0}%", pct(m.move_mult)));
    }
    if m.action_speed < 0.97 {
        out.push(format!("Удары и движения рук медленнее на {:.0}%", pct(m.action_speed)));
    }
    if m.stamina_regen < 0.97 {
        out.push(format!("Дух переводите медленнее на {:.0}%", pct(m.stamina_regen)));
    }
    if m.max_stamina < 0.97 {
        out.push(format!("Сил меньше на {:.0}%", pct(m.max_stamina)));
    }
    if body.bleeding() > 0.05 {
        out.push(format!("Кровь уходит: {:.1} здоровья в секунду", body.bleeding()));
    }
    out
}

fn text(fonts: &UiFonts, s: impl Into<String>, size: f32, color: Color, bold: bool) -> impl Bundle {
    (
        Text::new(s),
        TextFont { font: if bold { fonts.bold.clone() } else { fonts.regular.clone() }.into(), font_size: bevy::text::FontSize::Px(size), ..default() },
        TextColor(color),
    )
}

/// Части тела на силуэте: (часть, левый край, верх, ширина, высота, скругление).
/// Тело видно со спины, как своё: левая рука — слева.
const SHAPES: [(BodyPart, f32, f32, f32, f32, f32); 7] = [
    (BodyPart::Head, 60.0, 0.0, 56.0, 66.0, 28.0),
    (BodyPart::Chest, 46.0, 78.0, 84.0, 92.0, 16.0),
    (BodyPart::Belly, 52.0, 174.0, 72.0, 58.0, 12.0),
    (BodyPart::LeftArm, 8.0, 82.0, 34.0, 156.0, 14.0),
    (BodyPart::RightArm, 134.0, 82.0, 34.0, 156.0, 14.0),
    (BodyPart::LeftLeg, 52.0, 236.0, 34.0, 156.0, 12.0),
    (BodyPart::RightLeg, 90.0, 236.0, 34.0, 156.0, 12.0),
];

/// Панель «Тело» в окне инвентаря.
pub fn body_panel(root: &mut ChildSpawnerCommands, fonts: &UiFonts, body: &Body, k: &Knowledge) {
    let views = wound_views(body, k);
    let worst = |part: BodyPart| views.iter().filter(|v| v.part == part).map(|v| v.alarm).max();
    root.spawn((
        Hit::Panel,
        Interaction::default(),
        FocusPolicy::Pass,
        Node {
            flex_direction: FlexDirection::Column,
            padding: crate::ui_theme::Skin::Wood.padding(),
            row_gap: px(8),
            align_self: AlignSelf::Center,
            width: px(380),
            ..default()
        },
        crate::ui_theme::theme().skin(crate::ui_theme::Skin::Wood),
    ))
    .with_children(|p| {
        p.spawn(crate::ui_theme::theme().title("Тело", 28.0));
        crate::ui_theme::theme().divider(p);
        p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(10), ..default() }).with_children(|row| {
            // Силуэт на листе пергамента.
            row.spawn((
                Node {
                    width: px(FIG_W + 2.0 * PLATE_PAD),
                    height: px(FIG_H + 2.0 * PLATE_PAD),
                    padding: UiRect::all(px(PLATE_PAD)),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(4)),
                    flex_shrink: 0.0,
                    ..default()
                },
                BorderColor::all(Color::srgb(0.36, 0.27, 0.15)),
                BackgroundColor(PARCHMENT),
            ))
            .with_children(|plate| {
            plate.spawn(Node { width: px(FIG_W), height: px(FIG_H), ..default() }).with_children(|fig| {
                let sc = |v: f32| px(v * FIG_SCALE);
                // Шея — не цель, просто чтобы голова не висела в воздухе.
                fig.spawn((
                    Node { position_type: PositionType::Absolute, left: sc(77.0), top: sc(60.0), width: sc(22.0), height: sc(22.0), ..default() },
                    BackgroundColor(OUTLINE),
                    FocusPolicy::Pass,
                ));
                for (part, x, y, w, h, r) in SHAPES {
                    let fill = part_fill(worst(part));
                    fig.spawn((
                        Hit::Care(CareTarget::Part(part)),
                        CareZone { border: OUTLINE },
                        Interaction::default(),
                        FocusPolicy::Pass,
                        Node {
                            position_type: PositionType::Absolute,
                            left: sc(x),
                            top: sc(y),
                            width: sc(w),
                            height: sc(h),
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::all(sc(r)),
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::Center,
                            row_gap: px(3),
                            ..default()
                        },
                        BorderColor::all(OUTLINE),
                        BackgroundColor(fill),
                    ))
                    .with_children(|z| {
                        // По метке на каждую рану, цветом тревоги.
                        for v in views.iter().filter(|v| v.part == part) {
                            z.spawn((
                                Node { width: px(10), height: px(10), border: UiRect::all(px(1)), border_radius: BorderRadius::all(px(5)), ..default() },
                                BackgroundColor(alarm_color(v.alarm)),
                                BorderColor::all(Color::BLACK.with_alpha(0.6)),
                                FocusPolicy::Pass,
                            ));
                        }
                        if part == BodyPart::Head {
                            // Рот: сюда кладут то, что пьют и едят.
                            z.spawn((
                                Hit::Care(CareTarget::Mouth),
                                CareZone { border: Color::srgb(0.3, 0.12, 0.08) },
                                Interaction::default(),
                                FocusPolicy::Pass,
                                Node {
                                    position_type: PositionType::Absolute,
                                    left: sc(15.0),
                                    bottom: sc(9.0),
                                    width: sc(22.0),
                                    height: sc(10.0),
                                    border: UiRect::all(px(2)),
                                    border_radius: BorderRadius::all(px(6)),
                                    ..default()
                                },
                                BorderColor::all(Color::srgb(0.3, 0.12, 0.08)),
                                BackgroundColor(Color::srgb(0.42, 0.2, 0.15)),
                            ));
                        }
                    });
                }
                fig.spawn((text(fonts, "Л", 13.0, OUTLINE, true), Node { position_type: PositionType::Absolute, left: sc(18.0), top: sc(244.0), ..default() }));
                fig.spawn((text(fonts, "П", 13.0, OUTLINE, true), Node { position_type: PositionType::Absolute, left: sc(144.0), top: sc(244.0), ..default() }));
            });
            });

            // Список ран по частям тела.
            row.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(3), flex_grow: 1.0, flex_shrink: 1.0, min_width: px(0), ..default() }).with_children(|list| {
                if views.is_empty() {
                    list.spawn(text(fonts, "Ран нет", 15.0, DIM, false));
                }
                for part in BodyPart::ALL {
                    let here: Vec<&WoundView> = views.iter().filter(|v| v.part == part).collect();
                    if here.is_empty() {
                        continue;
                    }
                    list.spawn(text(fonts, part.name(), 14.0, TITLE, true));
                    for v in here {
                        list.spawn((
                            Hit::Care(CareTarget::Wound(v.id)),
                            CareZone { border: alarm_color(v.alarm).with_alpha(0.35) },
                            Interaction::default(),
                            FocusPolicy::Pass,
                            Node {
                                flex_direction: FlexDirection::Column,
                                padding: UiRect::axes(px(6), px(3)),
                                border: UiRect::left(px(3)).with_top(px(1)).with_right(px(1)).with_bottom(px(1)),
                                ..default()
                            },
                            BorderColor::all(alarm_color(v.alarm).with_alpha(0.35)),
                            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3)),
                        ))
                        .with_children(|w| {
                            w.spawn((text(fonts, v.title.clone(), 13.0, alarm_color(v.alarm), true), FocusPolicy::Pass));
                            if !v.details.is_empty() {
                                w.spawn((text(fonts, v.details.join(", "), 12.0, TEXT, false), FocusPolicy::Pass));
                            }
                        });
                    }
                }
            });
        });
        let general = general_symptoms(body, k);
        let fx = effects(body);
        if !general.is_empty() || !fx.is_empty() {
            p.spawn(text(fonts, "Самочувствие", 14.0, TITLE, true));
            p.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(12), row_gap: px(1), ..default() }).with_children(|g| {
                for (line, a) in general {
                    g.spawn(text(fonts, line, 12.0, alarm_color(a), false));
                }
            });
            if !fx.is_empty() {
                p.spawn(text(fonts, fx.join("; "), 12.0, DIM, false));
            }
        }
        p.spawn(text(
            fonts,
            "Возьмите средство из сумки и положите на рану, на часть тела или в рот.\nЧто и куда класть — решать вам: не то средство не поможет, а то и навредит.",
            11.0,
            DIM,
            false,
        ));
    });
}

/// Подсветка места под мышью. Только наведение — окно не знает и не выдаёт, что правильно.
fn highlight_zones(mut zones: Query<(&Interaction, &CareZone, &mut BorderColor), Changed<Interaction>>) {
    for (i, zone, mut border) in &mut zones {
        let c = if *i == Interaction::None { zone.border } else { HOVER };
        *border = BorderColor::all(c);
    }
}
