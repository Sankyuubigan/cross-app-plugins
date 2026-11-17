//! Очередь генераций: submit → Running → Done/Failed/Cancelled, с event-hook
//! (инъекция, ядро не знает tauri — паттерн speech-core::events) и отменой
//! через engine-proc-util (kill дерева PID движка).
//!
//! Тяжёлая работа сериализуется одним RUN_LOCK (VRAM у модели один): очередь
//! = фьючерсы, ждущие лок. Одновременно идёт максимум одна генерация.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;

use crate::config::{self, AudioGenSettings};
use crate::{engine, wav};

/// Событие ядра: фасад по job_id перечитывает запись и шлёт в UI.
#[derive(Debug, Clone, Serialize)]
pub struct AudioGenEvent {
    pub job_id: String,
    pub status: String,
    pub progress: u8,
}

/// Тип хука событий (инъекция из фасада).
pub type AudioGenEventHook = Arc<dyn Fn(&AudioGenEvent) + Send + Sync>;

static EVENT_HOOK: OnceLock<Mutex<Option<AudioGenEventHook>>> = OnceLock::new();
static JOBS: OnceLock<Mutex<HashMap<String, JobRecord>>> = OnceLock::new();
static RUN_LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();
static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
/// PID запущенного сервера текущей задачи (для cancel — kill дерева).
static CURRENT_SERVER_PID: AtomicU32 = AtomicU32::new(0);

fn jobs() -> &'static Mutex<HashMap<String, JobRecord>> {
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn run_lock() -> &'static AsyncMutex<()> {
    RUN_LOCK.get_or_init(|| AsyncMutex::new(()))
}

fn event_slot() -> &'static Mutex<Option<AudioGenEventHook>> {
    EVENT_HOOK.get_or_init(|| Mutex::new(None))
}

/// Установить хук событий (фасад эмитит tauri-событие по каждому обновлению).
pub fn set_event_hook(hook: Option<AudioGenEventHook>) {
    *event_slot().lock().unwrap() = hook;
}

fn notify(job_id: &str, status: &str, progress: u8) {
    let event = AudioGenEvent { job_id: job_id.to_string(), status: status.to_string(), progress };
    if let Some(hook) = event_slot().lock().unwrap().as_ref() {
        hook(&event);
    }
}

/// Статус задачи (сериализуется в UI как строка).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::Done => "done",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }
}

