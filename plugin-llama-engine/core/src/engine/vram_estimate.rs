//! Единый источник правды по оценке VRAM-потребления модели.
//!
//! Все прогнозы (pre-flight запуска, пик-линия после генерации, счётчик под
//! полем ввода в чате) считаются ТОЛЬКО здесь. Отдельные формулы в llm.rs —
//! запрещены: любое расхождение между «что юзер видит в интерфейсе» и «что
//! писал движку в лог» — баг (см. фикс фейкового VRAM-диалога, tasks/).
//!
//! KV-кэш считается по слоям внимания с учётом:
//!   - массивов `attention.head_count_kv` (per-layer GQA — gemma4 и др.);
//!   - массивов `attention.sliding_window_pattern` — НАСТОЯЩИЙ признак SWA-слоёв.
//!     У gemma4 наоборот, чем в классических гибридах: SWA-слои несут БОЛЬШЕ
//!     KV-голов (8), а dense — меньше (1). Поэтому граница «SWA vs global» не
//!     выводится из числа голов, а читается из GGUF-паттерна;
//!   - SWA-размерностей `attention.key_length_swa`/`value_length_swa`;
//!   - legacy-скаляров + `full_attention_interval` (гибриды qwen35).
//!
//! KV-кэш двухкомпонентный (повторяет llama-kv-cache-iswa.cpp):
//!   - dense (non-SWA) слои видят ПОЛНЫЙ контекст: `PAD(ctx, 256)` ячеек;
//!   - SWA-слои ограничены sliding_window: `PAD(min(ctx, n_swa·n_seq_max + n_ubatch), 256)`.
//!     n_seq_max = [`SERVER_N_PARALLEL`], n_ubatch = [`N_UBATCH`].
//!
//! `ctx_size` — ЭФФЕКТИВНЫЙ контекст (промпт + запас генерации). Вопреки более
//! ранним комментариям llama.cpp выделяет KV-кэш НЕ лениво: при создании
//! контекста резервируются все ячейки `n_ctx_seq`. Pre-flight считает по полному
//! лимиту, счётчик под полем ввода — по занятому промпту + запасу (оценка UX).
//!
//! KV-квант источника (BeeLlama KVarN `kvarn5/kvarn4` + precision tail 1024)
//! учитывается через `KvQuantSpec` / `kv_spec_from_source`: оценка должна
//! знать РЕАЛЬНЫЙ cache-type иначе pre-flight занижает `-ngl` до уровня
//! vanilla llama.cpp и выгрыш KVarN в GPU-слоях не работает.

use crate::engine::llm_gguf::{extract_gguf_arch, extract_u32_with_arch, extract_i64_array_with_arch};
use crate::engine::sources::SourceSpec;

const MIB: f64 = 1024.0 * 1024.0;

/// Число параллельных слотов (`--parallel`), с которым приложение запускает
/// `llama-server`. ЕДИНСТВЕННЫЙ источник правды: и команда запуска в `llm.rs`,
/// и расчёт SWA-окна ниже читают эту константу.
///
/// Приложение настольное и выполняет запросы СТРОГО последовательно, поэтому
/// слот ровно один. Раньше слот не передавался, и движок брал свой дефолт
/// `n_parallel=4`: `llama-server` умножает `-c` на число слотов, то есть на
/// карте резервировался KV вчетверо больший, чем нужно, а оценка в dense-ветке
/// считала одну последовательность — то есть вчетверо меньше реальности.
/// Расхождение ломало pre-flight и заставляло резать слои на CPU.
pub const SERVER_N_PARALLEL: u32 = 1;

/// `n_ubatch` — дефолт llama.cpp (аргумент движку не передаётся).
pub const N_UBATCH: u32 = 512;

/// Резерв свободной VRAM, не отдаваемый движку (PRP B2, 20.11.2026).
/// WDDM на Windows выполняет spillover GPU-аллокаций в системный Commit
/// (ОЗУ+pagefile): инцидент E5 — запрос ≈7270 МБ при свободных 7255 прошёл
/// проверку «влезает», WDDM вынес излишек в commit, Commit Limit кончился,
/// `_aligned_malloc` вернул NULL → GGML_ASSERT → 0xC0000409. Оценка спеца
/// для Windows — 768–1024 МБ; берём 1024 (консервативно, до лога -lv 4).
/// Не-Windows (Linux/Vulkan без WDDM-переаллокации) — прежние 512.
pub fn vram_reserve_mb() -> f64 {
    #[cfg(windows)]
    {
        1024.0
    }
    #[cfg(not(windows))]
    {
        512.0
    }
}

/// Оценка VRAM-потребления для контекста `ctx_size`.
#[derive(Debug, Clone, Copy)]
pub struct VramEstimate {
    /// Размер GGUF-файла (веса модели) в МБ.
    pub model_mb: f64,
    /// KV-кэш на 1 токен для dense (non-SWA) слоёв, МБ.
    pub kv_per_token_mb: f64,
    /// KV-кэш на указанный контекст (dense + SWA), МБ.
    pub kv_mb: f64,
    /// SWA-часть KV-кэша (ячейки ограничены sliding_window), МБ.
    pub kv_swa_mb: f64,
    /// Workspace CUDA-буферы (compute), МБ.
    pub buffers_mb: f64,
    /// Итого (модель + KV + буферы), МБ.
    pub total_mb: f64,
    /// Число блоков модели (для линейного пересчёта по -ngl).
    pub num_layers: u32,
    /// Число слоёв, несущих KV (≤ num_layers для гибридов).
    pub num_attn_layers: u32,
}

/// Байт-вес на элемент KV-кэша: FP16 = 2.0, q8_0 = 34/32 = 1.0625 (как в llama.cpp).
fn kv_bytes_per_elem(quant: bool) -> f64 {
    if quant { 34.0 / 32.0 } else { 2.0 }
}

/// Спецификация KV-кванта для оценки VRAM (реальный cache-type источника).
///
/// `k_bytes`/`v_bytes` — байт на элемент тела кэша; `tail_tokens` — число
/// последних токенов, которые движок хранит в F16 (`--kv-tail-tokens`).
/// Для BeeLlama: `kvarn5/kvarn4` ≈ 5.375/4.375 bpv, tail 1024 (см.
/// `engine_sources.json` → `runtime`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KvQuantSpec {
    pub k_bytes: f64,
    pub v_bytes: f64,
    pub tail_tokens: u32,
}

impl KvQuantSpec {
    /// Без кванта (vanilla llama.cpp / чекбокс выключен).
    pub fn fp16() -> Self {
        Self { k_bytes: 2.0, v_bytes: 2.0, tail_tokens: 0 }
    }

    /// Дефолтный KV-spec источника: legacy-чекбоксы → q8_0/f16, без tail.
    pub fn from_legacy_flags(kv_quant_keys: bool, kv_quant_values: bool) -> Self {
        Self {
            k_bytes: kv_bytes_per_elem(kv_quant_keys),
            v_bytes: kv_bytes_per_elem(kv_quant_values),
            tail_tokens: 0,
        }
    }
}

/// Байт на элемент для cache-type строки llama.cpp/BeeLlama.
/// bpv (bits per value) из README Anbeeld → байты = bpv / 8.
/// Неизвестный тип → None (вызывающий берёт консервативный F16).
pub fn cache_type_bytes(cache_type: &str) -> Option<f64> {
    let ct = cache_type.trim().to_ascii_lowercase();
    let bpv: f64 = match ct.as_str() {
        "f16" | "bf16" => 16.0,
        "q8_0" => 8.5,
        "q6_0" => 6.5,
        "q5_0" => 5.5,
        "q4_1" => 5.0,
        "q4_0" => 4.5,
        "q3_1" => 4.0,
        "q3_0" => 3.5,
        "q2_1" => 3.0,
        "q2_0" => 2.5,
        "kvarn8" => 8.375,
        "kvarn6" => 6.375,
        "kvarn5" => 5.375,
        "kvarn4" => 4.375,
        "kvarn3" => 3.375,
        "kvarn2" => 2.375,
        _ => return None,
    };
    Some(bpv / 8.0)
}

/// Резолв KV-spec: runtime источника (KVarN для BeeLlama) ПЕРЕКРЫВАЕТ
/// legacy-чекбоксы `kv_quant_*` — тот же приоритет, что в `build_cmd`
/// (`llm.rs`: `--cache-type-k` из `source.runtime` иначе q8_0).
pub fn kv_spec_from_source(
    source: Option<&SourceSpec>,
    kv_quant_keys: bool,
    kv_quant_values: bool,
) -> KvQuantSpec {
    if let Some(spec) = source {
        let rt = &spec.runtime;
        if rt.cache_type_k.is_some() || rt.cache_type_v.is_some() {
            let k_bytes = rt
                .cache_type_k
                .as_deref()
                .and_then(cache_type_bytes)
                .unwrap_or(2.0);
            let v_bytes = rt
                .cache_type_v
                .as_deref()
                .and_then(cache_type_bytes)
                .unwrap_or(2.0);
            return KvQuantSpec {
                k_bytes,
                v_bytes,
                tail_tokens: rt.kv_tail_tokens.unwrap_or(0),
            };
        }
    }
    KvQuantSpec::from_legacy_flags(kv_quant_keys, kv_quant_values)
}

/// KV-spec для текущего `engine_source` из `app_config.json` (без AppHandle).
/// Для call-site'ов, где `SourceSpec` уже в скоупе, — предпочитать
/// `kv_spec_from_source` (без повторного чтения конфига).
pub fn current_kv_spec(kv_quant_keys: bool, kv_quant_values: bool) -> KvQuantSpec {
    let cfg = crate::engine::config::load_config_early();
    let sid = crate::engine::sources::resolve_source(cfg.engine_source.as_deref());
    kv_spec_from_source(
        crate::engine::sources::source_spec(&sid).as_ref(),
        kv_quant_keys,
        kv_quant_values,
    )
}

