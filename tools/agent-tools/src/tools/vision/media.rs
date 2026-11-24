//! Подготовка медиа: картинка → `ChatAttachment` (base64), видео → N кадров.
//!
//! Картинка: валидация по РЕАЛЬНОМУ декодированию (magic bytes), а не по расширению
//! (иначе `broken.txt`→`shot.png` уронит сервер кодом 3/4 вместо 2). Ресайз `--max-dim`
//! (4K-скрин/6 ракурсов легко выжирают VRAM). Приведение к PNG/JPEG.
//!
//! Видео: `ffprobe` (длительность) → ОДИН `ffmpeg` (N равномерных кадров в TempDir).
//! Таймкоды НЕ рисуются на пикселях (drawtext/freetype на Windows хрупок) — уходят
//! структурированной шапкой в промт (см. `frame_header`). Крейт `image` видео НЕ декодит.

use std::path::Path;
use std::process::Command;

use base64::Engine;
use llama_engine_core::engine::ChatAttachment;

use super::error::VisionError;

/// Изображение → вложение (base64, без data-префикса — его добавит ядро).
///
/// `max_dim`: 0 или `no_resize=true` → без ресайза. Иначе большая сторона ≤ max_dim.
/// Валидация — по декодированию: битый/не-картинка → `Media` (код 2), не «уронит сервер».
pub fn image_to_attachment(
    path: &str,
    max_dim: u32,
    no_resize: bool,
) -> Result<ChatAttachment, VisionError> {
    let p = Path::new(path);
    let bytes = std::fs::read(p)
        .map_err(|e| VisionError::Input(format!("не прочитать картинку {path}: {e}")))?;

    // Декодируем (валидация magic-bytes + источник для ресайза).
    let img = image::load_from_memory(&bytes)
        .map_err(|e| VisionError::Media(format!("«{path}» не декодируется как картинка: {e}")))?;

    let (w, h) = (img.width(), img.height());
    let (out_bytes, mime) = if !no_resize && max_dim > 0 && (w > max_dim || h > max_dim) {
        let scale = max_dim as f64 / (w.max(h) as f64);
        let nw = ((w as f64 * scale).round() as u32).max(1);
        let nh = ((h as f64 * scale).round() as u32).max(1);
        let resized = img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3);
        let enc = encode_png(&resized)?;
        (enc, "image/png")
    } else {
        // Приводим к PNG (единый формат, без потерь; llama-server ест PNG).
        let enc = encode_png(&img)?;
        (enc, "image/png")
    };

    let data_base64 = base64::engine::general_purpose::STANDARD.encode(&out_bytes);
    Ok(ChatAttachment {
        file_name: p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        mime_type: mime.to_string(),
        data_base64,
        file_path: Some(path.to_string()),
        is_dir: Some(false),
    })
}

fn encode_png(img: &image::DynamicImage) -> Result<Vec<u8>, VisionError> {
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .map_err(|e| VisionError::Media(format!("не закодировать PNG: {e}")))?;
    Ok(buf)
}

/// Кадр видео: готовое вложение (base64 в памяти) + таймкод в секундах.
#[derive(Debug, Clone)]
pub struct Frame {
    pub attachment: ChatAttachment,
    pub time_secs: f64,
}

/// Извлечь `n` равномерных кадров из видео через ffmpeg в TempDir.
///
/// Шаги: `ffprobe` → длительность → один `ffmpeg` на кадр (быстрый seek) в TempDir.
/// Байты читаются в память ДО удаления TempDir → возвращаются готовые `ChatAttachment`.
/// Нет ffmpeg/ffprobe в PATH → `Input` (код 2).
pub fn video_to_frames(
    video: &str,
    n: usize,
    max_dim: u32,
    no_resize: bool,
) -> Result<(Vec<Frame>, String), VisionError> {
    let n = n.clamp(1, 12);
    let p = Path::new(video);
    if !p.is_file() {
        return Err(VisionError::Input(format!("видео не найдено: {video}")));
    }
    let duration = ffprobe_duration(video)?;
    let dir = tempfile::TempDir::new()
        .map_err(|e| VisionError::Internal(format!("TempDir: {e}")))?;
    let dir_path = dir.path().to_path_buf();

    // Равномерные таймкоды: t_i = (i + 0.5) * duration / n — середины n равных
    // сегментов. Никогда не 0 и не ровно duration (на которых ffmpeg не находит
    // кадр), и покрывают весь клип равномерно.
    let times: Vec<f64> = (0..n)
        .map(|i| (i as f64 + 0.5) * duration / (n as f64))
        .collect();

    let scale = if max_dim > 0 && !no_resize {
        format!("scale='min({max_dim},iw)':'min({max_dim},ih)':force_original_aspect_ratio=decrease")
    } else {
        "null".to_string()
    };

    let mut frames = Vec::with_capacity(n);
    for (i, t) in times.iter().enumerate() {
        let out = dir_path.join(format!("frame_{i:02}.png"));
        // Точный seek: -ss ПОСЛЕ -i (медленнее, но надёжно на коротких клипах;
        // быстрый seek перед -i мажет по кейфреймам и на t≈duration не находит кадр).
        let seek = t.min(duration - 0.02).max(0.0);
        let status = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                video,
                "-ss",
                &format!("{:.3}", seek),
                "-frames:v",
                "1",
                "-vf",
                &scale,
                "-y",
                out.to_string_lossy().as_ref(),
            ])
            .status()
            .map_err(|e| VisionError::Input(format!("ffmpeg недоступен в PATH: {e}")))?;
        if !status.success() || !out.is_file() {
            return Err(VisionError::Media(format!(
                "ffmpeg не извлёк кадр {i} (t={:.2}s) из {video}",
                t
            )));
        }
        // Читаем байты в память (TempDir удалится на выходе).
        let att = image_to_attachment(out.to_str().unwrap(), max_dim, no_resize)?;
        frames.push(Frame {
            attachment: att,
            time_secs: *t,
        });
    }

    let header = frame_header(video, duration, &times);
    Ok((frames, header))
}

