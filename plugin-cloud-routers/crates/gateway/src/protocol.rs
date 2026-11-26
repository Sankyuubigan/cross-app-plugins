// Заготовки Kiro-протокола: активная разработка (PRP 16.11.26), не мёртвый код.
#![allow(dead_code)]

//! Протоколы вендоров: каким языком шлюз говорит с каждым апстримом.
//!
//! ## Зачем трейт, а не `if protocol == Kiro`
//!
//! До этого модуля протокол был захардкожен как единственная реальность:
//! `resolve_route` клеил `base_url + path`, `auth_header` всегда ставил Bearer,
//! `parse_models_response` требовал `data[].id`. Любой новый вендор превращался
//! бы в россыпь `if kiro` по `proxy.rs`/`upstream.rs` — то есть в костыль,
//! который следующий вендор ломает. Трейт делает добавление вендора
//! конструктивно чистым: новый файл с реализацией контракта, ноль правок в
//! существующем коде прокси.
//!
//! ## Граница движения
//!
//! Адаптер строит **запрос** (URL, метод, заголовки, тело) и разбирает
//! **ответ** (модели, поток). Ретраи по ключам, кулдауны, деактивация ключей,
//! маскировка ошибок — остаются в `proxy.rs`: это политика шлюза, одинаковая
//! для всех вендоров. Адаптер не знает про ключи вообще — ему дают готовый
//! bearer-токен строкой.
//!
//! ## OpenAI-адаптер — вынос, не переписывание
//!
//! `OpenAiProtocol` — побайтово то поведение, что было вшито в `proxy.rs` и
//! `upstream.rs`: те же пути, те же заголовки, тот же разбор. Гейт шага 2:
//! весь существующий сьют зелёный без изменений.

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use cloud_routers_core::config::{GatewayConfig, ProviderConfig};
use cloud_routers_core::config::DiscoveredModel;

use crate::upstream::{Route, resolve_route, CHAT_COMPLETIONS_PATH, MODELS_PATH};

/// Запрос к апстриму, готовый к отправке через `reqwest`.
///
/// Тело — байты (уже сериализованный JSON): адаптер сам решает формат.
pub struct UpstreamRequest {
    pub route: Route,
    pub method: reqwest::Method,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

/// Контракт вендора. Реализация обязана быть чистой функцией от
/// (провайдер, конфиг, токен, вход) — без чтения диска и глобального стейта.
pub trait VendorProtocol: Send + Sync {
    /// Какой протокол реализует (для диагностики).
    fn name(&self) -> &'static str;

    /// Запрос списка моделей. Токен — готовый секрет (ключ или access-токен).
    fn models_request(
        &self,
        provider: &ProviderConfig,
        config: &GatewayConfig,
        credential: &str,
    ) -> Result<UpstreamRequest, String>;

    /// Разобрать тело ответа списка моделей.
    fn parse_models(&self, body: &str) -> Result<Vec<DiscoveredModel>, String>;

    /// Запрос инференса. `payload` — исходное тело клиента (OpenAI-формат);
    /// адаптер переводит его в формат вендора.
    fn chat_request(
        &self,
        provider: &ProviderConfig,
        config: &GatewayConfig,
        credential: &str,
        client_headers: &HeaderMap,
        payload: &serde_json::Value,
        upstream_model: &str,
    ) -> Result<UpstreamRequest, String>;

    /// Ответ апстрима — потоковый? Решает по content-type ответа.
    ///
    /// Дефолт — OpenAI-правило (`text/event-stream`); Kiro переопределяет
    /// (бинарный EventStream).
    fn is_stream_response(&self, content_type: &str) -> bool {
        content_type.starts_with("text/event-stream")
    }

