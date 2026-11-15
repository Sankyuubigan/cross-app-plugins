//! Тема кита: `setup_theme(mode, cx)` — одна строка для всего окна.
//!
//! ВАЖНО (урок VibeBridge): `Theme::change(mode)` сбрасывает colors на дефолт
//! registered-темы (apply_config), поэтому кастомная палитра ставится СТРОГО
//! после change — через `Theme::update` (не трогает mode, reconcile синхронизирует
//! semantic-токены, смена font_family пере-резолвит шрифты окна).

use gpui_kit::component::theme::{Theme, ThemeMode};
use gpui_kit::{App, SharedString, px};

use crate::palette;

/// Шрифты (план спеца: Segoe UI 14px; mono — Consolas, паритет egui).
const FONT_UI: &str = "Segoe UI";
const FONT_MONO: &str = "Consolas";
const FONT_SIZE: f32 = 14.0;

/// Тема окна одной строкой: change + палитра кита поверх. Тёмная — SSOT-палитра
/// egui visuals; светлая — GitHub-light-аналог (черновик на утверждении).
/// Вызывать при старте окна и при переключении ☀/🌙.
pub fn setup_theme(mode: ThemeMode, cx: &mut App) {
    if mode.is_dark() {
        Theme::change(mode, None, cx);
        apply_dark_palette(cx);
        log::info!("my-gpui-kit: тема — тёмная палитра 4 слоёв (SSOT egui visuals)");
    } else {
        Theme::change(mode, None, cx);
        apply_light_palette(cx);
        log::info!("my-gpui-kit: тема — светлая палитра (GitHub-light, черновик)");
    }
}

