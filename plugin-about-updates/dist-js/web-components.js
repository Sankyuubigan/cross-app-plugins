import { getAppVersion, getReleaseHistory, installRelease, checkForUpdate, downloadAndInstallUpdate, getSupportUrl, } from './index';
import { open } from '@tauri-apps/plugin-shell';
import { logFront, logFrontendEvent } from '@my-tauri-plugins/plugin-logs';
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
function errorText(e) {
    if (typeof e === 'string')
        return e;
    if (e instanceof Error)
        return e.message || String(e);
    if (e && typeof e === 'object' && 'message' in e)
        return String(e.message);
    return String(e);
}
/** Лог стадии обновления в файл лога хоста (plugin-logs). Префикс SSOT с Rust-стороной плагина. */
function logStage(msg) {
    logFront(`[about-updates] ${msg}`);
}
/** Ошибка стадии обновления — уровень ERROR в файл лога хоста. */
function logStageError(msg) {
    void logFrontendEvent('ERROR', `[about-updates] ${msg}`);
}
/**
 * <about-updates-panel repo="owner/repo"></about-updates-panel>
 *
 * Фреймворк-агностичная плашка «О приложении»: версия, проверка обновлений,
 * история релизов и откат. Работает в vanilla TS, React, Vue, Svelte и т.п.
 *
 * Атрибут `repo` опционален. Сам репозиторий для логики задаётся в
 * tauri.conf.json хоста (plugins.about-updates.repo).
 */
