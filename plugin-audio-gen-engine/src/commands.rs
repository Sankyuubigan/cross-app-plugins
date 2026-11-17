use serde::Serialize;
use tauri::{AppHandle, Runtime};

use crate::{apply_config_overrides, config_dir};

/// Модель реестра в формате UI: описание + фактическая установка на диске.
#[derive(Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub license: String,
    pub vram_gb: Option<u32>,
    pub sample_rate: u32,
    pub channels: u32,
    pub max_duration_secs: u32,
    pub default_steps: u32,
    pub weights_repo: String,
    pub installed: bool,
}

/// Список известных моделей с признаком установки (есть .gguf в `<models_dir>/<id>`).
#[tauri::command]
pub async fn audio_gen_list_models<R: Runtime>(app: AppHandle<R>) -> Result<Vec<ModelEntry>, String> {
    let dir = config_dir(&app);
    let mut settings = audio_gen_core::config::load(&dir);
    apply_config_overrides(&mut settings);
    let models_root = audio_gen_core::config::models_dir(&settings, &dir);

    let registry = audio_gen_core::models::load_registry()?;
    Ok(registry
        .into_iter()
        .map(|m| {
            let installed = audio_gen_core::models::is_installed(&models_root, &m);
            ModelEntry {
                id: m.id,
                name: m.name,
                kind: m.kind,
                license: m.license,
                vram_gb: m.vram_gb,
                sample_rate: m.sample_rate,
                channels: m.channels,
                max_duration_secs: m.max_duration_secs,
                default_steps: m.default_steps,
                weights_repo: m.weights_repo,
                installed,
            }
        })
        .collect())
}

/// Запустить генерацию: ставит задачу в очередь и сразу возвращает job_id.
/// Прогресс — события `audio-gen-event` + опрос `audio_gen_status`.
#[tauri::command]
pub async fn audio_gen_generate<R: Runtime>(
    app: AppHandle<R>,
    model_id: String,
    prompt: String,
    duration_secs: Option<f32>,
    steps: Option<u32>,
    seed: Option<u64>,
    make_loop: Option<bool>,
    name: Option<String>,
) -> Result<String, String> {
    let dir = config_dir(&app);
    let mut settings = audio_gen_core::config::load(&dir);
    apply_config_overrides(&mut settings);

    let req = audio_gen_core::GenerateRequest {
        model_id,
        prompt,
        duration_secs,
        steps,
        seed,
        make_loop: make_loop.unwrap_or(false),
        name,
    };
    audio_gen_core::job::submit(req, settings, dir).map_err(|e| {
        log::error!("audio_gen_generate: {e}");
        e
    })
}

/// Снимок состояния: все задачи + резолвленные пути настроек (для UI).
#[tauri::command]
pub async fn audio_gen_status<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let dir = config_dir(&app);
    let mut settings = audio_gen_core::config::load(&dir);
    apply_config_overrides(&mut settings);

    let jobs = audio_gen_core::job::jobs_snapshot();
    Ok(serde_json::json!({
        "jobs": jobs,
        "settings": {
            "backend": settings.backend,
            "loopCrossfadeSecs": settings.loop_crossfade_secs,
            "normalize": settings.normalize,
            "engineDir": audio_gen_core::config::engine_dir(&settings, &dir),
            "modelsDir": audio_gen_core::config::models_dir(&settings, &dir),
            "outputDir": audio_gen_core::config::output_dir(&settings, &dir),
        },
    }))
}

/// Отменить задачу (убивает сервер движка по PID-дереву).
#[tauri::command]
pub async fn audio_gen_cancel(job_id: String) -> Result<bool, String> {
    Ok(audio_gen_core::job::cancel(&job_id))
}

/// Открыть папку вывода в проводнике (создать при отсутствии).
#[tauri::command]
pub async fn audio_gen_open_output_dir<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let dir = config_dir(&app);
    let mut settings = audio_gen_core::config::load(&dir);
    apply_config_overrides(&mut settings);
    let out = audio_gen_core::config::output_dir(&settings, &dir);
    std::fs::create_dir_all(&out).map_err(|e| format!("не удалось создать папку вывода: {e}"))?;

    #[cfg(windows)]
    let spawned = std::process::Command::new("explorer").arg(&out).spawn();
    #[cfg(not(windows))]
    let spawned = std::process::Command::new("xdg-open").arg(&out).spawn();

    match spawned {
        Ok(_) => Ok(out.to_string_lossy().to_string()),
        Err(e) => {
            let msg = format!("не удалось открыть папку {}: {e}", out.display());
            log::error!("audio_gen_open_output_dir: {msg}");
            Err(msg)
        }
    }
}
