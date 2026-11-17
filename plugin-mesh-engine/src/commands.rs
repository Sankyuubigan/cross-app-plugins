//! Тонкие Tauri-мосты (PLUGIN_STANDARD §0.1 п.2): сериализация + лог.
//! Err → log::error! (§12.1). Пути — `current_exe().parent()` (rules.md:53).
//! Команды `<R: Runtime>`-дженерики (§4.4); логика — только в ядре.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime};

use crate::engine;

fn exe_parent_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn engine_dir() -> PathBuf {
    // SSOT: явная папка из app_config.json (`trellis_dir`) либо `<exe>/trellis`.
    engine::config::engine_dir_early()
}

fn outputs_dir(session_id: &str) -> PathBuf {
    exe_parent_dir().join("mesh_outputs").join(sanitize_session(session_id))
}

fn sanitize_session(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

#[derive(Serialize)]
pub struct MeshQuantStatus {
    pub quant: String,
    pub dir: String,
    pub gguf_count: usize,
    pub pixal3d_files: usize,
}

#[derive(Serialize)]
pub struct MeshEngineStatus {
    pub installed: bool,
    pub exe_path: String,
    pub dir: String,
    pub weights: Vec<MeshQuantStatus>,
}

/// Тело статуса (без AppHandle) — используется командой и ответами install/set_dir.
fn status_impl() -> MeshEngineStatus {
    let dir = engine_dir();
    let exe = engine::exe_path(&dir);
    let installed = exe.is_file();
    log::info!("[mesh] status: installed={installed} dir={}", dir.display());
    MeshEngineStatus {
        installed,
        exe_path: exe.display().to_string(),
        dir: dir.display().to_string(),
        weights: engine::weights_status(&dir)
            .into_iter()
            .map(|w| MeshQuantStatus {
                quant: w.quant,
                dir: w.dir,
                gguf_count: w.gguf_count,
                pixal3d_files: w.pixal3d_files,
            })
            .collect(),
    }
}

#[tauri::command]
pub fn mesh_engine_status<R: Runtime>(_app: AppHandle<R>) -> MeshEngineStatus {
    status_impl()
}

/// Аргументы установки: бекенд server-бандла (auto/vulkan/cuda/cuda12/rocm),
/// квант весов (q4/q8/f16), докачать ли пресет Pixal3D (только f16).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshInstallArgs {
    pub backend: Option<String>,
    pub quant: Option<String>,
}

#[tauri::command]
pub async fn mesh_engine_install<R: Runtime>(
    _app: AppHandle<R>,
    args: MeshInstallArgs,
) -> Result<MeshEngineStatus, String> {
    let quant = match args.quant.as_deref() {
        None | Some("") => engine::DEFAULT_QUANT,
        Some(s) => engine::MeshQuant::parse(s)?,
    };
    let dir = engine_dir();
    log::info!(
        "[mesh] install: dir={} backend={:?} quant={} (trellis2 + pixal3d всегда)",
        dir.display(),
        args.backend,
        quant.dir_name()
    );
    // Инсталлер async (downloader-core) — вызывается прямо в tokio-контексте.
    engine::installer::install(&dir, args.backend.as_deref(), quant, &|msg: String| {
        log::info!("[mesh-install] {msg}");
    })
    .await?;
    Ok(status_impl())
}

/// Проверка обновления server-бандла: тег из меты vs последний релиз GitHub.
/// Some(tag) — доступно обновление; ошибка сети — честный Err (не «актуален»).
#[tauri::command]
pub async fn mesh_engine_check_update<R: Runtime>(
    _app: AppHandle<R>,
) -> Result<Option<String>, String> {
    let dir = engine_dir();
    engine::installer::check_update(&dir, &|msg: String| {
        log::info!("[mesh-update] {msg}");
    })
    .await
}

