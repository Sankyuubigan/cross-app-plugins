//! Демо Шага 1: тема окна — одна строка `setup_theme(mode, cx)`.
//! Кнопки переключают тёмную/светлую палитры; две плашки показывают слои
//! «карточка» (secondary) и «поле» (input) поверх фона окна.

// Правило владельца §6.8.1: консольное окно у GUI-приложения не нужно.
#![windows_subsystem = "windows"]

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, TitleBar, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Window, application, div, init, open_window, px,
};
use my_gpui_kit::{ThemeMode, setup_theme};

struct ThemeDemo {
    dark: bool,
}

impl ThemeDemo {
    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        setup_theme(ThemeMode::Dark, cx);
        Self { dark: true }
    }
}

impl Render for ThemeDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark_btn = Button::new("set_dark")
            .label("Тёмная")
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.dark = true;
                setup_theme(ThemeMode::Dark, cx);
                cx.notify();
            }));
        let light_btn = Button::new("set_light")
            .label("Светлая")
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.dark = false;
                setup_theme(ThemeMode::Light, cx);
                cx.notify();
            }));
        // Активная тема — залитая primary, вторая — ghost (паритет вкладок).
        let (dark_btn, light_btn) = if self.dark {
            (dark_btn.primary(), light_btn.ghost())
        } else {
            (dark_btn.ghost(), light_btn.primary())
        };

        v_flex()
            .size_full()
            .gap_4()
            .p_6()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(SharedString::from(
                "setup_theme(mode, cx) — тема одной строкой (палитра 4 слоёв)",
            ))
            .child(h_flex().gap_2().child(dark_btn).child(light_btn))
            // Слой «карточка» — светлее фона окна, с рамкой.
            .child(
                div()
                    .p_4()
                    .bg(cx.theme().secondary)
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(px(6.0))
                    .child(SharedString::from("Карточка (secondary) — слой вверх")),
            )
            // Слой «поле» — утопленный, темнее окна.
            .child(
                div()
                    .p_4()
                    .bg(cx.theme().input)
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(px(6.0))
                    .child(SharedString::from("Поле (input) — утопленный слой")),
            )
    }
}

fn main() {
    application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            init(cx);
            open_window(TitleBar::window_options(), cx, |window, cx| {
                cx.new(|cx| ThemeDemo::new(window, cx))
            })
            .expect("failed to open window");
        });
}
