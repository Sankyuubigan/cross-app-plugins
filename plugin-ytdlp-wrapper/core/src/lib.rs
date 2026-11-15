//! ytdlp-core — чистое ядро плагина `plugin-ytdlp-wrapper` (SSOT, Шаг 4d
//! PRP 261115_universal_plugins). Без Tauri: установка/обновление бинаря
//! yt-dlp и Deno (скачивание — единый движок downloader-core), конфиг,
//! скачивание видео через subprocess + события UI через хук, VTT-субтитры.
//!
//! Host-специфика (папки данных/установки) — параметры `data_dir: &Path`,
//! `dir: &Path`. События UI — [`downloader::YtdlpEvent`] через хук:
//! Tauri-фасад эмитит `ytdlp:download-*`, GPUI-хост — свой обработчик.

pub mod downloader;
pub mod net_errors;
pub mod subtitles;
pub mod ytdlp_manager;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct YtdlpConfig {
    #[serde(default)]
    pub ytdlp_dir: Option<String>,
    #[serde(default)]
    pub ffmpeg_path: Option<String>,
    #[serde(default)]
    pub deno_dir: Option<String>,
    #[serde(default)]
    pub download_path: Option<String>,
    #[serde(default)]
    pub auth_mode: Option<String>,
    #[serde(default)]
    pub auth_browser: Option<String>,
    #[serde(default)]
    pub cookies_file: Option<String>,
    #[serde(default)]
    pub download_format: Option<String>,
    #[serde(default)]
    pub download_quality: Option<String>,
    #[serde(default)]
    pub proxy: Option<String>,
    /// Источник кукисов для anti-bot (механика dpi_gui): имя браузера или папка
    /// User Data. Сохраняется командой grab_ytdlp_cookies для префилла UI.
    #[serde(default)]
    pub cookie_browser: Option<String>,
    #[serde(default)]
    pub cookie_profile_dir: Option<String>,
}

pub fn load_config(data_dir: &Path) -> YtdlpConfig {
    let path = config_path(data_dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let mut cfg: YtdlpConfig = serde_json::from_str(&text).unwrap_or_default();
            // Merge defaults for missing fields
            if cfg.auth_mode.is_none() { cfg.auth_mode = Some("none".to_string()); }
            if cfg.auth_browser.is_none() { cfg.auth_browser = Some("chrome".to_string()); }
            if cfg.download_format.is_none() { cfg.download_format = Some("mp4".to_string()); }
            if cfg.download_quality.is_none() { cfg.download_quality = Some("max".to_string()); }
            cfg
        }
        Err(_) => YtdlpConfig {
            auth_mode: Some("none".to_string()),
            auth_browser: Some("chrome".to_string()),
            download_format: Some("mp4".to_string()),
            download_quality: Some("max".to_string()),
            ..Default::default()
        },
    }
}

pub fn save_config(data_dir: &Path, cfg: &YtdlpConfig) -> Result<(), String> {
    let path = config_path(data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Не удалось создать директорию: {e}"))?;
    }
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("Ошибка сериализации: {e}"))?;
    json_store::write_atomic(&path, &json)
        .map_err(|e| format!("Ошибка записи конфига: {e}"))
}

fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("ytdlp_config.json")
}

pub use downloader::{YtdlpEvent, YtdlpEventHook};
pub use ytdlp_manager::YtdlpStatus;
