//! ⚙️ plugin-llama-engine — переиспользуемый движок llama.cpp.
//!
//! Движок инференса — ОТДЕЛЬНЫЙ процесс `llama-server.exe` (полный релиз
//! llama.cpp, папка `<exe>/llamacpp`), общение по HTTP (localhost, random-порт +
//! api-key). Приложение НЕ линкует llama.cpp нативно.
//!
//! Плагин владеет: установкой/обновлением выбранного варианта бекенда
//! (cuda / vulkan / cpu / hip-radeon), каталогом `models_catalog.json`
//! (embedded `include_str!` + опциональные внешние копии), списком моделей,
//! движковыми параметрами (те, что выставляет юзер в <llama-models-panel>),
//! авто-подбором mmproj и VRAM-уведомлениями.
//!
//! ## Подключение в хосте
//!
//! `Cargo.toml`:
//! ```toml
//! plugin-llama-engine = { path = "../../cross-app-plugins/plugin-llama-engine" }
//! ```
//!
//! `main.rs`:
//! ```ignore
//! fn main() {
//!     plugin_llama_engine::engine::config::set_app_data_dir_name("com.kingorch.app");
//!     tauri::Builder::default()
//!         .plugin(plugin_llama_engine::init())
//!         // ...
//!         .run(tauri::generate_context!())
//! }
//! ```
//!
//! Фасад хоста (`src/infra/mod.rs`):
//! ```ignore
//! pub use plugin_llama_engine::engine::*;
//! ```
//!
//! `tauri.conf.json` (опционально, альтернатива `set_app_data_dir_name`):
//! ```json
//! "plugins": { "llama-engine": { "data_dir_name": "com.kingorch.app" } }
//! ```
//!
//! `capabilities/default.json`: добавить `"llama-engine:default"`.
//!
//! Синхронизация списков моделей с хостовым UI — CustomEvent на document:
//! `llama:models-changed` (payload — конфиг движка), хост перечитывает
//! `get_config()`.

pub mod commands;
pub mod engine;
pub mod generate;

use serde::Deserialize;
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{RunEvent, Wry};

/// Конфиг плагина (`plugins.llama-engine` в `tauri.conf.json` хоста).
///
/// Устойчив к отсутствию секции: если ключа `llama-engine` в `plugins` нет,
/// Tauri передаёт `null`, и наивная десериализация в структуру паникует на
/// старте хоста ("invalid type: null, expected struct Config"). Здесь `null`
/// трактуется как конфиг по умолчанию (как у plugin-cloud-routers).
#[derive(Clone, Default)]
pub struct Config {
    /// Имя папки app-data (например `com.kingorch.app`); применяется до первой
    /// блокировки файла конфига. Пусто — остаётся значение хоста из
    /// `set_app_data_dir_name` (или fallback `com.kingorch.app`).
    pub data_dir_name: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ConfigInner {
    data_dir_name: Option<String>,
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Option::<ConfigInner>::deserialize(deserializer)?
            .map(|c| Config { data_dir_name: c.data_dir_name })
            .unwrap_or_default())
    }
}

/// Инициализация плагина: `.plugin(plugin_llama_engine::init())`.
pub fn init() -> TauriPlugin<Wry, Config> {
    Builder::<Wry, Config>::new("llama-engine")
        .setup(|app, api| {
            if let Some(name) = &api.config().data_dir_name {
                if !name.is_empty() {
                    engine::config::set_app_data_dir_name(name);
                }
            }
            install_host_bridges(app);
            Ok(())
        })
        .on_event(|_app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                // Гарантированное убийство движков на выходе (Job Object + плановый килл).
                engine::process_util::kill_active_engines();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_engine_status,
            commands::install_llamacpp,
            commands::set_engine_variant,
            commands::list_engine_sources,
            commands::set_engine_source,
            commands::check_engine_update,
            commands::install_engine_update,
            commands::remove_engine,
            commands::set_engine_dir,
            commands::get_models_catalog,
            commands::get_engine_config,
            commands::get_auto_download_info,
            commands::auto_download_default_model,
            commands::add_model,
            commands::remove_model,
            commands::delete_model_file,
            commands::get_mmproj_path,
            commands::ensure_mmproj,
            commands::get_model_capabilities,
            commands::get_all_capabilities,
            commands::get_model_context_size,
            commands::estimate_prompt_memory,
            commands::get_model_params,
            commands::set_model_params,
            commands::reset_model_params,
            commands::download_model,
            generate::generate_text,
        ])
        .build()
}

/// Мосты ядра `llama-engine-core` в Tauri-хост (вызывается из setup плагина):
/// - прогресс скачивания → событие `download_progress` (тот же payload, что раньше);
/// - VRAM-уведомления → событие `vram_notice`.
///
/// Фоллбэк-мост удалён (Шаг 4a PRP 261115_universal_plugins): единый движок
/// скачивания теперь ВНУТРИ llama-engine-core (деп downloader-core) — цепочка
/// reqwest → curl → process, stall/resume/зеркала HF живут в downloader-core,
/// адаптер хоста больше не нужен.
fn install_host_bridges(app: &tauri::AppHandle) {
    use std::sync::Arc;

    // Прогресс скачивания: единый глобальный хук downloader-core (ядро зовёт его
    // из потоков скачивания) → событие `download_progress`. Слоты set_emit_hook
    // (событие downloader:progress плагина downloader) и set_progress_hook (этот)
    // — РАЗНЫЕ слоты, конфликтов нет. Конвертер типов downloader_core::
    // DownloadProgress → llama-шейп {downloaded, total, speed_bps} (форма payload'а
    // для KO-фронта, chat.ts:815).
    let handle = app.clone();
    engine::downloader::set_progress_hook(Some(Arc::new(
        move |p: &engine::downloader::DownloadProgress| {
            use tauri::Emitter;
            let payload = serde_json::json!({
                "downloaded": p.downloaded_bytes,
                "total": p.total_bytes.unwrap_or(0),
                "speed_bps": p.speed_bps,
            });
            let _ = handle.emit("download_progress", payload);
        },
    )));

    // VRAM-уведомления (одна кнопка «ОК» на фронте).
    let handle = app.clone();
    engine::vram::subscribe(Arc::new(move |notice: &engine::vram::VramNotice| {
        use tauri::Emitter;
        let _ = handle.emit("vram_notice", notice.note.clone());
    }));
}

/// Хелпер для хоста (setup): папка движка `<exe>/llamacpp` (или из конфига).
pub fn get_engine_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    commands::get_engine_dir(app)
}