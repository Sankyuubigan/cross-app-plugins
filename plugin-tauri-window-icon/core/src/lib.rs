//! 🪟 Ядро иконки панели задач: чистый win32, БЕЗ Tauri.
//!
//! Обход пробела tauri/tao (доказано по исходникам resolved-версий, 21.11.2026):
//! кнопка панели задач Windows запрашивает иконки окна (WM_GETICON) один раз —
//! в момент создания кнопки — и больше их не перечитывает. Единственный путь
//! иконки в tauri — tao `set_window_icon` — захардкожен на `IconType::Small`
//! (tao-0.34.8 window.rs:882-891), `IconType::Big` в tao существует, но никем
//! не вызывается; WM_SETICON в tauri отсутствует даже в 2.12.5. Итог: ICON_BIG
//! остаётся NULL, и кнопка рисует фолбэк Explorer — shell-иконку по пути exe
//! из кэша. В дев-сборке exe заменяется при каждой пересборке → кэш протухает
//! → плавающая заглушка (диагностика 06.11.26, king_orch_3
//! tasks/261104_2259-6ec77ce6/handoff.md).
//!
//! Решение: окно создаётся скрытым (`visible: false`), хост выставляет на него
//! обе иконки из ресурсов самого exe ДО показа — кнопка создаётся уже с нашей
//! иконкой, кэш Explorer больше ни при чём. У GPUI этого пробела нет (class
//! icon из ресурсов своего exe при каждом запуске, gpui-pre-windows 0.3.12
//! platform.rs:1686-1700), поэтому ядро нужно только таури-хостам; сам крейт —
//! чистый win32 и таури не тянет.
//!
//! HICON, переданный в WM_SETICON, принадлежит окну (система освободит при
//! уничтожении); хэндлы живут до смерти процесса — выставляются один раз.

#[cfg(windows)]
use windows_sys::Win32::Foundation::HWND;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PrivateExtractIconsW, SendMessageTimeoutW, HICON, ICON_BIG, ICON_SMALL, SMTO_ABORTIFHUNG,
    WM_SETICON,
};

/// Широкая (UTF-16) строка пути с нуль-терминатором — формат аргументов
/// win32-API. Вынесена отдельно, чтобы покрыть юнит-тестом (сами win32-вызовы
/// без живого окна и exe с иконками не тестируются — см. PRP, ЦИКЛ ВАЛИДАЦИИ
/// п.6). Эквивалент `OsStr::encode_wide` для путей `current_exe` (валидный
/// Unicode), но без платформенного импорта — компилируется везде.
pub fn to_wide(s: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    wide.push(0);
    wide
}

/// Выставить окну иконки SMALL (заголовок) и BIG (кнопка панели задач) из
/// ресурсов текущего exe. `hwnd` — дескриптор окна (`win.hwnd().0 as isize`).
/// Вызывается ДО `show()` окна: кнопка панели создаётся при показе, когда
/// ICON_BIG уже стоит. Ошибка не фатальна для старта — вызывающий пишет
/// `log::warn!` и всё равно показывает окно (деградация = прежнее поведение).
pub fn apply_window_icons(hwnd: isize) -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe()
            .map_err(|e| format!("current_exe не удался: {e}"))?
            .display()
            .to_string();
        log::info!("[WINDOW_ICON] hwnd={hwnd:#x} извлекаю иконки из {}", exe);

        // Широкая (UTF-16) строка пути с нуль-терминатором.
        let exe_w = to_wide(&exe);

        let small = extract_icon(&exe_w, 32)?;
        let big = extract_icon(&exe_w, 256)?;
        log::info!(
            "[WINDOW_ICON] извлечены HICON: SMALL 32x32 = {small:p}, BIG 256x256 = {big:p}"
        );

        send_set_icon(hwnd, ICON_SMALL, small, "SMALL")?;
        send_set_icon(hwnd, ICON_BIG, big, "BIG")?;
        log::info!("[WINDOW_ICON] WM_SETICON SMALL+BIG доставлены окну {hwnd:#x}");
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = hwnd; // не Windows: иконка задаётся средой, делать нечего
        Ok(())
    }
}

/// Извлечь из exe один HICON заданного размера (первая иконка-ресурс).
#[cfg(windows)]
fn extract_icon(exe_wide: &[u16], size: i32) -> Result<HICON, String> {
    let mut hicon: HICON = std::ptr::null_mut();
    let mut icon_id: u32 = 0;
    let n = unsafe {
        PrivateExtractIconsW(
            exe_wide.as_ptr(),
            0,
            size,
            size,
            &mut hicon,
            &mut icon_id,
            1,
            0,
        )
    };
    if n < 1 || hicon.is_null() {
        return Err(format!("PrivateExtractIconsW {size}x{size}: не извлекли (n={n})"));
    }
    Ok(hicon)
}

/// Доставить WM_SETICON окну с таймаутом (окно могло зависнуть — не ждём вечно).
#[cfg(windows)]
fn send_set_icon(hwnd: isize, kind: u32, hicon: HICON, label: &str) -> Result<(), String> {
    let mut result: usize = 0;
    let sent = unsafe {
        SendMessageTimeoutW(
            hwnd as HWND,
            WM_SETICON,
            kind as usize,
            hicon as isize,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        )
    };
    if sent == 0 {
        return Err(format!("WM_SETICON {label}: SendMessageTimeoutW не доставил"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::to_wide;

    #[test]
    fn to_wide_adds_terminator() {
        let w = to_wide("C:\\test\\app.exe");
        assert_eq!(w.last(), Some(&0));
        let body: Vec<u16> = "C:\\test\\app.exe".encode_utf16().collect();
        assert_eq!(w, [body.as_slice(), &[0]].concat());
    }

    #[test]
    fn to_wide_cyrillic_path() {
        let w = to_wide("D:\\проект\\ядро.exe");
        assert_eq!(w.last(), Some(&0));
        let body: Vec<u16> = "D:\\проект\\ядро.exe".encode_utf16().collect();
        assert_eq!(w, [body.as_slice(), &[0]].concat());
    }
}
