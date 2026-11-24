//! Детектор и проверка ключей OpenRouter.
//!
//! SSOT для всего, что специфично для OpenRouter (дневная квота
//! бесплатных запросов, проверка ключей): по образцу
//! `is_gemini_provider` ([`crate::key_check`]) — host/base_url, а не
//! protocol или id: пользователь переименовывает и пересоздаёт провайдеров.
//!
//! Пресет OpenRouter уже в каталоге (`assets/provider-presets.json`,
//! base_url `https://openrouter.ai/api/v1`).
//!
//! ## Схема эндпоинта ключей
//!
//! `GET {base_url}/key` (Bearer-ключ) возвращает
//! `{ "data": { label, usage, usage_daily, usage_weekly, usage_monthly,
//! limit, limit_remaining, limit_reset, is_free_tier,
//! free_model_daily_requests, … } }`.
//!
//! `free_model_daily_requests` (`{ used, limit, remaining }`, все целые) —
//! **точная** дневная квота бесплатных запросов аккаунта, добавлена
//! OpenRouter 16.09.2026. Это источник истины для дашборда: аккаунтная
//! (не per-key), учитывает запросы мимо шлюза, сбрасывается в 00:00 UTC.
//! Локальный счётчик `free_usage.json` удалён — он считал ретраи и врал.
//!
//! Денежные поля (`usage*`, `limit*` в $) — про долларовый потолок ключа,
//! к бесплатной квоте запросов отношения не имеют.

use cloud_routers_core::config::{GatewayConfig, ProviderConfig};
use serde::{Deserialize, Serialize};

use crate::key_check::{CheckPlan, CheckVerdict, KeyCheckKind};

/// Сегодняшний UTC-день `ГГГГ-ММ-ДД` — ключ дня квоты (сброс в 00:00 UTC).
///
/// Без `chrono`: в шлюзе календарных часов нет, а тащить зависимость ради
/// одной строки запрещено минимализмом. Считаем дни от эпохи Unix.
pub fn today_ymd() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0);
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Дни от эпохи Unix → (год, месяц, день) — алгоритм «civil_from_days»
/// Говарда Хиннанта. Чистая функция: покрывается точными тестами на
/// известные дни (в отличие от обёртки над `now()`). Guard отрицательного z
/// (trunc-деление Rust ≠ floor).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

/// Провайдер — OpenRouter? (base_url содержит `openrouter.ai`).
pub fn is_openrouter_provider(p: &ProviderConfig) -> bool {
    p.base_url.contains("openrouter.ai")
}

/// Цель проверки ключей: `GET {base_url}/key` (эндпоинт ключевой, не модельный).
pub fn key_info_target_url(base_url: &str) -> Result<String, String> {
    let base = base_url.trim();
    if base.is_empty() {
        return Err("у провайдера нет адреса — проверять нечего".to_string());
    }
    Ok(format!("{base}/key"))
}

/// Конверт ответа `/api/v1/key`: данные лежат под ключом `data`.
#[derive(Debug, Deserialize)]
struct KeyEnvelope {
    data: Option<OpenRouterKeyInfo>,
}

/// Распарсить тело ответа `/api/v1/key`. `None` — тело не JSON, `data`
/// отсутствует или null: ответ не о ключе (мусор релея, HTML-страница и т.п.).
pub fn parse_key_info(body: &str) -> Option<OpenRouterKeyInfo> {
    serde_json::from_str::<KeyEnvelope>(body).ok()?.data
}

/// Классификация ответа OpenRouter: 200 «валиден» только с распарсиваемой
/// `data` — мусорный 200 (HTML-страница релея и т.п.) ключ не осуждает.
pub fn classify_openrouter(status: Option<u16>, body: &str) -> CheckVerdict {
    match status {
        Some(200) if parse_key_info(body).is_some() => CheckVerdict::Valid,
        Some(401 | 403) => CheckVerdict::Invalid,
        Some(429) => CheckVerdict::ValidLimited,
        _ => CheckVerdict::Unknown,
    }
}

