//! Настройки игры: графика, управление, звук. Хранятся в `settings.txt` рядом с exe
//! (простой формат «ключ = значение», его можно поправить и руками).
//!
//! Применяются сразу: камера (дальность прорисовки, FOV, эффекты, сглаживание),
//! тени солнца, вода, дым, масштаб рендера, окно (vsync, полный экран), мышь и громкость.
//! Качество текстур применяется при загрузке — то есть после перезапуска игры.

use bevy::anti_alias::fxaa::Fxaa;
use bevy::anti_alias::taa::TemporalAntiAliasing;
use bevy::audio::{GlobalVolume, Volume};
use bevy::camera::visibility::VisibilityRange;
use bevy::camera::primitives::Aabb;
use bevy::camera::RenderTarget;
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, ShadowFilteringMethod, VolumetricFog};
use bevy::mesh::skinning::SkinnedMesh;
use bevy::pbr::ScreenSpaceAmbientOcclusion;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, VideoModeSelection, WindowMode};
use std::path::PathBuf;

/// Все настройки. Уровни качества: 0 — низкое и дальше по возрастанию.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Settings {
    /// Дальность прорисовки, м: дальше камера ничего не рисует, дымка прячет край.
    pub draw_distance: f32,
    /// Тени: 0 — нет, 1 — низкие, 2 — средние, 3 — высокие.
    pub shadows: u8,
    /// Вода: 0 — простая (крупные волны), 1 — подробная (рябь, пена у берега).
    pub water: u8,
    /// Эффекты: 0 — низкие (FXAA, без блума и дыма), 1 — средние (TAA, блум, дым),
    /// 2 — высокие (+ SSAO), 3 — ультра (+ объёмный туман с лучами солнца).
    pub effects: u8,
    /// Текстуры: 0 — до 512 px, 1 — до 1024 px, 2 — полные. После перезапуска.
    pub textures: u8,
    /// Масштаб рендера 3D-сцены, % от разрешения окна (интерфейс всегда чёткий).
    pub render_scale: u32,
    pub vsync: bool,
    /// Окно: 0 — оконный режим, 1 — окно без рамки на весь экран, 2 — полный экран.
    pub window_mode: u8,
    /// Угол обзора по вертикали, градусы.
    pub fov: f32,
    /// Множитель чувствительности мыши.
    pub sensitivity: f32,
    /// Общая громкость 0..1.
    pub volume: f32,
    /// Громкость музыки, эффектов и окружения 0..1 (поверх общей, см. `audio::AudioSettings`).
    pub music: f32,
    pub sfx: f32,
    pub ambient: f32,
    /// Показывать FPS и пинг в углу экрана.
    pub show_fps: bool,
    /// Последний адрес, к которому подключались (поле «Подключиться»).
    pub last_address: String,
    /// Ник игрока (виден другим вместо числового номера).
    pub nickname: String,
}

/// Готовые наборы графики.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Preset {
    Low,
    Medium,
    High,
    Ultra,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Low, Preset::Medium, Preset::High, Preset::Ultra];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Low => "Низкие",
            Preset::Medium => "Средние",
            Preset::High => "Высокие",
            Preset::Ultra => "Ультра",
        }
    }

    /// (дальность, тени, вода, эффекты, текстуры, масштаб рендера)
    fn values(self) -> (f32, u8, u8, u8, u8, u32) {
        match self {
            Preset::Low => (200.0, 1, 0, 0, 0, 75),
            Preset::Medium => (350.0, 2, 1, 1, 1, 100),
            Preset::High => (600.0, 3, 1, 2, 2, 100),
            Preset::Ultra => (1000.0, 3, 1, 3, 2, 100),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        let mut s = Settings {
            draw_distance: 0.0,
            shadows: 0,
            water: 0,
            effects: 0,
            textures: 0,
            render_scale: 100,
            vsync: true,
            window_mode: 0,
            fov: 80.0,
            sensitivity: 1.0,
            volume: 0.8,
            music: 0.6,
            sfx: 0.9,
            ambient: 0.8,
            show_fps: false,
            last_address: String::new(),
            // Для начала — имя пользователя Windows; его можно сменить в главном меню.
            nickname: crate::protocol::clean_name(&std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default()),
        };
        // По умолчанию — средние: игра должна идти плавно и на обычной видеокарте.
        s.apply_preset(Preset::Medium);
        s
    }
}

pub const DRAW_DISTANCES: [f32; 7] = [150.0, 200.0, 275.0, 350.0, 450.0, 600.0, 1000.0];
pub const RENDER_SCALES: [u32; 6] = [50, 60, 70, 75, 85, 100];