/// Раскладка KV-кэша на две компоненты (dense — полный ctx, SWA — окно).
struct KvLayout {
    /// Байт на токен для dense (non-SWA) слоёв.
    global_bytes_per_token: f64,
    /// Байт на ЯЧЕЙКУ для SWA-слоёв.
    swa_bytes_per_cell: f64,
    /// Число attention-слоёв.
    attn_layers: u32,
    /// sliding_window (n_swa) — если есть SWA-слои.
    swa_window: Option<u32>,
}

/// PAD до ближайшего кратного 256 (llama-context.cpp: n_ctx = GGML_PAD(n_ctx, 256)).
fn pad256(v: u32) -> u32 {
    v.saturating_add(255) / 256 * 256
}

/// Число ячеек SWA-кэша (llama-kv-cache-iswa.cpp): SWA капится окном, а не полным
/// контекстом. Окно умножается на число параллельных слотов — берётся из
/// [`SERVER_N_PARALLEL`], то есть ровно то значение, которое передаётся движку.
fn swa_cells_count(n_ctx_seq: u32, swa_window: u32) -> u32 {
    let size = (swa_window * SERVER_N_PARALLEL + N_UBATCH).min(n_ctx_seq);
    pad256(size)
}

/// KV-раскладка (Байт/токен для dense + Байт/ячейку для SWA) по данным GGUF.
///
/// Порядок приоритета:
/// 1. per-layer массив `head_count_kv` — признак SWA берётся из массива
///    `attention.sliding_window_pattern` (истинный источник у gemma4: SWA-слои
///    могут иметь БОЛЬШЕ голов, чем dense), при отсутствии паттерна — fallback
///    «local-слой имеет меньше KV-голов»;
/// 2. `full_attention_interval` (гибрид) — attention только в каждом iv-слое;
/// 3. классика — все слои, скалярный heads_kv.
fn kv_layout_gguf(
    model_path: &str,
    arch: Option<&str>,
    num_layers: u32,
    heads_kv_scalar: u32,
    heads_scalar: u32,
    key_len_global: u32,
    value_len_global: u32,
    key_len_swa: Option<u32>,
    value_len_swa: Option<u32>,
    interval: Option<u32>,
    swa_window: Option<u32>,
    swa_pattern: Option<Vec<i64>>,
    b_k: f64,
    b_v: f64,
) -> KvLayout {
    let _ = heads_scalar; // legacy-параметр, значение не влияет на расчёт (heads_kv_scalar).
    let per = |heads: u32, klen: u32, vlen: u32| -> f64 {
        heads as f64 * (klen as f64 * b_k + vlen as f64 * b_v)
    };

    if let Some(arr) = extract_i64_array_with_arch(model_path, arch, "attention.head_count_kv") {
        // Per-layer (gemma4-style). «Глобальная» норма KV-голов — максимальное
        // значение массива; но SWA vs global определяется sliding_window_pattern,
        // а НЕ числом голов.
        let global_kv = arr.iter().copied().max().unwrap_or(heads_kv_scalar as i64).max(1) as u32;
        let has_swa_dims = key_len_swa.is_some() && value_len_swa.is_some();
        let mut global_bytes: f64 = 0.0;
        let mut swa_bytes: f64 = 0.0;
        let mut attn = 0u32;
        for l in 0..num_layers {
            let hkv = arr
                .get(l as usize)
                .copied()
                .unwrap_or(global_kv as i64)
                .clamp(1, 1_000_000) as u32;
            let is_swa = match swa_pattern.as_deref() {
                // Истинный признак SWA: per-layer bool-массив из GGUF.
                Some(p) => p.get(l as usize).copied().unwrap_or(0) != 0,
                // Fallback для гибридов без паттерна: local-слой = с меньшим числом голов.
                None => has_swa_dims && hkv < global_kv,
            };
            let (klen, vlen) = if is_swa {
                (key_len_swa.unwrap_or(key_len_global), value_len_swa.unwrap_or(value_len_global))
            } else {
                (key_len_global, value_len_global)
            };
            let bytes = per(hkv, klen, vlen);
            if is_swa { swa_bytes += bytes; } else { global_bytes += bytes; }
            attn += 1;
        }
        return KvLayout {
            global_bytes_per_token: global_bytes,
            swa_bytes_per_cell: swa_bytes,
            attn_layers: attn,
            swa_window: if swa_bytes > 0.0 { swa_window } else { None },
        };
    }

    if let Some(iv) = interval.filter(|iv| *iv >= 1 && num_layers % iv == 0) {
        let attn_layers = num_layers / iv;
        return KvLayout {
            global_bytes_per_token: per(heads_kv_scalar, key_len_global, value_len_global) * attn_layers as f64,
            swa_bytes_per_cell: 0.0,
            attn_layers,
            swa_window: None,
        };
    }

    KvLayout {
        global_bytes_per_token: per(heads_kv_scalar, key_len_global, value_len_global) * num_layers as f64,
        swa_bytes_per_cell: 0.0,
        attn_layers: num_layers,
        swa_window: None,
    }
}

/// Оценка VRAM для заданного (эффективного) контекста.
///
/// Legacy-API: `kv_quant_*` = false → F16, true → q8_0, tail = 0.
/// Для BeeLlama (KVarN + precision tail) — `estimate_vram_with_spec`.
pub fn estimate_vram(
    model_path: &str,
    ctx_size: u32,
    kv_quant_keys: bool,
    kv_quant_values: bool,
) -> VramEstimate {
    estimate_vram_with_spec(
        model_path,
        ctx_size,
        &KvQuantSpec::from_legacy_flags(kv_quant_keys, kv_quant_values),
    )
}

/// Оценка VRAM с явной спецификацией KV-кванта (KVarN, q8_0, F16 + tail).
///
/// Precision tail (`--kv-tail-tokens`): последние N токенов хранятся в F16,
/// остальное тело — в `k_bytes`/`v_bytes`. Для tail считаем второй layout
/// с F16-байтами; SWA-ячейки режутся так же.
pub fn estimate_vram_with_spec(
    model_path: &str,
    ctx_size: u32,
    spec: &KvQuantSpec,
) -> VramEstimate {
    let model_mb = std::fs::metadata(model_path)
        .map(|m| m.len() as f64 / MIB)
        .unwrap_or(0.0);

    let arch = extract_gguf_arch(model_path);
    let num_layers = extract_u32_with_arch(model_path, arch.as_deref(), "block_count").unwrap_or(32);
    let embd = extract_u32_with_arch(model_path, arch.as_deref(), "embedding_length").unwrap_or(4096);
    let heads = extract_u32_with_arch(model_path, arch.as_deref(), "attention.head_count").unwrap_or(32);
    let head_dim = embd / heads.max(1);
    let heads_kv_scalar = extract_u32_with_arch(model_path, arch.as_deref(), "attention.head_count_kv").unwrap_or(heads);
    let key_len = extract_u32_with_arch(model_path, arch.as_deref(), "attention.key_length").unwrap_or(head_dim);
    let value_len = extract_u32_with_arch(model_path, arch.as_deref(), "attention.value_length").unwrap_or(head_dim);
    let key_len_swa = extract_u32_with_arch(model_path, arch.as_deref(), "attention.key_length_swa");
    let value_len_swa = extract_u32_with_arch(model_path, arch.as_deref(), "attention.value_length_swa");
    let interval = extract_u32_with_arch(model_path, arch.as_deref(), "full_attention_interval");
    let swa_window = extract_u32_with_arch(model_path, arch.as_deref(), "attention.sliding_window");
    let swa_pattern = extract_i64_array_with_arch(model_path, arch.as_deref(), "attention.sliding_window_pattern");

    let layout = kv_layout_gguf(
        model_path,
        arch.as_deref(),
        num_layers,
        heads_kv_scalar,
        heads,
        key_len,
        value_len,
        key_len_swa,
        value_len_swa,
        interval,
        swa_window,
        swa_pattern.clone(),
        spec.k_bytes,
        spec.v_bytes,
    );

    // Precision tail: те же слои/размерности, но F16-байта (k_bytes=v_bytes=2.0).
    // Если хвоста нет или тело уже F16 — второй layout не нужен.
    let tail_active = spec.tail_tokens > 0
        && (spec.k_bytes != 2.0 || spec.v_bytes != 2.0);
    let layout_f16 = if tail_active {
        Some(kv_layout_gguf(
            model_path,
            arch.as_deref(),
            num_layers,
            heads_kv_scalar,
            heads,
            key_len,
            value_len,
            key_len_swa,
            value_len_swa,
            interval,
            swa_window,
            swa_pattern,
            2.0,
            2.0,
        ))
    } else {
        None
    };

    let n_ctx_seq = pad256(ctx_size);
    let tail_cells_total = if tail_active {
        (spec.tail_tokens as u64).min(n_ctx_seq as u64) as u32
    } else {
        0
    };
    let body_cells = n_ctx_seq - tail_cells_total;

    let swa_cells = layout
        .swa_window
        .map(|w| swa_cells_count(n_ctx_seq, w))
        .unwrap_or(n_ctx_seq);
    let swa_tail_cells = if layout_f16.is_some() {
        (spec.tail_tokens as u64).min(swa_cells as u64) as u32
    } else {
        0
    };
    let swa_body_cells = swa_cells - swa_tail_cells;

    // Dense (non-SWA): тело квантованное + хвост F16.
    let mut kv_dense_bytes = layout.global_bytes_per_token * body_cells as f64;
    if let Some(f16) = &layout_f16 {
        kv_dense_bytes += f16.global_bytes_per_token * tail_cells_total as f64;
    }
    // SWA: ячейки окна, тело/хвост аналогично.
    let mut kv_swa_bytes = layout.swa_bytes_per_cell * swa_body_cells as f64;
    if let Some(f16) = &layout_f16 {
        kv_swa_bytes += f16.swa_bytes_per_cell * swa_tail_cells as f64;
    }

    let kv_per_token_mb = layout.global_bytes_per_token / MIB;
    let kv_swa_mb = kv_swa_bytes / MIB;
    let kv_mb = kv_dense_bytes / MIB + kv_swa_mb;
    // Workspace CUDA-буферов (промежуточные тензоры) ~10%, cap 640 МБ.
    // Прежний cap 256 — ложь (PRP B2): факт CPU-прогона -lv 4 — compute 153 МБ
    // уже при ubatch 512, а на CUDA у 9B-модели ~500-650 МБ (спец); формула
    // (model+kv)*0.10 для 9B даёт ~538 — в этом диапазоне.
    let buffers_mb = ((model_mb + kv_mb) * 0.10).min(640.0);
    VramEstimate {
        model_mb,
        kv_per_token_mb,
        kv_mb,
        kv_swa_mb,
        buffers_mb,
        total_mb: model_mb + kv_mb + buffers_mb,
        num_layers,
        num_attn_layers: layout.attn_layers,
    }
}

