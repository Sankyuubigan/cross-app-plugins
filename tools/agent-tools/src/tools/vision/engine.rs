//! Резолв путей (чистая функция) + абстракция инференса (`VisionRunner`).
//!
//! `resolve_paths` — чистая, 100% тестируемая: цепочка приоритетов
//! `--server-url`/env → `--engine-dir/--model/--mmproj`/env → общий app_config.
//! `LlamaCoreRunner` — реальная реализация поверх `llama-engine-core`: file-lock
//! (защита от OOM при параллельных вызовах), ctrlc→cancel, timeout, VLM-дефолты LFM3.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use llama_engine_core::engine::{
    llamacpp_installer, sources, ChatAttachment, LlamaEngine, LlmMessage, ModelParams,
};

use super::error::VisionError;

/// Дефолтная модель зрения (LFM3-VL-2B) — имя из каталога.
pub const DEFAULT_VLM: &str = "lfm3-vl-2b";

/// VLM-дефолты LFM3-VL-2B (если модели нет в app_config).
fn lfm3_default_params() -> ModelParams {
    ModelParams {
        temperature: 0.1,
        top_k: 50,
        top_p: 1.0,
        min_p: 0.0,
        repetition_penalty: 1.1,
        presence_penalty: 0.0,
        dry_multiplier: 0.0,
        dry_base: 1.75,
        dry_allowed_length: 2,
        dry_penalty_last_n: 0,
        xtc_probability: 0.0,
        xtc_threshold: 0.1,
    }
}

/// Разрешённые пути для запуска (результат `resolve_paths`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaths {
    /// `Some` — внешний сервер (без спавна); `None` — спавнить локальный.
    pub server_url: Option<String>,
    /// Каталог движка (для спавна).
    pub engine_dir: Option<PathBuf>,
    /// Путь к модели .gguf.
    pub model: Option<String>,
    /// Путь к mmproj .gguf.
    pub mmproj: Option<String>,
}

/// Чистый резолв путей: CLI > env > app_config. Без I/O (кроме чтения конфига).
///
/// `cfg` передаётся снаружи (для тестов — подставной), env читается здесь.
pub fn resolve_paths(
    server_url: Option<&str>,
    engine_dir: Option<&str>,
    model: Option<&str>,
    mmproj: Option<&str>,
    cfg: &llama_engine_core::engine::EngineConfig,
) -> ResolvedPaths {
    // 1. Внешний сервер: флаг > env.
    let server = server_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| env_nonempty("AGENT_TOOLS_VISION_URL"));

    // 2. Движок/модель/mmproj: флаг > env > app_config.
    let engine = engine_dir
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| env_nonempty("AGENT_TOOLS_VISION_ENGINE_DIR").map(PathBuf::from))
        .or_else(|| cfg.llamacpp_dir.as_ref().map(PathBuf::from));

    let model = model
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| env_nonempty("AGENT_TOOLS_VISION_MODEL"))
        .or_else(|| pick_vision_model(cfg));

    let mmproj = mmproj
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| env_nonempty("AGENT_TOOLS_VISION_MMPROJ"))
        .or_else(|| pick_mmproj(cfg, model.as_deref()));

    ResolvedPaths {
        server_url: server,
        engine_dir: engine,
        model,
        mmproj,
    }
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Выбрать vision-модель из app_config: приоритет LFM3-VL, иначе первая vision:true.
fn pick_vision_model(cfg: &llama_engine_core::engine::EngineConfig) -> Option<String> {
    // Прямой путь: если в models лежит путь с lfm3-vl в имени.
    if let Some(p) = cfg.models.iter().find(|m| {
        let low = m.to_lowercase();
        low.contains("lfm3-vl") || low.contains("lfm3_vl")
    }) {
        return Some(p.clone());
    }
    // По model_meta.vision == true.
    cfg.model_meta
        .iter()
        .find(|(_, meta)| meta.vision)
        .map(|(path, _)| path.clone())
}

/// mmproj для модели: из mmproj_files, иначе auto_detect (локально, без сети).
fn pick_mmproj(
    cfg: &llama_engine_core::engine::EngineConfig,
    model: Option<&str>,
) -> Option<String> {
    let model = model?;
    if let Some(p) = cfg.mmproj_files.get(model) {
        if PathBuf::from(p).is_file() {
            return Some(p.clone());
        }
    }
    // Локальный авто-детект (БЕЗ сети — ensure_mmproj_for_model качает, не зовём).
    llama_engine_core::engine::auto_detect_mmproj(model)
}

/// Абстракция инференса — для unit-тестов без реального движка.
pub trait VisionRunner {
    /// Посмотреть медиа + промт, вернуть текст ответа.
    fn infer(
        &self,
        prompt: &str,
        attachments: &[ChatAttachment],
        max_tokens: usize,
        reasoning_budget: u32,
    ) -> Result<String, VisionError>;
}

