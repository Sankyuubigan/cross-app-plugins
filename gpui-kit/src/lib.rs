//! my-gpui-kit — общий GPUI UI-кит владельца: SSOT темы/каркаса/панелей для
//! всех GPUI-проектов (PRP 261115_my_gpui_kit.md).
//!
//! Кит называется **gpui-kit** (папка `cross-app-plugins/gpui-kit`; рядом с крейтом
//! лежат правила `AI_UI_RULES.md` и доки `from_Tauri_to_GPUI.md`). Cargo-пакет
//! вынужденно `my-gpui-kit`: имя `gpui-kit` на crates.io занято крейтом longbridge,
//! от которого кит зависит — одноимённый пакет Cargo не разрешает.
//!
//! Состав (фичи — чтобы хост тянул только нужное):
//! - default: тема [`setup_theme`] + SSOT-палитра [`palette`] (Шаг 1);
//! - `"log-collector"`: LogsPanel (Шаг 3); `"llama"`: LlamaSettingsCard (Шаг 3);
//!   `"webview"`: LegacyWebViewHost (Шаг 4) — добавляются по мере реализации.
//!
//! Ключевое правило темы (урок VibeBridge): `Theme::change` сбрасывает colors
//! на дефолт registered-темы — палитра ставится ТОЛЬКО после change через
//! `Theme::update`. Всё это внутри [`setup_theme`]; хостам вручную не делать.

pub mod card;
pub mod checkbox;
pub mod palette;
pub mod shell;
pub mod theme;

// Диагностический замер драг-цикла окна (Шаг 0 спеца, фриз при
// перетаскивании): без VIBEBRIDGE_DRAG_PROBE=1 в окружении не делает ничего.
pub mod drag_probe;

// Фикс фриза при перетаскивании (F2, по цифрам замера): timeBeginPeriod(1)
// на время модального move/size-цикла — всегда активен, без env-гейта.
pub mod drag_fix;

#[cfg(feature = "log-collector")]
pub mod logs_panel;
#[cfg(feature = "llama")]
pub mod llama_settings;
#[cfg(feature = "webview")]
pub mod webview_host;
#[cfg(feature = "about-updates")]
pub mod about_updates_card;

#[cfg(feature = "log-collector")]
pub use logs_panel::LogsPanel;
#[cfg(feature = "llama")]
pub use llama_settings::{LlamaModelItem, LlamaSettingsCard};
#[cfg(feature = "webview")]
pub use webview_host::LegacyWebViewHost;
#[cfg(feature = "about-updates")]
pub use about_updates_card::{
    progress_line, watch_updates, AboutUpdatesCard, AboutUpdatesConfig, UpdatePhase,
    UpdatesSnapshot,
};

pub use card::AppCard;
pub use checkbox::Checkbox;
pub use gpui_kit::component::theme::ThemeMode;
pub use shell::{AppShell, AppTab, window_options};
pub use theme::setup_theme;

// Хосты НЕ зависят от gpui-kit напрямую (он транзитивен): примитивы GPUI и
// компонентный слой доступны через кит — my_gpui_kit::{div, px, Context, …},
// my_gpui_kit::component::{ActiveTheme, button::Button, …}, my_gpui_kit::assets.
pub use gpui_kit;
pub use gpui_kit::*;
pub use gpui_kit::component;
pub use gpui_kit::assets;
