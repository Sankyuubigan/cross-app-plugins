//! Инструмент `vision` — «глаза» для ИИ-агентов без зрения.
//!
//! Поток: parse → resolve_prompt → resolve_paths → prepare_media → infer →
//! validate_non_empty → stdout. Коды: 0 ок / 2 ввод / 3 бэкенд-таймаут / 4 внутренняя.
//! Лимита вызовов НЕТ (VLM локальна и бесплатна) — код 1 не используется.

pub mod cli;
pub mod engine;
pub mod error;
pub mod media;

use clap::Parser;

use crate::exit::Exit;

use self::cli::{VisionArgs, VisionParser, VisionSub};
use self::engine::{resolve_paths, LlamaCoreRunner, VisionRunner};
use self::error::VisionError;
use self::media::{image_to_attachment, video_to_frames};

/// Разобрать `argv` и выполнить инструмент.
pub fn run(argv: Vec<String>) -> Exit {
    match VisionParser::try_parse_from(argv) {
        Ok(parsed) => match parsed.command {
            VisionSub::Vision(args) => execute(&args),
        },
        Err(err) => {
            let _ = err.print();
            if err.exit_code() == 0 {
                Exit::Ok
            } else {
                Exit::Input
            }
        }
    }
}

fn execute(args: &VisionArgs) -> Exit {
    match execute_inner(args) {
        Ok(text) => {
            println!("{}", text);
            Exit::Ok
        }
        Err(err) => {
            eprintln!("{}", err);
            err.to_exit()
        }
    }
}

fn execute_inner(args: &VisionArgs) -> Result<String, VisionError> {
    // 1. Промт.
    let prompt = args
        .resolve_prompt()
        .map_err(VisionError::Input)?;
    if prompt.trim().is_empty() {
        return Err(VisionError::Input("пустой промт — нечего спрашивать".into()));
    }

    // 2. Медиа.
    if !args.has_media() {
        return Err(VisionError::Input(
            "нужна ≥1 картинка (--image) или видео (--video)".into(),
        ));
    }
    let (attachments, media_note) = prepare_media(args)?;

    // 3. Движок/модель.
    let cfg = llama_engine_core::engine::load_config_early();
    let paths = resolve_paths(
        args.server_url.as_deref(),
        args.engine_dir.as_deref(),
        args.model.as_deref(),
        args.mmproj.as_deref(),
        &cfg,
    );

    // 4. Инференс.
    let runner = LlamaCoreRunner::new(paths, args.verbose, engine::timeout_duration(args.timeout));
    let max_tokens = 2048usize;
    let mut text = runner.infer(
        &prompt,
        &attachments,
        max_tokens,
        args.reasoning_budget(),
    )?;

    // 5. Шапка с таймкодами (если видео) — в stdout перед ответом.
    if let Some(note) = media_note {
        text = format!("{note}\n\n{text}");
    }
    Ok(text)
}

/// Картинки → attachments (+ опц. шапка таймкодов для видео).
fn prepare_media(args: &VisionArgs) -> Result<(Vec<llama_engine_core::engine::ChatAttachment>, Option<String>), VisionError> {
    let mut attachments = Vec::new();
    for img in args.images() {
        attachments.push(image_to_attachment(img, args.max_dim, args.no_resize)?);
    }
    let mut note = None;
    if let Some(video) = &args.video {
        let (frames, header) = video_to_frames(video, args.max_frames, args.max_dim, args.no_resize)?;
        for f in frames {
            attachments.push(f.attachment);
        }
        note = Some(header);
    }
    Ok((attachments, note))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_prompt_is_input_error() {
        // parse + execute_inner без медиа/промта → Input (код 2).
        let args = VisionArgs {
            prompt: Some("   ".into()),
            prompt_file: None,
            image: vec![],
            video: None,
            max_frames: 6,
            max_dim: 1280,
            no_resize: false,
            timeout: 120,
            server_url: None,
            engine_dir: None,
            model: None,
            mmproj: None,
            think: false,
            no_think: false,
            verbose: false,
        };
        let err = execute_inner(&args).unwrap_err();
        assert!(matches!(err, VisionError::Input(_)));
    }

    #[test]
    fn no_media_is_input_error() {
        let args = VisionArgs {
            prompt: Some("что тут".into()),
            prompt_file: None,
            image: vec![],
            video: None,
            max_frames: 6,
            max_dim: 1280,
            no_resize: false,
            timeout: 120,
            server_url: None,
            engine_dir: None,
            model: None,
            mmproj: None,
            think: false,
            no_think: false,
            verbose: false,
        };
        let err = execute_inner(&args).unwrap_err();
        assert!(matches!(err, VisionError::Input(_)));
        assert_eq!(err.to_exit(), Exit::Input);
    }
}