//! Управление движком audio.cpp (`audiocpp_server`) как подпроцессом + HTTP-генерация.
//!
//! Эндпоинты сверены с официальным README `app/server/README.md` (v0.9.x):
//! - `GET /health` — готовность (lazy-модель при этом ещё не загружена);
//! - `POST /v1/audio/speech` — JSON `{model, input, options}` → WAV-байты;
//! - `GET /v1/models` — список сконфигурированных моделей.
//!
//! Схема server.json СГЕНЕРИРОВАНА ядром и сверена с официальными примерами
//! `app/server/config-examples` (kokoro.json) и README движка: host/port/backend/
//! device/threads/lazy_load/max_loaded_models + models[{id, family, path, task}].
//! Пути Windows — с прямыми слэшами (требование README конфига). Имена request-
//! опций сверены с model_specs движка: moss_soundeffect (duration_sec/num_steps),
//! stable_audio (duration_sec/num_inference_steps), ace_step (num_inference_steps).
//!
//! Процессы — через engine-proc-util: PID-реестр, taskkill /F /T по дереву,
//! Windows Job Object KILL_ON_JOB_CLOSE (сироты при падении хоста исключены).

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Дефолтный хост сервера — только loopback, наружу не слушаем.
pub const SERVER_HOST: &str = "127.0.0.1";

/// Ждать готовности /health после спавна (холодный старт движка недолог,
/// но на CPU-машине может занять пару секунд).
pub const HEALTH_TIMEOUT_SECS: u64 = 30;

/// Таймаут HTTP-запроса генерации: SA3 Medium — секунды, MOSS — до ~15 с на клип,
/// плюс загрузка модели при lazy_load. Запас — 10 минут.
const GENERATE_TIMEOUT_SECS: u64 = 600;

#[derive(Debug)]
pub struct EngineHandle {
    pub child: Child,
    pub pid: u32,
    pub port: u16,
    pub backend: String,
}

impl EngineHandle {
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", SERVER_HOST, self.port)
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        engine_proc_util::unregister_engine_pid(self.pid);
        engine_proc_util::kill_process_tree(&mut self.child);
    }
}

/// Разрешить бэкенд и найти exe сервера.
///
/// Порядок просмотра (правило спец-доки: CUDA надёжен, Vulkan на Windows
/// нестабилен на эффектах — его не используем вовсе):
/// 1. `pref == "cuda"|"cpu"` — только он; `pref == "auto"` — cuda, затем cpu.
/// 2. Раскладки: `<engine_dir>/<backend>/audiocpp_server.exe` и плоская
///    `<engine_dir>/audiocpp_server.exe` (если бэкенд один).
pub fn resolve_server_exe(engine_dir: &Path, backend_pref: &str) -> Result<(PathBuf, String), String> {
    let order: &[&str] = match backend_pref {
        "cuda" => &["cuda"],
        "cpu" => &["cpu"],
        _ => &["cuda", "cpu"],
    };
    for backend in order {
        let nested = engine_dir.join(backend).join("audiocpp_server.exe");
        if nested.exists() {
            return Ok((nested, backend.to_string()));
        }
    }
    let flat = engine_dir.join("audiocpp_server.exe");
    if flat.exists() {
        // Плоская раскладка: считаем бэкенд по настройке (auto → cpu как консервативный).
        let backend = if backend_pref == "cuda" { "cuda" } else { "cpu" };
        return Ok((flat, backend.to_string()));
    }
    Err(format!(
        "audiocpp_server.exe не найден в {} (искал <backend>/audiocpp_server.exe для {}). \
         Установите движок audio.cpp (Шаг 5 плана: скачивание через downloader-core).",
        engine_dir.display(),
        order.join("/")
    ))
}

/// Свободный TCP-порт на loopback (bind :0 → посмотреть номер → отпустить).
pub fn pick_free_port() -> std::io::Result<u16> {
    std::net::TcpListener::bind((SERVER_HOST, 0))?.local_addr().map(|a| a.port())
}

