use serde::{Deserialize, Deserializer};
use std::path::PathBuf;
use std::sync::OnceLock;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Runtime,
};

/// Глобальная копия конфига плагина — для свободных функций без доступа к
/// `AppHandle`/`State`. Заполняется в `setup`.
static CONFIG: OnceLock<Config> = OnceLock::new();

const DEFAULT_CONFIG: Config = Config {
    default_engine_dir: None,
    default_models_dir: None,
    default_output_dir: None,
};

pub(crate) fn set_config(c: Config) {
    let _ = CONFIG.set(c);
}

/// Текущий конфиг плагина (или пустой по умолчанию до инициализации).
pub(crate) fn config() -> &'static Config {
    CONFIG.get().unwrap_or(&DEFAULT_CONFIG)
}

/// Конфиг плагина. Задаётся в `tauri.conf.json` хоста под ключом `plugins.audio-gen`:
/// ```json
/// "plugins": {
///   "audio-gen": {
///     "default_engine_dir": "D:\\nn\\audio.cpp",
///     "default_models_dir": "D:\\nn\\audio_models",
///     "default_output_dir": "D:\\Projects\\3d_game\\Audio\\Gen"
///   }
/// }
/// ```
///
/// Все поля опциональны; без них — config-относительные дефолты ядра
/// (`<config_dir>/audio`, `<config_dir>/audio_models`, `<config_dir>/audio_gen_output`).
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub default_engine_dir: Option<String>,
    pub default_models_dir: Option<String>,
    pub default_output_dir: Option<String>,
}

// PLUGIN_STANDARD: конфиг должен переживать null (хост без секции plugins.audio-gen
// передаёт null — десериализуем в дефолт, а не падаем).
impl<'de> Deserialize<'de> for Config {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Option::<serde_json::Value>::deserialize(deserializer)?;
        let v = match v {
            None | Some(serde_json::Value::Null) => return Ok(Config::default()),
            Some(v) => v,
        };
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Raw {
            default_engine_dir: Option<String>,
            default_models_dir: Option<String>,
            default_output_dir: Option<String>,
        }
        let raw: Raw = serde_json::from_value(v).map_err(serde::de::Error::custom)?;
        Ok(Config {
            default_engine_dir: raw.default_engine_dir,
            default_models_dir: raw.default_models_dir,
            default_output_dir: raw.default_output_dir,
        })
    }
}

/// Оверрайды из конфига хоста, прокинутые в настройки ядра.
pub(crate) fn apply_config_overrides(settings: &mut audio_gen_core::AudioGenSettings) {
    let c = config();
    if let Some(d) = c.default_engine_dir.as_ref().filter(|v| !v.trim().is_empty()) {
        settings.engine_dir = d.clone();
    }
    if let Some(d) = c.default_models_dir.as_ref().filter(|v| !v.trim().is_empty()) {
        settings.models_dir = d.clone();
    }
    if let Some(d) = c.default_output_dir.as_ref().filter(|v| !v.trim().is_empty()) {
        settings.output_dir = d.clone();
    }
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

/// Состояние плагина. Сервер движка живёт в рамках задачи генерации (spawn →
/// generate → kill), поэтому долгоживущего состояния нет — только конфиг.
pub struct PluginState {
    pub config: Config,
}

// Ядро (ССОТ): вся логика — в `audio-gen-core` (без tauri). Реэкспорты —
// единые пути `plugin_audio_gen_engine::<module>` для хостов.
pub use audio_gen_core::{config, engine, job, models, wav};
pub use audio_gen_core::{
    AudioGenEvent, AudioGenSettings, EngineHandle, GenerateRequest, JobRecord, JobStatus, ModelInfo,
};

/// Инициализация плагина. Вызывается из хоста: `.plugin(plugin_audio_gen_engine::init())`.
pub fn init<R: Runtime>() -> TauriPlugin<R, Config> {
    Builder::<R, Config>::new("audio-gen")
        .setup(|app, api| {
            set_config(api.config().clone());
            app.manage(PluginState { config: api.config().clone() });
            // Мост событий ядра в tauri-emit (ядро не знает tauri — инъекция хука).
            let handle = app.clone();
            let hook: audio_gen_core::AudioGenEventHook = std::sync::Arc::new(move |e| {
                use tauri::Emitter;
                let _ = handle.emit(
                    "audio-gen-event",
                    serde_json::json!({
                        "jobId": e.job_id,
                        "status": e.status,
                        "progress": e.progress,
                    }),
                );
            });
            audio_gen_core::job::set_event_hook(Some(hook));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::audio_gen_list_models,
            commands::audio_gen_generate,
            commands::audio_gen_status,
            commands::audio_gen_cancel,
            commands::audio_gen_open_output_dir,
        ])
        .build()
}

mod commands;