/// VRAM при оффлоаде ровно `ngl` слоёв на GPU (остальные — в ОЗУ).
/// Линейная аппроксимация: и веса, и KV-слоты offloaded-слоёв лежат в VRAM,
/// CPU-слои (и их KV) — в ОЗУ.
pub fn vram_for_ngl(
    model_path: &str,
    ngl: u32,
    ctx_size: u32,
    kv_quant_keys: bool,
    kv_quant_values: bool,
) -> f64 {
    let est = estimate_vram(model_path, ctx_size, kv_quant_keys, kv_quant_values);
    vram_for_ngl_estimate(&est, ngl)
}

/// `vram_for_ngl` с явным KV-spec (BeeLlama KVarN и др.).
pub fn vram_for_ngl_with_spec(model_path: &str, ngl: u32, ctx_size: u32, spec: &KvQuantSpec) -> f64 {
    let est = estimate_vram_with_spec(model_path, ctx_size, spec);
    vram_for_ngl_estimate(&est, ngl)
}

/// Численное ядро `vram_for_ngl` (отдельно — для тестов).
///
/// ФИЗИКА llama.cpp (инцидент 19.11.2026, 8 ГБ карта): KV-кэш ЕДИНЫЙ — при
/// ЛЮБОМ ngl>0 он лежит на GPU ЦЕЛИКОМ и пре-аллоцируется под ВЕСЬ
/// `--ctx-size` при создании контекста, ДО любого промпта (лог инцидента:
/// аллокация 8192 МБ на 0.09 с при промпте 852 токена; размер аллокации равен
/// ПОЛНОМУ KV при frac 16/36 — пропорциональная модель `kv*frac` обещала вдвое
/// меньше и пропускала заведомо смертельные запуски до cudaMalloc failed).
/// При ngl==0 весь кэш живёт в ОЗУ — в VRAM остаются только compute-буферы.
pub fn vram_for_ngl_estimate(est: &VramEstimate, ngl: u32) -> f64 {
    if ngl == 0 {
        // Всё на CPU: веса и KV в ОЗУ, в VRAM — только буферы (консервативно).
        est.buffers_mb
    } else {
        let frac = (ngl.min(est.num_layers) as f64) / est.num_layers.max(1) as f64;
        est.model_mb * frac + est.kv_mb + est.buffers_mb
    }
}

/// Максимальное число слоёв, помещающихся в бюджет VRAM.
///
/// `budget_mb` — свободная VRAM (или лимит), `reserve_mb` — запас на буферы/
/// фрагментацию/базу. Возвращает 0, если даже минимальный оффлоад не влезает
/// (тогда остаётся ngl=0). Формула: `floor((бюджет - резерв) / (МБ на слой))`,
/// где МБ на слой = (веса + KV) / block_count.
pub fn max_fitting_ngl(
    model_path: &str,
    ctx_size: u32,
    kv_quant_keys: bool,
    kv_quant_values: bool,
    budget_mb: f64,
    reserve_mb: f64,
) -> u32 {
    let est = estimate_vram(model_path, ctx_size, kv_quant_keys, kv_quant_values);
    max_fitting_ngl_estimate(&est, budget_mb, reserve_mb)
}

/// Численное ядро `max_fitting_ngl` (отдельно — для тестов без файла).
pub fn max_fitting_ngl_estimate(est: &VramEstimate, budget_mb: f64, reserve_mb: f64) -> u32 {
    max_fitting_ngl_safe_estimate(est, budget_mb, reserve_mb, 1.0)
}

/// Максимальное число слоёв с запасом безопасности `safety_factor` (>1).
///
/// Это ТОЧНАЯ ОБРАТНАЯ к [`vram_for_ngl_safe`] при том же критерии
/// «влезает ли модель целиком». Критерий при ngl>0:
/// `(веса·frac + KV_ПОЛНЫЙ + буферы)·safety + reserve ≤ budget` — KV не
/// масштабируется по слоям (единый кэш на GPU при любом ngl>0, см.
/// [`vram_for_ngl_estimate`]), поэтому frac отсчитывается от весов ЗА вычетом
/// полного KV. Возвращает 0 = «оффлоад невозможен, всё на CPU» — это НЕ отказ
/// запуска, а последний рычаг ступенчатого оффлоада (медленнее, но работает).
pub fn max_fitting_ngl_safe_estimate(est: &VramEstimate, budget_mb: f64, reserve_mb: f64, safety_factor: f64) -> u32 {
    if est.num_layers == 0 {
        return 0;
    }
    let safety = safety_factor.max(1.0);
    let budget_for_weights = (budget_mb - reserve_mb) / safety - est.buffers_mb - est.kv_mb;
    if budget_for_weights >= est.model_mb {
        return est.num_layers;
    }
    if budget_for_weights <= 0.0 {
        // Даже один слой + полный KV не проходят критерий → оффлоад невозможен.
        return 0;
    }
    let frac = budget_for_weights / est.model_mb;
    ((frac * est.num_layers as f64).floor() as i64)
        .clamp(0, est.num_layers as i64) as u32
}

/// Полный прогноз VRAM при оффлоаде `ngl` слоёв, с теми же запасами, что и
/// проверка «влезает ли целиком»: `(веса·frac + KV·frac + буферы) × фактор + резерв`.
///
/// Единая формула для входной проверки и для клампа — иначе они расходятся
/// (см. комментарий у [`max_fitting_ngl_safe_estimate`]).
pub fn vram_for_ngl_safe(
    est: &VramEstimate,
    ngl: u32,
    reserve_mb: f64,
    safety_factor: f64,
) -> f64 {
    vram_for_ngl_estimate(est, ngl) * safety_factor.max(1.0) + reserve_mb
}

/// Максимальное число слоёв с запасом безопасности (обёртка с чтением GGUF).
pub fn max_fitting_ngl_safe(
    model_path: &str,
    ctx_size: u32,
    kv_quant_keys: bool,
    kv_quant_values: bool,
    budget_mb: f64,
    reserve_mb: f64,
    safety_factor: f64,
) -> u32 {
    let est = estimate_vram(model_path, ctx_size, kv_quant_keys, kv_quant_values);
    max_fitting_ngl_safe_estimate(&est, budget_mb, reserve_mb, safety_factor)
}

/// `max_fitting_ngl_safe` с явным KV-spec — pre-flight/OOM-ретрай BeeLlama.
pub fn max_fitting_ngl_safe_with_spec(
    model_path: &str,
    ctx_size: u32,
    spec: &KvQuantSpec,
    budget_mb: f64,
    reserve_mb: f64,
    safety_factor: f64,
) -> u32 {
    let est = estimate_vram_with_spec(model_path, ctx_size, spec);
    max_fitting_ngl_safe_estimate(&est, budget_mb, reserve_mb, safety_factor)
}

// ------------------------------------------------------------- RAM-гейт ---

/// Хост-слак RAM-гейта поверх CPU-весов и CPU-KV: кучи llama-server, страницы
/// рантайма, CPU-side compute-буферы. Константа едина для формулы и тестов
/// (SSOT — см. шапку модуля).
pub const RAM_HOST_SLACK_MB: f64 = 512.0;

/// Ожидаемое потребление СИСТЕМНОЙ RAM при оффлоаде `ngl` слоёв на GPU
/// (остальные `num_layers − ngl` — на CPU).
///
/// Зеркало [`vram_for_ngl_estimate`] с обратной долей: веса и KV CPU-слоёв
/// лежат в ОЗУ. Линейная аппроксимация по слоям — та же, что в VRAM-семействе
/// (см. комментарий у `vram_for_ngl`). Буферы `buffers_mb` — VRAM-side
/// (CUDA workspace), в RAM не входят: их CPU-доля покрыта слаком.
pub fn ram_for_ngl_mb(est: &VramEstimate, ngl: u32) -> f64 {
    let layers = est.num_layers.max(1) as f64;
    let cpu_frac = 1.0 - (ngl.min(est.num_layers) as f64) / layers;
    (est.model_mb + est.kv_mb) * cpu_frac + RAM_HOST_SLACK_MB
}

/// [`ram_for_ngl_mb`] с масштабированием KV от `base_ctx` (на который считана
/// `est`) к целевому `ctx`. Линейно по dense-части; SWA-часть при урезании
/// контекста на деле ужимается слабее (окно ограничено `sliding_window`),
/// поэтому линейная модель ПЕРЕоценивает RAM при урезании — гейт errs в
/// безопасную сторону (урежет контекст сильнее, чем необходимо).
pub fn ram_for_ngl_ctx_mb(est: &VramEstimate, ngl: u32, ctx: u32, base_ctx: u32) -> f64 {
    if base_ctx == 0 || ctx >= base_ctx {
        return ram_for_ngl_mb(est, ngl);
    }
    let kv_scaled = est.kv_mb * (ctx as f64 / base_ctx as f64);
    let layers = est.num_layers.max(1) as f64;
    let cpu_frac = 1.0 - (ngl.min(est.num_layers) as f64) / layers;
    (est.model_mb + kv_scaled) * cpu_frac + RAM_HOST_SLACK_MB
}

