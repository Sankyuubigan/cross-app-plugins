//! Проверка наличия обновления: цепочка Primary → Fallback → Graceful Empty.
//!
//! 1. **Primary** — latest.json (формат Tauri v2), если задан URL. Канал
//!    авторитетный: ответил и версия не новее текущей → «обновлений нет»,
//!    фолбэк не нужен (у Tauri-хостов это тот же канал, что у официального
//!    updater — расхождений в вердикте быть не должно).
//! 2. **Fallback** — api.github.com/repos/{repo}/releases/latest + ассет
//!    `-setup.exe` (+ подпись `.sig`, если задан pubkey).
//! 3. **Graceful Empty** — нет релизов / нет подходящего ассета / версия не
//!    новее текущей → `Ok(None)` («Обновлений пока нет»). Сетевые ошибки и
//!    битые ответы отдают `Err` — молча превращать сбой сети в «обновлений
//!    нет» нельзя (ложь в UI).

use crate::history;
use crate::latest::{self, LatestJson};
use crate::version::compare_versions;
use std::cmp::Ordering;

/// Конфиг проверки: откуда брать обновления.
#[derive(Debug, Clone, Default)]
pub struct UpdateConfig {
    /// GitHub-репозиторий «owner/repo» (Fallback-канал).
    pub repo: String,
    /// URL latest.json (Primary-канал). None — только Fallback.
    pub latest_json_url: Option<String>,
    /// minisign-публичный ключ (plain или Base64-обёртка Tauri): задан →
    /// у Fallback-кандидата качается `.sig` и установщик проверяется.
    pub pubkey: Option<String>,
}

/// Найденное обновление.
#[derive(Debug, Clone)]
pub struct UpdateCandidate {
    pub version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub download_url: String,
    /// Текст подписи (Base64-обёртка или plain minisign). Проверка — после
    /// скачивания, в installer::run_install.
    pub signature: Option<String>,
    /// Канал, из которого пришёл кандидат: "latest.json" | "github".
    pub source: &'static str,
}

/// Проверить обновления по цепочке. `Ok(None)` — обновлений пока нет.
pub async fn check_for_update(
    cfg: &UpdateConfig,
    current_version: &str,
) -> Result<Option<UpdateCandidate>, String> {
    // 1. Primary: latest.json.
    if let Some(url) = &cfg.latest_json_url {
        match latest::fetch_latest_json(url).await {
            Ok(parsed) => match primary_candidate(&parsed, current_version) {
                Ok(Some(candidate)) => return Ok(Some(candidate)),
                Ok(None) => return Ok(None),
                Err(e) => {
                    log::warn!(
                        "[about-updates] Primary (latest.json) без артефакта платформы — фолбэк на GitHub: {}",
                        e
                    );
                }
            },
            Err(e) => {
                log::warn!("[about-updates] Primary (latest.json) недоступен — фолбэк на GitHub: {}", e);
            }
        }
    }

    // 2. Fallback: GitHub Releases.
    github_latest(cfg, current_version).await
}

/// Вердикт по разобранному latest.json: Some — обновление, None — версия
/// актуальна; Err — для текущей платформы артефакта нет (нужен фолбэк).
fn primary_candidate(
    parsed: &LatestJson,
    current: &str,
) -> Result<Option<UpdateCandidate>, String> {
    let art = latest::pick_platform(parsed).ok_or_else(|| {
        format!(
            "в latest.json нет артефакта для платформы {}",
            latest::platform_key()
        )
    })?;
    if compare_versions(&parsed.version, current) != Ordering::Greater {
        return Ok(None);
    }
    Ok(Some(UpdateCandidate {
        version: parsed.version.clone(),
        notes: parsed.notes.clone(),
        pub_date: parsed.pub_date.clone(),
        download_url: art.url.clone(),
        signature: Some(art.signature.clone()),
        source: "latest.json",
    }))
}

