//! Мост `install_report`: прежние сигнатуры с AppHandle → ядро с `data_dir`.
//! Ядро: `about_updates_core::install_report` (чистое, для GPUI-хостов).
//!
//! Типы ([`InstallKind`], [`InstallReport`], [`InstallState`]) — реэкспорт из
//! ядра: King Orch импортирует `plugin_about_updates::install_report::InstallKind`,
//! путь сохранён.

pub use about_updates_core::install_report::{InstallKind, InstallReport, InstallState};
use about_updates_core::install_report as core;
use tauri::{AppHandle, Manager, Runtime};

use std::path::PathBuf;

fn data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| {
        log::error!("[about-updates] нет app_data_dir для отчёта об установке: {}", e);
        format!("Не удалось определить каталог данных приложения: {}", e)
    })
}

/// Путь к файлу отчёта (`app_data_dir`, тот же каталог, что и `rollback_backup`).
pub fn report_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    Ok(core::report_path(&data_dir(app)?))
}

/// Прочитать отчёт. Отсутствие или повреждение файла — не ошибка установки:
/// тихо отдаём `None` с предупреждением в лог (core rules §2.2 — молчать нельзя).
pub fn read<R: Runtime>(app: &AppHandle<R>) -> Option<InstallReport> {
    core::read(&data_dir(app).ok()?)
}

/// Зафиксировать намерение установить `version`. Вызывается до запуска инсталлера.
pub fn begin<R: Runtime>(
    app: &AppHandle<R>,
    kind: InstallKind,
    version: &str,
    installer: &std::path::Path,
) -> Result<(), String> {
    let dir = data_dir(app)?;
    let previous_version = app.package_info().version.to_string();
    core::begin(&dir, kind, version, &previous_version, installer)
}

/// Отметить, что установщик даже не запустился.
pub fn fail<R: Runtime>(app: &AppHandle<R>, detail: &str) {
    if let Ok(dir) = data_dir(app) {
        core::fail(&dir, detail);
    }
}

/// Сверить отчёт с фактически установленной версией и записать вердикт в лог.
///
/// Вызывается на старте приложения. Если отчёта нет или он уже не `Pending` —
/// ничего не делает. Иначе сверяет текущую версию с запрошенной:
/// совпало → `Done` + `info!`, не совпало → `Failed` + `error!`.
pub fn resolve_pending<R: Runtime>(app: &AppHandle<R>) -> Option<InstallReport> {
    let dir = data_dir(app).ok()?;
    let current = app.package_info().version.to_string();
    core::resolve_pending(&dir, &current)
}
