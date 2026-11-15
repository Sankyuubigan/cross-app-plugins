//! Tauri-команды плагина (тонкий слой → about-updates-core).

use about_updates_core::install_report::InstallReport;
use about_updates_core::models::ReleaseInfo;
use tauri::{AppHandle, Manager, Runtime};

use crate::install_report;
use crate::installer;
use crate::updater_rollback;
use crate::PluginState;

/// Список доступных релизов через GitHub Releases REST API.
/// Репозиторий берётся из состояния плагина (задано в tauri.conf.json хоста).
#[tauri::command]
pub async fn get_release_history<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Vec<ReleaseInfo>, String> {
    let repo = app.state::<PluginState>().repo.clone();
    let current = app.package_info().version.to_string();
    about_updates_core::fetch_release_history(&repo, &current).await
}

/// Откат к конкретной версии.
///
/// Единственный источник правды — GitHub Releases. Фронтенд передаёт сюда реальный
/// URL установщика (`download_url`, полученный из GitHub API в `get_release_history`)
/// и версию, мы качаем ровно этот ассет и запускаем NSIS-инсталлер
/// (даунгрейд разрешён — `allowDowngrades` включён в `tauri.conf.json` хоста).
///
/// Запуск и перезапуск — на стороне инсталлера (`/P /UPDATE /R`, см. `installer.rs`),
/// поэтому здесь только бэкап данных, скачивание и выход приложения.
#[tauri::command]
pub async fn install_release<R: Runtime>(
    app: AppHandle<R>,
    download_url: String,
    version: String,
) -> Result<(), String> {
    log::info!("[about-updates] откат: запрос на установку {}", download_url);

    // 1. Бэкап данных перед понижением версии. Для отката это обязательно:
    //    после даунгрейда старая версия может не прочитать конфиг/сессии новее.
    updater_rollback::backup_before_rollback(&app).map_err(|e| {
        log::error!("[about-updates] бэкап перед откатом не удался: {}", e);
        format!("Не удалось создать бэкап перед откатом: {}", e)
    })?;

    // 2. Скачивание, отчёт, запуск инсталлера, выход приложения.
    installer::run_install(
        &app,
        install_report::InstallKind::Rollback,
        &version,
        &download_url,
    )
    .await
}

/// Отчёт о последней установке (откат/обновление) с вердиктом.
///
/// Вызывается фронтендом или при диагностике; попутно разрешает «висящий» отчёт,
/// если приложение было запущено вручную после неудачной установки.
#[tauri::command]
pub fn get_install_report<R: Runtime>(app: AppHandle<R>) -> Option<InstallReport> {
    install_report::resolve_pending(&app).or_else(|| install_report::read(&app))
}

/// Версия хост-приложения (из его Cargo.toml / tauri.conf.json).
#[tauri::command]
pub fn get_app_version<R: Runtime>(app: AppHandle<R>) -> String {
    app.package_info().version.to_string()
}

/// Настроенная ссылка «Поддержать автора» (или null, если не задана).
#[tauri::command]
pub fn get_support_url<R: Runtime>(app: AppHandle<R>) -> Option<String> {
    app.state::<PluginState>().support_url.clone()
}