/// Тёмная палитра: слои (окно → карточка → поле), акценты, шрифт. Только после
/// `Theme::change(…, Dark)`.
fn apply_dark_palette(cx: &mut App) {
    Theme::update(cx, |t| {
        // Слои фона: окно → карточка (secondary/popover) → поле (input/list).
        t.colors.background = palette::c(palette::WINDOW_BG);
        t.colors.secondary = palette::c(palette::CARD_BG);
        t.colors.secondary_foreground = palette::c(palette::TEXT);
        t.colors.secondary_hover = palette::c(palette::SEC_HOVER);
        t.colors.secondary_active = palette::c(palette::SEC_ACTIVE);
        t.colors.popover = palette::c(palette::CARD_BG);
        t.colors.popover_foreground = palette::c(palette::TEXT);
        t.colors.input = palette::c(palette::FIELD_BG);
        t.colors.list = palette::c(palette::FIELD_BG);
        t.colors.list_even = palette::c(palette::FIELD_BG);
        t.colors.list_hover = palette::c(palette::SEC_BG);
        t.colors.list_head = palette::c(palette::CARD_BG);
        t.colors.list_active = palette::c(palette::SEC_ACTIVE);
        t.colors.list_active_border = palette::c(palette::PRIMARY);

        // Текст и рамки.
        t.colors.foreground = palette::c(palette::TEXT);
        t.colors.muted = palette::c(palette::FAINT);
        t.colors.muted_foreground = palette::c(palette::MUTED);
        t.colors.border = palette::c(palette::BORDER);
        t.colors.window_border = palette::c(palette::BORDER);

        // Дефолтные и вторичные кнопки (кнопка без варианта — как egui secondary).
        t.colors.button = palette::c(palette::SEC_BG);
        t.colors.button_foreground = palette::c(palette::TEXT);
        t.colors.button_hover = palette::c(palette::SEC_HOVER);
        t.colors.button_active = palette::c(palette::SEC_ACTIVE);
        t.colors.button_secondary = palette::c(palette::SEC_BG);
        t.colors.button_secondary_foreground = palette::c(palette::TEXT);
        t.colors.button_secondary_hover = palette::c(palette::SEC_HOVER);
        t.colors.button_secondary_active = palette::c(palette::SEC_ACTIVE);

        // Акценты: primary синий (главные действия, активная вкладка),
        // success зелёный (сохранение/применение).
        t.colors.primary = palette::c(palette::PRIMARY);
        t.colors.primary_foreground = palette::c(palette::ON_ACCENT);
        t.colors.primary_hover = palette::c(palette::PRIMARY_HOVER);
        t.colors.primary_active = palette::c(palette::PRIMARY);
        t.colors.button_primary = palette::c(palette::PRIMARY);
        t.colors.button_primary_foreground = palette::c(palette::ON_ACCENT);
        t.colors.button_primary_hover = palette::c(palette::PRIMARY_HOVER);
        t.colors.button_primary_active = palette::c(palette::PRIMARY);
        t.colors.success = palette::c(palette::SUCCESS);
        t.colors.success_foreground = palette::c(palette::ON_ACCENT);
        t.colors.success_hover = palette::c(palette::SUCCESS_HOVER);
        t.colors.success_active = palette::c(palette::SUCCESS);
        t.colors.button_success = palette::c(palette::SUCCESS);
        t.colors.button_success_foreground = palette::c(palette::ON_ACCENT);
        t.colors.button_success_hover = palette::c(palette::SUCCESS_HOVER);
        t.colors.button_success_active = palette::c(palette::SUCCESS);
        t.colors.accent = palette::c(palette::PRIMARY);
        t.colors.accent_foreground = palette::c(palette::ON_ACCENT);
        t.colors.selection = palette::c(palette::PRIMARY);
        t.colors.ring = palette::c(palette::PRIMARY);
        t.colors.link = palette::c(palette::PRIMARY);
        t.colors.link_hover = palette::c(palette::PRIMARY_HOVER);
        t.colors.link_active = palette::c(palette::PRIMARY);

        // Шапка/вкладки/скроллбар.
        t.colors.title_bar = palette::c(palette::WINDOW_BG);
        t.colors.title_bar_border = palette::c(palette::BORDER);
        t.colors.tab_bar = palette::c(palette::WINDOW_BG);
        t.colors.tab = palette::c(palette::WINDOW_BG);
        t.colors.tab_active = palette::c(palette::CARD_BG);
        t.colors.tab_foreground = palette::c(palette::TEXT);
        t.colors.tab_active_foreground = palette::c(palette::TEXT);
        t.colors.scrollbar = palette::c(palette::FIELD_BG);
        t.colors.scrollbar_thumb = palette::c(palette::SCROLL_THUMB);
        t.colors.scrollbar_thumb_hover = palette::c(palette::SCROLL_THUMB_HOVER);

        apply_fonts(t);
    });
}

