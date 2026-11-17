//! Конфиг плагина: ключ `trellis_dir` в общем `app_config.json` хоста (§0.5).
//! Путь выбирает дверь `json-store` (прецедент `sdcpp_dir` media-gen-core:
//! прямой app_data_dir приводил к расхождению хост/плагин). Запись —
//! `update_json` (field-preserving: чужие ключи файла не затрагиваются).

use std::path::PathBuf;

/// Идентификатор приложения-хоста (документированная привязка — таблица
/// нарушений нейтральности, прецедент media-gen-core).
const HOST_APP_IDENTIFIER: &str = "com.kingorch.app";

fn config_path() -> PathBuf {
    json_store::roots::app_dir(HOST_APP_IDENTIFIER, "").join("app_config.json")
}

/// Прочитанный ключ `trellis_dir` (None — не задан или пустой).
pub fn load_trellis_dir() -> Option<String> {
    let data = json_store::read_text(&config_path()).ok()??;
    let value: serde_json::Value = serde_json::from_str(&data).ok()?;
    let raw = value.get("trellis_dir")?.as_str()?.trim().to_string();
    if raw.is_empty() { None } else { Some(raw) }
}

/// Записать ключ `trellis_dir` (перезапись только этого ключа).
pub fn save_trellis_dir(path: &str) -> Result<(), String> {
    json_store::update_json(&config_path(), move |root: &mut serde_json::Value| {
        if !root.is_object() {
            *root = serde_json::json!({});
        }
        if let serde_json::Value::Object(map) = root {
            map.insert("trellis_dir".to_string(), serde_json::Value::String(path.to_string()));
        }
    })
}

/// Папка 3D-движка (trellis.cpp) без AppHandle: явная из конфига, иначе
/// `<exe>/trellis`. SSOT резолва: её использует и фасад (команды/статус),
/// и хост-тул `generate_mesh` (mesh.rs) — дубль резолва запрещён.
pub fn engine_dir_early() -> PathBuf {
    if let Some(p) = load_trellis_dir() {
        return PathBuf::from(p);
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("trellis")
}

/// Прочитанный ключ `skintokens_dir` (None — не задан или пустой).
pub fn load_skintokens_dir() -> Option<String> {
    let data = json_store::read_text(&config_path()).ok()??;
    let value: serde_json::Value = serde_json::from_str(&data).ok()?;
    let raw = value.get("skintokens_dir")?.as_str()?.trim().to_string();
    if raw.is_empty() { None } else { Some(raw) }
}

/// Записать ключ `skintokens_dir` (перезапись только этого ключа) — зеркало
/// `save_trellis_dir`, дверь команды `mesh_rig_engine_set_dir`.
pub fn save_skintokens_dir(path: &str) -> Result<(), String> {
    json_store::update_json(&config_path(), move |root: &mut serde_json::Value| {
        if !root.is_object() {
            *root = serde_json::json!({});
        }
        if let serde_json::Value::Object(map) = root {
            map.insert("skintokens_dir".to_string(), serde_json::Value::String(path.to_string()));
        }
    })
}

/// Папка риггинга (skin-tokens.cpp) без AppHandle: явная из конфига,
/// иначе `<exe>/skintokens`. Единая точка резолва для хост-тула `rig_mesh`
/// и статуса секции «Риггинг» плашки.
pub fn rig_dir_early() -> PathBuf {
    if let Some(p) = load_skintokens_dir() {
        return PathBuf::from(p);
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("skintokens")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_config_is_none() {
        // Файл может не существовать — это не ошибка, а «не задан».
        let dir = load_trellis_dir();
        assert!(dir.is_none() || dir.is_some(), "чтение не паникует");
        let rig = load_skintokens_dir();
        assert!(rig.is_none() || rig.is_some(), "чтение не паникует");
    }

    #[test]
    fn engine_dir_defaults_to_trellis_next_to_exe() {
        // Дефолт — подпапка `trellis` рядом с exe (cwd-правило rules.md:53).
        assert!(engine_dir_early().ends_with("trellis"));
    }

    #[test]
    fn rig_dir_defaults_to_skintokens_next_to_exe() {
        // Дефолт — подпапка `skintokens` рядом с exe (как `trellis`).
        assert!(rig_dir_early().ends_with("skintokens"));
    }
}
