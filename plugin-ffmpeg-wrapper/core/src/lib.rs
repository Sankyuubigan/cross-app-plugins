//! ffmpeg-core — чистое ядро плагина `plugin-ffmpeg-wrapper` (SSOT,
//! Шаг 4c PRP 261115_universal_plugins). Без Tauri: установка официального
//! билда FFmpeg (автоскачивание через единый движок downloader-core), запуск
//! subprocess (без shell, CREATE_NO_WINDOW), метаданные через ffprobe,
//! stdin-pipe runner.
//!
//! Публичный API для нативных тулов хоста — функции принимают каталоги
//! параметрами, без продуктовых имён (host-специфика — `dir: &Path`).

pub mod installer;
pub mod pipe;
pub mod probe;
pub mod runner;

pub use installer::{
    ensure_ffmpeg, ffmpeg_dir, ffmpeg_exe, ffprobe_exe, is_installed, tool_version,
    FFMPEG_DIR_NAME,
};
pub use pipe::FfmpegPipe;
pub use probe::probe_json;
pub use runner::{run_tool, DEFAULT_RUN_TIMEOUT, RunOutput};
