// SSOT guest-js — источник для dist-js (npm run build) и api-iife.js
// (npm run build:global). Из JS свойства camelCase (PLUGIN_STANDARD §10):
// installed/exePath/ggufCount/pixal3dFiles/glbPath/durationSecs.
// Артефакты сборки (dist-js/, api-iife.js) коммитятся (§5.2).

import { invoke } from "@tauri-apps/api/core";

export interface MeshQuantStatus {
  quant: string
  dir: string
  ggufCount: number
  pixal3dFiles: number
}

export interface MeshEngineStatus {
  installed: boolean
  exePath: string
  dir: string
  weights: MeshQuantStatus[]
}

export interface MeshGenerateArgs {
  imagePath: string
  sessionId: string
  model?: string
  quant?: string
  backend?: string
  gpu?: number
  seed?: number
  resolution?: number
  bgRemoval?: string
  faces?: string
}

export interface MeshGenerateOut {
  glbPath: string
  port: number
  durationSecs: number
}

export function getMeshEngineStatus(): Promise<MeshEngineStatus> {
  return invoke<MeshEngineStatus>("plugin:mesh-engine|mesh_engine_status")
}

export function meshGenerate(args: MeshGenerateArgs): Promise<MeshGenerateOut> {
  return invoke<MeshGenerateOut>("plugin:mesh-engine|mesh_generate", { args })
}

export interface MeshInstallArgs {
  backend?: string
  quant?: string
}

export function installMeshEngine(args: MeshInstallArgs): Promise<MeshEngineStatus> {
  return invoke<MeshEngineStatus>("plugin:mesh-engine|mesh_engine_install", { args })
}

export function setMeshEngineDir(path: string): Promise<MeshEngineStatus> {
  return invoke<MeshEngineStatus>("plugin:mesh-engine|mesh_engine_set_dir", { path })
}

export function checkMeshEngineUpdate(): Promise<string | null> {
  return invoke<string | null>("plugin:mesh-engine|mesh_engine_check_update")
}

// ─────────────── Риггинг (skin-tokens.cpp, секция «Риггинг» плашки) ───────────────

export interface RigEngineStatus {
  ready: boolean
  exePresent: boolean
  dllsPresent: boolean
  weightsPresent: number
  weightsTotal: number
  dir: string
  weightsDir: string
}

export function getRigEngineStatus(): Promise<RigEngineStatus> {
  return invoke<RigEngineStatus>("plugin:mesh-engine|mesh_rig_engine_status")
}

export function installRigEngine(): Promise<RigEngineStatus> {
  return invoke<RigEngineStatus>("plugin:mesh-engine|mesh_rig_engine_install")
}

export function setRigEngineDir(path: string): Promise<RigEngineStatus> {
  return invoke<RigEngineStatus>("plugin:mesh-engine|mesh_rig_engine_set_dir", { path })
}

// Стейт обновления (клон updates.ts plugin-media-gen-engine): бейдж и кнопки панели.
export * from "./updates"

// Web Components (панель движка) регистрируются при импорте пакета.
import "./web-components"
