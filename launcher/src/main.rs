//! Лаунчер Medieval RPG: проверяет последнюю сборку в GitHub Releases, сам ставит
//! и обновляет игру (только изменившиеся файлы), обновляет сам себя и запускает игру.
//!
//! Лаунчер кладётся в папку с игрой (или в пустую папку — тогда он поставит игру туда).

#![cfg_attr(windows, windows_subsystem = "windows")]

mod github;
mod theme;
mod update;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Rect, pos2, vec2};
use github::Release;
use theme::{Theme, ink};
use update::{Bytes, Step};

const W: f32 = 1024.0;
const H: f32 = 640.0;

#[derive(Clone, Debug, PartialEq)]
enum Phase {
    Checking,
    SelfUpdating,
    Work(Step),
    Ready { changed: Option<usize> },
    /// Нет связи, но игра стоит — можно играть.
    Offline(String),
    Error(String),
}

impl Phase {
    fn busy(&self) -> bool {
        matches!(self, Phase::Checking | Phase::SelfUpdating | Phase::Work(_))
    }
}

struct Shared {
    phase: Mutex<Phase>,
    release: Mutex<Option<Release>>,
    installed: Mutex<Option<String>>,
    bytes: Arc<Bytes>,
    restart: AtomicBool,
}

fn game_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("MRPG_GAME_DIR") {
        return PathBuf::from(d);
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn set(sh: &Shared, ctx: &egui::Context, p: Phase) {
    *sh.phase.lock().unwrap() = p;
    ctx.request_repaint();
}

fn check(sh: Arc<Shared>, dir: PathBuf, ctx: egui::Context) {
    set(&sh, &ctx, Phase::Checking);
    update::cleanup(&dir);
    *sh.installed.lock().unwrap() = update::installed_version(&dir);
    let agent = github::agent();
    let release = match github::latest(&agent) {
        Ok(r) => r,
        Err(e) => {
            let p = if update::game_installed(&dir) { Phase::Offline(e) } else { Phase::Error(e) };
            return set(&sh, &ctx, p);
        }
    };
    *sh.release.lock().unwrap() = Some(release.clone());
    ctx.request_repaint();

    set(&sh, &ctx, Phase::SelfUpdating);
    if let Ok(true) = update::self_update(&agent, &release) {
        sh.restart.store(true, Ordering::Relaxed);
        ctx.request_repaint();
        return;
    }

    let installed = sh.installed.lock().unwrap().clone();
    if installed.as_deref() == Some(release.tag_name.as_str()) && update::game_installed(&dir) {
        return set(&sh, &ctx, Phase::Ready { changed: None });
    }
    let Some(asset) = release.asset(github::GAME_ASSET).cloned() else {
        let e = "в последней сборке нет архива игры".to_string();
        let p = if update::game_installed(&dir) { Phase::Offline(e) } else { Phase::Error(e) };
        return set(&sh, &ctx, p);
    };
    let step = |s: Step| set(&sh, &ctx, Phase::Work(s));
    let uctx = update::Ctx { agent: &agent, dir: &dir, bytes: &sh.bytes, step: &step };
    match update::install(&uctx, &release, &asset) {
        Ok(n) => {
            *sh.installed.lock().unwrap() = Some(release.tag_name.clone());
            set(&sh, &ctx, Phase::Ready { changed: Some(n) });
        }
        Err(e) => set(&sh, &ctx, Phase::Error(e)),
    }
}

struct App {
    th: Theme,
    sh: Arc<Shared>,
    dir: PathBuf,
    launch_error: Option<String>,
    started: Instant,
    /// Для скриншотов: MRPG_LAUNCHER_SCREENSHOT=путь.png, MRPG_LAUNCHER_SCREENSHOT_AFTER=сек
    shot: Option<(PathBuf, f32, bool)>,
    speed: (Instant, u64, f64),
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let sh = Arc::new(Shared {
            phase: Mutex::new(Phase::Checking),
            release: Mutex::new(None),
            installed: Mutex::new(None),
            bytes: Arc::new(Bytes::default()),
            restart: AtomicBool::new(false),
        });
        let dir = game_dir();
        let app = Self {
            th: Theme::load(&cc.egui_ctx),
            sh,
            dir,
            launch_error: None,
            started: Instant::now(),
            shot: std::env::var_os("MRPG_LAUNCHER_SCREENSHOT").map(|p| {
                let after = std::env::var("MRPG_LAUNCHER_SCREENSHOT_AFTER").ok().and_then(|s| s.parse().ok()).unwrap_or(3.0);
                (PathBuf::from(p), after, false)
            }),
            speed: (Instant::now(), 0, 0.0),
        };
        app.spawn_check(&cc.egui_ctx);
        app
    }

    fn spawn_check(&self, ctx: &egui::Context) {
        let (sh, dir, ctx) = (self.sh.clone(), self.dir.clone(), ctx.clone());
        std::thread::spawn(move || check(sh, dir, ctx));
    }

    fn play(&mut self, ctx: &egui::Context) {
        let exe = self.dir.join(update::GAME_EXE);
        match std::process::Command::new(&exe).current_dir(&self.dir).spawn() {
            Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) => self.launch_error = Some(format!("не запустить игру: {e}")),
        }
    }

    fn open_folder(&self) {
        let cmd = if cfg!(windows) { "explorer" } else { "xdg-open" };
        let _ = std::process::Command::new(cmd).arg(&self.dir).spawn();
    }

    fn take_screenshot(&mut self, ctx: &egui::Context) {
        let Some((path, after, sent)) = &mut self.shot else { return };
        if !*sent && self.started.elapsed().as_secs_f32() > *after {
            *sent = true;
            let (path, ctx2) = (path.clone(), ctx.clone());
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                let [w, h] = img.size;
                let raw: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, raw) {
                    let _ = buf.save(&path);
                }
                ctx2.send_viewport_cmd(egui::ViewportCommand::Close);
                ctx2.request_repaint();
            })));
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn build_label(tag: Option<&str>) -> String {
    match tag {
        Some(t) => match github::build_number(t) {
            Some(n) => format!("сборка №{n}"),
            None => t.to_string(),
        },
        None => "—".into(),
    }
}