/// Сгенерировать конфиг сервера (server.json) для запуска.
///
/// Схема сверена с официальными примерами `app/server/config-examples` (kokoro.json)
/// и README движка. `installed` — пары (модель, путь к .gguf) всех установленных
/// моделей; сервер получает их все, а задачу выбирает по запросу.
pub fn write_server_config(
    config_path: &Path,
    port: u16,
    backend: &str,
    installed: &[(crate::models::ModelInfo, PathBuf)],
) -> Result<(), String> {
    let models: Vec<serde_json::Value> = installed
        .iter()
        .map(|(m, gguf)| {
            serde_json::json!({
                "id": m.id,
                "family": m.family,
                "path": gguf.to_string_lossy().replace('\\', "/"),
                // Код задачи сервера: "gen" (проверено живым сервером v0.9.5:
                // "sfx"/"music" отвергаются — допустимы vad/asr/diar/sep/gen/tts/...).
                "task": "gen",
            })
        })
        .collect();
    let cfg = serde_json::json!({
        "host": SERVER_HOST,
        "port": port,
        "backend": backend,
        "device": 0,
        "threads": 4,
        "lazy_load": true,
        "max_loaded_models": 1,
        "models": models,
    });
    let text = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(config_path, text).map_err(|e| format!("запись server.json: {e}"))
}

/// Запустить audiocpp_server с нашим сгенерированным конфигом.
///
/// `engine_dir` = cwd сервера (там же оседает audiocpp_server.log).
pub fn spawn_server(
    engine_dir: &Path,
    exe: &Path,
    backend: &str,
    port: u16,
    config_path: &Path,
) -> Result<EngineHandle, String> {
    let mut cmd = Command::new(exe);
    cmd.current_dir(engine_dir)
        .arg("--config")
        .arg(config_path)
        .arg("--log")
        .arg("--log-file")
        .arg("audiocpp_server.log");
    // Без CREATE_NO_WINDOW сервер мелькает чёрным консольным окном (правило хостов).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0_0800_0000);
    }
    let child = cmd.spawn().map_err(|e| format!("не удалось запустить {}: {e}", exe.display()))?;
    let pid = child.id();
    engine_proc_util::register_engine_pid(pid);
    engine_proc_util::assign_child_to_kill_job(&child);
    log::info!("audio.cpp сервер запущен: {} (backend {backend}, pid {pid}, port {port})", exe.display());
    Ok(EngineHandle { child, pid, port, backend: backend.to_string() })
}

/// Ждать `GET /health` до готовности (поллинг TCP + HTTP-проверка).
pub async fn wait_health(base_url: &str, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let client = reqwest::Client::new();
    loop {
        if Instant::now() > deadline {
            return Err(format!("сервер не ответил на /health за {} с", timeout.as_secs()));
        }
        // Сначала ждём, пока порт вообще откроется (дёшево, без HTTP).
        let host = base_url.trim_start_matches("http://");
        if TcpStream::connect(host).is_ok() {
            if let Ok(resp) = client.get(format!("{base_url}/health")).send().await {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Сгенерировать аудио: `POST /v1/audio/speech {model, input, options}` → WAV-байты.
///
/// `options` — модель-специфичные параметры (имена полей зависят от семейства:
/// duration_sec/num_steps у MOSS, duration_sec/num_inference_steps у SA3/ACE —
/// собираются в job.rs по model_specs движка).
pub async fn generate_wav(
    base_url: &str,
    model_id: &str,
    prompt: &str,
    options: serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<u8>, String> {
    let mut body = serde_json::json!({
        "model": model_id,
        "input": prompt,
    });
    if !options.is_empty() {
        if let Some(obj) = body.as_object_mut() {
            obj.insert("options".to_string(), serde_json::Value::Object(options));
        }
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(GENERATE_TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("{base_url}/v1/audio/speech"))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("запрос генерации не прошёл: {e}"))?;

    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = resp.bytes().await.map_err(|e| format!("тело ответа не прочитано: {e}"))?;

    if !status.is_success() {
        let snippet = String::from_utf8_lossy(&bytes[..bytes.len().min(400)]);
        return Err(format!("сервер вернул {status}: {snippet}"));
    }
    // README движка: неудачный запрос может вернуть JSON-тело вместо WAV —
    // проверяем content-type, а не только статус.
    if !content_type.starts_with("audio/") && !content_type.is_empty() {
        let snippet = String::from_utf8_lossy(&bytes[..bytes.len().min(400)]);
        return Err(format!("сервер вернул не WAV ({content_type}): {snippet}"));
    }
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_exe_missing_dir_is_clear_error() {
        let err = resolve_server_exe(Path::new("Z:/no/such/dir"), "auto").unwrap_err();
        assert!(err.contains("audiocpp_server.exe не найден"));
    }

    #[test]
    fn pick_free_port_returns_open_port() {
        let port = pick_free_port().expect("порт должен выделяться");
        assert!(port > 0);
    }
}
