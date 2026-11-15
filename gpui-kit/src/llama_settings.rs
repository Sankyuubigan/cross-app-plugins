//! LlamaSettingsCard — карточка выбора модели из конфига llama-engine (фича
//! `llama`): Select/SelectState поверх `llama-engine-core::engine::config`.
//!
//! Паритет референса VibeBridge `gpui_ui/settings_view.rs`:
//! - список: [0] «Первая доступная» (value = пустая строка), далее модели
//!   конфига; отсутствующий на диске GGUF — метка «— файл не найден» + disabled;
//! - выбор приходит событием `SelectEvent::Confirm`; пустая строка = None
//!   (в конфиг хоста пишется None — «Первая доступная»).
//!
//! Контракт (PRP Шаг 3): хост создаёт entity через [`make_model_select`],
//! подключает [`wire_model_select`] (это и есть `on_pick(Option<String>)` —
//! Subscription надо СОХРАНИТЬ, правило скилла gpui-kit) и рендерит
//! [`LlamaSettingsCard`] со своей entity. Так стейт Select переживает кадры.

use gpui_kit::base::StyledExt as _;
use crate::AppCard;
use gpui_kit::component::searchable_list::SearchableListItem;
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme, IndexPath};
use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement as _, RenderOnce, SharedString,
    Styled, StyleRefinement, Subscription, Window, div, px,
};
use llama_engine_core::engine::config::EngineConfig;
use std::path::Path;

/// Элемент выпадающего списка моделей: value — путь модели (пустая строка =
/// «Первая доступная», у хоста ai_model_path = None).
#[derive(Clone)]
pub struct LlamaModelItem {
    label: String,
    path: String,
    missing: bool,
}

impl SearchableListItem for LlamaModelItem {
    type Value = String;

    fn title(&self) -> SharedString {
        SharedString::from(self.label.clone())
    }

    fn value(&self) -> &String {
        &self.path
    }

    /// Отсутствующий на диске GGUF — серым, применить нельзя (паритет egui:
    /// «файл не найден → warn, не сохранять»).
    fn disabled(&self) -> bool {
        self.missing
    }
}

/// Имя файла из пути (для списка моделей).
fn file_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Список элементов Select: [0] «Первая доступная», далее модели конфига
/// движка (отсутствующие на диске — disabled с меткой).
pub fn build_model_items(engine: Option<&EngineConfig>) -> Vec<LlamaModelItem> {
    let mut items = vec![LlamaModelItem {
        label: "Первая доступная".into(),
        path: String::new(),
        missing: false,
    }];
    if let Some(engine) = engine {
        for m in &engine.models {
            let (label, missing) = if Path::new(m).is_file() {
                (file_label(m), false)
            } else {
                (format!("{} — файл не найден", file_label(m)), true)
            };
            items.push(LlamaModelItem { label, path: m.clone(), missing });
        }
    }
    items
}

/// IndexPath выбранного элемента Select (None в конфиге → «Первая доступная»;
/// неизвестный путь — фолбэк «Первая доступная»).
pub fn selected_model_index(items: &[LlamaModelItem], current: Option<&str>) -> Option<IndexPath> {
    let ix = match current {
        None => 0,
        Some(p) => items.iter().position(|i| i.path == p).unwrap_or(0),
    };
    Some(IndexPath::default().row(ix))
}

/// Создать entity Select для списка моделей (хост вызывает в своём new/setup;
/// `C` — `App` или `Context<T>` хоста). Дефолт конфига движка — если хост не
/// прочитал его заранее: `engine = Some(&load_config())` на стороне хоста.
pub fn make_model_select<C: AppContext>(
    engine: Option<&EngineConfig>,
    current: Option<&str>,
    window: &mut Window,
    cx: &mut C,
) -> Entity<SelectState<Vec<LlamaModelItem>>> {
    let items = build_model_items(engine);
    let selected = selected_model_index(&items, current);
    cx.new(|s_cx| SelectState::new(items, selected, window, s_cx))
}

