//! Отчёт о запуске установщика: переживает перезапуск процесса.
//!
//! После запуска NSIS приложение закрывается, и записать в лог «чем закончилось»
//! уже некому — процесс мёртв. Поэтому до запуска фиксируется намерение в
//! `<data_dir>/install_report.json`, а на старте следующей сессии оно сверяется
//! с **фактически установленной** версией (`current_version`, у Tauri —
//! `app.package_info().version`) и вердикт пишется в общий лог. Так «откат
//! сработал / не сработал» — проверяемый факт, а не надежда на флаги.
//!
//! Запись — только через `json-store` (PLUGIN_STANDARD §2): tmp → sync → rename.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Имя файла отчёта в каталоге данных приложения.
const REPORT_FILE: &str = "install_report.json";

/// Что инициировало установку.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallKind {
    /// Откат на предыдущий релиз.
    Rollback,
    /// Установка более нового релиза.
    Update,
}

impl InstallKind {
    pub fn as_str(self) -> &'static str {
        match self {
            InstallKind::Rollback => "откат",
            InstallKind::Update => "обновление",
        }
    }
}

/// Состояние отчёта.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallState {
    /// Установщик запущен, результат ещё неизвестен.
    Pending,
    /// Версия на месте — установка состоялась.
    Done,
    /// Установщик не отработал: версия не изменилась либо запуск не удался.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallReport {
    pub kind: InstallKind,
    /// Версия, которую устанавливали.
    pub target_version: String,
    /// Версия, которая работала до установки.
    pub previous_version: String,
    pub installer: String,
    pub installer_bytes: u64,
    /// Аргументы NSIS — чтобы по логу было видно, что именно запускалось.
    pub nsis_args: String,
    pub started_at: String,
    pub state: InstallState,
    #[serde(default)]
    pub finished_at: Option<String>,
    /// Фактическая версия на момент проверки.
    #[serde(default)]
    pub current_version: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

/// Путь к файлу отчёта (`data_dir`, тот же каталог, что и `rollback_backup`).
pub fn report_path(data_dir: &Path) -> PathBuf {
    data_dir.join(REPORT_FILE)
}

fn now_iso() -> String {
    chrono::Local::now().to_rfc3339()
}

fn write_report(data_dir: &Path, report: &InstallReport) -> Result<(), String> {
    let path = report_path(data_dir);
    let json = serde_json::to_string_pretty(report)
        .map_err(|e| format!("Не удалось сериализовать отчёт об установке: {}", e))?;
    json_store::write_atomic(&path, &json).map_err(|e| {
        log::error!("[about-updates] не удалось записать отчёт {}: {}", path.display(), e);
        format!("Не удалось записать отчёт об установке: {}", e)
    })
}

/// Прочитать отчёт. Отсутствие или повреждение файла — не ошибка установки:
/// тихо отдаём `None` с предупреждением в лог (core rules §2.2 — молчать нельзя).
pub fn read(data_dir: &Path) -> Option<InstallReport> {
    let path = report_path(data_dir);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            log::warn!("[about-updates] не удалось прочитать отчёт об установке: {}", e);
            return None;
        }
    };
    match serde_json::from_str::<InstallReport>(&content) {
        Ok(r) => Some(r),
        Err(e) => {
            log::warn!(
                "[about-updates] отчёт об установке {} не разобран ({}), игнорируем",
                path.display(),
                e
            );
            None
        }
    }
}

