//! AboutUpdatesCard — карточка «Обновления» кита (фича `about-updates`):
//! проверка/установка обновлений через `about-updates-core` — та же логика,
//! что у Tauri-хостов (Primary latest.json → Fallback GitHub → Graceful Empty,
//! minisign-проверка установщика, отчёт об установке, бэкап перед откатом).
//!
//! Самодостаточность: сетевые вызовы ядра — фоновые std::thread + общий
//! tokio-runtime модуля; результаты пишутся в [`AboutUpdatesState`]
//! (`Arc<Mutex>`), перерисовку поднимает watch-цикл [`watch_updates`]
//! (150 мс, `WeakEntity::update` — задача гаснет вместе с вью хоста).
//!
//! Хост (3 строки):
//! ```ignore
//! struct MyView { updates: AboutUpdatesState, /* … */ }
//! // new():
//! self.updates = AboutUpdatesState::new(cfg);
//! my_gpui_kit::watch_updates(&self.updates, cx, |_this, cx| cx.notify());
//! // render():
//! my_gpui_kit::AboutUpdatesCard::new(self.updates.clone())
//! ```
//!
//! В колбэке `watch_updates` удобно обработать [`UpdatePhase::InstallSpawned`]:
//! хост ДОЛЖЕН закрыть приложение — NSIS `/R` перезапустит его после установки
//! (Tauri-фасад делает то же самое через `app.exit(0)`).
//!
//! Прогресс: глобальный хук `set_progress_hook` (ССОТ downloader-core) ставится
//! на время установки и снимается после; строка — формат «18.4 / 64.0 МБ (29%)».

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use about_updates_core::{
    backup_before_rollback, check_for_update, fetch_release_history, install_report,
    run_install, DownloadProgress, InstallKind, InstallOptions, ReleaseInfo, UpdateConfig,
    set_progress_hook,
};
use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::{
    App, Context, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled, StyleRefinement, Window, div, px,
};

/// Период watch-цикла (паритет drain-цикла VibeBridge).
const WATCH_TICK: Duration = Duration::from_millis(150);

/// Конфиг карточки — хост-специфика, параметры ядра (`data_dir`, `current_version`).
#[derive(Clone, Debug)]
pub struct AboutUpdatesConfig {
    /// GitHub-репозиторий «owner/repo» (Fallback-канал + история релизов).
    pub repo: String,
    /// Версия хоста (сравнение при проверке, previous_version в отчёте).
    pub current_version: String,
    /// Каталог данных приложения (отчёт установки, бэкап, установщик).
    pub data_dir: PathBuf,
    /// URL latest.json (Primary-канал). None — только GitHub.
    pub latest_json_url: Option<String>,
    /// minisign-публичный ключ — проверка подписи установщика (plain или
    /// Base64-обёртка Tauri).
    pub pubkey: Option<String>,
    /// Кнопка «Поддержать автора». None — кнопки нет.
    pub support_url: Option<String>,
}

/// Стадия state-machine карточки (без `Failed` — ошибка живёт в
/// [`UpdatesSnapshot::last_error`], фаза возвращается в `Idle`).
#[derive(Clone, Debug, PartialEq)]
pub enum UpdatePhase {
    /// До первой проверки (или после ошибки — повтор разрешён).
    Idle,
    /// Идёт проверка (клики игнорируются — guard в стейте).
    Checking,
    /// Проверка завершена: обновлений нет.
    UpToDate,
    /// Есть кандидат (Primary или Fallback), готов к установке.
    UpdateAvailable {
        version: String,
        notes: Option<String>,
        pub_date: Option<String>,
        download_url: String,
        signature: Option<String>,
        /// Канал-источник кандидата (для лога; `latest.json`/`github`).
        source: &'static str,
    },
    /// Скачивание/запуск установщика (прогресс — [`UpdatesSnapshot::progress`]).
    Installing { version: String },
    /// Установщик запущен; хост должен закрыть приложение (NSIS /R перезапустит).
    InstallSpawned { version: String },
}

