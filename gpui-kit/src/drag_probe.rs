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
//!
//! v2 (19.11.26, диагноз PAINT-шторма: тысячи WM_PAINT/с при драге):
//! - регион обновления на входе WM_PAINT (GetUpdateRect до обработки):
//!   пустой / ≤25% / >25% клиентской площади — отличает мусорные PAINT
//!   от полноценных кадров;
//! - интервалы между PAINT (avg/min/max) — vsync-ритм vs спам;
//! - распределение времени обработки (<1мс / 1-5мс / >5мс).

#[cfg(windows)]
use std::cell::RefCell;
#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
#[cfg(windows)]
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM};
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::GetUpdateRect;
#[cfg(windows)]
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, WM_EXITSIZEMOVE, WM_MOVE, WM_MOVING, WM_NCDESTROY, WM_PAINT, WM_SIZING,
    WM_SYSCOMMAND, WM_TIMER,
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
        log::info!("drag-probe: замер установлен v2 (VIBEBRIDGE_DRAG_PROBE=1)");
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
    // v2: диагностика PAINT-шторма.
    paint_empty: u64,
    paint_small: u64,
    paint_large: u64,
    paint_large_area_total: u64,
    paint_lt1ms: u64,
    paint_1to5ms: u64,
    paint_gt5ms: u64,
    last_paint: Option<Instant>,
    paint_interval_count: u64,
    paint_interval_total: Duration,
    paint_interval_max: Duration,
    paint_interval_min: Option<Duration>,
}

/// Классификация региона обновления на входе WM_PAINT (v2): пустой — PAINT
/// без реальной работы; Small — ≤25% клиентской площади (кусочная
/// инвалидация); Large — больше (полноценный кадр), с площадью в px².
#[cfg(windows)]
enum PaintRegion {
    Empty,
    Small,
    Large(u64),
}

/// Регион обновления окна ДО обработки WM_PAINT (пока не валидирован
/// BeginPaint внутри gpui-pre). Ошибка чтения — Empty + запись в лог
/// (ошибки не глотаются), клиентская площадь — для порога 25%.
#[cfg(windows)]
fn paint_region_info(hwnd: HWND) -> PaintRegion {
    let mut rect = RECT::default();
    let has_region = unsafe { GetUpdateRect(hwnd, Some(&mut rect), BOOL(0)) }.as_bool();
    if !has_region {
        return PaintRegion::Empty;
    }
    let area =
        (rect.right - rect.left).max(0) as u64 * (rect.bottom - rect.top).max(0) as u64;
    let mut client = RECT::default();
    let client_ok = unsafe { GetClientRect(hwnd, &mut client) }.is_ok();
    if !client_ok {
        log::error!(
            "drag-probe: GetClientRect не удался: {}",
            std::io::Error::last_os_error()
        );
        return PaintRegion::Large(area);
    }
    let client_area =
        (client.right - client.left).max(0) as u64 * (client.bottom - client.top).max(0) as u64;
    if client_area > 0 && area * 4 <= client_area {
        PaintRegion::Small
    } else {
        PaintRegion::Large(area)
    }
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
            _ => {}
        }
    }

    /// Учёт одного WM_PAINT с v2-диагностикой шторма: регион обновления,
    /// бакет времени обработки и интервал от предыдущего PAINT.
    fn record_paint(&mut self, spent: Duration, region: PaintRegion) {
        self.paint_count += 1;
        self.paint_total += spent;
        self.paint_max = self.paint_max.max(spent);
        match spent {
            s if s < Duration::from_millis(1) => self.paint_lt1ms += 1,
            s if s < Duration::from_millis(5) => self.paint_1to5ms += 1,
            _ => self.paint_gt5ms += 1,
        }
        match region {
            PaintRegion::Empty => self.paint_empty += 1,
            PaintRegion::Small => self.paint_small += 1,
            PaintRegion::Large(area) => {
                self.paint_large += 1;
                self.paint_large_area_total += area;
            }
        }
        if let Some(last) = self.last_paint {
            let interval = last.elapsed();
            self.paint_interval_count += 1;
            self.paint_interval_total += interval;
            self.paint_interval_max = self.paint_interval_max.max(interval);
            self.paint_interval_min =
                Some(self.paint_interval_min.map_or(interval, |m: Duration| m.min(interval)));
        }
        self.last_paint = Some(Instant::now());
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
        // v2: диагноз PAINT-шторма — регионы, бакеты времени, интервалы.
        log::info!(
            "drag-probe: WM_PAINT регионы — пустых {}, крошечных {} (≤25%), полных {} (avg {:.0}px²)",
            self.paint_empty,
            self.paint_small,
            self.paint_large,
            if self.paint_large > 0 {
                self.paint_large_area_total as f64 / self.paint_large as f64
            } else {
                0.0
            }
        );
        log::info!(
            "drag-probe: WM_PAINT время  — <1мс: {}, 1-5мс: {}, >5мс: {}",
            self.paint_lt1ms,
            self.paint_1to5ms,
            self.paint_gt5ms
        );
        if self.paint_interval_count > 0 {
            let n = self.paint_interval_count as f64;
            let avg = self.paint_interval_total.as_secs_f64() * 1000.0 / n;
            let max = self.paint_interval_max.as_secs_f64() * 1000.0;
            let min = self
                .paint_interval_min
                .map_or(0.0, |m| m.as_secs_f64() * 1000.0);
            log::info!(
                "drag-probe: WM_PAINT интервал — avg {avg:.2}мс, min {min:.2}мс, max {max:.2}мс"
            );
        }
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
    // v2: регион обновления читаем ДО DefSubclassProc — BeginPaint gpui-pre
    // валидирует его, после обработки пустой регион ничего не расскажет.
    let paint_region = (msg == WM_PAINT).then(|| paint_region_info(hwnd));
    let result = DefSubclassProc(hwnd, msg, wparam, lparam);
    if let Some(start) = start {
        if let Some(region) = paint_region {
            let spent = start.elapsed();
            STATE.with(|cell| {
                cell.borrow_mut().record_paint(spent, region);
            });
        } else {
            STATE.with(|cell| {
                cell.borrow_mut().account(msg, wparam, start.elapsed());
            });
        }
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
