//! Снимки системы репутации без рук: `MRPG_REP_DEMO=папка` (вместе с `MRPG_BACKGROUND=1`).
//!
//! Игрок появляется в розыске (сервер ставит гарнизону −100, горожанам −40 и штраф),
//! подходит к казначею гарнизона, платит штраф, отрабатывает смену общественных работ и открывает окно отношений. На каждом шаге — снимок в папку:
//! `rep_1_fine.png`, `rep_2_paid.png`, `rep_3_work.png`, `rep_4_standing.png`.
//! Всё идёт через настоящий сервер: те же сообщения, что шлёт окно торговца.

use crate::player::LocalPlayer;
use crate::trade_client::{TradeState, send};
use crate::trade_net::TradeClientMessage;
use bevy::prelude::*;
use bevy_renet::RenetClient;

pub struct RepDemoPlugin;

impl Plugin for RepDemoPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var_os("MRPG_REP_DEMO").is_some() {
            app.add_systems(Update, rep_demo);
        }
    }
}

#[derive(Default)]
struct Demo {
    step: u8,
    frames: u32,
    shot_frames: u32,
}

#[allow(clippy::too_many_arguments)]
fn rep_demo(
    mut commands: Commands,
    mut demo: Local<Demo>,
    mut state: ResMut<TradeState>,
    mut console: ResMut<crate::console::Console>,
    player: Query<(), With<LocalPlayer>>,
    mut client: Option<ResMut<RenetClient>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(client) = client.as_deref_mut() else { return };
    if player.is_empty() || state.merchants.is_empty() {
        return;
    }
    let dir = std::env::var("MRPG_REP_DEMO").unwrap_or_else(|_| ".".into());
    let Some(id) = crate::trade::merchant_by_key("markianos") else { return };
    let Some(m) = state.merchant(id).cloned() else { return };
    demo.frames += 1;
    let shot = |commands: &mut Commands, name: &str| {
        info!("репутация: снимок {name}");
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(format!("{dir}/{name}.png")));
    };
    match demo.step {
        // Встать в двух шагах перед казначеем, лицом к нему.
        0 if demo.frames > 20 => {
            let pos = Vec3::from(m.pos);
            let front = Vec3::new(-m.yaw.sin(), 0.0, -m.yaw.cos());
            let at = pos + front * 2.2;
            let to = pos - at;
            let yaw = (-to.x).atan2(-to.z);
            // Бессмертие и «не замечают»: на медленном программном рендере стража успевает
            // остановить игрока раньше, чем он дойдёт до казначея, и драка испортит снимки.
            console.queue.push("god 1".into());
            console.queue.push("notarget 1".into());
            console.queue.push(format!("tp {:.2} {:.2} {:.2} {:.3} -0.05", at.x, at.y + 0.05, at.z, yaw));
            demo.step = 1;
            demo.frames = 0;
        }
        1 if demo.frames > 20 => {
            send(client, &TradeClientMessage::Open { merchant: id });
            demo.step = 2;
            demo.frames = 0;
        }
        // Окно открыто: штраф и вражда с гарнизоном.
        2 if state.view.is_some() && state.rep.is_some() && demo.frames > 40 => {
            shot(&mut commands, "rep_1_fine");
            demo.step = 3;
            demo.frames = 0;
        }
        3 if demo.frames > 10 => {
            send(client, &TradeClientMessage::PayFine);
            demo.step = 4;
            demo.frames = 0;
        }
        // Штраф уплачен: −100 → −50, смена общественных работ открыта.
        4 if state.fine == 0 && state.view.as_ref().is_some_and(|v| v.refuse.is_none()) && demo.frames > 10 => {
            shot(&mut commands, "rep_2_paid");
            demo.step = 5;
            demo.frames = 0;
        }
        5 if demo.frames > 10 => {
            send(client, &TradeClientMessage::StartLesson { index: 0 });
            demo.step = 6;
            demo.frames = 0;
        }
        6 if state.lesson.is_some() => {
            demo.step = 7;
            demo.frames = 0;
        }
        // Работа: ровные мазки «умелого работника». На программном рендере кадр идёт долго,
        // и разбор движений мыши не успевает, поэтому мазки шлём прямо, как это делает
        // шкала работы; сервер сам не примет слишком частые.
        7 if state.lesson.is_some() => {
            demo.shot_frames += 1;
            if demo.shot_frames.is_multiple_of(3) {
                send(client, &TradeClientMessage::Stroke { q: 0.8 });
            }
            demo.frames = 0;
        }
        // Смена окончена: уведомление о прибавке репутации.
        7 if demo.frames > 6 => {
            shot(&mut commands, "rep_3_work");
            demo.step = 8;
            demo.frames = 0;
        }
        8 if demo.frames > 10 => {
            if state.view.take().is_some() {
                send(client, &TradeClientMessage::Close);
            }
            state.rep_open = true;
            state.rev += 1;
            demo.step = 9;
            demo.frames = 0;
        }
        9 if demo.frames > 10 => {
            shot(&mut commands, "rep_4_standing");
            demo.step = 10;
            demo.frames = 0;
            demo.shot_frames = 0;
        }
        10 => {
            demo.shot_frames += 1;
            let saved = std::path::Path::new(&format!("{dir}/rep_4_standing.png")).exists();
            if (saved && demo.shot_frames > 6) || demo.shot_frames > 200 {
                exit.write(AppExit::Success);
            }
        }
        _ => {}
    }
}
