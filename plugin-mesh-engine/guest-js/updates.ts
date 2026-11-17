/**
 * Стейт обновления 3D-движка (клон updates.ts plugin-media-gen-engine).
 * Единая точка записи стейта — setMeshUpdateState; checkMeshUpdate — ручная
 * проверка (ошибки наверх: UI показывает честную ошибку, а не «актуален»);
 * initMeshUpdateWatcher — фоновая проверка при старте (ошибки глотаются:
 * «не смогли проверить» ≠ «обновлений нет», core_rules §2.2).
 */
import { checkMeshEngineUpdate } from "./index"

export interface MeshUpdateState {
  hasUpdate: boolean
  tag?: string
}

let state: MeshUpdateState = { hasUpdate: false }
const subs = new Set<(s: MeshUpdateState) => void>()
let started = false

export function getMeshUpdateState(): MeshUpdateState {
  return state
}

export function onUpdateMeshState(fn: (s: MeshUpdateState) => void): () => void {
  subs.add(fn)
  fn(state)
  return () => {
    subs.delete(fn)
  }
}

export function setMeshUpdateState(next: MeshUpdateState): void {
  state = next
  subs.forEach((fn) => fn(state))
}

export async function checkMeshUpdate(): Promise<MeshUpdateState> {
  const tag = await checkMeshEngineUpdate()
  const next: MeshUpdateState = tag ? { hasUpdate: true, tag } : { hasUpdate: false }
  setMeshUpdateState(next)
  return next
}

export function initMeshUpdateWatcher(): void {
  if (started) return
  started = true
  void checkMeshUpdate().catch(() => {
    /* фоновая проверка: состояние не портим (core_rules §2.2) */
  })
}