/// Шапка с таймкодами для промта (НЕ рисуем на пикселях).
pub fn frame_header(video: &str, duration: f64, times: &[f64]) -> String {
    let name = Path::new(video)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| video.to_string());
    let listing: Vec<String> = times
        .iter()
        .enumerate()
        .map(|(i, t)| format!("Frame {} = {:.2}s", i + 1, t))
        .collect();
    format!(
        "[Media: {} frames extracted from {} ({:.2}s): {}]",
        times.len(),
        name,
        duration,
        listing.join(", ")
    )
}

fn ffprobe_duration(video: &str) -> Result<f64, VisionError> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            video,
        ])
        .output()
        .map_err(|e| VisionError::Input(format!("ffprobe недоступен в PATH: {e}")))?;
    if !out.status.success() {
        return Err(VisionError::Media(format!(
            "ffprobe не смог прочитать {video}: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.trim()
        .parse::<f64>()
        .map_err(|e| VisionError::Media(format!("ffprobe вернул не число ({text:?}): {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_header_lists_timestamps_not_pixels() {
        let h = frame_header("clip.mp4", 3.0, &[0.0, 1.5, 3.0]);
        assert!(h.contains("3 frames"));
        assert!(h.contains("clip.mp4"));
        assert!(h.contains("Frame 1 = 0.00s"));
        assert!(h.contains("Frame 3 = 3.00s"));
    }

    #[test]
    fn missing_image_is_input_error() {
        let err = image_to_attachment("nope-does-not-exist.png", 1280, false).unwrap_err();
        assert!(matches!(err, VisionError::Input(_)));
        assert_eq!(err.to_exit(), crate::exit::Exit::Input);
    }

    #[test]
    fn non_image_bytes_are_media_error() {
        // Файл есть, но это не картинка — валидация по декодированию, не по расширению.
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("fake.png");
        std::fs::write(&p, b"this is not a png").unwrap();
        let err = image_to_attachment(p.to_str().unwrap(), 1280, false).unwrap_err();
        assert!(matches!(err, VisionError::Media(_)));
    }

    #[test]
    fn real_png_becomes_attachment() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("real.png");
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]));
        image::DynamicImage::ImageRgba8(img)
            .save(&p)
            .unwrap();
        let att = image_to_attachment(p.to_str().unwrap(), 1280, false).unwrap();
        assert_eq!(att.mime_type, "image/png");
        assert!(!att.data_base64.is_empty());
    }

    #[test]
    fn resize_shrinks_large_image() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("big.png");
        let img = image::RgbaImage::from_pixel(2000, 1000, image::Rgba([9, 9, 9, 255]));
        image::DynamicImage::ImageRgba8(img).save(&p).unwrap();
        let att = image_to_attachment(p.to_str().unwrap(), 1280, false).unwrap();
        // Декодируем обратно и проверяем, что большая сторона ≤ 1280.
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&att.data_base64)
            .unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert!(decoded.width() <= 1280 && decoded.height() <= 1280);
    }

    #[test]
    fn missing_video_is_input_error() {
        let err = video_to_frames("nope.mp4", 6, 1280, false).err();
        // Либо Input (файла нет), либо Input (ffprobe нет) — оба код 2.
        assert!(matches!(err, Some(VisionError::Input(_))));
    }

    /// Видео-тест: генерим крошечный mp4 (testsrc) через ffmpeg в TempDir и
    /// проверяем, что извлекаются N кадров с таймкодами. Если ffmpeg нет в PATH —
    /// тест проходит (skip), CI без ffmpeg не падает.
    #[test]
    fn real_video_yields_frames_with_timestamps() {
        let dir = tempfile::TempDir::new().unwrap();
        let mp4 = dir.path().join("clip.mp4");
        let made = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=64x64:rate=10",
                "-pix_fmt",
                "yuv420p",
                "-y",
                mp4.to_str().unwrap(),
            ])
            .status();
        if made.map(|s| !s.success()).unwrap_or(true) {
            eprintln!("ffmpeg недоступен — видео-тест пропущен");
            return;
        }
        let (frames, header) =
            video_to_frames(mp4.to_str().unwrap(), 4, 1280, false).expect("извлечь кадры");
        assert_eq!(frames.len(), 4, "4 кадра");
        assert!(header.contains("4 frames"));
        assert!(header.contains("clip.mp4"));
        // Кадры — валидные PNG в памяти (base64 непустой).
        for f in &frames {
            assert!(!f.attachment.data_base64.is_empty());
            assert_eq!(f.attachment.mime_type, "image/png");
        }
        // Таймкоды — середины сегментов (0.125, 0.375, 0.625, 0.875 для 1s/4).
        assert!((frames[0].time_secs - 0.125).abs() < 0.05);
        assert!((frames[3].time_secs - 0.875).abs() < 0.05);
    }
}