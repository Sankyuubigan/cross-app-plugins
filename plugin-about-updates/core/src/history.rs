//! История релизов GitHub Releases (для панели «О программе» и отката).

use crate::models::ReleaseInfo;

/// URL ассета `-setup.exe` релиза (подпись `.sig` не считается). None —
/// подходящего ассета нет.
pub fn setup_asset_url(rel: &serde_json::Value) -> Option<String> {
    let assets = rel.get("assets")?.as_array()?;
    for a in assets {
        let n = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if n.ends_with("-setup.exe") && !n.ends_with(".sig") {
            let url = a
                .get("browser_download_url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !url.is_empty() {
                return Some(url.to_string());
            }
        }
    }
    None
}

/// Список релизов с ассетами `-setup.exe` через GitHub Releases REST API.
/// `current_version` помечает релиз как текущий (is_current).
pub async fn fetch_release_history(
    repo: &str,
    current_version: &str,
) -> Result<Vec<ReleaseInfo>, String> {
    let client = reqwest::Client::builder()
        .user_agent("plugin-about-updates")
        .build()
        .map_err(|e| {
            log::error!("[about-updates] HTTP-клиент: {}", e);
            format!("Ошибка создания HTTP-клиента: {}", e)
        })?;

    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=100");
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            log::error!("[about-updates] запрос истории релизов не удался: {}", e);
            format!("Ошибка запроса GitHub: {}", e)
        })?;

    if !resp.status().is_success() {
        log::error!("[about-updates] GitHub API вернул HTTP {}", resp.status());
        return Err(format!("GitHub API error: {}", resp.status()));
    }

    let releases: serde_json::Value = resp.json().await.map_err(|e| {
        log::error!("[about-updates] ответ GitHub API не распознан: {}", e);
        format!("Некорректный ответ GitHub API: {}", e)
    })?;

    let mut out: Vec<ReleaseInfo> = Vec::new();
    let arr = releases.as_array().ok_or_else(|| {
        log::error!("[about-updates] ответ GitHub API не массив");
        "Некорректный ответ GitHub API".to_string()
    })?;

    for rel in arr {
        let tag = rel
            .get("tag_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let version = tag.trim_start_matches('v').to_string();
        if version.is_empty() {
            continue;
        }

        let pub_date = rel
            .get("published_at")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let notes = rel.get("body").and_then(|v| v.as_str()).map(|s| s.to_string());

        let Some(download_url) = setup_asset_url(rel) else {
            log::warn!("[about-updates] у релиза {} нет ассета -setup.exe, пропускаем", version);
            continue;
        };

        out.push(ReleaseInfo {
            version: version.clone(),
            pub_date,
            notes,
            download_url,
            is_current: version == current_version,
        });
    }

    log::info!("[about-updates] история релизов: {} шт.", out.len());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Выбирается именно ассет `-setup.exe`, а не его подпись и не архив.
    #[test]
    fn setup_asset_prefers_setup_exe() {
        let rel = serde_json::json!({
            "tag_name": "v26.11.5",
            "assets": [
                {"name": "App_26.11.5_x64-setup.exe.sig",
                 "browser_download_url": "https://github.com/o/r/releases/download/v26.11.5/App_26.11.5_x64-setup.exe.sig"},
                {"name": "App_26.11.5_x64-setup.exe",
                 "browser_download_url": "https://github.com/o/r/releases/download/v26.11.5/App_26.11.5_x64-setup.exe"},
                {"name": "App_26.11.5.zip",
                 "browser_download_url": "https://github.com/o/r/releases/download/v26.11.5/App_26.11.5.zip"}
            ]
        });
        let url = setup_asset_url(&rel).expect("ассет найден");
        assert!(url.ends_with("App_26.11.5_x64-setup.exe"));
    }

    #[test]
    fn release_without_setup_asset_is_none() {
        let rel = serde_json::json!({
            "tag_name": "v1.0.0",
            "assets": [{"name": "readme.txt", "browser_download_url": "https://x/readme.txt"}]
        });
        assert!(setup_asset_url(&rel).is_none());
        let no_assets = serde_json::json!({"tag_name": "v1.0.0"});
        assert!(setup_asset_url(&no_assets).is_none());
    }
}
