//! drag_fix — повышение разрешения системного таймера на время модального
//! move/size-цикла окна (фикс F2 по цифрам замера drag_probe).
//!
//! Механика фриза (см. сводки drag_probe): gpui-pre-windows на WM_ENTERSIZEMOVE
//! заводит SetTimer(USER_TIMER_MINIMUM = 10мс) и на каждый WM_TIMER рисует
//! полный кадр. Без timeBeginPeriod системное разрешение таймера 15.6мс →
//! WM_TIMER реально тикает ~16-19мс (замер: 29-60 вызовов/с) — потолок кадров
//! при драге. Guard диспетчера gpui (increase_timer_resolution,
//! platform_scheduler.rs:91) в модальном цикле не активен: он держится только
//! пока главный поток парится в park_deadline. timeBeginPeriod(1) на время
//! цикла поднимает частоту до ~10мс (~90-100/с) и выравнивает выдачу кадров.
//!
//! Сабкласс вешается на HWND при [`install`] — БЕЗ env-гейта: это фикс
//! поведения, а не диагностика. Для сверки «до/после» рядом остаётся
//! `drag_probe` (VIBEBRIDGE_DRAG_PROBE=1). По завершении цикла
//! (WM_EXITSIZEMOVE) разрешение восстанавливается — timeEndPeriod.

#[cfg(windows)]
use std::cell::Cell;

#[cfg(windows)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
#[cfg(windows)]
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_NCDESTROY,
};

/// Идентификатор сабкласса (произвольный уникальный; не пересекается с
/// drag_probe — "drag").
#[cfg(windows)]
const SUBCLASS_ID: usize = 0x6472_6766; // "drgf"

/// Включить фикс: вызвать из колбэка создания окна (`open_window`).
#[cfg(windows)]
pub fn install(window: &crate::Window) {
    // window_handle() у gpui::Window — инхерентный метод, затеняющий трейт;
    // трейт-метод raw-window-handle вызываем явно (UFCS). WindowHandle в rwh
    // 0.6 — инхерентный as_raw() (raw_window_handle() — deprecated-трейт).
    let handle = match HasWindowHandle::window_handle(window) {
        Ok(handle) => handle,
        Err(e) => {
            log::error!("drag-fix: дескриптор окна недоступен: {e}");
            return;
        }
    };
    let RawWindowHandle::Win32(w32) = handle.as_raw() else {
        log::warn!("drag-fix: окно не Win32 — фикс не установлен");
        return;
    };
    let hwnd = HWND(w32.hwnd.get() as *mut std::ffi::c_void);
    // Состояние (PERIOD_RAISED) живёт в thread-local: install вызывается из
    // колбэка создания окна на главном потоке — там же обрабатываются все
    // сообщения окна.
    let ok = unsafe { SetWindowSubclass(hwnd, Some(fix_proc), SUBCLASS_ID, 0) };
    if ok.as_bool() {
        log::info!("drag-fix: timeBeginPeriod(1) на время драга — установлен");
    } else {
        log::error!(
            "drag-fix: SetWindowSubclass не удался: {}",
            std::io::Error::last_os_error()
        );
    }
}

#[cfg(windows)]
thread_local! {
    /// Поднято ли разрешение прямо сейчас (пара Enter/Exit гарантирована
    /// системой, но флаг защищает от лишних timeBeginPeriod и гарантирует
    /// timeEndPeriod при уничтожении окна).
    static PERIOD_RAISED: Cell<bool> = Cell::new(false);
}

/// Сабкласс-процедура: на весь модальный move/size-цикл поднимает разрешение
/// системного таймера до 1мс, по завершении возвращает. Всё остальное —
/// без изменений, всегда DefSubclassProc.
#[cfg(windows)]
unsafe extern "system" fn fix_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _ref_data: usize,
) -> LRESULT {
    match msg {
        WM_ENTERSIZEMOVE => {
            PERIOD_RAISED.with(|cell| {
                if !cell.get() {
                    // Результат (0 = успех) игнорируем осознанно: даже при
                    // отказе драг остаётся рабочим, просто без ускорения.
                    let _ = timeBeginPeriod(1);
                    cell.set(true);
                }
            });
        }
        WM_EXITSIZEMOVE => {
            PERIOD_RAISED.with(|cell| {
                if cell.take() {
                    let _ = timeEndPeriod(1);
                }
            });
        }
        WM_NCDESTROY => {
            // Аккуратное снятие сабкласса и возврат разрешения при закрытии
            // окна во время драга (крайний случай).
            let ok = unsafe { RemoveWindowSubclass(hwnd, Some(fix_proc), SUBCLASS_ID) };
            if !ok.as_bool() {
                log::error!(
                    "drag-fix: RemoveWindowSubclass не удался: {}",
                    std::io::Error::last_os_error()
                );
            }
            PERIOD_RAISED.with(|cell| {
                if cell.take() {
                    let _ = timeEndPeriod(1);
                }
            });
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