/// Снимок стейта для рендера (дешёвый clone из `Arc<Mutex>`).
#[derive(Clone)]
pub struct UpdatesSnapshot {
    pub phase: UpdatePhase,
    /// Последняя ошибка проверки/установки/истории (показывается в danger-цвете).
    pub last_error: Option<String>,
    /// Прогресс скачивания установщика (None — между фазами).
    pub progress: Option<DownloadProgress>,
    /// История релизов (лениво: кнопка «Обновить» в секции истории).
    pub history: Arc<Vec<ReleaseInfo>>,
    pub history_loading: bool,
    pub current_version: String,
    pub support_url: Option<String>,
    /// Вердикт по прошлой установке (resolve_pending при создании стейта).
    pub report_note: Option<String>,
}

struct Inner {
    cfg: AboutUpdatesConfig,
    phase: UpdatePhase,
    last_error: Option<String>,
    progress: Option<DownloadProgress>,
    history: Vec<ReleaseInfo>,
    history_loading: bool,
    report_note: Option<String>,
    dirty: bool,
}

/// Общий стейт карточки (Clone = тот же стейт). Хост держит ОДИН экземпляр
/// и передаёт копию в [`AboutUpdatesCard`] и [`watch_updates`].
#[derive(Clone)]
pub struct AboutUpdatesState(Arc<Mutex<Inner>>);

impl AboutUpdatesState {
    /// Создать стейт. Сразу вердикт по прошлой установке (паритет фасада:
    /// `resolve_pending` в setup плагина) — тихо, если файла отчёта нет.
    pub fn new(cfg: AboutUpdatesConfig) -> Self {
        let report_note =
            install_report::resolve_pending(&cfg.data_dir, &cfg.current_version).map(|r| {
                format!(
                    "прошлое обновление до {}: {}",
                    r.target_version,
                    match r.state {
                        install_report::InstallState::Done => "успешно",
                        install_report::InstallState::Pending => "не подтверждено",
                        install_report::InstallState::Failed => "не удалось",
                    }
                )
            });
        let inner = Inner {
            cfg,
            phase: UpdatePhase::Idle,
            last_error: None,
            progress: None,
            history: Vec::new(),
            history_loading: false,
            report_note,
            dirty: false,
        };
        Self(Arc::new(Mutex::new(inner)))
    }

    /// Снимок для рендера.
    pub fn snapshot(&self) -> UpdatesSnapshot {
        let g = lock(&self.0);
        UpdatesSnapshot {
            phase: g.phase.clone(),
            last_error: g.last_error.clone(),
            progress: g.progress.clone(),
            history: Arc::new(g.history.clone()),
            history_loading: g.history_loading,
            current_version: g.cfg.current_version.clone(),
            support_url: g.cfg.support_url.clone(),
            report_note: g.report_note.clone(),
        }
    }

    /// Забрать dirty-флаг (вызывается watch-циклом: true = было изменение).
    pub fn take_dirty(&self) -> bool {
        let mut g = lock(&self.0);
        std::mem::take(&mut g.dirty)
    }

    /// Проверить обновления (фоново). Повтор во время проверки/установки
    /// игнорируется с предупреждением (guard `action_allowed`).
    pub fn check(&self) {
        let (cfg, allowed) = {
            let g = lock(&self.0);
            (g.cfg.clone(), action_allowed(&g.phase, Action::Check))
        };
        if !allowed {
            log::warn!("[about-updates] идёт проверка/установка — повторная проверка проигнорирована");
            return;
        }
        {
            let mut g = lock(&self.0);
            g.phase = UpdatePhase::Checking;
            g.last_error = None;
            g.dirty = true;
        }
        let inner = self.0.clone();
        let spawn_res = std::thread::Builder::new()
            .name("about-updates-check".into())
            .spawn(move || {
                let ucfg = UpdateConfig {
                    repo: cfg.repo.clone(),
                    latest_json_url: cfg.latest_json_url.clone(),
                    pubkey: cfg.pubkey.clone(),
                };
                let result = runtime()
                    .map(|rt| rt.block_on(check_for_update(&ucfg, &cfg.current_version)))
                    .unwrap_or_else(|| Err("tokio-runtime недоступен (проверка не выполнена)".into()));
                let mut g = lock(&inner);
                g.dirty = true;
                match result {
                    Ok(Some(cand)) => {
                        log::info!("[about-updates] доступно обновление {} ({})", cand.version, cand.source);
                        g.phase = UpdatePhase::UpdateAvailable {
                            version: cand.version,
                            notes: cand.notes,
                            pub_date: cand.pub_date,
                            download_url: cand.download_url,
                            signature: cand.signature,
                            source: cand.source,
                        };
                    }
                    Ok(None) => {
                        log::info!("[about-updates] обновлений нет (авторитетный ответ каналов)");
                        g.phase = UpdatePhase::UpToDate;
                    }
                    Err(e) => {
                        log::error!("[about-updates] проверка не удалась: {e}");
                        g.phase = UpdatePhase::Idle;
                        g.last_error = Some(e);
                    }
                }
            });
        if let Err(e) = spawn_res {
            log::error!("[about-updates] поток проверки не запущен: {e}");
        }
    }

