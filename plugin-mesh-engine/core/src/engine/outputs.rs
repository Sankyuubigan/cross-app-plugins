//! Сохранение GLB-артефактов: sequenced-имена `mesh-<N>.glb` (счётчик по
//! факту файлов в папке — защита от коллизии двух генераций за секунду,
//! прецедент `sequenced_stem` excel-тулов King Orch).

use std::path::{Path, PathBuf};

/// Сохранить GLB в папку сессии под следующим свободным номером.
pub fn save_glb_sequenced(session_dir: &Path, glb: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(session_dir)
        .map_err(|e| format!("не удалось создать папку {}: {e}", session_dir.display()))?;
    let path = session_dir.join(format!("mesh-{}.glb", next_sequence(session_dir)?));
    std::fs::write(&path, glb)
        .map_err(|e| format!("не удалось записать {}: {e}", path.display()))?;
    log::info!("[mesh] GLB сохранён: {} ({} байт)", path.display(), glb.len());
    Ok(path)
}

/// Следующий номер: max(mesh-N.glb) + 1, иначе 1.
fn next_sequence(session_dir: &Path) -> Result<u32, String> {
    let mut max: u32 = 0;
    let entries = std::fs::read_dir(session_dir)
        .map_err(|e| format!("не удалось прочитать {}: {e}", session_dir.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(rest) = name.strip_prefix("mesh-").and_then(|r| r.strip_suffix(".glb")) {
            if let Ok(n) = rest.parse::<u32>() {
                max = max.max(n);
            }
        }
    }
    Ok(max + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mesh_out_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn sequenced_names_skip_existing() {
        let dir = temp_dir("seq");
        let _ = std::fs::remove_dir_all(&dir);
        let p1 = save_glb_sequenced(&dir, b"glb1").unwrap();
        let p2 = save_glb_sequenced(&dir, b"glb2").unwrap();
        assert!(p1.ends_with("mesh-1.glb"), "{p1:?}");
        assert!(p2.ends_with("mesh-2.glb"), "{p2:?}");
        // Чужой файл mesh-9.glb поднимает счётчик — тихой перезаписи нет.
        std::fs::write(dir.join("mesh-9.glb"), b"old").unwrap();
        let p3 = save_glb_sequenced(&dir, b"glb3").unwrap();
        assert!(p3.ends_with("mesh-10.glb"), "{p3:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