/// Максимальный контекст, чей RAM-след влезает в `ram_budget_mb` при оффлоаде
/// `ngl` слоёв (`est` считалась на `base_ctx`).
///
/// Аналитическая ОБРАТНАЯ к [`ram_for_ngl_ctx_mb`] (та линейна по `ctx`).
/// Возвращает:
/// - `base_ctx`, если влезает как есть;
/// - 0, если RAM-след не влезает ни при каком контексте (CPU-веса сами по
///   себе больше бюджета) — вызывающий обязан отказаться от запуска;
/// - иначе — наибольший кратный 256 контекст, проходящий критерий
///   `ram_for_ngl_ctx_mb(est, ngl, ctx, base_ctx) ≤ ram_budget_mb`.
///
/// Значение ниже 4096 означает «не влезает и на минимальном контексте» —
/// гейт в `llm.rs` трактует это как отказ.
pub fn max_fitting_ctx_estimate(est: &VramEstimate, ngl: u32, base_ctx: u32, ram_budget_mb: f64) -> u32 {
    if base_ctx == 0 {
        return 0;
    }
    if ram_for_ngl_mb(est, ngl) <= ram_budget_mb {
        return base_ctx;
    }
    let layers = est.num_layers.max(1) as f64;
    let cpu_frac = 1.0 - (ngl.min(est.num_layers) as f64) / layers;
    let headroom = ram_budget_mb - RAM_HOST_SLACK_MB - est.model_mb * cpu_frac;
    if headroom <= 0.0 {
        return 0;
    }
    let kv_cpu_full = est.kv_mb * cpu_frac;
    if kv_cpu_full <= 0.0 {
        // KV не потребляет, а след на base_ctx уже превысил бюджет —
        // масштабирование контекста не поможет.
        return 0;
    }
    // Здесь kv_cpu_full > headroom (иначе ram(base_ctx) прошёл бы критерий
    // выше), поэтому frac < 1.0 гарантированно.
    let frac = headroom / kv_cpu_full;
    let ctx = (base_ctx as f64 * frac / 256.0).floor() * 256.0;
    ctx.max(0.0) as u32
}

// ------------------------------------------------- VRAM-гейт контекста ---

/// VRAM-след ПОЛНОГО оффлоада (ngl = num_layers) при контексте `ctx`, где
/// `est` считалась на `base_ctx` (`ctx ≤ base_ctx` — гейт только режет).
///
/// Разложение: веса (константа) + dense-часть KV (масштабируется линейно по
/// pad256-ячейкам — точная модель SSOT для dense-only моделей) + SWA-часть
/// (удерживается на уровне base_ctx: при урезании контекста ниже окна SWA-
/// ячейки не растут, поэтому удержание — консервативная переоценка) +
/// compute-буферы (константа). Резерв НЕ входит — его добавляет критерий в
/// [`max_fitting_ctx_vram_estimate`]. Для dense-only моделей (без SWA) —
/// точная линейная инверсия; для SWA/tail-моделей — консервативная.
pub fn vram_for_ctx_mb(est: &VramEstimate, ctx: u32, base_ctx: u32) -> f64 {
    let swa = est.kv_swa_mb.min(est.kv_mb); // защита от кривых входов
    let dense_full = est.kv_mb - swa;
    if base_ctx == 0 || ctx >= base_ctx || dense_full <= 0.0 {
        // Кроить нечем (SWA-only / вырожденные входы): честный след на base.
        return est.model_mb + est.kv_mb + est.buffers_mb;
    }
    let dense_ctx = dense_full * (pad256(ctx) as f64) / (pad256(base_ctx) as f64);
    est.model_mb + dense_ctx + swa + est.buffers_mb
}

/// Максимальный контекст, чей VRAM-след ПОЛНОГО оффлоада влезает в
/// `budget_mb` при резерве `reserve_mb` — VRAM-собрат ОЗУ-версии
/// [`max_fitting_ctx_estimate`]. Аналитическая ОБРАТНАЯ к критерию
/// `vram_for_ctx_mb(est, ctx, base_ctx) + reserve_mb ≤ budget_mb`.
///
/// Возвращает:
/// - `base_ctx`, если влезает как есть (гейт не срабатывает);
/// - 0, если не влезает ни при каком контексте (веса + SWA + буферы + резерв
///   сами по себе больше бюджета) — контекстом карту не спасти, вызывающий
///   обязан уйти к урезанию слоёв (ngl);
/// - иначе — наибольший кратный 256 контекст, проходящий критерий.
///
/// Инварианты (закреплены property-тестами P1/P2/P4): результат ≤ base_ctx;
/// результат > 0 проходит критерий; шаг +256 (при кратном pad256) — уже нет;
/// результат 0 — сентинел «даже минимальный реальный контекст (256) критерий
/// не проходит» (включая случай cells < 256, когда ctx=0 критерий формально
/// проходит, но реального контекста нет — не отказ, а честный «нечего запускать»).
pub fn max_fitting_ctx_vram_estimate(est: &VramEstimate, base_ctx: u32, budget_mb: f64, reserve_mb: f64) -> u32 {
    if base_ctx == 0 {
        return 0;
    }
    if vram_for_ctx_mb(est, base_ctx, base_ctx) + reserve_mb <= budget_mb {
        return base_ctx;
    }
    let swa = est.kv_swa_mb.min(est.kv_mb);
    let dense_full = est.kv_mb - swa;
    let headroom = budget_mb - reserve_mb - est.model_mb - swa - est.buffers_mb;
    if dense_full <= 0.0 || headroom <= 0.0 {
        // Веса+SWA+буферы+резерв сами по себе не влезают, либо KV целиком
        // SWA-часть (урезанием ctx не масштабируется) — контекстом не помочь.
        return 0;
    }
    // Критерий: model + dense·pad256(ctx)/pad256(base) + swa + buffers + reserve ≤ budget
    // ⟺ pad256(ctx) ≤ pad256(base)·headroom/dense_full. Округление ВНИЗ до
    // кратного 256 даёт одновременно выполнение критерия и максимальность
    // (шаг +256 кратен 256 → pad256 не меняет → критерий ломается).
    let cells = (pad256(base_ctx) as f64) * headroom / dense_full;
    let ctx = (cells / 256.0).floor() * 256.0;
    (ctx.max(0.0) as u32).min(base_ctx)
}

// ───────────────────── Тир-слот и hardware-потолок (PRP C) ─────────────────────

/// Нижний тир слота (PRP C, инвариант спеца): `slot = clamp(next_tier(need), 16384, hardware_max)`.
pub const TIERS_MIN: u32 = 16384;

/// Наименьший тир (16384 × 2^n) ≥ `need`, зажатый сверху `window`.
///
/// SSOT для плагина (mid-turn step-up, PRP C6) и хоста (choose_slot кэша
/// движка, PRP C4) — тир один, расхождений не бывает. Окно меньше нижнего
/// тира → само окно (кап сильнее пола); need > окна → окно (step-up за окно
/// бессмыслен — слот не может превышать окно модели).
pub fn next_tier(need: u32, window: u32) -> u32 {
    if window == 0 {
        return 0;
    }
    let mut tier = TIERS_MIN;
    while tier < need && tier < window {
        tier = tier.saturating_mul(2);
    }
    tier.min(window)
}

