//! Движок llama.cpp — инференс через ОТДЕЛЬНЫЙ процесс `llama-server.exe`
//! (полный релиз llama.cpp), общение по HTTP localhost (random-порт + api-key).
//! Приложение НЕ линкует llama.cpp нативно (см. docs/llama_cpp_engine.md плагина).
//!
//! Модуль — ЧИСТОЕ ЯДРО `llama-engine-core` (SSOT, без tauri): Tauri-плагин
//! ре-экспортирует его через `tauri_plugin_llama_engine::engine::*` с
//! API-совместимыми обёртками, GPUI-хосты берут крейт напрямую.
//!
//! Точки инъекции хоста (см. crate::lib):
//! - `downloader::set_progress_hook` — прогресс скачивания (плагин эмитит
//!   `download_progress`, GPUI-хосты рисуют свой бар);
//! - `downloader::set_fallback_downloader` — фоллбэк-скачивание (плагин
//!   адаптирует единый движок `tauri_plugin_downloader`);
//! - `vram::subscribe` — VRAM-уведомления (плагин эмитит `vram_notice`);
//! - `analytics::set_track_hook` — аналитика (плагин при feature `host-logs`
//!   мостит в `tauri_plugin_logs::track_event`).

pub mod analytics;
pub mod config;
pub mod detokenizer;
#[cfg(test)]
mod engine_liveness;
pub mod downloader;
pub mod gpu_detector;
pub mod llamacpp_installer;
pub mod llm;
pub mod llm_gguf;
pub mod llm_multimodal;
pub mod llm_types;
pub mod mem_profiler;
pub mod mmproj;
pub mod process_util;
pub mod sources;
pub mod startup_diagnosis;
pub mod stream_diagnostics;
pub mod vram;
pub mod vram_estimate;

// ── Re-exports (совместимость с прежним фасадом хоста `crate::infra::*`) ──

pub use config::{
    auto_detect_mmproj, find_catalog_entry_for_model, find_sibling_llm_for_mmproj, is_mmproj_file,
    load_catalog, load_config, load_config_early, save_config, CatalogEntry, EngineConfig,
    ModelMeta, ModelParams,
};
pub use llm::{
    build_json_object_grammar_with_keys, build_json_only_grammar, chain_err, estimate_vram_mb,
    estimate_vram_mb_with_spec,
    extract_f32_from_gguf, extract_gguf_arch, extract_model_filename, extract_u32_from_gguf,
    extract_u32_with_arch, get_hybrid_grammar, get_hybrid_grammar_with_think_budget,
    get_json_contract_grammar, auto_think_budget_chars, auto_think_budget_chars_with_envelope,
    strict_json_string_rules,
    ENVELOPE_TOKENS, CHARS_PER_TOKEN, THINK_BODY_RULE, ChatAttachment, GrammarSpec, LlamaEngine,
    LlmMessage, PromptFormat, ToolDefinition, FunctionDef, ToolCall,
};
pub use llm_gguf::{extract_context_length, extract_i64_array_from_gguf, extract_i64_array_with_arch};
pub use mem_profiler::{current_process_rss, peak_line, MemGuard, MemSampler};
pub use mmproj::ensure_mmproj_for_model;
pub use startup_diagnosis::{
    build_context, diagnose_shared_kv_before_spawn, diagnose_startup_failure, log_full_trace,
    DiagnosisContext, StartupDiagnosis, StartupFailureKind,
};
pub use vram::notify_vram;
pub use vram_estimate::{
    cache_type_bytes, current_kv_spec, estimate_vram, estimate_vram_with_spec,
    kv_spec_from_source, max_fitting_ngl_safe_with_spec, vram_for_ngl_with_spec, KvQuantSpec,
};
