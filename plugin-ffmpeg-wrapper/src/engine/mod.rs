//! Тонкий re-export ядра `ffmpeg-core` (ССОТ, Шаг 4c PRP 261115): вся логика
//! установки/запуска/probe/pipe живёт в `core/` (крейт ffmpeg-core, без tauri).
//! Путь `plugin_ffmpeg_wrapper::engine::*` сохранён для хостов
//! (King Orch infra/tools/ffmpeg.rs: `use plugin_ffmpeg_wrapper::engine;`).

pub use ffmpeg_core::{installer, pipe, probe, runner};

pub use ffmpeg_core::{
    ensure_ffmpeg, ffmpeg_dir, ffmpeg_exe, ffprobe_exe, is_installed, tool_version,
    DEFAULT_RUN_TIMEOUT, FFMPEG_DIR_NAME, FfmpegPipe, RunOutput, probe_json, run_tool,
};