/// Hardware-потолок слота (PRP C1): максимальный контекст, который ЭТО железо
/// способно держать для модели, — min(VRAM-инверсия при полном оффлоаде,
/// RAM-инверсия при финальном ngl), обе на est окна GGUF.
///
/// Хост привязывает бюджет компакции и step-up кэша движка к потолку железа,
/// а не к активному слоту: переполнение слота лечится step-up (respawn на тир
/// выше), усечение истории — только у потолка железа, где step-up невозможен
/// (§00.2: отказ «данных нет» делается невозможным, а не реже).
///
/// Сентинелы инверсий:
/// - VRAM-инверсия вернула 0 («веса + SWA + буферы + резерв не влезают при
///   любом контексте») → полный оффлоад невозможен, ngl уйдёт в резку/CPU и
///   VRAM перестаёт биндить контекст — сторона не ограничивает (base_ctx).
///   Тот же смысл у `vram_budget_mb <= 0` (GPU нет / память не читается).
/// - RAM-инверсия вернула 0 (CPU-веса сами по себе больше ОЗУ-бюджета) —
///   запуск обречён на RAM-гейте; честный 0 в потолке (не маскируем).
pub fn compute_hardware_ctx_ceiling(
    est: &VramEstimate,
    base_ctx: u32,
    ngl: u32,
    vram_budget_mb: f64,
    reserve_mb: f64,
    ram_budget_mb: f64,
) -> u32 {
    let vram_ceiling = if vram_budget_mb <= 0.0 {
        base_ctx
    } else {
        match max_fitting_ctx_vram_estimate(est, base_ctx, vram_budget_mb, reserve_mb) {
            0 => base_ctx,
            c => c,
        }
    };
    let ram_ceiling = max_fitting_ctx_estimate(est, ngl, base_ctx, ram_budget_mb);
    vram_ceiling.min(ram_ceiling)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn est(model_mb: f64, kv_mb: f64, layers: u32) -> VramEstimate {
        VramEstimate {
            model_mb,
            kv_per_token_mb: kv_mb / 2048.0,
            kv_mb,
            kv_swa_mb: 0.0,
            buffers_mb: ((model_mb + kv_mb) * 0.10).min(640.0),
            total_mb: model_mb + kv_mb,
            num_layers: layers,
            num_attn_layers: layers,
        }
    }

    #[test]
    fn next_tier_is_monotone_capped_and_exact() {
        // Точное попадание: need внутри тира → верх тира.
        assert_eq!(next_tier(1, 262_144), 16_384);
        assert_eq!(next_tier(16_384, 262_144), 16_384);
        assert_eq!(next_tier(16_385, 262_144), 32_768);
        assert_eq!(next_tier(100_000, 262_144), 131_072);
        // need > окна → окно (step-up за окно бессмыслен).
        assert_eq!(next_tier(1_000_000, 262_144), 262_144);
        // Окно меньше нижнего тира → само окно (кап сильнее пола).
        assert_eq!(next_tier(1, 8_192), 8_192);
        assert_eq!(next_tier(0, 0), 0);
        // Монотонность по need + всегда тир (грид).
        let mut prev = 0;
        let mut need = 0u32;
        while need <= 300_000 {
            let t = next_tier(need, 262_144);
            assert!(t >= prev, "не монотонно на need {}", need);
            assert!(t % 16_384 == 0, "вне тиров: {}", t);
            prev = t;
            need += 4_097;
        }
    }

    #[test]
    fn hardware_ceiling_is_min_of_vram_and_ram_sides() {
        // est: веса 5000, KV 1000 (dense-only), 10 слоёв, буферы min(600,640)=600.
        let e = est(5_000.0, 1_000.0, 10);
        // (1) Всё влезает → потолок = окно.
        assert_eq!(compute_hardware_ctx_ceiling(&e, 32_768, 10, 8_000.0, 512.0, 4_000.0), 32_768);
        // (2) VRAM-сторона биндит: headroom = 7000−512−5000−0−600 = 888;
        // cells = 32768·888/1000 = 29094.4 → ctx 28928 (критерий 5000+883.1+600+512
        // = 6995.1 ≤ 7000; +256 → 7002.75 > 7000). RAM при полном оффлоаде не биндит.
        assert_eq!(compute_hardware_ctx_ceiling(&e, 32_768, 10, 7_000.0, 512.0, 4_000.0), 28_928);
        // (3) RAM-сторона биндит (ngl=0, всё в ОЗУ): headroom = 6000−512−5000 = 488;
        // frac 0.488 → ctx = 15872 (5000+484.4+512 = 5996.4 ≤ 6000; +256 → 6004.2 > 6000).
        // VRAM-бюджет 0 (GPU нет) → VRAM-сторона не ограничивает.
        assert_eq!(compute_hardware_ctx_ceiling(&e, 32_768, 0, 0.0, 512.0, 6_000.0), 15_872);
        // (4) 0-сентинел VRAM (веса+SWA+буферы+резерв не влезают ни при каком
        // ctx) → сторона не ограничивает; RAM при полном оффлоаде — только слак.
        assert_eq!(compute_hardware_ctx_ceiling(&e, 32_768, 10, 4_000.0, 512.0, 4_000.0), 32_768);
        // (5) min обеих сторон: VRAM даёт 28928, RAM 15872 → 15872.
        assert_eq!(compute_hardware_ctx_ceiling(&e, 32_768, 0, 7_000.0, 512.0, 6_000.0), 15_872);
    }

    #[test]
    fn vram_for_ngl_full_kv_on_gpu_at_any_offload() {
        // 20 слоёв, веса 10000 МБ, KV 2000 МБ, буферы = min(0.1*12000, 640)=640.
        let e = est(10_000.0, 2_000.0, 20);
        // Полный оффлоад: веса+KV+буферы.
        let full = vram_for_ngl_estimate(&e, 20);
        assert!((full - 12_640.0).abs() < 0.01, "full: {}", full);
        // Половина слоёв: веса/2 + ПОЛНЫЙ KV + буферы (единый KV на GPU при
        // любом ngl>0 — физика llama.cpp, инцидент 19.11.2026).
        let half = vram_for_ngl_estimate(&e, 10);
        assert!((half - (5_000.0 + 2_000.0 + 640.0)).abs() < 0.01, "half: {}", half);
        // Один слой: веса/20 + полный KV + буферы.
        let one = vram_for_ngl_estimate(&e, 1);
        assert!((one - (500.0 + 2_000.0 + 640.0)).abs() < 0.01, "one: {}", one);
        // Больше слоёв, чем есть — кап на num_layers (полный оффлоад).
        assert_eq!(vram_for_ngl_estimate(&e, 999), full);
        // Ноль слоёв: KV и веса в ОЗУ — в VRAM только буферы.
        assert!((vram_for_ngl_estimate(&e, 0) - 640.0).abs() < 0.01, "zero: {}", vram_for_ngl_estimate(&e, 0));
    }

    #[test]
    fn max_fitting_ngl_computes_floor_of_budget() {
        // Модель 10000+2000 МБ, 20 слоёв, буферы = min(12000·0.1, 640) = 640.
        let e = est(10_000.0, 2_000.0, 20);
        // Без фактора: KV на GPU ПОЛНЫЙ (ngl>0) → бюджета на веса остаётся
        // 10 500 − 500 − 640 − 2000 = 7360 → frac 0.736 → floor(14.72) = 14.
        assert_eq!(max_fitting_ngl_estimate(&e, 10_500.0, 500.0), 14);
        // С фактором 1.25: (веса·frac + KV_ПОЛНЫЙ + буферы)·1.25 + резерв ≤ бюджет
        // → веса·frac ≤ (10 500 − 500)/1.25 − 2000 − 640 = 5360 → floor(10.72) = 10.
        assert_eq!(max_fitting_ngl_safe_estimate(&e, 10_500.0, 500.0, 1.25), 10);
        // Фактор никогда не расширяет бюджет.
        assert!(max_fitting_ngl_safe_estimate(&e, 10_500.0, 500.0, 1.25) <= max_fitting_ngl_estimate(&e, 10_500.0, 500.0));
        // Резерв больше бюджета → 0 (не влезает даже минимальный оффлоад).
        assert_eq!(max_fitting_ngl_estimate(&e, 400.0, 500.0), 0);
        // Бюджет больше полного оффлоада → кап на число слоёв.
        assert_eq!(max_fitting_ngl_estimate(&e, 99_000.0, 0.0), 20);
        // Нулевое число слоёв → 0.
        assert_eq!(max_fitting_ngl_estimate(&est(100.0, 100.0, 0), 1000.0, 0.0), 0);
    }

    #[test]
    fn ram_for_ngl_mb_mirrors_vram_with_inverse_fraction() {
        // Веса 10000, KV 2000, 20 слоёв. CPU-доля = (10000+2000)·(1−ngl/20)+512.
        let e = est(10_000.0, 2_000.0, 20);
        // Полный оффлоад: только хост-слак.
        assert!((ram_for_ngl_mb(&e, 20) - 512.0).abs() < 0.01, "full: {}", ram_for_ngl_mb(&e, 20));
        // Ноль слоёв: всё на CPU.
        assert!((ram_for_ngl_mb(&e, 0) - 12_512.0).abs() < 0.01, "zero: {}", ram_for_ngl_mb(&e, 0));
        // Половина: 6000 + 512.
        assert!((ram_for_ngl_mb(&e, 10) - 6_512.0).abs() < 0.01, "half: {}", ram_for_ngl_mb(&e, 10));
        // Больше слоёв, чем есть — кап (полный оффлоад).
        assert!((ram_for_ngl_mb(&e, 999) - 512.0).abs() < 0.01);
    }

    #[test]
    fn ram_for_ngl_ctx_mb_scales_kv_linearly() {
        let e = est(10_000.0, 2_000.0, 20); // est считалась на base_ctx
        // ngl=10, ctx = base/2: CPU = 5000 (веса) + 500 (KV) + 512.
        assert!((ram_for_ngl_ctx_mb(&e, 10, 4096, 8192) - 6_012.0).abs() < 0.01);
        // ctx ≥ base_ctx → без масштабирования (тот же след, что ram_for_ngl_mb).
        assert!((ram_for_ngl_ctx_mb(&e, 10, 8192, 8192) - ram_for_ngl_mb(&e, 10)).abs() < 0.01);
        assert!((ram_for_ngl_ctx_mb(&e, 10, 16384, 8192) - ram_for_ngl_mb(&e, 10)).abs() < 0.01);
        // base_ctx = 0 — вырожденный случай, без деления на ноль.
        assert!((ram_for_ngl_ctx_mb(&e, 10, 4096, 0) - ram_for_ngl_mb(&e, 10)).abs() < 0.01);
    }

    #[test]
    fn max_fitting_ctx_inverts_ram_formula() {
        let e = est(10_000.0, 2_000.0, 20); // KV 2000 на base_ctx, 20 слоёв
        // Полный оффлоад: RAM не зависит от контекста → base_ctx целиком.
        assert_eq!(max_fitting_ctx_estimate(&e, 20, 28_672, 600.0), 28_672);
        // ngl=10, base 8192: CPU-веса 5000, KV-CPU 1000 на 8192. Бюджет 6012:
        // 5000 + 1000·r + 512 ≤ 6012 → r ≤ 0.5 → 4096.
        assert_eq!(max_fitting_ctx_estimate(&e, 10, 8_192, 6_012.0), 4_096);
        // Бюджет на волоске выше → base_ctx целиком.
        assert_eq!(max_fitting_ctx_estimate(&e, 10, 8_192, 13_013.0), 8_192);
        // CPU-веса сами по себе не влезают (5000 + 512 > 5500) → 0 = отказ.
        assert_eq!(max_fitting_ctx_estimate(&e, 10, 8_192, 5_500.0), 0);
        // base_ctx = 0 → 0.
        assert_eq!(max_fitting_ctx_estimate(&e, 10, 0, 99_000.0), 0);
    }

    #[test]
    fn max_fitting_ctx_result_passes_criterion_next_step_fails() {
        // Свойство-тест, связывающий обратную с прямой формулой: результат
        // проходит критерий RAM, а шаг вверх на 256 — уже нет (если < base).
        let e = est(4_000.0, 3_000.0, 30);
        let base = 16_384u32;
        let budget = 5_000.0;
        let ctx = max_fitting_ctx_estimate(&e, 0, base, budget);
        assert!(ctx > 0 && ctx < base, "ctx: {}", ctx);
        assert!(ram_for_ngl_ctx_mb(&e, 0, ctx, base) <= budget,
            "ctx {} must fit: {}", ctx, ram_for_ngl_ctx_mb(&e, 0, ctx, base));
        assert!(ram_for_ngl_ctx_mb(&e, 0, (ctx + 256).min(base), base) > budget,
            "ctx+256 must not fit");
        assert_eq!(ctx % 256, 0, "кратность PAD256");
    }

    #[test]
    fn kv_bytes_quant_ratio() {
        // q8_0 (34/32) против FP16 (2.0) — соотношение ровно (34/32)/2.
        let mk = |qk: bool, qv: bool| {
            let b_k = kv_bytes_per_elem(qk);
            let b_v = kv_bytes_per_elem(qv);
            16.0 * (128.0 * b_k + 128.0 * b_v)
        };
        assert!((mk(true, true) / mk(false, false) - (34.0 / 32.0) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn cache_type_bytes_parses_kvarn_and_standards() {
        // bpv/8: kvarn5 = 5.375/8, kvarn4 = 4.375/8 (README Anbeeld).
        assert!((cache_type_bytes("kvarn5").unwrap() - 5.375 / 8.0).abs() < 1e-12);
        assert!((cache_type_bytes("kvarn4").unwrap() - 4.375 / 8.0).abs() < 1e-12);
        assert!((cache_type_bytes("q8_0").unwrap() - 8.5 / 8.0).abs() < 1e-12);
        assert_eq!(cache_type_bytes("f16").unwrap(), 2.0);
        assert_eq!(cache_type_bytes("bf16").unwrap(), 2.0);
        assert!(cache_type_bytes("unknown_type").is_none());
    }

    #[test]
    fn kv_spec_from_source_prefers_beellama_kvarn_over_flags() {
        use crate::engine::sources::source_spec;
        let bee = source_spec("beellama").expect("beellama in registry");
        // Даже legacy-флаги true → KVarN из runtime перекрывает (приоритет как в build_cmd).
        let spec = kv_spec_from_source(Some(&bee), true, true);
        assert!((spec.k_bytes - 5.375 / 8.0).abs() < 1e-12, "k: {}", spec.k_bytes);
        assert!((spec.v_bytes - 4.375 / 8.0).abs() < 1e-12, "v: {}", spec.v_bytes);
        assert_eq!(spec.tail_tokens, 1024);

        let gg = source_spec("ggml-org").expect("ggml-org");
        // Официальный llama.cpp: runtime без cache-type → legacy flags.
        let spec_gg = kv_spec_from_source(Some(&gg), false, false);
        assert_eq!(spec_gg, KvQuantSpec::fp16());
        let spec_gg_q = kv_spec_from_source(Some(&gg), true, true);
        assert_eq!(spec_gg_q.k_bytes, 34.0 / 32.0);

        // None → legacy.
        assert_eq!(kv_spec_from_source(None, false, false), KvQuantSpec::fp16());
    }

    #[test]
    fn estimate_kvarn_kv_smaller_than_fp16() {
        // KV-часть kvarn5/kvarn4 (без tail) ≈ (5.375+4.375)/(16+16) ≈ 30.5% от F16.
        // С tail 1024 при большом контексте — чуть больше (хвост F16), но ≪ F16.
        let path = "nonexistent.gguf"; // metadata → 0, GGUF-ключи → дефолты (32 слоя и т.д.)
        let fp16 = estimate_vram_with_spec(path, 8192, &KvQuantSpec::fp16());
        let kvarn = estimate_vram_with_spec(
            path,
            8192,
            &KvQuantSpec { k_bytes: 5.375 / 8.0, v_bytes: 4.375 / 8.0, tail_tokens: 0 },
        );
        assert!(kvarn.kv_mb > 0.0 && fp16.kv_mb > 0.0);
        let ratio = kvarn.kv_mb / fp16.kv_mb;
        let expected = (5.375 + 4.375) / (16.0 + 16.0);
        assert!(
            (ratio - expected).abs() < 0.02,
            "kvarn/fp16 kv ratio {} vs expected {}",
            ratio,
            expected
        );

        // С precision tail 1024: тело kvarn, хвост F16 → между чистым kvarn и F16.
        let with_tail = estimate_vram_with_spec(
            path,
            8192,
            &KvQuantSpec { k_bytes: 5.375 / 8.0, v_bytes: 4.375 / 8.0, tail_tokens: 1024 },
        );
        assert!(
            with_tail.kv_mb > kvarn.kv_mb && with_tail.kv_mb < fp16.kv_mb,
            "tail kv {} not in (kvarn {}, fp16 {})",
            with_tail.kv_mb, kvarn.kv_mb, fp16.kv_mb
        );
    }

    #[test]
    fn max_fitting_ngl_kvarn_allows_more_layers_than_fp16() {
        // Один budget: меньший KV (kvarn) → больше слоёв на GPU. Это и есть цель BeeLlama.
        let path = "nonexistent.gguf";
        let budget = 6_000.0;
        let reserve = 1_024.0;
        let safety = 1.25;
        let ngl_fp16 = max_fitting_ngl_safe_with_spec(path, 8192, &KvQuantSpec::fp16(), budget, reserve, safety);
        let ngl_kvarn = max_fitting_ngl_safe_with_spec(
            path,
            8192,
            &KvQuantSpec { k_bytes: 5.375 / 8.0, v_bytes: 4.375 / 8.0, tail_tokens: 1024 },
            budget,
            reserve,
            safety,
        );
        // F16-KV на дефолтном layout (32 слоя, 8K ctx) съедает бюджет почти весь
        // (см. ratio-тест) — kvarn обязан дать строго больше слоёв либо оба капа
        // на num_layers (тогда равны — допускаем только строгий ≥).
        assert!(
            ngl_kvarn >= ngl_fp16,
            "kvarn ngl {} < fp16 ngl {}",
            ngl_kvarn,
            ngl_fp16
        );
        if ngl_fp16 < 32 {
            assert!(
                ngl_kvarn > ngl_fp16,
                "kvarn must fit strictly more layers when fp16 is truncated: {} vs {}",
                ngl_kvarn,
                ngl_fp16
            );
        }
    }

    #[test]
    fn estimate_vram_bool_wrapper_matches_spec_api() {
        let path = "nonexistent.gguf";
        let a = estimate_vram(path, 4096, false, false);
        let b = estimate_vram_with_spec(path, 4096, &KvQuantSpec::fp16());
        assert_eq!(a.kv_mb.to_bits(), b.kv_mb.to_bits());
        assert_eq!(a.total_mb.to_bits(), b.total_mb.to_bits());

        let c = estimate_vram(path, 4096, true, true);
        let d = estimate_vram_with_spec(path, 4096, &KvQuantSpec::from_legacy_flags(true, true));
        assert_eq!(c.kv_mb.to_bits(), d.kv_mb.to_bits());
    }

    #[test]
    fn swa_cells_uses_windows_not_full_ctx() {
        // Один слот (SERVER_N_PARALLEL=1), ubatch=512.
        // window 1024: cells = min(ctx, 1024·1+512=1536), PAD 256.
        assert_eq!(swa_cells_count(pad256(18_432), 1024), 1_536);
        // Меньше окна — полный ctx (PAD 256).
        assert_eq!(swa_cells_count(pad256(1_024), 1024), 1_024);
        // При одном слоте окно 1024 + ubatch уже перекрывает ctx 2048, поэтому
        // SWA-кап срабатывает именно на ctx, а не на окне × слоты.
        assert_eq!(swa_cells_count(pad256(1_800), 1024), 1_536);
    }

    /// Регрессия: константа слотов в расчёте SWA и флаг `--parallel` в команде
    /// запуска обязаны быть ОДНИМ числом. Расхождение означало, что оценка VRAM
    /// считает вчетверо меньше KV, чем реально резервирует движок, и pre-flight
    /// без причины резал слои модели на CPU.
    #[test]
    fn swa_cells_follow_the_configured_slot_count() {
        assert_eq!(SERVER_N_PARALLEL, 1);
        // Окно 4096 при одном слоте: 4096·1 + 512 = 4608, без четвертного
        // деления, которое давало бы 16 896.
        assert_eq!(swa_cells_count(pad256(18_432), 4_096), 4_608);
    }

    /// Регрессия на сценарий юзера (GTX 1070 Ti, 8192 МБ VRAM, 9B Q4).
    ///
    /// Главное свойство: выбранный pre-flight `-ngl` обязан проходить тот же
    /// критерий, по которому решается «влезает ли целиком». Раньше входная
    /// проверка и кламп считали по РАЗНЫМ формулам, и кламп возвращал 30 слоёв,
    /// для которых прогноз памяти всё равно превышал бюджет — то есть pre-flight
    /// недозащищал ровно от CUDA OOM/TDR, ради которых запас и добавлен.
    #[test]
    fn pre_flight_ngl_satisfies_the_same_budget_check() {
        let reserve = 512.0;
        let safety = 1.25;
        let budget = 7_376.0;
        let e = est(5_167.0, 212.0, 32); // Ornith-1.5-9B IQ4_NL, ctx 19968

        // Полный оффлоад действительно не влезает — иначе тест не проверяет путь.
        assert!(
            vram_for_ngl_safe(&e, e.num_layers, reserve, safety) > budget,
            "модель должна требовать больше бюджета — премушаны теста"
        );

        let ngl = max_fitting_ngl_safe_estimate(&e, budget, reserve, safety);
        assert!(ngl > 0 && ngl < e.num_layers, "ngl={} вне ожидаемого диапазона", ngl);

        // Выбранный оффлоад влезает…
        assert!(
            vram_for_ngl_safe(&e, ngl, reserve, safety) <= budget,
            "ngl={} даёт {:.0} МБ при бюджете {:.0} МБ — не влезает",
            ngl,
            vram_for_ngl_safe(&e, ngl, reserve, safety),
            budget
        );
        // …и является максимальным: ещё один слой уже не влезает.
        assert!(
            vram_for_ngl_safe(&e, ngl + 1, reserve, safety) > budget,
            "ngl={} не максимален: ngl+1 даёт {:.0} МБ и влезает в {:.0} МБ",
            ngl,
            vram_for_ngl_safe(&e, ngl + 1, reserve, safety),
            budget
        );
    }

    /// Регрессия на числа юзера (GTX 1070 Ti 8 ГБ, лог 30.10.26) + честные
    /// константы B2 (буферы = min(10%·(model+kv), 640) — спец: compute у 9B
    /// реально ~500-650 МБ; CPU-прогон -lv 4 дал нижнюю границу 153 МБ).
    ///
    /// Факт лога 30.10.26: свободно 7700 из 8192, использовано 4899 МБ — полный
    /// оффлоад влезал. Решение резки смотрит на ФАКТ (×1.0): при честных буферах
    /// фактическая потребность (≈6429) влезает → резки нет. Padded-оценка (×1.25)
    /// честно показывает ≈7908 > 7700, но её инверсия не уходит глубже 30/32
    /// (исторический баг «26-28 слоёв → 5 t/s» из-за ложных буферов 256 не вернулся).
    #[test]
    fn pre_flight_full_offload_fits_users_8gb_card() {
        let reserve = 512.0; // как в llm.rs pre-flight после 31.10.26
        let safety = 1.25;
        let free = 7_700.0; // «свободно 7700 из 8192» — факт юзера

        // Гибридные метаданные KV (реальный боевой кейс BeLlama/KVarN): ~212 МБ.
        let e_hybrid = est(5_167.0, 212.0, 32);
        // Фактическая потребность (по ней решает pre-flight): влезает → резки нет.
        let need_factual = vram_for_ngl_safe(&e_hybrid, e_hybrid.num_layers, reserve, 1.0);
        assert!(
            need_factual <= free,
            "факт ({:.0} МБ) обязан влезать в {:.0} — полный оффлоад без резки",
            need_factual,
            free
        );
        // Padded-оценка на честных буферах честно превышает свободные…
        let need_padded = vram_for_ngl_safe(&e_hybrid, e_hybrid.num_layers, reserve, safety);
        assert!(
            need_padded > free,
            "padded {:.0} обязан превышать {:.0} — иначе тест не различает факт/×1.25",
            need_padded,
            free
        );
        // …но её инверсия не уходит глубже 30/32 (против старых 26-28).
        let ngl_padded = max_fitting_ngl_safe_estimate(&e_hybrid, free, reserve, safety);
        assert!(
            ngl_padded >= 30,
            "padded-инверсия ушла глубже 30/32 (старый баг 26-28), got {}",
            ngl_padded
        );

        // Консервативная f16-оценка KV без метаданных (как в логе юзера: KV 640).
        let e_plain = est(5_167.0, 640.0, 32);
        let need_factual_plain = vram_for_ngl_safe(&e_plain, e_plain.num_layers, reserve, 1.0);
        assert!(
            need_factual_plain <= free,
            "консервативный факт ({:.0}) тоже влезает — резки нет",
            need_factual_plain
        );
        let ngl_plain = max_fitting_ngl_safe_estimate(&e_plain, free, reserve, safety);
        assert!(
            ngl_plain >= 28,
            "консервативная padded-инверсия ушла глубже 28/32, got {}",
            ngl_plain
        );
    }

    /// Регрессия на числа юзера из лога 01.11.26 (лог юзера, G-A).
    ///
    /// Факт лога: файл 5167 МБ, KV 768 МБ (f16-оценка, ctx 24422), свободно
    /// 7697 из 8192, 32 слоя. Прежнее решение резки с фактором:
    /// `(5167+768+256)×1.25+512 = 8251 > 7697` → урезание до 29/32 слоёв →
    /// 5 t/s → два сгоревших факт-экстрактора («14 минут юзера»), хотя
    /// реальный пик VRAM по тому же логу — 5822 МБ. Решение резки обязано
    /// считаться по факту (файл+KV+буферы+резерв, §00.2): 7041 ≤ 7697 →
    /// полный оффлоад без урезания. Буферы честные (B2): 593.5 МБ, поэтому
    /// потребность ~7041 (при ложных 256 была 6703).
    #[test]
    fn pre_flight_cut_decision_is_factual_users_nov1_log() {
        let reserve = 512.0;
        let safety = 1.25;
        let free = 7_697.0; // «свободно 7697 из 8192» — факт лога 01.11.26
        let e = est(5_167.0, 768.0, 32); // Ornith IQ4_NL, f16-KV, ctx 24422

        // Фактическая потребность влезает — резать нельзя.
        let need_factual = vram_for_ngl_safe(&e, e.num_layers, reserve, 1.0);
        assert!(
            (need_factual - 7_040.5).abs() < 1.0,
            "фактическая формула: ожидалось ~7041 МБ (5167+768+593.5+512), got {:.0}",
            need_factual
        );
        assert!(
            need_factual <= free,
            "факт ({:.0} МБ) влезает в свободные {:.0} МБ — урезание запрещено",
            need_factual,
            free
        );

        // Оценка с фактором на тех же числах НЕ влезает — именно она дала
        // «нужно ~8251 МБ» в логе юзера. Документируем: фактор в резку не входит.
        let need_padded = vram_for_ngl_safe(&e, e.num_layers, reserve, safety);
        assert!(
            need_padded > free,
            "оценка ×1.25 ({:.0} МБ) должна превышать свободные {:.0} МБ — иначе тест не воспроизводит лог",
            need_padded,
            free
        );

        // Старое решение резки дало в логе ровно 29 слоёв («урезан с 999 до 29»);
        // на честных буферах B2 ×1.25 даёт 27: bfw = 5748 − 593.5 − 768 = 4386.5
        // → frac 0.849 → floor(27.17) = 27. Фактическая формула — полный оффлоад.
        let ngl_old = max_fitting_ngl_safe_estimate(&e, free, reserve, safety);
        assert_eq!(ngl_old, 27, "×1.25-инверсия на честных буферах B2");
        let ngl_factual = max_fitting_ngl_safe_estimate(&e, free, reserve, 1.0);
        assert_eq!(
            ngl_factual, e.num_layers,
            "по факту полный оффлоад обязан влезать, got {}",
            ngl_factual
        );

        // Инверсия сохраняется и на фактическом пути: если факта не хватает,
        // кламп по 1.0 влезает в бюджет и максимален (узкий бюджет 6000 МБ).
        let tight = 6_000.0;
        assert!(vram_for_ngl_safe(&e, e.num_layers, reserve, 1.0) > tight);
        let ngl_tight = max_fitting_ngl_safe_estimate(&e, tight, reserve, 1.0);
        assert!(
            vram_for_ngl_safe(&e, ngl_tight, reserve, 1.0) <= tight
                && vram_for_ngl_safe(&e, ngl_tight + 1, reserve, 1.0) > tight,
            "ngl={} не проходит фактический критерий влезания/максимальности",
            ngl_tight
        );
    }


    /// Инвариант формы: кламп — обратная функция к прогнозу, а не отдельная
    /// эвристика. Проверяется на сетке параметров, где результат известен.
    #[test]
    fn max_fitting_ngl_is_inverse_of_vram_for_ngl_safe() {
        let e = est(5_167.0, 212.0, 32);
        let reserve = 1_024.0;
        let safety = 1.25;
        for budget in [2_000.0, 4_000.0, 6_000.0, 7_376.0, 9_000.0, 40_000.0] {
            let ngl = max_fitting_ngl_safe_estimate(&e, budget, reserve, safety);
            assert!(
                vram_for_ngl_safe(&e, ngl, reserve, safety) <= budget,
                "бюджет {:.0}: ngl={} выходит за бюджет",
                budget,
                ngl
            );
            if ngl < e.num_layers {
                assert!(
                    vram_for_ngl_safe(&e, ngl + 1, reserve, safety) > budget,
                    "бюджет {:.0}: ngl={} не максимален",
                    budget,
                    ngl
                );
            }
        }
    }

    // ------------------------------------------------ VRAM-гейт контекста ---

    /// U2, инцидент 19.11.2026 (GTX 1070 Ti, 8 ГБ): файл 5167 МБ, окно GGUF
    /// 262144 (KV 8192 МБ), свободно 7614 МБ. Старая пропорциональная формула
    /// на этом кейсе давала ложное «влезает» (vram(ngl=16) = 5167·0.5 +
    /// 8192·0.5 + 256 = 6935.5 + 512 ≤ 7614 — а реальный след при ngl=16:
    /// 5167·0.5 + 8192 + 256 = 10991.5 > 7614) и посылала llama-server с
    /// ctx 262144 → cudaMalloc failed. VRAM-гейт обязан вернуть максимальный
    /// кратный 256 контекст, проходящий критерий.
    #[test]
    fn vram_ctx_gate_survives_users_8gb_incident() {
        let e = est(5_167.0, 8_192.0, 32); // kv_swa_mb = 0 (dense-only), буферы 640 (B2)
        let base = 262_144u32;
        let free = 7_614.0;
        let reserve = 1024.0; // B2: Windows WDDM-запас (vram_reserve_mb)

        // Полный слот не влезает — иначе тест не проверяет гейт.
        assert!(
            vram_for_ctx_mb(&e, base, base) + reserve > free,
            "полный KV 8192 + файл + буферы обязан превышать свободные {:.0}",
            free
        );

        let ctx = max_fitting_ctx_vram_estimate(&e, base, free, reserve);
        // B2-числа: headroom = 7614−1024−5167−640 = 783 → cells 25056 → floor 97×256.
        assert_eq!(ctx, 24_832, "инцидентные числа (честные константы): максимальный кратный 256 ctx");
        assert_eq!(ctx % 256, 0);
        // Критерий выполняется; шаг +256 (pad256 не меняет) — уже нет.
        assert!(
            vram_for_ctx_mb(&e, ctx, base) + reserve <= free,
            "ctx {} обязан влезать: {:.0} МБ",
            ctx,
            vram_for_ctx_mb(&e, ctx, base) + reserve
        );
        assert!(
            vram_for_ctx_mb(&e, ctx + 256, base) + reserve > free,
            "ctx+256 обязан не влезать (максимальность)"
        );
    }

    /// U3, большая карта: свободной VRAM хватает на полный след при окне —
    /// гейт НЕ трогает слот (возвращает base_ctx как есть).
    #[test]
    fn vram_ctx_gate_keeps_window_on_big_card() {
        let e = est(5_167.0, 8_192.0, 32);
        let base = 262_144u32;
        // Полный след 13999 (5167+8192+640, буферы честные B2) + 512 < 15000.
        assert_eq!(max_fitting_ctx_vram_estimate(&e, base, 15_000.0, 512.0), base);
        // Запас ровно на границе критерия — тоже окно.
        let need = vram_for_ctx_mb(&e, base, base) + 512.0;
        assert_eq!(max_fitting_ctx_vram_estimate(&e, base, need, 512.0), base);
    }

    /// Веса + буферы + резерв сами по себе не влезают ни при каком контексте →
    /// 0 = «контекстом карту не спасти», вызывающий уходит к урезанию ngl.
    #[test]
    fn vram_ctx_gate_returns_zero_when_weights_do_not_fit() {
        let e = est(5_167.0, 8_192.0, 32);
        let base = 262_144u32;
        // 5167 + 640 + 512 = 6319 > 5900 → headroom < 0 → 0.
        assert_eq!(max_fitting_ctx_vram_estimate(&e, base, 5_900.0, 512.0), 0);
        // base_ctx == 0 — вырожденный вход, без деления на ноль.
        assert_eq!(max_fitting_ctx_vram_estimate(&e, 0, 15_000.0, 512.0), 0);
    }

    /// Инверсия VRAM-ctx-гейта: результат проходит критерий, максимален и
    /// кратен 256. SWA-часть удерживается на base_ctx — максимальность при
    /// этом сохраняется (dense-часть всё равно растёт от +256).
    #[test]
    fn vram_ctx_gate_is_inverse_of_vram_for_ctx_mb() {
        let base = 16_384u32;
        let reserve = 512.0;
        // Две модели: dense-only и со SWA-частью.
        let mut e = est(4_000.0, 3_000.0, 30);
        let e_swa = VramEstimate { kv_swa_mb: 800.0, ..e.clone() };
        for (name, est) in [("dense", e.clone()), ("swa", e_swa)] {
            for budget in [5_000.0, 6_000.0, 7_000.0, 9_000.0, 12_000.0, 20_000.0] {
                let ctx = max_fitting_ctx_vram_estimate(&est, base, budget, reserve);
                assert!(ctx <= base, "{}: ctx {} > base", name, ctx);
                assert_eq!(ctx % 256, 0, "{}: ctx {} не кратен 256", name, ctx);
                if ctx == 0 {
                    // Сентинел «ничего не влезает»: даже минимальный реальный
                    // контекст (256) не проходит критерий — честный отказ, а не
                    // контекст-ноль, обязанный влезать.
                    assert!(
                        vram_for_ctx_mb(&est, 256, base) + reserve > budget,
                        "{}: бюджет {:.0}: вернулся 0, но 256 токенов влезает ({:.0})",
                        name,
                        budget,
                        vram_for_ctx_mb(&est, 256, base) + reserve
                    );
                    continue;
                }
                assert!(
                    vram_for_ctx_mb(&est, ctx, base) + reserve <= budget,
                    "{}: бюджет {:.0}: ctx {} не влезает ({:.0})",
                    name,
                    budget,
                    ctx,
                    vram_for_ctx_mb(&est, ctx, base) + reserve
                );
                if ctx < base {
                    assert!(
                        vram_for_ctx_mb(&est, (ctx + 256).min(base), base) + reserve > budget,
                        "{}: бюджет {:.0}: ctx {} не максимален",
                        name,
                        budget,
                        ctx
                    );
                }
            }
        }
    }

    /// Урезание ctx НЕ ужимает SWA-часть (окно ограничено) — след на ctx ниже
    /// окна держит SWA на уровне base_ctx (консервативная переоценка).
    #[test]
    fn vram_for_ctx_holds_swa_at_base_level() {
        let mut e = est(4_000.0, 3_000.0, 30);
        e.kv_swa_mb = 800.0; // dense = 2200
        let base = 16_384u32;
        // На base: полный след (dense + swa целиком).
        let full = vram_for_ctx_mb(&e, base, base);
        assert!((full - (4_000.0 + 3_000.0 + 640.0)).abs() < 0.01, "full: {}", full);
        // На base/2: dense вдвое меньше, SWA целиком.
        let half = vram_for_ctx_mb(&e, 8_192, base);
        let dense_half = 2_200.0 * 8_192.0 / 16_384.0;
        assert!((half - (4_000.0 + dense_half + 800.0 + 640.0)).abs() < 0.01, "half: {}", half);
    }

    // ----------------------------------------------- property P1-P4 (proptest) ---

    use proptest::prelude::*;

    /// Общая стратегия est для property-тестов: свайп realistic-диапазонов
    /// (модель до 50 ГБ, KV до 20 ГБ, SWA — доля KV, буферы, слои, окно —
    /// кратное 256 до 262144, бюджет, резерв).
    fn est_strategy() -> impl Strategy<Value = (VramEstimate, f64, f64, u32)> {
        (
            100.0..50_000.0,      // model_mb
            10.0..20_000.0,       // kv_mb
            0.0..1.0,             // swa_frac
            0.0..1_000.0,         // buffers_mb
            1u32..200,            // layers
            1u32..1024,           // window_cells (×256)
            500u32..100_000,      // budget_raw
            0u32..=2_048,         // reserve_raw
        )
            .prop_map(|(model, kv, swa_frac, buffers, layers, cells, budget, reserve)| {
                let base = cells * 256;
                let e = VramEstimate {
                    model_mb: model,
                    kv_per_token_mb: kv / base as f64,
                    kv_mb: kv,
                    kv_swa_mb: kv * swa_frac,
                    buffers_mb: buffers,
                    total_mb: model + kv + buffers,
                    num_layers: layers,
                    num_attn_layers: layers,
                };
                (e, budget as f64, reserve as f64, base)
            })
    }

    proptest! {
        // P1 + P4: инвариант невозможности отказа — для ЛЮБЫХ входов гейт
        // возвращает контекст, проходящий VRAM-критерий, либо 0 (явная ветка
        // «контекстом не помочь» → урезание ngl). Тихого «влезает, но не
        // влезло» не существует. Попутно: результат ≤ окна и кратен 256.
        #[test]
        fn prop_ctx_gate_never_overfits_silently(
            (e, budget, reserve, base) in est_strategy(),
        ) {
            let ctx = max_fitting_ctx_vram_estimate(&e, base, budget, reserve);
            prop_assert!(ctx <= base, "P4: ctx {} > окна {}", ctx, base);
            prop_assert_eq!(ctx % 256, 0, "кратность 256");
            prop_assert!(
                ctx == 0 || vram_for_ctx_mb(&e, ctx, base) + reserve <= budget,
                "P1: ctx {} не проходит критерий при бюджете {:.0}",
                ctx,
                budget
            );
        }

        // P2: монотонность — больше свободной VRAM → выбранный ctx не меньше.
        #[test]
        fn prop_ctx_gate_monotone_in_budget(
            (e, budget, reserve, base) in est_strategy(),
            extra in 1u32..=50_000,
        ) {
            let b2 = budget + extra as f64;
            let ctx1 = max_fitting_ctx_vram_estimate(&e, base, budget, reserve);
            let ctx2 = max_fitting_ctx_vram_estimate(&e, base, b2, reserve);
            prop_assert!(ctx1 <= ctx2, "P2: бюджет {:.0}+{:.0} дал ctx {} < {}",
                budget, extra, ctx2, ctx1);
        }

        // P3: полная KV-модель — при ngl>0 в оценке ПОЛНЫЙ kv_mb; при ngl==0
        // оценки KV нет вовсе (только буферы); полный оффлоад — точная сумма.
        #[test]
        fn prop_ngl_estimate_full_kv_model(
            (e, _budget, _reserve, _base) in est_strategy(),
            ngl_raw in 0u32..=250,
        ) {
            let ngl = ngl_raw;
            if ngl == 0 {
                prop_assert!(
                    (vram_for_ngl_estimate(&e, 0) - e.buffers_mb).abs() < 1e-9,
                    "ngl=0: ровно буферы, KV не входит"
                );
            } else {
                prop_assert!(
                    vram_for_ngl_estimate(&e, ngl) >= e.kv_mb + e.buffers_mb - 1e-9,
                    "ngl>0: полный KV обязан входить"
                );
                prop_assert!(
                    vram_for_ngl_estimate(&e, ngl) <= e.model_mb + e.kv_mb + e.buffers_mb + 1e-9,
                    "ngl>0: оценка не превышает сумму частей"
                );
            }
            if ngl >= e.num_layers {
                let full = vram_for_ngl_estimate(&e, e.num_layers);
                prop_assert!((full - (e.model_mb + e.kv_mb + e.buffers_mb)).abs() < 1e-9,
                    "полный оффлоад — точная сумма");
            }
        }

        // P3+P1 для ngl-инверсии: выбранный safe-кламп проходит собственный
        // критерий и максимален на сетке любой плотности (то же свойство, что
        // проверяет max_fitting_ngl_is_inverse_of_vram_for_ngl_safe, но на
        // произвольных входах).
        #[test]
        fn prop_ngl_safe_inverse_passes_and_maximal(
            (e, budget, reserve, _base) in est_strategy(),
        ) {
            let ngl = max_fitting_ngl_safe_estimate(&e, budget, reserve, 1.25);
            let fits = |n: u32| vram_for_ngl_safe(&e, n, reserve, 1.25) <= budget;
            if ngl == 0 {
                // Сентинел «оффлоад невозможен»: оффлоад даже одного слоя не
                // проходит критерий (в т.ч. резерв > бюджета — case a, и
                // веса одного слоя не влезают — case b). Контр-example
                // (budget 500 / reserve 501) из regressions-файла ловит ровно
                // это: fits(0) там ложен по построению.
                prop_assert!(
                    !fits(1),
                    "ngl=0 вернулся, но оффлоад одного слоя проходит критерий"
                );
                return Ok(());
            }
            prop_assert!(fits(ngl), "ngl={} не проходит собственный критерий", ngl);
            if ngl < e.num_layers {
                prop_assert!(!fits(ngl + 1), "ngl={} не максимален", ngl);
            }
        }
    }
}