# plugin-audio-gen-engine

Переиспользуемый Tauri-плагин **локальной генерации звуков и музыки** через движок
[0xShug0/audio.cpp](https://github.com/0xShug0/audio.cpp) (Apache-2.0) — без Python,
без облака. Назначение в проекте: генерация SFX и фоновых лупов для игры 3d_game
(Godot), но плагин универсален (PLUGIN_STANDARD §0.1).

## Модели (реестр `audio_models.json`)

| id | Модель | Назначение | Лицензия | VRAM |
|---|---|---|---|---|
| `sa3-medium` | Stable Audio 3.0 Medium (1.4B) | SFX + инструментальная музыка | Stability Community (<$1M/год) | ~6 ГБ |
| `moss-sfx-v2` | MOSS-SoundEffect v2.0 | SFX без порогов, 48 кГц | Apache-2.0 | ~4 ГБ |
| `ace-step-1.5` | ACE-Step 1.5 | Песни/вокал | MIT | 4–8 ГБ |

ЗАПРЕЩЕНЫ (NC-лицензии, см. план): AudioCraft, HunyuanVideo-Foley, TangoFlux, MMAudio, ElevenLabs.

## Архитектура

```
src/       фасад tauri-плагина (init<R:Runtime>, null-safe Config, event-мост audio-gen-event)
core/      audio-gen-core — ЧИСТОЕ ядро без tauri (ССОТ):
           config.rs (audio_gen_settings.json через json-store)
           models.rs (вшитый реестр + детект установки .gguf)
           engine.rs (спавн audiocpp_server через engine-proc-util, HTTP /v1/audio/speech)
           wav.rs   (нормализация -1 dBFS, луп-кроссфейд, i16 PCM, hound)
           job.rs   (очередь, прогресс-хук, отмена по PID-дереву)
guest-js/  TS API + <audio-gen-panel> (Shadow DOM) + shims window.__TAURI__
dist-js/, api-iife.js — коммитятся (два канала, PLUGIN_STANDARD §5)
```

## Команды (invoke `plugin:audio-gen|<cmd>`)

- `audio_gen_list_models` — реестр моделей + флаг установки;
- `audio_gen_generate` {modelId, prompt, durationSecs?, steps?, seed?, makeLoop?, name?} → jobId;
- `audio_gen_status` — снимок задач + резолвленные пути;
- `audio_gen_cancel` {jobId} — kill сервера по PID-дереву;
- `audio_gen_open_output_dir` — открыть папку вывода.

Прогресс: событие `audio-gen-event` {jobId, status, progress}.

## Интеграция хоста

См. [docs/INTEGRATION.md](docs/INTEGRATION.md).

## Сборка

```bash
npm install && npm run build && npm run build:global   # dist-js + api-iife.js (коммитятся)
cargo check -p audio-gen-core            # ядро без tauri
test-plugin.bat audio-gen-core           # тесты ядра (конвенция репозитория)
```

## Статус (scaffold 17.11.2026)

Готово: каркас, ядро (7/7 тестов), фасад, JS-каналы, permissions, реестр моделей.
Перед боевой генерацией (Шаг 5 плана): установить движок audio.cpp (CUDA-зип) и веса,
настроить engine_dir (server.json), сверить поля `options` запроса с живым сервером
(`/v1/models?include_params=true`).
