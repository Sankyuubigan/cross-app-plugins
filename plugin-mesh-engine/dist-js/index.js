// SSOT guest-js — источник для dist-js (npm run build) и api-iife.js
// (npm run build:global). Из JS свойства camelCase (PLUGIN_STANDARD §10):
// installed/exePath/ggufCount/pixal3dFiles/glbPath/durationSecs.
// Артефакты сборки (dist-js/, api-iife.js) коммитятся (§5.2).
import { invoke } from "@tauri-apps/api/core";
export function getMeshEngineStatus() {
    return invoke("plugin:mesh-engine|mesh_engine_status");
}
export function meshGenerate(args) {
    return invoke("plugin:mesh-engine|mesh_generate", { args });
}
export function installMeshEngine(args) {
    return invoke("plugin:mesh-engine|mesh_engine_install", { args });
}
export function setMeshEngineDir(path) {
    return invoke("plugin:mesh-engine|mesh_engine_set_dir", { path });
}
export function checkMeshEngineUpdate() {
    return invoke("plugin:mesh-engine|mesh_engine_check_update");
}
export function getRigEngineStatus() {
    return invoke("plugin:mesh-engine|mesh_rig_engine_status");
}
export function installRigEngine() {
    return invoke("plugin:mesh-engine|mesh_rig_engine_install");
}
export function setRigEngineDir(path) {
    return invoke("plugin:mesh-engine|mesh_rig_engine_set_dir", { path });
}
// Стейт обновления (клон updates.ts plugin-media-gen-engine): бейдж и кнопки панели.
export * from "./updates";
// Web Components (панель движка) регистрируются при импорте пакета.
import "./web-components";
