//! Префлайт: наличие trellis-server.exe и весов выбранного кванта.
//!
//! Числового VRAM-гейта НЕТ (решение PRP v2.1, §00.2): пороги квантов —
//! цифры специалиста, на реальной машине не измерены; ложный отказ хуже
//! честной ошибки самого процесса, которая и так дойдёт до юзера через
//! существующий путь (§0.7). Ожидания по VRAM описаны в доке установки
//! и md-агенте, не в коде.

use std::path::{Path, PathBuf};

use super::presets::{weights_dir, MeshQuant};

/// Резолв путей движка: `<engine_dir>/trellis-server.exe`,
/// `<engine_dir>/models/<quant>`.
pub struct EnginePaths {
    /// Путь к trellis-server.exe.
    pub exe: PathBuf,
    /// Папка весов выбранного кванта.
    pub models_dir: PathBuf,
}

/// Резолв путей для генерации (quant определяет папку весов).
pub fn engine_paths(engine_dir: &Path, quant: MeshQuant) -> EnginePaths {
    EnginePaths {
        exe: exe_path(engine_dir),
        models_dir: weights_dir(engine_dir, quant),
    }
}

/// Канонический путь бинарника движка: `<engine_dir>/trellis-server.exe`.
pub fn exe_path(engine_dir: &Path) -> PathBuf {
    engine_dir.join("trellis-server.exe")
}

/// Файлы движка и весов. Ошибка = готовая инструкция установки (юзер видит
/// её как есть — путь и репо-источник).
pub fn check_engine_files(paths: &EnginePaths, preset_needs_pixal3d: bool) -> Result<(), String> {
    if !paths.exe.is_file() {
        return Err(format!(
            "движок 3D не установлен: нет {} — скачай Windows-пребилд trellis-server \
             из релизов pwilkin/trellis.cpp и положи его в эту папку",
            paths.exe.display()
        ));
    }
    if !paths.models_dir.is_dir() {
        return Err(format!(
            "веса не найдены: ожидалась папка {} — скачай веса ilintar/trellis2-gguf \
             (папка q4/q8/f16 целиком) и положи её туда",
            paths.models_dir.display()
        ));
    }
    if gguf_count(&paths.models_dir) == 0 {
        return Err(format!(
            "в папке {} нет *.gguf — веса ilintar/trellis2-gguf не скачаны",
            paths.models_dir.display()
        ));
    }
    if preset_needs_pixal3d && pixal3d_count(&paths.models_dir) == 0 {
        return Err(format!(
            "веса Pixal3D не найдены: в {} нет pixal3d_*.gguf — добавь 5 файлов из \
             vegax87/Pixal3D в ту же папку (декодеры общие с TRELLIS.2)",
            paths.models_dir.display()
        ));
    }
    Ok(())
}

fn gguf_count(dir: &Path) -> usize {
    count_matching(dir, |name| name.ends_with(".gguf"))
}

fn pixal3d_count(dir: &Path) -> usize {
    count_matching(dir, |name| name.starts_with("pixal3d_") && name.ends_with(".gguf"))
}

fn count_matching(dir: &Path, pred: impl Fn(&str) -> bool) -> usize {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .filter(|e| pred(&e.file_name().to_string_lossy()))
                .count()
        })
        .unwrap_or(0)
}

/// Строка статуса одного кванта (для команды mesh_engine_status).
pub struct QuantWeightsStatus {
    /// Имя кванта (q4/q8/f16).
    pub quant: String,
    /// Канонический путь папки весов.
    pub dir: String,
    /// Сколько *.gguf найдено.
    pub gguf_count: usize,
    /// Сколько pixal3d_*.gguf найдено (веса Pixal3D в той же папке).
    pub pixal3d_files: usize,
}

/// Статус всех квант-папок (UI/диагностика: чем заполнен движок).
pub fn weights_status(engine_dir: &Path) -> Vec<QuantWeightsStatus> {
    [MeshQuant::Q4, MeshQuant::Q8, MeshQuant::Fp16]
        .into_iter()
        .map(|q| {
            let dir = weights_dir(engine_dir, q);
            QuantWeightsStatus {
                quant: q.dir_name().to_string(),
                dir: dir.display().to_string(),
                gguf_count: gguf_count(&dir),
                pixal3d_files: pixal3d_count(&dir),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mesh_preflight_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn missing_exe_gives_install_instruction() {
        let dir = temp_dir("noexe");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let paths = engine_paths(&dir, MeshQuant::Q8);
        let err = check_engine_files(&paths, false).unwrap_err();
        assert!(err.contains("trellis-server"), "{err}");
        assert!(err.contains("pwilkin/trellis.cpp"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_weights_gives_hf_repo_hint() {
        let dir = temp_dir("noweights");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("models").join("q8")).unwrap();
        fs::write(exe_path(&dir), b"fake").unwrap();
        let paths = engine_paths(&dir, MeshQuant::Q8);
        let err = check_engine_files(&paths, false).unwrap_err();
        assert!(err.contains("ilintar/trellis2-gguf"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pixal3d_preset_requires_pixal3d_weights() {
        let dir = temp_dir("nopix");
        let _ = fs::remove_dir_all(&dir);
        let q8 = dir.join("models").join("q8");
        fs::create_dir_all(&q8).unwrap();
        fs::write(exe_path(&dir), b"fake").unwrap();
        fs::write(q8.join("trellis2_slat.gguf"), b"fake").unwrap();
        let paths = engine_paths(&dir, MeshQuant::Q8);
        // trellis-пресет проходит с одними весами TRELLIS.2…
        check_engine_files(&paths, false).unwrap();
        // …а pixal3d честно требует свои файлы.
        let err = check_engine_files(&paths, true).unwrap_err();
        assert!(err.contains("pixal3d_*.gguf"), "{err}");
        assert!(err.contains("vegax87/Pixal3D"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn weights_status_reports_counts() {
        let dir = temp_dir("status");
        let _ = fs::remove_dir_all(&dir);
        let q8 = dir.join("models").join("q8");
        fs::create_dir_all(&q8).unwrap();
        fs::write(q8.join("a.gguf"), b"x").unwrap();
        fs::write(q8.join("pixal3d_decoder.gguf"), b"x").unwrap();
        let status = weights_status(&dir);
        let q8s = status.iter().find(|s| s.quant == "q8").unwrap();
        assert_eq!(q8s.gguf_count, 2);
        assert_eq!(q8s.pixal3d_files, 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
