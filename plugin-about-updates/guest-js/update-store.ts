import {
  checkForUpdate,
  downloadAndInstallUpdate,
  installRelease,
  type ReleaseInfo,
} from './index'
import type { Update, DownloadEvent } from '@tauri-apps/plugin-updater'
import { logFront, logFrontendEvent } from '@my-tauri-plugins/plugin-logs'

/**
 * Модульный стор обновлений — SSOT состояния «проверка/скачивание/откат».
 *
 * Живёт на уровне модуля, ВНЕ любого UI-компонента: unmount вкладки/удаление
 * `<about-updates-panel>` из DOM состояние не убивают. Баг «прогрессбар
 * пропал после смены вкладки, кнопка снова активна → вторая параллельная
 * загрузка (гонка установщиков)» невозможен по дизайну: хост-UI только
 * рендерит стейт и зовёт действия, все гварды гонок — здесь.
 *
 * Framework-agnostic: subscribe/getState совместимы с useSyncExternalStore
 * (React 18), наблюдаемых/сигналов GPUI, подписок vanilla-кода.
 *
 * Хост-специфика — только через setHooks(): плагин не знает про host-команды
 * (dpi_gui: остановка обхода перед установкой) и про способ перезапуска
 * (plugin-process не является зависимостью плагина).
 */
export type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'up-to-date'
  | 'available'
  | 'downloading'
  | 'installed'
  | 'error'

export interface UpdateState {
  status: UpdateStatus
  /** Найденное проверкой обновление, ждёт явного клика «Обновить» (auto-install запрещён). */
  update: Update | null
  /** Процент скачивания 0–100; осмыслен только при totalBytes !== null. */
  progress: number
  /** Скачано байт (честные КБ, когда сервер не отдал размер). */
  downloadedBytes: number
  /** Полный размер, если сервер отдал contentLength; иначе null. */
  totalBytes: number | null
  errorMsg: string
  rollbackBusy: boolean
  rollbackMsg: string
}

export interface UpdateStoreHooks {
  /**
   * Хост-хук перед скачиванием/установкой/откатом (dpi_gui: stop_bypass).
   * Бросок исключения отменяет операцию со статусом error; хост, который хочет
   * «продолжать несмотря на неудачу хука», глотает ошибку внутри хука.
   */
  onBeforeInstall?: () => Promise<void>
  /** Хост-хук после успешной установки обновления (dpi_gui: relaunch). */
  onInstalled?: () => void
}

/**
 * Текст ошибки из reject-значения `invoke`.
 *
 * Tauri v2 реджектит промис СЫРОЙ строкой, когда команда вернула `Err(String)`.
 * Поэтому `(e as Error).message` у такой ошибки равен `undefined` — юзер видел
 * «Ошибка отката: undefined» вместо реальной причины. Приводим любую форму
 * (строка / Error / объект с message) к читаемому тексту.
 *
 * Дублировать лог здесь не нужно: каждая ветка `Err` на стороне Rust уже
 * пишется через `log::error!` (core rules §2.5 — дубли логов запрещены).
 */
export function errorText(e: unknown): string {
  if (typeof e === 'string') return e
  if (e instanceof Error) return e.message || String(e)
  if (e && typeof e === 'object' && 'message' in e) return String((e as { message: unknown }).message)
  return String(e)
}

/** Лог стадии обновления в файл лога хоста (plugin-logs). Префикс SSOT с Rust-стороной плагина. */
export function logStage(msg: string) {
  logFront(`[about-updates] ${msg}`)
}

/** Ошибка стадии обновления — уровень ERROR в файл лога хоста. */
export function logStageError(msg: string) {
  void logFrontendEvent('ERROR', `[about-updates] ${msg}`)
}

let state: UpdateState = {
  status: 'idle',
  update: null,
  progress: 0,
  downloadedBytes: 0,
  totalBytes: null,
  errorMsg: '',
  rollbackBusy: false,
  rollbackMsg: '',
}

