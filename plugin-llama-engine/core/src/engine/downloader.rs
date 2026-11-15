//! Скачивание моделей/бинарей llama-движка — через ЕДИНЫЙ движок downloader-core
//! (ССОТ, Шаг 4a PRP 261115_universal_plugins: дубль логики скачивания удалён).
//!
//! Прежние ~500 строк (свой multi-chunk reqwest, свой .part→rename, свои
//! FallbackRequest/set_fallback_downloader) удалены: цепочка reqwest → curl →
//! process, multi-chunk/stall/resume/зеркала HF — внутри downloader-core.
//! Прогресс — глобальные хуки downloader-core: set_progress_hook/set_emit_hook
//! (Tauri-фасад конвертирует и эмитит `download_progress`, GPUI-хосты рисуют
//! свой бар).

use downloader_core::{download, download_bytes_to_vec, DownloadOptions};
use std::fs;
use std::io::Cursor;

/// Минимальный валидный GGUF (валидация магии+размера — в downloader-core,
/// DownloadOptions { magic, min_size }).
const MIN_GGUF_SIZE: u64 = 1024 * 1024;

// Единый тип прогресса и хук — SSOT downloader-core (llama-шейп {downloaded,
// total, speed_bps} конвертируется в мосту хоста: plugin-llama-engine/src/lib.rs).
pub use downloader_core::{DownloadProgress, ProgressHook, set_progress_hook};

/// Скачать GGUF-модель. Валидация магии GGUF + минимального размера — внутри
/// downloader-core (magic/min_size в DownloadOptions). Сигнатура прежняя
/// (String, String) — потребители mmproj.rs и команды фасада не меняются.
pub async fn download_model(url: String, save_path: String) -> Result<(), String> {
    log::info!("[download_model] {url} -> {save_path}");
    let on_log = |msg: String| log::info!("[download] {msg}");
    download(
        &url,
        &save_path,
        DownloadOptions {
            label: "Модель".into(),
            kind: "model".into(),
            magic: Some(b"GGUF".to_vec()),
            min_size: Some(MIN_GGUF_SIZE),
            ..Default::default()
        },
        Some(&on_log),
    )
    .await
}

/// Скачать бинарь движка. `extract_zip` — архив: качаем в память, распаковываем
/// нужный exe (по имени или единственный .exe). Не-архив — сразу в `save_path`.
pub async fn download_binary(url: String, save_path: String, extract_zip: bool) -> Result<(), String> {
    log::info!("[download_binary] {url} -> {save_path} (zip={extract_zip})");
    let on_log = |msg: String| log::info!("[download] {msg}");
    let result = if extract_zip {
        let bytes = download_bytes_to_vec(
            &url,
            DownloadOptions {
                label: "Бинарь".into(),
                kind: "bin".into(),
                ..Default::default()
            },
            None,
        )
        .await?;
        extract_binary_from_zip(&bytes, &save_path)?;
        Ok(())
    } else {
        download(
            &url,
            &save_path,
            DownloadOptions {
                label: "Бинарь".into(),
                kind: "bin".into(),
                ..Default::default()
            },
            Some(&on_log),
        )
        .await
    };
    result?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&save_path, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("chmod {save_path}: {e}"))?;
    }

    log::info!("[download_binary] готово: {save_path}");
    Ok(())
}

/// Извлечь бинарь из zip-архива: запись, оканчивающаяся на имя целевого файла
/// (например `llama-server.exe`); иначе — единственный .exe в архиве.
fn extract_binary_from_zip(bytes: &[u8], save_path: &str) -> Result<(), String> {
    let target_name = std::path::Path::new(save_path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Некорректный путь сохранения: {save_path}"))?;

    let mut arch = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("Не удалось открыть zip-архив: {e}"))?;

    // Точное совпадение имени приоритетнее «первого попавшегося .exe».
    let mut chosen: Option<(usize, String)> = None;
    for i in 0..arch.len() {
        let name = arch
            .by_index(i)
            .map_err(|e| format!("zip: {e}"))?
            .name()
            .to_string();
        if name.ends_with(target_name) {
            chosen = Some((i, name));
            break;
        }
        if target_name.ends_with(".exe") && name.ends_with(".exe") && chosen.is_none() {
            chosen = Some((i, name));
        }
    }
    let (idx, name) = chosen
        .ok_or_else(|| format!("В архиве нет бинаря ({target_name})"))?;

    let mut entry = arch
        .by_index(idx)
        .map_err(|e| format!("zip: {e}"))?;
    let mut out = fs::File::create(save_path)
        .map_err(|e| format!("Не удалось создать {}: {e}", save_path))?;
    std::io::copy(&mut entry, &mut out)
        .map_err(|e| format!("Распаковка {name}: {e}"))?;

    log::info!("[download_binary] распакован {name} -> {save_path}");
    Ok(())
}
