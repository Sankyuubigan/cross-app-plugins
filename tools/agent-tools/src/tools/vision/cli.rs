//! Аргументы сабкоманды `vision` (clap) + разбор намерения.
//!
//! Вход: промт (`--prompt` / `--prompt-file` / stdin через `-`) + ≥1 картинка
//! (`--image`, повторяемый — 3D-ракурсы) ИЛИ видео (`--video`). Паритет с consult
//! по чтению промта из файла/stdin — Windows-агенты ломают кавычки в `--prompt "..."`.

use clap::{ArgAction, Args, Parser, Subcommand};

/// Способ разбора одной сабкоманды (как у consult).
#[derive(Debug, Parser)]
#[command(name = "agent-tools", bin_name = "agent-tools")]
pub struct VisionParser {
    #[command(subcommand)]
    pub command: VisionSub,
}

#[derive(Debug, Subcommand)]
pub enum VisionSub {
    /// Посмотреть картинку/видео VLM и ответить на промт.
    #[command(about = ABOUT, long_about = LONG_ABOUT)]
    Vision(VisionArgs),
}

#[derive(Debug, Args, Clone)]
pub struct VisionArgs {
    /// Промт-вопрос к модели. `-` или отсутствие → stdin.
    #[arg(long, value_name = "TEXT", conflicts_with = "prompt_file")]
    pub prompt: Option<String>,

    /// Файл с промтом (альтернатива `--prompt`, удобно для многострочных/кириллицы).
    #[arg(long, value_name = "PATH", conflicts_with = "prompt")]
    pub prompt_file: Option<String>,

    /// Картинка (PNG/JPEG/WEBP/BMP/GIF/TGA). Повторяемый — несколько ракурсов.
    #[arg(long, value_name = "PATH", action = ArgAction::Append)]
    pub image: Vec<String>,

    /// Видео (mp4/mkv/webm/…). Извлекаются N равномерных кадров (см. --max-frames).
    #[arg(long, value_name = "PATH")]
    pub video: Option<String>,

    /// Сколько кадров извлекать из видео (дефолт 6, 1..=12).
    #[arg(long, value_name = "N", default_value_t = 6)]
    pub max_frames: usize,

    /// Макс. сторона кадра/картинки при ресайзе (дефолт 1280). `--no-resize` отменяет.
    #[arg(long, value_name = "PX", default_value_t = 1280)]
    pub max_dim: u32,

    /// Не ресайзить (передавать как есть).
    #[arg(long)]
    pub no_resize: bool,

    /// Таймаут запроса, сек (дефолт 120).
    #[arg(long, value_name = "SECS", default_value_t = 120)]
    pub timeout: u64,

    /// Подключиться к уже запущенному llama-server (env AGENT_TOOLS_VISION_URL).
    #[arg(long, value_name = "URL")]
    pub server_url: Option<String>,

    /// Каталог движка (llama-server.exe) — перекрывает app_config.
    #[arg(long, value_name = "DIR")]
    pub engine_dir: Option<String>,

    /// Путь к модели .gguf (перекрывает app_config/дефолт LFM3-VL).
    #[arg(long, value_name = "PATH")]
    pub model: Option<String>,

    /// Путь к mmproj .gguf (перекрывает app_config).
    #[arg(long, value_name = "PATH")]
    pub mmproj: Option<String>,

    /// Разрешить «думатель» (reasoning) с потолком 768 токенов.
    #[arg(long)]
    pub think: bool,

    /// Запретить «думатель» (reasoning off) — дефолт.
    #[arg(long, conflicts_with = "think")]
    pub no_think: bool,

    /// Подробный лог движка в stderr (по умолчанию stdout чист).
    #[arg(long)]
    pub verbose: bool,
}

/// Что должен сделать вызов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Посмотреть медиа и ответить.
    Ask,
}

impl VisionArgs {
    pub fn intent(&self) -> Intent {
        Intent::Ask
    }