/// Реальный раннер поверх `llama-engine-core`.
pub struct LlamaCoreRunner {
    paths: ResolvedPaths,
    verbose: bool,
    timeout: std::time::Duration,
}

impl LlamaCoreRunner {
    pub fn new(paths: ResolvedPaths, verbose: bool, timeout: std::time::Duration) -> Self {
        Self {
            paths,
            verbose,
            timeout,
        }
    }
}

impl VisionRunner for LlamaCoreRunner {
    fn infer(
        &self,
        prompt: &str,
        attachments: &[ChatAttachment],
        max_tokens: usize,
        reasoning_budget: u32,
    ) -> Result<String, VisionError> {
        let verbose = self.verbose;
        let log_cb = move |msg: String| {
            if verbose {
                eprintln!("[llama-engine] {msg}");
            }
        };

        // File-lock: параллельные вызовы встают в очередь, а не пробивают VRAM.
        // Только для локального спавна (внешний сервер шарится без лока).
        let _lock = if self.paths.server_url.is_none() {
            Some(acquire_vision_lock()?)
        } else {
            None
        };

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        // Ctrl+C → cancel (движок прерывает стрим/спавн, Drop убивает дерево).
        let _ = ctrlc::set_handler(move || {
            cancel_clone.store(true, Ordering::SeqCst);
        });

        // Watchdog: --timeout прерывает зависший запрос (включая префилл картинок
        // до первого SSE-байта) → cancel_flag. Спец-нюанс 3: таймаут должен
        // прерывать префилл, а не только стрим.
        let timeout = self.timeout;
        let cancel_watchdog = cancel.clone();
        let watchdog = std::thread::spawn(move || {
            std::thread::sleep(timeout);
            cancel_watchdog.store(true, Ordering::SeqCst);
        });

        let messages = vec![LlmMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
            ..Default::default()
        }];

        let outcome = self.run_inference(&messages, attachments, max_tokens, reasoning_budget, &cancel, log_cb);
        // Watchdog больше не нужен — отменяем (cancel уже мог выставиться, это ок).
        cancel.store(false, Ordering::SeqCst);
        drop(watchdog);

        let text = outcome?.trim().to_string();
        if text.is_empty() {
            return Err(VisionError::Backend(format!(
                "модель вернула пустой ответ. Возможно, весь бюджет ушёл в «думатель» — \
                 увеличьте max_tokens или добавьте --no-think"
            )));
        }
        Ok(text)
    }
}

impl LlamaCoreRunner {
    /// Собственно инференс (спавн/внешний + generate_chat_multimodal).
    /// Вынесен, чтобы `infer` оборачивал его watchdog/lock.
    fn run_inference(
        &self,
        messages: &[LlmMessage],
        attachments: &[ChatAttachment],
        max_tokens: usize,
        reasoning_budget: u32,
        cancel: &Arc<AtomicBool>,
        log_cb: impl Fn(String) + Send + Sync + 'static,
    ) -> Result<String, VisionError> {
        let result = if let Some(url) = &self.paths.server_url {
            let engine = LlamaEngine::connect_external(
                url,
                None,
                self.paths.model.as_deref().unwrap_or(DEFAULT_VLM),
                self.paths.mmproj.as_deref(),
                8192,
                reasoning_budget,
                log_cb,
                |_| {},
            )
            .map_err(map_backend_err)?;
            engine
                .generate_chat_multimodal(
                    messages,
                    attachments,
                    max_tokens,
                    &lfm3_default_params(),
                    "Auto",
                    cancel.clone(),
                    "vision",
                    None,
                    |_, _| {},
                    |_| {},
                )
                .map_err(map_backend_err)?
        } else {
            let engine_dir = self.paths.engine_dir.clone().ok_or_else(|| {
                VisionError::ConfigMissing(
                    "не задан каталог движка (нет --engine-dir и app_config.llamacpp_dir)".into(),
                )
            })?;
            let model = self.paths.model.clone().ok_or_else(|| {
                VisionError::ConfigMissing(
                    "не задана модель (нет --model и vision-модели в app_config)".into(),
                )
            })?;
            let engine = LlamaEngine::new_with_mmproj(
                &engine_dir,
                &model,
                self.paths.mmproj.as_deref(),
                8192,
                false,
                false,
                reasoning_budget,
                log_cb,
                |_| {},
            )
            .map_err(map_backend_err)?;
            engine
                .generate_chat_multimodal(
                    messages,
                    attachments,
                    max_tokens,
                    &lfm3_default_params(),
                    "Auto",
                    cancel.clone(),
                    "vision",
                    None,
                    |_, _| {},
                    |_| {},
                )
                .map_err(map_backend_err)?
        };
        Ok(result.text)
    }
}

