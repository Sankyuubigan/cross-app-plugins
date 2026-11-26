//! Проба начала SSE-потока: отказ апстрима внутри успешного ответа.
//!
//! ## Зачем
//!
//! Часть апстримов сообщает об отказе **внутри успешного потока**: HTTP 200,
//! `content-type: text/event-stream`, а в теле —
//! `data: {"error":{...,"code":"upstream_busy"}}`. По HTTP-статусу такой ответ
//! неотличим от здорового, поэтому `proxy.rs` не может ни пометить ключ, ни
//! перейти к следующему: клиент получает нестандартную ошибку и обычно ничего
//! не ретраит.
//!
//! Проба читает **начало** потока и решает: отдать клиенту или считать попытку
//! неудачной. Это единственная точка, где можно «передумать» после получения
//! ответа: пока клиенту не ушло ни байта, повтор другим ключом не создаёт
//! склейки двух ответов (инвариант `proxy.rs`).
//!
//! ## Границы
//!
//! Что считать ошибкой, а что — контентом, решает адаптер протокола
//! ([`VendorProtocol::classify_stream_frame`] / [`VendorProtocol::frame_has_content`]):
//! формат кадра вендор-специфичен. Модуль только читает байты, режет их на
//! кадры и соблюдает лимиты. Кулдаун и ответ наружу — политика `proxy.rs`.
//!
//! ## Лимиты: тишина, а не абсолютный дедлайн
//!
//! Проба не имеет права задерживать здоровый поток: она возвращает управление
//! на первом содержательном кадре. Но «первый содержательный кадр» не обязан
//! приходить быстро: 100milionseek держит модель в очереди и шлёт
//! `: waiting for the model` и `: ping` десятки секунд, а кадр-ошибку —
//! только когда очередь дойдёт (живые замеры 26.11.2026: 34–69 с).
//!
//! Поэтому отсчёт идёт от **последней активности**, а не от старта: любой
//! пришедший байт (включая `: ping`) его сдвигает. Пока апстрим «дышит»,
//! проба ждёт; сдаётся она только на настоящей тишине
//! ([`ProbeLimits::idle_timeout`]) или на абсолютном потолке
//! ([`ProbeLimits::hard_cap`]).
//!
//! Абсолютный дедлайн в 5 с (первая версия модуля) был ошибкой: проба видела
//! одни пинги, объявляла поток здоровым и отдавала клиенту 200, а кадр busy
//! приходил через полминуты — когда failover уже невозможен. Клиент получал
//! неретраибельную ошибку при живом механизме повтора.
//!
//! Сдача по тишине/потолку — **не ошибка**: накопленный префикс уходит
//! клиенту, поток продолжается как обычно. Но она видна: [`PassReason`]
//! объясняет, почему проба не дождалась, и `proxy.rs` пишет это в журнал.
//!
//! ## Байтовая точность
//!
//! Прочитанное не теряется и не дублируется: `prefix` — ровно прочитанные
//! байты, `rest` — продолжение исходного потока. Вместе они дают исходную
//! последовательность байт в исходном порядке (тест
//! `prefix_and_rest_reproduce_the_stream_byte_for_byte`).

use std::time::{Duration, Instant};

use axum::body::Bytes;
use futures_util::{Stream, StreamExt};

use crate::protocol::{StreamError, VendorProtocol};

/// Пределы пробы.
///
/// Время задано двумя границами, и обе нужны:
///
/// - [`Self::idle_timeout`] — сколько терпеть **молчание**. Отсчитывается от
///   последнего полученного байта, поэтому пинги и «waiting for the model»
///   продлевают ожидание: апстрим в очереди — не сломанный апстрим.
/// - [`Self::hard_cap`] — абсолютный потолок от старта пробы. Страховка от
///   апстрима, который вечно шлёт пинги и никогда не присылает ни контента,
///   ни ошибки: без него клиент не увидел бы ни байта.
#[derive(Debug, Clone, Copy)]
pub struct ProbeLimits {
    /// Сколько байт максимум прочитать до решения.
    pub max_bytes: usize,
    /// Сколько терпеть тишину между чанками.
    pub idle_timeout: Duration,
    /// Абсолютный потолок ожидания от начала пробы.
    pub hard_cap: Duration,
}

