//! Реестр моделей генерации аудио (`audio_models.json` вшит в бинарник).
//!
//! Реестр описывает МОДЕЛИ (что можно генерировать, лицензии, требования),
//! не установку: фактическая установка = файлы .gguf в `<models_dir>/<id>/`.
//! Источник рекомендаций: спец-дока 16.11.2026 (E:\Downloads\3-2026-11-16 (1).md).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Вшитый реестр моделей (редактируется в коде репозитория).
const REGISTRY_JSON: &str = include_str!("../../audio_models.json");

/// Описание одной модели генерации аудио.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Идентификатор = id в server.json движка (например "sa3-medium").
    pub id: String,
    /// Человекочитаемое имя.
    pub name: String,
    /// Категория: "sfx" / "sfx+music" / "music+song".
    pub kind: String,
    /// Семейство движка: "moss_soundeffect" / "stable_audio" / "ace_step".
    /// Определяет имена request-опций (num_steps vs num_inference_steps).
    pub family: String,
    /// Движок (сейчас только "audio.cpp").
    pub engine: String,
    /// Минимальная версия движка.
    pub min_engine_version: String,
    /// Лицензия модели (краткое человекочитаемое описание).
    pub license: String,
    /// Требования по VRAM в ГБ (ориентир; None = неизвестно/не критично).
    pub vram_gb: Option<u32>,
    /// Частота дискретизации выхода (Гц).
    pub sample_rate: u32,
    /// Число каналов выхода (1 моно / 2 стерео).
    pub channels: u32,
    /// Максимальная длительность одного клипа (сек).
    pub max_duration_secs: u32,
    /// Откуда качать веса (HF-репозиторий или release-зип движка).
    pub weights_repo: String,
    /// Пакетная папка внутри models_dir (target_directory из model_specs движка).
    #[serde(default)]
    pub weights_dir: String,
    /// Ожидаемые файлы пакета (имена в репо audio.cpp-gguf).
    #[serde(default)]
    pub weights_files: Vec<String>,
    /// Комментарий по весам (имена файлов сверять в репо при установке).
    pub weights_note: String,
    /// Типичные шаги диффузии (для UI-дефолта).
    pub default_steps: u32,
}

#[derive(Debug, Deserialize)]
struct Registry {
    models: Vec<ModelInfo>,
}

/// Загружает вшитый реестр моделей. Паника невозможна: реестр валиден по тесту.
pub fn load_registry() -> Result<Vec<ModelInfo>, String> {
    let reg: Registry =
        serde_json::from_str(REGISTRY_JSON).map_err(|e| format!("битый audio_models.json: {e}"))?;
    Ok(reg.models)
}

/// Установлена ли модель: есть ли .gguf-файлы в пакетной папке
/// `<models_dir>/<weights_dir>/` (weights_dir из реестра,=target_directory спецификации движка).
pub fn is_installed(models_dir: &Path, model: &ModelInfo) -> bool {
    let dir = if model.weights_dir.is_empty() {
        models_dir.join(&model.id)
    } else {
        models_dir.join(&model.weights_dir)
    };
    match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .any(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("gguf"))
                    .unwrap_or(false)
            }),
        Err(_) => false,
    }
}

/// Полный путь к первому найденному .gguf пакетной папки модели.
pub fn weights_gguf_path(models_dir: &Path, model: &ModelInfo) -> Option<PathBuf> {
    let dir = if model.weights_dir.is_empty() {
        models_dir.join(&model.id)
    } else {
        models_dir.join(&model.weights_dir)
    };
    // Сначала ожидаемое имя из реестра, затем любой .gguf в папке.
    for name in &model.weights_files {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("gguf"))
                .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parses_and_has_expected_models() {
        let models = load_registry().expect("реестр должен парситься");
        assert!(models.len() >= 3, "как минимум 3 модели в реестре");
        for m in &models {
            assert!(!m.id.is_empty());
            assert!(!m.license.is_empty(), "у {} пустая лицензия", m.id);
            assert!(m.sample_rate > 0);
            assert!(!m.family.is_empty(), "у {} пустое семейство", m.id);
            assert!(!m.weights_dir.is_empty(), "у {} пустая пакетная папка", m.id);
        }
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"sa3-medium"));
        assert!(ids.contains(&"sa3-small-sfx"));
        assert!(ids.contains(&"moss-sfx-v2"));
        assert!(ids.contains(&"ace-step-1.5"));
    }

    #[test]
    fn is_installed_false_on_missing_dir() {
        let tmp = std::env::temp_dir().join(format!("audio_gen_test_{}", std::process::id()));
        let models = load_registry().unwrap();
        let m = &models[0];
        assert!(!is_installed(&tmp, m));
        assert!(weights_gguf_path(&tmp, m).is_none());
    }
}
