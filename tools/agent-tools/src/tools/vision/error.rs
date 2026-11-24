//! Ошибки тула зрения и их маппинг в коды возврата процесса.
//!
//! Единый `VisionError` вместо `Result<_, String>`: коды `2/3/4` не размазываются
//! по файлам и не схлопываются все в `4`. `to_exit()` — единственное место, где
//! ошибка превращается в [`crate::exit::Exit`].

use crate::exit::Exit;

/// Категория ошибки тула зрения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisionError {
    /// Ошибка ввода/валидации: нет файла, пустой промт, не-картинка, нет ffmpeg,
    /// битое видео, неизвестный флаг. → код 2.
    Input(String),
    /// Медиа-ошибка: картинка/кадр не декодируется, ресайз не удался. → код 2.
    Media(String),
    /// Конфиг не найден: нет движка/модели/mmproj, нечего запускать. → код 2.
    ConfigMissing(String),
    /// Бэкенд: llama-server не поднялся/не ответил, HTTP-ошибка, пустой вывод
    /// модели (thinking съел бюджет). → код 3.
    Backend(String),
    /// Таймаут запроса (включая префилл картинок до первого SSE-байта). → код 3.
    Timeout(String),
    /// Внутренняя ошибка (lock, ctrlc-регистрация и т.п.). → код 4.
    Internal(String),
}

impl VisionError {
    /// Единственное место маппинга ошибки в код процесса.
    pub fn to_exit(&self) -> Exit {
        match self {
            VisionError::Input(_) | VisionError::Media(_) | VisionError::ConfigMissing(_) => Exit::Input,
            VisionError::Backend(_) | VisionError::Timeout(_) => Exit::Network,
            VisionError::Internal(_) => Exit::Internal,
        }
    }

    /// Человекочитаемый текст для stderr (агент читает код, но текст — для юзера).
    pub fn message(&self) -> &str {
        match self {
            VisionError::Input(s)
            | VisionError::Media(s)
            | VisionError::ConfigMissing(s)
            | VisionError::Backend(s)
            | VisionError::Timeout(s)
            | VisionError::Internal(s) => s,
        }
    }
}

impl std::fmt::Display for VisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tag = match self {
            VisionError::Input(_) => "ОШИБКА ВВОДА",
            VisionError::Media(_) => "ОШИБКА МЕДИА",
            VisionError::ConfigMissing(_) => "НЕТ КОНФИГА",
            VisionError::Backend(_) => "ОШИБКА БЭКЕНДА",
            VisionError::Timeout(_) => "ТАЙМАУТ",
            VisionError::Internal(_) => "ВНУТРЕННЯЯ ОШИБКА",
        };
        write!(f, "[{}] {}", tag, self.message())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_family_maps_to_exit_2() {
        assert_eq!(VisionError::Input("x".into()).to_exit(), Exit::Input);
        assert_eq!(VisionError::Media("x".into()).to_exit(), Exit::Input);
        assert_eq!(VisionError::ConfigMissing("x".into()).to_exit(), Exit::Input);
    }

    #[test]
    fn backend_family_maps_to_exit_3() {
        assert_eq!(VisionError::Backend("x".into()).to_exit(), Exit::Network);
        assert_eq!(VisionError::Timeout("x".into()).to_exit(), Exit::Network);
    }

    #[test]
    fn internal_maps_to_exit_4() {
        assert_eq!(VisionError::Internal("x".into()).to_exit(), Exit::Internal);
    }

    #[test]
    fn display_carries_tag_and_message() {
        let text = VisionError::Backend("сервер молчит".into()).to_string();
        assert!(text.contains("ОШИБКА БЭКЕНДА"));
        assert!(text.contains("сервер молчит"));
    }
}