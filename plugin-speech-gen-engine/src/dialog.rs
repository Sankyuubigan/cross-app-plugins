use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use reqwest::Client;
use tauri::{AppHandle, Emitter, Runtime};

use crate::dialog_settings::DialogSettings;
use crate::process_util::{kill_process_tree, JobGuard};

/// Время ожидания подъёма HTTP сервера speech.cpp. Может включать прогрев
/// модели на диске, поэтому щедрее, чем у CrispASR.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(300);

/// Состояние диалогового движка.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogStatus {
    Stopped,
    Starting,
    Ready,
    Error(String),
}

/// Движок полного дуплекса на базе speech.cpp (`speech serve <модель>`).
///
/// speech.cpp запускается как **отдельный prebuilt-процесс** (`speech.exe`) и
/// общается по HTTP/WebSocket localhost. Нативная линковка запрещена глобальным
/// правилом (§6.7).
///
/// Это движковая часть (spawn/stop/status/port). WS-клиент диалога
/// (`/v1/dialogue`), микрофон и воспроизведение — ответственность хоста.
pub struct DialogEngine {
    child: Mutex<Option<Child>>,
    job: Mutex<Option<JobGuard>>,
    port: Mutex<u16>,
    status: Mutex<DialogStatus>,
}

impl DialogEngine {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            job: Mutex::new(None),
            port: Mutex::new(0),
            status: Mutex::new(DialogStatus::Stopped),
        }
    }

    fn pick_port() -> Option<u16> {
        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        let port = listener.local_addr().ok()?.port();
        Some(port)
    }

    fn kill_now(mut child: Child, job: Option<JobGuard>) {
        if let Some(j) = &job {
            j.terminate();
        }
        kill_process_tree(&mut child);
    }

    fn reap(&self) {
        let child = self
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let job = self.job.lock().unwrap_or_else(|e| e.into_inner()).take();
        match child {
            Some(c) => Self::kill_now(c, job),
            None => drop(job),
        }
    }

    pub fn port(&self) -> u16 {
        *self.port.lock().unwrap()
    }

    pub fn status(&self) -> DialogStatus {
        self.status.lock().unwrap().clone()
    }

    fn emit_status<R: Runtime>(app: &AppHandle<R>, status: &DialogStatus) {
        let label = match status {
            DialogStatus::Stopped => "stopped",
            DialogStatus::Starting => "starting",
            DialogStatus::Ready => "ready",
            DialogStatus::Error(_) => "error",
        };
        let _ = app.emit("dialog-status", label);
    }

    /// Запускает `speech.exe serve <модель> --port N` и ждёт готовности HTTP.
    ///
    /// Готовность = ЛЮБОЙ HTTP-ответ на `/health` или `/`: эндпоинт здоровья
    /// между версиями speech.cpp отличается, а факт ответа HTTP уже означает,
    /// что сервер поднялся. Реальная готовность модели подтверждается позже —
    /// событием `started` в WS `/v1/dialogue` (см. хост).
    pub async fn ensure<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        settings: &DialogSettings,
    ) -> Result<u16, String> {
        let engine_exe = if settings.engine_exe.is_empty() {
            crate::download::resolve_speech_exe(&crate::tts_settings::load(app).engine_dir)
                .ok_or_else(|| {
                    "speech.exe не найден: скачай движок на вкладке «Диалог» \
                     или укажи путь в настройках"
                        .to_string()
                })?
                .to_string_lossy()
                .to_string()
        } else {
            settings.engine_exe.clone()
        };

        if engine_exe.is_empty() || !std::path::Path::new(&engine_exe).exists() {
            return Err(format!("speech.exe не найден: {engine_exe}"));
        }

        // Живой сервер с тем же назначением переиспользуем.
        if self.child.lock().unwrap().is_some() && *self.port.lock().unwrap() != 0 {
            return Ok(*self.port.lock().unwrap());
        }

        *self.status.lock().unwrap() = DialogStatus::Starting;
        Self::emit_status(app, &self.status.lock().unwrap());

        let port = if settings.port != 0 {
            settings.port
        } else {
            Self::pick_port()
                .ok_or_else(|| "не удалось подобрать свободный порт".to_string())?
        };

        let args = vec![
            "serve".to_string(),
            settings.model.clone(),
            "--port".to_string(),
            port.to_string(),
        ];

        crate::log::app_log(
            app,
            &format!("[dialog] запуск speech.cpp: {engine_exe} {}", args.join(" ")),
        );

        let mut cmd = Command::new(&engine_exe);
        cmd.args(&args);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        cmd.stdout(Stdio::null()).stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("не удалось запустить speech.exe: {e}"))?;

        let job = JobGuard::assign(&child);

        // stderr → лог хоста (попадёт в Logs tab и test/last_logs).
        let err_log = Arc::new(Mutex::new(Vec::<String>::new()));
        if let Some(stderr) = child.stderr.take() {
            let app_clone = app.clone();
            let err_log_clone = Arc::clone(&err_log);
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().flatten() {
                    crate::log::app_log(&app_clone, &format!("[dialog] {line}"));
                    if let Ok(mut buf) = err_log_clone.lock() {
                        if buf.len() < 200 {
                            buf.push(line);
                        }
                    }
                }
            });
        }

        let client = Client::new();
        let base = format!("http://127.0.0.1:{port}");
        let started = std::time::Instant::now();
        let mut ready = false;
        while started.elapsed() < HEALTH_TIMEOUT {
            if let Ok(Some(_)) = child.try_wait() {
                let log = err_log.lock().unwrap_or_else(|e| e.into_inner());
                let joined = log.join("\n");
                *self.status.lock().unwrap() =
                    DialogStatus::Error("процесс завершился".into());
                Self::emit_status(app, &self.status.lock().unwrap());
                return Err(format!("speech.exe завершился сразу после старта. Лог: {joined}"));
            }
            for path in ["/health", "/"] {
                if client
                    .get(format!("{base}{path}"))
                    .timeout(Duration::from_secs(2))
                    .send()
                    .await
                    .is_ok()
                {
                    ready = true;
                    break;
                }
            }
            if ready {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        if !ready {
            Self::kill_now(child, job);
            *self.status.lock().unwrap() = DialogStatus::Error("таймаут старта".into());
            Self::emit_status(app, &self.status.lock().unwrap());
            return Err(format!(
                "таймаут ожидания HTTP сервера speech.cpp ({:?})",
                HEALTH_TIMEOUT
            ));
        }

        *self.child.lock().unwrap() = Some(child);
        *self.job.lock().unwrap() = job;
        *self.port.lock().unwrap() = port;
        *self.status.lock().unwrap() = DialogStatus::Ready;
        Self::emit_status(app, &self.status.lock().unwrap());

        crate::log::app_log(app, &format!("[dialog] speech.cpp готов на порту {port}"));

        Ok(port)
    }

    pub async fn stop<R: Runtime>(&self, app: &AppHandle<R>) {
        self.reap();
        *self.port.lock().unwrap() = 0;
        *self.status.lock().unwrap() = DialogStatus::Stopped;
        Self::emit_status(app, &self.status.lock().unwrap());
        crate::log::app_log(app, "[dialog] speech.cpp остановлен");
    }
}

impl Default for DialogEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DialogEngine {
    fn drop(&mut self) {
        self.reap();
    }
}
