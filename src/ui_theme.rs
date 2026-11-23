//! Общая тема интерфейса в духе Херсонеса VI века.
//!
//! Все окна строятся из одних деталей:
//! * подложки ([`Skin`]) — тёмная доска в бронзовой раме с витым жгутом и бляхами-крестами,
//!   кожа со строчкой и железными уголками, пергамент с рамкой чернилами и киноварью;
//! * кнопки ([`ThemeButton`]) — кожаная вставка в бронзовом ободке, наведение и нажатие
//!   анимируются сами;
//! * ячейки ([`ThemeSlot`]) — углубление с тонким бронзовым ободком;
//! * разделитель с крестом, рамка полосы (здоровье, выносливость), затемнение мира за окном;
//! * шрифты с кириллицей: заголовки — унциал Ponomar (так писали книги в VI веке),
//!   текст — Alegreya (гуманистический каллиграфический шрифт).
//!
//! Текстуры делает `tools/ui/build_ui_textures.py` из фотосканов ambientCG (CC0) и рельефа,
//! освещённого как металл. Всё встраивается в exe, как и шрифты.
//!
//! Как пользоваться:
//! ```ignore
//! fn setup(mut commands: Commands, theme: Res<UiTheme>) {
//!     commands.spawn((Node { padding: Skin::Wood.padding(), ..default() }, theme.skin(Skin::Wood), Appear::default()))
//!         .with_children(|p| {
//!             p.spawn(theme.title("Сумка", 24.0));
//!             theme.divider(p);
//!             theme.button(p, "Забрать всё", MyAction, true);
//!         });
//! }
//! ```

#![allow(deprecated)]
// Interaction: в Bevy 0.20 помечен устаревшим, но работает и проще.
#![allow(dead_code)] // часть деталей — для главного меню, хотбара и будущих окон

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use bevy::text::FontSize;
use bevy::ui::FocusPolicy;

/// Цвета текста и заливок.
pub mod ink {
    use bevy::prelude::Color;
    /// Заголовки на дереве и коже: светлая бронза.
    pub const TITLE: Color = Color::srgb(0.95, 0.82, 0.55);
    /// Основной текст на тёмном.
    pub const TEXT: Color = Color::srgb(0.90, 0.85, 0.74);
    /// Пояснения, второстепенное.
    pub const DIM: Color = Color::srgb(0.66, 0.60, 0.50);
    pub const GOOD: Color = Color::srgb(0.58, 0.84, 0.45);
    pub const BAD: Color = Color::srgb(0.95, 0.45, 0.35);
    /// Деньги и цены.
    pub const GOLD: Color = Color::srgb(0.96, 0.78, 0.38);
    /// Чернила на пергаменте (сажа с камедью).
    pub const INK: Color = Color::srgb(0.20, 0.13, 0.08);
    /// Бледные чернила на пергаменте.
    pub const INK_DIM: Color = Color::srgb(0.42, 0.32, 0.22);
    /// Киноварь: заголовки и выделения в рукописях.
    pub const RUBRIC: Color = Color::srgb(0.62, 0.15, 0.09);
}

/// Шрифты с кириллицей (лицензия OFL — assets/fonts/*-OFL.txt).
#[derive(Resource, Clone)]
pub struct UiFonts {
    pub regular: Handle<Font>,
    pub bold: Handle<Font>,
    /// Унциал для заголовков окон.
    pub title: Handle<Font>,
}

/// Подложка окна.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Skin {
    /// Главные окна: сумка, торговля, кузня, перегонка.
    Wood,
    /// Малые окна: подсказки, всплывающие описания, HUD.
    Leather,
    /// Книги, знания, записи.
    Parchment,
}

impl Skin {
    /// Толщина рамки в текстуре (пиксели) — столько же займёт на экране.
    const fn border(self) -> f32 {
        match self {
            Skin::Wood => 32.0,
            Skin::Leather => 18.0,
            Skin::Parchment => 40.0,
        }
    }

    /// Отступ содержимого от края, чтобы текст не залезал на раму.
    pub fn padding(self) -> UiRect {
        match self {
            Skin::Wood => UiRect::axes(px(30), px(28)),
            Skin::Leather => UiRect::axes(px(16), px(12)),
            Skin::Parchment => UiRect::axes(px(40), px(36)),
        }
    }

    /// Цвет основного текста на этой подложке.
    pub fn text(self) -> Color {
        if self == Skin::Parchment {
            ink::INK
        } else {
            ink::TEXT
        }
    }

