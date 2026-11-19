//! Единый кастомный `log::Log` (core rules §2.5) — ядро переехало в чистый
//! крейт `log-collector-core` (SSOT, без tauri: `core/` этого плагина). Здесь только
//! Tauri-обвязка: `install(app)` = `install_standalone` + UI-подписчик
//! `logs:message` (живая вкладка «Логи»).

pub use log_collector_core::logger::*;

/// UI-подписчик `logs:message` ставится РОВНО один раз за процесс (контракт
/// «повторные вызовы безвредны»): подписчики теперь список (`push_emit_hook`),
/// повторный push задублировал бы emit для фронта.
static UI_HOOK_PUSHED: std::sync::Once = std::sync::Once::new();

/// Установка логгера и путей в setup плагина (API-совместимость: хосты
/// вызывают `plugin_log_collector::logger::install(&app, ...)`). Повторные
/// вызовы безвредны. Live-строки лога уходят фронту как `logs:message`;
/// подписчик добавляется через `push_emit_hook` — строка может быть нужна
/// нескольким потребителям (см. core/logger.rs `push_emit_hook`), и хост-хук
/// (например, пер-сессионный архив), поставленный раньше setup, затирался.
pub fn install(app: &tauri::AppHandle, last_logs: bool, log_file_name: Option<&str>) {
    // UI-подписчик — ровно один за процесс (контракт «повторные вызовы
    // безвредны»: повторный push задублировал бы emit для фронта).
    UI_HOOK_PUSHED.call_once(|| {
        let handle = app.clone();
        log_collector_core::logger::push_emit_hook(std::sync::Arc::new(move |entry: &str| {
            use tauri::Emitter;
            let _ = handle.emit("logs:message", entry);
        }));
    });
    log_collector_core::logger::install_standalone(last_logs, log_file_name);
}