impl Default for ProbeLimits {
    fn default() -> Self {
        // 30 с тишины — с большим запасом: живой апстрим шлёт пинг раз в
        // несколько секунд, а между «waiting for the model» и первым токеном
        // на 100milionseek проходило до 69 с при непрерывных пингах.
        // 120 с потолка — дольше клиент (ZCode) сам оборвёт запрос по своему
        // таймауту; ждать дольше смысла нет. 64 КБ — с запасом на пинги,
        // кадры метаданных и SSE-заголовки, без риска копить ответ в памяти.
        Self {
            max_bytes: 64 * 1024,
            idle_timeout: Duration::from_secs(30),
            hard_cap: Duration::from_secs(120),
        }
    }
}

/// Почему проба отдала поток, не дождавшись контента.
///
/// Нужен для журнала: сдача по тишине означает, что поздняя ошибка апстрима
/// уйдёт клиенту как есть и failover не сработает. Без этой причины такой
/// случай был неотличим от здорового потока — живой разбор 27.11.2026
/// упёрся именно в молчание пробы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassReason {
    /// Пришёл содержательный кадр — обычный, здоровый случай.
    Content,
    /// Апстрим замолчал дольше `idle_timeout`.
    Idle,
    /// Прочитан лимит объёма.
    ByteLimit,
    /// Поток закончился, не прислав контента (например, только `[DONE]`).
    EndOfStream,
    /// Сработал абсолютный потолок `hard_cap`.
    HardCap,
}

/// Чем закончилась проба.
#[must_use]
pub enum ProbeOutcome<S, E> {
    /// Поток здоров либо «не разобрались»: отдать клиенту.
    /// `prefix` уже прочитан и должен уйти первым, затем — `rest`.
    /// `reason` говорит, дождалась ли проба контента.
    Pass { prefix: Bytes, rest: S, reason: PassReason },
    /// В потоке найден отказ. `prefix`/`rest` сохранены, чтобы отдать поток
    /// клиенту без изменений, если код отказа не наш (решает `proxy.rs`).
    Error { error: StreamError, prefix: Bytes, rest: S },
    /// Поток оборвался до первого содержательного кадра: клиенту ничего не
    /// отдано, попытку можно повторить другим ключом.
    Broken(E),
}

/// Прочитать начало потока и решить, можно ли отдавать его клиенту.
///
/// Возвращает управление на первом содержательном кадре (обычный случай),
/// на кадре-ошибке, по тишине, по лимиту объёма, на потолке или на конце
/// потока. В каждом случае [`PassReason`] объясняет причину.
pub async fn probe_stream<S, E>(
    mut stream: S,
    adapter: &dyn VendorProtocol,
    limits: ProbeLimits,
) -> ProbeOutcome<S, E>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    let started = Instant::now();
    // Тишина отсчитывается от последнего байта, а не от старта: пинг — тоже
    // признак жизни, и апстрим в очереди не должен считаться сломанным.
    let mut last_activity = started;
    // Всё прочитанное: и разобранные кадры, и хвост без разделителя.
    let mut buf: Vec<u8> = Vec::new();
    // Начало ещё не разобранного хвоста: полные кадры уже проверены.
    let mut scanned = 0usize;

    loop {
        // 1. Разобрать все полные кадры, пришедшие к этому моменту.
        while let Some(end) = frame_end(&buf[scanned..]) {
            let frame = &buf[scanned..scanned + end];
            if let Some(error) = adapter.classify_stream_frame(frame) {
                let prefix = Bytes::from(buf);
                return ProbeOutcome::Error { error, prefix, rest: stream };
            }
            if adapter.frame_has_content(frame) {
                let prefix = Bytes::from(buf);
                return ProbeOutcome::Pass { prefix, rest: stream, reason: PassReason::Content };
            }
            // Пинг, пустая delta, одна role — поток ещё не «ожил», смотрим дальше.
            scanned += end;
        }

        // 2. Лимит объёма: дальше не читаем, отдаём что есть.
        if buf.len() >= limits.max_bytes {
            let prefix = Bytes::from(buf);
            return ProbeOutcome::Pass { prefix, rest: stream, reason: PassReason::ByteLimit };
        }

        // 3. Сколько ещё ждать: до тишины или до потолка — что раньше.
        let idle_deadline = last_activity + limits.idle_timeout;
        let hard_deadline = started + limits.hard_cap;
        let deadline = idle_deadline.min(hard_deadline);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let prefix = Bytes::from(buf);
            let reason = if idle_deadline <= hard_deadline {
                PassReason::Idle
            } else {
                PassReason::HardCap
            };
            return ProbeOutcome::Pass { prefix, rest: stream, reason };
        }
        match tokio::time::timeout(remaining, stream.next()).await {
            Ok(Some(Ok(chunk))) => {
                if !chunk.is_empty() {
                    last_activity = Instant::now();
                }
                buf.extend_from_slice(&chunk);
            }
            // Обрыв до первого контента: клиенту ничего не ушло — можно
            // повторить другим ключом (решение принимает `proxy.rs`).
            Ok(Some(Err(e))) => return ProbeOutcome::Broken(e),
            // Поток закончился, не прислав содержательного кадра (например,
            // только `[DONE]`): отдаём как есть.
            Ok(None) => {
                let prefix = Bytes::from(buf);
                return ProbeOutcome::Pass { prefix, rest: stream, reason: PassReason::EndOfStream };
            }
            // Тишина или потолок — не ошибка: медленный провайдер не должен
            // ломаться. Причина едет наружу, чтобы случай был виден в журнале.
            Err(_) => {
                let prefix = Bytes::from(buf);
                let reason = if idle_deadline <= hard_deadline {
                    PassReason::Idle
                } else {
                    PassReason::HardCap
                };
                return ProbeOutcome::Pass { prefix, rest: stream, reason };
            }
        }
    }
}