    pub fn title(self) -> Color {
        if self == Skin::Parchment {
            ink::RUBRIC
        } else {
            ink::TITLE
        }
    }

    pub fn dim(self) -> Color {
        if self == Skin::Parchment {
            ink::INK_DIM
        } else {
            ink::DIM
        }
    }
}

/// Текстуры и шрифты темы.
#[derive(Resource, Clone)]
pub struct UiTheme {
    pub fonts: UiFonts,
    wood: Handle<Image>,
    leather: Handle<Image>,
    parchment: Handle<Image>,
    /// normal, hover, pressed, disabled
    button: [Handle<Image>; 4],
    /// normal, hover, selected
    slot: [Handle<Image>; 3],
    bar_frame: Handle<Image>,
    divider_line: Handle<Image>,
    divider_mid: Handle<Image>,
    vignette: Handle<Image>,
}

pub struct UiThemePlugin;

impl Plugin for UiThemePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SideDock>().add_systems(
            Update,
            (
                style_buttons,
                style_slots,
                appear_children,
                appear,
                update_plaques,
                scroll_wheel,
                refresh_slices_on_new_camera,
            )
                .chain(),
        );
    }
}

fn png(images: &mut Assets<Image>, bytes: &[u8]) -> Handle<Image> {
    let img = Image::from_buffer(
        bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::linear(),
        RenderAssetUsages::default(),
    )
    .expect("встроенная текстура интерфейса");
    images.add(img)
}

/// Загружает шрифты и текстуры темы (встроены в exe, поэтому готовы сразу, в том же кадре).
/// Все системы Startup, которые строят интерфейс, ставятся `.after(setup_fonts)`.
pub fn setup_fonts(
    mut commands: Commands,
    mut fonts: ResMut<Assets<Font>>,
    mut images: ResMut<Assets<Image>>,
) {
    macro_rules! font {
        ($f:literal) => {
            fonts.add(Font::from_bytes(
                include_bytes!(concat!("../assets/fonts/", $f)).to_vec(),
            ))
        };
    }
    macro_rules! img {
        ($f:literal) => {
            png(&mut images, include_bytes!(concat!("../assets/ui/", $f)))
        };
    }
    let ui_fonts = UiFonts {
        regular: font!("Alegreya-Regular.ttf"),
        bold: font!("Alegreya-Bold.ttf"),
        title: font!("Ponomar-Regular.ttf"),
    };
    commands.insert_resource(ui_fonts.clone());
    let theme = UiTheme {
        fonts: ui_fonts,
        wood: img!("panel_wood.png"),
        leather: img!("panel_leather.png"),
        parchment: img!("panel_parchment.png"),
        button: [
            img!("button_normal.png"),
            img!("button_hover.png"),
            img!("button_pressed.png"),
            img!("button_disabled.png"),
        ],
        slot: [
            img!("slot_normal.png"),
            img!("slot_hover.png"),
            img!("slot_selected.png"),
        ],
        bar_frame: img!("bar_frame.png"),
        divider_line: img!("divider_line.png"),
        divider_mid: img!("divider_mid.png"),
        vignette: img!("vignette.png"),
    };
    let _ = THEME.set(theme.clone());
    commands.insert_resource(theme);
}

static THEME: std::sync::OnceLock<UiTheme> = std::sync::OnceLock::new();

/// Тема для вспомогательных функций построения окон, куда неудобно протаскивать `Res<UiTheme>`.
/// Готова после [`setup_fonts`] (первый Startup), до неё — паника.
pub fn theme() -> &'static UiTheme {
    THEME.get().expect("ui_theme::setup_fonts ещё не выполнен")
}

fn sliced(image: Handle<Image>, border: f32, tile: bool) -> ImageNode {
    let mode = if tile {
        SliceScaleMode::Tile { stretch_value: 1.0 }
    } else {
        SliceScaleMode::Stretch
    };
    with_mode(
        image,
        NodeImageMode::Sliced(TextureSlicer {
            border: BorderRect::all(border),
            center_scale_mode: mode,
            sides_scale_mode: mode,
            max_corner_scale: 1.0,
        }),
    )
}

fn with_mode(image: Handle<Image>, image_mode: NodeImageMode) -> ImageNode {
    // Картинка рамки — на весь узел, включая отступы (по умолчанию Bevy рисует только в content box).
    ImageNode { image_mode, visual_box: VisualBox::BorderBox, ..ImageNode::new(image) }
}

