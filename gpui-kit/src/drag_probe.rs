//! drag_probe — измерение драг-цикла окна (Шаг 0 диагностики, ответ спеца).
//!
//! Инструмент диагностики фриза при перетаскивании окна: сабклассом HWND
//! считает частоту и длительность обработки WM_MOVE / WM_TIMER(id=1) /
//! WM_PAINT / WM_MOVING во время модального move-цикла и пишет сводку в лог
//! по завершении драга. Ничего не меняет в поведении окна.
//!
//! Включается ТОЛЬКО переменной окружения `VIBEBRIDGE_DRAG_PROBE=1` — без неё
//! сабкласс не вешается и код не влияет на приложение. После прочтения цифр
//! судьба инструмента (удалить / оставить выключенным) — решение владельца.
//!
//! Сводка по одному драгу:
//! - длительность драга, тип (перемещение/ресайз);
//! - счётчики и частота (шт/с) целевых сообщений;
//! - avg/max время обработки каждого (мс) и суммарная занятость главного потока.

#[cfg(windows)]
use std::cell::RefCell;
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    WM_EXITSIZEMOVE, WM_MOVE, WM_MOVING, WM_NCDESTROY, WM_PAINT, WM_SIZING, WM_SYSCOMMAND,
    WM_TIMER,
};

/// SC_MOVE = 0xF010. Старшие 4 бита WPARAM Windows использует внутренне,
/// поэтому сравнение по маске 0xFFF0 — документированное поведение.
#[cfg(windows)]
const SC_MOVE: usize = 0xF010;

/// WM_TIMER с этим id gpui-pre-windows заводит на время модального
/// move/size-цикла (SIZE_MOVE_LOOP_TIMER_ID в его events.rs).
#[cfg(windows)]
const SIZE_MOVE_LOOP_TIMER_ID: usize = 1;

/// Идентификатор сабкласса (произвольный уникальный).
#[cfg(windows)]
const SUBCLASS_ID: usize = 0x6472_6167; // "drag"

/// Включить замер: вызвать из колбэка создания окна (`open_window`).
/// Без `VIBEBRIDGE_DRAG_PROBE=1` в окружении — ничего не делает.
pub fn install(window: &crate::Window) {
    #[cfg(windows)]
    install_windows(window);
    #[cfg(not(windows))]
    let _ = window;
}

#[cfg(windows)]
fn install_windows(window: &crate::Window) {
    if std::env::var_os("VIBEBRIDGE_DRAG_PROBE").is_none() {
        return;
    }
    // window_handle() у gpui::Window — инхерентный метод (AnyWindowHandle),
    // затеняющий трейт; трейт-метод raw-window-handle вызываем явно (UFCS).
    let handle = match HasWindowHandle::window_handle(window) {
        Ok(handle) => handle,
        Err(e) => {
            log::error!("drag-probe: дескриптор окна недоступен: {e}");
            return;
        }
    };
    let RawWindowHandle::Win32(w32) = handle.as_raw() else {
        log::warn!("drag-probe: окно не Win32 — замер не установлен");
        return;
    };
    let hwnd = HWND(w32.hwnd.get() as *mut std::ffi::c_void);
    // Состояние живёт в thread-local: install вызывается из колбэка создания
    // окна на главном потоке — там же обрабатываются все сообщения окна.
    let ok = unsafe { SetWindowSubclass(hwnd, Some(probe_proc), SUBCLASS_ID, 0) };
    if ok.as_bool() {
        log::info!("drag-probe: замер установлен (VIBEBRIDGE_DRAG_PROBE=1)");
    } else {
        log::error!(
            "drag-probe: SetWindowSubclass не удался: {}",
            std::io::Error::last_os_error()
        );
    }
}

#[cfg(windows)]
thread_local! {
    static STATE: RefCell<ProbeState> = RefCell::new(ProbeState::default());
}

#[cfg(windows)]
#[derive(Default)]
struct ProbeState {
    drag_started: Option<Instant>,
    resize_seen: bool,
    move_count: u64,
    moving_count: u64,
    timer_count: u64,
    paint_count: u64,
    move_total: Duration,
    move_max: Duration,
    timer_total: Duration,
    timer_max: Duration,
    paint_total: Duration,
    paint_max: Duration,
}