/// Запись о задаче генерации (снимок для UI).
#[derive(Debug, Clone, Serialize)]
pub struct JobRecord {
    pub id: String,
    pub model_id: String,
    pub prompt: String,
    pub status: JobStatus,
    pub progress: u8,
    pub message: String,
    pub output_path: Option<String>,
    pub duration_secs: Option<f32>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

/// Параметры генерации.
#[derive(Debug, Clone, Deserialize)]
pub struct GenerateRequest {
    pub model_id: String,
    pub prompt: String,
    /// Длительность клипа (сек); None = дефолт модели.
    pub duration_secs: Option<f32>,
    /// Шаги диффузии; None = дефолт модели.
    pub steps: Option<u32>,
    pub seed: Option<u64>,
    /// Сделать бесшовный луп (кроссфейд хвоста в начало).
    pub make_loop: bool,
    /// Имя файла без расширения; None = автоген из времени.
    pub name: Option<String>,
}

static JOB_COUNTER: AtomicU32 = AtomicU32::new(0);

fn next_job_id() -> String {
    let n = JOB_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    format!("job_{}_{}", chrono::Local::now().format("%Y%m%d_%H%M%S"), n)
}

/// Поставить задачу в очередь. Возвращает job_id. Задача стартует сразу
/// (tokio::spawn), но тяжёлая секция сериализована RUN_LOCK — очередная ждёт.
pub fn submit(
    req: GenerateRequest,
    settings: AudioGenSettings,
    config_dir: PathBuf,
) -> Result<String, String> {
    if req.prompt.trim().is_empty() {
        return Err("пустой промпт".to_string());
    }
    let id = next_job_id();
    let record = JobRecord {
        id: id.clone(),
        model_id: req.model_id.clone(),
        prompt: req.prompt.clone(),
        status: JobStatus::Queued,
        progress: 0,
        message: "в очереди".to_string(),
        output_path: None,
        duration_secs: None,
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        finished_at: None,
    };
    jobs().lock().unwrap().insert(id.clone(), record);
    notify(&id, "queued", 0);

    tokio::spawn(run_job(id.clone(), req, settings, config_dir));
    Ok(id)
}

/// Основной конвейер задачи: сервер → генерация → постобработка → файл.
async fn run_job(id: String, req: GenerateRequest, settings: AudioGenSettings, config_dir: PathBuf) {
    let _permit = run_lock().lock().await;
    if CANCEL_REQUESTED.swap(false, Ordering::Relaxed) {
        finish(&id, JobStatus::Cancelled, "отменена до старта", None, None);
        return;
    }

    set_status(&id, JobStatus::Running, 10, "резолв движка");
    let engine_dir = config::engine_dir(&settings, &config_dir);
    let models_dir = config::models_dir(&settings, &config_dir);
    let output_dir = config::output_dir(&settings, &config_dir);

    let work = async {
        let (exe, backend) = engine::resolve_server_exe(&engine_dir, &settings.backend)?;
        let registry = crate::models::load_registry()?;
        let model = registry
            .iter()
            .find(|m| m.id == req.model_id)
            .cloned()
            .ok_or_else(|| format!("модель '{}' не найдена в реестре аудио-моделей", req.model_id))?;
        if !crate::models::is_installed(&models_dir, &model) {
            return Err(format!(
                "модель '{}' не установлена (нет .gguf в {})",
                req.model_id,
                models_dir.join(&model.weights_dir).display()
            ));
        }

        // Какие модели доступны серверу: все установленные (реестр ∩ диск),
        // чтобы server.json был полным независимо от текущей задачи.
        let installed: Vec<(crate::models::ModelInfo, PathBuf)> = registry
            .iter()
            .filter(|m| crate::models::is_installed(&models_dir, m))
            .filter_map(|m| crate::models::weights_gguf_path(&models_dir, m).map(|p| (m.clone(), p)))
            .collect();
        if installed.is_empty() {
            return Err("ни одна модель не установлена (см. Шаг 5 плана: веса в models_dir)".to_string());
        }

        set_status(&id, JobStatus::Running, 15, "конфиг сервера");
        let port = engine::pick_free_port().map_err(|e| e.to_string())?;
        let config_path = engine_dir.join("audiocpp_server.generated.json");
        engine::write_server_config(&config_path, port, &backend, &installed)?;

        set_status(&id, JobStatus::Running, 20, "запуск сервера");
        let server = engine::spawn_server(&engine_dir, &exe, &backend, port, &config_path)?;
        CURRENT_SERVER_PID.store(server.pid, Ordering::Relaxed);

        let result = async {
            engine::wait_health(&server.base_url(), std::time::Duration::from_secs(engine::HEALTH_TIMEOUT_SECS)).await?;
            if CANCEL_REQUESTED.load(Ordering::Relaxed) {
                return Err(CANCELLED_MSG.to_string());
            }

            set_status(&id, JobStatus::Running, 40, "генерация (нейросеть)");
            let wav_bytes = engine::generate_wav(&server.base_url(), &req.model_id, &req.prompt, request_options(&model, &req))
                .await?;

            set_status(&id, JobStatus::Running, 85, "постобработка WAV");
            let loop_secs = if req.make_loop { settings.loop_crossfade_secs } else { 0.0 };
            let (processed, dur) = wav::postprocess(&wav_bytes, settings.normalize, loop_secs)?;

            std::fs::create_dir_all(&output_dir).map_err(|e| format!("output_dir: {e}"))?;
            let file_name = match req.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                Some(n) => sanitize_name(n),
                None => format!("gen_{}", chrono::Local::now().format("%Y%m%d_%H%M%S")),
            };
            let out_path = unique_path(&output_dir, &file_name);
            // Бинарный WAV пишем напрямую (json-store — для JSON-записей);
            // JSON-реестр клипов ниже — через json-store (fs::write запрещён для JSON).
            std::fs::write(&out_path, &processed).map_err(|e| format!("запись WAV: {e}"))?;
            register_clip(&output_dir, &out_path, &req, dur)?;

            Ok((out_path, dur))
        }
        .await;

        CURRENT_SERVER_PID.store(0, Ordering::Relaxed);
        drop(server); // Drop = unregister PID + kill дерева (сервер одноразовый на задачу)
        result
    };

    match work.await {
        Ok((path, dur)) => {
            let p = path.to_string_lossy().to_string();
            finish(&id, JobStatus::Done, "готово", Some(p), Some(dur));
        }
        // Отмена может прийти и посреди генерации (сервер убит → ошибка связи):
        // тогда флаг выставлен — классифицируем как Cancelled, а не Failed.
        Err(e) if e == CANCELLED_MSG || CANCEL_REQUESTED.swap(false, Ordering::Relaxed) => {
            finish(&id, JobStatus::Cancelled, "отменена", None, None)
        }
        Err(e) => {
            log::error!("audio-gen: задача {id} упала: {e}");
            finish(&id, JobStatus::Failed, &e, None, None);
        }
    }
}

const CANCELLED_MSG: &str = "__cancelled__";

