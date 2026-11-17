# Интеграция plugin-audio-gen-engine в хост

## Tauri-хост

1. Cargo.toml хоста:
```toml
plugin-audio-gen-engine = { path = "../cross-app-plugins/plugin-audio-gen-engine" }
```

2. `tauri.conf.json` (все поля опциональны, секция может отсутствовать — Config переживает null):
```json
{
  "app": {
    "security": { "capabilities": { "audio-gen:default": {} } }
  },
  "plugins": {
    "audio-gen": {
      "default_engine_dir": "D:\\nn\\audio.cpp",
      "default_models_dir": "D:\\nn\\audio_models",
      "default_output_dir": "D:\\Projects\\3d_game\\Audio\\Gen"
    }
  }
}
```

3. `.plugin(plugin_audio_gen_engine::init())` в builder.

4. JS (npm-канал): `import { generate, listModels, onEvent } from '@my-tauri-plugins/plugin-audio-gen'`.
Vanilla-канал: подключить `api-iife.js` → `window.__TAURI__.audioGen.*`.
Панель: `import { defineAudioGenPanel } from .../web-components'; defineAudioGenPanel()` → `<audio-gen-panel>`.

## GPUI-хост

Ядро без tauri: `audio-gen-core` напрямую (config_dir — своя папка настроек,
события — `job::set_event_hook`, запуск — собственный tokio-рантайм).

## Установка движка (один раз, Шаг 5 плана)

1. Скачать CUDA-зип audio.cpp ≥ 0.9.5 (cpu — фолбэк; Vulkan на Windows нестабилен на эффектах — не используем).
2. Распаковать в `default_engine_dir` (ожидаем `<engine_dir>/cuda/audiocpp_server.exe` или плоско `<engine_dir>/audiocpp_server.exe`).
3. Веса — в `<models_dir>/<model_id>/*.gguf` (репо audio-cpp/audio.cpp-gguf; имена файлов сверять в репо).
4. Настроить server.json движка (id моделей = id из audio_models.json).
5. Первый запрос свеpить: `GET /v1/models?include_params=true` — уточнить имена полей `options` (duration/steps/seed).

## Игра 3d_game (Godot)

Сгенерированные WAV копируются в `res://Audio/SFX/` и `res://Audio/Music/`,
подключаются через `PlaySoundEffect` (SkillDB) / `AudioManager.play_sfx`.
Лупы музыки: `make_loop: true` → кроссфейд 2.5 с (настраивается).
A/B-тест против программного синтеза — Шаг 5.1 плана `tasks/261116_2218-b2a0cddb/17.11.26 Плагин audio-gen-engine.md`.
