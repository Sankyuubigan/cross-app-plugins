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

  // guest-js/updates.ts
  var state = { hasUpdate: false };
  var subs = /* @__PURE__ */ new Set();
  var started = false;
  function onUpdateMeshState(fn) {
    subs.add(fn);
    fn(state);
    return () => {
      subs.delete(fn);
    };
  }
  function setMeshUpdateState(next) {
    state = next;
    subs.forEach((fn) => fn(state));
  }
  async function checkMeshUpdate() {
    const tag = await checkMeshEngineUpdate();
    const next = tag ? { hasUpdate: true, tag } : { hasUpdate: false };
    setMeshUpdateState(next);
    return next;
  }
  function initMeshUpdateWatcher() {
    if (started) return;
    started = true;
    void checkMeshUpdate().catch(() => {
    });
  }

  // guest-js/components/ui-shared.ts
  var STYLE = `
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
  function formatBytes(n) {
    if (!isFinite(n) || n <= 0) return "0 B";
    const units = ["B", "KB", "MB", "GB"];
    let i = 0;
    while (n >= 1024 && i < units.length - 1) {
      n /= 1024;
      i++;
    }
    return `${n.toFixed(1)} ${units[i]}`;
  }
  function toast(msg, kind = "success") {
    const el = document.createElement("div");
    el.textContent = msg;
    el.style.cssText = `position:fixed; right:16px; bottom:16px; z-index:5000; max-width:420px; padding:10px 14px;border-radius:8px; font-size:13px; color:#fff; background:${kind === "success" ? "rgba(60,140,70,.95)" : "rgba(180,60,60,.95)"};`;
    document.body.appendChild(el);
    setTimeout(() => el.remove(), 4200);
  }
  function progressText(downloaded, total) {
    const pct = total > 0 ? downloaded / total * 100 : 0;
    return `${formatBytes(downloaded)} / ${total > 0 ? formatBytes(total) : "?"}${total > 0 ? ` (${pct.toFixed(0)}%)` : ""}`;
  }

  // guest-js/shims/dialog.ts
  var g2 = window;
  async function open(opts) {
    const dialog = g2.__TAURI__?.dialog;
    if (!dialog?.open) throw new Error("window.__TAURI__.dialog.open \u043D\u0435\u0434\u043E\u0441\u0442\u0443\u043F\u0435\u043D");
    return dialog.open(opts);
  }

  // guest-js/shims/event.ts
  var g3 = window;
  async function listen(name, cb) {
    const event = g3.__TAURI__?.event ?? g3.__TAURI_INTERNALS__?.event;
    if (!event?.listen) throw new Error("window.__TAURI__.event.listen \u043D\u0435\u0434\u043E\u0441\u0442\u0443\u043F\u0435\u043D");
    return event.listen(name, (e) => cb(e));
  }

  // guest-js/components/mesh-engine-panel.ts
  var QUANT_HINTS = {
    q4: "q4 \u2014 ~6 \u0413\u0411 (\u043B\u0451\u0433\u043A\u0438\u0439, \u0437\u0430\u043C\u0435\u0442\u043D\u0430\u044F \u043F\u043E\u0442\u0435\u0440\u044F \u043A\u0430\u0447\u0435\u0441\u0442\u0432\u0430)",
    q8: "q8 \u2014 ~9.5 \u0413\u0411 (\u043F\u043E\u0447\u0442\u0438 \u0431\u0435\u0437 \u043F\u043E\u0442\u0435\u0440\u044C; \u0440\u0435\u043A\u043E\u043C\u0435\u043D\u0434\u0443\u0435\u043C\u044B\u0439)",
    f16: "f16 \u2014 ~16.5 \u0413\u0411 (\u043F\u043E\u043B\u043D\u044B\u0439)"
  };
  var BACKEND_HINTS = {
    auto: "\u0410\u0432\u0442\u043E: NVIDIA \u2192 CUDA (cc<7.5 \u2192 CUDA12-legacy), \u0438\u043D\u0430\u0447\u0435 Vulkan.",
    vulkan: "Vulkan \u2014 \u0443\u043D\u0438\u0432\u0435\u0440\u0441\u0430\u043B\u044C\u043D\u044B\u0439, ~14 \u041C\u0411. \u0420\u0430\u0431\u043E\u0442\u0430\u0435\u0442 \u043D\u0430 \u043B\u044E\u0431\u043E\u043C GPU.",
    cuda: "CUDA \u2014 NVIDIA (715 \u041C\u0411). \u041D\u0443\u0436\u0435\u043D \u0441\u043E\u0432\u0440\u0435\u043C\u0435\u043D\u043D\u044B\u0439 \u0434\u0440\u0430\u0439\u0432\u0435\u0440.",
    cuda12: "CUDA12 \u2014 legacy-\u0440\u0430\u043D\u0442\u0430\u0439\u043C \u0434\u043B\u044F \u0441\u0442\u0430\u0440\u044B\u0445 NVIDIA (cc < 7.5).",
    rocm: "ROCm \u2014 AMD (190 \u041C\u0411). \u0422\u0440\u0435\u0431\u0443\u0435\u0442 TheRock ROCm runtime \u0432 PATH."
  };
  var MeshEnginePanel = class extends HTMLElement {
    connectedCallback() {
      if (this.root) return;
      this.root = this.attachShadow({ mode: "open" });
      this.root.innerHTML = `
      <style>${STYLE}</style>
      <div>
        <div class="row">
          <label>\u0421\u0442\u0430\u0442\u0443\u0441:</label><span id="status" class="hint">\u041F\u0440\u043E\u0432\u0435\u0440\u043A\u0430\u2026</span><span id="updateBadge" style="display:none; margin-left:8px; color:#e6a23c;">\u{1F504}</span>
        </div>
        <div class="row"><label>\u041F\u0443\u0442\u044C:</label><span id="path" class="hint" style="word-break:break-all; flex:1;"></span></div>
        <div class="row"><label>\u0412\u0435\u0441\u0430:</label><span id="weights" class="hint" style="flex:1;"></span></div>
        <div class="row" style="margin-top:8px;">
          <label>\u041A\u0432\u0430\u043D\u0442:</label>
          <select id="quant" style="flex:1; min-width:220px;"></select>
        </div>
        <div class="row">
          <label>\u0411\u0435\u043A\u0435\u043D\u0434:</label>
          <select id="backend" style="flex:1; min-width:220px;"></select>
        </div>
        <div id="hint" class="hint"></div>
        <div class="row" style="margin-top:10px;">
          <button id="install" class="primary">\u0423\u0441\u0442\u0430\u043D\u043E\u0432\u0438\u0442\u044C</button>
          <button id="checkUpdate" class="secondary" style="display:none;">\u041F\u0440\u043E\u0432\u0435\u0440\u0438\u0442\u044C \u043E\u0431\u043D\u043E\u0432\u043B\u0435\u043D\u0438\u0435</button>
          <button id="installUpdate" class="primary" style="display:none;">\u041E\u0431\u043D\u043E\u0432\u0438\u0442\u044C</button>
          <button id="setDir" class="secondary">\u0418\u0437\u043C\u0435\u043D\u0438\u0442\u044C \u043F\u0443\u0442\u044C</button>
        </div>
        <div id="progress" class="progress-container">
          <div class="progress-status" id="progressStatus">0 MB / 0 MB</div>
          <div class="progress-track"><div class="progress-bar" id="progressBar"></div></div>
        </div>
        <hr style="border:none; border-top:1px solid rgba(128,128,128,.35); margin:14px 0 10px;" />
        <div class="row"><label><b>\u0420\u0438\u0433\u0433\u0438\u043D\u0433</b> (\u0441\u043A\u0435\u043B\u0435\u0442 + \u0432\u0435\u0441\u0430 \u0441\u043A\u0438\u043D\u043D\u0438\u043D\u0433\u0430 \u0434\u043B\u044F GLB):</label><span id="rigStatus" class="hint">\u041F\u0440\u043E\u0432\u0435\u0440\u043A\u0430\u2026</span></div>
        <div class="row"><label>\u041F\u0443\u0442\u044C:</label><span id="rigPath" class="hint" style="word-break:break-all; flex:1;"></span></div>
        <div class="row"><label>\u0412\u0435\u0441\u0430:</label><span id="rigWeights" class="hint" style="word-break:break-all; flex:1;"></span></div>
        <div id="rigHint" class="hint"></div>
        <div class="row" style="margin-top:8px;">
          <button id="rigInstall" class="primary">\u0421\u043A\u0430\u0447\u0430\u0442\u044C \u0432\u0435\u0441\u0430 \u0440\u0438\u0433\u0433\u0438\u043D\u0433\u0430 (1.25 \u0413\u0411)</button>
          <button id="rigSetDir" class="secondary">\u0418\u0437\u043C\u0435\u043D\u0438\u0442\u044C \u043F\u0443\u0442\u044C</button>
        </div>
        <div id="rigProgress" class="progress-container">
          <div class="progress-status" id="rigProgressStatus">0 MB / 0 MB</div>
          <div class="progress-track"><div class="progress-bar" id="rigProgressBar"></div></div>
        </div>
      </div>`;
      this.root.getElementById("backend").addEventListener("change", () => this.renderHint());
      this.root.getElementById("install").addEventListener("click", () => void this.onInstall());
      this.root.getElementById("checkUpdate").addEventListener("click", () => void this.onCheckUpdate());
      this.root.getElementById("installUpdate").addEventListener("click", () => void this.onInstall());
      this.root.getElementById("setDir").addEventListener("click", () => void this.onSetDir());
      this.root.getElementById("rigInstall").addEventListener("click", () => void this.onRigInstall());
      this.root.getElementById("rigSetDir").addEventListener("click", () => void this.onRigSetDir());
      this.unsubUpdate = onUpdateMeshState((s) => this.renderUpdateState(s));
      void this.refresh();
      void listen(
        "downloader:progress",
        (e) => {
          const p = e.payload;
          if (p.kind === "mesh-engine") {
            this.setProgress(p.downloaded, p.total);
            if (p.status === "done" || p.status === "error") {
              setTimeout(() => this.showProgress(false), 400);
            }
          } else if (p.kind === "rig-engine") {
            this.rigSetProgress(p.downloaded, p.total);
            if (p.status === "done" || p.status === "error") {
              setTimeout(() => this.showRigProgress(false), 400);
            }
          }
        }
      ).then((u) => {
        this.unlisten = u;
      }).catch(() => {
      });
    }
    disconnectedCallback() {
      this.unlisten?.();
      this.unsubUpdate?.();
    }
    setStatus(s) {
      const el = this.root.getElementById("status");
      if (el) el.textContent = s;
    }
    setProgress(downloaded, total) {
      const bar = this.root.getElementById("progressBar");
      bar.style.width = `${total > 0 ? downloaded / total * 100 : 0}%`;
      const st = this.root.getElementById("progressStatus");
      st.textContent = progressText(downloaded, total);
    }
    showProgress(on) {
      this.root.getElementById("progress").style.display = on ? "block" : "none";
    }
    weightsSummary(st) {
      if (!st.weights.length) return "\u2014";
      return st.weights.map((w) => `${w.quant}: ${w.ggufCount} \u0444\u0430\u0439\u043B\u043E\u0432${w.pixal3dFiles ? `, pixal3d: ${w.pixal3dFiles}` : ""}`).join(", ");
    }
    renderHint() {
      const backend = this.root.getElementById("backend").value;
      const hint = this.root.getElementById("hint");
      hint.textContent = BACKEND_HINTS[backend] ?? "";
    }
    // ── Секция «Риггинг» (skin-tokens.cpp): статус + докачка 3 весов F16 ──
    rigSetProgress(downloaded, total) {
      const bar = this.root.getElementById("rigProgressBar");
      bar.style.width = `${total > 0 ? downloaded / total * 100 : 0}%`;
      const st = this.root.getElementById("rigProgressStatus");
      st.textContent = progressText(downloaded, total);
    }
    showRigProgress(on) {
      this.root.getElementById("rigProgress").style.display = on ? "block" : "none";
    }
    renderRig(st) {
      this.root.getElementById("rigPath").textContent = st.dir || "\u2014";
      this.root.getElementById("rigWeights").textContent = st.weightsDir ? `${st.weightsPresent}/${st.weightsTotal} (F16) \u2014 ${st.weightsDir}` : "\u2014";
      const status = this.root.getElementById("rigStatus");
      const hint = this.root.getElementById("rigHint");
      if (st.ready) {
        status.textContent = "\u0413\u043E\u0442\u043E\u0432 \u043A \u0440\u0438\u0433\u0443";
        hint.textContent = "";
      } else {
        const missing = [];
        if (!st.exePresent) missing.push("skintokens-cli.exe");
        if (!st.dllsPresent) missing.push("\u0440\u0430\u043D\u0442\u0430\u0439\u043C-DLL \u0434\u0432\u0438\u0436\u043A\u0430");
        if (st.weightsPresent < st.weightsTotal) missing.push(`\u0432\u0435\u0441\u0430 (${st.weightsPresent}/${st.weightsTotal})`);
        status.textContent = `\u041D\u0435 \u0433\u043E\u0442\u043E\u0432: ${missing.join(", ")}`;
        hint.textContent = st.exePresent ? "" : "CLI \u0440\u0438\u0433\u0433\u0438\u043D\u0433\u0430 \u043D\u0435 \u0441\u043A\u0430\u0447\u0438\u0432\u0430\u0435\u0442\u0441\u044F \u043F\u0430\u043D\u0435\u043B\u044C\u044E (\u043F\u0440\u0435\u0431\u0438\u043B\u0434\u043E\u0432 upstream \u043D\u0435\u0442) \u2014 exe/DLL \u043A\u043B\u0430\u0434\u0443\u0442\u0441\u044F \u0432 \u043F\u0430\u043F\u043A\u0443 \u0432\u0440\u0443\u0447\u043D\u0443\u044E \u0438\u0437 \u043B\u043E\u043A\u0430\u043B\u044C\u043D\u043E\u0439 \u0441\u0431\u043E\u0440\u043A\u0438 skin-tokens.cpp.";
      }
      const btn = this.root.getElementById("rigInstall");
      btn.textContent = st.weightsPresent === st.weightsTotal ? "\u0414\u043E\u043A\u0430\u0447\u0430\u0442\u044C \u0432\u0435\u0441\u0430 \u0440\u0438\u0433\u0433\u0438\u043D\u0433\u0430" : "\u0421\u043A\u0430\u0447\u0430\u0442\u044C \u0432\u0435\u0441\u0430 \u0440\u0438\u0433\u0433\u0438\u043D\u0433\u0430 (1.25 \u0413\u0411)";
    }
    async refreshRig() {
      try {
        this.renderRig(await getRigEngineStatus());
      } catch (e) {
        this.root.getElementById("rigStatus").textContent = `\u041E\u0448\u0438\u0431\u043A\u0430: ${e}`;
      }
    }
    async onRigInstall() {
      const btn = this.root.getElementById("rigInstall");
      btn.disabled = true;
      this.showRigProgress(true);
      this.root.getElementById("rigProgressStatus").textContent = "\u041F\u043E\u0434\u0433\u043E\u0442\u043E\u0432\u043A\u0430\u2026";
      try {
        const st = await installRigEngine();
        this.renderRig(st);
        toast("\u0412\u0435\u0441\u0430 \u0440\u0438\u0433\u0433\u0438\u043D\u0433\u0430 \u0441\u043A\u0430\u0447\u0430\u043D\u044B!");
      } catch (e) {
        toast(`\u041E\u0448\u0438\u0431\u043A\u0430 \u0441\u043A\u0430\u0447\u0438\u0432\u0430\u043D\u0438\u044F \u0432\u0435\u0441\u043E\u0432 \u0440\u0438\u0433\u0433\u0438\u043D\u0433\u0430: ${e}`, "error");
      } finally {
        btn.disabled = false;
        this.showRigProgress(false);
        void this.refreshRig();
      }
    }
    async onRigSetDir() {
      try {
        const sel = await open({ directory: true, title: "\u041F\u0430\u043F\u043A\u0430 \u0434\u0432\u0438\u0436\u043A\u0430 \u0440\u0438\u0433\u0433\u0438\u043D\u0433\u0430" });
        if (!sel) return;
        const path = Array.isArray(sel) ? sel[0] : sel;
        if (!path) return;
        const st = await setRigEngineDir(path);
        this.renderRig(st);
        toast("\u041F\u0443\u0442\u044C \u0440\u0438\u0433\u0433\u0438\u043D\u0433\u0430 \u0438\u0437\u043C\u0435\u043D\u0451\u043D.");
      } catch (e) {
        toast(`\u041E\u0448\u0438\u0431\u043A\u0430 \u0438\u0437\u043C\u0435\u043D\u0435\u043D\u0438\u044F \u043F\u0443\u0442\u0438 \u0440\u0438\u0433\u0433\u0438\u043D\u0433\u0430: ${e}`, "error");
      }
    }
    async refresh() {
      let st;
      try {
        st = await getMeshEngineStatus();
      } catch (e) {
        this.setStatus(`\u041E\u0448\u0438\u0431\u043A\u0430: ${e}`);
        return;
      }
      this.setStatus(st.installed ? "\u0414\u0432\u0438\u0436\u043E\u043A \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043B\u0435\u043D" : "\u0414\u0432\u0438\u0436\u043E\u043A \u043D\u0435 \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043B\u0435\u043D");
      this.root.getElementById("path").textContent = st.dir || "\u2014";
      this.root.getElementById("weights").textContent = this.weightsSummary(st);
      const quant = this.root.getElementById("quant");
      quant.innerHTML = "";
      for (const q of ["q8", "q4", "f16"]) {
        const o = document.createElement("option");
        o.value = q;
        o.textContent = QUANT_HINTS[q] ?? q;
        quant.appendChild(o);
      }
      const backend = this.root.getElementById("backend");
      backend.innerHTML = "";
      for (const b of ["auto", "vulkan", "cuda", "cuda12", "rocm"]) {
        const o = document.createElement("option");
        o.value = b;
        o.textContent = b === "auto" ? "\u0410\u0432\u0442\u043E (\u0440\u0435\u043A\u043E\u043C\u0435\u043D\u0434\u0443\u0435\u0442\u0441\u044F)" : b;
        backend.appendChild(o);
      }
      this.renderHint();
      const installBtn = this.root.getElementById("install");
      installBtn.textContent = st.installed ? "\u0414\u043E\u043A\u0430\u0447\u0430\u0442\u044C / \u043E\u0431\u043D\u043E\u0432\u0438\u0442\u044C" : "\u0423\u0441\u0442\u0430\u043D\u043E\u0432\u0438\u0442\u044C";
      const checkBtn = this.root.getElementById("checkUpdate");
      checkBtn.style.display = st.installed ? "" : "none";
      void this.refreshRig();
    }
    renderUpdateState(s) {
      const badge = this.root.getElementById("updateBadge");
      badge.style.display = s.hasUpdate ? "" : "none";
      const updBtn = this.root.getElementById("installUpdate");
      updBtn.style.display = s.hasUpdate ? "" : "none";
    }
    async onCheckUpdate() {
      const btn = this.root.getElementById("checkUpdate");
      btn.disabled = true;
      try {
        const s = await checkMeshUpdate();
        if (s.hasUpdate) toast(`\u0414\u043E\u0441\u0442\u0443\u043F\u043D\u043E \u043E\u0431\u043D\u043E\u0432\u043B\u0435\u043D\u0438\u0435 3D-\u0434\u0432\u0438\u0436\u043A\u0430: ${s.tag}`);
        else toast("\u0414\u0432\u0438\u0436\u043E\u043A 3D-\u0433\u0435\u043D\u0435\u0440\u0430\u0446\u0438\u0438 \u0430\u043A\u0442\u0443\u0430\u043B\u0435\u043D");
      } catch (e) {
        toast(`\u041D\u0435 \u0443\u0434\u0430\u043B\u043E\u0441\u044C \u043F\u0440\u043E\u0432\u0435\u0440\u0438\u0442\u044C \u043E\u0431\u043D\u043E\u0432\u043B\u0435\u043D\u0438\u044F: ${e}`, "error");
      } finally {
        btn.disabled = false;
      }
    }
    async onInstall() {
      const btn = this.root.getElementById("install");
      btn.disabled = true;
      this.showProgress(true);
      this.root.getElementById("progressStatus").textContent = "\u041F\u043E\u0434\u0433\u043E\u0442\u043E\u0432\u043A\u0430\u2026";
      const quant = this.root.getElementById("quant").value;
      const backend = this.root.getElementById("backend").value;
      try {
        const st = await installMeshEngine({ backend, quant });
        this.setStatus(st.installed ? "\u0414\u0432\u0438\u0436\u043E\u043A \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043B\u0435\u043D" : "\u0414\u0432\u0438\u0436\u043E\u043A \u043D\u0435 \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043B\u0435\u043D");
        this.root.getElementById("weights").textContent = this.weightsSummary(st);
        await checkMeshUpdate().catch(() => {
        });
        toast("\u0414\u0432\u0438\u0436\u043E\u043A 3D-\u0433\u0435\u043D\u0435\u0440\u0430\u0446\u0438\u0438 \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043B\u0435\u043D!");
      } catch (e) {
        toast(`\u041E\u0448\u0438\u0431\u043A\u0430 \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043A\u0438 \u0434\u0432\u0438\u0436\u043A\u0430: ${e}`, "error");
      } finally {
        btn.disabled = false;
        this.showProgress(false);
        await this.refresh();
      }
    }
    async onSetDir() {
      try {
        const sel = await open({ directory: true, title: "\u041F\u0430\u043F\u043A\u0430 \u0434\u0432\u0438\u0436\u043A\u0430 3D-\u0433\u0435\u043D\u0435\u0440\u0430\u0446\u0438\u0438" });
        if (!sel) return;
        const path = Array.isArray(sel) ? sel[0] : sel;
        if (!path) return;
        const st = await setMeshEngineDir(path);
        this.root.getElementById("path").textContent = st.dir || "\u2014";
        this.setStatus(st.installed ? "\u0414\u0432\u0438\u0436\u043E\u043A \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043B\u0435\u043D" : "\u0414\u0432\u0438\u0436\u043E\u043A \u043D\u0435 \u0443\u0441\u0442\u0430\u043D\u043E\u0432\u043B\u0435\u043D");
        this.root.getElementById("weights").textContent = this.weightsSummary(st);
        toast("\u041F\u0443\u0442\u044C \u0434\u0432\u0438\u0436\u043A\u0430 \u0438\u0437\u043C\u0435\u043D\u0451\u043D.");
      } catch (e) {
        toast(`\u041E\u0448\u0438\u0431\u043A\u0430 \u0438\u0437\u043C\u0435\u043D\u0435\u043D\u0438\u044F \u043F\u0443\u0442\u0438: ${e}`, "error");
      }
    }
  };
  if (!customElements.get("mesh-engine-panel")) {
    customElements.define("mesh-engine-panel", MeshEnginePanel);
  }

  // guest-js/index.ts
  function getMeshEngineStatus() {
    return invoke("plugin:mesh-engine|mesh_engine_status");
  }
  function meshGenerate(args) {
    return invoke("plugin:mesh-engine|mesh_generate", { args });
  }
  function installMeshEngine(args) {
    return invoke("plugin:mesh-engine|mesh_engine_install", { args });
  }
  function setMeshEngineDir(path) {
    return invoke("plugin:mesh-engine|mesh_engine_set_dir", { path });
  }
  function checkMeshEngineUpdate() {
    return invoke("plugin:mesh-engine|mesh_engine_check_update");
  }
  function getRigEngineStatus() {
    return invoke("plugin:mesh-engine|mesh_rig_engine_status");
  }
  function installRigEngine() {
    return invoke("plugin:mesh-engine|mesh_rig_engine_install");
  }
  function setRigEngineDir(path) {
    return invoke("plugin:mesh-engine|mesh_rig_engine_set_dir", { path });
  }

  // guest-js/iife-entry.ts
  if ("__TAURI__" in window) {
    const tauri = window.__TAURI__;
    if (tauri && !tauri["mesh-engine"]) {
      Object.defineProperty(tauri, "mesh-engine", {
        configurable: true,
        value: {
          checkMeshEngineUpdate,
          getMeshEngineStatus,
          initMeshUpdateWatcher,
          installMeshEngine,
          meshGenerate,
          setMeshEngineDir
        }
      });
    }
  }
})();