let hooks: UpdateStoreHooks = {}

const listeners = new Set<() => void>()

function set(patch: Partial<UpdateState>) {
  state = { ...state, ...patch }
  for (const listener of listeners) listener()
}

export const updateStore = {
  subscribe(listener: () => void): () => void {
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  },

  getState(): UpdateState {
    return state
  },

  /** Хост-хуки: подключить до первого вызова startUpdate/startRollback. */
  setHooks(partial: UpdateStoreHooks): void {
    hooks = { ...hooks, ...partial }
  },

  /**
   * Проверка ТОЛЬКО ищет обновление. Установка стартует исключительно из
   * startUpdate() по явному действию юзера — правило хостов: auto-install запрещён.
   */
  async checkForUpdates(): Promise<void> {
    if (state.status === 'checking' || state.status === 'downloading') return
    set({ status: 'checking', errorMsg: '' })
    logStage('проверка обновлений начата')
    try {
      const found = await checkForUpdate()
      if (found) {
        set({ update: found, status: 'available' })
        logStage(`найдено обновление ${found.version}`)
      } else {
        set({ update: null, status: 'up-to-date' })
        logStage('обновлений нет')
      }
    } catch (e) {
      set({ status: 'error', errorMsg: errorText(e) })
      logStageError(`ошибка проверки: ${errorText(e)}`)
    }
  },

  /** Скачивание и установка найденного обновления (по явному действию юзера). */
  async startUpdate(): Promise<void> {
    if (state.status === 'checking' || state.status === 'downloading') return
    const update = state.update
    if (!update) return
    set({ status: 'downloading', progress: 0, downloadedBytes: 0, totalBytes: null, errorMsg: '' })
    logStage(`установка обновления ${update.version} начата`)
    try {
      if (hooks.onBeforeInstall) await hooks.onBeforeInstall()
      let downloaded = 0
      let total: number | null = null
      await downloadAndInstallUpdate(update, (e: DownloadEvent) => {
        if (e.event === 'Started') {
          total = e.data.contentLength ?? null
          set({ totalBytes: total })
          logStage(total
            ? `скачивание начато (${(total / (1024 * 1024)).toFixed(1)} МБ)`
            : 'скачивание начато')
        } else if (e.event === 'Progress') {
          downloaded += e.data.chunkLength
          set({
            downloadedBytes: downloaded,
            progress: total ? Math.min(100, Math.round((downloaded / total) * 100)) : 0,
          })
        } else if (e.event === 'Finished') {
          logStage('скачивание завершено, запуск установки (NSIS)')
        }
      })
      set({ status: 'installed' })
      logStage('установка завершена, перезапуск…')
      if (hooks.onInstalled) hooks.onInstalled()
    } catch (e) {
      set({ status: 'error', errorMsg: errorText(e) })
      logStageError(`ошибка установки: ${errorText(e)}`)
    }
  },

  /** Откат на конкретный релиз по URL установщика (по явному действию юзера). */
  async startRollback(target: ReleaseInfo): Promise<void> {
    if (state.rollbackBusy) return
    set({
      rollbackBusy: true,
      rollbackMsg: `Откат на ${target.version}: скачивание установщика...`,
    })
    logStage(`откат на ${target.version}: скачивание установщика`)
    try {
      if (hooks.onBeforeInstall) await hooks.onBeforeInstall()
      await installRelease(target.downloadUrl, target.version)
      set({ rollbackMsg: `Откат на ${target.version} запущен, приложение будет перезапущено.` })
      logStage(`откат на ${target.version} запущен`)
    } catch (e) {
      set({ rollbackBusy: false, rollbackMsg: errorText(e) })
      logStageError(`ошибка отката: ${errorText(e)}`)
    }
  },

  /** Сообщение об ошибке загрузки истории релизов (UI-флоу, не операция стора). */
  setRollbackMsg(msg: string): void {
    set({ rollbackMsg: msg })
  },
}
