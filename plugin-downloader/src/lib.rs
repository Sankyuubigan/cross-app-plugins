//! 📦 plugin-downloader — тонкий Tauri-фасад над `downloader-core` (ядро без Tauri).
//!
//! Вся логика (6 уровней, stall/resume, редиректы GitHub→S3, ETA) живёт в
//! `downloader-core` (`core/`). Здесь: регистрация плагина, команды, мост
//! эмита события `downloader:progress` (прежний формат payload-а) и отмена
//! всех загрузок при выходе хоста.
//!
//! ## Подключение в хосте
//!
//! `Cargo.toml`:
//! ```toml
//! plugin-downloader = { path = "../../cross-app-plugins/plugin-downloader" }
//! ```
//!
//! `main.rs`: `.plugin(plugin_downloader::init())`
//! `capabilities/default.json`: `"downloader:default"`
//!
//! ## Rust API (для других крейтов/плагинов) — прежний корневой путь
//! ```ignore
//! plugin_downloader::download(url, dest, opts, None).await?;
//! plugin_downloader::download_blocking(url, dest, opts, Some(&log_cb))?;
//! ```

pub mod commands;

use serde::Deserialize;
use std::sync::Arc;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::Wry;

/// Корневой реэкспорт ядра: прежние пути вызовов хостов не меняются.
pub use downloader_core::{
    cancel, cancel_all, download, download_blocking, download_bytes_blocking,
    download_bytes_to_vec, list_active, set_progress_hook, DownloadOptions, DownloadProgress,
    ProgressPayload, PROGRESS_EVENT,
};

/// Конфиг плагина (`plugins.downloader` в `tauri.conf.json` хоста).
///
/// Устойчив к отсутствию секции: если ключа `downloader` в `plugins` нет,
/// Tauri передаёт `null`, и наивная десериализация в структуру паникует на
/// старте хоста ("invalid type: null, expected struct Config"). Здесь `null`
/// трактуется как конфиг по умолчанию.
#[derive(Clone, Default)]
pub struct Config {}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Принимаем null (нет секции) или любой объект (поля пока нет).
        let _ = Option::<serde::de::IgnoredAny>::deserialize(deserializer)?;
        Ok(Config::default())
    }
}

/// Инициализация плагина: `.plugin(plugin_downloader::init())`.
pub fn init() -> TauriPlugin<Wry, Config> {
    Builder::<Wry, Config>::new("downloader")
        .setup(|app, _api| {
            // Мост эмита: ядро зовёт хук → фасад эмитит прежнее событие
            // `downloader:progress` (payload — прежний формат ProgressPayload).
            let handle = app.clone();
            let hook: downloader_core::EmitHook = Arc::new(move |payload: &ProgressPayload| {
                use tauri::Emitter;
                let _ = handle.emit(PROGRESS_EVENT, payload.clone());
            });
            downloader_core::set_emit_hook(Some(hook));
            Ok(())
        })
        .on_event(|_app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                downloader_core::cancel_all();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::download_file,
            commands::download_bytes,
            commands::cancel_download,
            commands::list_active,
        ])
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Регресс: Tauri передаёт null, если ключа plugins.downloader нет в конфиге.
    // Наивный #[derive(Deserialize)] падал: "invalid type: null, expected struct Config".
    #[test]
    fn config_accepts_null() {
        let c: Config = serde_json::from_str("null").expect("null → Config::default()");
        let _ = c;
    }

    #[test]
    fn config_accepts_empty_object() {
        let _c: Config = serde_json::from_str("{}").expect("{} → Config::default()");
    }

    #[test]
    fn config_accepts_unknown_fields() {
        let _c: Config = serde_json::from_str(r#"{"future_flag": true}"#)
            .expect("unknown fields ignored");
    }
}
