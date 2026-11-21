//! VRAM/RAM-preflight для бандла изображений.
//!
//! Три порога (считаются статически, без запуска):
//! - full-resident: всё в VRAM (макс. скорость) — сумма весов + буферы + запас;
//! - offload-минимум: diffusion + compute-буферы + запас (TE/VAE стейджатся из RAM);
//! - RAM-минимум: веса должны влезть в оперативку (params живут в RAM при offload).
//!
//! Вердикт: Fast → молча быстро; Offload → стейджинг + честная запись в лог;
//! Insufficient → понятная ошибка, а не чёрный квадрат.
//!
//! ВАЖНО: вердикт Offload НЕ означает «передать `--offload-to-cpu`». Такой флаг
//! подставляет `--params-backend '*=cpu'`, а явный `--params-backend` отключает
//! auto-fit (docs/backend.md) — веса всех модулей остаются в RAM и стримятся по
//! PCIe на каждом шаге. Разложение весов делает сам auto-fit движка.

use serde::Serialize;

use crate::engine::models_catalog::{bundle_disk_bytes, ImageBundleEntry};

/// Compute-буферы diffusion + запас (МБ). Уточняется замером на железе.
pub const COMPUTE_BUFFERS_MB: f64 = 2048.0;
/// Запас поверх расчёта (МБ).
pub const SAFETY_RESERVE_MB: f64 = 1024.0;

/// Профиль запроса для preflight (Баг C, 21.11.2026): compute-буферы зависят
/// от резолюции и режима, а константный 2048 МБ занижал пик на ~1 ГБ и
/// пропускал в генерацию заведомо-непроходимые запросы (8 ГБ Pascal → CUDA
/// OOM в середине job → слепые ретраи). Дефолт — базовый замер 1024² t2i.
#[derive(Clone, Copy, Debug)]
pub struct ImageRequestProfile {
    pub width: u32,
    pub height: u32,
    /// edit/img2img/upscale: workspace под референс + латент + второе
    /// conditioning-прохождение.
    pub edit_mode: bool,
}

impl Default for ImageRequestProfile {
    fn default() -> Self {
        Self { width: 1024, height: 1024, edit_mode: false }
    }
}

/// Compute capability GPU 0 через NVML: `Some((major, minor))`, `None` —
/// NVML недоступен (CPU-машина / не-NVIDIA) или GPU не отвечает.
pub fn gpu_compute_capability() -> Option<(u32, u32)> {
    let nvml = nvml_wrapper::Nvml::init().ok()?;
    let device = nvml.device_by_index(0).ok()?;
    let cap = device.cuda_compute_capability().ok()?;
    if cap.major <= 0 {
        return None;
    }
    Some((cap.major as u32, cap.minor as u32))
}

/// Честный буфер от запроса (Баг C, 21.11.2026 — формула спеца, ответ
/// `06-ответ.md`; начальная оценка, точная калибровка — телеметрия VRAM-пиков):
/// - активации/workspace ∝ пикселям, базовый замер — 1024² t2i (COMPUTE_BUFFERS_MB);
/// - edit-режим ×1.5 — нижняя граница вилки спеца «×1.5-2» (референс + латент);
/// - Pascal/Volta (compute capability < 8) ×1.5 — FP32-активации (спец: «×2
///   на activations»; множитель применяется к части буферов, поэтому 1.5
///   суммарно).
pub fn compute_buffers_for_request_mb(
    width: u32,
    height: u32,
    edit_mode: bool,
    fp32_gpu: bool,
) -> f64 {
    let px_scale =
        ((width.max(64) as f64) * (height.max(64) as f64)) / (1024.0 * 1024.0);
    let mode_scale: f64 = if edit_mode { 1.5 } else { 1.0 };
    let gpu_scale: f64 = if fp32_gpu { 1.5 } else { 1.0 };
    COMPUTE_BUFFERS_MB * px_scale * mode_scale * gpu_scale
}

/// Канонические маркеры ресурсного отказа (SSOT формулировок плагина):
/// image- и video-preflight начинают отказ ровно с них, а хост-детектор
/// (`orchestrator/vram.rs::is_resource_error`) маршрутизирует ретраи по этим
/// же строкам. Рассинхрон маркера = замаскированная ошибка + слепые ретраи
/// модели (инцидент 27.10.26: «Не хватает RAM» ≠ «Недостаточно RAM») —
/// менять формулировку только здесь.
pub mod resource_markers {
    /// На карте не хватает памяти целиком (статическое свойство машины).
    pub const VRAM: &str = "Недостаточно VRAM";
    /// Не хватает свободной оперативки под веса.
    pub const RAM: &str = "Недостаточно RAM";
    /// Не хватает места на диске под докачку.
    pub const DISK: &str = "Недостаточно места на диске";
    /// Суммарно не хватает VRAM+RAM (совместный минимум).
    pub const MEMORY: &str = "Недостаточно памяти";
}