    /// Кадр потока — это ошибка апстрима? `None` — обычный кадр (контент,
    /// пинг, служебная метка) либо поток не размечен ошибками вовсе.
    ///
    /// ## Зачем
    ///
    /// Часть апстримов сообщает об отказе **внутри успешного потока**: HTTP
    /// `200`, а в теле — `data: {"error":{...,"code":"upstream_busy"}}`. По
    /// HTTP-статусу такой отказ неотличим от нормального ответа, поэтому шлюз
    /// не может ни пометить ключ, ни перейти к следующему — клиент получает
    /// ошибку в потоке и сам решает, что делать (обычно ничего: формат
    /// нестандартный).
    ///
    /// Разбор — **вендор-специфичная** часть (формат кадра знает только
    /// адаптер), а решение «что делать с отказом» остаётся политикой
    /// `proxy.rs` — граница из шапки модуля не нарушается.
    ///
    /// Дефолт `None` — «поток ошибок не размечивает»: Kiro (бинарный
    /// EventStream) и любой будущий протокол ведут себя как раньше, пока явно
    /// не научатся.
    fn classify_stream_frame(&self, _frame: &[u8]) -> Option<StreamError> {
        None
    }

    /// Кадр потока содержит полезный контент (токен/вызов инструмента)?
    ///
    /// Нужно «пробе» начала потока: пустые кадры, `: ping` и метаданные
    /// (одна `role`, пустая `delta`) не должны считаться «поток ожил» — иначе
    /// ошибка, пришедшая сразу после них, уйдёт клиенту.
    ///
    /// Дефолт `true`: протокол без разметки считается «любой кадр — контент»,
    /// то есть проба не ждёт лишнего.
    fn frame_has_content(&self, _frame: &[u8]) -> bool {
        true
    }

    /// Поток протокола состоит из кадров, которые умеет разбирать
    /// [`Self::classify_stream_frame`]?
    ///
    /// Нужно, чтобы шлюз не «пробовал» поток, размеченный не по правилам SSE:
    /// Kiro отдаёт бинарный EventStream, где поиск разделителя `\n\n` бессмыслен
    /// и добавил бы задержку до первого байта. Дефолт `false` — проба не
    /// делается, поведение прежнее.
    fn stream_frames_are_inspectable(&self) -> bool {
        false
    }
}

/// Ошибка, пришедшая **внутри** потока (а не HTTP-статусом).
///
/// `code` — код апстрима (`upstream_busy`, `rate_limit_error`, ...): по нему
/// [`crate::proxy`] решает, какой [`cloud_routers_core::FailureKind`] применить.
/// `message` — текст для журнала и для тела ответа клиенту; апстрим часто
/// кладёт туда ссылку на биллинг, и терять её нельзя.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamError {
    pub code: Option<String>,
    pub message: String,
}

/// Разобрать тело ошибки апстрима (JSON) в [`StreamError`].
///
/// Один разбор на два пути: кадр внутри SSE (`data: {...}`) и обычное тело
/// ответа со статусом (`503` + `{"error":{...}}`). Формы одни и те же — значит
/// и разбор должен быть один, иначе код «занято» распознавался бы в потоке, но
/// не в статусе (живой случай 26.11.2026: 100milionseek отвечает и так, и так).
///
/// `None` — тело не про ошибку (нет `error`, нет `type: error`): вызывающий
/// решает сам.
pub fn error_from_value(value: &serde_json::Value) -> Option<StreamError> {
    let obj = value.as_object()?;

    // Обёртка `error` — основной путь. `error` строкой тоже бывает
    // (`{"error":"busy"}`): тогда это и код, и сообщение.
    let (code, message) = match obj.get("error") {
        Some(serde_json::Value::Object(err)) => (
            err.get("code").and_then(|v| v.as_str()),
            err.get("message").and_then(|v| v.as_str()),
        ),
        Some(serde_json::Value::String(text)) => (Some(text.as_str()), Some(text.as_str())),
        _ => {
            // Тело без обёртки: `{"type":"error","message":"..."}`.
            let is_error_frame = obj
                .get("type")
                .and_then(|v| v.as_str())
                .map(|t| t.eq_ignore_ascii_case("error"))
                .unwrap_or(false)
                || obj.contains_key("error_code");
            if !is_error_frame {
                return None;
            }
            (
                obj.get("code")
                    .or_else(|| obj.get("error_code"))
                    .and_then(|v| v.as_str()),
                obj.get("message").and_then(|v| v.as_str()),
            )
        }
    };

    Some(StreamError {
        code: code.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()),
        message: message
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "Апстрим вернул ошибку".to_string()),
    })
}

