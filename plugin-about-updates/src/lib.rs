//! 📦 plugin-about-updates — тонкий Tauri-фасад над `about-updates-core` (ядро без Tauri).
//!
//! Вся логика (история релизов, проверка обновлений latest.json→GitHub,
//! скачивание/запуск NSIS-инсталлера, отчёт об установке, бэкап) живёт в
//! `about-updates-core` (`core/`). Здесь: регистрация плагина, команды и
//! AppHandle-мосты — ОБЕ сигнатуры: с AppHandle для Tauri-хостов (этот модуль),
//! чистые с `data_dir: &Path` для GPUI-хостов (ядро).
//!
//! ## Подключение в хосте
//!
//! `Cargo.toml`:
//! ```toml
//! plugin-about-updates = { path = "../../cross-app-plugins/plugin-about-updates" }
//! ```
//!
//! `main.rs`: `.plugin(plugin_about_updates::init())`
//! `tauri.conf.json`:
//! ```json
//! "plugins": { "about-updates": { "repo": "owner/repo" } }
//! ```
//! Опционально: `"pubkey"` — minisign-публичный ключ (plain minisign-текст или
//! Base64-обёртка Tauri); задан → Fallback-установка проверяет подпись скачанного
//! установщика. `"support_url"` — кнопка «Поддержать автора».
//!
//! ## Rust API (для других крейтов/плагинов) — прежние пути
//! ```ignore
//! plugin_about_updates::backup_before_rollback(&app)?;
//! plugin_about_updates::installer::run_install(
//!     &app, plugin_about_updates::install_report::InstallKind::Update, "1.2.3", &url,
//! ).await?; // после успешного запуска инсталлера приложение закрывается само
//! ```

pub mod commands;
pub mod install_report;
pub mod installer;
pub mod models;
mod updater_rollback;

pub use models::ReleaseInfo;
/// Бэкап данных перед понижением/обновлением версии — общий для хоста и плагина
/// (SSOT: хост не держит копию этого кода у себя).
pub use updater_rollback::backup_before_rollback;

use serde::Deserialize;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

/// Конфиг плагина. Задаётся в `tauri.conf.json` хоста:
/// ```json
/// "plugins": {
///   "about-updates": { "repo": "ВАШ_USER/ВАШ_РЕПО" }
/// }
/// ```
#[derive(Deserialize, Clone)]
pub struct Config {
    /// GitHub-репозиторий в формате "owner/repo".
    pub repo: String,
    /// Опциональная ссылка «Поддержать автора» (открывается в браузере).
    /// Если не задана — кнопка не показывается.
    #[serde(default)]
    pub support_url: Option<String>,
    /// Опциональный minisign-публичный ключ (plain minisign-текст «untrusted
    /// comment: …» или Base64-обёртка Tauri). Задан → Fallback-установка
    /// проверяет подпись скачанного установщика перед запуском.
    #[serde(default)]
    pub pubkey: Option<String>,
}

/// Состояние плагина, доступное командам во время выполнения.
pub(crate) struct PluginState {
    pub repo: String,
    pub support_url: Option<String>,
    pub pubkey: Option<String>,
}

/// Инициализация плагина. Вызывается из хоста: `.plugin(plugin_about_updates::init())`.
pub fn init<R: Runtime>() -> TauriPlugin<R, Config> {
    Builder::<R, Config>::new("about-updates")
        .setup(|app, api| {
            let cfg = api.config().clone();
            app.manage(PluginState {
                repo: cfg.repo,
                support_url: cfg.support_url,
                pubkey: cfg.pubkey,
            });

            // Вердикт по прошлой установке («откат состоялся / не состоялся»).
            // Именно в фоновой задаче, а не прямо здесь: глобальный логгер хоста
            // поднимается в setup'е плагина логов, который регистрируется позже,
            // и сообщение из setup этого плагина ушло бы в никуда.
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                install_report::resolve_pending(&handle);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_release_history,
            commands::install_release,
            commands::get_install_report,
            commands::get_app_version,
            commands::get_support_url
        ])
        .build()
}
