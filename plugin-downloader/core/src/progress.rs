//! Реестр активных загрузок: task_id, отмена, инъекция хуков прогресса/эмита.
//!
//! Ядро БЕЗ Tauri. Хост (Tauri-фасад плагина или GPUI-приложение) ставит
//! колбэки вместо статик AppHandle:
//! - [`set_emit_hook`] — полный payload события (фасад эмитит `downloader:progress`);
//! - [`set_progress_hook`] — единый минимальный прогресс [`DownloadProgress`].

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

/// Имя единого события прогресса (слушают все UI-компоненты).
pub const PROGRESS_EVENT: &str = "downloader:progress";

/// Единый тип прогресса скачивания (ССОТ для всех хостов проекта).
/// Полный payload события вебвью — [`ProgressPayload`]; этот — минимальный контракт.
#[derive(Clone, Debug, Serialize)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    /// Известный полный размер; None — сервер не отдал Content-Length.
    pub total_bytes: Option<u64>,
    /// Скорость, байт/сек (0 = нет данных).
    pub speed_bps: f64,
}

/// Колбэк минимального прогресса (GPUI-хосты, about-updates и т.п.).
pub type ProgressHook = Arc<dyn Fn(&DownloadProgress) + Send + Sync>;

/// Колбэк полного payload-а события (Tauri-фасад эмитит его в вебвью).
pub type EmitHook = Arc<dyn Fn(&ProgressPayload) + Send + Sync>;

#[derive(Clone, Debug, Serialize)]
pub struct ProgressPayload {
    pub task_id: String,
    /// Человекочитаемая метка: «Движок llama.cpp», «Модель», …
    pub label: String,
    /// Логическая категория: "engine" | "model" | "mmproj" | "app" | "bin" | "9router" | "file"
    pub kind: String,
    pub url: String,
    pub dest: String,
    pub downloaded: u64,
    pub total: u64,
    pub speed_bps: f64,
    /// Оценка оставшегося времени, сек; < 0 — неизвестно.
    pub eta_s: f64,
    /// Текущий уровень цепочки (1..=6).
    pub level: u32,
    pub level_name: String,
    pub status: String, // "running" | "done" | "error" | "cancelled"
    pub message: String,
}

/// ETA, сек: (total − downloaded) / speed, округлённое; −1.0 — неизвестно.
pub fn eta_seconds(speed_bps: f64, downloaded: u64, total: u64) -> f64 {
    if speed_bps > 1.0 && total > downloaded {
        ((total - downloaded) as f64 / speed_bps).round()
    } else {
        -1.0
    }
}

struct TaskEntry {
    cancel: Arc<AtomicBool>,
    /// PID внешнего процесса (уровни 3..=6), если запущен.
    child_pid: Mutex<Option<u32>>,
    started: Instant,
    label: String,
    kind: String,
    url: String,
    dest: String,
}

static EMIT_HOOK: OnceLock<Mutex<Option<EmitHook>>> = OnceLock::new();
static PROGRESS_HOOK: OnceLock<Mutex<Option<ProgressHook>>> = OnceLock::new();
static TASKS: OnceLock<Mutex<HashMap<String, TaskEntry>>> = OnceLock::new();

/// Poison-устойчивый lock: упавший колбэк хоста не ломает реестр навсегда.
fn lock<T>(cell: &Mutex<T>) -> MutexGuard<'_, T> {
    cell.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn tasks() -> &'static Mutex<HashMap<String, TaskEntry>> {
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Установить/снять полный emit-колбэк (фасад плагина: app.emit(...)).
/// Повторный вызов заменяет колбэк.
pub fn set_emit_hook(hook: Option<EmitHook>) {
    *lock(EMIT_HOOK.get_or_init(|| Mutex::new(None))) = hook;
}

/// Установить/снять колбэк минимального прогресса. Повторный вызов заменяет.
pub fn set_progress_hook(hook: Option<ProgressHook>) {
    *lock(PROGRESS_HOOK.get_or_init(|| Mutex::new(None))) = hook;
}

/// Развести payload по обоим хукам (emit — прежний формат, progress — единый тип).
fn dispatch(payload: &ProgressPayload) {
    if let Some(hook) = EMIT_HOOK.get().and_then(|c| lock(c).clone()) {
        hook(payload);
    }
    let progress = DownloadProgress {
        downloaded_bytes: payload.downloaded,
        total_bytes: (payload.total > 0).then_some(payload.total),
        speed_bps: payload.speed_bps,
    };
    if let Some(hook) = PROGRESS_HOOK.get().and_then(|c| lock(c).clone()) {
        hook(&progress);
    }
}

