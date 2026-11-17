// Точка входа vanilla-канала плагина (esbuild -> api-iife.js, PLUGIN_STANDARD §5.4).
// Голых импортов '@tauri-apps/*' нет — esbuild-алиасы заменяют их на shims/.
import {
  cancel,
  generate,
  listModels,
  onEvent,
  openOutputDir,
  status,
} from './index'

const g = window as unknown as { __TAURI__?: Record<string, unknown> }

if ('__TAURI__' in window) {
  const tauri = g.__TAURI__
  if (tauri && !tauri.audioGen) {
    Object.defineProperty(tauri, 'audioGen', {
      configurable: true,
      value: {
        cancel,
        generate,
        listModels,
        onEvent,
        openOutputDir,
        status,
      },
    })
  }
}
