//! Настройки плагина: `audio_gen_settings.json` в config_dir хоста.
//!
//! Ядро получает `config_dir` параметром (tauri-фасад передаёт app_config_dir,
//! GPUI-хосты — свою папку настроек). Записи — только через json-store
//! (атомарные, fs::write запрещён правилом плагинов).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Настройки генерации аудио. Все пути могут быть пустыми — тогда берутся
/// exe/config-относительные дефолты (см. резолверы ниже).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioGenSettings {
    /// Папка движка audio.cpp (внутри — audiocpp_server.exe или <backend>/audiocpp_server.exe).
    pub engine_dir: String,
    /// Папка весов моделей: `<models_dir>/<model_id>/*.gguf`.
    pub models_dir: String,
    /// Куда складывать сгенерированные WAV.
    pub output_dir: String,
    /// Бэкенд движка: "auto" (cuda если есть, иначе cpu) / "cuda" / "cpu".
    pub backend: String,
    /// Длина луп-кроссфейда в секундах для музыки (0 = без кроссфейда).
    pub loop_crossfade_secs: f32,
    /// Нормализовать ли громкость после генерации (RMS -18 dBFS, пик ≤ -1 dBFS).
    pub normalize: bool,
}

impl Default for AudioGenSettings {
    fn default() -> Self {
        Self {
            engine_dir: String::new(),
            models_dir: String::new(),
            output_dir: String::new(),
            backend: "auto".to_string(),
            loop_crossfade_secs: 2.5,
            normalize: true,
        }
    }
}

const FILE_NAME: &str = "audio_gen_settings.json";

fn config_path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
}

/// Загружает настройки; при отсутствии/невалидном файле — дефолт.
pub fn load(config_dir: &Path) -> AudioGenSettings {
    let path = config_path(config_dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => AudioGenSettings::default(),
    }
}

/// Сохраняет настройки атомарно (json-store).
pub fn save(config_dir: &Path, settings: &AudioGenSettings) -> Result<(), String> {
    let path = config_path(config_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    json_store::write_atomic(&path, &text).map_err(|e| format!("не удалось сохранить настройки: {e}"))
}

/// Резолв папки движка: настройка или `<config_dir>/audio`.
pub fn engine_dir(settings: &AudioGenSettings, config_dir: &Path) -> PathBuf {
    if settings.engine_dir.trim().is_empty() {
        config_dir.join("audio")
    } else {
        PathBuf::from(&settings.engine_dir)
    }
}

/// Резолв папки моделей: настройка или `<config_dir>/audio_models`.
pub fn models_dir(settings: &AudioGenSettings, config_dir: &Path) -> PathBuf {
    if settings.models_dir.trim().is_empty() {
        config_dir.join("audio_models")
    } else {
        PathBuf::from(&settings.models_dir)
    }
}

/// Резолв папки вывода: настройка или `<config_dir>/audio_gen_output`.
pub fn output_dir(settings: &AudioGenSettings, config_dir: &Path) -> PathBuf {
    if settings.output_dir.trim().is_empty() {
        config_dir.join("audio_gen_output")
    } else {
        PathBuf::from(&settings.output_dir)
    }
}