impl Settings {
    pub fn apply_preset(&mut self, p: Preset) {
        let (d, sh, w, fx, tx, rs) = p.values();
        self.draw_distance = d;
        self.shadows = sh;
        self.water = w;
        self.effects = fx;
        self.textures = tx;
        self.render_scale = rs;
    }

    /// Какой набор совпадает с текущими значениями (если совпадает).
    pub fn preset(&self) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| {
            let mut s = self.clone();
            s.apply_preset(*p);
            s == *self
        })
    }

    /// Самая большая сторона текстур моделей и фотоматериалов (0 — без ограничения).
    pub fn texture_cap(&self) -> u32 {
        match self.textures {
            0 => 512,
            1 => 1024,
            _ => 0,
        }
    }

    /// Мелкие предметы (товары, добыча, утварь) не рисуются дальше этого расстояния.
    pub fn detail_distance(&self) -> f32 {
        (self.draw_distance * 0.3).clamp(45.0, 160.0)
    }

    /// Факелы и лампы гаснут для видеокарты дальше этого расстояния (плюс радиус света).
    pub fn light_distance(&self) -> f32 {
        match self.effects {
            0 => 18.0,
            1 => 28.0,
            _ => 35.0,
        }
    }

    fn clamp(&mut self) {
        self.draw_distance = self.draw_distance.clamp(100.0, 1500.0);
        self.shadows = self.shadows.min(3);
        self.water = self.water.min(1);
        self.effects = self.effects.min(3);
        self.textures = self.textures.min(2);
        self.window_mode = self.window_mode.min(2);
        self.render_scale = self.render_scale.clamp(40, 100);
        self.fov = self.fov.clamp(55.0, 110.0);
        self.sensitivity = self.sensitivity.clamp(0.1, 5.0);
        self.volume = self.volume.clamp(0.0, 1.0);
        self.music = self.music.clamp(0.0, 1.0);
        self.sfx = self.sfx.clamp(0.0, 1.0);
        self.ambient = self.ambient.clamp(0.0, 1.0);
    }

    pub fn to_text(&self) -> String {
        let b = |v: bool| if v { "да" } else { "нет" };
        format!(
            "# Настройки Medieval RPG. Можно править руками; неизвестные строки пропускаются.\n\
             draw_distance = {}\nshadows = {}\nwater = {}\neffects = {}\ntextures = {}\nrender_scale = {}\n\
             vsync = {}\nwindow_mode = {}\nfov = {}\nsensitivity = {}\nvolume = {}\nmusic = {}\nsfx = {}\nambient = {}\nshow_fps = {}\nlast_address = {}\nnickname = {}\n",
            self.draw_distance,
            self.shadows,
            self.water,
            self.effects,
            self.textures,
            self.render_scale,
            b(self.vsync),
            self.window_mode,
            self.fov,
            self.sensitivity,
            self.volume,
            self.music,
            self.sfx,
            self.ambient,
            b(self.show_fps),
            self.last_address,
            self.nickname,
        )
    }

    pub fn from_text(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let f = |d: f32| v.parse::<f32>().unwrap_or(d);
            let u = |d: u8| v.parse::<u8>().unwrap_or(d);
            let b = |d: bool| match v.to_lowercase().as_str() {
                "да" | "yes" | "true" | "1" | "on" => true,
                "нет" | "no" | "false" | "0" | "off" => false,
                _ => d,
            };
            match k {
                "draw_distance" => s.draw_distance = f(s.draw_distance),
                "shadows" => s.shadows = u(s.shadows),
                "water" => s.water = u(s.water),
                "effects" => s.effects = u(s.effects),
                "textures" => s.textures = u(s.textures),
                "render_scale" => s.render_scale = v.parse().unwrap_or(s.render_scale),
                "vsync" => s.vsync = b(s.vsync),
                "window_mode" => s.window_mode = u(s.window_mode),
                "fov" => s.fov = f(s.fov),
                "sensitivity" => s.sensitivity = f(s.sensitivity),
                "volume" => s.volume = f(s.volume),
                "music" => s.music = f(s.music),
                "sfx" => s.sfx = f(s.sfx),
                "ambient" => s.ambient = f(s.ambient),
                "show_fps" => s.show_fps = b(s.show_fps),
                "last_address" => s.last_address = v.to_string(),
                "nickname" => s.nickname = crate::protocol::clean_name(v),
                _ => {}
            }
        }
        s.clamp();
        s
    }

    /// Файл настроек — рядом с exe (игра распаковывается в свою папку).
    pub fn path() -> Option<PathBuf> {
        Some(std::env::current_exe().ok()?.parent()?.join("settings.txt"))
    }

    pub fn load() -> Settings {
        let Some(path) = Self::path() else { return Settings::default() };
        match std::fs::read_to_string(&path) {
            Ok(text) => Settings::from_text(&text),
            Err(_) => {
                // Старый файл меню (сборка до настроек) — подхватываем адрес.
                let mut s = Settings::default();
                if let Some(last) = path.parent().map(|d| d.join("last_address.txt")).and_then(|f| std::fs::read_to_string(f).ok()) {
                    s.last_address = last.trim().to_string();
                }
                s
            }
        }
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Err(e) = std::fs::write(&path, self.to_text()) {
            warn!("Не удалось сохранить настройки в {}: {e}", path.display());
        }
    }
}

