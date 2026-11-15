# AI_UI_RULES — обязательные правила GPUI-UI (кит gpui-kit)

> Этот файл скармливается ИИ-агенту в начале любой задачи на GPUI-интерфейс.
> Кит: `D:\Projects\my-tauri-plugins\gpui-kit` (Cargo-пакет `my-gpui-kit`).

## 0. Перед работой

- Прочитай скиллы `gpui-kit` (Coding Guides) и `gpui-kit-design-guides` (Design Guides):
  где лежат и как вызывать — `from_Tauri_to_GPUI.md` §0.1 (рядом с крейтом).
- Точные сигнатуры API сверяй по сорцам подключённой версии (NEVER INVENT AN API).

## 1. ЗАПРЕЩЕНО

1. Верстать цвета/шрифты с нуля или оставлять дефолтную угольную тему `#09090B` —
   только `setup_theme(mode, cx)` + `palette` из крейта.
2. Рисовать свою шапку окна — только `AppShell` (вкладки, тема ☀/🌙, min/max/close).
3. Кнопки на 100% ширины контейнера внутри `v_flex()` — оборачивать в `h_flex()`
   или ограничивать ширину.
4. RadioGroup/простыни радиокнопок для 3+ вариантов — только `Select`/`SelectState`.
5. Popup/Select/тултипы/модалки поверх активного `gpui-webview` (Airspace Rule) —
   перед открытием `hide()`, после закрытия `show()`.
6. Блокирующие диалоги (rfd, MessageBoxW) в gpui listeners — только асинхронные
   `prompt_for_paths` + oneshot/WeakEntity (урок краша 15.11.26).
7. Терять подписки — каждая `cx.subscribe` хранится в `_subscriptions: Vec<Subscription>`.
8. `unwrap()`/`expect()` вне тестов; ошибки — в лог, не глотать.

## 2. ОБЯЗАТЕЛЬНО

1. `setup_theme(mode, cx)` первой строкой при старте окна (одна строка — тема целиком).
2. `AppShell` для каркаса окна; drag окна — только по пустой зоне шапки (см. урок).
3. `AppCard` для карточек (bg secondary + border + rounded 6 + p 3).
4. Поля ввода/списки/деревья — «утопленный слой»: `bg(input)` + `border_1()` + скругление.
5. Страницы настроек — ограничение ширины `max_w(px(920.))`.
6. Акценты: `primary` (синий) — главные действия и активная вкладка; `success` (зелёный) —
   сохранить/применить; вторичные кнопки — нейтральные с рамкой.
7. Шрифт и размеры задаёт тема (Segoe UI 14px / Consolas 14px) — не переопределять
   без причины.
8. Каждое изменение стейта, влияющее на UI, — `cx.notify()`.

## 3. Версии и уроки

- Пин: `gpui-kit = "=0.9.1"` (gpui-pre 0.3.12 транзитивно). Обновление — осознанное,
  раз в 1–2 месяца, не «вчера с GitHub».
- **WCA::Drag**: hit-test возвращает drag-область, если WCA-hitbox есть в стеке под
  мышью (gpui-pre window.rs:2258) — вешать `window_control_area(Drag)` только на
  отдельный пустой div; дети drag-контейнера на Windows некликабельны (клики уходят
  в HTCAPTION).
- **Theme::change сбрасывает палитру**: кастом ставить только через `Theme::update`
  ПОСЛЕ change (всё внутри `setup_theme`).
- Кнопки окна: `window.minimize_window() / zoom_window() / remove_window()`.