/// OpenAI-совместимый вендор: поведение, ранее вшитое в прокси напрямую.
pub struct OpenAiProtocol;

impl OpenAiProtocol {
    fn bearer(credential: &str) -> HeaderValue {
        crate::upstream::auth_header(credential)
    }
}

impl VendorProtocol for OpenAiProtocol {
    fn name(&self) -> &'static str {
        "openai"
    }

    /// SSE-поток размечен `\n\n`-кадрами — проба начала потока применима.
    fn stream_frames_are_inspectable(&self) -> bool {
        true
    }

    fn models_request(
        &self,
        provider: &ProviderConfig,
        config: &GatewayConfig,
        credential: &str,
    ) -> Result<UpstreamRequest, String> {
        let route = resolve_route(provider, config, MODELS_PATH)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            Self::bearer(credential),
        );
        // Релей без этих заголовков отвечает 401 «missing or invalid
        // x-relay-token» (кейс 25.10.2026 — «Обновить модели» у Google): чат
        // ставит их через chat_request, а этот путь был единственным без них.
        crate::upstream::apply_route_headers(&route, &mut headers);
        Ok(UpstreamRequest {
            route,
            method: reqwest::Method::GET,
            headers,
            body: Vec::new(),
        })
    }

    fn parse_models(&self, body: &str) -> Result<Vec<DiscoveredModel>, String> {
        cloud_routers_core::models::parse_models_response(body)
    }

    fn chat_request(
        &self,
        provider: &ProviderConfig,
        config: &GatewayConfig,
        credential: &str,
        client_headers: &HeaderMap,
        payload: &serde_json::Value,
        upstream_model: &str,
    ) -> Result<UpstreamRequest, String> {
        // Единственное изменение тела — подстановка модели (бывший rewrite_body).
        let mut body = payload.clone();
        let Some(obj) = body.as_object_mut() else {
            return Err("Тело запроса должно быть JSON-объектом".to_string());
        };
        obj.insert(
            "model".to_string(),
            serde_json::Value::String(upstream_model.to_string()),
        );
        let bytes = serde_json::to_vec(&body)
            .map_err(|e| format!("Не удалось сериализовать тело запроса: {}", e))?;

        let route = resolve_route(provider, config, CHAT_COMPLETIONS_PATH)?;
        let mut headers = crate::upstream::forward_headers(client_headers);
        headers.insert(
            HeaderName::from_static("authorization"),
            Self::bearer(credential),
        );
        crate::upstream::apply_route_headers(&route, &mut headers);
        // `content-type` только когда есть тело (бывшее правило build_request).
        if !bytes.is_empty() {
            headers.insert(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/json"),
            );
        }
        Ok(UpstreamRequest {
            route,
            method: reqwest::Method::POST,
            headers,
            body: bytes,
        })
    }

    /// OpenAI-поток: ошибка приходит кадром `data: {"error":{...}}`.
    ///
    /// Формы, которые встречаются в живых апстримах (проверено на дампах
    /// шлюза 26.11.2026):
    ///
    /// ```text
    /// data: {"error":{"message":"...","type":"server_error","code":"upstream_busy"}}
    /// event: error
    /// data: {"error":{"message":"..."}}
    /// data: {"message":"...","type":"error"}          // без обёртки error
    /// ```
    ///
    /// Кадр без `error` (обычный контент, `[DONE]`, `: ping`) — `None`:
    /// классификация не должна срабатывать на здоровом потоке.
    fn classify_stream_frame(&self, frame: &[u8]) -> Option<StreamError> {
        let payload = sse_payload(frame)?;
        let value: serde_json::Value = serde_json::from_str(&payload).ok()?;
        error_from_value(&value)
    }

    /// Контентный кадр OpenAI: непустой `delta` (текст, reasoning, tool_calls)
    /// либо `finish_reason`. Пустая `delta` и кадр с одной `role` — метаданные,
    /// поток ещё не «ожил».
    fn frame_has_content(&self, frame: &[u8]) -> bool {
        let Some(payload) = sse_payload(frame) else {
            return false;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&payload) else {
            return false;
        };
        let Some(choice) = value
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|c| c.first())
        else {
            return false;
        };
        if choice.get("finish_reason").map(|v| !v.is_null()).unwrap_or(false) {
            return true;
        }
        let Some(delta) = choice.get("delta").and_then(|d| d.as_object()) else {
            return false;
        };
        ["content", "reasoning_content", "reasoning", "tool_calls", "function_call"]
            .iter()
            .any(|field| match delta.get(*field) {
                Some(serde_json::Value::String(s)) => !s.is_empty(),
                Some(serde_json::Value::Array(a)) => !a.is_empty(),
                Some(serde_json::Value::Object(o)) => !o.is_empty(),
                _ => false,
            })
    }
}