/// Опции запроса по семейству модели. Имена полей СВЕРЕНЫ с model_specs
/// движка v0.9.5 (moss_soundeffect.json / stable_audio.json / ace_step.json):
/// - duration_sec — у всех семейств (дефолт MOSS 10, SA3 120(!), ACE -1);
/// - num_steps — только MOSS; num_inference_steps — SA3/ACE;
/// - seed — у всех;
/// - truncate_output_to_duration (SA3) — обрезать хвост паддинга (дефолт
///   duration_padding_sec 6 с) — для игровых SFX нужна точная длина.
/// Дефолт длительности: SA3-дефолт 120 с расточителен для SFX → 10 с,
/// если юзер не задал явно.
fn request_options(model: &crate::models::ModelInfo, req: &GenerateRequest) -> serde_json::Map<String, serde_json::Value> {
    let mut o = serde_json::Map::new();
    // Дефолт 10 с для всех семейств (SA3-дефолт 120 с расточителен для SFX).
    let duration = req.duration_secs.unwrap_or(10.0);
    o.insert("duration_sec".to_string(), serde_json::json!(duration));
    match model.family.as_str() {
        "moss_soundeffect" => {
            if let Some(s) = req.steps {
                o.insert("num_steps".to_string(), serde_json::json!(s));
            }
        }
        _ => {
            if let Some(s) = req.steps {
                o.insert("num_inference_steps".to_string(), serde_json::json!(s));
            }
            if model.family == "stable_audio" {
                o.insert("truncate_output_to_duration".to_string(), serde_json::json!(true));
            }
        }
    }
    if let Some(sd) = req.seed {
        o.insert("seed".to_string(), serde_json::json!(sd));
    }
    o
}

fn set_status(id: &str, status: JobStatus, progress: u8, message: &str) {
    if let Some(rec) = jobs().lock().unwrap().get_mut(id) {
        rec.status = status;
        rec.progress = progress;
        rec.message = message.to_string();
    }
    notify(id, status.as_str(), progress);
}

fn finish(
    id: &str,
    status: JobStatus,
    message: &str,
    output_path: Option<String>,
    duration_secs: Option<f32>,
) {
    let progress = if status == JobStatus::Done { 100 } else { 0 };
    if let Some(rec) = jobs().lock().unwrap().get_mut(id) {
        rec.status = status;
        rec.progress = progress;
        rec.message = message.to_string();
        rec.output_path = output_path;
        rec.duration_secs = duration_secs;
        rec.finished_at = Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
    }
    notify(id, status.as_str(), progress);
}

/// Отмена: флаг + kill дерева PID сервера (engine-proc-util, правило хостов —
/// убийство только по PID/дереву, не по имени образа).
pub fn cancel(job_id: &str) -> bool {
    CANCEL_REQUESTED.store(true, Ordering::Relaxed);
    let pid = CURRENT_SERVER_PID.swap(0, Ordering::Relaxed);
    if pid != 0 {
        engine_proc_util::kill_pid_tree(pid);
    }
    let exists = jobs().lock().unwrap().contains_key(job_id);
    if exists {
        let done = {
            let mut map = jobs().lock().unwrap();
            if let Some(rec) = map.get_mut(job_id) {
                if matches!(rec.status, JobStatus::Done | JobStatus::Failed) {
                    true
                } else {
                    rec.status = JobStatus::Cancelled;
                    rec.message = "отменена юзером".to_string();
                    false
                }
            } else {
                true
            }
        };
        if !done {
            notify(job_id, "cancelled", 0);
        }
        true
    } else {
        false
    }
}

/// Снимок записи задачи.
pub fn job_info(job_id: &str) -> Option<JobRecord> {
    jobs().lock().unwrap().get(job_id).cloned()
}

/// Все задачи (новые первыми — по id-префиксу времени работает и так).
pub fn jobs_snapshot() -> Vec<JobRecord> {
    let mut v: Vec<JobRecord> = jobs().lock().unwrap().values().cloned().collect();
    v.sort_by(|a, b| b.id.cmp(&a.id));
    v
}

/// Санитизация имени файла: буквы/цифры/дефис/подчёркивание, остальное — '_'.
fn sanitize_name(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    out.truncate(64);
    if out.is_empty() {
        out = "clip".to_string();
    }
    out
}

/// Уникальный путь: `name.wav`, при занятости — `name_2.wav`, `name_3.wav`...
fn unique_path(dir: &PathBuf, stem: &str) -> PathBuf {
    let mut path = dir.join(format!("{stem}.wav"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}_{n}.wav"));
        n += 1;
    }
    path
}

/// Реестр сгенерированных клипов: `<output_dir>/clips.json` (через json-store).
fn register_clip(
    output_dir: &PathBuf,
    out_path: &PathBuf,
    req: &GenerateRequest,
    dur: f32,
) -> Result<(), String> {
    #[derive(Serialize, Deserialize)]
    struct ClipEntry {
        file: String,
        model_id: String,
        prompt: String,
        duration_secs: f32,
        created_at: String,
    }
    let index_path = output_dir.join("clips.json");
    let mut clips: Vec<ClipEntry> = match json_store::read_json(&index_path) {
        Ok(v) => v,
        Err(_) => Vec::new(), // файла нет (первый клип) — начинаем с пустого
    };
    clips.push(ClipEntry {
        file: out_path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        model_id: req.model_id.clone(),
        prompt: req.prompt.clone(),
        duration_secs: dur,
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    });
    let text = serde_json::to_string_pretty(&clips).map_err(|e| e.to_string())?;
    json_store::write_atomic(&index_path, &text).map_err(|e| e.to_string())
}