/// Конец первого полного кадра в `buf` (позиция ЗА разделителем) или `None`.
///
/// Кадр SSE заканчивается пустой строкой: `\n\n` либо `\r\n\r\n`. Смешанные
/// окончания (`\r\n\n`) тоже считаются разделителем — так велит спецификация
/// SSE, а апстримы не обязаны быть аккуратными.
fn frame_end(buf: &[u8]) -> Option<usize> {
    let mut line_start = 0usize;
    let mut i = 0usize;
    while i < buf.len() {
        if buf[i] != b'\n' {
            i += 1;
            continue;
        }
        // Строка `line_start..line_end`; `\r` перед `\n` — часть окончания.
        let line_end = if i > line_start && buf[i - 1] == b'\r' { i - 1 } else { i };
        if line_end == line_start {
            return Some(i + 1); // пустая строка — конец кадра
        }
        line_start = i + 1;
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::OpenAiProtocol;

    fn ok(bytes: &'static [u8]) -> Result<Bytes, std::io::Error> {
        Ok(Bytes::from_static(bytes))
    }

    /// Поток из готовых чанков: границы чанков задаёт тест, а не сеть.
    fn stream_of(
        items: Vec<Result<Bytes, std::io::Error>>,
    ) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Unpin {
        futures_util::stream::iter(items)
    }

    #[tokio::test]
    async fn error_frame_is_detected_even_when_split_across_chunks() {
        // Живой кадр 100milionseek (дамп шлюза 26.11.2026), порезанный так,
        // что границы чанков не совпадают с границами кадра.
        let body = concat!(
            ": waiting for the model\n\n",
            "data: {\"error\":{\"message\":\"The service is busy right now.\",",
            "\"type\":\"server_error\",\"code\":\"upstream_busy\"}}\n\n"
        );
        let raw = body.as_bytes();
        let items = vec![
            Ok(Bytes::copy_from_slice(&raw[..7])),
            Ok(Bytes::copy_from_slice(&raw[7..30])),
            Ok(Bytes::copy_from_slice(&raw[30..])),
        ];

        let outcome = probe_stream(stream_of(items), &OpenAiProtocol, ProbeLimits::default()).await;
        match outcome {
            ProbeOutcome::Error { error, prefix, .. } => {
                assert_eq!(error.code.as_deref(), Some("upstream_busy"));
                assert!(error.message.contains("busy"), "{}", error.message);
                // Префикс — всё прочитанное, включая кадр-ошибку: без него
                // нельзя отдать поток как есть при незнакомом коде.
                assert_eq!(prefix.as_ref(), raw);
            }
            _ => panic!("ожидался Error"),
        }
    }

    #[tokio::test]
    async fn unknown_code_is_reported_without_judgement() {
        // Код не из таблицы «занято»: проба обязана его вернуть, но решение
        // (failover или отдать как есть) принимает proxy.rs.
        let items = vec![ok(b"data: {\"error\":{\"code\":\"weird_error\",\"message\":\"x\"}}\n\n")];
        let outcome = probe_stream(stream_of(items), &OpenAiProtocol, ProbeLimits::default()).await;
        match outcome {
            ProbeOutcome::Error { error, .. } => {
                assert_eq!(error.code.as_deref(), Some("weird_error"));
            }
            _ => panic!("ожидался Error"),
        }
    }

    #[tokio::test]
    async fn metadata_frames_are_skipped_until_content() {
        let body = concat!(
            ": ping\n\n",
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n"
        );
        let outcome = probe_stream(
            stream_of(vec![Ok(Bytes::from_static(body.as_bytes()))]),
            &OpenAiProtocol,
            ProbeLimits::default(),
        )
        .await;
        match outcome {
            // Один чанк содержит всё: префикс — ровно он, проба не режет кадры.
            ProbeOutcome::Pass { prefix, .. } => assert_eq!(prefix.as_ref(), body.as_bytes()),
            _ => panic!("ожидался Pass"),
        }
    }

    #[tokio::test]
    async fn prefix_and_rest_reproduce_the_stream_byte_for_byte() {
        let full = concat!(
            ": ping\n\n",
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Привет\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\" мир\"}}]}\n\n",
            "data: [DONE]\n\n"
        )
        .as_bytes()
        .to_vec();
        // Режем по 7 байт: границы чанков рвут и кадры, и UTF-8 — шлюз не
        // имеет права на перекодирование.
        let items: Vec<Result<Bytes, std::io::Error>> =
            full.chunks(7).map(|c| Ok(Bytes::copy_from_slice(c))).collect();

        let outcome = probe_stream(stream_of(items), &OpenAiProtocol, ProbeLimits::default()).await;
        let (prefix, mut rest) = match outcome {
            ProbeOutcome::Pass { prefix, rest, reason } => {
                assert_eq!(reason, PassReason::Content);
                (prefix, rest)
            }
            _ => panic!("ожидался Pass"),
        };
        let mut got = prefix.to_vec();
        while let Some(item) = rest.next().await {
            got.extend_from_slice(&item.expect("чанк без ошибки"));
        }
        assert_eq!(got, full, "prefix + rest обязаны дать исходный поток байт в байт");
        assert!(
            prefix.len() < full.len(),
            "проба должна остановиться на контентном кадре, не дочитывая поток"
        );
    }

    #[tokio::test]
    async fn done_only_stream_passes_when_it_ends() {
        let outcome = probe_stream(
            stream_of(vec![ok(b"data: [DONE]\n\n")]),
            &OpenAiProtocol,
            ProbeLimits::default(),
        )
        .await;
        match outcome {
            ProbeOutcome::Pass { prefix, .. } => assert_eq!(prefix.as_ref(), b"data: [DONE]\n\n"),
            _ => panic!("ожидался Pass"),
        }
    }

    #[tokio::test]
    async fn idle_silence_passes_with_idle_reason() {
        // Апстрим прислал пинг и замолчал: ждать вечно нельзя.
        let stream = async_stream::stream! {
            yield Ok::<Bytes, std::io::Error>(Bytes::from_static(b": ping\n\n"));
            tokio::time::sleep(Duration::from_secs(30)).await;
            yield Ok(Bytes::from_static(b"data: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n"));
        };
        let limits = ProbeLimits {
            max_bytes: 1024,
            idle_timeout: Duration::from_millis(60),
            hard_cap: Duration::from_secs(30),
        };
        let outcome = probe_stream(Box::pin(stream), &OpenAiProtocol, limits).await;
        match outcome {
            ProbeOutcome::Pass { prefix, reason, .. } => {
                assert_eq!(prefix.as_ref(), b": ping\n\n");
                assert_eq!(reason, PassReason::Idle);
            }
            _ => panic!("тишина не должна превращаться в ошибку"),
        }
    }

    #[tokio::test]
    async fn pings_keep_the_probe_waiting_for_the_busy_frame() {
        // ГЛАВНЫЙ РЕГРЕССИОННЫЙ ТЕСТ (живой баг 27.11.2026).
        //
        // 100milionseek держит модель в очереди: `: waiting for the model`,
        // затем пинги каждые 50 мс, и только через ~400 мс — кадр busy.
        // Проба с абсолютным дедлайном в 5 с отдавала такой поток клиенту
        // (пинги — не контент, а ошибки ещё нет), и клиент получал
        // неретраибельный 200+error. Теперь пинг — признак жизни: отсчёт
        // идёт от последней активности, и проба дожидается кадра busy.
        let stream = async_stream::stream! {
            yield Ok::<Bytes, std::io::Error>(Bytes::from_static(b": waiting for the model\n\n"));
            for _ in 0..8 {
                tokio::time::sleep(Duration::from_millis(50)).await;
                yield Ok(Bytes::from_static(b": ping\n\n"));
            }
            yield Ok(Bytes::from_static(
                b"data: {\"error\":{\"message\":\"busy\",\"code\":\"upstream_busy\"}}\n\n",
            ));
        };
        // Тишина 150 мс: пинг раз в 50 мс её не превышает, значит проба обязана
        // ждать, а не сдаться. Абсолютный потолок намеренно меньше суммарного
        // времени пингов — так тест доказывает, что решает именно тишина.
        let limits = ProbeLimits {
            max_bytes: 64 * 1024,
            idle_timeout: Duration::from_millis(150),
            hard_cap: Duration::from_secs(30),
        };
        let outcome = probe_stream(Box::pin(stream), &OpenAiProtocol, limits).await;
        match outcome {
            ProbeOutcome::Error { error, .. } => {
                assert_eq!(error.code.as_deref(), Some("upstream_busy"));
            }
            ProbeOutcome::Pass { reason, .. } => {
                panic!("проба сдалась ({:?}) вместо ожидания кадра busy", reason)
            }
            ProbeOutcome::Broken(_) => panic!("неожиданный обрыв"),
        }
    }

    #[tokio::test]
    async fn hard_cap_stops_an_endless_ping_stream() {
        // Апстрим вечно шлёт пинги и ничего больше: без потолка клиент не
        // увидел бы ни байта. Потолок отдаёт накопленное с причиной HardCap.
        let stream = async_stream::stream! {
            loop {
                tokio::time::sleep(Duration::from_millis(20)).await;
                yield Ok::<Bytes, std::io::Error>(Bytes::from_static(b": ping\n\n"));
            }
        };
        let limits = ProbeLimits {
            max_bytes: 64 * 1024,
            idle_timeout: Duration::from_secs(30),
            hard_cap: Duration::from_millis(120),
        };
        let outcome = probe_stream(Box::pin(stream), &OpenAiProtocol, limits).await;
        match outcome {
            ProbeOutcome::Pass { prefix, reason, .. } => {
                assert_eq!(reason, PassReason::HardCap);
                assert!(!prefix.is_empty(), "накопленные пинги обязаны уйти клиенту");
            }
            _ => panic!("потолок не должен превращаться в ошибку"),
        }
    }

    #[tokio::test]
    async fn byte_limit_is_not_an_error() {
        let limits = ProbeLimits {
            max_bytes: 16,
            idle_timeout: Duration::from_secs(30),
            hard_cap: Duration::from_secs(60),
        };
        let items = vec![
            ok(b"0123456789abcdef"),
            ok(b"tail-not-read"),
        ];
        let outcome = probe_stream(stream_of(items), &OpenAiProtocol, limits).await;
        match outcome {
            ProbeOutcome::Pass { prefix, mut rest, .. } => {
                assert_eq!(prefix.as_ref(), b"0123456789abcdef");
                // Остаток не потерян: он уходит клиенту следом.
                let tail = rest.next().await.expect("остаток").expect("ok");
                assert_eq!(tail.as_ref(), b"tail-not-read");
            }
            _ => panic!("лимит объёма не должен превращаться в ошибку"),
        }
    }

    #[tokio::test]
    async fn broken_stream_is_reported() {
        let items = vec![
            ok(b": ping\n\n"),
            Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset")),
        ];
        let outcome = probe_stream(stream_of(items), &OpenAiProtocol, ProbeLimits::default()).await;
        assert!(matches!(outcome, ProbeOutcome::Broken(_)));
    }

    #[test]
    fn frame_end_handles_lf_crlf_and_mixed_endings() {
        assert_eq!(frame_end(b"data: x\n\n"), Some(9));
        assert_eq!(frame_end(b"data: x\r\n\r\n"), Some(11));
        assert_eq!(frame_end(b"data: x\r\n\n"), Some(10));
        // Неполный кадр: разделителя ещё нет.
        assert_eq!(frame_end(b"data: x\n"), None);
        assert_eq!(frame_end(b"data: x"), None);
        assert_eq!(frame_end(b""), None);
        // Пустая строка в начале — пустой кадр, а не «нет кадра».
        assert_eq!(frame_end(b"\n\n"), Some(1));
    }
}