/// Зафиксировать намерение установить `version`. Вызывается до запуска инсталлера.
/// `previous_version` — версия, которая работает сейчас (у Tauri-фасада —
/// `app.package_info().version`).
pub fn begin(
    data_dir: &Path,
    kind: InstallKind,
    version: &str,
    previous_version: &str,
    installer: &Path,
) -> Result<(), String> {
    let installer_bytes = std::fs::metadata(installer).map(|m| m.len()).unwrap_or(0);
    // Аргументы берём из того же плана, по которому реально запускаем инсталлер,
    // чтобы в отчёте не могло оказаться не то, что было запущено.
    let nsis_args = crate::installer::nsis_launch_plan(installer)
        .args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    let report = InstallReport {
        kind,
        target_version: version.to_string(),
        previous_version: previous_version.to_string(),
        installer: installer.display().to_string(),
        installer_bytes,
        nsis_args,
        started_at: now_iso(),
        state: InstallState::Pending,
        finished_at: None,
        current_version: None,
        detail: None,
    };
    log::info!(
        "[about-updates] отчёт об установке: {} {} (было {}), установщик {} ({} байт), аргументы {}",
        kind.as_str(),
        report.target_version,
        report.previous_version,
        report.installer,
        report.installer_bytes,
        report.nsis_args
    );
    write_report(data_dir, &report)
}

/// Отметить, что установщик даже не запустился.
pub fn fail(data_dir: &Path, detail: &str) {
    let Some(mut report) = read(data_dir) else { return };
    report.state = InstallState::Failed;
    report.detail = Some(detail.to_string());
    report.finished_at = Some(now_iso());
    if let Err(e) = write_report(data_dir, &report) {
        log::error!("[about-updates] не удалось сохранить отчёт о неудаче: {}", e);
    }
}

/// Вердикт по факту: совпала фактически установленная версия с запрошенной —
/// установка состоялась, иначе установщик не отработал. Единственное место,
/// где выносится решение (тестируется без Tauri — см. `tests::verdict_*`).
fn decide(target: &str, current: &str) -> (InstallState, String) {
    if target == current {
        (
            InstallState::Done,
            format!("версия {} установлена", current),
        )
    } else {
        (
            InstallState::Failed,
            format!(
                "запрошена {}, работает {} — установщик не отработал",
                target, current
            ),
        )
    }
}

