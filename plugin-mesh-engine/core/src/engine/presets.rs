//! Пресеты моделей и квантов весов trellis.cpp.
//!
//! Раскладка весов СТРОГО каноническая (решение PRP v2.1): `models/<quant>/`.
//! В HF-репо ilintar/trellis2-gguf квант лежит в папках q4/q8/f16 — скачанная
//! папка кладётся целиком. Веса Pixal3D (vegax87/Pixal3D, 5 файлов
//! pixal3d_*.gguf) кладутся в ТА ЖЕ папку — декодеры/dinov3/birefnet там
//! байт-идентичны общим с TRELLIS.2. Фоллбэк-поиска по диску нет: тихий
//! поиск «а вдруг где-то ещё» даёт ложные находки и половинные раскладки;
//! честная ошибка с инструкцией установки надёжнее.

use std::path::{Path, PathBuf};

/// Семейство модели (`--model` trellis-server).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshPreset {
    /// TRELLIS.2 (нода SLAT → GLB; дефолт).
    Trellis,
    /// Pixal3D (GLB c uv/atlas).
    Pixal3d,
}

impl MeshPreset {
    /// Значение CLI-флага `--model` (верифицировано по trellis_args.cpp).
    pub fn as_flag(self) -> &'static str {
        match self {
            MeshPreset::Trellis => "trellis",
            MeshPreset::Pixal3d => "pixal3d",
        }
    }

    /// Разбор строкового пресета. Принимает и "trellis", и "trellis2"
    /// (публичное имя репо — TRELLIS.2), чтобы тул/фронт не гадали.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "trellis" | "trellis2" => Ok(MeshPreset::Trellis),
            "pixal3d" => Ok(MeshPreset::Pixal3d),
            other => Err(format!(
                "неизвестный пресет модели '{other}': доступны trellis2 | pixal3d"
            )),
        }
    }
}

/// Квант весов (имя папки под `models/`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshQuant {
    Q4,
    Q8,
    Fp16,
}

impl MeshQuant {
    /// Имя папки весов.
    pub fn dir_name(self) -> &'static str {
        match self {
            MeshQuant::Q4 => "q4",
            MeshQuant::Q8 => "q8",
            MeshQuant::Fp16 => "f16",
        }
    }

    /// Разбор строкового кванта.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "q4" => Ok(MeshQuant::Q4),
            "q8" => Ok(MeshQuant::Q8),
            "f16" | "fp16" => Ok(MeshQuant::Fp16),
            other => Err(format!("неизвестный квант '{other}': доступны q4 | q8 | f16")),
        }
    }
}

/// Квант по умолчанию: q8 (качество/память — разумный дефолт MVP; q4 — для
/// слабых GPU, f16 — для больших).
pub const DEFAULT_QUANT: MeshQuant = MeshQuant::Q8;

/// Разрешение по умолчанию (дефолт сервера; 1024/1536 на Vulkan рискованны —
/// FlexiDualGrid-краши, issues #27/#37/#3966 trellis.cpp).
pub const DEFAULT_RESOLUTION: u32 = 512;

/// Канонический путь весов: `<engine_dir>/models/<quant>`.
pub fn weights_dir(engine_dir: &Path, quant: MeshQuant) -> PathBuf {
    engine_dir.join("models").join(quant.dir_name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_parse_accepts_trellis2_alias() {
        assert_eq!(MeshPreset::parse("trellis").unwrap(), MeshPreset::Trellis);
        assert_eq!(MeshPreset::parse("Trellis2").unwrap(), MeshPreset::Trellis);
        assert_eq!(MeshPreset::parse(" pixal3d ").unwrap(), MeshPreset::Pixal3d);
        let err = MeshPreset::parse("stable").unwrap_err();
        assert!(err.contains("trellis2 | pixal3d"), "{err}");
    }

    #[test]
    fn preset_flags_match_server_args() {
        assert_eq!(MeshPreset::Trellis.as_flag(), "trellis");
        assert_eq!(MeshPreset::Pixal3d.as_flag(), "pixal3d");
    }

    #[test]
    fn quant_parse_and_dirs() {
        assert_eq!(MeshQuant::parse("q4").unwrap(), MeshQuant::Q4);
        assert_eq!(MeshQuant::parse("FP16").unwrap(), MeshQuant::Fp16);
        assert!(MeshQuant::parse("int8").is_err());
        assert_eq!(MeshQuant::Fp16.dir_name(), "f16");
        assert_eq!(DEFAULT_QUANT.dir_name(), "q8");
    }

    #[test]
    fn weights_dir_is_strict_canonical() {
        let d = weights_dir(Path::new("C:/apps/app/trellis"), MeshQuant::Q4);
        assert!(d.ends_with(r"models\q4"), "{d:?}");
    }
}
