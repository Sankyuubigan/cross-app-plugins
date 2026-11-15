//! Мост `installer`: прежняя сигнатура с AppHandle → ядро с `data_dir`.
//!
//! Хост (King Orch) вызывает `installer::run_install(&app, kind, version, url)` —
//! путь и сигнатура сохранены; после успешного запуска инсталлера фасад
//! закрывает приложение (`app.exit(0)`), ядро закрывать некому. Остальные
//! функции ([`download_installer`], [`installer_path`], план NSIS) — прямые
//! реэкспорты ядра, вызовы хостов не меняются.

pub use about_updates_core::installer::{
    download_installer, installer_file_name, installer_path, nsis_launch_plan, spawn_installer,
    LaunchPlan, NSIS_ARGS,
};
use about_updates_core::install_report::InstallKind;
use about_updates_core::installer as core;
use tauri::{AppHandle, Manager, Runtime};

/// Полный цикл установки релиза: скачать → зафиксировать намерение в отчёте →
/// запустить инсталлер → закрыть приложение (в отличие от ядра, у фасада есть
/// чем закрывать).
pub async fn run_install<R: Runtime>(
    app: &AppHandle<R>,
    kind: InstallKind,
    version: &str,
    download_url: &str,
) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| {
        log::error!("[about-updates] нет app_data_dir для установки: {}", e);
        format!("Не удалось определить каталог данных приложения: {}", e)
    })?;
    let previous_version = app.package_info().version.to_string();
    let pubkey = app
        .try_state::<crate::PluginState>()
        .map(|s| s.pubkey.clone())
        .unwrap_or(None);

    let opts = core::InstallOptions {
        data_dir: &data_dir,
        previous_version: &previous_version,
        // Подпись приходит с кандидатом из check-цепочки (latest.json/GitHub);
        // команды плагина её не передают (прежнее поведение Fallback-канала).
        signature: None,
        pubkey: pubkey.as_deref(),
    };
    core::run_install(kind, version, download_url, &opts).await?;

    // Штатный выход: срабатывает RunEvent::ExitRequested — движки убиваются,
    // отчёты об ошибках флашатся, в лог попадает «Приложение закрыто».
    log::info!("[about-updates] приложение закрывается; перезапуск выполнит NSIS (/R)");
    app.exit(0);
    Ok(())
}
