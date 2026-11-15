//! Телеметрия ядра.
//!
//! Ядро не знает про plugin-log-collector: хост ставит колбэк-хук
//! ([`set_track_hook`]) — Tauri-плагин при feature `host-logs` подставляет
//! `plugin_log_collector::track_event` (единый стандарт логирования King Orch
//! `core_rules.md` §2.5); без хука события уходят просто в `log::info!`.

use std::sync::{Arc, Mutex, OnceLock};

/// Обработчик аналитики, который ставит хост.
pub type TrackHook = Arc<dyn Fn(&str, Option<serde_json::Value>) + Send + Sync>;

static TRACK_HOOK: OnceLock<Mutex<Option<TrackHook>>> = OnceLock::new();

fn lock<T>(guard: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    guard.lock().unwrap_or_else(|p| p.into_inner())
}

/// Хост ставит обработчик аналитики (например, мост в plugin-log-collector).
/// Повторный вызов заменяет хук; `None` возвращает поведение «только лог».
pub fn set_track_hook(hook: Option<TrackHook>) {
    let cell = TRACK_HOOK.get_or_init(|| Mutex::new(None));
    *lock(cell) = hook;
}

/// Аналитическое событие (например `llm_started` / `llm_finished` / `llm_error`).
pub fn track_event(name: &str, props: Option<serde_json::Value>) {
    let hook = TRACK_HOOK.get().and_then(|cell| lock(cell).clone());
    match hook {
        Some(hook) => hook(name, props),
        None => {
            log::info!("[EVENT] {}", name);
        }
    }
}