/// Глобальный счётчик task_id.
static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub struct TaskHandle {
    pub id: String,
    pub cancel: Arc<AtomicBool>,
    pub label: String,
    pub kind: String,
    pub url: String,
    pub dest: String,
    pub started: Instant,
    level: std::sync::atomic::AtomicU32,
    level_name: Mutex<String>,
    last_bytes: Mutex<u64>,
    last_emit: Mutex<Instant>,
}

impl TaskHandle {
    pub fn new(label: &str, kind: &str, url: &str, dest: &str) -> Self {
        let id = format!("dl_{}", COUNTER.fetch_add(1, Ordering::Relaxed));
        let cancel = Arc::new(AtomicBool::new(false));
        let entry = TaskEntry {
            cancel: cancel.clone(),
            child_pid: Mutex::new(None),
            started: Instant::now(),
            label: label.to_string(),
            kind: kind.to_string(),
            url: url.to_string(),
            dest: dest.to_string(),
        };
        lock(tasks()).insert(id.clone(), entry);
        Self {
            id,
            cancel,
            label: label.to_string(),
            kind: kind.to_string(),
            url: url.to_string(),
            dest: dest.to_string(),
            started: Instant::now(),
            level: std::sync::atomic::AtomicU32::new(0),
            level_name: Mutex::new(String::new()),
            last_bytes: Mutex::new(0),
            last_emit: Mutex::new(Instant::now() - std::time::Duration::from_secs(1)),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    pub fn check_cancel(&self) -> Result<(), String> {
        if self.is_cancelled() {
            Err("Отменено".to_string())
        } else {
            Ok(())
        }
    }

    pub fn set_level(&self, level: u32, name: &str) {
        self.level.store(level, Ordering::Relaxed);
        *lock(&self.level_name) = name.to_string();
    }

    pub fn level(&self) -> u32 {
        self.level.load(Ordering::Relaxed)
    }

    pub fn set_child_pid(&self, pid: u32) {
        if let Some(entry) = lock(tasks()).get(&self.id) {
            *lock(&entry.child_pid) = Some(pid);
        }
    }

    /// Эмит прогресса. `force=false` — троттлинг до ~5/сек, `force=true` — всегда
    /// (для terminal-статусов done/error/cancelled).
    pub fn emit_progress(
        &self,
        downloaded: u64,
        total: u64,
        speed_bps: f64,
        message: &str,
        force: bool,
    ) {
        if !force {
            let mut last = lock(&self.last_emit);
            if last.elapsed() < std::time::Duration::from_millis(200) {
                return;
            }
            *last = Instant::now();
        }
        *lock(&self.last_bytes) = downloaded;
        let payload = ProgressPayload {
            task_id: self.id.clone(),
            label: self.label.clone(),
            kind: self.kind.clone(),
            url: self.url.clone(),
            dest: self.dest.clone(),
            downloaded,
            total,
            speed_bps,
            eta_s: eta_seconds(speed_bps, downloaded, total),
            level: self.level(),
            level_name: lock(&self.level_name).clone(),
            status: "running".into(),
            message: message.to_string(),
        };
        dispatch(&payload);
    }

    pub fn emit_terminal(&self, status: &str, message: &str, downloaded: u64, total: u64) {
        let payload = ProgressPayload {
            task_id: self.id.clone(),
            label: self.label.clone(),
            kind: self.kind.clone(),
            url: self.url.clone(),
            dest: self.dest.clone(),
            downloaded,
            total,
            speed_bps: 0.0,
            eta_s: -1.0,
            level: self.level(),
            level_name: lock(&self.level_name).clone(),
            status: status.into(),
            message: message.to_string(),
        };
        dispatch(&payload);
    }
}

impl Drop for TaskHandle {
    fn drop(&mut self) {
        lock(tasks()).remove(&self.id);
    }
}

/// Отменить конкретную задачу.
pub fn cancel(task_id: &str) -> bool {
    let map = lock(tasks());
    if let Some(entry) = map.get(task_id) {
        entry.cancel.store(true, Ordering::SeqCst);
        if let Some(pid) = *lock(&entry.child_pid) {
            kill_pid(pid);
        }
        true
    } else {
        false
    }
}

/// Отменить все активные (вызывается при ExitRequested).
pub fn cancel_all() {
    let map = lock(tasks());
    for entry in map.values() {
        entry.cancel.store(true, Ordering::SeqCst);
        if let Some(pid) = *lock(&entry.child_pid) {
            kill_pid(pid);
        }
    }
}

/// Снимок активных задач для команды list_active.
pub fn list_active() -> Vec<serde_json::Value> {
    let map = lock(tasks());
    map.iter()
        .map(|(id, e)| {
            serde_json::json!({
                "task_id": id,
                "label": e.label,
                "kind": e.kind,
                "url": e.url,
                "dest": e.dest,
                "elapsed_ms": e.started.elapsed().as_millis() as u64,
                "cancelled": e.cancel.load(Ordering::SeqCst),
            })
        })
        .collect()
}

/// Эмит из внешнего репортера (reqwest multi-chunk, TaskHandle не Clone):
/// все поля известны на месте вызова, уровень всегда 1.
pub fn emit_external(
    task_id: &str,
    label: &str,
    kind: &str,
    url: &str,
    dest: &str,
    downloaded: u64,
    total: u64,
    speed_bps: f64,
) {
    let payload = ProgressPayload {
        task_id: task_id.to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        url: url.to_string(),
        dest: dest.to_string(),
        downloaded,
        total,
        speed_bps,
        eta_s: eta_seconds(speed_bps, downloaded, total),
        level: 1,
        level_name: "reqwest".into(),
        status: "running".into(),
        message: String::new(),
    };
    dispatch(&payload);
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("taskkill");
    cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    let _ = cmd.output();
}

#[cfg(not(windows))]
fn kill_pid(pid: u32) {
    let _ = std::process::Command::new("kill").args(["-9", &pid.to_string()]).output();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Хуки — глобальные статики: hook-тесты обязаны идти серийно.
    static TEST_SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn eta_known_and_unknown() {
        assert_eq!(eta_seconds(10.0, 0, 100), 10.0); // 100/10
        assert_eq!(eta_seconds(7.5, 25, 100), 10.0); // 75/7.5 → округление
        assert_eq!(eta_seconds(0.0, 0, 100), -1.0); // нет скорости
        assert_eq!(eta_seconds(10.0, 100, 100), -1.0); // ничего не осталось
        assert_eq!(eta_seconds(10.0, 200, 100), -1.0); // downloaded > total
    }

    #[test]
    fn progress_hook_receives_unified_type() {
        let _g = TEST_SERIAL.lock().unwrap();
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        static LAST: OnceLock<Mutex<Option<DownloadProgress>>> = OnceLock::new();
        set_progress_hook(Some(Arc::new(|p: &DownloadProgress| {
            CALLS.fetch_add(1, Ordering::SeqCst);
            *LAST.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(p.clone());
        })));
        let task = TaskHandle::new("тест", "file", "http://x", "C:/tmp/x");
        task.set_level(2, "curl");
        task.emit_progress(40, 100, 12.0, "curl", true);
        let calls = CALLS.load(Ordering::SeqCst);
        let got = LAST.get().and_then(|m| m.lock().unwrap().clone());
        set_progress_hook(None);
        assert_eq!(calls, 1);
        let got = got.expect("хук получил прогресс");
        assert_eq!(got.downloaded_bytes, 40);
        assert_eq!(got.total_bytes, Some(100));
        assert_eq!(got.speed_bps, 12.0);
    }

    #[test]
    fn emit_hook_receives_full_payload() {
        let _g = TEST_SERIAL.lock().unwrap();
        static GOT: OnceLock<Mutex<Option<ProgressPayload>>> = OnceLock::new();
        set_emit_hook(Some(Arc::new(|p: &ProgressPayload| {
            *GOT.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(p.clone());
        })));
        let task = TaskHandle::new("модель", "model", "http://y", "C:/tmp/y");
        task.set_level(6, "Chrome CDP");
        task.emit_terminal("done", "Готово", 500, 500);
        let got = GOT.get().and_then(|m| m.lock().unwrap().clone());
        set_emit_hook(None);
        let got = got.expect("emit-хук получил payload");
        assert_eq!(got.status, "done");
        assert_eq!(got.label, "модель");
        assert_eq!(got.kind, "model");
        assert_eq!(got.level, 6);
        assert_eq!(got.level_name, "Chrome CDP");
        assert_eq!(got.total, 500);
    }

    #[test]
    fn without_hooks_no_panic() {
        let _g = TEST_SERIAL.lock().unwrap();
        // Ядро без хостов (тесты/CLI) обязано работать молча.
        let task = TaskHandle::new("тихо", "file", "http://z", "C:/tmp/z");
        task.emit_progress(1, 2, 0.0, "", true);
        task.emit_terminal("done", "ок", 2, 2);
    }

    #[test]
    fn cancel_registry_roundtrip() {
        let task = TaskHandle::new("cancel", "file", "http://c", "C:/tmp/c");
        assert!(!task.is_cancelled());
        assert!(cancel(&task.id), "задача найдена в реестре");
        assert!(task.is_cancelled());
        assert!(task.check_cancel().is_err());
        assert!(!cancel("dl_несуществующий"));
        assert!(list_active().iter().any(|v| v["task_id"] == task.id), "живая задача в реестре");
        drop(task);
        assert!(
            list_active().iter().all(|v| v["task_id"] != "cancel"),
            "Drop снял задачу"
        );
    }
}
