//! Чистое ядро mesh-плагина (PLUGIN_STANDARD §0.1): image→3D через ОТДЕЛЬНЫЙ
//! процесс `trellis-server.exe` (Windows-пребилд pwilkin/trellis.cpp),
//! общение по HTTP localhost — случайный порт, `GET /health` + `POST
//! /generate` multipart → `model/gltf-binary`. Приложение НЕ линкует
//! trellis.cpp нативно. БЕЗ tauri: все пути — явные параметры (GPUI-хост
//! зовёт ядро напрямую).
//!
//! Контракт сервера верифицирован по исходникам (16.11.2026,
//! src/trellis-server.cpp + src/trellis_args.cpp): авторизации нет —
//! изоляция строится на привязке к 127.0.0.1 и случайном порту; поля
//! multipart: image (файл) + seed/resolution/bg_removal/faces (текст);
//! успех — `model/gltf-binary`; ошибки — JSON `{"error": "..."}` со
//! статусом 400/500.
//!
//! Веса (GGUF): `<engine_dir>/models/<q4|q8|f16>/` — ilintar/trellis2-gguf
//! (+ pixal3d_*.gguf из vegax87/Pixal3D в ту же папку, декодеры общие).

pub mod engine;
