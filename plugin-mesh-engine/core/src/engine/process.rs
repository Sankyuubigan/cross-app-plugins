//! Жизненный цикл `trellis-server.exe` (паттерн `image_engine.rs`): спавн
//! на случайном порту диапазона mesh-движка, ожидание `GET /health`,
//! `Drop` → kill дерева + снятие PID. Kill — только PID/Job Object
//! (SSOT `engine-proc-util`); убийство по имени образа запрещено (§0.3).
//!
//! Аутентификации у сервера нет (верифицировано по исходникам) — изоляция =
//! bind 127.0.0.1 + случайный порт из диапазона, не пересекающегося с
//! image/video-движками.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use engine_proc_util::{
    assign_child_to_kill_job, kill_process_tree, register_engine_pid, unregister_engine_pid,
};

use super::presets::MeshPreset;

/// Диапазон портов mesh-движка: НЕ пересекается с image/video (19400–20900).
pub const PORT_MIN: u16 = 21500;
pub const PORT_RANGE: u16 = 1500;
/// Первый старт читает GGUF с диска — таймаут /health щедрый.
pub const HEALTH_TIMEOUT: Duration = Duration::from_secs(300);
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Свободный порт в диапазоне mesh-движка (bind-проба 127.0.0.1).
pub fn pick_free_port() -> Option<u16> {
    for offset in 0..PORT_RANGE {
        let port = PORT_MIN + offset;
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Some(port);
        }
    }
    None
}

/// Параметры спавна trellis-server.
pub struct SpawnConfig<'a> {
    /// Полный путь к trellis-server.exe.
    pub exe: &'a Path,
    /// Папка GGUF-весов (`--models`).
    pub models_dir: &'a Path,
    /// Семейство модели (`--model trellis|pixal3d`).
    pub model: MeshPreset,
    /// `--backend` (Vulkan|CUDA|CPU|HTP…); None → выбор движка.
    pub backend: Option<&'a str>,
    /// `--gpu N` — индекс GPU; None → дефолт движка.
    pub gpu: Option<i32>,
}

/// Живой trellis-server. `Drop` убивает дерево процессов и снимает PID.
pub struct MeshEngine {
    child: Child,
    port: u16,
}

impl MeshEngine {
    /// Спавн + ожидание /health. При любой ошибке процесс убивается (Drop).
    pub fn spawn(cfg: &SpawnConfig) -> Result<Self, String> {
        let port = pick_free_port().ok_or_else(|| {
            format!("нет свободного порта в диапазоне {PORT_MIN}–{}/TCP", PORT_MIN + PORT_RANGE - 1)
        })?;

        let mut cmd = Command::new(cfg.exe);
        cmd.arg("--models")
            .arg(cfg.models_dir)
            .arg("--model")
            .arg(cfg.model.as_flag())
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string());
        if let Some(backend) = cfg.backend {
            cmd.arg("--backend").arg(backend);
        }
        if let Some(gpu) = cfg.gpu {
            cmd.arg("--gpu").arg(gpu.to_string());
        }
        apply_no_window(&mut cmd);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        log::info!(
            "[mesh] спавн {}: --models {} --model {} --port {}",
            cfg.exe.display(),
            cfg.models_dir.display(),
            cfg.model.as_flag(),
            port
        );

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("не удалось запустить {}: {e}", cfg.exe.display()))?;
        register_engine_pid(child.id());
        assign_child_to_kill_job(&child);
        pipe_logs(&mut child);

        let engine = MeshEngine { child, port };
        if let Err(e) = engine.wait_health() {
            drop(engine); // Drop убьёт процесс и снимет PID
            return Err(e);
        }
        Ok(engine)
    }

    /// Базовый URL живого сервера.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Порт, на котором слушает сервер (диагностика).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Ожидание `GET /health` → 2xx ("ok", верифицированный контракт).
    fn wait_health(&self) -> Result<(), String> {
        let deadline = Instant::now() + HEALTH_TIMEOUT;
        let client = reqwest::blocking::Client::builder()
            .timeout(POLL_INTERVAL)
            .build()
            .map_err(|e| format!("[mesh] HTTP-клиент: {e}"))?;
        let url = format!("{}/health", self.base_url());
        loop {
            if let Ok(resp) = client.get(&url).send() {
                if resp.status().is_success() {
                    return Ok(());
                }
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "[mesh] trellis-server не ответил на /health за {} сек. Первый старт \
                     читает GGUF-веса с диска — это может занимать минуты; проверь лог \
                     движка ([trellis-server] …) и свободное место",
                    HEALTH_TIMEOUT.as_secs()
                ));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}

/// CREATE_NO_WINDOW: консоль движка не мелькает (прецедент image_engine.rs).
fn apply_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Поток-переносчик stdout/stderr в лог: диагностика краша без консоли.
fn pipe_logs(child: &mut Child) {
    if let Some(out) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                log::info!("[trellis-server] {line}");
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                log::info!("[trellis-server] {line}");
            }
        });
    }
}

impl Drop for MeshEngine {
    fn drop(&mut self) {
        log::info!("[mesh] остановка trellis-server (pid {})", self.child.id());
        kill_process_tree(&mut self.child);
        unregister_engine_pid(self.child.id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_port_is_inside_mesh_range() {
        let port = pick_free_port().expect("должен найтись свободный порт");
        assert!(port >= PORT_MIN && port < PORT_MIN + PORT_RANGE, "{port}");
    }
}