/// Настройки, прочитанные при запуске: качество текстур нужно до того, как появится App.
static STARTUP: std::sync::OnceLock<Settings> = std::sync::OnceLock::new();

pub fn startup() -> &'static Settings {
    STARTUP.get_or_init(Settings::load)
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(startup().clone())
            .insert_resource(SceneTarget::default())
            .add_systems(
                PostUpdate,
                (apply_window, apply_audio, apply_sun, apply_cameras, apply_render_scale, apply_smoke, apply_water, detail_ranges, toggle_effects_f9, toggle_fullscreen_f11)
                    .before(bevy::camera::CameraUpdateSystems),
            );
    }
}

/// Номер «версии» настроек на камере: если отличается — настройки надо применить заново.
#[derive(Component)]
struct AppliedTo(u32);

fn bump(settings: &Res<Settings>, generation: &mut Local<u32>) -> u32 {
    if settings.is_changed() {
        **generation = generation.wrapping_add(1);
    }
    **generation
}

fn apply_window(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    if !settings.is_changed() {
        return;
    }
    let present = if settings.vsync { PresentMode::AutoVsync } else { PresentMode::AutoNoVsync };
    if window.present_mode != present {
        window.present_mode = present;
    }
    let mode = match settings.window_mode {
        0 => WindowMode::Windowed,
        1 => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        _ => WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Current),
    };
    if window.mode != mode {
        window.mode = mode;
    }
}

fn apply_audio(settings: Res<Settings>, mut volume: ResMut<GlobalVolume>, audio: Option<ResMut<crate::audio::AudioSettings>>) {
    if settings.is_changed() {
        volume.volume = Volume::Linear(settings.volume);
        if let Some(mut a) = audio {
            a.music = settings.music;
            a.sfx = settings.sfx;
            a.ambient = settings.ambient;
        }
    }
}

#[derive(Component)]
struct SunConfigured;

/// Тени солнца: размер карты, число каскадов, дальность.
fn apply_sun(settings: Res<Settings>, mut map: ResMut<DirectionalLightShadowMap>, mut sun: Query<(&mut DirectionalLight, &mut bevy::light::CascadeShadowConfig)>,
    fresh: Query<Entity, (With<DirectionalLight>, Without<SunConfigured>)>,
    mut commands: Commands,
) {
    if !settings.is_changed() && fresh.is_empty() {
        return;
    }
    for e in &fresh {
        commands.entity(e).insert(SunConfigured);
    }
    let full = crate::graphics::mood().shadow_distance;
    let (size, cascades, first, dist) = match settings.shadows {
        0 => (1024, 1, 10.0f32, 30.0f32),
        1 => (1024, 2, 10.0, 45.0),
        2 => (2048, 3, 12.0, 90.0),
        _ => (4096, 4, 12.0, full),
    };
    let dist = dist.min(settings.draw_distance);
    if map.size != size {
        map.size = size;
    }
    for (mut light, mut cfg) in &mut sun {
        light.shadow_maps_enabled = settings.shadows > 0;
        *cfg = CascadeShadowConfigBuilder { num_cascades: cascades, first_cascade_far_bound: first.min(dist * 0.5), maximum_distance: dist, ..default() }.build();
    }
}

