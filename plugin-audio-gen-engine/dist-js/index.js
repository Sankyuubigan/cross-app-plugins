import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
const CMD = (c) => `plugin:audio-gen|${c}`;
/** Список известных моделей (SA3/MOSS/ACE-Step) с признаком установки. */
export function listModels() {
    return invoke(CMD('audio_gen_list_models'));
}
/** Запустить генерацию (возвращает job_id сразу; прогресс — onEvent/status). */
export function generate(opts) {
    return invoke(CMD('audio_gen_generate'), { ...opts });
}
/** Снимок состояния: задачи + резолвленные пути настроек. */
export function status() {
    return invoke(CMD('audio_gen_status'));
}
/** Отменить задачу (убивает сервер движка). */
export function cancel(jobId) {
    return invoke(CMD('audio_gen_cancel'), { jobId });
}
/** Открыть папку вывода в проводнике. */
export function openOutputDir() {
    return invoke(CMD('audio_gen_open_output_dir'));
}
/** Подписка на события прогресса. Возвращает unlisten. */
export function onEvent(handler) {
    return listen('audio-gen-event', (e) => handler(e.payload));
}
