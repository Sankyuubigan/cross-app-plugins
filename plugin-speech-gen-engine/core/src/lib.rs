//! Чистое ядро speech-плагина (ССОТ, Шаг 4e PRP 261115): движки TTS/STT/диалога
//! (prebuilt-процессы CrispASR/speech.cpp + HTTP/WS), аудио-обработка голосов,
//! скачивание движков — БЕЗ tauri.
//!
//! События фронтов — хук-событийный паттерн: `events::set_event_hook`
//! (SpeechEvent: SttStatus/SttResult/SttKey/TtsCaps/DialogStatus). Логи — через
//! единый `log::Log` логгер хоста (`log::app_log`). Папки данных хоста —
//! параметры `config_dir`/`models_dir` (Tauri-фасад передаёт app_config_dir,
//! GPUI-хосты — свои пути).
//!
//! Скачивание — единый движок проекта `downloader-core` (деп ядра).

pub mod audio;
pub mod clone;
pub mod dialog;
pub mod dialog_settings;
pub mod download;
pub mod events;
pub mod inject;
pub mod log;
pub mod process_util;
pub mod stt;
pub mod stt_settings;
pub mod tts;
pub mod tts_settings;
pub mod voices;
pub mod ws_client;

pub use dialog::{DialogEngine, DialogStatus};
pub use dialog_settings::DialogSettings;
pub use events::{SpeechEvent, SpeechEventHook};
pub use stt::{SttEngine, SttStatus};
pub use stt_settings::SttSettings;
pub use tts::TtsEngine;
pub use tts_settings::TtsSettings;
