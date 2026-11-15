use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Сохраняемые настройки TTS-движка CrispASR.
///
/// Сериализуется в `tts_settings.json` внутри папки конфига хоста
/// (`config_dir`, 100% локально, без облака); Tauri-фасад передаёт
/// app_config_dir, GPUI-хосты — свою папку настроек.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TtsSettings {
    /// Папка, где живут бинари движка по бэкендам: `<engine_dir>/<backend>/crispasr.exe`.
    pub engine_dir: String,
    /// ОБЩАЯ папка моделей TTS. Внутри — подпапка на каждый пресет.
    pub models_dir: String,
    /// Выбранный тип бэкенда движка: "cpu" / "cuda" / "vulkan" / "cpu-legacy" / …
    pub engine_backend: String,
    /// Выбранный пресет TTS-модели.
    pub preset: String,
}

const FILE_NAME: &str = "tts_settings.json";

fn config_path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
}

/// Загружает настройки из диска. При отсутствии/невалидном файле возвращает дефолтные.
pub fn load(config_dir: &Path) -> TtsSettings {
    let path = config_path(config_dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => TtsSettings::default(),
    }
}

/// Сохраняет настройки на диск.
pub fn save(config_dir: &Path, settings: &TtsSettings) -> Result<(), String> {
    let path = config_path(config_dir);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("не удалось сохранить настройки: {e}"))
}