    /// Установить релиз: `Update` — «Скачать и установить» по кандидату;
    /// `Rollback` — откат из истории (с бэкапом данных, паритет команды
    /// install_release фасада). Подпись кандидата + pubkey конфига — в ядро.
    pub fn install(&self, kind: InstallKind, version: &str, download_url: &str) {
        let (cfg, allowed, signature) = {
            let g = lock(&self.0);
            let sig = match &g.phase {
                UpdatePhase::UpdateAvailable { signature, .. } => signature.clone(),
                _ => None,
            };
            (g.cfg.clone(), action_allowed(&g.phase, Action::Install), sig)
        };
        if !allowed {
            log::warn!("[about-updates] идёт проверка/установка — повторная установка проигнорирована");
            return;
        }
        {
            let mut g = lock(&self.0);
            g.phase = UpdatePhase::Installing { version: version.to_string() };
            g.last_error = None;
            g.progress = None;
            g.dirty = true;
        }
        let inner = self.0.clone();
        let version = version.to_string();
        let url = download_url.to_string();
        let spawn_res = std::thread::Builder::new()
            .name("about-updates-install".into())
            .spawn(move || {
                install_in_thread(&inner, &cfg, kind, &version, &url, signature.as_deref());
            });
        if let Err(e) = spawn_res {
            log::error!("[about-updates] поток установки не запущен: {e}");
        }
    }

    /// «Скачать и установить» по текущему кандидату (кнопка карточки).
    pub fn install_candidate(&self) {
        let cand = {
            let g = lock(&self.0);
            match &g.phase {
                UpdatePhase::UpdateAvailable { version, download_url, .. } => {
                    Some((version.clone(), download_url.clone()))
                }
                _ => None,
            }
        };
        if let Some((version, url)) = cand {
            self.install(InstallKind::Update, &version, &url);
        }
    }

    /// Перечитать историю релизов (лениво, кнопка секции истории).
    pub fn refresh_history(&self) {
        let cfg = {
            let mut g = lock(&self.0);
            if g.history_loading {
                log::warn!("[about-updates] история уже загружается — повтор проигнорирован");
                return;
            }
            g.history_loading = true;
            g.dirty = true;
            g.cfg.clone()
        };
        let inner = self.0.clone();
        let spawn_res = std::thread::Builder::new()
            .name("about-updates-history".into())
            .spawn(move || {
                let result = runtime()
                    .map(|rt| rt.block_on(fetch_release_history(&cfg.repo, &cfg.current_version)))
                    .unwrap_or_else(|| Err("tokio-runtime недоступен (история не получена)".into()));
                let mut g = lock(&inner);
                g.history_loading = false;
                g.dirty = true;
                match result {
                    Ok(list) => {
                        log::info!("[about-updates] история релизов: {} шт.", list.len());
                        g.history = list;
                    }
                    Err(e) => {
                        log::error!("[about-updates] история релизов не получена: {e}");
                        g.last_error = Some(e);
                    }
                }
            });
        if let Err(e) = spawn_res {
            log::error!("[about-updates] поток истории не запущен: {e}");
        }
    }
}

