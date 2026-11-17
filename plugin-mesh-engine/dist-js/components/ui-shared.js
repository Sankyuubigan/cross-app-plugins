/** Общие UI-примитивы панелей mesh-engine (вербатим-копия из image-engine). */
export const STYLE = `
  :host { display: block; color: var(--text, #eee); font-family: var(--font, system-ui, sans-serif); font-size: 13px; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .row + .row { margin-top: 8px; }
  label { color: var(--text-muted, #999); min-width: 60px; }
  button { padding: 6px 10px; cursor: pointer; border-radius: 6px; border: 1px solid var(--border, #333);
           background: var(--session-hover, #eee); color: var(--text, #eee); font: inherit; font-size: 13px; }
  button.primary { background: var(--primary, #4a90d9); color: #fff; border-color: var(--primary, #4a90d9); }
  button.danger { background: var(--danger, #b54242); color: #fff; border-color: var(--danger, #b54242); }
  button:disabled { opacity: .5; cursor: default; }
  select, input[type=text] { padding: 6px; border-radius: 6px; border: 1px solid var(--border, #333);
           background: var(--bg-elevated, #1c1c1c); color: var(--text, #eee); font: inherit; font-size: 13px; }
  .progress-container { display: none; margin-top: 10px; }
  .progress-status { color: var(--text-muted, #999); font-size: 12px; margin-bottom: 5px; word-break: break-all; }
  .progress-track { height: 8px; border-radius: 6px; background: var(--border, #333); overflow: hidden; }
  .progress-bar { height: 100%; width: 0%; background: var(--primary, #4a90d9); transition: width .1s linear; }
  .hint { color: var(--text-muted, #999); font-size: 12px; white-space: pre-line; }
`;
export function formatBytes(n) {
    if (!isFinite(n) || n <= 0)
        return '0 B';
    const units = ['B', 'KB', 'MB', 'GB'];
    let i = 0;
    while (n >= 1024 && i < units.length - 1) {
        n /= 1024;
        i++;
    }
    return `${n.toFixed(1)} ${units[i]}`;
}
/** Лёгкий всплывающий тост (без внешних зависимостей; использует CSS-переменные хоста). */
export function toast(msg, kind = 'success') {
    const el = document.createElement('div');
    el.textContent = msg;
    el.style.cssText =
        `position:fixed; right:16px; bottom:16px; z-index:5000; max-width:420px; padding:10px 14px;` +
            `border-radius:8px; font-size:13px; color:#fff; ` +
            `background:${kind === 'success' ? 'rgba(60,140,70,.95)' : 'rgba(180,60,60,.95)'};`;
    document.body.appendChild(el);
    setTimeout(() => el.remove(), 4200);
}
/** Текст прогресса по байтовому состоянию. */
export function progressText(downloaded, total) {
    const pct = total > 0 ? (downloaded / total) * 100 : 0;
    return `${formatBytes(downloaded)} / ${total > 0 ? formatBytes(total) : '?'}${total > 0 ? ` (${pct.toFixed(0)}%)` : ''}`;
}
