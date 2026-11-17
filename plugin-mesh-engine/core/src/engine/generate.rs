//! Оркестрация одной генерации image→GLB (per-call spawn, как ImageEngine):
//! префлайт → спавн trellis-server → /health → POST /generate → GLB-файл.
//! Resident-режим (сервер живёт между вызовами, веса не перечитываются) —
//! оптимизация после MVP; per-call проще и герметичен по утечкам (Drop).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::client::{post_generate, GenerateFields, GEN_TIMEOUT};
use super::outputs::save_glb_sequenced;
use super::preflight::{check_engine_files, engine_paths};
use super::presets::{MeshPreset, MeshQuant};

/// Все параметры одной генерации. Ядро принимает их ЯВНО (без AppHandle,
/// PLUGIN_STANDARD §0.1 п.1): гейт источников и пути вывода — ответственность
/// хост-адаптера.
pub struct GenerateMeshParams {
    /// Корневая папка движка (обычно `<exe>/trellis`).
    pub engine_dir: PathBuf,
    /// Папка вывода сессии (`<…>/mesh_outputs/<session_id>`).
    pub out_dir: PathBuf,
    /// Приложенное изображение (путь уже прошёл гейт источников хоста).
    pub image_path: PathBuf,
    /// Семейство модели (дефолт Trellis).
    pub model: MeshPreset,
    /// Квант весов (дефолт DEFAULT_QUANT = q8).
    pub quant: MeshQuant,
    /// `--backend` (None → выбор движка).
    pub backend: Option<String>,
    /// `--gpu N` (None → дефолт движка).
    pub gpu: Option<i32>,
    /// `seed` (None → дефолт сервера).
    pub seed: Option<u32>,
    /// Разрешение (512 | 1024 | 1536).
    pub resolution: u32,
    /// `bg_removal` (None → дефолт сервера threshold).
    pub bg_removal: Option<String>,
    /// `faces` ("auto" | число ≥ 1000).
    pub faces: Option<String>,
}

/// Результат генерации.
pub struct MeshGenOutcome {
    /// Путь к сохранённому GLB.
    pub glb_path: PathBuf,
    /// Порт, на котором работал сервер (диагностика).
    pub port: u16,
    /// Длительность генерации.
    pub duration: Duration,
}

/// Чистая валидация параметров ДО спавна движка (тестится без fs/сети).
pub fn validate(
    resolution: u32,
    bg_removal: Option<&str>,
    faces: Option<&str>,
) -> Result<(), String> {
    if !super::client::RESOLUTIONS_ALLOWED.contains(&resolution) {
        return Err(format!(
            "недопустимое разрешение {resolution}: сервер принимает 512 / 1024 / 1536"
        ));
    }
    if let Some(bg) = bg_removal {
        let bg = bg.trim().to_ascii_lowercase();
        if bg != "threshold" && bg != "birefnet" {
            return Err(format!("bg_removal '{bg}': допустимо threshold | birefnet"));
        }
    }
    if let Some(f) = faces {
        let f = f.trim().to_ascii_lowercase();
        if f != "auto" {
            let n: u32 = f
                .parse()
                .map_err(|_| format!("faces '{f}': ожидается 'auto' или целое число ≥ 1000"))?;
            if n < 1000 {
                return Err(format!(
                    "faces {n} < 1000 — сервер отвергнет (минимум 1000 или 'auto')"
                ));
            }
        }
    }
    Ok(())
}

/// Одна генерация image→GLB. Процесс движка живёт только на время вызова.
pub fn generate_mesh(p: &GenerateMeshParams) -> Result<MeshGenOutcome, String> {
    validate(p.resolution, p.bg_removal.as_deref(), p.faces.as_deref())?;
    if !p.image_path.is_file() {
        return Err(format!("изображение не найдено: {}", p.image_path.display()));
    }

    let paths = engine_paths(&p.engine_dir, p.quant);
    check_engine_files(&paths, p.model == MeshPreset::Pixal3d)?;

    let started = Instant::now();
    log::info!(
        "[mesh] генерация: model={} quant={} res={} backend={:?} gpu={:?} seed={:?} faces={:?} image={}",
        p.model.as_flag(),
        p.quant.dir_name(),
        p.resolution,
        p.backend,
        p.gpu,
        p.seed,
        p.faces,
        p.image_path.display()
    );

    let engine = super::process::MeshEngine::spawn(&super::process::SpawnConfig {
        exe: &paths.exe,
        models_dir: &paths.models_dir,
        model: p.model,
        backend: p.backend.as_deref(),
        gpu: p.gpu,
    })?;

    let fields = GenerateFields {
        seed: p.seed,
        resolution: p.resolution,
        bg_removal: p.bg_removal.clone(),
        faces: p.faces.clone(),
    };
    let glb = post_generate(&engine.base_url(), &p.image_path, &fields, GEN_TIMEOUT)?;
    let glb_path = save_glb_sequenced(&p.out_dir, &glb)?;
    let duration = started.elapsed();
    log::info!("[mesh] готово за {duration:?}: {} ({} байт)", glb_path.display(), glb.len());
    Ok(MeshGenOutcome { glb_path, port: engine.port(), duration })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_canonical_inputs() {
        validate(512, None, None).unwrap();
        validate(1024, Some("threshold"), Some("auto")).unwrap();
        validate(1536, Some("Birefnet"), Some("5000")).unwrap();
    }

    #[test]
    fn validate_rejects_bad_resolution() {
        let err = validate(256, None, None).unwrap_err();
        assert!(err.contains("512 / 1024 / 1536"), "{err}");
    }

    #[test]
    fn validate_rejects_bad_bg_removal() {
        let err = validate(512, Some("rembg"), None).unwrap_err();
        assert!(err.contains("threshold | birefnet"), "{err}");
    }

    #[test]
    fn validate_rejects_faces_below_server_minimum() {
        assert!(validate(512, None, Some("999")).unwrap_err().contains("минимум 1000"));
        assert!(validate(512, None, Some("abc")).unwrap_err().contains("'auto'"));
        validate(512, None, Some("1000")).unwrap();
    }
}