/// Камеры: дальность, дымка, FOV, сглаживание и пост-эффекты.
#[allow(clippy::type_complexity)]
fn apply_cameras(
    mut commands: Commands,
    settings: Res<Settings>,
    mut generation: Local<u32>,
    mut cams: Query<(Entity, &mut Projection, Option<&AppliedTo>, Has<crate::player::PlayerCamera>), With<Camera3d>>,
) {
    let g = bump(&settings, &mut generation);
    let m = crate::graphics::mood();
    for (e, mut proj, applied, is_player) in &mut cams {
        if applied.is_some_and(|a| a.0 == g) {
            continue;
        }
        if let Projection::Perspective(p) = &mut *proj {
            p.far = settings.draw_distance;
            if is_player {
                p.fov = settings.fov.to_radians();
            }
        }
        let mut ec = commands.entity(e);
        ec.insert((AppliedTo(g), crate::graphics::distance_fog(&m, settings.draw_distance)));
        // Сглаживание: TAA требует предварительных проходов (глубина, движение) — на низких
        // эффектах дешевле FXAA.
        if settings.effects == 0 {
            ec.remove::<TemporalAntiAliasing>().insert((Fxaa::default(), ShadowFilteringMethod::Gaussian));
        } else {
            ec.remove::<Fxaa>().insert((TemporalAntiAliasing::default(), ShadowFilteringMethod::Temporal));
        }
        if settings.effects >= 1 {
            ec.insert(Bloom { intensity: if m.day { 0.16 } else { 0.2 }, ..Bloom::NATURAL });
        } else {
            ec.remove::<Bloom>();
        }
        if settings.effects >= 2 {
            ec.insert(ScreenSpaceAmbientOcclusion::default());
        } else {
            ec.remove::<ScreenSpaceAmbientOcclusion>();
        }
        if settings.effects >= 3 {
            ec.insert(VolumetricFog { ambient_intensity: 0.0, step_count: 48, jitter: 0.5, ..default() });
        } else {
            ec.remove::<VolumetricFog>();
        }
    }
}

/// Небо — сфера вокруг камеры; она должна помещаться внутрь дальности прорисовки.
pub fn sky_radius(settings: &Settings) -> f32 {
    settings.draw_distance * 0.9
}

/// Картинка, в которую рисуется 3D-сцена при масштабе рендера меньше 100 %.
#[derive(Resource, Default)]
pub struct SceneTarget {
    image: Option<Handle<Image>>,
    size: UVec2,
    node: Option<Entity>,
}

#[derive(Component)]
struct UiCamera;

fn apply_render_scale(
    mut commands: Commands,
    settings: Res<Settings>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut target: ResMut<SceneTarget>,
    mut images: ResMut<Assets<Image>>,
    cams: Query<(Entity, Option<&RenderTarget>), With<Camera3d>>,
    ui_cam: Query<Entity, With<UiCamera>>,
) {
    let full = window.physical_size();
    let scale = settings.render_scale as f32 / 100.0;
    // При 100 % интерфейс рисует сама 3D-камера (как раньше), отдельная камера интерфейса
    // выключена. Когда 3D-сцена рисуется в картинку меньшего размера, окно целиком —
    // за камерой интерфейса: она очищает его, кладёт картинку сцены и рисует интерфейс.
    let scaled = settings.render_scale < 100;
    // Камера интерфейса существует только в этом режиме: иначе Bevy отдал бы интерфейс ей,
    // даже выключенной, и интерфейс пропал бы.
    match (scaled, ui_cam.iter().next()) {
        (true, None) => {
            commands.spawn((
                UiCamera,
                IsDefaultUiCamera,
                Camera2d,
                Camera { order: 10, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
                Msaa::Off,
            ));
        }
        (false, Some(e)) => commands.entity(e).despawn(),
        _ => {}
    }
    if settings.render_scale >= 100 || full.x == 0 || full.y == 0 {
        // Полное разрешение: камеры рисуют прямо в окно.
        if let Some(node) = target.node.take() {
            commands.entity(node).despawn();
        }
        target.image = None;
        target.size = UVec2::ZERO;
        for (e, rt) in &cams {
            if !matches!(rt, None | Some(RenderTarget::Window(_))) {
                commands.entity(e).insert(RenderTarget::default());
            }
        }
        return;
    }
    let size = ((full.as_vec2() * scale).round().as_uvec2()).max(UVec2::splat(64));
    if target.image.is_none() || target.size != size {
        let image = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb));
        let handle = images.add(image);
        target.image = Some(handle.clone());
        target.size = size;
        match target.node {
            Some(node) => {
                commands.entity(node).insert(ImageNode::new(handle));
            }
            None => {
                let node = commands
                    .spawn((
                        ImageNode::new(handle),
                        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
                        // Под всем остальным интерфейсом.
                        GlobalZIndex(i32::MIN),
                    ))
                    .id();
                target.node = Some(node);
            }
        }
    }
    let handle = target.image.clone().unwrap();
    for (e, rt) in &cams {
        let ok = matches!(rt, Some(RenderTarget::Image(i)) if i.handle == handle);
        if !ok {
            commands.entity(e).insert(RenderTarget::Image(handle.clone().into()));
        }
    }
}

