/** Модель реестра в формате UI (описание + фактическая установка). */
export interface ModelEntry {
    id: string;
    name: string;
    kind: string;
    license: string;
    vram_gb: number | null;
    sample_rate: number;
    channels: number;
    max_duration_secs: number;
    default_steps: number;
    weights_repo: string;
    installed: boolean;
}
/** Запись задачи генерации (снимок). */
export interface JobRecord {
    id: string;
    model_id: string;
    prompt: string;
    status: 'queued' | 'running' | 'done' | 'failed' | 'cancelled';
    progress: number;
    message: string;
    output_path: string | null;
    duration_secs: number | null;
    created_at: string;
    finished_at: string | null;
}
/** Снимок состояния плагина. */
export interface StatusSnapshot {
    jobs: JobRecord[];
    settings: {
        backend: string;
        loopCrossfadeSecs: number;
        normalize: boolean;
        engineDir: string;
        modelsDir: string;
        outputDir: string;
    };
}
/** Событие прогресса задачи (tauri-событие `audio-gen-event`). */
export interface AudioGenEvent {
    jobId: string;
    status: string;
    progress: number;
}
/** Параметры генерации (camelCase — tauri сам мапит на snake_case Rust). */
export interface GenerateOptions {
    modelId: string;
    prompt: string;
    durationSecs?: number;
    steps?: number;
    seed?: number;
    makeLoop?: boolean;
    name?: string;
}
/** Список известных моделей (SA3/MOSS/ACE-Step) с признаком установки. */
export declare function listModels(): Promise<ModelEntry[]>;
/** Запустить генерацию (возвращает job_id сразу; прогресс — onEvent/status). */
export declare function generate(opts: GenerateOptions): Promise<string>;
/** Снимок состояния: задачи + резолвленные пути настроек. */
export declare function status(): Promise<StatusSnapshot>;
/** Отменить задачу (убивает сервер движка). */
export declare function cancel(jobId: string): Promise<boolean>;
/** Открыть папку вывода в проводнике. */
export declare function openOutputDir(): Promise<string>;
/** Подписка на события прогресса. Возвращает unlisten. */
export declare function onEvent(handler: (e: AudioGenEvent) => void): Promise<() => void>;