/// Достать JSON-payload из SSE-кадра (`data: ...`).
///
/// Кадр может содержать несколько строк `data:` (многострочный JSON) — они
/// склеиваются, как требует SSE. Комментарии (`: ping`) и прочие поля
/// (`event:`, `id:`) игнорируются. `[DONE]` и пустой payload — `None`.
fn sse_payload(frame: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(frame).ok()?;
    let mut data = String::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with(':') {
            continue; // комментарий/пинг
        }
        let Some(rest) = line.strip_prefix("data:") else {
            continue;
        };
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        if !data.is_empty() {
            data.push('\n');
        }
        data.push_str(rest);
    }
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    Some(data.to_string())
}

/// Выбрать адаптер по протоколу провайдера.
///
/// Неизвестного протокола здесь быть не может: `VendorProtocol::parse`
/// в core отвергает его при чтении конфига. Поэтому `unreachable!` запрещён —
/// возвращаем ошибку текстом.
pub fn adapter_for(
    protocol: cloud_routers_core::protocol::VendorProtocol,
) -> Result<Box<dyn VendorProtocol>, String> {
    use cloud_routers_core::protocol::VendorProtocol as P;
    match protocol {
        P::Openai => Ok(Box::new(OpenAiProtocol)),
        P::Kiro => Ok(Box::new(crate::protocol_kiro::KiroProtocol)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloud_routers_core::config::AccountKey;

    fn provider() -> ProviderConfig {
        ProviderConfig {
            id: "groq".into(),
            name: "Groq".into(),
            base_url: "https://api.groq.com/openai/v1".into(),
            is_enabled: true,
            use_relay: false,
            keys: vec![AccountKey { id: "k1".into(), key: "gsk-1".into(), is_active: true, name: None }],
            ..Default::default()
        }
    }

    #[test]
    fn openai_models_request_matches_legacy_behavior() {
        let p = OpenAiProtocol;
        let cfg = GatewayConfig::default();
        let req = p.models_request(&provider(), &cfg, "gsk-1").expect("req");
        assert_eq!(req.method, reqwest::Method::GET);
        assert_eq!(req.route.request_url(), "https://api.groq.com/openai/v1/models");
        assert_eq!(
            req.headers.get("authorization").expect("auth").to_str().expect("str"),
            "Bearer gsk-1"
        );
        assert!(req.body.is_empty(), "у GET /models тела быть не должно");
    }

    #[test]
    fn openai_models_request_carries_relay_headers() {
        // Регресс 25.10.2026: models_request строил relay-URL без
        // x-relay-token/x-target-url, и воркер отвечал 401 «missing or invalid
        // x-relay-token» на «Обновить модели» любого relay-провайдера.
        let p = OpenAiProtocol;
        let mut provider = provider();
        // Релей назначается провайдеру привязкой к слоту (27.10.2026).
        provider.relay_id = Some("vercel".into());
        let cfg = GatewayConfig {
            relays: vec![cloud_routers_core::config::RelaySlot {
                id: "vercel".into(),
                kind: cloud_routers_core::config::RELAY_KIND_VERCEL.into(),
                url: Some("https://relay.workers.dev".into()),
                relay_secret: Some("s3cret".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let req = p.models_request(&provider, &cfg, "gsk-1").expect("req");
        assert!(req.route.is_relay());
        assert_eq!(req.route.request_url(), "https://relay.workers.dev/api/gateway/models");
        assert_eq!(
            req.headers
                .get(crate::upstream::HDR_RELAY_TOKEN)
                .expect("token")
                .to_str()
                .expect("str"),
            "s3cret"
        );
        assert_eq!(
            req.headers
                .get(crate::upstream::HDR_TARGET_URL)
                .expect("target")
                .to_str()
                .expect("str"),
            "https://api.groq.com/openai/v1/models"
        );
    }

    #[test]
    fn openai_chat_request_joins_version_inclusive_base() {
        // Конвенция (08.11.2026): base_url включает версионный сегмент,
        // путь ресурса — без `/v1`. Итог: ровно один `/v1` в URL.
        let p = OpenAiProtocol;
        let cfg = GatewayConfig::default();
        let payload = serde_json::json!({"model": "groq/llama", "messages": []});
        let req = p
            .chat_request(&provider(), &cfg, "gsk-1", &HeaderMap::new(), &payload, "llama")
            .expect("req");
        assert_eq!(req.method, reqwest::Method::POST);
        assert_eq!(
            req.route.request_url(),
            "https://api.groq.com/openai/v1/chat/completions"
        );
        let body: serde_json::Value = serde_json::from_slice(&req.body).expect("json");
        assert_eq!(body["model"], "llama", "модель подставлена апстримная");
        assert_eq!(
            req.headers.get("content-type").expect("ct").to_str().expect("str"),
            "application/json"
        );
    }

    #[test]
    fn openai_chat_request_does_not_double_version_segment() {
        // Регресс 08.11.2026 (сессия 1794158295623): chat-путь
        // `/v1/chat/completions` на версионной базе openrouter
        // (`https://openrouter.ai/api/v1`) склеивался в
        // `.../api/v1/v1/chat/completions` → 404 «Not Found» на каждый чат.
        // Google-база `.../v1beta/openai` лишний сегмент прощала, поэтому баг
        // проявлялся только на строгих провайдерах.
        let p = OpenAiProtocol;
        let cfg = GatewayConfig::default();
        let mut or = provider();
        or.id = "openrouter".into();
        or.base_url = "https://openrouter.ai/api/v1".into();
        let payload = serde_json::json!({"model": "openrouter/free", "messages": []});
        let req = p
            .chat_request(&or, &cfg, "sk-or", &HeaderMap::new(), &payload, "openrouter/free")
            .expect("req");
        assert_eq!(
            req.route.request_url(),
            "https://openrouter.ai/api/v1/chat/completions",
            "один `/v1`: версия из базы, без дубля в пути"
        );
    }

    #[test]
    fn kiro_adapter_is_registered() {
        assert!(
            adapter_for(cloud_routers_core::protocol::VendorProtocol::Kiro).is_ok(),
            "Kiro-адаптер обязан быть зарегистрирован (шаг 3)"
        );
    }

    // ── Разметка ошибок внутри SSE-потока (26.11.2026) ──────────────────────

    #[test]
    fn stream_error_frame_is_recognised() {
        let p = OpenAiProtocol;
        // Живой кадр 100milionseek из дампа шлюза 26.11.2026.
        let frame = br#"data: {"error":{"message":"The service is busy right now. Please try again in a moment.","type":"server_error","code":"upstream_busy"}}"#;
        let err = p.classify_stream_frame(frame).expect("ошибка распознана");
        assert_eq!(err.code.as_deref(), Some("upstream_busy"));
        assert!(err.message.contains("busy"), "{}", err.message);
    }

    #[test]
    fn stream_error_frame_forms_without_code() {
        let p = OpenAiProtocol;
        // Обёртка error без code: код неизвестен, но это точно отказ.
        let err = p
            .classify_stream_frame(br#"data: {"error":{"message":"boom"}}"#)
            .expect("ошибка распознана");
        assert_eq!(err.code, None);
        assert_eq!(err.message, "boom");

        // error строкой: строка служит и кодом, и сообщением.
        let err = p.classify_stream_frame(br#"data: {"error":"busy"}"#).expect("распознана");
        assert_eq!(err.code.as_deref(), Some("busy"));

        // Кадр без обёртки, но с type=error.
        let err = p
            .classify_stream_frame(br#"data: {"type":"error","code":"x","message":"bad"}"#)
            .expect("распознана");
        assert_eq!(err.code.as_deref(), Some("x"));
    }

    #[test]
    fn healthy_frames_are_not_errors() {
        let p = OpenAiProtocol;
        // Обычный контентный кадр.
        assert_eq!(
            p.classify_stream_frame(br#"data: {"choices":[{"delta":{"content":"Hi"}}]}"#),
            None
        );
        // Кадр с одной role — метаданные, не ошибка.
        assert_eq!(
            p.classify_stream_frame(br#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#),
            None
        );
        // [DONE], пинг и мусор — не ошибки (и не паника).
        assert_eq!(p.classify_stream_frame(b"data: [DONE]"), None);
        assert_eq!(p.classify_stream_frame(b": ping"), None);
        assert_eq!(p.classify_stream_frame("data: {битый json".as_bytes()), None);
        assert_eq!(p.classify_stream_frame(b""), None);
        // Валидный JSON без error и без type=error.
        assert_eq!(p.classify_stream_frame(br#"data: {"usage":{"total_tokens":5}}"#), None);
    }

    #[test]
    fn frame_has_content_distinguishes_metadata() {
        let p = OpenAiProtocol;
        assert!(p.frame_has_content(br#"data: {"choices":[{"delta":{"content":"a"}}]}"#));
        assert!(p.frame_has_content(
            r#"data: {"choices":[{"delta":{"reasoning_content":"думаю"}}]}"#.as_bytes()
        ));
        assert!(p.frame_has_content(
            br#"data: {"choices":[{"delta":{"tool_calls":[{"id":"1"}]}}]}"#
        ));
        assert!(p.frame_has_content(
            br#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#
        ));
        // Пустая delta / только role — ещё не контент.
        assert!(!p.frame_has_content(br#"data: {"choices":[{"delta":{"role":"assistant"}}]}"#));
        assert!(!p.frame_has_content(br#"data: {"choices":[{"delta":{"content":""}}]}"#));
        assert!(!p.frame_has_content(br#"data: {"choices":[{"delta":{}}]}"#));
        assert!(!p.frame_has_content(b": ping"));
        assert!(!p.frame_has_content(b"data: [DONE]"));
    }

    #[test]
    fn multiline_data_is_joined() {
        // SSE разрешает многострочный data: — строки склеиваются через \n.
        // Живая форма — pretty-printed JSON, разложенный по строкам `data:`.
        let frame = concat!(
            "data: {\"error\":\n",
            "data:  {\"message\":\"busy\",\n",
            "data:   \"code\":\"upstream_busy\"}}"
        );
        let err = OpenAiProtocol
            .classify_stream_frame(frame.as_bytes())
            .expect("распознана");
        assert_eq!(err.code.as_deref(), Some("upstream_busy"));
        assert_eq!(err.message, "busy");
    }

    #[test]
    fn kiro_stream_frames_are_never_errors() {
        // Kiro не переопределяет метод: бинарный EventStream ведёт себя как раньше.
        let kiro = crate::protocol_kiro::KiroProtocol;
        assert_eq!(kiro.classify_stream_frame(br#"data: {"error":{"code":"x"}}"#), None);
        // И любой кадр считается контентом — проба не ждёт лишнего.
        assert!(kiro.frame_has_content(b""));
    }
}