class AboutUpdatesPanel extends HTMLElement {
    constructor() {
        super(...arguments);
        this.repo = '';
        this.busy = false;
        /** Обновление, найденное проверкой, но ещё не подтверждённое кликом «Обновить». */
        this.pendingUpdate = null;
        this.historyLoaded = false;
        this.historyOpen = false;
    }
    static get observedAttributes() {
        return ['repo'];
    }
    connectedCallback() {
        this.repo = this.getAttribute('repo') ?? '';
        this.root = this.attachShadow({ mode: 'open' });
        this.render();
    }
    attributeChangedCallback() {
        this.repo = this.getAttribute('repo') ?? '';
        if (this.root)
            this.render();
    }
    setStatus(s) {
        const el = this.root.getElementById('status');
        if (el)
            el.textContent = s;
    }
    async render() {
        if (!this.root)
            return;
        this.root.innerHTML = `
      <style>
        :host { display: block; color: var(--text, #333); font-family: var(--font, system-ui, sans-serif); }
        button { margin: 6px 6px 0 0; padding: 6px 10px; cursor: pointer; border-radius: 6px;
                 border: 1px solid var(--border, #ccc); background: var(--session-hover, #eee);
                 color: var(--text, #333); font: inherit; }
        button.primary { background: var(--primary, #4a90d9); color: #fff; border-color: var(--primary, #4a90d9); }
        button:disabled { opacity: .5; cursor: default; }
        .muted { color: var(--text-muted, #888); font-size: 12px; }
        .row { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; margin-top: 8px; }
        pre { white-space: pre-wrap; max-height: 160px; overflow: auto; background: var(--bg-color, #f6f6f6);
              padding: 8px; border: 1px solid var(--border, #ccc); border-radius: 6px; margin: 6px 0 0;
              color: var(--text, #333); }
        progress { width: 100%; height: 8px; margin-top: 8px; accent-color: var(--primary, #4a90d9); }
      </style>
      <div>
        <div><strong>Версия:</strong> <span id="ver">…</span></div>
        <div class="row">
          <button id="check">Проверить обновления</button>
          <button id="install" class="primary" hidden>Обновить</button>
          <button id="rollback">История / откат</button>
        </div>
        <div id="status" class="muted" style="margin-top:8px"></div>
        <progress id="install-progress" max="100" hidden></progress>
        <div id="history" style="margin-top:8px" hidden></div>
      </div>`;
        const ver = await getAppVersion().catch(() => '?');
        this.root.getElementById('ver').textContent = ver;
        const supportUrl = await getSupportUrl().catch(() => null);
        if (supportUrl) {
            const sup = document.createElement('button');
            sup.textContent = 'Поддержать автора';
            sup.addEventListener('click', () => {
                open(supportUrl).catch(() => window.open(supportUrl, '_blank'));
            });
            this.root.getElementById('rollback').parentElement.appendChild(sup);
        }
        this.root.getElementById('check').addEventListener('click', () => this.onCheck());
        this.root.getElementById('install').addEventListener('click', () => this.onInstall());
        this.root.getElementById('rollback').addEventListener('click', () => this.onRollback());
    }
    setInstallVisible(visible) {
        const el = this.root.getElementById('install');
        if (el)
            el.hidden = !visible;
    }
    /**
     * Проверка ТОЛЬКО ищет обновление. Установка стартует исключительно из
     * onInstall() по явному клику «Обновить» — правило хостов: auto-install запрещён.
     */
    async onCheck() {
        if (this.busy)
            return;
        this.busy = true;
        this.setStatus('Проверка…');
        logStage('проверка обновлений начата');
        try {
            const update = await checkForUpdate();
            if (update) {
                this.pendingUpdate = update;
                this.setInstallVisible(true);
                this.setStatus(`Доступно обновление ${update.version}.`);
                logStage(`найдено обновление ${update.version}`);
            }
            else {
                this.pendingUpdate = null;
                this.setInstallVisible(false);
                this.setStatus('У вас последняя версия.');
                logStage('обновлений нет');
            }
        }
        catch (e) {
            this.setStatus('Ошибка: ' + errorText(e));
            logStageError(`ошибка проверки: ${errorText(e)}`);
        }
        finally {
            this.busy = false;
        }
    }
    async onInstall() {
        if (this.busy || !this.pendingUpdate)
            return;
        this.busy = true;
        const bar = this.root.getElementById('install-progress');
        logStage(`установка обновления ${this.pendingUpdate.version} начата`);
        try {
            // Прогресс есть только у скачивания (Started/Progress/Finished из plugin-updater);
            // фаза установки NSIS-инсталлера плагином не отчитывается — там статус «Установка…».
            let contentLength = null;
            let downloaded = 0;
            this.setStatus('Скачивание…');
            if (bar) {
                bar.hidden = false;
                bar.value = 0;
            }
            await downloadAndInstallUpdate(this.pendingUpdate, (e) => {
                if (e.event === 'Started') {
                    contentLength = e.data.contentLength ?? null;
                    if (bar && contentLength)
                        bar.max = contentLength;
                    logStage(contentLength
                        ? `скачивание начато (${(contentLength / (1024 * 1024)).toFixed(1)} МБ)`
                        : 'скачивание начато');
                }
                else if (e.event === 'Progress') {
                    downloaded += e.data.chunkLength;
                    if (!bar)
                        return;
                    if (contentLength) {
                        bar.value = downloaded;
                        this.setStatus(`Скачивание… ${Math.min(100, Math.round((downloaded / contentLength) * 100))}%`);
                    }
                    else {
                        // Сервер не отдал размер — честные килобайты вместо выдуманного процента.
                        this.setStatus(`Скачивание… ${Math.round(downloaded / 1024)} КБ`);
                    }
                }
                else if (e.event === 'Finished') {
                    if (bar)
                        bar.value = bar.max;
                    this.setStatus('Установка…');
                    logStage('скачивание завершено, запуск установки (NSIS)');
                }
            });
            this.setStatus('Обновление установлено, перезапуск…');
            logStage('установка завершена, перезапуск…');
        }
        catch (e) {
            this.setStatus('Ошибка: ' + errorText(e));
            logStageError(`ошибка установки: ${errorText(e)}`);
        }
        finally {
            if (bar)
                bar.hidden = true;
            this.busy = false;
        }
    }
    async onRollback() {
        const box = this.root.getElementById('history');
        this.historyOpen = !this.historyOpen;
        box.hidden = !this.historyOpen;
        if (!this.historyOpen || this.historyLoaded)
            return;
        this.historyLoaded = true;
        this.busy = true;
        this.setStatus('Загрузка истории релизов…');
        try {
            const history = await getReleaseHistory();
            box.innerHTML = '';
            const title = document.createElement('div');
            title.className = 'muted';
            title.textContent = 'Доступные релизы:';
            box.appendChild(title);
            const row = document.createElement('div');
            row.className = 'row';
            const select = document.createElement('select');
            const placeholder = document.createElement('option');
            placeholder.value = '';
            placeholder.textContent = '— выберите версию —';
            placeholder.disabled = true;
            placeholder.selected = true;
            select.appendChild(placeholder);
            for (const r of history) {
                if (r.isCurrent)
                    continue;
                const opt = document.createElement('option');
                opt.value = r.version;
                opt.textContent = r.pubDate ? `v${r.version} — ${new Date(r.pubDate).toLocaleDateString()}` : `v${r.version}`;
                opt.dataset.url = r.downloadUrl;
                select.appendChild(opt);
            }
            row.appendChild(select);
            const rollbackBtn = document.createElement('button');
            rollbackBtn.className = 'primary';
            rollbackBtn.textContent = 'Откатить';
            rollbackBtn.disabled = true;
            rollbackBtn.addEventListener('click', async () => {
                const selected = select.selectedOptions[0];
                const url = selected?.dataset.url;
                const version = selected?.value;
                if (!url || !version || this.busy)
                    return;
                this.busy = true;
                this.setStatus(`Откат на ${version}…`);
                try {
                    await installRelease(url, version);
                }
                catch (e) {
                    this.setStatus('Ошибка отката: ' + errorText(e));
                }
                finally {
                    this.busy = false;
                }
            });
            select.addEventListener('change', () => {
                rollbackBtn.disabled = select.selectedOptions[0]?.dataset.url ? false : true;
            });
            row.appendChild(rollbackBtn);
            box.appendChild(row);
            if (select.options.length <= 1) {
                const none = document.createElement('div');
                none.className = 'muted';
                none.style.marginTop = '8px';
                none.textContent = 'Реестр более старых версий пуст.';
                box.appendChild(none);
            }
            this.setStatus('');
        }
        catch (e) {
            this.setStatus('Ошибка: ' + errorText(e));
        }
        finally {
            this.busy = false;
        }
    }
}
if (!customElements.get('about-updates-panel')) {
    customElements.define('about-updates-panel', AboutUpdatesPanel);
}
