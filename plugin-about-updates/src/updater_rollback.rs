//! Мост бэкапа: прежняя сигнатура с AppHandle → ядро с `data_dir`.
//! Ядро: `about_updates_core::rollback` (дефолт — `app_config.json`, `sessions/`).

use about_updates_core::rollback as core;
use tauri::{AppHandle, Manager, Runtime};

/// Копирует `app_config.json` и папку `sessions/` в `rollback_backup/` рядом с данными
/// приложения. Гарантирует, что при даунгрейде старая версия сможет прочитать данные,
/// либо пользователь сможет вручную вернуть бэкап, если схема данных новее нечитаема.
pub fn backup_before_rollback<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    core::backup_before_rollback(&data_dir)
}