#[cfg(windows)]
impl ProbeState {
    fn account(&mut self, msg: u32, wparam: WPARAM, spent: Duration) {
        match msg {
            WM_MOVE => {
                self.move_count += 1;
                self.move_total += spent;
                self.move_max = self.move_max.max(spent);
            }
            WM_MOVING => self.moving_count += 1,
            WM_TIMER if wparam.0 == SIZE_MOVE_LOOP_TIMER_ID => {
                self.timer_count += 1;
                self.timer_total += spent;
                self.timer_max = self.timer_max.max(spent);
            }
            WM_PAINT => {
                self.paint_count += 1;
                self.paint_total += spent;
                self.paint_max = self.paint_max.max(spent);
            }
            _ => {}
        }
    }

    /// Сводка в лог (ровно один блок строк на драг) и сброс счётчиков.
    fn report_and_reset(&mut self) {
        let Some(started) = self.drag_started else { return };
        let secs = started.elapsed().as_secs_f64().max(0.001);
        let kind = if self.resize_seen {
            "перемещение/ресайз"
        } else {
            "перемещение"
        };
        let busy = (self.move_total + self.timer_total + self.paint_total).as_secs_f64();
        let share = ((busy / secs) * 100.0).min(999.0);
        log::info!(
            "drag-probe: драг {secs:.1}с ({kind}); занятость главного потока ≈{share:.0}% \
             (WM_MOVE+WM_TIMER+WM_PAINT суммарно {busy:.2}с)"
        );
        let per_sec = |n: u64| (n as f64 / secs).round();
        let stat = |n: u64, total: Duration, max: Duration| {
            if n == 0 {
                String::from("нет")
            } else {
                format!(
                    "{} шт ({}/с), avg {:.2}мс, max {:.2}мс",
                    n,
                    per_sec(n),
                    total.as_secs_f64() * 1000.0 / n as f64,
                    max.as_secs_f64() * 1000.0
                )
            }
        };
        log::info!(
            "drag-probe: WM_MOVE     — {}",
            stat(self.move_count, self.move_total, self.move_max)
        );
        log::info!(
            "drag-probe: WM_MOVING   — {}",
            stat(self.moving_count, Duration::ZERO, Duration::ZERO)
        );
        log::info!(
            "drag-probe: WM_TIMER(1) — {}",
            stat(self.timer_count, self.timer_total, self.timer_max)
        );
        log::info!(
            "drag-probe: WM_PAINT    — {}",
            stat(self.paint_count, self.paint_total, self.paint_max)
        );
        *self = ProbeState::default();
    }
}

/// Сабкласс-процедура: флаги драга до обработки, замер времени дефолтной
/// обработки целевых сообщений, сводка на WM_EXITSIZEMOVE.
///
/// Состояние НЕ держится заимствованным через DefSubclassProc: дефолтная
/// обработка может прокачивать вложенные сообщения, и повторный вход в
/// процедуру не должен упираться в borrow_mut.
#[cfg(windows)]
unsafe extern "system" fn probe_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _ref_data: usize,
) -> LRESULT {
    if msg == WM_SYSCOMMAND && (wparam.0 & 0xFFF0) == SC_MOVE {
        STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            state.drag_started = Some(Instant::now());
            state.resize_seen = false;
        });
    } else if msg == WM_SIZING {
        STATE.with(|cell| cell.borrow_mut().resize_seen = true);
    }

    // Замеряем только целевые сообщения: WM_MOVE / WM_MOVING /
    // WM_TIMER(id=1) / WM_PAINT.
    let measure = matches!(
        (msg, wparam.0),
        (WM_MOVE, _)
            | (WM_MOVING, _)
            | (WM_TIMER, SIZE_MOVE_LOOP_TIMER_ID)
            | (WM_PAINT, _)
    );
    let start = measure.then(Instant::now);
    let result = DefSubclassProc(hwnd, msg, wparam, lparam);
    if let Some(start) = start {
        STATE.with(|cell| {
            cell.borrow_mut().account(msg, wparam, start.elapsed());
        });
    }

    // Сводка — после учёта: WM_EXITSIZEMOVE завершает драг.
    if msg == WM_EXITSIZEMOVE {
        STATE.with(|cell| cell.borrow_mut().report_and_reset());
    }
    if msg == WM_NCDESTROY {
        // Аккуратное снятие сабкласса при уничтожении окна.
        let ok = unsafe { RemoveWindowSubclass(hwnd, Some(probe_proc), SUBCLASS_ID) };
        if !ok.as_bool() {
            log::error!(
                "drag-probe: RemoveWindowSubclass не удался: {}",
                std::io::Error::last_os_error()
            );
        }
    }
    result
}
