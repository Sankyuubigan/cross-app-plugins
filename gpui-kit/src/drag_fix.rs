//! drag_fix — плавное перетаскивание окна (фиксы F2+F3 по цифрам замеров
//! drag_probe).
//!
//! Механика подлагивания (см. сводки drag_probe): gpui-pre-windows на
//! WM_ENTERSIZEMOVE заводит SetTimer(USER_TIMER_MINIMUM = 10мс) и на каждый
//! WM_TIMER рисует ПОЛНЫЙ кадр; vsync-поток дополнительно инвалидирует окно
//! каждый vsync → WM_PAINT → тоже полный кадр. При ПЕРЕМЕЩЕНИИ содержимое
//! не меняется — все эти кадры рисуются впустую, а джиттер их выдачи
//! (паузы до 124мс, кадры 30-44мс — замеры 22.11) виден глазом.
//!
//! Фикс F2: timeBeginPeriod(1) на время цикла — ровная выдача таймера.
//! Фикс F3 (22.11.26): при перемещении (НЕ при ресайзе — там контент
//! реально меняется) WM_PAINT валидируется без рисования и WM_TIMER(id=1)
//! модального цикла гасится: окно двигает DWM готовым битмапом — та же
//! механика плавности, что у WebView2-окон. Обновления UI, произошедшие
//! во время драга, отрисуются после его завершения (незакрытый update-регион
//! и обычный рендер сохранены: PAINT валидируется осознанно — намерение
//! перерисоваться восстанавливают vsync-инвалиды).
//!
//! Сабкласс вешается на HWND при [`install`] — БЕЗ env-гейта: это фикс
//! поведения, а не диагностика. Для сверки «до/после» рядом остаётся
//! `drag_probe` (VIBEBRIDGE_DRAG_PROBE=1): при фиксе в драге PAINT ≈ 0.
//! По завершении цикла (WM_EXITSIZEMOVE) разрешение восстанавливается и
//! в лог пишутся счётчики погашенных сообщений.

#[cfg(windows)]
use std::cell::Cell;

#[cfg(windows)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::ValidateRect;
#[cfg(windows)]
use windows::Win32::Media::{timeBeginPeriod, timeEndPeriod};
#[cfg(windows)]
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_NCDESTROY, WM_PAINT, WM_SIZING, WM_TIMER,
};

/// Идентификатор сабкласса (произвольный уникальный; не пересекается с
/// drag_probe — "drag").
#[cfg(windows)]
const SUBCLASS_ID: usize = 0x6472_6766; // "drgf"

/// WM_TIMER с этим id gpui-pre-windows заводит на время модального
/// move/size-цикла (SIZE_MOVE_LOOP_TIMER_ID в его events.rs) и рисует
/// на каждый тик полный кадр.
#[cfg(windows)]
const SIZE_MOVE_LOOP_TIMER_ID: usize = 1;

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
        log::info!(
            "drag-fix: timeBeginPeriod(1) + глушение перерисовок при драге — установлен"
        );
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
    /// Внутри модального move/size-цикла прямо сейчас.
    static IN_DRAG: Cell<bool> = Cell::new(false);
    /// В текущем цикле был ресайз — контент меняется, гейты F3 отключаются.
    static RESIZE_SEEN: Cell<bool> = Cell::new(false);
    /// Счётчики погашенных сообщений текущего драга (вердикт в лог).
    static SUPPRESSED_PAINT: Cell<u64> = Cell::new(0);
    static SUPPRESSED_TIMER: Cell<u64> = Cell::new(0);
}

/// Сабкласс-процедура: F2 — поднять разрешение таймера на цикл; F3 — при
/// ПЕРЕМЕЩЕНИИ (без ресайза) гасить впустую полные кадры (WM_PAINT валидируется
/// без рисования, WM_TIMER(1) модального цикла не доходит до gpui) — окно
/// двигает DWM готовым битмапом. Всё остальное — всегда DefSubclassProc.
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
            IN_DRAG.set(true);
            RESIZE_SEEN.set(false);
            SUPPRESSED_PAINT.set(0);
            SUPPRESSED_TIMER.set(0);
            PERIOD_RAISED.with(|cell| {
                if !cell.get() {
                    // Результат (0 = успех) игнорируем осознанно: даже при
                    // отказе драг остаётся рабочим, просто без ускорения.
                    let _ = timeBeginPeriod(1);
                    cell.set(true);
                }
            });
        }
        WM_SIZING => {
            RESIZE_SEEN.set(true);
        }
        WM_PAINT if IN_DRAG.get() && !RESIZE_SEEN.get() => {
            // F3: валидируем update-регион без рисования — кадр при
            // перемещении впустую (контент не меняется). Намерение
            // перерисоваться восстанавливают vsync-инвалиды после драга.
            SUPPRESSED_PAINT.set(SUPPRESSED_PAINT.get() + 1);
            if !unsafe { ValidateRect(hwnd, None) }.as_bool() {
                // Не глотаем: невалидированный PAINT = бесконечный шторм.
                log::error!(
                    "drag-fix: ValidateRect не удался: {}",
                    std::io::Error::last_os_error()
                );
                return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            }
            return LRESULT(0);
        }
        WM_TIMER if IN_DRAG.get() && !RESIZE_SEEN.get() && wparam.0 == SIZE_MOVE_LOOP_TIMER_ID => {
            // F3: форс-кадр модального цикла gpui-pre гасим (см. выше).
            SUPPRESSED_TIMER.set(SUPPRESSED_TIMER.get() + 1);
            return LRESULT(0);
        }
        WM_EXITSIZEMOVE => {
            IN_DRAG.set(false);
            let (paint, timer) = (SUPPRESSED_PAINT.get(), SUPPRESSED_TIMER.get());
            log::info!("drag-fix: драг завершён — погашено PAINT {paint}, TIMER {timer}");
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
            IN_DRAG.set(false);
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
