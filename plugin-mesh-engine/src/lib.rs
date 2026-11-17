//! Фасад mesh-плагина (PLUGIN_STANDARD §0.1): тонкие Tauri-мосты; вся логика
//! в ядре `mesh-gen-core` (без tauri). Хосты без Tauri (GPUI) зовут ядро
//! напрямую через реэкспорт `engine`. Убийство trellis-server при выходе из
//! приложения — `on_event(ExitRequested)` (прецедент plugin-media-gen-engine).
//!
//! Идентификатор `mesh-engine` → invoke `plugin:mesh-engine|<cmd>`,
//! capability `mesh-engine:default`.
//!
//! Config-типа НЕТ (MVP, configless): `Builder<R>` без дженерика конфига —
//! C=() null-safe по построению (PLUGIN_STANDARD §4.3: Config обязан
//! выживать null).

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{RunEvent, Runtime};

pub mod commands;

/// Ядро плагина — публичный контракт: `plugin_mesh_engine::engine::*`.
pub use mesh_gen_core::engine;

/// Регистрация плагина в хосте: `.plugin(plugin_mesh_engine::init())`.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::<R>::new("mesh-engine")
        .invoke_handler(tauri::generate_handler![
            commands::mesh_engine_status,
            commands::mesh_generate,
            commands::mesh_engine_install,
            commands::mesh_engine_check_update,
            commands::mesh_engine_set_dir,
            commands::mesh_rig_engine_status,
            commands::mesh_rig_engine_install,
            commands::mesh_rig_engine_set_dir,
        ])
        .on_event(|_app, event| {
            if let RunEvent::ExitRequested { .. } = event {
                engine::kill_active_engines();
            }
        })
        .build()
}
