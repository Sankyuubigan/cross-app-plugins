//! Единый кастомный `log::Log` (core rules §2.5) — ядро переехало в чистый
//! крейт `logs-core` (SSOT, без tauri: `core/` этого плагина). Здесь только
//! Tauri-обвязка: `install(app)` = `install_standalone` + emit-хук
//! `logs:message` (живая вкладка «Логи»).

pub use logs_core::logger::*;

/// Установка логгера и путей в setup плагина (API-совместимость: хосты
/// вызывают `tauri_plugin_logs::logger::install(&app, ...)`). Повторные
/// вызовы безвредны. Live-строки лога уходят фронту как `logs:message`
/// через `set_emit_hook` — тот же контракт, что был раньше.
pub fn install(app: &tauri::AppHandle, last_logs: bool, log_file_name: Option<&str>) {
    let handle = app.clone();
    logs_core::logger::set_emit_hook(Some(std::sync::Arc::new(move |entry: &str| {
        use tauri::Emitter;
        let _ = handle.emit("logs:message", entry);
    })));
    logs_core::logger::install_standalone(last_logs, log_file_name);
}
