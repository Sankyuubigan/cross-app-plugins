//! Демо фичи `webview`: LegacyWebViewHost + Airspace Rule (§2.3).
//! Кнопка «Открыть модалку» → host.hide → GPUI-оверлей → «Закрыть» → show.
//! Запуск: cargo run -p my-gpui-kit --features webview --example webview_demo

// Правило владельца §6.8.1: консольное окно у GUI-приложения не нужно.
#![windows_subsystem = "windows"]

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Window, application, div, init, open_window, px,
};
use my_gpui_kit::{LegacyWebViewHost, setup_theme, ThemeMode};

struct WebviewDemo {
    host: LegacyWebViewHost,
    modal_open: bool,
}

impl WebviewDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        setup_theme(ThemeMode::Dark, cx);
        let host = LegacyWebViewHost::build("https://example.com", window, cx)
            .expect("webview не создан");
        Self { host, modal_open: false }
    }
}

impl Render for WebviewDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Entity webview как child: prepaint элемента сам ведёт bounds/clip.
        let mut root = v_flex()
            .size_full()
            .child(
                h_flex().gap_2().p_2().child(
                    Button::new("open_modal").label(if self.modal_open {
                        "Закрыть модалку (show)"
                    } else {
                        "Открыть модалку (hide)"
                    }).on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        if this.modal_open {
                            this.host.show(cx);
                            this.modal_open = false;
                        } else {
                            // Airspace Rule: нативный HWND рисуется ПОВЕРХ GPUI —
                            // модалку открываем только со спрятанным webview.
                            this.host.hide(cx);
                            this.modal_open = true;
                        }
                        cx.notify();
                    })),
                ),
            )
            .child(div().flex().flex_1().child(self.host.webview.clone()));

        if self.modal_open {
            root = root.child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .bg(cx.theme().secondary)
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(px(8.0))
                    .child(SharedString::from(
                        "GPUI-модалка ПОВЕРХ webview: host.hide() сработал.",
                    )),
            );
        }
        root
    }
}

fn main() {
    application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx: &mut App| {
            init(cx);
            open_window(my_gpui_kit::window_options(), cx, |window, cx| {
                cx.new(|cx| WebviewDemo::new(window, cx))
            })
            .expect("failed to open window");
        });
}