/// Подписка на выбор модели: `on_pick(Option<String>)` — None = «Первая
/// доступная» (Confirm с пустым value маппится в None). Возвращённый
/// Subscription хост обязан хранить (правило скилла gpui-kit).
pub fn wire_model_select<T: 'static>(
    select: &Entity<SelectState<Vec<LlamaModelItem>>>,
    cx: &mut Context<T>,
    mut on_pick: impl FnMut(&mut T, Option<String>, &mut Context<T>) + 'static,
) -> Subscription {
    cx.subscribe(select, move |this, _, event: &SelectEvent<Vec<LlamaModelItem>>, cx| {
        match event {
            SelectEvent::Confirm(Some(v)) => {
                let picked = if v.is_empty() { None } else { Some(v.clone()) };
                on_pick(this, picked, cx);
            }
            SelectEvent::Confirm(None) => {}
        }
    })
}

/// Карточка «Модель для AI-подбора»: Select моделей (entity хоста) + опц.
/// примечание. ОДНА на окно (константный id `llama_model_select`).
#[derive(IntoElement)]
pub struct LlamaSettingsCard {
    select: Entity<SelectState<Vec<LlamaModelItem>>>,
    title: SharedString,
    note: Option<SharedString>,
    style: StyleRefinement,
}

impl LlamaSettingsCard {
    pub fn new(select: Entity<SelectState<Vec<LlamaModelItem>>>) -> Self {
        Self {
            select,
            title: "Модель для AI-подбора файлов".into(),
            note: None,
            style: StyleRefinement::default(),
        }
    }

    /// Заголовок карточки.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = title.into();
        self
    }

    /// Примечание под списком (паритет референса: описание, зачем модель).
    pub fn note(mut self, note: impl Into<SharedString>) -> Self {
        self.note = Some(note.into());
        self
    }
}

impl Styled for LlamaSettingsCard {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for LlamaSettingsCard {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Карточка кита (AppCard: secondary-слой, рамка, p_3) + Select + note.
        let mut card = AppCard::new()
            .title(self.title.clone())
            .refine_style(&self.style)
            .child(
                Select::new(&self.select)
                    .id("llama_model_select")
                    .menu_width(px(420.0)),
            );
        if let Some(note) = self.note {
            card = card.child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(note),
            );
        }
        card
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// build_items: [0] — «Первая доступная» с пустым value; без конфига —
    /// единственный элемент.
    #[test]
    fn model_items_first_entry_is_always_available() {
        let items = build_model_items(None);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "Первая доступная");
        assert_eq!(items[0].path, "");
        assert!(!items[0].missing);
    }

    /// Модель без файла на диске — disabled с меткой (паритет egui).
    #[test]
    fn model_items_marks_missing_files() {
        let engine = EngineConfig {
            models: vec![r"D:\точно\нет\missing.gguf".to_string()],
            ..Default::default()
        };
        let items = build_model_items(Some(&engine));
        assert_eq!(items.len(), 2);
        assert!(items[1].missing);
        assert!(items[1].disabled());
        assert!(items[1].label.contains("файл не найден"));
        assert_eq!(items[1].path, r"D:\точно\нет\missing.gguf");
    }

    /// IndexPath выбранного: None → 0 («Первая доступная»), Some → позиция
    /// модели; неизвестный путь — фолбэк «Первая доступная».
    #[test]
    fn selected_index_maps_config_to_row() {
        let items = vec![
            LlamaModelItem {
                label: "Первая доступная".into(),
                path: String::new(),
                missing: false,
            },
            LlamaModelItem { label: "a.gguf".into(), path: "D:/a.gguf".into(), missing: false },
        ];
        let row = |ix: Option<IndexPath>| ix.map(|p| p.row);
        assert_eq!(row(selected_model_index(&items, None)), Some(0));
        assert_eq!(row(selected_model_index(&items, Some("D:/a.gguf"))), Some(1));
        assert_eq!(row(selected_model_index(&items, Some("X:/b.gguf"))), Some(0));
    }
}