#[derive(Serialize, Clone, Debug)]
pub struct MemoryEstimate {
    /// Суммарный размер файлов бандла на диске, МБ.
    pub disk_mb: f64,
    /// Full-resident: всё + буферы + запас, МБ.
    pub full_mb: f64,
    /// Offload-минимум: diffusion + буферы + запас, МБ.
    pub min_mb: f64,
    /// RAM-минимум: веса + запас, МБ.
    pub ram_need_mb: f64,
    /// Свободно VRAM сейчас (NVML), МБ. 0 — NVML недоступен.
    pub vram_free_mb: f64,
    /// Всего VRAM (NVML), МБ. 0 — NVML недоступен.
    pub vram_total_mb: f64,
    /// Свободно RAM сейчас, МБ.
    pub ram_free_mb: f64,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreflightVerdict {
    Fast,
    Offload,
    Insufficient,
}

/// Размер diffusion-файла бандла (для offload-минимума).
fn diffusion_bytes(entry: &ImageBundleEntry) -> u64 {
    entry
        .files
        .iter()
        .find(|f| f.role == "diffusion")
        .and_then(|f| f.size_bytes)
        .unwrap_or_else(|| bundle_disk_bytes(entry))
}

/// Статическая оценка по бандлу + факты NVML/sysinfo (базовый профиль 1024² t2i).
pub fn estimate_image_memory_mb(entry: &ImageBundleEntry) -> MemoryEstimate {
    estimate_image_memory_mb_for(entry, &ImageRequestProfile::default())
}

/// То же с профилем запроса (Баг C): буферы от резолюции/режима/GPU.
pub fn estimate_image_memory_mb_for(
    entry: &ImageBundleEntry,
    req: &ImageRequestProfile,
) -> MemoryEstimate {
    const MIB: f64 = 1024.0 * 1024.0;
    // Compute-буфер: замер бандла точнее общей константы (напр. Krea2 при
    // img2img 2048² буферизует больше, чем 2048 МБ — решает замер §4 BUNDLES.md).
    // БЕЗ замера — честная формула от запроса (compute_buffers_for_request_mb):
    // Pascal/Volta-флаг берётся с GPU 0 (None → не FP32, прежняя база).
    let measured = entry.compute_buffers_mb.unwrap_or_else(|| {
        let fp32_gpu = gpu_compute_capability().is_some_and(|(major, _)| major < 8);
        compute_buffers_for_request_mb(req.width, req.height, req.edit_mode, fp32_gpu)
    });
    let buffers_mb = measured;
    let disk_mb = bundle_disk_bytes(entry) as f64 / MIB;
    let full_mb = disk_mb + buffers_mb + SAFETY_RESERVE_MB;
    let min_mb = diffusion_bytes(entry) as f64 / MIB + buffers_mb + SAFETY_RESERVE_MB;
    let ram_need_mb = disk_mb + SAFETY_RESERVE_MB;

    let (vram_free_mb, vram_total_mb) = match nvml_wrapper::Nvml::init() {
        Ok(nvml) => match nvml.device_by_index(0).and_then(|d| d.memory_info()) {
            Ok(mem) => (
                mem.free as f64 / MIB,
                mem.total as f64 / MIB,
            ),
            Err(_) => (0.0, 0.0),
        },
        Err(_) => (0.0, 0.0),
    };

    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let ram_free_mb = sys.available_memory() as f64 / MIB;

    MemoryEstimate {
        disk_mb,
        full_mb,
        min_mb,
        ram_need_mb,
        vram_free_mb,
        vram_total_mb,
        ram_free_mb,
    }
}

/// Вердикт preflight: влез full → Fast; влез минимум → Offload; иначе Insufficient.
/// Без NVML (CPU-машина / не-NVIDIA) — Offload, если хватает RAM.
/// Базовый профиль 1024² t2i (вызовы, не знающие запроса).
pub fn preflight_check(entry: &ImageBundleEntry) -> (PreflightVerdict, MemoryEstimate, String) {
    preflight_check_for(entry, &ImageRequestProfile::default())
}

/// Вердикт preflight под конкретный запрос (Баг C): буферы от резолюции/режима.
pub fn preflight_check_for(
    entry: &ImageBundleEntry,
    req: &ImageRequestProfile,
) -> (PreflightVerdict, MemoryEstimate, String) {
    let est = estimate_image_memory_mb_for(entry, req);
    if est.vram_total_mb <= 0.0 {
        // NVML недоступен: полагаемся на RAM + CPU-режим.
        if est.ram_free_mb >= est.ram_need_mb {
            return (
                PreflightVerdict::Offload,
                est,
                "NVML недоступен (нет NVIDIA GPU) — режим CPU/стейджинг из RAM.".to_string(),
            );
        }
        return (
            PreflightVerdict::Insufficient,
            est.clone(),
            format!(
                "{}: нужно ~{:.1} ГБ для весов бандла, свободно {:.1} ГБ.",
                resource_markers::RAM,
                est.ram_need_mb / 1024.0,
                est.ram_free_mb / 1024.0
            ),
        );
    }
    if est.vram_free_mb >= est.full_mb {
        (
            PreflightVerdict::Fast,
            est,
            "Бандл влезает в VRAM целиком — максимальная скорость.".to_string(),
        )
    } else if est.vram_free_mb >= est.min_mb && est.ram_free_mb >= est.ram_need_mb {
        (
            PreflightVerdict::Offload,
            est.clone(),
            format!(
                "Бандл не влезает в VRAM целиком (~{:.1} ГБ) — auto-fit разнесёт веса: \
                 diffusion в VRAM, текст-энкодер/VAE в RAM (их GPU-копии освободятся \
                 после своей фазы, diffusion останется в VRAM на всех шагах).",
                est.full_mb / 1024.0
            ),
        )
    } else {
        (
            PreflightVerdict::Insufficient,
            est.clone(),
            format!(
                "{}: минимум ~{:.1} ГБ VRAM + ~{:.1} ГБ RAM. Свободно: {:.1} ГБ VRAM, {:.1} ГБ RAM.",
                resource_markers::MEMORY,
                est.min_mb / 1024.0,
                est.ram_need_mb / 1024.0,
                est.vram_free_mb / 1024.0,
                est.ram_free_mb / 1024.0
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::models_catalog::load_image_catalog;

    #[test]
    fn estimate_matches_catalog_thresholds() {
        let catalog = load_image_catalog();
        assert_eq!(
            catalog.len(),
            6,
            "в каталоге 6 бандлов (2 qwen-turbo + 2 krea2-turbo + 1 flux3-klein + 1 minimax-h3-image)"
        );
        for e in &catalog {
            let est = estimate_image_memory_mb(e);
            // Диск: из bytes каталога. Qwen-стек 11.2–11.4 ГБ десятичных;
            // Krea2: Q4_K_M 10.8 ГБ, Q3_K_M 9.1 ГБ;
            // flux3 AD-Q4_K: diffusion 3.65 + TE 2.5 + VAE 0.34 ≈ 6.2 ГиБ;
            // minimax_h3_image Q4_K_M: diffusion 11.4 + TE 13.1 + VAE 1.2
            // ≈ 25.7 ГБ десятичных (~24.5 ГиБ).
            assert!(
                est.disk_mb > 6_000.0 && est.disk_mb < 27_000.0,
                "{}: disk {}",
                e.name,
                est.disk_mb
            );
            // Full-режим дороже min (буферы/вторая копия весов) — инвариант формулы.
            assert!(est.full_mb > est.min_mb, "{}: full {} <= min {}", e.name, est.full_mb, est.min_mb);
            // Замер 19.10.2026: turbo full-пик 13.0–14.6 ГБ — минимумы бандлов
            // по диапазону: diffusion 3.5–7.2 ГБ + буферы (2048) + резерв ⇒ 6–11 ГБ;
            // minimax_h3_image: diffusion 10.9 ГиБ + буферы (2048) + резерв
            // ⇒ ~13.7 ГБ. Верх 16 ГБ: формула Бага C на FP32-GPU (Pascal)
            // даёт буферы ×1.5 ⇒ minimax min до ~15.9 ГБ.
            assert!(
                est.min_mb > 5000.0 && est.min_mb < 16000.0,
                "{}: min {}",
                e.name,
                est.min_mb
            );
            // У КАЖДОГО набора редактирование (правило юзера): qwen/krea2 несут
            // mmproj — 4 файла; flux3 и minimax_h3_image правят нативно БЕЗ
            // mmproj (docs/flux3.md, docs/minimax_h3_image.md) — 3 файла.
            let expected_files = if e.name.starts_with("flux3")
                || e.name.starts_with("minimax_h3_image")
            {
                3
            } else {
                4
            };
            assert_eq!(e.files.len(), expected_files, "{}: файлов {}", e.name, e.files.len());
        }
    }

    /// Замер бандла перекрывает константу (SSOT расчёта preflight): None —
    /// COMPUTE_BUFFERS_MB, Some(x) — ровно x (правило «замер, а не подбор»).
    #[test]
    fn per_bundle_compute_buffers_used_by_estimate() {
        let make_entry = |buffers: Option<f64>| ImageBundleEntry {
            name: "synthetic".to_string(),
            label: String::new(),
            note: String::new(),
            is_default: false,
            files: vec![crate::engine::models_catalog::BundleFile {
                role: "diffusion".to_string(),
                filename: "synthetic-gb.gguf".to_string(),
                download_url: String::new(),
                // 1 Гиби = 1 073 741 824 байта = ровно 1024 МБ.
                size_bytes: Some(1_073_741_824),
                modes: Vec::new(),
                is_default: false,
            }],
            model_args: None,
            lora_base: None,
            compute_buffers_mb: buffers,
            // F4: синтетическая запись для замера формулы памяти — ref-edit
            // контракт не объявлен (estimate травится только весами/буфером).
            ref_edit_mode: None,
            ref_edit_lora: None,
            ref_edit_preset: None,
            ref_edit_lora_strength: 1.0,
            preset: Default::default(),
            size_gb: None,
            vram_fast_gb: None,
            vram_min_gb: None,
            ram_min_gb: None,
        };

        // None → базовая формула от профиля по умолчанию (1024² t2i): множитель
        // FP32-GPU зависит от железа машины (Баг C), поэтому ожидание считаем
        // той же формулой — фиксируется КОНТРАКТ «None → формула», а не число.
        let none_entry = make_entry(None);
        let est = estimate_image_memory_mb(&none_entry);
        let fp32_gpu =
            gpu_compute_capability().is_some_and(|(major, _)| major < 8);
        let expected_buffers =
            compute_buffers_for_request_mb(1024, 1024, false, fp32_gpu);
        assert!(
            (est.min_mb - (1024.0 + expected_buffers + SAFETY_RESERVE_MB)).abs() < 0.5,
            "min без замера = 1024 + формула({}) + SAFETY(1024), got {}",
            expected_buffers,
            est.min_mb
        );

        // Some(512) → замер важнее константы: min = 1024 + 512 + 1024 = 2560 МБ.
        let measured_entry = make_entry(Some(512.0));
        let est = estimate_image_memory_mb(&measured_entry);
        assert!(
            (est.min_mb - 2560.0).abs() < 0.5,
            "min с замером 512: diffusion 1024 + 512 + 1024, got {}",
            est.min_mb
        );
        assert!(
            (est.full_mb - (1024.0 + 512.0 + 1024.0)).abs() < 0.5,
            "full с замером: идёт та же величина буфера, got {}",
            est.full_mb
        );
    }

    /// Баг C (21.11.2026): честная формула буфера от запроса. Базовый замер —
    /// 1024² t2i не-FP32 GPU = ровно COMPUTE_BUFFERS_MB; резолюция ×N,
    /// edit ×1.5, FP32-GPU ×1.5 (спец: 06-ответ.md).
    #[test]
    fn compute_buffers_scale_with_request_profile() {
        let base = compute_buffers_for_request_mb(1024, 1024, false, false);
        assert!(
            (base - COMPUTE_BUFFERS_MB).abs() < 0.5,
            "базовый профиль = замер 1024² t2i, got {}",
            base
        );
        let hi_res = compute_buffers_for_request_mb(2048, 2048, false, false);
        assert!(
            (hi_res - COMPUTE_BUFFERS_MB * 4.0).abs() < 0.5,
            "2048² = ×4 пикселей, got {}",
            hi_res
        );
        let edit = compute_buffers_for_request_mb(1024, 1024, true, false);
        assert!(
            (edit - COMPUTE_BUFFERS_MB * 1.5).abs() < 0.5,
            "edit = ×1.5 workspace, got {}",
            edit
        );
        let pascal = compute_buffers_for_request_mb(1024, 1024, false, true);
        assert!(
            (pascal - COMPUTE_BUFFERS_MB * 1.5).abs() < 0.5,
            "FP32-GPU (Pascal) = ×1.5, got {}",
            pascal
        );
        // Малые размеры не рушат формулу (кламп 64 на сторону — защита от нуля);
        // масштаб строго пропорционален пикселям: 64²/1024² ≈ 0.0039.
        let small = compute_buffers_for_request_mb(64, 64, false, false);
        let tiny_scale = (64.0 * 64.0) / (1024.0 * 1024.0);
        assert!(
            (small - COMPUTE_BUFFERS_MB * tiny_scale).abs() < 0.5,
            "64² = ×{tiny_scale:.4}, got {}",
            small
        );
        // Профиль по умолчанию — базовый замер (обратная совместимость вызовов).
        let prof = ImageRequestProfile::default();
        assert_eq!(prof.width, 1024);
        assert_eq!(prof.height, 1024);
        assert!(!prof.edit_mode);
    }
}
