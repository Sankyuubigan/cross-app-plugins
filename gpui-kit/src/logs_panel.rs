//! LogsPanel — вкладка «Логи» кита (фича `logs`): хвост лог-файла (последние
//! 400 строк, паритет egui/веб-компонентов tauri-plugin-logs) + кнопки
//! Копировать / Сохранить… / Очистить / Обновить.
//!
//! Панель самодостаточна: в дефолтном режиме она сама читает хвост и путь
//! из `logs-core` при каждом рендере. Хост, ведущий drain-цикл (VibeBridge),
//! передаёт хвост через [`LogsPanel::tail`] — тогда чтение отключается, а
//! кнопка «Обновить» вызывает [`LogsPanel::on_refresh`].
//!
//! Поведение кнопок (паритет референса VibeBridge `gpui_ui/logs_view.rs`):
//! - «Копировать» — arboard (на лету, без ошибок глотания — каждая в лог);
//! - «Сохранить…» — экспорт хвоста в файл рядом с exe (модальный диалог
//!   сохранения в GPUI-listener недоступен — блокирующий rfd запрещён,
//!   урок краша 15.11.26);
//! - «Очистить» — трнкейт лог-файла (плагин пишет append-ом, безопасно);
//! - «Обновить» — window.refresh() или колбэк `on_refresh`.
//!
//! Панель предполагается ОДНОЙ на окно (константные id кнопок и блока текста).

use std::path::PathBuf;

use gpui_kit::base::StyledExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::{
    App, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled, StyleRefinement, Window, div, px,
};
use logs_core::logger;

/// Сколько последних строк лога показываем (паритет egui logs_view.rs).
const LOG_TAIL_LINES: usize = 400;

type RefreshFn = Box<dyn Fn(&mut Window, &mut App) + 'static>;

/// Панель логов: `LogsPanel::new()` — сама читает лог; `.tail(...)`/`.path(...)`
/// — хостовые данные (drain-режим).
///
/// Цвета и шрифты — только из `cx.theme()` / `Consolas` (никаких hex).
#[derive(IntoElement)]
pub struct LogsPanel {
    /// Хвост лога. None — панель читает `read_tail(LOG_TAIL_LINES)` сама.
    tail: Option<SharedString>,
    /// Путь лог-файла (для «Очистить» и шапки). None — `log_file_path()`.
    path: Option<PathBuf>,
    /// Заголовок шапки (дефолт «Логи приложения (последние 400 строк)»).
    title: SharedString,
    /// Доп. колбэк «Обновить» (drain-режим: хост перечитывает и notify-ит).
    on_refresh: Option<RefreshFn>,
    style: StyleRefinement,
}

impl LogsPanel {
    pub fn new() -> Self {
        Self {
            tail: None,
            path: None,
            title: "Логи приложения (последние 400 строк)".into(),
            on_refresh: None,
            style: StyleRefinement::default(),
        }
    }

    /// Хвост лога от хоста (drain-режим: RootView обновляет ~раз в секунду).
    pub fn tail(mut self, tail: impl Into<SharedString>) -> Self {
        self.tail = Some(tail.into());
        self
    }

    /// Явный путь лог-файла (иначе — `logs_core::logger::log_file_path()`).
    pub fn path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    /// Заголовок шапки.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    /// Колбэк кнопки «Обновить» (drain-режим; без него — window.refresh()).
    pub fn on_refresh(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_refresh = Some(Box::new(f));
        self
    }
}

impl Styled for LogsPanel {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for LogsPanel {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tail = self.tail.unwrap_or_else(|| SharedString::from(logger::read_tail(LOG_TAIL_LINES)));
        let path = self.path.or_else(logger::log_file_path);
        let path_text = path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "файл лога не найден (лог пишется в память/консоль)".to_string());

        // Кнопки (App-замыкания: Button::on_click принимает
        // Fn(&ClickEvent, &mut Window, &mut App) — listener не нужен).
        let copy_tail = tail.clone();
        let copy = Button::new("logs_copy").label("Копировать").on_click(move |_, _, _| {
            copy_to_clipboard(&copy_tail);
        });