    /// Промт: `--prompt-file` > `--prompt` > stdin (если `--prompt -` или пусто).
    /// Читает stdin; вызывающий код ловит пустой промт как Input.
    pub fn resolve_prompt(&self) -> Result<String, String> {
        if let Some(path) = self.prompt_file.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            return std::fs::read_to_string(path)
                .map_err(|e| format!("не прочитать --prompt-file {path}: {e}"));
        }
        match self.prompt.as_deref().map(str::trim) {
            Some("-") | None => read_stdin(),
            Some(text) if text.is_empty() => read_stdin(),
            Some(text) => Ok(text.to_string()),
        }
    }

    /// Есть ли что смотреть (≥1 картинка или видео).
    pub fn has_media(&self) -> bool {
        !self.images().is_empty() || self.video.is_some()
    }

    pub fn images(&self) -> Vec<&str> {
        self.image.iter().map(String::as_str).collect()
    }

    /// Потолок reasoning-токенов: `--think` → 768, иначе 0 (выкл).
    pub fn reasoning_budget(&self) -> u32 {
        if self.think {
            768
        } else {
            0
        }
    }
}

fn read_stdin() -> Result<String, String> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("не прочитать stdin: {e}"))?;
    Ok(buf)
}

/// Справка.
pub const ABOUT: &str = "Посмотреть картинку/видео VLM (LFM3-VL-2B) и ответить на промт";
pub const LONG_ABOUT: &str = "Принимает промт + ≥1 картинку (или видео), запускает локальную VLM \
через движок llama.cpp (plugin-llama-engine) и печатает текстовый ответ в stdout. \
Код 0 — ответ получен; 2 — ошибка ввода (нет медиа/файла/ffmpeg); 3 — бэкенд/таймаут; \
4 — внутренняя ошибка (пустой вывод модели).";

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(extra: &[&str]) -> Result<VisionArgs, clap::Error> {
        let mut argv = vec!["agent-tools", "vision"];
        argv.extend_from_slice(extra);
        VisionParser::try_parse_from(argv).map(|p| match p.command {
            VisionSub::Vision(a) => a,
        })
    }

    #[test]
    fn single_image_is_parsed() {
        let a = parse(&["--prompt", "что тут", "--image", "shot.png"]).expect("parse");
        assert_eq!(a.images(), vec!["shot.png"]);
        assert!(a.has_media());
        assert_eq!(a.intent(), Intent::Ask);
    }

    #[test]
    fn image_is_repeatable_for_angles() {
        let a = parse(&[
            "--prompt",
            "x",
            "--image",
            "front.png",
            "--image",
            "side.png",
            "--image",
            "top.png",
        ])
        .expect("parse");
        assert_eq!(a.images().len(), 3);
    }

    #[test]
    fn video_is_parsed() {
        let a = parse(&["--prompt", "x", "--video", "clip.mp4"]).expect("parse");
        assert!(a.has_media());
        assert_eq!(a.video.as_deref(), Some("clip.mp4"));
    }

    #[test]
    fn no_media_is_not_enough_but_parses() {
        // has_media() = false — валидацию делает mod.rs (код 2), clap не падает.
        let a = parse(&["--prompt", "x"]).expect("parse");
        assert!(!a.has_media());
    }

    #[test]
    fn prompt_and_prompt_file_conflict() {
        assert!(parse(&["--prompt", "a", "--prompt-file", "b.txt"]).is_err());
    }

    #[test]
    fn think_and_no_think_conflict() {
        assert!(parse(&["--prompt", "x", "--image", "i.png", "--think", "--no-think"]).is_err());
    }

    #[test]
    fn reasoning_budget_follows_flags() {
        let think = parse(&["--prompt", "x", "--image", "i.png", "--think"]).unwrap();
        assert_eq!(think.reasoning_budget(), 768);
        let no_think = parse(&["--prompt", "x", "--image", "i.png"]).unwrap();
        assert_eq!(no_think.reasoning_budget(), 0);
    }

    #[test]
    fn defaults_are_sane() {
        let a = parse(&["--prompt", "x", "--image", "i.png"]).unwrap();
        assert_eq!(a.max_frames, 6);
        assert_eq!(a.max_dim, 1280);
        assert_eq!(a.timeout, 120);
        assert!(!a.no_resize);
    }
}