/// Дым из труб — много полупрозрачных слоёв; на низких эффектах его не рисуем.
fn apply_smoke(settings: Res<Settings>, mut smoke: Query<&mut Visibility, With<MeshMaterial3d<crate::smoke::SmokeMaterial>>>, added: Query<(), Added<MeshMaterial3d<crate::smoke::SmokeMaterial>>>) {
    if !settings.is_changed() && added.is_empty() {
        return;
    }
    let want = if settings.effects >= 1 { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut smoke {
        v.set_if_neq(want);
    }
}

/// Море: на простом качестве — только крупные волны и без пены по глубине.
fn apply_water(settings: Res<Settings>, mut mats: ResMut<Assets<crate::sky::WaterMaterial>>, added: Query<(), Added<MeshMaterial3d<crate::sky::WaterMaterial>>>) {
    if !settings.is_changed() && added.is_empty() {
        return;
    }
    let simple = settings.water == 0;
    for (_, m) in mats.iter_mut() {
        m.extension.water.params.z = if simple { 0.0 } else { 1.0 };
        m.extension.water.params.w = if simple { 1.0 } else { 0.0 };
    }
}

/// Отмечает мелкие меши (предметы, утварь, товары), которым назначена дальность видимости.
#[derive(Component)]
struct DetailRanged;

/// Мелкие предметы дальше `detail_distance` не рисуются вовсе (и не дают теней).
#[allow(clippy::type_complexity)]
fn detail_ranges(
    mut commands: Commands,
    settings: Res<Settings>,
    fresh: Query<(Entity, &Aabb, &GlobalTransform), (With<Mesh3d>, Without<DetailRanged>, Without<SkinnedMesh>, Without<NoDetailRange>)>,
    mut ranged: Query<&mut VisibilityRange, With<DetailRanged>>,
) {
    let end = settings.detail_distance();
    let range = |end: f32| VisibilityRange { start_margin: 0.0..0.0, end_margin: end..end + 8.0, use_aabb: false };
    for (e, aabb, gt) in &fresh {
        let size = Vec3::from(aabb.half_extents) * 2.0 * gt.scale().abs();
        if size.max_element() < 2.5 {
            commands.entity(e).insert((DetailRanged, range(end)));
        } else {
            commands.entity(e).insert((DetailRanged,));
        }
    }
    if settings.is_changed() {
        for mut r in &mut ranged {
            *r = range(end);
        }
    }
}

/// Меш, который нельзя прятать по расстоянию (оружие в руках, прицел и т. п.).
#[derive(Component)]
pub struct NoDetailRange;

/// F9 — быстро переключить эффекты: низкие ↔ сохранённые (или высокие).
fn toggle_effects_f9(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>, mut prev: Local<Option<u8>>) {
    if !keys.just_pressed(KeyCode::F9) {
        return;
    }
    if settings.effects > 0 {
        *prev = Some(settings.effects);
        settings.effects = 0;
    } else {
        settings.effects = prev.take().unwrap_or(2);
    }
    info!("Эффекты: {}", effects_name(settings.effects));
    settings.save();
}

/// F11 — полный экран ↔ окно.
fn toggle_fullscreen_f11(keys: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>, mut prev: Local<Option<u8>>) {
    if !keys.just_pressed(KeyCode::F11) {
        return;
    }
    if settings.window_mode > 0 {
        *prev = Some(settings.window_mode);
        settings.window_mode = 0;
    } else {
        settings.window_mode = prev.take().unwrap_or(1);
    }
    settings.save();
}

pub fn window_mode_name(v: u8) -> &'static str {
    match v {
        0 => "в окне",
        1 => "окно без рамки",
        _ => "полный экран",
    }
}

pub fn effects_name(v: u8) -> &'static str {
    match v {
        0 => "низкие",
        1 => "средние",
        2 => "высокие",
        _ => "ультра",
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip() {
        let mut s = Settings::default();
        s.apply_preset(Preset::Low);
        s.fov = 95.0;
        s.vsync = false;
        s.last_address = "abc.at.ply.gg:12345".into();
        s.nickname = "Феодор".into();
        let back = Settings::from_text(&s.to_text());
        assert_eq!(s, back);
        assert_eq!(back.preset(), Some(Preset::Low));
    }

    #[test]
    fn garbage_and_out_of_range_are_safe() {
        let s = Settings::from_text("fov = 9000\nshadows = abc\n???\nrender_scale = 5\n");
        assert_eq!(s.fov, 110.0);
        assert_eq!(s.shadows, Settings::default().shadows);
        assert_eq!(s.render_scale, 40);
    }

    #[test]
    fn default_is_medium() {
        assert_eq!(Settings::default().preset(), Some(Preset::Medium));
    }
}

