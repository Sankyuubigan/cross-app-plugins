//! Контрастный `Checkbox` кита (ССОТ) — заменяет `gpui_kit::component::checkbox::Checkbox`
//! там, где стоковый не годится.
//!
//! Зачем: стоковый чекбокс (gpui-component) жёстко рисует рамку НЕОТМЕЧЕННОГО
//! индикатора цветом `theme().input` — тем же токеном, что «утопленный» фон
//! полей/списков/деревьев (`bg(input)` по AI_UI_RULES §2.4). На таком фоне рамка
//! сливается (баг тёмной темы). Отдельного токена «рамка чекбокса» в теме нет,
//! а менять `input` глобально нельзя — поедут фоны всех полей.
//!
//! Решение: рамка из `muted_foreground` (свёрнут) / `primary` (отмечен) —
//! контрастирует на любом слое, в обеих темах (значения задаёт `setup_theme`).
//! Клик не «всплывает» в родительскую строку (`stop_propagation`).

use std::rc::Rc;

use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

/// Контрастный чекбокс: `Checkbox::new(id).checked(b).on_click(|new, w, cx| …)`.
/// Лейбл опционален (`.label("…")`).
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    checked: bool,
    label: Option<SharedString>,
    on_click: Option<Rc<dyn Fn(&bool, &mut Window, &mut App) + 'static>>,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self { id: id.into(), checked: false, label: None, on_click: None }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Обработчик клика; получает ЗАПРОШЕННОЕ состояние (¬текущее). Контролируемый
    /// компонент: владелец пишет значение и зовёт `cx.notify()`.
    pub fn on_click(self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        Self { on_click: Some(Rc::new(handler)), ..self }
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let checked = self.checked;
        let border = if checked {
            cx.theme().primary
        } else {
            cx.theme().muted_foreground
        };
        let mut bx = div()
            .id(self.id)
            .w(px(16.0))
            .h(px(16.0))
            .flex_shrink_0()
            .rounded(px(3.0))
            .border_1()
            .border_color(border)
            .flex()
            .items_center()
            .justify_center()
            .when(checked, |d| d.bg(cx.theme().primary));
        if checked {
            bx = bx.child(
                div()
                    .text_color(cx.theme().primary_foreground)
                    .child(SharedString::from("✓")),
            );
        }
        let on_click = self.on_click.clone();
        let box_el = bx.on_click(move |_ev, window, cx: &mut App| {
            // Клик по чекбоксу не должен тогглить кликабельную строку-родителя.
            cx.stop_propagation();
            if let Some(cb) = &on_click {
                cb(&!checked, window, cx);
            }
        });

        h_flex()
            .gap_2()
            .items_center()
            .child(box_el)
            .when_some(self.label, |this, label| this.child(div().child(label)))
    }
}