/// Тело потока установки: бэкап (откат) → глобальный хук прогресса →
/// `run_install` ядра (скачивание + minisign + отчёт + NSIS) → вердикт в стейт.
fn install_in_thread(
    inner: &Arc<Mutex<Inner>>,
    cfg: &AboutUpdatesConfig,
    kind: InstallKind,
    version: &str,
    url: &str,
    signature: Option<&str>,
) {
    if kind == InstallKind::Rollback {
        if let Err(e) = backup_before_rollback(&cfg.data_dir) {
            log::error!("[about-updates] бэкап перед откатом не удался: {e}");
            let mut g = lock(inner);
            g.dirty = true;
            g.phase = UpdatePhase::Idle;
            g.last_error = Some(format!("бэкап перед откатом: {e}"));
            return;
        }
    }
    // Глобальный хук прогресса (ССОТ downloader-core): пишем в стейт карточки.
    let hook_state = inner.clone();
    set_progress_hook(Some(Arc::new(move |p: &DownloadProgress| {
        let mut g = lock(&hook_state);
        g.progress = Some(p.clone());
        g.dirty = true;
    })));
    let opts = InstallOptions {
        data_dir: &cfg.data_dir,
        previous_version: &cfg.current_version,
        signature,
        pubkey: cfg.pubkey.as_deref(),
    };
    let result = match runtime() {
        Some(rt) => rt.block_on(run_install(kind, version, url, &opts)),
        None => Err("tokio-runtime недоступен (установка не выполнена)".into()),
    };
    set_progress_hook(None);
    let mut g = lock(inner);
    g.progress = None;
    g.dirty = true;
    match result {
        Ok(()) => {
            log::info!("[about-updates] установщик {version} запущен; хост закрывает приложение (NSIS /R перезапустит)");
            g.phase = UpdatePhase::InstallSpawned { version: version.to_string() };
        }
        Err(e) => {
            log::error!("[about-updates] установка {version} не удалась: {e}");
            g.phase = UpdatePhase::Idle;
            g.last_error = Some(e);
        }
    }
}

