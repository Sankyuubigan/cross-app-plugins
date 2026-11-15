//! Демо Шага 2: `AppShell` + `AppCard`. Шапка 36px: вкладки-кнопки (активная
//! — синяя пилюля), drag окна — только по пустой зоне, ☀/🌙 через
//! `setup_theme`, кнопки окна. Контент — карточки кита.

// Правило владельца §6.8.1: консольное окно у GUI-приложения не нужно.
#![windows_subsystem = "windows"]

use gpui_kit::{
    AppContext as _, Context, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Window, application, div, init, open_window,
};
use my_gpui_kit::{AppCard, AppShell, AppTab, ThemeMode, setup_theme};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Main,
    Settings,
}

struct ShellDemo {
    tab: Tab,
    dark: bool,
}

impl ShellDemo {
    fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        setup_theme(ThemeMode::Dark, cx);
        Self {
            tab: Tab::Main,
            dark: true,
        }
    }
}

impl Render for ShellDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_btn = |id: &'static str, label: &'static str, tab: Tab| {
            let mut t = AppTab::new(id, label);
            if self.tab == tab {
                t = t.active();
            }
            t.on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                this.tab = tab;
                cx.notify();
            }))
        };

        let theme_label = if self.dark {
            SharedString::from("☀ Светлая")
        } else {
            SharedString::from("🌙 Тёмная")
        };

        AppShell::new("gpui-kit — демо каркаса")
            .tab(tab_btn("tab_main", "Главная", Tab::Main))
            .tab(tab_btn("tab_settings", "Настройки", Tab::Settings))
            .theme_toggle(theme_label, cx.listener(|this: &mut Self, _, _, cx| {
                this.dark = !this.dark;
                if this.dark {
                    setup_theme(ThemeMode::Dark, cx);
                } else {
                    setup_theme(ThemeMode::Light, cx);
                }
                cx.notify();
            }))
            .child(match self.tab {
                Tab::Main => div().flex().flex_col().gap_3().p_4()
                    .child(AppCard::new().title("Главная").child("Карточка кита: слой secondary, рамка, скругление 6, p_3."))
                    .child(AppCard::new().child("Вторая карточка без заголовка — тот же слой.")),
                Tab::Settings => div().flex().flex_col().gap_3().p_4()
                    .child(AppCard::new().title("Настройки").child("Страницы настроек верстаются с max_w(px(920.)) — AppShell::max_w.")),
            })
    }
}

fn main() {
    application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            init(cx);
            open_window(my_gpui_kit::window_options(), cx, |window, cx| {
                cx.new(|cx| ShellDemo::new(window, cx))
            })
            .expect("failed to open window");
        });
}
