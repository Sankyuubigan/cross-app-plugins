//! 🧠 about-updates-core — ядро проверки/установки обновлений БЕЗ Tauri.
//!
//! SSOT для всех хостов проекта: Tauri-фасад (plugin-about-updates) и GPUI-хосты
//! (AboutUpdatesCard в my-gpui-kit) работают через один и тот же код.
//!
//! Состав:
//! - [`version`] — кастомное сравнение версий (НЕ semver: ведущие нули, prerelease);
//! - [`minisign`] — проверка подписи установщика (minisign-verify, verify_stream —
//!   файл читается порциями, не грузится в RAM; внешний Base64-слой Tauri снимается);
//! - [`latest`] — официальный формат latest.json (Tauri v2, поле `url`);
//! - [`check`] — цепочка Primary (latest.json) → Fallback (api.github.com + .sig)
//!   → Graceful Empty («Обновлений пока нет»);
//! - [`history`] — история релизов GitHub Releases;
//! - [`installer`] — скачивание установщика (через downloader-core), NSIS /P /UPDATE /R;
//! - [`install_report`] — отчёт о запуске установщика (переживает перезапуск процесса);
//! - [`rollback`] — бэкап данных перед понижением версии;
//! - [`models`] — общий тип ReleaseInfo.
//!
//! Хост-специфика (каталог данных, версия приложения, выход из приложения) —
//! параметры: `data_dir: &Path`, `current_version: &str`. Tauri-фасад достаёт их
//! из AppHandle и вызывает `app.exit(0)` сам; GPUI-хосты подставляют свои.

pub mod check;
pub mod history;
pub mod install_report;
pub mod installer;
pub mod latest;
pub mod minisign;
pub mod models;
pub mod rollback;
pub mod version;

// Корневые реэкспорты: самые частые точки входа.
pub use check::{check_for_update, UpdateCandidate, UpdateConfig};
pub use history::{fetch_release_history, setup_asset_url};
pub use install_report::{InstallKind, InstallReport, InstallState};
pub use installer::{download_installer, run_install, InstallOptions};
pub use latest::{fetch_latest_json, LatestJson, PlatformArtifact};
pub use minisign::{unwrap_base64_layer, verify_installer_file};
pub use models::ReleaseInfo;
pub use rollback::{backup_before_rollback, DEFAULT_BACKUP_ITEMS};
pub use version::compare_versions;

// Прогресс скачивания установщика идёт через хуки downloader-core (ССОТ);
// реэкспорт — GPUI-хостам достаточно одного депа about-updates-core.
pub use downloader_core::{DownloadProgress, ProgressHook, set_progress_hook};
