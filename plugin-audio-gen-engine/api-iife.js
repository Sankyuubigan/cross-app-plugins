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

  // guest-js/shims/event.ts
  var g2 = window;
  async function listen(event, handler) {
    const ev = g2.__TAURI__?.event;
    if (!ev?.listen) {
      throw new Error("window.__TAURI__.event.listen \u043D\u0435\u0434\u043E\u0441\u0442\u0443\u043F\u0435\u043D");
    }
    return ev.listen(event, handler);
  }

  // guest-js/index.ts
  var CMD = (c) => `plugin:audio-gen|${c}`;
  function listModels() {
    return invoke(CMD("audio_gen_list_models"));
  }
  function generate(opts) {
    return invoke(CMD("audio_gen_generate"), { ...opts });
  }
  function status() {
    return invoke(CMD("audio_gen_status"));
  }
  function cancel(jobId) {
    return invoke(CMD("audio_gen_cancel"), { jobId });
  }
  function openOutputDir() {
    return invoke(CMD("audio_gen_open_output_dir"));
  }
  function onEvent(handler) {
    return listen("audio-gen-event", (e) => handler(e.payload));
  }

  // guest-js/iife-entry.ts
  var g3 = window;
  if ("__TAURI__" in window) {
    const tauri = g3.__TAURI__;
    if (tauri && !tauri.audioGen) {
      Object.defineProperty(tauri, "audioGen", {
        configurable: true,
        value: {
          cancel,
          generate,
          listModels,
          onEvent,
          openOutputDir,
          status
        }
      });
    }
  }
})();