/// Watch-цикл карточки: каждые 150 мс забирает dirty-флаг стейта и при
/// изменениях вызывает `on_change` в контексте entity хоста (обычно
/// `|_this, cx| cx.notify()`; здесь же обрабатывать `InstallSpawned` —
/// закрывать приложение). Задача гаснет вместе с entity хоста; detach
/// внутри — хранить результат не нужно.
pub fn watch_updates<T: 'static>(
    state: &AboutUpdatesState,
    cx: &mut Context<T>,
    on_change: impl Fn(&mut T, &mut Context<T>) + 'static,
) {
    let state = state.clone();
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(WATCH_TICK).await;
            if !state.take_dirty() {
                continue;
            }
            if this.update(cx, |this, cx| on_change(this, cx)).is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Карточка (RenderOnce, как LogsPanel/LlamaSettingsCard): рендер из
/// [`AboutUpdatesState::snapshot`]; сам стейт — у хоста.
#[derive(IntoElement)]
pub struct AboutUpdatesCard {
    state: AboutUpdatesState,
    style: StyleRefinement,
}

impl AboutUpdatesCard {
    pub fn new(state: AboutUpdatesState) -> Self {
        Self {
            state,
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for AboutUpdatesCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for AboutUpdatesCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let snap = self.state.snapshot();

        let mut col = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .bg(cx.theme().secondary)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(6.0))
            .overflow_hidden()
            .refine_style(&self.style);

        // Шапка: заголовок + «Поддержать автора» (если задан support_url).
        let mut header = h_flex().gap_2().child(div().flex_1().child("Обновления"));
        if let Some(url) = snap.support_url.clone() {
            header = header.child(
                Button::new("au_support")
                    .label("Поддержать автора")
                    .ghost()
                    .on_click(move |_, _, _| open_url_in_browser(&url)),
            );
        }
        col = col.child(header);

        if let Some(err) = snap.last_error.clone() {
            col = col.child(
                div()
                    .text_size(px(12.0))
                    .text_color(cx.theme().danger)
                    .child(SharedString::from(err)),
            );
        }
        if let Some(note) = snap.report_note.clone() {
            col = col.child(
                div()
                    .text_size(px(12.0))
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(note)),
            );
        }
        col = col.child(div().child(SharedString::from(status_text(&snap))));

        // Проверка/кандидат: кнопка проверки всегда, при кандидате — установка.
        let state_for_check = self.state.clone();
        let check_btn = Button::new("au_check")
            .label("Проверить обновления")
            .on_click(move |_, _, _| state_for_check.check());
        match &snap.phase {
            UpdatePhase::UpdateAvailable { version, notes, .. } => {
                let state_for_install = self.state.clone();
                let install_btn = Button::new("au_install")
                    .label(format!("Скачать и установить {version}"))
                    .primary()
                    .on_click(move |_, _, _| state_for_install.install_candidate());
                col = col.child(h_flex().gap_2().child(check_btn).child(install_btn));
                if let Some(n) = notes {
                    col = col.child(
                        div()
                            .id("au_notes")
                            .max_h(px(96.0))
                            .p_2()
                            .bg(cx.theme().input)
                            .rounded(px(6.0))
                            .text_size(px(12.0))
                            .text_color(cx.theme().muted_foreground)
                            .overflow_y_scroll()
                            .child(SharedString::from(n.clone())),
                    );
                }
            }
            _ => col = col.child(check_btn),
        }

        // Прогресс установки: «18.4 / 64.0 МБ (29%)» + полоса.
        if let UpdatePhase::Installing { version } = &snap.phase {
            let line = snap
                .progress
                .as_ref()
                .map(progress_line)
                .unwrap_or_else(|| "подготовка…".into());
            let frac = match snap.progress.as_ref() {
                Some(p) => p
                    .total_bytes
                    .map(|t| (p.downloaded_bytes as f32 / t as f32).clamp(0.0, 1.0))
                    .unwrap_or(0.05),
                None => 0.05,
            };
            col = col.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(format!("Установка {version}: {line}"))),
                    )
                    .child(
                        div()
                            .flex()
                            .h(px(6.0))
                            .w_full()
                            .bg(cx.theme().input)
                            .rounded(px(3.0))
                            .overflow_hidden()
                            .child(div().h_full().flex_grow(frac).bg(cx.theme().accent)),
                    ),
            );
        }

        col = col.child(history_section(&snap, &self.state, cx));
        col
    }
}

/// Секция истории релизов: заголовок + «Обновить» (ленивая загрузка) + строки
/// с кнопкой «Откатить» (is_current — без кнопки).
fn history_section(snap: &UpdatesSnapshot, state: &AboutUpdatesState, cx: &App) -> impl IntoElement {
    let state_for_hist = state.clone();
    let section = div().flex().flex_col().gap_1().child(
        h_flex()
            .gap_2()
            .child(div().flex_1().child("История релизов"))
            .child(
                Button::new("au_history")
                    .label(if snap.history_loading { "Загрузка…" } else { "Обновить" })
                    .ghost()
                    .on_click(move |_, _, _| state_for_hist.refresh_history()),
            ),
    );
    if snap.history.is_empty() && !snap.history_loading {
        return section.child(
            div()
                .text_size(px(12.0))
                .text_color(cx.theme().muted_foreground)
                .child("Нажмите «Обновить», чтобы получить список релизов."),
        );
    }
    let mut rows = div()
        .id("au_history_rows")
        .flex()
        .flex_col()
        .gap_1()
        .max_h(px(220.0))
        .overflow_y_scroll();
    for (i, rel) in snap.history.iter().enumerate() {
        let state_for_row = state.clone();
        let version = rel.version.clone();
        let url = rel.download_url.clone();
        let mut row = h_flex().justify_between().child(
            h_flex()
                .gap_2()
                .child(div().child(SharedString::from(rel.version.clone())))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(date_short(rel.pub_date.as_deref()))),
                ),
        );
        if rel.is_current {
            row = row.child(
                div()
                    .text_size(px(12.0))
                    .text_color(cx.theme().muted_foreground)
                    .child("текущая"),
            );
        } else {
            row = row.child(
                Button::new(SharedString::from(format!("au_rollback_{i}")))
                    .label("Откатить")
                    .on_click(move |_, _, _| {
                        state_for_row.install(InstallKind::Rollback, &version, &url)
                    }),
            );
        }
        rows = rows.child(row);
    }
    section.child(rows)
}