impl UiTheme {
    /// Подложка окна (рама и фон). Ставится на тот же узел, что и `Node`; `BackgroundColor` не нужен.
    pub fn skin(&self, skin: Skin) -> impl Bundle {
        let image = match skin {
            Skin::Wood => self.wood.clone(),
            Skin::Leather => self.leather.clone(),
            Skin::Parchment => self.parchment.clone(),
        };
        (
            sliced(image, skin.border(), true),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.6),
                px(0),
                px(6),
                px(2),
                px(18),
            ),
        )
    }

    /// Затемнение мира за открытым окном (на весь корневой узел, вместо полупрозрачного чёрного).
    pub fn backdrop(&self) -> ImageNode {
        with_mode(self.vignette.clone(), NodeImageMode::Stretch)
    }

    /// Вид ячейки. Для живой подсветки при наведении добавьте [`ThemeSlot`] и `Interaction`.
    pub fn slot_image(&self, selected: bool) -> ImageNode {
        sliced(self.slot[if selected { 2 } else { 0 }].clone(), 10.0, false)
    }

    /// Ячейка с подсветкой при наведении (нужен ещё `Interaction` на узле).
    pub fn slot(&self, selected: bool) -> impl Bundle {
        (self.slot_image(selected), ThemeSlot { selected })
    }

    /// Рамка полосы (здоровье, выносливость, нагрев): центр прозрачный, заливка — дочерним узлом.
    pub fn bar_frame(&self) -> ImageNode {
        sliced(self.bar_frame.clone(), 10.0, false)
    }

    /// Кожаная плашка под подсказку («[E] Открыть дверь», реплика горожанина).
    /// Внутрь кладётся текст с [`Plaque`]; пока текст пуст или скрыт — плашки не видно.
    pub fn plaque<'a>(&self, p: &'a mut ChildSpawnerCommands) -> EntityCommands<'a> {
        p.spawn((
            PlaqueBox,
            Node {
                padding: UiRect::axes(px(18), px(9)),
                display: Display::None,
                max_width: px(760),
                justify_content: JustifyContent::Center,
                ..default()
            },
            self.skin(Skin::Leather),
            FocusPolicy::Pass,
        ))
    }

    /// Полоса в железной рамке: `node` задаёт размер, `k` — заполнение 0..1.
    /// `fill_extra` ставится на заливку (маркер, чтобы потом менять ширину и цвет).
    pub fn bar(
        &self,
        p: &mut ChildSpawnerCommands,
        mut node: Node,
        k: f32,
        color: Color,
        fill_extra: impl Bundle,
    ) {
        node.padding = UiRect::axes(px(5), px(4));
        p.spawn((node, self.bar_frame(), FocusPolicy::Pass))
            .with_children(|b| {
                b.spawn((
                    fill_extra,
                    Node {
                        width: percent(k.clamp(0.0, 1.0) * 100.0),
                        height: percent(100),
                        border_radius: BorderRadius::all(px(2)),
                        ..default()
                    },
                    BackgroundColor(color),
                ));
            });
    }

    /// Текстура кнопки для состояния (для своих систем подсветки).
    pub fn button_image(&self, enabled: bool) -> ImageNode {
        self.button_image_state(if enabled { 0 } else { 3 })
    }

    /// 0 — обычная, 1 — наведена, 2 — нажата/выбрана, 3 — неактивна.
    pub fn button_image_state(&self, idx: usize) -> ImageNode {
        sliced(self.button[idx.min(3)].clone(), 14.0, false)
    }

    pub fn text(&self, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
        (
            Text::new(s),
            TextFont {
                font: self.fonts.regular.clone().into(),
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(color),
        )
    }

    pub fn bold(&self, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
        (
            Text::new(s),
            TextFont {
                font: self.fonts.bold.clone().into(),
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(color),
        )
    }

    /// Заголовок окна: унциал светлой бронзой с тенью.
    pub fn title(&self, s: impl Into<String>, size: f32) -> impl Bundle {
        self.title_colored(s, size, ink::TITLE)
    }

    pub fn title_colored(&self, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
        (
            Text::new(s),
            TextFont {
                font: self.fonts.title.clone().into(),
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(color),
            TextShadow {
                offset: Vec2::new(1.0, 2.0),
                color: Color::srgba(0.0, 0.0, 0.0, 0.75),
            },
        )
    }

    /// Разделитель: бронзовая линия с крестом посередине.
    pub fn divider(&self, p: &mut ChildSpawnerCommands) {
        let line = |img: Handle<Image>| {
            (
                with_mode(
                    img,
                    NodeImageMode::Tiled {
                        tile_x: true,
                        tile_y: false,
                        stretch_value: 1.0,
                    },
                ),
                Node {
                    flex_grow: 1.0,
                    height: px(12),
                    ..default()
                },
                FocusPolicy::Pass,
            )
        };
        p.spawn((
            Node {
                width: percent(100),
                align_items: AlignItems::Center,
                margin: UiRect::vertical(px(2)),
                ..default()
            },
            FocusPolicy::Pass,
        ))
        .with_children(|r| {
            r.spawn(line(self.divider_line.clone()));
            r.spawn((
                ImageNode::new(self.divider_mid.clone()),
                Node {
                    width: px(48),
                    height: px(24),
                    flex_shrink: 0.0,
                    ..default()
                },
                FocusPolicy::Pass,
            ));
            r.spawn(line(self.divider_line.clone()));
        });
    }

    /// Кнопка с подписью. `extra` — компонент действия (что делать по нажатию).
    pub fn button<'a>(
        &self,
        p: &'a mut ChildSpawnerCommands,
        label: &str,
        extra: impl Bundle,
        enabled: bool,
    ) -> EntityCommands<'a> {
        self.button_sized(p, label, 16.0, extra, enabled)
    }

    /// Маленькая кнопка-переключатель: `on` — выбрана (вдавлена, подпись ярче).
    pub fn toggle_button<'a>(
        &self,
        p: &'a mut ChildSpawnerCommands,
        label: &str,
        extra: impl Bundle,
        on: bool,
    ) -> EntityCommands<'a> {
        let mut e = self.small_button(p, label, extra, true);
        e.insert((
            ThemeButton { enabled: true, on },
            self.button_image_state(if on { 2 } else { 0 }),
        ));
        e
    }

    /// Маленькая кнопка для строк списков (торговля, рецепты).
    pub fn small_button<'a>(
        &self,
        p: &'a mut ChildSpawnerCommands,
        label: &str,
        extra: impl Bundle,
        enabled: bool,
    ) -> EntityCommands<'a> {
        let mut e = self.button_sized(p, label, 13.0, extra, enabled);
        e.insert(Node {
            padding: UiRect::axes(px(10), px(3)),
            min_height: px(24),
            flex_shrink: 0.0,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        });
        e
    }

    pub fn button_sized<'a>(
        &self,
        p: &'a mut ChildSpawnerCommands,
        label: &str,
        size: f32,
        extra: impl Bundle,
        enabled: bool,
    ) -> EntityCommands<'a> {
        let color = if enabled {
            ink::TITLE
        } else {
            Color::srgb(0.55, 0.53, 0.5)
        };
        let mut e = p.spawn((
            extra,
            Interaction::default(),
            ThemeButton { enabled, on: false },
            self.button_image(enabled),
            Node {
                padding: UiRect::axes(px(16), px(7)),
                min_height: px(34),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ));
        e.with_children(|b| {
            b.spawn((
                self.bold(label, size, color),
                TextShadow {
                    offset: Vec2::new(1.0, 1.0),
                    color: Color::srgba(0.0, 0.0, 0.0, 0.8),
                },
                TextLayout::justify(Justify::Center),
            ));
        });
        e
    }
}