/// Смена папки движка: persist в app_config.json + свежий статус.
#[tauri::command]
pub fn mesh_engine_set_dir<R: Runtime>(
    _app: AppHandle<R>,
    path: String,
) -> Result<MeshEngineStatus, String> {
    engine::config::save_trellis_dir(&path)?;
    log::info!("[mesh] set_dir: {}", path);
    Ok(status_impl())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshGenerateArgs {
    pub image_path: String,
    pub session_id: String,
    pub model: Option<String>,
    pub quant: Option<String>,
    pub backend: Option<String>,
    pub gpu: Option<i32>,
    pub seed: Option<u32>,
    pub resolution: Option<u32>,
    pub bg_removal: Option<String>,
    pub faces: Option<String>,
}

#[derive(Serialize)]
pub struct MeshGenerateOut {
    pub glb_path: String,
    pub port: u16,
    pub duration_secs: u64,
}

#[tauri::command]
pub async fn mesh_generate<R: Runtime>(
    _app: AppHandle<R>,
    args: MeshGenerateArgs,
) -> Result<MeshGenerateOut, String> {
    log::info!("[mesh] generate: session={} image={}", args.session_id, args.image_path);
    let params = build_params(args)?;
    let outcome = tokio::task::spawn_blocking(move || engine::generate_mesh(&params))
        .await
        .map_err(|e| {
            let msg = format!("[mesh] поток генерации прерван: {e}");
            log::error!("{msg}");
            msg
        })?;
    match outcome {
        Ok(o) => Ok(MeshGenerateOut {
            glb_path: o.glb_path.display().to_string(),
            port: o.port,
            duration_secs: o.duration.as_secs(),
        }),
        Err(e) => {
            log::error!("[mesh] generate: {e}");
            Err(e)
        }
    }
}

fn build_params(args: MeshGenerateArgs) -> Result<engine::GenerateMeshParams, String> {
    let model = match args.model.as_deref() {
        None | Some("") => engine::MeshPreset::Trellis,
        Some(s) => engine::MeshPreset::parse(s)?,
    };
    let quant = match args.quant.as_deref() {
        None | Some("") => engine::DEFAULT_QUANT,
        Some(s) => engine::MeshQuant::parse(s)?,
    };
    if args.session_id.trim().is_empty() {
        return Err("session_id пуст — некуда сохранить результат".into());
    }
    let out_dir = outputs_dir(&args.session_id);
    Ok(engine::GenerateMeshParams {
        engine_dir: engine_dir(),
        out_dir,
        image_path: PathBuf::from(args.image_path),
        model,
        quant,
        backend: args.backend,
        gpu: args.gpu,
        seed: args.seed,
        resolution: args.resolution.unwrap_or(engine::DEFAULT_RESOLUTION),
        bg_removal: args.bg_removal,
        faces: args.faces,
    })
}

// ─────────────── Риггинг (skin-tokens.cpp, этап 2 PRP «Риггинг skin-tokens.md») ───────────────

fn rig_dir() -> PathBuf {
    // SSOT: явная папка из app_config.json (`skintokens_dir`) либо `<exe>/skintokens`.
    engine::config::rig_dir_early()
}

/// Статус раскладки риггинга для секции «Риггинг» плашки. Честность:
/// exe/DLL панель НЕ скачивает (пребилдов upstream нет — exe кладётся из
/// локальной сборки), команда установки качает только 3 веса.
#[derive(Serialize)]
pub struct RigEngineStatus {
    pub ready: bool,
    pub exe_present: bool,
    pub dlls_present: bool,
    pub weights_present: usize,
    pub weights_total: usize,
    pub dir: String,
    pub weights_dir: String,
}

impl From<engine::installer::RigStatus> for RigEngineStatus {
    fn from(s: engine::installer::RigStatus) -> Self {
        RigEngineStatus {
            ready: s.ready(),
            exe_present: s.exe_present,
            dlls_present: s.dlls_present,
            weights_present: s.weights_present,
            weights_total: engine::installer::SKINTOKENS_FILES.len(),
            dir: s.dir,
            weights_dir: s.weights_dir,
        }
    }
}

#[tauri::command]
pub fn mesh_rig_engine_status<R: Runtime>(_app: AppHandle<R>) -> RigEngineStatus {
    let dir = rig_dir();
    let status = engine::installer::rig_status(&dir);
    log::info!(
        "[rig] status: ready={} dir={}",
        status.ready(),
        dir.display()
    );
    status.into()
}

/// Скачать/докачать веса риггинга (3 файла F16, ~1.25 ГБ). Возврат — свежий статус.
#[tauri::command]
pub async fn mesh_rig_engine_install<R: Runtime>(
    _app: AppHandle<R>,
) -> Result<RigEngineStatus, String> {
    let dir = rig_dir();
    log::info!("[rig] install weights: dir={}", dir.display());
    let status = engine::installer::install_rig_weights(&dir, &|msg: String| {
        log::info!("[rig-install] {msg}");
    })
    .await
    .map_err(|e| {
        log::error!("[rig] install: {e}");
        e
    })?;
    Ok(status.into())
}

/// Смена папки риггинга: persist в app_config.json (`skintokens_dir`,
/// зеркало `mesh_engine_set_dir`) + свежий статус.
#[tauri::command]
pub fn mesh_rig_engine_set_dir<R: Runtime>(
    _app: AppHandle<R>,
    path: String,
) -> Result<RigEngineStatus, String> {
    engine::config::save_skintokens_dir(&path)?;
    log::info!("[rig] set_dir: {}", path);
    Ok(engine::installer::rig_status(&rig_dir()).into())
}
