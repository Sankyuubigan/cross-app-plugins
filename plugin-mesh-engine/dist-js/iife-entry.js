// Точка входа IIFE-канала: экспортирует функции плагина на window.__TAURI__
// (шаблон plugin-media-gen-engine/guest-js/iife-entry.ts).
// Импорт './index' тянет './web-components' — панель регистрируется автоматически.
import { checkMeshEngineUpdate, getMeshEngineStatus, initMeshUpdateWatcher, installMeshEngine, meshGenerate, setMeshEngineDir, } from "./index";
if ("__TAURI__" in window) {
    const tauri = window.__TAURI__;
    if (tauri && !tauri["mesh-engine"]) {
        Object.defineProperty(tauri, "mesh-engine", {
            configurable: true,
            value: {
                checkMeshEngineUpdate,
                getMeshEngineStatus,
                initMeshUpdateWatcher,
                installMeshEngine,
                meshGenerate,
                setMeshEngineDir,
            },
        });
    }
}
