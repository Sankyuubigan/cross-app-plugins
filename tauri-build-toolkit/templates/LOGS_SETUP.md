# Подключение plugin-log-collector (единый лог вывода приложения)

Виджет «Логи» вынесен в переиспользуемый плагин `plugin-log-collector`
(репозиторий: `D:\Projects\cross-app-plugins\plugin-log-collector`).
Плагин: один `log::Log` (включая host RSL при дебаге), файл `king_orch.log` рядом
с exe, dev-зеркало `test/last_logs.txt`, «Последние логи» в Store, опциональный
облачный репорт и аналитика Aptabase (поле `project`).

## Шаги подключения

### 1. Rust-зависимость (`src-tauri/Cargo.toml`)
```toml
plugin-log-collector = { path = "../../cross-app-plugins/plugin-log-collector" }
log = "0.4"
```

### 2. Конфиг (`src-tauri/tauri.conf.json` → `app.plugins.logs`)
```json
"plugins": {
  "logs": {
    "log_file_name": "king_orch.log",
    "last_logs": true,
    "reporting": {
      "enabled": true,
      "project": "<project-slug>",
      "app_key": "<aptabase-key>",
      "events": true
    }
  }
}
```

### 3. Капабилити (`src-tauri/capabilities/default.json`)
```json
"permissions": ["core:default", "logs:default"]
```

### 4. Инициализация в Rust (`src-tauri/src/main.rs` или `lib.rs`)
Плагин Wry-only: `init()` не-generic.
```rust
tauri::Builder::default()
    .plugin(plugin_log_collector::init())
    .setup(|app| {
        // выключить облако, пока не зарегистрирован app_key
        plugin_log_collector::set_reporting_enabled(false);
        plugin_log_collector::track_event("app_started", None);
        Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

### 5. Логирование на бэкенде
```rust
log::info!/warn!/error!("...")                    // локально
plugin_log_collector::track_event("имя", Some(json!({...})))  // локально + облако
```

### 6. Фронтенд (`package.json`)
```json
"dependencies": { "@my-tauri-plugins/plugin-logs": "file:../cross-app-plugins/plugin-log-collector" }
```
```ts
import "@my-tauri-plugins/plugin-logs";           // side-effect: ловит ошибки UI
import { logFront, onLogMessage, clearLogs, setReportingEnabled } from "@my-tauri-plugins/plugin-logs";
```
Виджет: `<logs-panel></logs-panel>` (web-component), стили — в вашем style.css.

## Примечания
- `reporting.enabled` включается с рабочего места: до этого облако молчит, файл живёт всегда.
- Файл лога кладётся рядом с exe через `current_exe().parent()` (правило cwd, см. desktop_rust_tauri_rules.md §2 в global_ai_docs).
- Exe-файл лога и dev-зеркало `test/last_logs.txt` (пишется только если рядом с exe есть каталог `test/`) обнуляются на старте сессии (truncate, §2.5.1) — каждый запуск чистая история, роста между сессиями нет.