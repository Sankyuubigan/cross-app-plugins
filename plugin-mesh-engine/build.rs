//! SSOT имён команд плагина (PLUGIN_STANDARD §4.2): массив `COMMANDS` +
//! `global_api_script_path("./api-iife.js")` — api-iife.js обязателен
//! (camelCase из JS, §10), иначе хост не зарегистрирует команды.
//!
//! Имена команд: snake_case в Rust (§10), kebab-case в permissions.

const COMMANDS: &[&str] = &[
    "mesh_engine_status",
    "mesh_generate",
    "mesh_engine_install",
    "mesh_engine_check_update",
    "mesh_engine_set_dir",
    "mesh_rig_engine_status",
    "mesh_rig_engine_install",
    "mesh_rig_engine_set_dir",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .global_api_script_path("./api-iife.js")
        .build();
}
