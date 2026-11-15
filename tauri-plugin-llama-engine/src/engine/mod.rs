//! Движок llama.cpp — инференс через ОТДЕЛЬНЫЙ процесс `llama-server.exe`
//! (полный релиз llama.cpp), общение по HTTP localhost (random-порт + api-key).
//! Приложение НЕ линкует llama.cpp нативно (см. docs/llama_cpp_engine.md).
//!
//! Ядро переехало в чистый крейт `llama-engine-core` (SSOT, без tauri,
//! `core/` этого плагина). Этот модуль — ФАСАД СОВМЕСТИМОСТИ: ре-экспорт
//! всего API ядра + API-совместимые обёртки, принимающие `AppHandle` —
//! существующие хосты (`engine::load_config(&app)` и т.д.) не меняются.
//! Локальные функции затеняют glob-реэкспорт ядра (правила Rust:
//! явная декларация сильнее glob).

pub use llama_engine_core::engine::*;

// ── Обёртки совместимости: ядро живёт без tauri ──

/// Универсальный модуль конфига: реэкспорт ядра (новый API без tauri —
/// GPUI/standalone) + AppHandle-мосты (старый API Tauri-хостов: King Orch
/// зовёт `engine::config::load_config(&app)`). Явные fn затеняют glob.
pub mod config {
    pub use llama_engine_core::engine::config::*;

    /// Совместимость: старые хосты передают AppHandle, ядро резолвит путь сама.
    pub fn get_config_path(_app: &tauri::AppHandle) -> std::path::PathBuf {
        llama_engine_core::engine::config::get_config_path()
    }

    /// Совместимость: старые хосты передают AppHandle, ядро читает файл напрямую.
    pub fn load_config(_app: &tauri::AppHandle) -> EngineConfig {
        llama_engine_core::engine::config::load_config()
    }

    /// Совместимость: ядро пишет в тот же `app_config.json` (field-preserving merge).
    pub fn save_config(_app: &tauri::AppHandle, config: &EngineConfig) -> Result<(), String> {
        llama_engine_core::engine::config::save_config(config)
    }

    /// Совместимость: каталог встроен в ядро (`include_str!`).
    pub fn load_catalog(_app: &tauri::AppHandle) -> Vec<CatalogEntry> {
        llama_engine_core::engine::config::load_catalog()
    }
}

/// Совместимость: старые хосты передают AppHandle, ядро читает файл напрямую.
pub fn load_config(_app: &tauri::AppHandle) -> llama_engine_core::engine::config::EngineConfig {
    llama_engine_core::engine::config::load_config()
}

/// Совместимость: ядро пишет в тот же `app_config.json` (field-preserving merge).
pub fn save_config(
    _app: &tauri::AppHandle,
    config: &llama_engine_core::engine::config::EngineConfig,
) -> Result<(), String> {
    llama_engine_core::engine::config::save_config(config)
}

/// Совместимость: каталог встроен в ядро (`include_str!`).
pub fn load_catalog(
    _app: &tauri::AppHandle,
) -> Vec<llama_engine_core::engine::config::CatalogEntry> {
    llama_engine_core::engine::config::load_catalog()
}

/// Совместимость: старые хосты передают AppHandle первым аргументом,
/// ядро качает mmproj без него.
pub async fn ensure_mmproj_for_model(
    _app: &tauri::AppHandle,
    model_path: &str,
) -> Result<Option<String>, String> {
    llama_engine_core::engine::mmproj::ensure_mmproj_for_model(model_path).await
}
