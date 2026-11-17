/** Общие UI-примитивы панелей mesh-engine (вербатим-копия из image-engine). */
export declare const STYLE = "\n  :host { display: block; color: var(--text, #eee); font-family: var(--font, system-ui, sans-serif); font-size: 13px; }\n  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }\n  .row + .row { margin-top: 8px; }\n  label { color: var(--text-muted, #999); min-width: 60px; }\n  button { padding: 6px 10px; cursor: pointer; border-radius: 6px; border: 1px solid var(--border, #333);\n           background: var(--session-hover, #eee); color: var(--text, #eee); font: inherit; font-size: 13px; }\n  button.primary { background: var(--primary, #4a90d9); color: #fff; border-color: var(--primary, #4a90d9); }\n  button.danger { background: var(--danger, #b54242); color: #fff; border-color: var(--danger, #b54242); }\n  button:disabled { opacity: .5; cursor: default; }\n  select, input[type=text] { padding: 6px; border-radius: 6px; border: 1px solid var(--border, #333);\n           background: var(--bg-elevated, #1c1c1c); color: var(--text, #eee); font: inherit; font-size: 13px; }\n  .progress-container { display: none; margin-top: 10px; }\n  .progress-status { color: var(--text-muted, #999); font-size: 12px; margin-bottom: 5px; word-break: break-all; }\n  .progress-track { height: 8px; border-radius: 6px; background: var(--border, #333); overflow: hidden; }\n  .progress-bar { height: 100%; width: 0%; background: var(--primary, #4a90d9); transition: width .1s linear; }\n  .hint { color: var(--text-muted, #999); font-size: 12px; white-space: pre-line; }\n";
export declare function formatBytes(n: number): string;
/** Лёгкий всплывающий тост (без внешних зависимостей; использует CSS-переменные хоста). */
export declare function toast(msg: string, kind?: 'success' | 'error'): void;
/** Payload события downloader:progress плагина. */
export interface ProgressPayload {
    downloaded: number;
    total: number;
    kind?: string;
    status?: string;
}
/** Текст прогресса по байтовому состоянию. */
export declare function progressText(downloaded: number, total: number): string;