        let save_tail = tail.clone();
        let save = Button::new("logs_save").label("Сохранить…").on_click(move |_, _, _| {
            save_tail_to_exe_dir(&save_tail);
        });

        let clear_path = path.clone();
        let clear = Button::new("logs_clear").label("Очистить").on_click(move |_, window, _| {
            clear_log_file(clear_path.as_deref());
            // Перерисовка: в self-режиме панель перечитает пустой хвост.
            window.refresh();
        });

        let refresh = Button::new("logs_refresh").label("Обновить").on_click(move |_, window, cx| {
            match self.on_refresh.as_ref() {
                Some(f) => f(window, cx),
                None => window.refresh(),
            }
        });

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .gap_2()
            .p_3()
            // Карточка — слой СВЕТЛЕЕ фона окна (паритет AppCard).
            .bg(cx.theme().secondary)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(6.0))
            .overflow_hidden()
            .refine_style(&self.style)
            // Шапка: заголовок + путь лог-файла (паритет egui).
            .child(
                h_flex()
                    .gap_3()
                    .child(div().flex_shrink_0().child(self.title))
                    .child(
                        div()
                            .font_family("Consolas")
                            .text_size(px(12.0))
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(SharedString::from(path_text)),
                    ),
            )
            // Кнопки: плагинный набор (Копировать/Сохранить/Очистить) + Обновить.
            .child(h_flex().gap_2().child(copy).child(save).child(clear).child(refresh))
            .child(div().h(px(1.0)).w_full().bg(cx.theme().border))
            // Текст лога: Consolas 12px, приглушённый, утопленный фон поля, скролл.
            .child(
                div()
                    .id("logs_text")
                    .flex_1()
                    .min_h(px(0.0))
                    .p_2()
                    .bg(cx.theme().input)
                    .rounded(px(6.0))
                    .font_family("Consolas")
                    .text_size(px(12.0))
                    .text_color(cx.theme().muted_foreground)
                    .overflow_y_scroll()
                    .child(tail),
            )
    }
}

/// Копирование в буфер обмена через arboard (краткоживущий клипборд-объект).
fn copy_to_clipboard(text: &str) {
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => match clipboard.set_text(text.to_string()) {
            Ok(()) => log::info!("логи скопированы в буфер обмена ({} строк)", text.lines().count()),
            Err(e) => log::error!("буфер обмена недоступен: {e}"),
        },
        Err(e) => log::error!("буфер обмена недоступен: {e}"),
    }
}

/// Экспорт хвоста в файл рядом с exe (диалог сохранения в GPUI запрещён —
/// урок краша 15.11.26). Пути приложения — от `current_exe().parent()`.
fn save_tail_to_exe_dir(tail: &str) {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.to_path_buf()));
    let Some(dir) = dir else {
        log::error!("сохранение лога: каталог exe не определён");
        return;
    };
    let path = dir.join(format!("log_export_{}.log", logger::timestamp()));
    match std::fs::write(&path, tail) {
        Ok(()) => log::info!("логи сохранены: {}", path.display()),
        Err(e) => log::error!("логи не сохранены: {e}"),
    }
}

/// Трнкейт лог-файла (плагин пишет append-ом на каждый вызов — безопасно).
fn clear_log_file(path: Option<&std::path::Path>) {
    match path {
        Some(path) => match std::fs::write(path, "") {
            Ok(()) => log::info!("лог-файл очищен владельцем"),
            Err(e) => log::error!("лог-файл не очищен: {e}"),
        },
        None => log::error!("очистка невозможна: путь лог-файла неизвестен"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builder-поля по умолчанию: self-режим (tail/path None, дефолтный заголовок).
    #[test]
    fn defaults_are_self_reading() {
        let panel = LogsPanel::new();
        assert!(panel.tail.is_none());
        assert!(panel.path.is_none());
        assert!(panel.on_refresh.is_none());
        assert_eq!(panel.title, "Логи приложения (последние 400 строк)");
    }

    /// Tail-режим хоста: переданный хвост перекрывает самочтение.
    #[test]
    fn builder_sets_host_tail() {
        let panel = LogsPanel::new().tail("строка 1\nстрока 2");
        assert_eq!(panel.tail.as_deref(), Some("строка 1\nстрока 2"));
    }
}
