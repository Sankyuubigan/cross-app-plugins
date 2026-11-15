//! Чистое ядро media-gen-плагина (ССОТ, Шаг 4f PRP 261115): инференс изображений/
//! видео через ОТДЕЛЬНЫЙ процесс `sd-server.exe` (полный релиз
//! leejet/stable-diffusion.cpp), общение по HTTP localhost (random-порт,
//! нативный async API `/sdcpp/v1/img_gen` + poll jobs). Приложение НЕ линкует
//! stable-diffusion.cpp нативно. БЕЗ tauri.
//!
//! Плагин владеет: установкой/обновлением выбранного варианта бекенда
//! (cuda12 / cpu / vulkan / rocm), каталогами `image_models_catalog.json`/
//! `video_models_catalog.json` (бандлы: diffusion + vae + text encoder + mmproj
//! с пресетами и порогами VRAM), скачиванием бандла, VRAM-preflight,
//! пулом весов и генерацией/редактированием. Tauri-фасад — в crate
//! `plugin-media-gen-engine`; GPUI-хосты берут это ядро напрямую.
//!
//! Скачивание — единый движок проекта `downloader-core` (деп ядра).

pub mod engine;
pub mod pool;