fn ru_date(iso: &str) -> Option<String> {
    const M: [&str; 12] = [
        "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября", "октября", "ноября", "декабря",
    ];
    let (y, rest) = iso.split_once('-')?;
    let (m, rest) = rest.split_once('-')?;
    let d: u32 = rest.get(..2)?.parse().ok()?;
    let m: usize = m.parse().ok()?;
    Some(format!("{d} {} {y}", M.get(m.checked_sub(1)?)?))
}

fn mb(b: u64) -> String {
    format!("{:.1}", b as f64 / 1_048_576.0)
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        if self.sh.restart.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let phase = self.sh.phase.lock().unwrap().clone();
        let release = self.sh.release.lock().unwrap().clone();
        let installed = self.sh.installed.lock().unwrap().clone();
        if phase.busy() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        self.take_screenshot(ctx);

        let mut pending: Option<u8> = None;
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(root, |ui| {
            let full = ui.max_rect();
            let th = &self.th;
            theme::backdrop(ui.painter(), th, full);

            // Заголовок
            let cx = full.center().x;
            theme::shadow_text(ui.painter(), pos2(cx, full.min.y + 62.0), Align2::CENTER_CENTER, "Херсонес", theme::title(60.0), ink::TITLE);
            theme::shadow_text(ui.painter(), pos2(cx, full.min.y + 106.0), Align2::CENTER_CENTER, "510 год от Рождества Христова", theme::font(19.0), ink::DIM);
            let mut div = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(pos2(cx, full.min.y + 130.0), vec2(520.0, 24.0))));
            theme::divider(&mut div, th, 520.0);

            // Левая доска: что нового
            let left = Rect::from_min_max(pos2(full.min.x + 36.0, full.min.y + 156.0), pos2(full.min.x + 616.0, full.max.y - 40.0));
            theme::nine_slice(ui.painter(), &th.wood, left, 32.0, true, Color32::WHITE);
            let mut l = ui.new_child(egui::UiBuilder::new().max_rect(left.shrink2(vec2(34.0, 28.0))));
            l.label(egui::RichText::new("Что нового").font(theme::title(28.0)).color(ink::TITLE));
            let sub = match &release {
                Some(r) => {
                    let date = r.published_at.as_deref().and_then(ru_date).map(|d| format!(" · {d}")).unwrap_or_default();
                    format!("{}{date}", build_label(Some(&r.tag_name)))
                }
                None if phase == Phase::Checking => "Спрашиваю GitHub…".into(),
                None => "Нет связи с GitHub".into(),
            };
            l.label(egui::RichText::new(sub).font(theme::font(17.0)).color(ink::DIM));
            let w = l.available_width();
            theme::divider(&mut l, th, w);
            l.add_space(4.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut l, |ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                let notes = release.as_ref().and_then(|r| r.body.as_deref()).map(github::plain_notes).unwrap_or_default();
                if notes.is_empty() {
                    ui.label(egui::RichText::new("Описание сборки появится, когда лаунчер свяжется с GitHub.").font(theme::font(18.0)).color(ink::DIM));
                }
                for (kind, line) in notes.iter().take(80) {
                    match kind {
                        2 => {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(line).font(theme::bold(19.0)).color(ink::GOLD));
                        }
                        1 => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(egui::RichText::new("•").font(theme::font(18.0)).color(ink::GOLD));
                                ui.label(egui::RichText::new(line).font(theme::font(17.5)).color(ink::TEXT));
                            });
                        }
                        _ => {
                            ui.label(egui::RichText::new(line).font(theme::font(17.5)).color(ink::TEXT));
                        }
                    }
                }
            });

            // Правая кожа: состояние и кнопка
            let right = Rect::from_min_max(pos2(left.max.x + 22.0, left.min.y), pos2(full.max.x - 36.0, left.max.y));
            theme::nine_slice(ui.painter(), &th.leather, right, 18.0, true, Color32::WHITE);
            let mut r = ui.new_child(egui::UiBuilder::new().max_rect(right.shrink2(vec2(26.0, 22.0))));
            let rw = r.available_width();
            r.label(egui::RichText::new("Игра").font(theme::title(28.0)).color(ink::TITLE));
            theme::divider(&mut r, th, rw);
            r.add_space(4.0);
            let row = |ui: &mut egui::Ui, k: &str, v: String, c: Color32| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(k).font(theme::font(18.0)).color(ink::DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(v).font(theme::bold(18.0)).color(c));
                    });
                });
            };
            let have_game = update::game_installed(&self.dir);
            let inst = if have_game { build_label(installed.as_deref()) } else { "не установлена".into() };
            let inst = if have_game && installed.is_none() { "неизвестно".into() } else { inst };
            row(&mut r, "Установлена", inst, ink::TEXT);
            row(&mut r, "Последняя", build_label(release.as_ref().map(|r| r.tag_name.as_str())), ink::TEXT);
            r.add_space(14.0);

            let done = self.sh.bytes.done.load(Ordering::Relaxed);
            let total = self.sh.bytes.total.load(Ordering::Relaxed);
            // Скорость, сглаженная
            let (t0, b0, sp) = self.speed;
            let dt = t0.elapsed().as_secs_f64();
            if dt > 0.5 {
                let inst_sp = done.saturating_sub(b0) as f64 / dt;
                self.speed = (Instant::now(), done, if sp == 0.0 { inst_sp } else { sp * 0.6 + inst_sp * 0.4 });
            }
            let (status, color) = match &phase {
                Phase::Checking => ("Проверяю, вышла ли новая сборка…".to_string(), ink::TEXT),
                Phase::SelfUpdating => ("Проверяю, свежий ли лаунчер…".to_string(), ink::TEXT),
                Phase::Work(Step::Comparing) => ("Сверяю файлы игры…".to_string(), ink::TEXT),
                Phase::Work(Step::Downloading { files }) => (format!("Загружаю обновление: файлов {files}"), ink::TEXT),
                Phase::Work(Step::Installing) => ("Ставлю обновление…".to_string(), ink::TEXT),
                Phase::Ready { changed: Some(0) } | Phase::Ready { changed: None } => ("Игра обновлена, можно играть".to_string(), ink::GOOD),
                Phase::Ready { changed: Some(n) } => (format!("Обновление установлено (файлов: {n})"), ink::GOOD),
                Phase::Offline(e) => (format!("Не удалось проверить обновления: {e}. Можно играть в установленную версию."), ink::GOLD),
                Phase::Error(e) => (format!("Ошибка: {e}"), ink::BAD),
            };
            r.add(egui::Label::new(egui::RichText::new(status).font(theme::font(18.0)).color(color)).wrap());
            r.add_space(8.0);
            let frac = match &phase {
                Phase::Work(Step::Downloading { .. }) if total > 0 => Some(done as f32 / total as f32),
                Phase::Work(Step::Installing) => Some(1.0),
                p if p.busy() => None,
                Phase::Ready { .. } => Some(1.0),
                _ => Some(0.0),
            };
            theme::bar(&mut r, th, rw, frac, ui.input(|i| i.time));
            let detail = match &phase {
                Phase::Work(Step::Downloading { .. }) if total > 0 => {
                    let sp = self.speed.2;
                    let eta = if sp > 1000.0 { (total.saturating_sub(done)) as f64 / sp } else { 0.0 };
                    let eta = if eta > 0.0 { format!(" · осталось {}:{:02}", eta as u64 / 60, eta as u64 % 60) } else { String::new() };
                    format!("{} из {} МБ · {} МБ/с{eta}", mb(done), mb(total), mb(sp as u64))
                }
                Phase::Ready { changed: Some(n) } if *n > 0 => {
                    let full = release.as_ref().and_then(|r| r.asset(github::GAME_ASSET)).map(|a| a.size).unwrap_or(0);
                    if full > total { format!("Скачано {} из {} МБ — только изменения", mb(total), mb(full)) } else { String::new() }
                }
                _ => String::new(),
            };
            r.label(egui::RichText::new(detail).font(theme::font(16.0)).color(ink::DIM));
            if let Some(e) = &self.launch_error {
                r.add(egui::Label::new(egui::RichText::new(e).font(theme::font(16.0)).color(ink::BAD)).wrap());
            }

            // Кнопки — внизу панели
            let can_play = have_game && !phase.busy();
            let bottom = Rect::from_min_max(pos2(right.min.x + 26.0, right.max.y - 160.0), pos2(right.max.x - 26.0, right.max.y - 24.0));
            let (mut want_check, mut want_folder) = (false, false);
            let mut b = ui.new_child(egui::UiBuilder::new().max_rect(bottom));
            let want_play = theme::button(&mut b, th, "Играть", vec2(rw, 72.0), 30.0, can_play).clicked();
            b.add_space(12.0);
            b.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let half = (rw - 12.0) / 2.0;
                if theme::button(ui, th, "Проверить снова", vec2(half, 44.0), 18.0, !phase.busy()).clicked() {
                    want_check = true;
                }
                if theme::button(ui, th, "Папка игры", vec2(half, 44.0), 18.0, true).clicked() {
                    want_folder = true;
                }
            });
            if want_play { pending = Some(0) } else if want_check { pending = Some(1) } else if want_folder { pending = Some(2) }

            ui.painter().text(
                pos2(cx, full.max.y - 20.0),
                Align2::CENTER_CENTER,
                format!("Лаунчер {} · {}", env!("CARGO_PKG_VERSION"), self.dir.display()),
                theme::font(14.0),
                ink::DIM,
            );
        });
        match pending {
            Some(0) => self.play(ctx),
            Some(1) => self.spawn_check(ctx),
            Some(2) => self.open_folder(),
            _ => {}
        }
    }
}

fn icon() -> egui::IconData {
    // Бронзовый крест из темы на прозрачном квадрате.
    let mid = theme::decode(include_bytes!("../../assets/ui/divider_mid.png"));
    let (w, h) = (mid.size[0], mid.size[1]);
    let s = w.max(h);
    let mut rgba = vec![0u8; s * s * 4];
    let oy = (s - h) / 2;
    for y in 0..h {
        for x in 0..w {
            let c = mid.pixels[y * w + x].to_srgba_unmultiplied();
            let i = ((y + oy) * s + x) * 4;
            rgba[i..i + 4].copy_from_slice(&c);
        }
    }
    egui::IconData { rgba, width: s as u32, height: s as u32 }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Херсонес — лаунчер")
            .with_inner_size([W, H])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_icon(Arc::new(icon())),
        ..Default::default()
    };
    eframe::run_native("Херсонес — лаунчер", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
