//! События speech-ядра для фронтов (хук-событийный паттерн, Шаг 4e PRP 261115):
//! вместо tauri-emit — глобальный хук `set_event_hook`; фасад ставит мост,
//! конвертирующий события в те же tauri-emit (`stt-status`/`stt-result`/
//! `stt-key`/`tts-caps`/`dialog-status`). GPUI-хосты подписываются напрямую.

use std::sync::{Arc, RwLock};

use crate::dialog::DialogStatus;
use crate::stt::SttStatus;

/// Все события speech-подсистем, уходящие на фронт.
#[derive(Debug, Clone)]
pub enum SpeechEvent {
    /// Статус STT-движка (лейбл: stopped/starting/listening/recording/
    /// transcribing/error; прежнее событие `stt-status`).
    SttStatus(&'static str),
    /// Распознанный текст (прежнее событие `stt-result`).
    SttResult { text: String, is_final: bool },
    /// Лог нажатия хоткея для UI настройки (прежнее событие `stt-key`).
    SttKey {
        name: String,
        code: u32,
        pressed: bool,
    },
    /// Возможности загруженного движка TTS (прежнее событие `tts-caps`).
    TtsCaps { language: bool },
    /// Статус диалогового движка (лейбл: stopped/starting/ready/error;
    /// прежнее событие `dialog-status`).
    DialogStatus(&'static str),
}

/// Хук событий: фасад/GPUI-хост ставят замыкание, получающее все события.
pub type SpeechEventHook = Arc<dyn Fn(&SpeechEvent) + Send + Sync>;

static EVENT_HOOK: RwLock<Option<SpeechEventHook>> = RwLock::new(None);

/// Ставит (или снимает — `None`) хук событий. Вызывать при инициализации хоста.
pub fn set_event_hook(hook: Option<SpeechEventHook>) {
    *EVENT_HOOK.write().unwrap() = hook;
}

/// Отправляет событие в установленный хук (без хука — бездействует).
pub fn notify(event: SpeechEvent) {
    if let Some(hook) = EVENT_HOOK.read().unwrap().as_ref() {
        hook(&event);
    }
}

/// Статус STT → событие (лейбл по прежней схеме emit_status).
pub fn notify_stt_status(status: &SttStatus) {
    let label = match status {
        SttStatus::Stopped => "stopped",
        SttStatus::Starting => "starting",
        SttStatus::Listening => "listening",
        SttStatus::Recording => "recording",
        SttStatus::Transcribing => "transcribing",
        SttStatus::Error(_) => "error",
    };
    notify(SpeechEvent::SttStatus(label));
}

/// Статус диалога → событие (лейбл по прежней схеме emit_status).
pub fn notify_dialog_status(status: &DialogStatus) {
    let label = match status {
        DialogStatus::Stopped => "stopped",
        DialogStatus::Starting => "starting",
        DialogStatus::Ready => "ready",
        DialogStatus::Error(_) => "error",
    };
    notify(SpeechEvent::DialogStatus(label));
}
