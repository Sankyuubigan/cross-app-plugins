/**
 * Стейт обновления 3D-движка (клон updates.ts plugin-media-gen-engine).
 * Единая точка записи стейта — setMeshUpdateState; checkMeshUpdate — ручная
 * проверка (ошибки наверх: UI показывает честную ошибку, а не «актуален»);
 * initMeshUpdateWatcher — фоновая проверка при старте (ошибки глотаются:
 * «не смогли проверить» ≠ «обновлений нет», core_rules §2.2).
 */
import { checkMeshEngineUpdate } from "./index";
let state = { hasUpdate: false };
const subs = new Set();
let started = false;
export function getMeshUpdateState() {
    return state;
}
export function onUpdateMeshState(fn) {
    subs.add(fn);
    fn(state);
    return () => {
        subs.delete(fn);
    };
}
export function setMeshUpdateState(next) {
    state = next;
    subs.forEach((fn) => fn(state));
}
export async function checkMeshUpdate() {
    const tag = await checkMeshEngineUpdate();
    const next = tag ? { hasUpdate: true, tag } : { hasUpdate: false };
    setMeshUpdateState(next);
    return next;
}
export function initMeshUpdateWatcher() {
    if (started)
        return;
    started = true;
    void checkMeshUpdate().catch(() => {
        /* фоновая проверка: состояние не портим (core_rules §2.2) */
    });
}
