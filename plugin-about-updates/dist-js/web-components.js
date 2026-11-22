import { getAppVersion, getReleaseHistory, getSupportUrl } from './index';
import { updateStore, errorText, logStageError } from './update-store';
import { open } from '@tauri-apps/plugin-shell';
/**
 * <about-updates-panel repo="owner/repo"></about-updates-panel>
 *
 * Фреймворк-агностичная плашка «О приложении»: версия, проверка обновлений,
 * история релизов и откат. Работает в vanilla TS, React, Vue, Svelte и т.п.
 *
 * ВЕСЬ стейт обновления/отката живёт в модульном updateStore (update-store.ts):
 * панель только рендерит его по подписке и зовёт действия. Удаление элемента
 * из DOM (unmount вкладки хоста) и возврат обратно НЕ теряют прогресс скачивания
 * и не позволяют запустить вторую параллельную установку — гварды в сторе.
 *
 * Атрибут `repo` опционален. Сам репозиторий для логики задаётся в
 * tauri.conf.json хоста (plugins.about-updates.repo).
 */
class AboutUpdatesPanel extends HTMLElement {
    constructor() {
        super(...arguments);
        this.repo = '';
        this.unsub = null;
        this.historyLoaded = false;
        this.historyOpen = false;
        this.rollbackSelect = null;
        this.rollbackBtn = null;
    }
    static get observedAttributes() {
        return ['repo'];
    }
    connectedCallback() {
        this.repo = this.getAttribute('repo') ?? '';
        this.root = this.attachShadow({ mode: 'open' });
        void this.render();
        this.unsub = updateStore.subscribe(() => this.syncState());
        this.syncState();
    }
    disconnectedCallback() {
        // Стейт остаётся в updateStore — при возврате элемента в DOM
        // connectedCallback переподпишется и syncState восстановит картинку.
        this.unsub?.();
        this.unsub = null;
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
        this.syncState();
    }
    /** Перерисовать DOM из стейта стора (единственный источник правды). */
    syncState() {
        if (!this.root)
            return;
        const st = updateStore.getState();
        const checkBtn = this.root.getElementById('check');
        if (checkBtn) {
            checkBtn.disabled =
                st.status === 'checking' || st.status === 'downloading' || st.rollbackBusy;
        }
        const installBtn = this.root.getElementById('install');
        if (installBtn)
            installBtn.hidden = st.status !== 'available';
        const bar = this.root.getElementById('install-progress');
        if (bar) {
            bar.hidden = st.status !== 'downloading';
            if (!bar.hidden) {
                if (st.totalBytes) {
                    bar.max = st.totalBytes;
                    bar.value = st.downloadedBytes;
                }
                else {
                    // Сервер не отдал размер — честная неопределённость вместо выдуманного процента.
                    bar.max = 100;
                    bar.removeAttribute('value');
                }
            }
        }
        if (this.rollbackBtn) {
            const hasUrl = Boolean(this.rollbackSelect?.selectedOptions[0]?.dataset.url);
            this.rollbackBtn.disabled = st.rollbackBusy || !hasUrl;
        }
        if (st.rollbackBusy) {
            this.setStatus(st.rollbackMsg || 'Откат…');
        }
        else {
            switch (st.status) {
                case 'idle':
                    this.setStatus('');
                    break;
                case 'checking':
                    this.setStatus('Проверка…');
                    break;
                case 'up-to-date':
                    this.setStatus('У вас последняя версия.');
                    break;
                case 'available':
                    this.setStatus(`Доступно обновление ${st.update?.version ?? ''}.`);
                    break;
                case 'downloading':
                    this.setStatus(st.totalBytes
                        ? `Скачивание… ${st.progress}%`
                        : `Скачивание… ${Math.round(st.downloadedBytes / 1024)} КБ`);
                    break;
                case 'installed':
                    this.setStatus('Обновление установлено, перезапуск…');
                    break;
                case 'error':
                    this.setStatus('Ошибка: ' + st.errorMsg);
                    break;
            }
        }
    }
    onCheck() {
        void updateStore.checkForUpdates();
    }
    onInstall() {
        void updateStore.startUpdate();
    }
    async onRollback() {
        const box = this.root.getElementById('history');
        this.historyOpen = !this.historyOpen;
        box.hidden = !this.historyOpen;
        if (!this.historyOpen || this.historyLoaded)
            return;
        this.historyLoaded = true;
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
            this.rollbackSelect = select;
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
            this.rollbackBtn = rollbackBtn;
            rollbackBtn.addEventListener('click', () => {
                const selected = this.rollbackSelect?.selectedOptions[0];
                const url = selected?.dataset.url;
                const version = selected?.value;
                if (!url || !version)
                    return;
                void updateStore.startRollback({ version, downloadUrl: url, isCurrent: false });
            });
            select.addEventListener('change', () => this.syncState());
            row.appendChild(rollbackBtn);
            box.appendChild(row);
            if (select.options.length <= 1) {
                const none = document.createElement('div');
                none.className = 'muted';
                none.style.marginTop = '8px';
                none.textContent = 'Реестр более старых версий пуст.';
                box.appendChild(none);
            }
            this.syncState();
        }
        catch (e) {
            logStageError(`ошибка загрузки истории: ${errorText(e)}`);
            this.setStatus('Ошибка: ' + errorText(e));
        }
    }
}
if (!customElements.get('about-updates-panel')) {
    customElements.define('about-updates-panel', AboutUpdatesPanel);
}
