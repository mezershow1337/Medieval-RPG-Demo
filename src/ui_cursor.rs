//! Мышь и окна: одно место, которое знает, открыто ли хоть одно окно.
//!
//! Пока открыто любое окно (сумка, торговля, кузница, книга, карта, стражник, усадьба, школа,
//! консоль, меню), курсор свободен. Как только последнее окно закрылось — чем угодно: Esc,
//! Tab, кнопкой «Уйти», сервером (сделка прошла, торговец закрыл лавку, урок начался) —
//! мышь снова захвачена и персонаж слушается, без лишнего клика по экрану.
//!
//! Раньше каждое окно возвращало мышь само и не во всех случаях (Esc, кнопки, закрытие
//! сервером), и после такого закрытия курсор висел на экране, а персонаж стоял.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

pub struct UiCursorPlugin;

impl Plugin for UiCursorPlugin {
    fn build(&self, app: &mut App) {
        // В PostUpdate: к этому времени все окна за кадр уже открылись или закрылись,
        // а Esc в меню (Update) видел ещё свободную мышь и паузу не открыл.
        app.add_systems(PostUpdate, return_cursor_after_windows);
    }
}

/// Все окна игры, которым нужна свободная мышь.
#[derive(SystemParam)]
pub struct OpenWindows<'w> {
    loot: Option<Res<'w, crate::loot_client::LootState>>,
    trade: Option<Res<'w, crate::trade_client::TradeState>>,
    menu: Option<Res<'w, crate::menu::MenuState>>,
    law: Option<Res<'w, crate::civil_client::LawUi>>,
    book: Option<Res<'w, crate::skill_book::BookUi>>,
    map: Option<Res<'w, crate::map::MapView>>,
    towns: Option<Res<'w, crate::towns_client::TownUi>>,
    school: Option<Res<'w, crate::school_client::SchoolUi>>,
    console: Option<Res<'w, crate::console::Console>>,
}

impl OpenWindows<'_> {
    /// Открыто окно, которое забирает мышь (меню и консоль считаются отдельно).
    pub fn game_window(&self) -> bool {
        self.loot.as_ref().is_some_and(|l| l.ui_open)
            || self.trade.as_ref().is_some_and(|t| t.window_open())
            || self.law.as_ref().is_some_and(|l| l.open())
            || self.book.as_ref().is_some_and(|b| b.open)
            || self.map.as_ref().is_some_and(|m| m.open)
            || self.towns.as_ref().is_some_and(|t| t.open())
            || self.school.as_ref().is_some_and(|s| s.open())
    }

    pub fn menu_open(&self) -> bool {
        self.menu.as_ref().is_some_and(|m| m.is_open())
    }

    pub fn console_open(&self) -> bool {
        self.console.as_ref().is_some_and(|c| c.open)
    }

    /// Хоть что-то открыто: окно, меню или консоль.
    pub fn any(&self) -> bool {
        self.game_window() || self.menu_open() || self.console_open()
    }

    fn in_game(&self) -> bool {
        // Без меню (тесты) — считаем, что идёт игра.
        self.menu.as_ref().is_none_or(|m| m.in_game)
    }
}

/// Что делать с мышью в этом кадре.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum CursorAction {
    Keep,
    Free,
    Lock,
}

/// `was_open` — было ли что-то открыто в прошлом кадре, `focused` — в фокусе ли окно игры:
/// без фокуса мышь не захватываем, иначе она застрянет в чужом окне (вернётся кликом).
fn decide(open: bool, was_open: bool, in_game: bool, focused: bool, background: bool) -> CursorAction {
    if open {
        CursorAction::Free
    } else if was_open && in_game && focused && !background {
        CursorAction::Lock
    } else {
        CursorAction::Keep
    }
}

fn return_cursor_after_windows(windows: OpenWindows, mut primary: Single<(&Window, &mut CursorOptions), With<PrimaryWindow>>, mut was_open: Local<bool>) {
    let open = windows.any();
    let (window, cursor) = &mut *primary;
    match decide(open, *was_open, windows.in_game(), window.focused, crate::settings::background_run()) {
        CursorAction::Free => {
            if cursor.grab_mode != CursorGrabMode::None || !cursor.visible {
                cursor.grab_mode = CursorGrabMode::None;
                cursor.visible = true;
            }
        }
        CursorAction::Lock => {
            if cursor.grab_mode != CursorGrabMode::Locked || cursor.visible {
                cursor.grab_mode = CursorGrabMode::Locked;
                cursor.visible = false;
            }
        }
        CursorAction::Keep => {}
    }
    *was_open = open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_window_frees_mouse() {
        assert_eq!(decide(true, false, true, true, false), CursorAction::Free);
        assert_eq!(decide(true, true, true, true, false), CursorAction::Free);
    }

    #[test]
    fn closing_last_window_returns_mouse_to_game() {
        assert_eq!(decide(false, true, true, true, false), CursorAction::Lock);
    }

    #[test]
    fn nothing_changes_while_playing() {
        // Окон не было и нет — мышь не трогаем (Esc без окон сам отпускает её).
        assert_eq!(decide(false, false, true, true, false), CursorAction::Keep);
    }

    /// Сумка открыта — мышь свободна; закрыли её (любым способом) — мышь снова в игре.
    #[test]
    fn closing_bag_relocks_mouse_in_app() {
        let mut app = App::new();
        app.add_plugins(UiCursorPlugin).insert_resource(crate::loot_client::LootState::default());
        let win = app
            .world_mut()
            .spawn((Window { focused: true, ..default() }, CursorOptions { grab_mode: CursorGrabMode::Locked, visible: false, ..default() }, PrimaryWindow))
            .id();
        let cursor = |app: &App| {
            let c = app.world().get::<CursorOptions>(win).unwrap();
            (c.grab_mode, c.visible)
        };
        app.update();
        assert_eq!(cursor(&app), (CursorGrabMode::Locked, false), "без окон мышь не трогаем");
        app.world_mut().resource_mut::<crate::loot_client::LootState>().ui_open = true;
        app.update();
        assert_eq!(cursor(&app), (CursorGrabMode::None, true), "открыта сумка — курсор виден");
        app.update();
        assert_eq!(cursor(&app), (CursorGrabMode::None, true));
        app.world_mut().resource_mut::<crate::loot_client::LootState>().ui_open = false;
        app.update();
        assert_eq!(cursor(&app), (CursorGrabMode::Locked, false), "сумку закрыли — мышь снова в игре");
    }

    #[test]
    fn no_grab_outside_game_without_focus_or_in_background() {
        assert_eq!(decide(false, true, false, true, false), CursorAction::Keep);
        assert_eq!(decide(false, true, true, false, false), CursorAction::Keep);
        assert_eq!(decide(false, true, true, true, true), CursorAction::Keep);
    }
}
