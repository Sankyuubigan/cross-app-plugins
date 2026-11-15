//! SSOT палитры кита gpui-kit: 4 слоя глубины + акценты (hex-константы).
//!
//! ТЁМНАЯ — паритет egui `ui/style.rs::visuals` (VibeBridge, утверждена
//! владельцем 15.11.26). СВЕТЛАЯ — черновик GitHub-light-аналога, ждёт
//! утверждения владельца (PRP 261115 Шаг 1): править можно только hex,
//! структуру слоёв не менять.

use gpui_kit::Hsla;

/// Литерал hex → Hsla (ThemeColor хранит Hsla).
pub fn c(hex: u32) -> Hsla {
    gpui_kit::rgb(hex).into()
}

// — ТЁМНАЯ: слои (паритет egui visuals(dark=true)) —
pub const WINDOW_BG: u32 = 0x181A20; // фон окна
pub const CARD_BG: u32 = 0x1F222A; // карточки — СВЕТЛЕЕ фона (слой вверх)
pub const FIELD_BG: u32 = 0x121419; // поля/списки/дерево — утопленный слой
pub const BORDER: u32 = 0x323846; // рамки
pub const TEXT: u32 = 0xE6EDF3; // основной текст
pub const MUTED: u32 = 0x8B949E; // второстепенный текст
pub const FAINT: u32 = 0x16181E; // faint-плашки

// — ТЁМНАЯ: акценты (GitHub-палитра, план спеца 15.11.26) —
pub const PRIMARY: u32 = 0x1F6FEB; // синие кнопки/активная вкладка
pub const PRIMARY_HOVER: u32 = 0x388BFD;
pub const SUCCESS: u32 = 0x238636; // зелёные кнопки (сохранить/применить)
pub const SUCCESS_HOVER: u32 = 0x2EA043;
pub const ON_ACCENT: u32 = 0xFFFFFF; // текст на синих/зелёных кнопках

// — ТЁМНАЯ: вторичные кнопки / скроллбар (паритет egui) —
pub const SEC_BG: u32 = 0x282C34;
pub const SEC_HOVER: u32 = 0x32363F;
pub const SEC_ACTIVE: u32 = 0x3B4252;
pub const SCROLL_THUMB: u32 = 0x3B4252;
pub const SCROLL_THUMB_HOVER: u32 = 0x4C566A;

// — СВЕТЛАЯ (ЧЕРНОВИК — на утверждение владельца) —
pub const L_WINDOW_BG: u32 = 0xF6F8FA; // фон окна
pub const L_CARD_BG: u32 = 0xFFFFFF; // карточки — светлее фона
pub const L_FIELD_BG: u32 = 0xEEF1F4; // поля/списки — чуть темнее окна
pub const L_BORDER: u32 = 0xD0D7DE;
pub const L_TEXT: u32 = 0x1F2328;
pub const L_MUTED: u32 = 0x656D76;
pub const L_FAINT: u32 = 0xE7EBEF;
pub const L_PRIMARY: u32 = 0x1F6FEB; // синий тот же
pub const L_PRIMARY_HOVER: u32 = 0x1B63D3; // в светлой теме hover ТЕМНЕЕ
pub const L_SUCCESS: u32 = 0x1A7F37;
pub const L_SUCCESS_HOVER: u32 = 0x15702E;
pub const L_SEC_BG: u32 = 0xEAEEF2;
pub const L_SEC_HOVER: u32 = 0xDFE4E9;
pub const L_SEC_ACTIVE: u32 = 0xD5DBE1;
pub const L_SCROLL_THUMB: u32 = 0xC4CCD4;
pub const L_SCROLL_THUMB_HOVER: u32 = 0xB0BAC4;
