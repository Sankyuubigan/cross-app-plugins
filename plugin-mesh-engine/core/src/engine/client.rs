//! HTTP-клиент `POST /generate` (multipart → GLB-байты).
//!
//! Верифицировано по src/trellis-server.cpp (16.11.2026): обязательная
//! файловая часть `image`; текстовые поля seed/resolution/bg_removal/faces;
//! успех — `model/gltf-binary`; ошибка — JSON `{"error": "..."}` (400/500).
//! Поле `model` в multipart существует, но сервером не используется
//! (семейство выбирается флагом `--model` при спавне) — не шлём.

use std::path::Path;
use std::time::Duration;

/// Бюджет одной генерации (веса + диффузия + текстуры): минуты, не секунды.
pub const GEN_TIMEOUT: Duration = Duration::from_secs(1800);

/// Разрешения, принимаемые сервером (проверка ДО спавна — честный отказ).
pub const RESOLUTIONS_ALLOWED: [u32; 3] = [512, 1024, 1536];

/// Поля запроса /generate (кроме файловой части image).
#[derive(Clone, Debug)]
pub struct GenerateFields {
    /// `seed` — None → дефолт движка (help: 42).
    pub seed: Option<u32>,
    /// `resolution` — 512 | 1024 | 1536.
    pub resolution: u32,
    /// `bg_removal` — "threshold" | "birefnet"; None → дефолт сервера
    /// (threshold: безопасно на Vulkan; birefnet требует CUDA, на нём
    /// Vulkan-путь падает по OOM ~2.4 ГБ — проблема апстрима).
    pub bg_removal: Option<String>,
    /// `faces` — "auto" | целевое число QEM-граней (минимум 1000).
    pub faces: Option<String>,
}

/// Отправить изображение на генерацию, вернуть байты GLB.
pub fn post_generate(
    base_url: &str,
    image_path: &Path,
    fields: &GenerateFields,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(image_path)
        .map_err(|e| format!("не удалось прочитать изображение {}: {e}", image_path.display()))?;
    let file_name = image_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "image.png".to_string());

    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("[mesh] HTTP-клиент: {e}"))?;

    let part = reqwest::blocking::multipart::Part::bytes(bytes)
        .file_name(file_name)
        .mime_str(image_mime(image_path))
        .map_err(|e| format!("[mesh] multipart mime: {e}"))?;
    let mut form = reqwest::blocking::multipart::Form::new().part("image", part);
    if let Some(seed) = fields.seed {
        form = form.text("seed", seed.to_string());
    }
    form = form.text("resolution", fields.resolution.to_string());
    if let Some(bg) = &fields.bg_removal {
        form = form.text("bg_removal", bg.clone());
    }
    if let Some(faces) = &fields.faces {
        form = form.text("faces", faces.clone());
    }

    let resp = client
        .post(format!("{base_url}/generate"))
        .multipart(form)
        .send()
        .map_err(|e| format!("[mesh] запрос /generate не прошёл: {e}"))?;

    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = resp
        .bytes()
        .map_err(|e| format!("[mesh] тело ответа /generate: {e}"))?;

    if !status.is_success() {
        return Err(format!("[mesh] сервер вернул {status}: {}", server_error_text(&body)));
    }
    if !content_type.starts_with("model/gltf-binary") {
        return Err(format!(
            "[mesh] неожиданный Content-Type '{content_type}' (ожидался model/gltf-binary) — \
             контракт сервера изменился"
        ));
    }
    Ok(body.to_vec())
}

/// Текст ошибки сервера: JSON `{"error": "..."}` или сырой текст (обрезка
/// безопасна по границе символа — UTF-8-паника недопустима).
fn server_error_text(body: &[u8]) -> String {
    let as_str = String::from_utf8_lossy(body);
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&as_str) {
        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            return err.to_string();
        }
    }
    let trimmed = as_str.trim();
    const MAX_CHARS: usize = 400;
    if trimmed.chars().count() > MAX_CHARS {
        let cut: String = trimmed.chars().take(MAX_CHARS).collect();
        format!("{cut}…")
    } else {
        trimmed.to_string()
    }
}

/// MIME по расширению вложения (png/jpg/jpeg/webp; прочее —/octet-stream,
/// валидацию формата делает сервер).
fn image_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_text_prefers_json_error_field() {
        let body = br#"{"error":"weights not found"}"#;
        assert_eq!(server_error_text(body), "weights not found");
    }

    #[test]
    fn error_text_falls_back_to_raw() {
        assert_eq!(server_error_text(b"boom"), "boom");
    }

    #[test]
    fn error_text_truncates_on_char_boundary() {
        let long = "ж".repeat(1000);
        let out = server_error_text(long.as_bytes());
        assert_eq!(out.chars().count(), 401); // 400 символов + многоточие
    }

    #[test]
    fn mime_by_extension() {
        assert_eq!(image_mime(Path::new("a.PNG")), "image/png");
        assert_eq!(image_mime(Path::new("a.jpg")), "image/jpeg");
        assert_eq!(image_mime(Path::new("a.webp")), "image/webp");
        assert_eq!(image_mime(Path::new("a.bmp")), "application/octet-stream");
    }
}
