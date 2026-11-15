//! Тонкий мост ядра `ytdlp-core` (ССОТ, Шаг 4d PRP 261115): вся логика живёт
//! в `core/` (крейт ytdlp-core, без tauri). Путь `plugin_ytdlp_wrapper::engine::*`
//! сохранён для хостов (KO infra/tools/youtube.rs: engine::subtitles;
//! lib.rs фасада: engine::downloader::kill_active_downloads).

use std::path::PathBuf;
use tauri::Manager;

pub use ytdlp_core::{net_errors, subtitles};
pub use ytdlp_core::{YtdlpConfig, YtdlpStatus};

pub mod downloader {
    use std::path::PathBuf;

    pub use ytdlp_core::downloader::{cancel_active, fetch_info, kill_active_downloads};

    /// Прежняя команда-сигнатура: мост в ядро (data_dir из AppHandle + хук,
    /// эмитящий те же события `ytdlp:download-*`, что и раньше).
    #[allow(clippy::too_many_arguments)]
    pub async fn start_download(
        app: tauri::AppHandle,
        dir: PathBuf,
        url: String,
        format: String,
        quality: String,
        output_dir: String,
        auth_mode: String,
        browser: Option<String>,
        cookies_file: Option<String>,
        proxy: Option<String>,
    ) -> Result<(), String> {
        use std::sync::Arc;
        use ytdlp_core::downloader::{start_download as core_start, YtdlpEvent, YtdlpEventHook};

        let data_dir = super::data_dir(&app);
        let handle = app.clone();
        let on_event: YtdlpEventHook = Arc::new(move |e: &YtdlpEvent| {
            use tauri::Emitter;
            match e {
                YtdlpEvent::Progress(pct) => {
                    let _ = handle.emit("ytdlp:download-progress", pct);
                }
                YtdlpEvent::Status(s) => {
                    let _ = handle.emit("ytdlp:download-status", s.clone());
                }
                YtdlpEvent::Cancelled => {
                    let _ = handle.emit("ytdlp:download-cancelled", ());
                }
                YtdlpEvent::Error(msg) => {
                    let _ = handle.emit("ytdlp:download-error", msg.clone());
                }
                YtdlpEvent::Finished => {
                    let _ = handle.emit("ytdlp:download-finished", ());
                }
            }
        });
        core_start(
            data_dir,
            dir,
            url,
            format,
            quality,
            output_dir,
            auth_mode,
            browser,
            cookies_file,
            proxy,
            Some(on_event),
        )
        .await
    }
}

pub mod ytdlp_manager {
    use std::path::Path;
    use super::YtdlpStatus;

    pub use ytdlp_core::ytdlp_manager::{
        check_update, default_dir, get_deno_path, get_exe_path, get_ffmpeg_path, get_version,
        install_deno, install_or_update, is_installed,
    };

    /// Прежняя сигнатура: data_dir достаётся из AppHandle (в ядре — параметр).
    pub fn get_status(dir: &Path, app: &tauri::AppHandle) -> Result<YtdlpStatus, String> {
        ytdlp_core::ytdlp_manager::get_status(dir, &super::data_dir(app))
    }
}

/// Обёртки с прежней сигнатурой: команды фасада не меняются.
pub fn load_config(app: &tauri::AppHandle) -> YtdlpConfig {
    ytdlp_core::load_config(&data_dir(app))
}

pub fn save_config(app: &tauri::AppHandle, cfg: &YtdlpConfig) -> Result<(), String> {
    ytdlp_core::save_config(&data_dir(app), cfg)
}

/// Папка данных хоста (в ядре — параметр `data_dir`).
pub fn data_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}