/// Светлая палитра (ЧЕРНОВИК — hex на утверждении владельца): та же структура
/// слоёв, GitHub-light-аналог. Только после `Theme::change(…, Light)`.
fn apply_light_palette(cx: &mut App) {
    Theme::update(cx, |t| {
        t.colors.background = palette::c(palette::L_WINDOW_BG);
        t.colors.secondary = palette::c(palette::L_CARD_BG);
        t.colors.secondary_foreground = palette::c(palette::L_TEXT);
        t.colors.secondary_hover = palette::c(palette::L_SEC_HOVER);
        t.colors.secondary_active = palette::c(palette::L_SEC_ACTIVE);
        t.colors.popover = palette::c(palette::L_CARD_BG);
        t.colors.popover_foreground = palette::c(palette::L_TEXT);
        t.colors.input = palette::c(palette::L_FIELD_BG);
        t.colors.list = palette::c(palette::L_FIELD_BG);
        t.colors.list_even = palette::c(palette::L_FIELD_BG);
        t.colors.list_hover = palette::c(palette::L_SEC_BG);
        t.colors.list_head = palette::c(palette::L_CARD_BG);
        t.colors.list_active = palette::c(palette::L_SEC_ACTIVE);
        t.colors.list_active_border = palette::c(palette::L_PRIMARY);

        t.colors.foreground = palette::c(palette::L_TEXT);
        t.colors.muted = palette::c(palette::L_FAINT);
        t.colors.muted_foreground = palette::c(palette::L_MUTED);
        t.colors.border = palette::c(palette::L_BORDER);
        t.colors.window_border = palette::c(palette::L_BORDER);

        t.colors.button = palette::c(palette::L_SEC_BG);
        t.colors.button_foreground = palette::c(palette::L_TEXT);
        t.colors.button_hover = palette::c(palette::L_SEC_HOVER);
        t.colors.button_active = palette::c(palette::L_SEC_ACTIVE);
        t.colors.button_secondary = palette::c(palette::L_SEC_BG);
        t.colors.button_secondary_foreground = palette::c(palette::L_TEXT);
        t.colors.button_secondary_hover = palette::c(palette::L_SEC_HOVER);
        t.colors.button_secondary_active = palette::c(palette::L_SEC_ACTIVE);

        t.colors.primary = palette::c(palette::L_PRIMARY);
        t.colors.primary_foreground = palette::c(palette::ON_ACCENT);
        t.colors.primary_hover = palette::c(palette::L_PRIMARY_HOVER);
        t.colors.primary_active = palette::c(palette::L_PRIMARY);
        t.colors.button_primary = palette::c(palette::L_PRIMARY);
        t.colors.button_primary_foreground = palette::c(palette::ON_ACCENT);
        t.colors.button_primary_hover = palette::c(palette::L_PRIMARY_HOVER);
        t.colors.button_primary_active = palette::c(palette::L_PRIMARY);
        t.colors.success = palette::c(palette::L_SUCCESS);
        t.colors.success_foreground = palette::c(palette::ON_ACCENT);
        t.colors.success_hover = palette::c(palette::L_SUCCESS_HOVER);
        t.colors.success_active = palette::c(palette::L_SUCCESS);
        t.colors.button_success = palette::c(palette::L_SUCCESS);
        t.colors.button_success_foreground = palette::c(palette::ON_ACCENT);
        t.colors.button_success_hover = palette::c(palette::L_SUCCESS_HOVER);
        t.colors.button_success_active = palette::c(palette::L_SUCCESS);
        t.colors.accent = palette::c(palette::L_PRIMARY);
        t.colors.accent_foreground = palette::c(palette::ON_ACCENT);
        t.colors.selection = palette::c(palette::L_PRIMARY);
        t.colors.ring = palette::c(palette::L_PRIMARY);
        t.colors.link = palette::c(palette::L_PRIMARY);
        t.colors.link_hover = palette::c(palette::L_PRIMARY_HOVER);
        t.colors.link_active = palette::c(palette::L_PRIMARY);

        t.colors.title_bar = palette::c(palette::L_WINDOW_BG);
        t.colors.title_bar_border = palette::c(palette::L_BORDER);
        t.colors.tab_bar = palette::c(palette::L_WINDOW_BG);
        t.colors.tab = palette::c(palette::L_WINDOW_BG);
        t.colors.tab_active = palette::c(palette::L_CARD_BG);
        t.colors.tab_foreground = palette::c(palette::L_TEXT);
        t.colors.tab_active_foreground = palette::c(palette::L_TEXT);
        t.colors.scrollbar = palette::c(palette::L_FIELD_BG);
        t.colors.scrollbar_thumb = palette::c(palette::L_SCROLL_THUMB);
        t.colors.scrollbar_thumb_hover = palette::c(palette::L_SCROLL_THUMB_HOVER);

        apply_fonts(t);
    });
}

/// Шрифты: edit детектит смену font_family и сам пере-резолвит дефолтные
/// шрифты окна (resolve_default_font).
fn apply_fonts(t: &mut gpui_kit::component::theme::Theme) {
    t.font_family = SharedString::from(FONT_UI);
    t.font_size = px(FONT_SIZE);
    t.mono_font_family = SharedString::from(FONT_MONO);
    t.mono_font_size = px(FONT_SIZE);
}
