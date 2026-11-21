// Generated from guest-js/ by esbuild (npm run build:global). Do not edit by hand.
"use strict";
(() => {
  // guest-js/shims/core.ts
  var g = window;
  function coreInvoke(...args) {
    const core = g.__TAURI__?.core;
    if (core?.invoke) return core.invoke(...args);
    const internals = g.__TAURI_INTERNALS__;
    if (internals?.invoke) return internals.invoke(...args);
    return Promise.reject(new Error("window.__TAURI__.core.invoke \u043D\u0435\u0434\u043E\u0441\u0442\u0443\u043F\u0435\u043D"));
  }
  function invoke(cmd, args) {
    return coreInvoke(cmd, args ?? {});
  }

  // guest-js/shims/updater.ts
  var g2 = window;
  async function check(options) {
    const updater = g2.__TAURI__?.updater;
    if (!updater?.check) {
      throw new Error("window.__TAURI__.updater.check \u043D\u0435\u0434\u043E\u0441\u0442\u0443\u043F\u0435\u043D");
    }
    return updater.check(options);
  }

  // guest-js/shims/shell.ts
  var g3 = window;
  async function open(path, openWith) {
    const shell = g3.__TAURI__?.shell;
    if (!shell?.open) {
      throw new Error("window.__TAURI__.shell.open \u043D\u0435\u0434\u043E\u0441\u0442\u0443\u043F\u0435\u043D");
    }
    return shell.open(path, openWith);
  }

  // ../plugin-log-collector/node_modules/@tauri-apps/api/external/tslib/tslib.es6.js
  function __classPrivateFieldGet(receiver, state, kind, f) {
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a getter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot read private member from an object whose class did not declare it");
    return kind === "m" ? f : kind === "a" ? f.call(receiver) : f ? f.value : state.get(receiver);
  }
  function __classPrivateFieldSet(receiver, state, value, kind, f) {
    if (kind === "m") throw new TypeError("Private method is not writable");
    if (kind === "a" && !f) throw new TypeError("Private accessor was defined without a setter");
    if (typeof state === "function" ? receiver !== state || !f : !state.has(receiver)) throw new TypeError("Cannot write private member to an object whose class did not declare it");
    return kind === "a" ? f.call(receiver, value) : f ? f.value = value : state.set(receiver, value), value;
  }

  // ../plugin-log-collector/node_modules/@tauri-apps/api/core.js
  var _Channel_onmessage;
  var _Channel_nextMessageIndex;
  var _Channel_pendingMessages;
  var _Channel_messageEndIndex;
  var _Resource_rid;
  var SERIALIZE_TO_IPC_FN = "__TAURI_TO_IPC_KEY__";
  function transformCallback(callback, once = false) {
    return window.__TAURI_INTERNALS__.transformCallback(callback, once);
  }
  var Channel = class {
    constructor(onmessage) {
      _Channel_onmessage.set(this, void 0);
      _Channel_nextMessageIndex.set(this, 0);
      _Channel_pendingMessages.set(this, []);
      _Channel_messageEndIndex.set(this, void 0);
      __classPrivateFieldSet(this, _Channel_onmessage, onmessage || (() => {
      }), "f");
      this.id = transformCallback((rawMessage) => {
        const index = rawMessage.index;
        if ("end" in rawMessage) {
          if (index == __classPrivateFieldGet(this, _Channel_nextMessageIndex, "f")) {
            this.cleanupCallback();
          } else {
            __classPrivateFieldSet(this, _Channel_messageEndIndex, index, "f");
          }
          return;
        }
        const message = rawMessage.message;
        if (index == __classPrivateFieldGet(this, _Channel_nextMessageIndex, "f")) {
          __classPrivateFieldGet(this, _Channel_onmessage, "f").call(this, message);
          __classPrivateFieldSet(this, _Channel_nextMessageIndex, __classPrivateFieldGet(this, _Channel_nextMessageIndex, "f") + 1, "f");
          while (__classPrivateFieldGet(this, _Channel_nextMessageIndex, "f") in __classPrivateFieldGet(this, _Channel_pendingMessages, "f")) {
            const message2 = __classPrivateFieldGet(this, _Channel_pendingMessages, "f")[__classPrivateFieldGet(this, _Channel_nextMessageIndex, "f")];
            __classPrivateFieldGet(this, _Channel_onmessage, "f").call(this, message2);
            delete __classPrivateFieldGet(this, _Channel_pendingMessages, "f")[__classPrivateFieldGet(this, _Channel_nextMessageIndex, "f")];
            __classPrivateFieldSet(this, _Channel_nextMessageIndex, __classPrivateFieldGet(this, _Channel_nextMessageIndex, "f") + 1, "f");
          }
          if (__classPrivateFieldGet(this, _Channel_nextMessageIndex, "f") === __classPrivateFieldGet(this, _Channel_messageEndIndex, "f")) {
            this.cleanupCallback();
          }
        } else {
          __classPrivateFieldGet(this, _Channel_pendingMessages, "f")[index] = message;
        }
      });
    }
    cleanupCallback() {
      window.__TAURI_INTERNALS__.unregisterCallback(this.id);
    }
    set onmessage(handler) {
      __classPrivateFieldSet(this, _Channel_onmessage, handler, "f");
    }
    get onmessage() {
      return __classPrivateFieldGet(this, _Channel_onmessage, "f");
    }
    [(_Channel_onmessage = /* @__PURE__ */ new WeakMap(), _Channel_nextMessageIndex = /* @__PURE__ */ new WeakMap(), _Channel_pendingMessages = /* @__PURE__ */ new WeakMap(), _Channel_messageEndIndex = /* @__PURE__ */ new WeakMap(), SERIALIZE_TO_IPC_FN)]() {
      return `__CHANNEL__:${this.id}`;
    }
    toJSON() {
      return this[SERIALIZE_TO_IPC_FN]();
    }
  };
  async function invoke2(cmd, args = {}, options) {
    return window.__TAURI_INTERNALS__.invoke(cmd, args, options);
  }
  _Resource_rid = /* @__PURE__ */ new WeakMap();

  // ../plugin-log-collector/node_modules/@tauri-apps/api/event.js
  var TauriEvent;
  (function(TauriEvent2) {
    TauriEvent2["WINDOW_RESIZED"] = "tauri://resize";
    TauriEvent2["WINDOW_MOVED"] = "tauri://move";
    TauriEvent2["WINDOW_CLOSE_REQUESTED"] = "tauri://close-requested";
    TauriEvent2["WINDOW_DESTROYED"] = "tauri://destroyed";
    TauriEvent2["WINDOW_FOCUS"] = "tauri://focus";
    TauriEvent2["WINDOW_BLUR"] = "tauri://blur";
    TauriEvent2["WINDOW_SCALE_FACTOR_CHANGED"] = "tauri://scale-change";
    TauriEvent2["WINDOW_THEME_CHANGED"] = "tauri://theme-changed";
    TauriEvent2["WINDOW_CREATED"] = "tauri://window-created";
    TauriEvent2["WINDOW_SUSPENDED"] = "tauri://suspended";
    TauriEvent2["WINDOW_RESUMED"] = "tauri://resumed";
    TauriEvent2["WEBVIEW_CREATED"] = "tauri://webview-created";
    TauriEvent2["DRAG_ENTER"] = "tauri://drag-enter";
    TauriEvent2["DRAG_OVER"] = "tauri://drag-over";
    TauriEvent2["DRAG_DROP"] = "tauri://drag-drop";
    TauriEvent2["DRAG_LEAVE"] = "tauri://drag-leave";
  })(TauriEvent || (TauriEvent = {}));
  async function _unlisten(event, eventId) {
    window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener(event, eventId);
    await invoke2("plugin:event|unlisten", {
      event,
      eventId
    });
  }
  async function listen(event, handler, options) {
    var _a;
    const target = typeof (options === null || options === void 0 ? void 0 : options.target) === "string" ? { kind: "AnyLabel", label: options.target } : (_a = options === null || options === void 0 ? void 0 : options.target) !== null && _a !== void 0 ? _a : { kind: "Any" };
    return invoke2("plugin:event|listen", {
      event,
      target,
      handler: transformCallback(handler)
    }).then((eventId) => {
      return async () => _unlisten(event, eventId);
    });
  }

  // ../plugin-log-collector/node_modules/@tauri-apps/plugin-dialog/dist-js/index.js
  async function save(options = {}) {
    if (typeof options === "object") {
      Object.freeze(options);
    }
    return await invoke("plugin:dialog|save", { options });
  }

  // ../plugin-log-collector/dist-js/web-components.js
  var LogsPanel = class extends HTMLElement {
    constructor() {
      super(...arguments);
      this.unlisten = null;
      this.lines = [];
      this.nearBottom = true;
      this.maxLines = 4e3;
    }
    connectedCallback() {
      if (!this.shadowRoot) {
        this.root = this.attachShadow({ mode: "open" });
      } else {
        this.root = this.shadowRoot;
      }
      this.render();
    }
    disconnectedCallback() {
      this.unlisten?.();
      this.unlisten = null;
    }
    render() {
      this.root.innerHTML = `
      <style>
        :host { display: flex; flex-direction: column; height: 100%; min-height: 0;
               font-family: var(--font, system-ui, sans-serif); }
        .toolbar { display: flex; align-items: center; gap: 8px; padding: 6px 10px;
                   background: var(--logs-toolbar-bg, #2a2d2e);
                   border-bottom: 1px solid var(--border, #3a3d3e);
                   border-top-left-radius: 8px; border-top-right-radius: 8px; }
        .title { font-weight: 600; color: var(--text, #e0e0e0); font-size: 13px; white-space: nowrap; }
        .hint { color: var(--text-muted, #9a9a9a); font-size: 11px; overflow: hidden;
                text-overflow: ellipsis; white-space: nowrap; }
        .spacer { flex: 1; }
        button { font: inherit; font-size: 12px; padding: 4px 10px; cursor: pointer;
                 border: 1px solid var(--border, #3a3d3e); border-radius: 6px;
                 background: var(--logs-btn-bg, #3a3d3e); color: var(--text, #e0e0e0); }
        button:hover { filter: brightness(1.15); }
        textarea { flex: 1; resize: none; width: 100%; box-sizing: border-box; border: none;
                   padding: 8px; background: var(--logs-area-bg, #1e1e1e);
                   color: var(--logs-area-text, #d4d4d4);
                   font-family: var(--mono-font, Consolas, monospace); font-size: 12px;
                   line-height: 1.45; outline: none;
                   border-bottom-left-radius: 8px; border-bottom-right-radius: 8px; }
      </style>
      <div class="toolbar">
        <span class="title">\u041B\u043E\u0433\u0438 \u0441\u0438\u0441\u0442\u0435\u043C\u044B</span>
        <span class="hint" id="hint"></span>
        <div class="spacer"></div>
        <button id="copy">\u041A\u043E\u043F\u0438\u0440\u043E\u0432\u0430\u0442\u044C</button>
        <button id="save">\u0421\u043E\u0445\u0440\u0430\u043D\u0438\u0442\u044C</button>
        <button id="clear">\u041E\u0447\u0438\u0441\u0442\u0438\u0442\u044C</button>
      </div>
      <textarea id="area" readonly spellcheck="false" placeholder="\u0417\u0434\u0435\u0441\u044C \u043F\u043E\u044F\u0432\u0438\u0442\u0441\u044F \u043B\u043E\u0433 \u043F\u0440\u0438\u043B\u043E\u0436\u0435\u043D\u0438\u044F\u2026"></textarea>`;
      const area = this.root.getElementById("area");
      area.addEventListener("scroll", () => {
        this.nearBottom = area.scrollHeight - area.scrollTop - area.clientHeight < 40;
      });
      this.root.getElementById("copy").addEventListener("click", () => {
        void this.onCopy();
      });
      this.root.getElementById("save").addEventListener("click", () => {
        void this.onSave();
      });
      this.root.getElementById("clear").addEventListener("click", () => {
        this.onClear();
      });
      void getLastLogsPath().then((p) => {
        if (p)
          this.root.getElementById("hint").textContent = `\u0424\u0430\u0439\u043B: ${p}`;
      }).catch(() => {
      });
      void this.loadHistory();
      void onLogMessage((line) => this.appendLine(line)).then((u) => {
        this.unlisten = u;
      }).catch((e) => {
        logFront(`[logs-panel] \u043D\u0435 \u0443\u0434\u0430\u043B\u043E\u0441\u044C \u043F\u043E\u0434\u043F\u0438\u0441\u0430\u0442\u044C\u0441\u044F \u043D\u0430 \u043B\u043E\u0433: ${String(e)}`);
      });
    }
    /**
     * Подтягивает строки, записанные ДО подписки (pre-Tauri старт, setup) —
     * иначе вкладка «Логи» выглядит пустой (core §2.5). Подписка идёт первой,
     * поэтому live-строки уже лежат в буфере; пересечение с хвостом файла
     * (максимальный суффикс history, совпадающий с префиксом буфера) вырезается,
     * чтобы не задвоить строки.
     */
    async loadHistory() {
      let history;
      try {
        history = (await readLogTail(this.maxLines)).split("\n").filter((l) => l.trim() !== "");
      } catch (e) {
        logFront(`[logs-panel] \u043D\u0435 \u0443\u0434\u0430\u043B\u043E\u0441\u044C \u0437\u0430\u0433\u0440\u0443\u0437\u0438\u0442\u044C \u0438\u0441\u0442\u043E\u0440\u0438\u044E \u043B\u043E\u0433\u0430: ${String(e)}`);
        return;
      }
      if (history.length === 0)
        return;
      let overlap = 0;
      const maxOverlap = Math.min(history.length, this.lines.length);
      for (let n = maxOverlap; n > 0; n--) {
        if (history.slice(history.length - n).join("\n") === this.lines.slice(0, n).join("\n")) {
          overlap = n;
          break;
        }
      }
      this.lines = history.concat(this.lines.slice(overlap));
      this.nearBottom = true;
      this.renderLines();
    }
    renderLines() {
      const area = this.root.getElementById("area");
      if (!area)
        return;
      if (this.lines.length > this.maxLines) {
        this.lines = this.lines.slice(this.lines.length - this.maxLines);
      }
      area.value = this.lines.length ? `${this.lines.join("\n")}
` : "";
      area.scrollTop = area.scrollHeight;
    }
    appendLine(line) {
      this.lines.push(line);
      const area = this.root.getElementById("area");
      if (!area)
        return;
      let trimmed = false;
      if (this.lines.length > this.maxLines) {
        const overflow = this.lines.length - this.maxLines;
        this.lines.splice(0, overflow);
        trimmed = true;
      }
      if (trimmed) {
        area.value = this.lines.join("\n");
      } else {
        area.value += `${line}
`;
      }
      if (this.nearBottom) {
        area.scrollTop = area.scrollHeight;
      }
    }
    onClear() {
      this.lines = [];
      this.nearBottom = true;
      const area = this.root.getElementById("area");
      if (area) {
        area.value = "";
        area.scrollTop = 0;
      }
    }
    async onCopy() {
      const text = this.lines.join("\n");
      try {
        await navigator.clipboard.writeText(text);
        logFront(`[logs-panel] \u043B\u043E\u0433\u0438 \u0441\u043A\u043E\u043F\u0438\u0440\u043E\u0432\u0430\u043D\u044B \u0432 \u0431\u0443\u0444\u0435\u0440 \u043E\u0431\u043C\u0435\u043D\u0430 (${this.lines.length} \u0441\u0442\u0440\u043E\u043A)`);
      } catch (e) {
        logFront(`[logs-panel] \u043D\u0435 \u0443\u0434\u0430\u043B\u043E\u0441\u044C \u0441\u043A\u043E\u043F\u0438\u0440\u043E\u0432\u0430\u0442\u044C \u043B\u043E\u0433\u0438: ${String(e)}`);
      }
    }
    async onSave() {
      const content = this.lines.join("\n");
      const saved = await saveLogsToFile(content).catch(() => null);
      if (saved)
        logFront(`[logs-panel] \u043B\u043E\u0433\u0438 \u0441\u043E\u0445\u0440\u0430\u043D\u0435\u043D\u044B \u0432 ${saved}`);
    }
  };
  if (!customElements.get("logs-panel")) {
    customElements.define("logs-panel", LogsPanel);
  }

  // ../plugin-log-collector/dist-js/index.js
  function onLogMessage(cb) {
    return listen("logs:message", (e) => cb(e.payload));
  }
  function getLastLogsPath() {
    return invoke("plugin:logs|get_last_logs_path");
  }
  function getLogFilePath() {
    return invoke("plugin:logs|get_log_file_path");
  }
  function readLogTail(maxLines) {
    return invoke("plugin:logs|read_log_tail", { maxLines });
  }
  function logFront(msg) {
    void invoke("plugin:logs|log_frontend_event", { level: "FE", msg }).catch(() => {
    });
  }
  async function logFrontendEvent(level, msg) {
    try {
      await invoke("plugin:logs|log_frontend_event", { level, msg });
    } catch {
    }
  }
  function timestampPart() {
    const d = /* @__PURE__ */ new Date();
    const pad = (n) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}_${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
  }
  async function defaultLogFilename() {
    const ts = timestampPart();
    try {
      const logPath = await getLogFilePath();
      const base = logPath ? logPath.split(/[\\/]/).pop() ?? "" : "";
      const stem = base.replace(/\.[^.]+$/, "");
      if (stem)
        return `${stem}_logs_${ts}.txt`;
    } catch (e) {
      logFront(`[logs-panel] \u043D\u0435 \u0443\u0434\u0430\u043B\u043E\u0441\u044C \u043F\u043E\u043B\u0443\u0447\u0438\u0442\u044C \u0438\u043C\u044F \u043B\u043E\u0433-\u0444\u0430\u0439\u043B\u0430 \u0434\u043B\u044F \u0441\u043E\u0445\u0440\u0430\u043D\u0435\u043D\u0438\u044F: ${String(e)}`);
    }
    return `logs_${ts}.txt`;
  }
  async function saveLogsToFile(content) {
    const path = await save({
      title: "\u0421\u043E\u0445\u0440\u0430\u043D\u0438\u0442\u044C \u043B\u043E\u0433\u0438",
      defaultPath: await defaultLogFilename(),
      filters: [{ name: "\u0422\u0435\u043A\u0441\u0442", extensions: ["log", "txt"] }]
    });
    if (!path)
      return null;
    try {
      await invoke("plugin:logs|save_logs_file", { path, content });
      return path;
    } catch {
      return null;
    }
  }

  // guest-js/web-components.ts
  function errorText(e) {
    if (typeof e === "string") return e;
    if (e instanceof Error) return e.message || String(e);
    if (e && typeof e === "object" && "message" in e) return String(e.message);
    return String(e);
  }
  function logStage(msg) {
    logFront(`[about-updates] ${msg}`);
  }
  function logStageError(msg) {
    void logFrontendEvent("ERROR", `[about-updates] ${msg}`);
  }
  var AboutUpdatesPanel = class extends HTMLElement {
    constructor() {
      super(...arguments);
      this.repo = "";
      this.busy = false;
      /** Обновление, найденное проверкой, но ещё не подтверждённое кликом «Обновить». */
      this.pendingUpdate = null;
      this.historyLoaded = false;
      this.historyOpen = false;
    }
    static get observedAttributes() {
      return ["repo"];
    }
    connectedCallback() {
      this.repo = this.getAttribute("repo") ?? "";
      this.root = this.attachShadow({ mode: "open" });
      this.render();
    }
    attributeChangedCallback() {
      this.repo = this.getAttribute("repo") ?? "";
      if (this.root) this.render();
    }
    setStatus(s) {
      const el = this.root.getElementById("status");
      if (el) el.textContent = s;
    }
    async render() {
      if (!this.root) return;
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
        <div><strong>\u0412\u0435\u0440\u0441\u0438\u044F:</strong> <span id="ver">\u2026</span></div>
        <div class="row">
          <button id="check">\u041F\u0440\u043E\u0432\u0435\u0440\u0438\u0442\u044C \u043E\u0431\u043D\u043E\u0432\u043B\u0435\u043D\u0438\u044F</button>
          <button id="install" class="primary" hidden>\u041E\u0431\u043D\u043E\u0432\u0438\u0442\u044C</button>
          <button id="rollback">\u0418\u0441\u0442\u043E\u0440\u0438\u044F / \u043E\u0442\u043A\u0430\u0442</button>
        </div>
        <div id="status" class="muted" style="margin-top:8px"></div>
        <progress id="install-progress" max="100" hidden></progress>
        <div id="history" style="margin-top:8px" hidden></div>
      </div>`;
      const ver = await getAppVersion().catch(() => "?");
      this.root.getElementById("ver").textContent = ver;
      const supportUrl = await getSupportUrl().catch(() => null);
      if (supportUrl) {
        const sup = document.createElement("button");
        sup.textContent = "\u041F\u043E\u0434\u0434\u0435\u0440\u0436\u0430\u0442\u044C \u0430\u0432\u0442\u043E\u0440\u0430";
        sup.addEventListener("click", () => {
          open(supportUrl).catch(() => window.open(supportUrl, "_blank"));
        });
        this.root.getElementById("rollback").parentElement.appendChild(sup);
      }
      this.root.getElementById("check").addEventListener("click", () => this.onCheck());
      this.root.getElementById("install").addEventListener("click", () => this.onInstall());
      this.root.getElementById("rollback").addEventListener("click", () => this.onRollback());
    }
    setInstallVisible(visible) {
      const el = this.root.getElementById("install");
      if (el) el.hidden = !visible;
    }
    /**
     * Проверка ТОЛЬКО ищет обновление. Установка стартует исключительно из
     * onInstall() по явному клику «Обновить» — правило хостов: auto-install запрещён.
     */
    async onCheck() {
      if (this.busy) return;
      this.busy = true;
      this.setStatus("\u041F\u0440\u043E\u0432\u0435\u0440\u043A\u0430\u2026");
      logStage("\u043F\u0440\u043E\u0432\u0435\u0440\u043A\u0430 \u043E\u0431\u043D\u043E\u0432\u043B\u0435\u043D\u0438\u0439 \u043D\u0430\u0447\u0430\u0442\u0430");
      try {
        const update = await checkForUpdate();
        if (update) {
          this.pendingUpdate = update;
          this.setInstallVisible(true);
          this.setStatus(`\u0414\u043E\u0441\u0442\u0443\u043F\u043D\u043E \u043E\u0431\u043D\u043E\u0432\u043B\u0435\u043D\u0438\u0435 ${update.version}.`);
          logStage(`\u043D\u0430\u0439\u0434\u0435\u043D\u043E \u043E\u0431\u043D\u043E\u0432\u043B\u0435\u043D\u0438\u0435 ${update.version}`);
        } else {
          this.pendingUpdate = null;
          this.setInstallVisible(false);
          this.setStatus("\u0423 \u0432\u0430\u0441 \u043F\u043E\u0441\u043B\u0435\u0434\u043D\u044F\u044F \u0432\u0435\u0440\u0441\u0438\u044F.");
          logStage("\u043E\u0431\u043D\u043E\u0432\u043B\u0435\u043D\u0438\u0439 \u043D\u0435\u0442");
        }
      } catch (e) {
        this.setStatus("\u041E\u0448\u0438\u0431\u043A\u0430: " + errorText(e));
        logStageError(`\u043E\u0448\u0438\u0431\u043A\u0430 \u043F\u0440\u043E\u0432\u0435\u0440\u043A\u0438: ${errorText(e)}`);
      } finally {
        this.busy = false;
      }
    }
    async onInstall() {
      if (this.busy || !this.pendingUpdate) return;
      this.busy = true;
      const bar = this.root.getElementById("install-progress");
      logStage(`\u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043A\u0430 \u043E\u0431\u043D\u043E\u0432\u043B\u0435\u043D\u0438\u044F ${this.pendingUpdate.version} \u043D\u0430\u0447\u0430\u0442\u0430`);
      try {
        let contentLength = null;
        let downloaded = 0;
        this.setStatus("\u0421\u043A\u0430\u0447\u0438\u0432\u0430\u043D\u0438\u0435\u2026");
        if (bar) {
          bar.hidden = false;
          bar.value = 0;
        }
        await downloadAndInstallUpdate(this.pendingUpdate, (e) => {
          if (e.event === "Started") {
            contentLength = e.data.contentLength ?? null;
            if (bar && contentLength) bar.max = contentLength;
            logStage(contentLength ? `\u0441\u043A\u0430\u0447\u0438\u0432\u0430\u043D\u0438\u0435 \u043D\u0430\u0447\u0430\u0442\u043E (${(contentLength / (1024 * 1024)).toFixed(1)} \u041C\u0411)` : "\u0441\u043A\u0430\u0447\u0438\u0432\u0430\u043D\u0438\u0435 \u043D\u0430\u0447\u0430\u0442\u043E");
          } else if (e.event === "Progress") {
            downloaded += e.data.chunkLength;
            if (!bar) return;
            if (contentLength) {
              bar.value = downloaded;
              this.setStatus(`\u0421\u043A\u0430\u0447\u0438\u0432\u0430\u043D\u0438\u0435\u2026 ${Math.min(100, Math.round(downloaded / contentLength * 100))}%`);
            } else {
              this.setStatus(`\u0421\u043A\u0430\u0447\u0438\u0432\u0430\u043D\u0438\u0435\u2026 ${Math.round(downloaded / 1024)} \u041A\u0411`);
            }
          } else if (e.event === "Finished") {
            if (bar) bar.value = bar.max;
            this.setStatus("\u0423\u0441\u0442\u0430\u043D\u043E\u0432\u043A\u0430\u2026");
            logStage("\u0441\u043A\u0430\u0447\u0438\u0432\u0430\u043D\u0438\u0435 \u0437\u0430\u0432\u0435\u0440\u0448\u0435\u043D\u043E, \u0437\u0430\u043F\u0443\u0441\u043A \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043A\u0438 (NSIS)");
          }
        });
        this.setStatus("\u041E\u0431\u043D\u043E\u0432\u043B\u0435\u043D\u0438\u0435 \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043B\u0435\u043D\u043E, \u043F\u0435\u0440\u0435\u0437\u0430\u043F\u0443\u0441\u043A\u2026");
        logStage("\u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043A\u0430 \u0437\u0430\u0432\u0435\u0440\u0448\u0435\u043D\u0430, \u043F\u0435\u0440\u0435\u0437\u0430\u043F\u0443\u0441\u043A\u2026");
      } catch (e) {
        this.setStatus("\u041E\u0448\u0438\u0431\u043A\u0430: " + errorText(e));
        logStageError(`\u043E\u0448\u0438\u0431\u043A\u0430 \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043A\u0438: ${errorText(e)}`);
      } finally {
        if (bar) bar.hidden = true;
        this.busy = false;
      }
    }
    async onRollback() {
      const box = this.root.getElementById("history");
      this.historyOpen = !this.historyOpen;
      box.hidden = !this.historyOpen;
      if (!this.historyOpen || this.historyLoaded) return;
      this.historyLoaded = true;
      this.busy = true;
      this.setStatus("\u0417\u0430\u0433\u0440\u0443\u0437\u043A\u0430 \u0438\u0441\u0442\u043E\u0440\u0438\u0438 \u0440\u0435\u043B\u0438\u0437\u043E\u0432\u2026");
      try {
        const history = await getReleaseHistory();
        box.innerHTML = "";
        const title = document.createElement("div");
        title.className = "muted";
        title.textContent = "\u0414\u043E\u0441\u0442\u0443\u043F\u043D\u044B\u0435 \u0440\u0435\u043B\u0438\u0437\u044B:";
        box.appendChild(title);
        const row = document.createElement("div");
        row.className = "row";
        const select = document.createElement("select");
        const placeholder = document.createElement("option");
        placeholder.value = "";
        placeholder.textContent = "\u2014 \u0432\u044B\u0431\u0435\u0440\u0438\u0442\u0435 \u0432\u0435\u0440\u0441\u0438\u044E \u2014";
        placeholder.disabled = true;
        placeholder.selected = true;
        select.appendChild(placeholder);
        for (const r of history) {
          if (r.isCurrent) continue;
          const opt = document.createElement("option");
          opt.value = r.version;
          opt.textContent = r.pubDate ? `v${r.version} \u2014 ${new Date(r.pubDate).toLocaleDateString()}` : `v${r.version}`;
          opt.dataset.url = r.downloadUrl;
          select.appendChild(opt);
        }
        row.appendChild(select);
        const rollbackBtn = document.createElement("button");
        rollbackBtn.className = "primary";
        rollbackBtn.textContent = "\u041E\u0442\u043A\u0430\u0442\u0438\u0442\u044C";
        rollbackBtn.disabled = true;
        rollbackBtn.addEventListener("click", async () => {
          const selected = select.selectedOptions[0];
          const url = selected?.dataset.url;
          const version = selected?.value;
          if (!url || !version || this.busy) return;
          this.busy = true;
          this.setStatus(`\u041E\u0442\u043A\u0430\u0442 \u043D\u0430 ${version}\u2026`);
          try {
            await installRelease(url, version);
          } catch (e) {
            this.setStatus("\u041E\u0448\u0438\u0431\u043A\u0430 \u043E\u0442\u043A\u0430\u0442\u0430: " + errorText(e));
          } finally {
            this.busy = false;
          }
        });
        select.addEventListener("change", () => {
          rollbackBtn.disabled = select.selectedOptions[0]?.dataset.url ? false : true;
        });
        row.appendChild(rollbackBtn);
        box.appendChild(row);
        if (select.options.length <= 1) {
          const none = document.createElement("div");
          none.className = "muted";
          none.style.marginTop = "8px";
          none.textContent = "\u0420\u0435\u0435\u0441\u0442\u0440 \u0431\u043E\u043B\u0435\u0435 \u0441\u0442\u0430\u0440\u044B\u0445 \u0432\u0435\u0440\u0441\u0438\u0439 \u043F\u0443\u0441\u0442.";
          box.appendChild(none);
        }
        this.setStatus("");
      } catch (e) {
        this.setStatus("\u041E\u0448\u0438\u0431\u043A\u0430: " + errorText(e));
      } finally {
        this.busy = false;
      }
    }
  };
  if (!customElements.get("about-updates-panel")) {
    customElements.define("about-updates-panel", AboutUpdatesPanel);
  }

  // guest-js/index.ts
  async function getReleaseHistory() {
    return invoke("plugin:about-updates|get_release_history");
  }
  async function installRelease(downloadUrl, version) {
    return invoke("plugin:about-updates|install_release", { downloadUrl, version });
  }
  async function getAppVersion() {
    return invoke("plugin:about-updates|get_app_version");
  }
  async function getSupportUrl() {
    return invoke("plugin:about-updates|get_support_url");
  }
  async function checkForUpdate() {
    return check();
  }
  async function downloadAndInstallUpdate(update, onEvent) {
    await update.downloadAndInstall(onEvent);
  }

  // guest-js/iife-entry.ts
  var g4 = window;
  if ("__TAURI__" in window) {
    const tauri = g4.__TAURI__;
    if (tauri && !tauri["about-updates"]) {
      Object.defineProperty(tauri, "about-updates", {
        configurable: true,
        value: {
          checkForUpdate,
          downloadAndInstallUpdate,
          getAppVersion,
          getReleaseHistory,
          getSupportUrl,
          installRelease
        }
      });
    }
  }
})();