/// Текст статуса по фазе.
fn status_text(snap: &UpdatesSnapshot) -> String {
    match &snap.phase {
        UpdatePhase::Idle => format!("Текущая версия: {}", snap.current_version),
        UpdatePhase::Checking => "Проверка обновлений…".into(),
        UpdatePhase::UpToDate => format!("У вас последняя версия ({})", snap.current_version),
        UpdatePhase::UpdateAvailable { version, .. } => {
            format!("Доступна новая версия: {version} (сейчас {})", snap.current_version)
        }
        UpdatePhase::Installing { version } => format!("Установка {version}…"),
        UpdatePhase::InstallSpawned { version } => format!(
            "Установщик {version} запущен — приложение закроется и перезапустится автоматически."
        ),
    }
}

/// «18.4 / 64.0 МБ (29%)» (единица одна на пару, как в PRP); при разных
/// единицах — «512 Б / 64.0 МБ (1%)»; без известного размера — «18.4 МБ».
pub fn progress_line(p: &DownloadProgress) -> String {
    match p.total_bytes.filter(|t| *t > 0) {
        Some(total) => {
            let (dval, dunit) = bytes_value_unit(p.downloaded_bytes);
            let (tval, tunit) = bytes_value_unit(total);
            let pct = ((p.downloaded_bytes as f64 / total as f64) * 100.0).round() as u64;
            if dunit == tunit {
                format!("{dval} / {tval} {tunit} ({pct}%)")
            } else {
                format!("{dval} {dunit} / {tval} {tunit} ({pct}%)")
            }
        }
        None => format_bytes(p.downloaded_bytes),
    }
}

/// 512 Б / 18.4 КБ / 18.4 МБ / 3.0 ГБ.
pub fn format_bytes(v: u64) -> String {
    let (val, unit) = bytes_value_unit(v);
    format!("{val} {unit}")
}

/// Значение и единица размера по отдельности (для пары «done / total»).
fn bytes_value_unit(v: u64) -> (String, &'static str) {
    const UNIT: f64 = 1024.0;
    let v = v as f64;
    if v < UNIT {
        (format!("{}", v as u64), "Б")
    } else if v < UNIT * UNIT {
        (format!("{:.1}", v / UNIT), "КБ")
    } else if v < UNIT * UNIT * UNIT {
        (format!("{:.1}", v / (UNIT * UNIT)), "МБ")
    } else {
        (format!("{:.1}", v / (UNIT * UNIT * UNIT)), "ГБ")
    }
}

/// ISO-дату GitHub — до дня («2026-11-15T18:00:00Z» → «2026-11-15»).
fn date_short(iso: Option<&str>) -> String {
    iso.map(|d| d.chars().take(10).collect()).unwrap_or_default()
}

/// Открыть URL в браузере по умолчанию (Windows: `cmd /C start`, прочие:
/// `xdg-open`); CREATE_NO_WINDOW — правило владельца для Command.
fn open_url_in_browser(url: &str) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        use std::os::windows::process::CommandExt;
        let res = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        match res {
            Ok(_) => log::info!("[about-updates] открыт в браузере: {url}"),
            Err(e) => log::error!("[about-updates] не удалось открыть {url}: {e}"),
        }
    }
    #[cfg(not(windows))]
    {
        match std::process::Command::new("xdg-open").arg(url).spawn() {
            Ok(_) => log::info!("[about-updates] открыт в браузере: {url}"),
            Err(e) => log::error!("[about-updates] не удалось открыть {url}: {e}"),
        }
    }
}

/// Долгоживущий tokio-runtime для сетевых вызовов ядра (reqwest требует
/// реактор). Один на процесс; None — создание не удалось (ошибка в лог).
fn runtime() -> Option<&'static tokio::runtime::Runtime> {
    static RT: OnceLock<Option<tokio::runtime::Runtime>> = OnceLock::new();
    RT.get_or_init(|| match tokio::runtime::Runtime::new() {
        Ok(rt) => Some(rt),
        Err(e) => {
            log::error!("[about-updates] tokio runtime не создан: {e}");
            None
        }
    })
    .as_ref()
}

