# plugin-tauri-window-icon

🪟 **Обход пробела tauri/tao: Windows taskbar ICON_BIG не выставляется.**

Единственный путь иконки окна в tauri — tao `set_window_icon` — захардкожен на
`IconType::Small` (tao-0.34.8 window.rs:882-891); `IconType::Big` в tao существует, но
никем не вызывается, WM_SETICON в tauri отсутствует даже в 2.12.5 (частичная мера
`default_window_icon_from_app_icon_resource` лишь выбирает дефолтную иконку таскбарного
размера). Кнопка панели задач читает WM_GETICON один раз при создании; при BIG=NULL
Explorer рисует фолбэк из кэша по пути exe, который в дев-сборке протухает при каждой
пересборке → «плавающая заглушка».

**Это первый таури-специфичный плагин репы** — все прочие универсальны по фронтендам:
у GPUI пробела нет (class icon из ресурсов своего exe при каждом запуске,
gpui-pre-windows 0.3.12 platform.rs:1686-1700), поэтому фикс GPUI-хосту не нужен.
Имя маркирует таури-специфичность (решение владельца 23.11.2026).

## Что делает

Хук `on_window_ready` срабатывает в момент создания окна (до `show()` и до создания
кнопки панели) и выставляет окну ICON_SMALL 32×32 + ICON_BIG 256×256 из ресурсов самого
exe — `window-icon-core::apply_window_icons(hwnd)`. Отказ — `log::warn!` с маркером
`[WINDOW_ICON]`, показ окна не блокируется (деградация = прежнее поведение таури).

## Требование к хостам

Окно создаётся скрытым (`visible: false`), показывается хостом когда угодно после setup.

## Подключение

```toml
# Cargo.toml хоста
plugin-tauri-window-icon = { path = "../cross-app-plugins/plugin-tauri-window-icon" }
```

```rust
// main.rs, цепочка .plugin(...)
.plugin(plugin_tauri_window_icon::init())
```

Плагин без команд и без фронтенд-API: permissions не требуются, JS/guest-js нет.
