//! # downloader-core — единый движок скачивания проекта (ССОТ, без Tauri).
//!
//! Многоуровневый движок с прогрессом (скорость/ETA), stall-детектом,
//! resume и отменой. Уровни (следующий — если предыдущий не сработал):
//!   1. reqwest (multi-chunk при Range, stall 60s, ретраи, редиректы GitHub→S3)
//!   2. curl (libcurl crate: low-speed abort, resume, прогресс)
//!   3. PowerShell Invoke-WebRequest (скрытое окно, таймаут, поллинг размера)
//!   4. bitsadmin (WinINet, скрытое окно, таймаут, поллинг)
//!   5. MCP/Deno downloader.ts (скрытое окно, таймаут, поллинг)
//!   6. Chrome CDP (скрытое окно, таймаут, поллинг)
//!
//! Без Range/Accept-Ranges сервер отдаёт файл однопоточным потоком (уровень 1).
//!
//! Прогресс/эмит — инъекцией хуков (ядро не знает про Tauri):
//! - [`set_emit_hook`] — полный payload [`ProgressPayload`] (фасад плагина
//!   эмитит событие `downloader:progress` прежнего формата);
//! - [`set_progress_hook`] — единый минимальный контракт [`DownloadProgress`]
//!   (GPUI-хосты, about-updates, llama-engine и т.п.).
//!
//! ## Rust API (для других крейтов/плагинов)
//! ```ignore
//! use downloader_core::{download, download_blocking, DownloadOptions};
//! download(url, dest, opts, None).await?;
//! download_blocking(url, dest, opts, Some(&log_cb))?;
//! ```

pub mod api;
pub mod chain;
pub mod chrome_cdp;
pub mod curl_dl;
pub mod process_dl;
pub mod progress;
pub mod reqwest_dl;

pub use api::{
    download, download_blocking, download_bytes_blocking, download_bytes_to_vec, DownloadOptions,
};
pub use progress::{
    cancel, cancel_all, emit_external, eta_seconds, list_active, set_emit_hook, set_progress_hook,
    DownloadProgress, EmitHook, ProgressHook, ProgressPayload, TaskHandle, PROGRESS_EVENT,
};