/// Fallback: последний релиз GitHub + ассет `-setup.exe` (+ `.sig` при pubkey).
async fn github_latest(
    cfg: &UpdateConfig,
    current: &str,
) -> Result<Option<UpdateCandidate>, String> {
    if cfg.repo.trim().is_empty() {
        // Репозиторий не задан — проверять нечем: «Обновлений пока нет».
        return Ok(None);
    }
    let client = reqwest::Client::builder()
        .user_agent("about-updates-core")
        .build()
        .map_err(|e| format!("Ошибка создания HTTP-клиента: {}", e))?;
    let url = format!("https://api.github.com/repos/{}/releases/latest", cfg.repo);
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            log::error!("[about-updates] GitHub (latest) недоступен: {}", e);
            format!("Ошибка запроса GitHub: {}", e)
        })?;

    // 404 = релизов нет вообще: это Graceful Empty, а не ошибка.
    if resp.status().as_u16() == 404 {
        log::info!("[about-updates] релизов в {} нет — Обновлений пока нет", cfg.repo);
        return Ok(None);
    }
    if !resp.status().is_success() {
        log::error!("[about-updates] GitHub API вернул HTTP {}", resp.status());
        return Err(format!("GitHub API error: {}", resp.status()));
    }
    let rel: serde_json::Value = resp.json().await.map_err(|e| {
        log::error!("[about-updates] ответ GitHub API не распознан: {}", e);
        format!("Некорректный ответ GitHub API: {}", e)
    })?;

    let tag = rel
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let version = tag.trim_start_matches('v').to_string();
    if version.is_empty() {
        return Ok(None);
    }
    let Some(download_url) = history::setup_asset_url(&rel) else {
        log::info!("[about-updates] у релиза {} нет ассета -setup.exe — Обновлений пока нет", version);
        return Ok(None);
    };
    if compare_versions(&version, current) != Ordering::Greater {
        return Ok(None);
    }

    // Подпись: ассет `<exe>.sig` — качается, только если задан pubkey.
    // Не скачалась → установка пойдёт без проверки (прежнее поведение
    // Fallback-канала), но warn в лог обязателен.
    let signature = match &cfg.pubkey {
        Some(_) => fetch_sig_asset(&client, &rel, &download_url).await,
        None => None,
    };

    Ok(Some(UpdateCandidate {
        version,
        notes: rel.get("body").and_then(|v| v.as_str()).map(|s| s.to_string()),
        pub_date: rel
            .get("published_at")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        download_url,
        signature,
        source: "github",
    }))
}

/// Скачать ассет `<exe>.sig` рядом с установщиком. Ошибка — не блокирует
/// установку: вернётся None с warn.
async fn fetch_sig_asset(
    client: &reqwest::Client,
    rel: &serde_json::Value,
    download_url: &str,
) -> Option<String> {
    let exe = download_url.rsplit('/').next()?;
    let sig_name = format!("{}.sig", exe);
    let assets = rel.get("assets")?.as_array()?;
    let sig_url = assets
        .iter()
        .find(|a| a.get("name").and_then(|v| v.as_str()) == Some(sig_name.as_str()))
        .and_then(|a| a.get("browser_download_url"))
        .and_then(|v| v.as_str())?
        .to_string();

    match client.get(&sig_url).send().await {
        Ok(resp) if resp.status().is_success() => match resp.text().await {
            Ok(t) => Some(t),
            Err(e) => {
                log::warn!("[about-updates] .sig {} не прочитан: {}", sig_url, e);
                None
            }
        },
        Ok(resp) => {
            log::warn!("[about-updates] .sig {} вернул HTTP {}", sig_url, resp.status());
            None
        }
        Err(e) => {
            log::warn!("[about-updates] .sig {} недоступен: {}", sig_url, e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latest_with(version: &str, platforms: &str) -> LatestJson {
        serde_json::from_str(&format!(
            r#"{{"version":"{}","platforms":{}}}"#,
            version, platforms
        ))
        .expect("latest.json разбирается")
    }

    /// Primary-кандидат отдаётся только для версии новее текущей.
    #[test]
    fn primary_candidate_only_for_newer_version() {
        let parsed = latest_with(
            "26.12.0",
            r#"{"windows-x86_64":{"signature":"sig-text","url":"https://x/App-setup.exe"}}"#,
        );
        let c = primary_candidate(&parsed, "26.11.5")
            .expect("разбор")
            .expect("обновление найдено");
        assert_eq!(c.version, "26.12.0");
        assert_eq!(c.source, "latest.json");
        assert_eq!(c.download_url, "https://x/App-setup.exe");
        assert_eq!(c.signature.as_deref(), Some("sig-text"));

        // та же версия → «Обновлений пока нет»
        assert!(primary_candidate(&parsed, "26.12.0").expect("разбор").is_none());
        // latest.json отстал от локальной версии → тоже нет
        assert!(primary_candidate(&parsed, "26.13.0").expect("разбор").is_none());
    }

    /// Артефакта для текущей платформы нет → Err (нужен фолбэк на GitHub).
    #[test]
    fn primary_requires_platform_artifact() {
        let parsed = latest_with(
            "26.12.0",
            r#"{"linux-x86_64":{"signature":"","url":"https://x"}}"#,
        );
        assert!(primary_candidate(&parsed, "26.11.5").is_err());
    }

    /// Пустой репозиторий в конфиге — не ошибка, а Graceful Empty.
    #[test]
    fn empty_repo_is_graceful_none() {
        let cfg = UpdateConfig::default();
        // github_latest с пустым repo возвращает Ok(None) без сети —
        // проверяем через саму функцию (раньше сетевого вызова).
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let out = rt.block_on(github_latest(&cfg, "26.11.5")).expect("graceful");
        assert!(out.is_none());
    }
}
