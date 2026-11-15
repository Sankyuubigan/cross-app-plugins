//! Бэкап пользовательских данных перед понижением/обновлением версии.
//! Перенос из Tauri-фасада: `data_dir: &Path` параметром (VibeBridge кладёт
//! данные рядом с exe, Tauri-хосты — app_data_dir; ядро не должно знать, где).

use std::fs;
use std::path::{Path, PathBuf};

/// Дефолтный список бэкапа: конфиг приложения и сессии.
pub const DEFAULT_BACKUP_ITEMS: [&str; 2] = ["app_config.json", "sessions"];

/// Копирует дефолтные данные ([`DEFAULT_BACKUP_ITEMS`]) в `rollback_backup/`
/// рядом с данными приложения. Гарантирует, что при даунгрейде старая версия
/// сможет прочитать данные, либо пользователь сможет вручную вернуть бэкап,
/// если схема данных новее нечитаема.
pub fn backup_before_rollback(data_dir: &Path) -> Result<(), String> {
    backup_items(data_dir, &DEFAULT_BACKUP_ITEMS)
}

/// Бэкап перечисленных элементов (файлы и папки) в `rollback_backup/` с
/// меткой времени: файл — `stem_{ts}.ext`, папка — `name_{ts}`.
pub fn backup_items(data_dir: &Path, items: &[&str]) -> Result<(), String> {
    if !data_dir.exists() {
        return Ok(());
    }

    let backup_dir = data_dir.join("rollback_backup");
    fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();

    for item in items {
        let src = data_dir.join(item);
        if !src.exists() {
            continue;
        }
        let dst = backup_target(&backup_dir, &src, &ts);
        if src.is_dir() {
            let _ = copy_dir(&src, &dst);
        } else {
            let _ = fs::copy(&src, &dst);
        }
    }

    Ok(())
}

/// Имя бэкап-копии одного элемента (сохраняет расширение файла).
fn backup_target(backup_dir: &Path, src: &Path, ts: &str) -> PathBuf {
    match (src.file_stem(), src.extension()) {
        (Some(stem), Some(ext)) => backup_dir.join(format!(
            "{}_{}.{}",
            stem.to_string_lossy(),
            ts,
            ext.to_string_lossy()
        )),
        (Some(stem), None) => backup_dir.join(format!("{}_{}", stem.to_string_lossy(), ts)),
        _ => backup_dir.join(format!(
            "{}_{}",
            src.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ts
        )),
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_copies_default_items_with_timestamps() {
        let root = std::env::temp_dir().join(format!(
            "about_updates_core_rollback_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let data = root.join("data");
        std::fs::create_dir_all(data.join("sessions")).unwrap();
        std::fs::write(data.join("app_config.json"), "{\"ok\":1}").unwrap();
        std::fs::write(data.join("sessions").join("one.txt"), "s").unwrap();

        backup_before_rollback(&data).unwrap();

        let backup = data.join("rollback_backup");
        let names: Vec<String> = fs::read_dir(&backup)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.iter().any(|n| n.starts_with("app_config_") && n.ends_with(".json")),
            "конфиг скопирован с меткой: {}",
            names.join(",")
        );
        assert!(
            names.iter().any(|n| n.starts_with("sessions_")),
            "папка сессий скопирована с меткой: {}",
            names.join(",")
        );

        // Пустой каталог данных — не ошибка; несуществующий элемент пропускается.
        assert!(backup_before_rollback(&root.join("empty")).is_ok());
        backup_items(&data, &["app_config.json", "нет_такого"]).unwrap();

        let _ = std::fs::remove_dir_all(&root);
    }
}