/// Ошибка ядра (String) → VisionError: таймаут-текст → Timeout (код 3), иначе Backend.
fn map_backend_err(msg: String) -> VisionError {
    let low = msg.to_lowercase();
    if low.contains("timeout") || low.contains("таймаут") || low.contains("не ответил") {
        VisionError::Timeout(msg)
    } else {
        VisionError::Backend(msg)
    }
}

/// Межпроцессный file-lock `%APPDATA%/llama-engine/vision.lock` (std File::lock, Rust ≥1.89).
fn acquire_vision_lock() -> Result<std::fs::File, VisionError> {
    use std::io::Write;
    let dir = llama_engine_core::engine::config::app_data_dir_early();
    std::fs::create_dir_all(&dir)
        .map_err(|e| VisionError::Internal(format!("создать каталог lock: {e}")))?;
    let path = dir.join("vision.lock");
    let file = std::fs::File::create(&path)
        .map_err(|e| VisionError::Internal(format!("создать vision.lock: {e}")))?;
    // Блокируемexclusive; ОС снимет лок при падении процесса (нет залипших .lock).
    file.lock()
        .map_err(|e| VisionError::Internal(format!("lock vision.lock: {e}")))?;
    // Пишем pid для диагностики (не влияет на лок).
    let mut f = &file;
    let _ = writeln!(f, "pid={}", std::process::id());
    Ok(file)
}

/// Таймаут запроса (сек) → config для раннера (используется в mod.rs).
pub fn timeout_duration(secs: u64) -> std::time::Duration {
    std::time::Duration::from_secs(secs.max(1))
}

/// Разрешённый вариант движка → путь к llama-server.exe (для диагностики).
pub fn server_exe_path(engine_dir: &PathBuf, source: &str, variant: &str) -> PathBuf {
    llamacpp_installer::variant_dir(engine_dir, source, variant).join("llama-server.exe")
}

/// Текущий источник движка (для диагностики).
pub fn current_source(cfg: &llama_engine_core::engine::EngineConfig) -> String {
    sources::resolve_source(cfg.engine_source.as_deref())
}

/// Текущий вариант движка (для диагностики).
pub fn current_variant(cfg: &llama_engine_core::engine::EngineConfig) -> String {
    llamacpp_installer::resolve_variant(cfg.engine_variant.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use llama_engine_core::engine::EngineConfig;

    #[test]
    fn flags_win_over_config() {
        let cfg = EngineConfig::default();
        let r = resolve_paths(
            Some("http://x:1"),
            Some("/eng"),
            Some("m.gguf"),
            Some("mm.gguf"),
            &cfg,
        );
        assert_eq!(r.server_url.as_deref(), Some("http://x:1"));
        assert_eq!(r.engine_dir, Some(PathBuf::from("/eng")));
        assert_eq!(r.model.as_deref(), Some("m.gguf"));
        assert_eq!(r.mmproj.as_deref(), Some("mm.gguf"));
    }

    #[test]
    fn empty_flags_fall_through_to_config() {
        let mut cfg = EngineConfig::default();
        cfg.llamacpp_dir = Some("/from-config".into());
        let r = resolve_paths(Some("  "), Some(""), None, None, &cfg);
        assert_eq!(r.server_url, None);
        assert_eq!(r.engine_dir, Some(PathBuf::from("/from-config")));
    }

    #[test]
    fn vision_model_picked_from_meta() {
        let mut cfg = EngineConfig::default();
        cfg.models = vec!["/m/a.gguf".into(), "/m/vlm.gguf".into()];
        cfg.model_meta.insert("/m/vlm.gguf".into(), llama_engine_core::engine::ModelMeta {
            uncen: false,
            vision: true,
            audio: false,
        });
        let r = resolve_paths(None, None, None, None, &cfg);
        assert_eq!(r.model.as_deref(), Some("/m/vlm.gguf"));
    }

    #[test]
    fn lfm3_model_prioritized_over_other_vision() {
        let mut cfg = EngineConfig::default();
        cfg.models = vec!["/m/other-vlm.gguf".into(), "/m/LFM3-VL-2B.gguf".into()];
        cfg.model_meta.insert("/m/other-vlm.gguf".into(), llama_engine_core::engine::ModelMeta {
            uncen: false,
            vision: true,
            audio: false,
        });
        let r = resolve_paths(None, None, None, None, &cfg);
        assert_eq!(r.model.as_deref(), Some("/m/LFM3-VL-2B.gguf"));
    }

    #[test]
    fn no_vision_model_returns_none() {
        let cfg = EngineConfig::default();
        let r = resolve_paths(None, None, None, None, &cfg);
        assert_eq!(r.model, None);
        assert_eq!(r.mmproj, None);
    }
}