/// Кнопка темы: картинка меняется по `Interaction` (обычная, наведена, нажата, неактивна).
#[derive(Component, Clone, Copy)]
pub struct ThemeButton {
    pub enabled: bool,
    /// Выбранный вариант (огонь «сильный», включённая баня): кнопка выглядит вдавленной.
    pub on: bool,
}

/// Ячейка темы: подсвечивается при наведении.
#[derive(Component, Clone, Copy)]
pub struct ThemeSlot {
    pub selected: bool,
}

#[allow(deprecated)]
fn style_buttons(
    theme: Res<UiTheme>,
    mut q: Query<
        (
            &Interaction,
            &ThemeButton,
            &mut ImageNode,
            Option<&mut UiTransform>,
        ),
        Or<(Changed<Interaction>, Changed<ThemeButton>)>,
    >,
) {
    for (i, b, mut img, tf) in &mut q {
        let idx = match (b.enabled, i) {
            (false, _) => 3,
            (true, Interaction::Pressed) => 2,
            (true, Interaction::Hovered) => 1,
            (true, Interaction::None) if b.on => 2,
            (true, Interaction::None) => 0,
        };
        img.image = theme.button[idx].clone();
        if let Some(mut tf) = tf {
            tf.scale = Vec2::splat(if idx == 2 { 0.97 } else { 1.0 });
        }
    }
}

