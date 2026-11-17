//! audio-gen-core — чистое ядро плагина генерации звуков и музыки (ССОТ, без tauri).
//!
//! Слои:
//! - [`config`] — настройки плагина (`audio_gen_settings.json` в config_dir хоста).
//! - [`models`] — реестр моделей (`audio_models.json`, вшит в бинарник) + детект установки.
//! - [`engine`] — управление audiocpp_server (спавн, health, HTTP-генерация WAV).
//! - [`wav`] — постобработка: нормализация, луп-кроссфейд, запись WAV (hound).
//! - [`job`] — очередь генераций + прогресс/отмена + event-hook (инъекция, ядро не знает tauri).
//!
//! Процессы движка — через engine-proc-util (PID-реестр + kill-job), загрузки —
//! downloader-core, атомарные JSON-записи — json-store (правило плагинов проекта).

pub mod config;
pub mod engine;
pub mod job;
pub mod models;
pub mod wav;

pub use config::AudioGenSettings;
pub use engine::EngineHandle;
pub use job::{
    cancel, job_info, jobs_snapshot, set_event_hook, submit, AudioGenEvent, AudioGenEventHook,
    GenerateRequest, JobRecord, JobStatus,
};
pub use models::{load_registry, ModelInfo};
