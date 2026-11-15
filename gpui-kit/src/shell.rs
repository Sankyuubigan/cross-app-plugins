//! AppShell — единый каркас окна кита.
//!
//! Шапка 36px: заголовок, вкладки-кнопки (активная залита primary), пустая
//! drag-зона, переключатель темы ☀/🌙 и кнопки окна (мин/макс/закрыть).
//! Под шапкой — контейнер контента (опционально с ограничением ширины).
//!
//! Урок WCA::Drag (VibeBridge, сборка №19): `window_control_area(Drag)`
//! вешается ТОЛЬКО на отдельный пустой div — hit-test окна возвращает
//! drag-область, если WCA-hitbox есть в стеке под мышью, и тогда клики по
//! детям drag-контейнера на Windows уходят в HTCAPTION (не кликабельны).
//! Поэтому вкладки/тема/кнопки окна НЕ дети drag-зоны.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::base::StyledExt as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, IntoElement, InteractiveElement as _, ParentElement, Pixels,
    RenderOnce, SharedString, Styled, StyleRefinement, Window, WindowControlArea, div, px,
};

/// Обработчик клика шапки (вкладка/тема): в хосте передаётся `cx.listener(...)`.
type ClickHandler = Rc<Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>>;

/// Вкладка шапки: `AppTab::new("tab_main", "Главная").active(true).on_click(cx.listener(...))`.
pub struct AppTab {
    id: &'static str,
    label: &'static str,
    active: bool,
    on_click: Option<ClickHandler>,
}

impl AppTab {
    pub fn new(id: &'static str, label: &'static str) -> Self {
        Self {
            id,
            label,
            active: false,
            on_click: None,
        }
    }

    /// Вкладка активна — кнопка заливается primary (синяя пилюля).
    pub fn active(mut self) -> Self {
        self.active = true;
        self
    }

    pub fn on_click(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(Box::new(f)));
        self
    }
}

/// Каркас окна: `AppShell::new("Заголовок").tab(...).theme_toggle(...).child(контент)`.
///
/// Тема задается хостом через [`crate::setup_theme`] (первая строка старта окна
/// и в обработчике ☀/🌙) — AppShell красит себя только из `cx.theme()`.
#[derive(IntoElement)]
pub struct AppShell {
    style: StyleRefinement,
    title: SharedString,
    tabs: Vec<AppTab>,
    theme_label: Option<SharedString>,
    on_theme_toggle: Option<ClickHandler>,
    window_buttons: bool,
    max_content_w: Option<Pixels>,
    children: Vec<AnyElement>,
}

impl AppShell {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            style: StyleRefinement::default(),
            title: title.into(),
            tabs: Vec::new(),
            theme_label: None,
            on_theme_toggle: None,
            window_buttons: true,
            max_content_w: None,
            children: Vec::new(),
        }
    }

    /// Вкладка шапки (в порядке добавления).
    pub fn tab(mut self, tab: AppTab) -> Self {
        self.tabs.push(tab);
        self
    }

    /// Переключатель темы: подпись подсказывает СЛЕДУЮЩУЮ тему
    /// (`"☀ Светлая"` в тёмном режиме, `"🌙 Тёмная"` в светлом).
    pub fn theme_toggle(
        mut self,
        label: impl Into<SharedString>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.theme_label = Some(label.into());
        self.on_theme_toggle = Some(Rc::new(Box::new(on_click)));
        self
    }

    /// Скрыть кнопки окна (по умолчанию рисуются).
    pub fn no_window_buttons(mut self) -> Self {
        self.window_buttons = false;
        self
    }

    /// Ограничение ширины контента (страницы настроек — `px(920.)`).
    pub fn max_w(mut self, max_w: Pixels) -> Self {
        self.max_content_w = Some(max_w);
        self
    }
}

impl Styled for AppShell {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for AppShell {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for AppShell {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Вкладки-кнопки: активная залита primary, неактивные ghost.
        let mut tabs = h_flex().gap_2().items_center();
        for tab in self.tabs {
            let mut btn = Button::new(tab.id).label(tab.label);
            btn = if tab.active { btn.primary() } else { btn.ghost() };
            if let Some(on_click) = tab.on_click {
                btn = btn.on_click(move |ev, window, cx| on_click(ev, window, cx));
            }
            tabs = tabs.child(btn);
        }

        // Пустая зона — таскание окна: drag-hitbox только здесь, дети шапки
        // вне её и остаются кликабельными (урок WCA::Drag).
        let drag = div()
            .id("appshell_drag")
            .flex_1()
            .h_full()
            .window_control_area(WindowControlArea::Drag);

        let mut header = div()
            .flex()
            .flex_row()
            .items_center()
            .h(px(36.0))
            .bg(cx.theme().title_bar)
            .border_b_1()
            .border_color(cx.theme().title_bar_border)
            .pl(px(12.0))
            .refine_style(&self.style)
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(div().text_color(cx.theme().muted_foreground).child(self.title))
                    .child(tabs),
            )
            .child(drag);

        if let Some(label) = self.theme_label {
            let mut toggle = Button::new("appshell_theme_toggle").label(label).ghost();
            if let Some(on_click) = self.on_theme_toggle {
                toggle = toggle.on_click(move |ev, window, cx| on_click(ev, window, cx));
            }
            header = header.child(toggle);
        }

        if self.window_buttons {
            // Кнопки окна (библиотечные WindowControls приватны): паритет egui.
            header = header
                .child(
                    Button::new("appshell_win_minimize")
                        .label("─")
                        .ghost()
                        .on_click(|_, window, _| window.minimize_window()),
                )
                .child(
                    Button::new("appshell_win_maximize")
                        .label("❐")
                        .ghost()
                        .on_click(|_, window, _| window.zoom_window()),
                )
                .child(
                    Button::new("appshell_win_close")
                        .label("✕")
                        .ghost()
                        .on_click(|_, window, _| window.remove_window()),
                );
        }

        // Контент: карточки/формы вмещает сам хост; ограничение ширины —
        // опция (страницы настроек не растягиваются на 1500px).
        let mut content = div()
            .flex()
            .flex_col()
            .flex_1()
            .overflow_hidden()
            .children(self.children);
        if let Some(max_w) = self.max_content_w {
            content = content
                .max_w(max_w)
                .mx_auto()
                .w_full();
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            // Рамка окна: системный кант Windows тёмный и в тёмной теме не
            // спасает от слияния с тёмными фонами других программ — рисуем
            // свою hairline из темы (детерминированно в обеих темах).
            .border_1()
            .border_color(cx.theme().border)
            .child(header)
            .child(content)
    }
}

/// WindowOptions для окна с AppShell: прозрачный системный тайтл-бар (без
/// белой полосы), drag и кнопки окна рисует AppShell.
pub fn window_options() -> gpui_kit::WindowOptions {
    gpui_kit::component::TitleBar::window_options()
}