#[allow(deprecated)]
fn style_slots(
    theme: Res<UiTheme>,
    mut q: Query<(&Interaction, &ThemeSlot, &mut ImageNode), Changed<Interaction>>,
) {
    for (i, s, mut img) in &mut q {
        img.image = match (s.selected, i) {
            (true, _) => theme.slot[2].clone(),
            (false, Interaction::None) => theme.slot[0].clone(),
            (false, _) => theme.slot[1].clone(),
        };
    }
}

/// Плавное появление окна: чуть приподнимается и разворачивается за ~0.16 с.
/// Ставить только когда окно открывается, а не при каждой перестройке.
#[derive(Component)]
pub struct Appear {
    t: f32,
}

impl Default for Appear {
    fn default() -> Self {
        Self { t: 0.0 }
    }
}

const APPEAR_SECS: f32 = 0.16;

fn appear(
    mut commands: Commands,
    time: Res<Time>,
    mut q: Query<(Entity, &mut Appear, Option<&mut UiTransform>)>,
) {
    for (e, mut a, tf) in &mut q {
        a.t += time.delta_secs() / APPEAR_SECS;
        let k = a.t.min(1.0);
        let ease = 1.0 - (1.0 - k).powi(3);
        let want = UiTransform {
            scale: Vec2::splat(0.94 + 0.06 * ease),
            translation: Val2::px(0.0, 10.0 * (1.0 - ease)),
            ..default()
        };
        match tf {
            Some(mut tf) => *tf = want,
            None => {
                commands.entity(e).insert(want);
            }
        }
        if k >= 1.0 {
            commands.entity(e).remove::<Appear>();
        }
    }
}

/// Поставить на корень окна (с затемнением): плавно появятся его дочерние окна, а не затемнение.
#[derive(Component)]
pub struct AppearChildren;

fn appear_children(mut commands: Commands, q: Query<(Entity, &Children), With<AppearChildren>>) {
    for (e, children) in &q {
        for c in children.iter() {
            commands.entity(c).insert(Appear::default());
        }
        commands.entity(e).remove::<AppearChildren>();
    }
}

/// Текст внутри [`UiTheme::plaque`].
#[derive(Component)]
pub struct Plaque;

#[derive(Component)]
struct PlaqueBox;

#[allow(clippy::type_complexity)]
fn update_plaques(
    texts: Query<(&Text, &Node, &ChildOf), (With<Plaque>, Or<(Changed<Text>, Changed<Node>)>)>,
    mut boxes: Query<&mut Node, (With<PlaqueBox>, Without<Plaque>)>,
) {
    for (text, node, parent) in &texts {
        let Ok(mut b) = boxes.get_mut(parent.parent()) else {
            continue;
        };
        let want = if text.0.trim().is_empty() || node.display == Display::None {
            Display::None
        } else {
            Display::Flex
        };
        if b.display != want {
            b.display = want;
        }
    }
}

/// Окна, пристыкованные к левому краю (кузня, перегонка). Сумка встаёт правее, а не под них.
#[derive(Resource, Default, Clone, Copy, PartialEq)]
pub struct SideDock {
    pub smith: f32,
    pub brew: f32,
}

impl SideDock {
    /// Сколько пикселей слева занято (с отступом).
    pub fn width(&self) -> f32 {
        let w = self.smith.max(self.brew);
        if w > 0.0 { w + 24.0 } else { 0.0 }
    }
}

/// Прокрутка колесом мыши для узла с `Overflow::scroll_y()` (нужен `Interaction` на нём).
#[derive(Component)]
pub struct ScrollWheel;

fn scroll_wheel(
    scroll: Res<bevy::input::mouse::AccumulatedMouseScroll>,
    mut lists: Query<(&Interaction, &mut ScrollPosition, &ComputedNode), With<ScrollWheel>>,
) {
    if scroll.delta.y == 0.0 {
        return;
    }
    let step = match scroll.unit {
        bevy::input::mouse::MouseScrollUnit::Line => 36.0,
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

/// Bevy 0.20 извлекает 9-slice картинки в рендер только при изменении узла. Узлы HUD, созданные
/// до появления камеры (в Startup), так и оставались невидимыми. Когда появляется новая камера,
/// несколько кадров «трогаем» все картинки, чтобы они извлеклись заново.
fn refresh_slices_on_new_camera(cams: Query<(), Added<Camera>>, mut images: Query<&mut ImageNode>, mut frames: Local<u8>) {
    if !cams.is_empty() {
        *frames = 3;
    }
    if *frames > 0 {
        *frames -= 1;
        for mut img in &mut images {
            img.set_changed();
        }
    }
}
