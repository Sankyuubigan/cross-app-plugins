//! AppCard — единая карточка кита: слой «secondary» (на ступень светлее окна),
//! рамка, скругление 6px, внутренний отступ p_3. Правило дизайна (:120):
//! карточки НЕ вкладывать в карточки — группа полей внутри карточки верстается
//! просто div'ами.

use gpui_kit::component::{ActiveTheme, v_flex};
use gpui_kit::base::StyledExt as _;
use gpui_kit::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, SharedString, Styled,
    StyleRefinement, Window, div, px,
};

/// Карточка кита: `AppCard::new().title("…").child(…)`.
///
/// Палитру и шрифты задаёт [`crate::setup_theme`] — карточка берёт цвета
/// только из `cx.theme()` (никаких hex внутри компонента).
#[derive(IntoElement)]
pub struct AppCard {
    style: StyleRefinement,
    title: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl AppCard {
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            title: None,
            children: Vec::new(),
        }
    }

    /// Заголовок карточки (опциональный): строка над содержимым.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }
}

impl Styled for AppCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for AppCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for AppCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut col = v_flex()
            .gap_2()
            .p_3()
            .bg(cx.theme().secondary)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(px(6.0))
            .refine_style(&self.style);

        if let Some(title) = self.title {
            col = col.child(div().text_color(cx.theme().foreground).child(title));
        }

        col.children(self.children)
    }
}
