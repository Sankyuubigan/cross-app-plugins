//! llama-engine-core — чистое ядро плагина `plugin-llama-engine` (SSOT,
//! PRP 261115_my_gpui_kit.md). Без Tauri: движок llama.cpp (внешний процесс
//! `llama-server.exe` + HTTP localhost), каталог моделей, парметры сэмплинга,
//! докачка mmproj, VRAM-уведомления, установка бекендов.
//!
//! Tauri-связки исходных модулей заменены инъекцией:
//! - `downloader::set_progress_hook` — прогресс скачивания (плагин эмитит
//!   `download_progress`, GPUI-хост — свой колбэк);
//! - скачивание — через единый движок downloader-core (деп ядра, Шаг 4a PRP
//!   261115): цепочка reqwest → curl → process, stall/resume/зеркала HF —
//!   внутри него; GPUI-хосты могут не ставить хуки вовсе;
//! - `vram::subscribe` — подписка на VRAM-уведомления (плагин форвардит их
//!   как событие `vram_notice`);
//! - `analytics::set_track_hook` — аналитика (плагин при feature `host-logs`
//!   ставит `plugin_log_collector::track_event`).
//!
//! Tauri-плагин ре-экспортирует модуль `engine` целиком
//! (`pub use llama_engine_core::engine;`-эквивалент через compat-модуль),
//! поэтому фасад хостов `plugin_llama_engine::engine::*` не меняется.

pub mod engine;