/// Poison-устойчивый lock (падение колбэка не ломает стейт навсегда).
fn lock<T>(cell: &Mutex<T>) -> MutexGuard<'_, T> {
    cell.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Действия стейта (для guard-логики).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    Check,
    Install,
}

/// Guard: во время проверки/установки/после запуска установщика повторные
/// действия запрещены; из любой спокойной фазы (в т.ч. откат из истории при
/// `UpToDate`) — разрешены.
fn action_allowed(phase: &UpdatePhase, action: Action) -> bool {
    let busy = matches!(
        phase,
        UpdatePhase::Checking | UpdatePhase::Installing { .. } | UpdatePhase::InstallSpawned { .. }
    );
    match action {
        Action::Check | Action::Install => !busy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cfg() -> AboutUpdatesConfig {
        AboutUpdatesConfig {
            repo: String::new(),
            current_version: "26.11.0".into(),
            data_dir: PathBuf::from("target/tmp-about-updates-tests"),
            latest_json_url: None,
            pubkey: None,
            support_url: None,
        }
    }

    #[test]
    fn format_bytes_units() {
        assert_eq!(format_bytes(512), "512 Б");
        assert_eq!(format_bytes(18 * 1024 * 1024 + 450_000), "18.4 МБ");
        assert_eq!(format_bytes(64 * 1024 * 1024), "64.0 МБ");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 ГБ");
    }

    /// Пример из PRP: «18.4 / 64.0 МБ (29%)».
    #[test]
    fn progress_line_with_total() {
        let p = DownloadProgress {
            downloaded_bytes: 18 * 1024 * 1024 + 450_000,
            total_bytes: Some(64 * 1024 * 1024),
            speed_bps: 0.0,
        };
        assert_eq!(progress_line(&p), "18.4 / 64.0 МБ (29%)");
    }

    #[test]
    fn progress_line_without_total() {
        let p = DownloadProgress {
            downloaded_bytes: 18 * 1024 * 1024 + 450_000,
            total_bytes: None,
            speed_bps: 0.0,
        };
        assert_eq!(progress_line(&p), "18.4 МБ");
    }

    /// Разные единицы у пары: «512 Б / 64.0 МБ (0%)» (0.0008% → round 0).
    #[test]
    fn progress_line_mixed_units() {
        let p = DownloadProgress {
            downloaded_bytes: 512,
            total_bytes: Some(64 * 1024 * 1024),
            speed_bps: 0.0,
        };
        assert_eq!(progress_line(&p), "512 Б / 64.0 МБ (0%)");
    }

    #[test]
    fn guards_block_busy_phase() {
        assert!(action_allowed(&UpdatePhase::Idle, Action::Check));
        assert!(action_allowed(&UpdatePhase::UpToDate, Action::Install));
        assert!(!action_allowed(&UpdatePhase::Checking, Action::Check));
        assert!(!action_allowed(
            &UpdatePhase::Installing { version: "1.2.3".into() },
            Action::Install
        ));
        assert!(!action_allowed(
            &UpdatePhase::InstallSpawned { version: "1.2.3".into() },
            Action::Check
        ));
    }

    /// Дефолты снапшота + dirty-флаг (resolve_pending по пустому каталогу — None).
    #[test]
    fn snapshot_defaults_and_dirty_take() {
        let state = AboutUpdatesState::new(test_cfg());
        let snap = state.snapshot();
        assert_eq!(snap.phase, UpdatePhase::Idle);
        assert!(snap.history.is_empty());
        assert!(snap.last_error.is_none());
        assert!(!state.take_dirty());
    }

    /// ISO-дата обрезается до дня; None — пустая строка.
    #[test]
    fn date_short_truncates_iso() {
        assert_eq!(date_short(Some("2026-11-15T18:00:00Z")), "2026-11-15");
        assert_eq!(date_short(None), "");
    }
}
