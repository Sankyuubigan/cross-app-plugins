//! 🪟 Таури-фасад window-icon-core — первый таури-специфичный модуль репы.
//!
//! Обход пробела tauri/tao: кнопка панели задач Windows запрашивает иконки
//! окна (WM_GETICON) один раз — при создании кнопки — и больше не перечитывает.
//! Единственный путь иконки в tauri — tao `set_window_icon` — захардкожен на
//! `IconType::Small` (tao-0.34.8 window.rs:882-891); `IconType::Big` в tao
//! существует, но никем не вызывается, WM_SETICON в tauri отсутствует даже в
//! 2.12.5. ICON_BIG остаётся NULL → кнопка рисует фолбэк Explorer —
//! shell-иконку по пути exe из кэша, который в дев-сборке протухает при
//! каждой пересборке (диагностика king_orch_3 06.11.26). У GPUI этого пробела
//! нет (class icon из ресурсов своего exe при каждом запуске), поэтому плагин
//! нужен только таури-хостам — что и отражено в имени (решение владельца
//! 23.11.2026: все прочие плагины репы универсальны по фронтендам).
//!
//! Почему хук `on_window_ready`, а не setup-хук (доказано по исходникам
//! tauri 2.10.3): окна из конфига создаются в НАЧАЛЕ setup() (app.rs:2370-2375,
//! RuntimeRunEvent::Ready), плагины инитятся раньше (build(), app.rs:2289);
//! attach_window дёргает window_created → on_window_ready синхронно на главном
//! потоке в момент создания окна (tauri-runtime-wry lib.rs:234-254
//! send_user_message) — т.е. ДО пользовательского .setup, где хост делает
//! show(), и ДО создания кнопки панели задач. WM_SETICON уходит окну, пока оно
//! скрыто — к моменту первого WM_GETICON обе иконки уже стоят.
//!
//! Соглашение с хостами: окно создаётся скрытым (`visible: false`), показ —
//! когда угодно после setup.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

/// Ставит окну ICON_SMALL (заголовок) + ICON_BIG (кнопка панели задач) из
/// ресурсов exe в момент создания окна. Отказ не фатален: warn в лог, окно
/// показывается как раньше (деградация = прежнее поведение таури).
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("window-icon")
        .on_window_ready(|window| {
            let icon_result = window
                .hwnd()
                .map(|hwnd| hwnd.0 as isize)
                .map_err(|e| format!("hwnd не получен: {e}"))
                .and_then(window_icon_core::apply_window_icons);
            if let Err(e) = icon_result {
                log::warn!(
                    "[WINDOW_ICON] плагин: иконки не выставлены (окно показывается как раньше): {e}"
                );
            }
        })
        .build()
}