/// Дневная квота бесплатных запросов аккаунта из `data.free_model_daily_requests`
/// (добавлена OpenRouter 16.09.2026). Все поля целые.
///
/// Квота **аккаунтная**, а не per-key: все ключи одного аккаунта вернут одно
/// и то же значение — суммировать нельзя. `used` может превышать `limit`
/// (BYOK / аккаунт освобождён от лимита), поэтому остаток берётся из
/// `remaining`, а не считается как `limit - used` (underflow на u64).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreeModelDailyRequests {
    /// Реально засчитанных `:free`-запросов за текущие UTC-сутки.
    pub used: u64,
    /// Реальный дневной потолок аккаунта (50 или 1000).
    pub limit: u64,
    /// Остаток до сброса (00:00 UTC). Истина платформы — не `limit - used`.
    pub remaining: u64,
}

/// Данные ключа из `data` ответа `/api/v1/key`. Отсутствующие/null поля —
/// честный `None`: рисовать «—» честнее, чем выдумывать 0.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OpenRouterKeyInfo {
    /// Тир ключа платформы: `false` — платный, `true` — бесплатный.
    pub is_free_tier: Option<bool>,
    /// Потрачено всего, $.
    pub usage: Option<f64>,
    /// Потрачено за сегодня, $.
    pub usage_daily: Option<f64>,
    /// Аккаунтный лимит расхода, $.
    pub limit: Option<f64>,
    /// Остаток лимита расхода, $.
    pub limit_remaining: Option<f64>,
    /// Как часто обнуляется долларовый лимит (`daily`/`weekly`/`monthly`/…).
    /// Незнакомое значение отображается как есть — не паникуем. Про запросы
    /// отношения не имеет.
    pub limit_reset: Option<String>,
    /// Точная дневная квота бесплатных запросов аккаунта. `None` — блок не
    /// пришёл (старый API, BYOK, освобождён от лимита) → «лимит неизвестен».
    #[serde(default)]
    pub free_model_daily_requests: Option<FreeModelDailyRequests>,
}

