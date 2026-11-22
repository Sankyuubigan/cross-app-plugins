import { type ReleaseInfo } from './index';
import type { Update } from '@tauri-apps/plugin-updater';
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
export type UpdateStatus = 'idle' | 'checking' | 'up-to-date' | 'available' | 'downloading' | 'installed' | 'error';
export interface UpdateState {
    status: UpdateStatus;
    /** Найденное проверкой обновление, ждёт явного клика «Обновить» (auto-install запрещён). */
    update: Update | null;
    /** Процент скачивания 0–100; осмыслен только при totalBytes !== null. */
    progress: number;
    /** Скачано байт (честные КБ, когда сервер не отдал размер). */
    downloadedBytes: number;
    /** Полный размер, если сервер отдал contentLength; иначе null. */
    totalBytes: number | null;
    errorMsg: string;
    rollbackBusy: boolean;
    rollbackMsg: string;
}
export interface UpdateStoreHooks {
    /**
     * Хост-хук перед скачиванием/установкой/откатом (dpi_gui: stop_bypass).
     * Бросок исключения отменяет операцию со статусом error; хост, который хочет
     * «продолжать несмотря на неудачу хука», глотает ошибку внутри хука.
     */
    onBeforeInstall?: () => Promise<void>;
    /** Хост-хук после успешной установки обновления (dpi_gui: relaunch). */
    onInstalled?: () => void;
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
export declare function errorText(e: unknown): string;
/** Лог стадии обновления в файл лога хоста (plugin-logs). Префикс SSOT с Rust-стороной плагина. */
export declare function logStage(msg: string): void;
/** Ошибка стадии обновления — уровень ERROR в файл лога хоста. */
export declare function logStageError(msg: string): void;
export declare const updateStore: {
    subscribe(listener: () => void): () => void;
    getState(): UpdateState;
    /** Хост-хуки: подключить до первого вызова startUpdate/startRollback. */
    setHooks(partial: UpdateStoreHooks): void;
    /**
     * Проверка ТОЛЬКО ищет обновление. Установка стартует исключительно из
     * startUpdate() по явному действию юзера — правило хостов: auto-install запрещён.
     */
    checkForUpdates(): Promise<void>;
    /** Скачивание и установка найденного обновления (по явному действию юзера). */
    startUpdate(): Promise<void>;
    /** Откат на конкретный релиз по URL установщика (по явному действию юзера). */
    startRollback(target: ReleaseInfo): Promise<void>;
    /** Сообщение об ошибке загрузки истории релизов (UI-флоу, не операция стора). */
    setRollbackMsg(msg: string): void;
};