/// Сверить отчёт с фактически установленной версией и записать вердикт в лог.
///
/// Вызывается на старте приложения. Если отчёта нет или он уже не `Pending` —
/// ничего не делает. Иначе сверяет текущую версию с запрошенной:
/// совпало → `Done` + `info!`, не совпало → `Failed` + `error!`.
pub fn resolve_pending(data_dir: &Path, current_version: &str) -> Option<InstallReport> {
    let mut report = read(data_dir)?;
    if report.state != InstallState::Pending {
        return None;
    }

    let (state, detail) = decide(&report.target_version, current_version);
    report.state = state;
    report.detail = Some(detail);
    report.current_version = Some(current_version.to_string());
    report.finished_at = Some(now_iso());

    match state {
        InstallState::Done => log::info!(
            "[about-updates] {} завершён: {} (установщик {})",
            report.kind.as_str(),
            report.detail.as_deref().unwrap_or_default(),
            report.installer
        ),
        _ => log::error!(
            "[about-updates] {} НЕ завершён: {} (установщик {}, запуск {})",
            report.kind.as_str(),
            report.detail.as_deref().unwrap_or_default(),
            report.installer,
            report.started_at
        ),
    }

    if let Err(e) = write_report(data_dir, &report) {
        log::error!("[about-updates] не удалось сохранить вердикт по установке: {}", e);
    }
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(kind: InstallKind, target: &str, state: InstallState) -> InstallReport {
        InstallReport {
            kind,
            target_version: target.to_string(),
            previous_version: "26.9.224".to_string(),
            installer: r"C:\Temp\App_26.9.177_x64-setup.exe".to_string(),
            installer_bytes: 14_464_328,
            nsis_args: "/P /UPDATE /R".to_string(),
            started_at: "2026-10-01T00:04:40+03:00".to_string(),
            state,
            finished_at: None,
            current_version: None,
            detail: None,
        }
    }

    #[test]
    fn round_trip_json_keeps_all_fields() {
        let report = sample(InstallKind::Rollback, "26.9.177", InstallState::Pending);
        let json = serde_json::to_string(&report).expect("сериализация");
        let back: InstallReport = serde_json::from_str(&json).expect("десериализация");
        assert_eq!(back.kind, InstallKind::Rollback);
        assert_eq!(back.state, InstallState::Pending);
        assert_eq!(back.target_version, "26.9.177");
        assert_eq!(back.installer_bytes, 14_464_328);
        assert_eq!(back.nsis_args, "/P /UPDATE /R");
    }

    #[test]
    fn missing_optional_fields_are_tolerated() {
        let json = r#"{
            "kind": "update",
            "target_version": "26.9.230",
            "previous_version": "26.9.224",
            "installer": "C:/Temp/App.exe",
            "installer_bytes": 1,
            "nsis_args": "/P /UPDATE /R",
            "started_at": "2026-10-01T00:00:00+03:00",
            "state": "pending"
        }"#;
        let report: InstallReport = serde_json::from_str(json).expect("старый отчёт читается");
        assert_eq!(report.state, InstallState::Pending);
        assert!(report.finished_at.is_none());
        assert!(report.detail.is_none());
    }

    /// Итог установки определяется сравнением версий — единственный проверяемый
    /// критерий: доверять флагам NSIS нельзя, после выхода их некому проверить.
    #[test]
    fn verdict_is_done_only_when_installed_version_matches() {
        let (state, detail) = decide("26.9.177", "26.9.177");
        assert_eq!(state, InstallState::Done);
        assert!(detail.contains("26.9.177"));

        // Инсталлер не отработал: версия не сменилась.
        let (state, detail) = decide("26.9.177", "26.9.224");
        assert_eq!(state, InstallState::Failed);
        assert!(detail.contains("26.9.177"), "причина называет запрошенную версию: {}", detail);
        assert!(detail.contains("26.9.224"), "причина называет текущую версию: {}", detail);

        // Установилась другая версия (гонка с параллельной установкой) — тоже провал.
        assert_eq!(decide("26.9.177", "26.9.230").0, InstallState::Failed);
    }

    /// Провал всегда объяснён: пустой `detail` = ложь в UI/логе (core rules §2.2).
    #[test]
    fn verdict_never_leaves_detail_empty() {
        for (target, current) in [("26.9.177", "26.9.177"), ("26.9.177", "26.9.224")] {
            let (_, detail) = decide(target, current);
            assert!(!detail.trim().is_empty(), "detail не должен быть пустым");
        }
    }

    #[test]
    fn kind_serializes_in_expected_form() {
        assert_eq!(serde_json::to_string(&InstallKind::Rollback).unwrap(), "\"rollback\"");
        assert_eq!(serde_json::to_string(&InstallKind::Update).unwrap(), "\"update\"");
        assert_eq!(InstallKind::Rollback.as_str(), "откат");
    }

    /// Полный жизненный цикл отчёта через файлы: begin → read → resolve_pending.
    /// Проверяет именно `data_dir`-механику (у фасада её заменяет AppHandle).
    #[test]
    fn report_lifecycle_over_files() {
        let dir = std::env::temp_dir().join(format!(
            "about_updates_core_report_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        begin(&dir, InstallKind::Update, "26.12.0", "26.11.5", Path::new("C:/nonexistent/setup.exe"))
            .expect("намерение записано");
        let r = read(&dir).expect("отчёт читается");
        assert_eq!(r.state, InstallState::Pending);
        assert_eq!(r.previous_version, "26.11.5");
        assert_eq!(r.nsis_args, "/P /UPDATE /R");

        // Версия не сменилась → вердикт Failed, повторный resolve — не Pending → None.
        let verdict = resolve_pending(&dir, "26.11.5").expect("вервикт вынесен");
        assert_eq!(verdict.state, InstallState::Failed);
        assert!(resolve_pending(&dir, "26.12.0").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