/// План проверки ключей OpenRouter: один `GET {base}/key` на ключ.
///
/// `models = vec!["key"]` — цикл zip в [`crate::key_check::run_plan`] требует
/// пары (модель, URL); «key» здесь маркер цели, а не модель — эндпоинт
/// ключевой, не модельный.
pub fn build_openrouter_plan(
    provider: &ProviderConfig,
    config: &GatewayConfig,
) -> Result<CheckPlan, String> {
    let target = key_info_target_url(&provider.base_url)?;
    let via_relay = provider.relay_id.is_some();
    // Общее разрешение вида слота обеих проверок (SSOT причины отказа).
    let proxy = crate::key_check::resolve_plan_route(provider, config)?;
    Ok(CheckPlan {
        kind: KeyCheckKind::OpenRouterKeyInfo,
        models: vec!["key".into()],
        via_relay,
        proxy,
        target_urls: vec![target],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(base: &str) -> ProviderConfig {
        ProviderConfig {
            id: "x".into(),
            name: "X".into(),
            base_url: base.into(),
            ..Default::default()
        }
    }

    #[test]
    fn detector_matches_openrouter_base_urls() {
        assert!(is_openrouter_provider(&provider("https://openrouter.ai/api/v1")));
        assert!(is_openrouter_provider(&provider("https://openrouter.ai/api/v1/")));
        assert!(!is_openrouter_provider(&provider("https://api.groq.com/openai/v1")));
        assert!(!is_openrouter_provider(&provider("")));
    }

    #[test]
    fn key_target_url_is_get_key_endpoint() {
        let t = key_info_target_url("https://openrouter.ai/api/v1").expect("url");
        assert_eq!(t, "https://openrouter.ai/api/v1/key");
        assert!(t.ends_with("/key"), "эндпоинт ключевой: {t}");
        assert!(!t.contains("countTokens"), "не модельный: {t}");
        assert!(!t.contains("chat/completions"), "лимит генерации не расходуется: {t}");
        assert!(key_info_target_url("   ").is_err(), "пустая база — отказ");
    }

    #[test]
    fn parse_key_info_reads_data_and_honest_none() {
        // Реальная схема ответа (сверена по фикстурам CodexBar / issue #6842).
        let body = r#"{"data":{"label":"my-key","is_free_tier":true,"usage":0.42,"usage_daily":0.03,
            "usage_weekly":0.2,"usage_monthly":0.4,"limit":10.0,"limit_remaining":9.97,
            "limit_reset":"daily","include_byok_in_limit":true}}"#;
        let info = parse_key_info(body).expect("parsed");
        assert_eq!(info.is_free_tier, Some(true));
        assert_eq!(info.usage, Some(0.42));
        assert_eq!(info.usage_daily, Some(0.03));
        assert_eq!(info.limit, Some(10.0));
        assert_eq!(info.limit_remaining, Some(9.97));
        assert_eq!(info.limit_reset.as_deref(), Some("daily"));
        // Блока бесплатной квоты в старом ответе нет — честный None.
        assert_eq!(info.free_model_daily_requests, None);

        // Отсутствующие поля → None; незнакомый limit_reset доходит как есть.
        let partial = parse_key_info(r#"{"data":{"limit_reset":"weekly","extra_field":123}}"#)
            .expect("parsed");
        assert_eq!(partial.is_free_tier, None);
        assert_eq!(partial.usage, None);
        assert_eq!(partial.limit_reset.as_deref(), Some("weekly"));

        // data: null / без data / не JSON — ответ не о ключе.
        assert_eq!(parse_key_info(r#"{"data":null}"#), None);
        assert_eq!(parse_key_info(r#"{"foo":1}"#), None);
        assert_eq!(parse_key_info("<html>page</html>"), None);
    }

    #[test]
    fn parse_key_info_reads_free_model_daily_requests() {
        // Блок добавлен OpenRouter 16.09.2026 — точная дневная квота.
        let body = r#"{"data":{"label":"k","is_free_tier":false,"usage_daily":0.03,
            "free_model_daily_requests":{"used":12,"limit":50,"remaining":38}}}"#;
        let info = parse_key_info(body).expect("parsed");
        let q = info.free_model_daily_requests.expect("квота");
        assert_eq!(q.used, 12);
        assert_eq!(q.limit, 50);
        assert_eq!(q.remaining, 38);
        // limit не зависит от is_free_tier: платный ключ, но квота 50.
        assert_eq!(info.is_free_tier, Some(false));

        // used > limit (BYOK / освобождён от лимита) — remaining берётся из ответа.
        let over = parse_key_info(
            r#"{"data":{"free_model_daily_requests":{"used":1004,"limit":1000,"remaining":0}}}"#,
        )
        .expect("parsed");
        let q = over.free_model_daily_requests.expect("квота");
        assert_eq!(q.used, 1004);
        assert_eq!(q.remaining, 0, "остаток — истина платформы, не limit-used");
    }

    #[test]
    fn classify_openrouter_verdicts() {
        let good = r#"{"data":{"is_free_tier":true}}"#;
        assert_eq!(classify_openrouter(Some(200), good), CheckVerdict::Valid);
        // Мусорный 200 — не валидность (ответ пришёл, но не о ключе).
        assert_eq!(classify_openrouter(Some(200), "<html>page</html>"), CheckVerdict::Unknown);
        assert_eq!(classify_openrouter(Some(200), r#"{"data":null}"#), CheckVerdict::Unknown);
        assert_eq!(classify_openrouter(Some(200), r#"{"foo":1}"#), CheckVerdict::Unknown);
        assert_eq!(
            classify_openrouter(Some(401), r#"{"error":{"message":"Invalid key"}}"#),
            CheckVerdict::Invalid
        );
        assert_eq!(classify_openrouter(Some(403), "forbidden"), CheckVerdict::Invalid);
        assert_eq!(classify_openrouter(Some(429), "rate limited"), CheckVerdict::ValidLimited);
        assert_eq!(classify_openrouter(Some(500), "boom"), CheckVerdict::Unknown);
        assert_eq!(classify_openrouter(None, "сеть: timeout"), CheckVerdict::Unknown);
    }

    #[test]
    fn build_openrouter_plan_rejects_dangling_relay() {
        let config = GatewayConfig::default();
        let mut p = provider("https://openrouter.ai/api/v1");
        p.relay_id = Some("ghost".into());
        let err = build_openrouter_plan(&p, &config).expect_err("релей без слота обязан не пустить");
        assert!(err.contains("ghost"), "{err}");
    }
}
