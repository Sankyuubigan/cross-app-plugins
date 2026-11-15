use serde::Deserialize;
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Runtime,
};

/// Глобальная копия конфига плагина — нужна свободным функциям
/// (`default_models_dir`/`default_engine_dir` и т.п.), у которых нет доступа
/// к `AppHandle`/`State`. Конфиг попадает сюда в `setup`.
static CONFIG: OnceLock<Config> = OnceLock::new();

const DEFAULT_CONFIG: Config = Config {
    default_engine_dir: None,
    default_models_dir: None,
};

pub(crate) fn set_config(c: Config) {
    let _ = CONFIG.set(c);
}

/// Текущий конфиг плагина (или пустой по умолчанию до инициализации).
pub(crate) fn config() -> &'static Config {
    CONFIG.get().unwrap_or(&DEFAULT_CONFIG)
}

/// Конфиг плагина. Задаётся в `tauri.conf.json` хоста под ключом `plugins.speech`:
/// ```json
/// "plugins": {
///   "speech": {
///     "default_engine_dir": "D:\\nn\\crispasr",
///     "default_models_dir": "D:\\nn\\tts_models"
///   }
/// }
/// ```
///
/// Поля опциональны. Без них используются exe-относительные пути по умолчанию
/// (`<exe_dir>/crispasr` и `<exe_dir>/tts_models` — как в исходном SpeechLab).
#[derive(Deserialize, Clone, Default)]
pub struct Config {
    /// Оверрайд папки движка CrispASR (по умолчанию `<exe_dir>/crispasr`).
    #[serde(default)]
    pub default_engine_dir: Option<String>,
    /// Оверрайд общей папки моделей TTS (по умолчанию `<exe_dir>/tts_models`).
    #[serde(default)]
    pub default_models_dir: Option<String>,
}

/// Оверрайд папки движка из конфига хоста (None = exe-дефолт ядра).
pub(crate) fn engine_dir_override() -> Option<String> {
    config()
        .default_engine_dir
        .as_ref()
        .filter(|v| !v.trim().is_empty())
        .cloned()
}

/// Оверрайд папки моделей из конфига хоста (None = exe-дефолт ядра).
pub(crate) fn models_dir_override() -> Option<String> {
    config()
        .default_models_dir
        .as_ref()
        .filter(|v| !v.trim().is_empty())
        .cloned()
}

/// Папка конфига хоста (в ядре — параметр `config_dir`): app_config_dir,
/// при недоступности — exe-относительный фолбэк.
pub fn config_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    app.path()
        .app_config_dir()
        .unwrap_or_else(|_| {
            std::env::current_exe()
                .unwrap_or_else(|_| PathBuf::from("."))
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf()
        })
}

/// Состояние плагина, доступное командам и хосту во время выполнения.
///
/// Публично, чтобы хост (напр. SpeechLab) мог делегировать движковую логику STT
/// в плагин, сохранив собственную оверлей-оркестрацию (hotkey/mic/web-push).
pub struct PluginState {
    pub config: Config,
    pub tts: speech_core::TtsEngine,
    pub stt: speech_core::SttEngine,
    pub dialog: speech_core::DialogEngine,
}

// Ядро (ССОТ, Шаг 4e PRP 261115): вся логика — в крейте `speech-core` (без tauri).
// Здесь реэкспорты — прежние пути `plugin_speech_gen_engine::<module>` для хостов.
pub use speech_core::{
    audio, clone, dialog, dialog_settings, download, events, inject, log, process_util, stt,
    stt_settings, tts, voices, ws_client,
};
pub use speech_core::{
    DialogEngine, DialogSettings, DialogStatus, SpeechEvent, SpeechEventHook, SttEngine, SttStatus,
    SttSettings, TtsEngine, TtsSettings,
};

/// Мост настроек TTS с прежней сигнатурой (AppHandle): ядро берёт `config_dir`.
pub mod tts_settings {
    pub use speech_core::tts_settings::TtsSettings;

    /// Загружает настройки из app_config_dir хоста.
    pub fn load<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> TtsSettings {
        speech_core::tts_settings::load(&super::config_dir(app))
    }

    /// Сохраняет настройки в app_config_dir хоста.
    pub fn save<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
        settings: &TtsSettings,
    ) -> Result<(), String> {
        speech_core::tts_settings::save(&super::config_dir(app), settings)
    }
}

/// Инициализация плагина. Вызывается из хоста: `.plugin(plugin_speech_gen_engine::init())`.
pub fn init<R: Runtime>() -> TauriPlugin<R, Config> {
    Builder::<R, Config>::new("speech")
        .setup(|app, api| {
            set_config(api.config().clone());
            let state = PluginState {
                config: api.config().clone(),
                tts: TtsEngine::new(),
                stt: SttEngine::new(),
                dialog: DialogEngine::new(),
            };
            app.manage(state);
            // Мост событий ядра в те же tauri-emit (Шаг 4e: ядро эмитит через
            // хук SpeechEvent, формы payload прежние).
            let handle = app.clone();
            let hook: SpeechEventHook = std::sync::Arc::new(move |e: &SpeechEvent| {
                use tauri::Emitter;
                match e {
                    SpeechEvent::SttStatus(label) => {
                        let _ = handle.emit("stt-status", *label);
                    }
                    SpeechEvent::SttResult { text, is_final } => {
                        let _ = handle.emit(
                            "stt-result",
                            serde_json::json!({ "text": text, "final": is_final }),
                        );
                    }
                    SpeechEvent::SttKey {
                        name,
                        code,
                        pressed,
                    } => {
                        let _ = handle.emit(
                            "stt-key",
                            serde_json::json!({ "name": name, "code": code, "pressed": pressed }),
                        );
                    }
                    SpeechEvent::TtsCaps { language } => {
                        let _ =
                            handle.emit("tts-caps", serde_json::json!({ "language": language }));
                    }
                    SpeechEvent::DialogStatus(label) => {
                        let _ = handle.emit("dialog-status", *label);
                    }
                }
            });
            speech_core::events::set_event_hook(Some(hook));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::tts_speak,
            commands::tts_presets,
            commands::tts_capabilities,
            commands::tts_unload,
            commands::tts_save_mp3,
            commands::tts_download_engine,
            commands::tts_delete_engine,
            commands::tts_get_engine_status,
            commands::tts_download_model,
            commands::tts_engine_backends,
            commands::tts_list_models,
            commands::tts_list_voices,
            commands::tts_add_voice,
            commands::tts_delete_voice,
            commands::tts_update_voice,
            commands::tts_voice_avatar,
            commands::tts_voice_audio,
            commands::tts_voice_trimmed_audio,
            commands::tts_check_update,
            commands::tts_default_dirs,
            commands::tts_get_settings,
            commands::tts_save_settings,
            commands::stt_get_settings,
            commands::stt_save_settings,
            commands::stt_start,
            commands::stt_stop,
            commands::stt_get_status,
            commands::stt_inject_text,
            commands::dialog_get_settings,
            commands::dialog_save_settings,
            commands::dialog_download_engine,
            commands::dialog_start,
            commands::dialog_stop,
            commands::dialog_status,
        ])
        .build()
}

mod commands;
