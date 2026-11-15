//! Проверка minisign-подписи установщика (эталон — официальный tauri-plugin-updater).
//!
//! ## Ловушка внешнего Base64-слоя
//!
//! В latest.json (формат Tauri v2) поле `signature` — это **Base64 текста
//! .sig-файла**: раскодировав его, получаем ASCII-текст, начинающийся с
//! «untrusted comment:». Тот же слой может быть и вокруг pubkey в конфиге
//! хоста. Поэтому всё, что приходит извне, прогоняется через
//! [`unwrap_base64_layer`]: Base64 → UTF-8 → «untrusted comment:», с фолбеком
//! на уже раскрытый текст.
//!
//! ## Потоковая проверка
//!
//! `verify_stream` из minisign-verify хэширует файл порциями (Blake2b-prehash,
//! алгоритм «ED» — тот же, что в подписях Tauri) — установщик на 60+ МБ не
//! грузится в RAM целиком.

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use std::io::Read;
use std::path::Path;

/// Размер порции для потокового хэширования установщика.
const CHUNK: usize = 256 * 1024;

/// Снять внешний Base64-слой Tauri-обёртки: Base64 → UTF-8 → «untrusted comment:».
/// Если вход уже раскрыт (plain minisign-текст) — вернуть как есть.
pub fn unwrap_base64_layer(input: &str) -> Result<String, String> {
    let s = input.trim();
    if s.starts_with("untrusted comment:") {
        return Ok(s.to_string());
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(s)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(s))
        .map_err(|e| format!("поле не Base64 и не plain minisign-текст: {}", e))?;
    let text = String::from_utf8(decoded)
        .map_err(|_| "Base64-слой не содержит UTF-8 minisign-текст".to_string())?;
    let text = text.trim().to_string();
    if text.starts_with("untrusted comment:") {
        Ok(text)
    } else {
        Err("внутри Base64-слоя нет minisign-текста («untrusted comment: …»)".to_string())
    }
}

/// Потоково проверить подпись файла: читает порциями, файл не грузится в RAM.
/// `signature` и `pubkey` — plain minisign-текст ИЛИ Base64-обёртка Tauri.
pub fn verify_installer_file(path: &Path, signature: &str, pubkey: &str) -> Result<(), String> {
    let sig_text = unwrap_base64_layer(signature)?;
    let pk_text = unwrap_base64_layer(pubkey)?;
    let signature =
        Signature::decode(&sig_text).map_err(|e| format!("подпись не разобрана: {}", e))?;
    let public_key = PublicKey::decode(&pk_text)
        .map_err(|e| format!("публичный ключ не разобран: {}", e))?;

    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("установщик {} не открыт: {}", path.display(), e))?;
    let mut verifier = public_key
        .verify_stream(&signature)
        .map_err(|e| format!("потоковая проверка не начата: {}", e))?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("чтение установщика: {}", e))?;
        if n == 0 {
            break;
        }
        verifier.update(&buf[..n]);
    }
    verifier.finalize().map_err(|_| {
        log::error!(
            "[about-updates] подпись установщика {} НЕ сошлась",
            path.display()
        );
        "подпись установщика не соответствует публичному ключу".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Фикстуры из тестов самого minisign-verify (prehashed-алгоритм «ED», тот
    // же, что подписывает Tauri): подпись содержимого "test".
    const TEST_PUBKEY_B64: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const TEST_SIG_PLAIN: &str = "untrusted comment: signature from minisign secret key\n\
         RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n\
         trusted comment: timestamp:1556193335\tfile:test\n\
         y/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==";
    const TEST_PUBKEY_TEXT: &str = "untrusted comment: minisign public key E7620F1842B4E81F\n\
         RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";

    fn temp_file(content: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "about_updates_core_minisign_{}_{}",
            std::process::id(),
            content.len()
        ));
        std::fs::write(&p, content).unwrap();
        p
    }

    /// plain .sig проходит насквозь; Base64-обёртка (как поле signature в
    /// latest.json) снимается; мусор — ошибка.
    #[test]
    fn unwraps_base64_layer_and_passes_plain() {
        assert_eq!(
            unwrap_base64_layer(TEST_SIG_PLAIN).unwrap(),
            TEST_SIG_PLAIN.trim()
        );
        let wrapped = base64::engine::general_purpose::STANDARD.encode(TEST_SIG_PLAIN);
        assert_eq!(unwrap_base64_layer(&wrapped).unwrap(), TEST_SIG_PLAIN.trim());
        assert!(unwrap_base64_layer("не base64 и не minisign !!!").is_err());
    }

    /// Подпись верна для точного содержимого; подменённый файл — ошибка.
    #[test]
    fn verifies_streaming_from_file() {
        let path = temp_file(b"test");
        verify_installer_file(&path, TEST_SIG_PLAIN, TEST_PUBKEY_TEXT)
            .expect("подпись верна");

        let bad = temp_file(b"Test");
        assert!(verify_installer_file(&bad, TEST_SIG_PLAIN, TEST_PUBKEY_TEXT).is_err());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&bad);
    }

    /// Обе обёртки Tauri (поле signature И tauri-pubkey из конфига) снимаются
    /// автоматически — «двойной Base64» не ломает проверку.
    #[test]
    fn verifies_wrapped_signature_and_pubkey() {
        let path = temp_file(b"test");
        let wrapped_sig = base64::engine::general_purpose::STANDARD.encode(TEST_SIG_PLAIN);
        let wrapped_pk = base64::engine::general_purpose::STANDARD.encode(TEST_PUBKEY_TEXT);
        verify_installer_file(&path, &wrapped_sig, &wrapped_pk)
            .expect("Base64-обёртки снимаются автоматически");
        let _ = std::fs::remove_file(&path);
    }

    /// verify_stream доступен только для prehashed-подписей («ED») — фикстура
    /// именно такая; это регресс-сторож формата Tauri.
    #[test]
    fn fixture_is_prehashed_stream_verifiable() {
        let sig = Signature::decode(TEST_SIG_PLAIN.trim()).expect("фикстура разбирается");
        let pk = PublicKey::from_base64(TEST_PUBKEY_B64).expect("ключ разбирается");
        pk.verify_stream(&sig).expect("prehashed — стрим доступен");
    }
}
