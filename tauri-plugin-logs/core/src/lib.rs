//! logs-core — чистое ядро плагина `tauri-plugin-logs` (SSOT, PRP
//! 261115_my_gpui_kit.md). Без Tauri: единый `log::Log`, файл лога рядом с
//! exe, dev-зеркало `test/last_logs.txt`, flight-recorder, отправка
//! ошибок/аналитики (Aptabase).
//!
//! Единственная tauri-связка исходного `logger.rs` — emit события
//! `logs:message` в UI через `AppHandle`. Здесь она заменена инъекцией:
//! хост ставит [`logger::set_emit_hook`] (Tauri-плагин — emit, GPUI-хост —
//! прямой колбэк в панель логов или None).
//!
//! Tauri-плагин ре-экспортирует эти модули, поэтому прежний путь
//! `tauri_plugin_logs::logger::read_tail` (и весь остальной API) не меняется.

pub mod error_reporting;
pub mod flight;
pub mod logger;
pub mod path;
