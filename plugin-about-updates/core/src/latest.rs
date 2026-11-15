//! Официальный формат latest.json (Tauri v2 updater).
//!
//! ```json
//! {
//!   "version": "26.11.5",
//!   "notes": "…",
//!   "pub_date": "2026-11-15T00:00:00Z",
//!   "platforms": {
//!     "windows-x86_64": {
//!       "signature": "…Base64 текста .sig-файла…",
//!       "url": "https://github.com/owner/repo/releases/download/v26.11.5/App_26.11.5_x64-setup.exe"
//!     }
//!   }
//! }
//! ```
//!
//! Поле `url` — источник правды для скачивания; `signature` проверяется
//! после скачивания ([`crate::minisign::verify_installer_file`]).

use serde::Deserialize;
use std::collections::BTreeMap;

/// Корень latest.json (формат Tauri v2).
#[derive(Debug, Clone, Deserialize)]
pub struct LatestJson {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub pub_date: Option<String>,
    pub platforms: BTreeMap<String, PlatformArtifact>,
}

/// Артефакт одной платформы. Неизвестные поля (например `with_update`)
/// serde игнорирует — генераторы формата развиваются независимо.
#[derive(Debug, Clone, Deserialize)]
pub struct PlatformArtifact {
    pub signature: String,
    pub url: String,
}

/// Ключ платформы в `platforms` — правила именования Tauri v2
/// («windows-x86_64», «darwin-aarch64», «linux-x86_64»).
pub fn platform_key() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "darwin-aarch64"
        } else {
            "darwin-x86_64"
        }
    } else if cfg!(target_arch = "aarch64") {
        "linux-aarch64"
    } else {
        "linux-x86_64"
    }
}

/// Артефакт текущей платформы.
pub fn pick_platform(latest: &LatestJson) -> Option<&PlatformArtifact> {
    latest.platforms.get(platform_key())
}

/// Скачать и разобрать latest.json.
pub async fn fetch_latest_json(url: &str) -> Result<LatestJson, String> {
    let client = reqwest::Client::builder()
        .user_agent("about-updates-core")
        .build()
        .map_err(|e| {
            log::error!("[about-updates] HTTP-клиент: {}", e);
            format!("Ошибка создания HTTP-клиента: {}", e)
        })?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("latest.json {} недоступен: {}", url, e))?;
    if !resp.status().is_success() {
        log::error!("[about-updates] latest.json {} вернул HTTP {}", url, resp.status());
        return Err(format!("latest.json {}: HTTP {}", url, resp.status()));
    }
    resp.json::<LatestJson>()
        .await
        .map_err(|e| {
            log::error!("[about-updates] latest.json {} не разобран: {}", url, e);
            format!("Некорректный latest.json {}: {}", url, e)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Пример формата Tauri v2: поле `url`, ключ платформы, лишние поля
    /// (with_update) и чужие платформы игнорируются.
    #[test]
    fn parses_tauri_v2_latest_json() {
        let raw = r#"{
            "version": "26.12.0",
            "notes": "Что нового",
            "pub_date": "2026-11-15T00:00:00Z",
            "platforms": {
                "windows-x86_64": {
                    "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQ==",
                    "url": "https://github.com/o/r/releases/download/v26.12.0/App_26.12.0_x64-setup.exe",
                    "with_update": true
                },
                "linux-x86_64": { "signature": "", "url": "https://example.com/app.AppImage" }
            }
        }"#;
        let parsed: LatestJson = serde_json::from_str(raw).expect("формат Tauri v2 разбирается");
        assert_eq!(parsed.version, "26.12.0");
        assert_eq!(parsed.notes.as_deref(), Some("Что нового"));
        assert!(parsed.platforms.contains_key(platform_key()));
        let art = pick_platform(&parsed).expect("артефакт текущей платформы");
        assert!(art.url.ends_with("-setup.exe"));
        assert!(!art.signature.is_empty());
    }

    /// Ключ платформы не пуст и соответствует семейству ОС.
    #[test]
    fn platform_key_matches_family() {
        let key = platform_key();
        assert!(key.starts_with("windows-") || key.starts_with("darwin-") || key.starts_with("linux-"));
    }
}
