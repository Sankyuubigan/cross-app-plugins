# Переиспользуемые модули как Tauri-плагины (репозиторий `cross-app-plugins`)

> **Назначение доки:** подробный, воспроизводимый инженерный референс по тому, как выносить
> переиспользуемый модуль (бизнес-логику **и** GUI) из Tauri-приложения в отдельный репозиторий
> `cross-app-plugins` в виде **Tauri-плагина** (Rust-крейт + npm-пакет + Web Component). Написано
> так, чтобы по этой доке архитектуру можно было **один-в-один повторить в любом другом
> Rust/Tauri-проекте**.
>
> **Эталонная реализация ( reference implementation ):** `plugin-log-collector` (модуль «Логи»:
> один `log::Log`, файл лога + зеркало `test/last_logs.txt`, UI-вкладка через web-component
> `<logs-panel>`) — первый плагин, доведённый до стандарта BOTH-канала (npm **и** vanilla).
> `plugin-about-updates` и `plugin-speech-gen-engine` следуют тому же стандарту.
>
> **Ключевой принцип:** модуль переиспользуется как **compile-time зависимость** (path-зависимость
> в `Cargo.toml`), GUI переиспользуется через **Web Component** (фреймворк-агностично). JS-сторона
> плагина доставляется хосту **двумя каналами из одного источника** — см. §5.4.

---

## 0. Архитектурная сводка

```
cross-app-plugins/                      ← отдельный git-репозиторий (НЕ внутри хост-проекта)
├─ Cargo.toml                          ← workspace: members + [workspace.dependencies]
├─ PLUGIN_STANDARD.md                  ← эта дока (SSOT инженерного стандарта)
└─ plugin-log-collector/                  ← один плагин = крейт + npm-пакет + iife-глобал
   ├─ Cargo.toml                       ← links + build-dep tauri-plugin
   ├─ build.rs                         ← COMMANDS + Bundle.New(COMMANDS).global_api_script_path(...)
   ├─ src/                             ← Rust: lib.rs, commands.rs, models.rs, logger.rs, path.rs ...
   ├─ permissions/default.toml         ← allow-* + [default]
   ├─ package.json                     ← @my-tauri-plugins/plugin-logs (+ esbuild devDep)
   ├─ tsconfig.json
   ├─ guest-js/
   │  ├─ index.ts                      ← типизированный TS-API (invoke) + side-effect import WC
   │  ├─ web-components.ts             ← <logs-panel> (Shadow DOM + CSS-переменные)
   │  └─ iife-entry.ts                 ← точка входа для esbuild (см. §5.4)
   ├─ dist-js/                         ← npm-канал (tsc, КОММИТИТСЯ, артефакт)
   └─ api-iife.js                      ← vanilla-канал (esbuild, КОММИТИТСЯ, артефакт)

repos-control/                         ← ХОСТ (vanilla, без npm/бандлера)
├─ src-tauri/
│  ├─ Cargo.toml                       ← path = "../../cross-app-plugins/plugin-log-collector"
│  ├─ main.rs                          ← .plugin(plugin_log_collector::init())
│  ├─ tauri.conf.json                  ← "plugins": { "logs": {...} }, "withGlobalTauri": true
│  └─ capabilities/default.json        ← "logs:default"
└─ ui/index.html                       ← <logs-panel></logs-panel> — и всё (JS плагина вшит Tauri)
```

**Три имени плагина (важно не путать):**
| Что | Пример | Где используется |
|-----|--------|-------------------|
| Cargo-пакет | `plugin-log-collector` | имя папки/крейта, `links` в `Cargo.toml` |
| Rust-lib ( crate name ) | `plugin_log_collector` | `plugin_log_collector::init()` в хосте |
| plugin-identifier | `logs` | `Builder::new("logs")` → invoke `plugin:logs\|<cmd>` и capability `logs:default` |
| npm-пакет | `@my-tauri-plugins/plugin-logs` | `import` во фронтенде bundler-хоста |

---

## 0.1 Универсальный плагин (ОБЯЗАТЕЛЬНЫЙ стандарт, 15.11.26)

**Каждый новый плагин обязан быть универсальным:** работать и в Tauri-хостах (King Orch, vanilla/bundler),
и в не-Tauri фронтендах (GPUI/egui — VibeBridge). Плагин, вызываемый только через `AppHandle`,
универсальным не считается.

Структура (на примере любого плагина):

```
plugin-<domain>/                  ← фасад (крейт с tauri)
├─ Cargo.toml                     ← links = "tauri-plugin-<старый-id>" (см. §4.1 — НЕ имя пакета!)
├─ core/                          ← ЯДРО: крейт <domain>-core БЕЗ tauri (SSOT всей логики)
│  ├─ Cargo.toml                  ← никаких tauri/tauri-plugin-зависимостей
│  └─ src/                        ← вся бизнес-логика + юнит-тесты
└─ src/                           ← тонкий фасад: pub use core::* + AppHandle-мосты
```

Правила:

1. **Ядро (`core/`) без tauri.** Вся логика, команды-«рабочие лошадки», конфиг-работа — в ядре.
   Вместо `AppHandle` — инъекции: колбэки/хуки (`set_progress_hook`, `set_emit_hook`,
   `set_fallback_downloader`, `set_track_hook`), пути параметром (`data_dir: &Path`).
   Тесты ядра не требуют Tauri-окружения.
2. **Фасад тонкий:** `pub use core::*;` + `#[tauri::command]`-обёртки + AppHandle-мосты.
   Мост обязан давать **ОБЕ сигнатуры**: старую с `AppHandle` (Tauri-хосты не переписываются) и
   новую без (GPUI-хосты зовут ядро напрямую). Явные `pub fn` затеняют glob-реэкспорт —
   glob + явные мосты в одном модуле работают (прецедент: `engine::config::load_config`).
   Одинаковые имена/форматы событий (emit `x:progress` и т.п.) сохраняются — JS-канал не меняется.
3. **Фронтенд — два носителя из одного стандарта:**
   - Tauri-хосты: Web Component (`guest-js/` + `api-iife.js`, §5) — как раньше;
   - GPUI-хосты: карточка в `gpui-kit` за фичей (`log-collector`, `llama`, `about-updates`, …),
     реэкспорт через `my-gpui-kit` — хост не линкует tauri.
   Логика UI дублируется минимально: WC — рендер, GPUI-карточка — рендер; состояние/вызовы — ядро.
4. **Событие/конфиг/пути**: конфиг-значения ядро принимает параметрами; фасад достаёт их из
   `tauri.conf.json` (`api.config()`) и передаёт в ядро. Дефолты — в ядре.
5. **Что НЕ переносить в ядро:** `app.exit()`, диалоги окна, реєстрацию плагина, emit-протокол —
   фасад (3-5 строк на мост).
6. **Эталоны паттерна:** `plugin-llama-engine` и `plugin-log-collector` (ядро+фасад+мосты),
   `gpui-kit` — носитель GPUI-фронтенда. Новые плагины делать строго по ним.

---

## 1. Репозиторий `cross-app-plugins`

Это **отдельный** git-репозиторий (sibling хост-проектов), а не сабмодуль и не папка внутри проекта.
Каждый плагин — отдельный workspace-member.

`cross-app-plugins/Cargo.toml` (workspace с общими зависимостями):
```toml
[workspace]
resolver = "2"
members = ["plugin-about-updates", "plugin-speech-gen-engine", "plugin-log-collector", "json-store"]

[workspace.dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
fs2 = "0.4"
# ... остальное общее (gix, chrono, anyhow, аудио-либы speech и т.п.)
```

Каждый плагин сам содержит и Rust-крейт, и npm-пакет (`package.json` + `guest-js/`). Фронтенд
плагина собирается **двумя путями**: `npm run build` (tsc → `dist-js/`, npm-канал) и
`npm run build:global` (esbuild → `api-iife.js`, vanilla-канал). Оба артефакта **коммитятся** —
cargo не должен требовать node при сборке хоста.

---

## 2. Записи в `app_data_dir` / `sessions` — только через `json-store`

**Общий крейт `json-store`** (`cross-app-plugins/json-store`) — обычная библиотека, **НЕ** Tauri-плагин (без `links`/`build.rs`/permissions). Единый механизм атомарных записей JSON для хоста и всех плагинов, чтобы протокол записи не дублировался:

```toml
[dependencies]
json-store = { path = "../json-store" }
```

Два примитива:

- **`write_atomic(path, content)`** — tmp в той же папке → `sync_all` → `rename` → удаление tmp при ошибке. Читатель никогда не видит битый/частичный файл.
- **`update_json(path, |value| { ... })`** — read-modify-write с эксклюзивной блокировкой на sidecar `<file>.lock` (`fs2::lock_exclusive`, RAII unlock). **Сам JSON файл не лочится** — читатели не блокируются. При ошибке парсинга — `Default` + `log::warn!` (не роняем файл).

**ЗАПРЕЩЕНО** в `app_data_dir`/`sessions`:
- `fs::write` напрямую;
- `fs::read_to_string` + `unwrap_or_default()` + `fs::write` (lost update + рваный файл).

**Исключения** (не `app_data_dir`): логи и выводы рядом с exe, sidecar-данные с собственным `DATA_DIR` (например, 9router SQLite) — остаются на прямой записи.

**Тесты** (в `json-store/src/lib.rs`): конкурентные `update_json` без потерь, читатель не видит невалидный JSON, tmp не остаётся при ошибке. Запуск: `cargo test -p json-store --lib` (через `test-plugin.bat json-store`).

---

## 3. Единый источник правды (SSOT) для JS-стороны

**Один источник — `guest-js/` (там правки руками).** Артефакты `dist-js/` и `api-iife.js` —
**производные**, коммитятся, генерируются сборкой, руками НЕ правятся. Поменял `guest-js/*` →
`npm run build` + `npm run build:global` + коммит обоих артефактов → хост при пересборке получает
новую версию.

> ⚠️ Правило: забыл пересобрать артефакт после правки `guest-js/*` → хост тянет старый JS
> (та же механика, что у официального `dist-js`).

---

## 4. Rust-часть плагина (КРИТИЧНО — иначе не соберётся/не запустится)

### 4.1 `Cargo.toml` — обязателен `links` + build-зависимость

```toml
[package]
name = "plugin-log-collector"
version = "0.1.0"
edition = "2021"
links = "tauri-plugin-logs"           # ← ОБЯЗАТЕЛЬНО; идентификатор-namespace, а НЕ имя пакета!

[dependencies]
tauri = { workspace = true }
# ...

[lib]
name = "plugin_log_collector"
path = "src/lib.rs"

[build-dependencies]
tauri-plugin = { version = "2", features = ["build"] }   # ← для permissions + глobal-скрипта
```

> ⚠️ **`links` — НЕ имя крейта, а ключ линковки + namespace permissions.** Cargo строит из него
> env `DEP_{LINKS}_PERMISSION_FILES_PATH`, а `tauri-build` хоста, разобрав имя, выводит из links
> plugin-identifier (для `links = "tauri-plugin-logs"` → `logs`). Capability `logs:default`,
> invoke `plugin:logs|...` и runtime `Builder::new("logs")` завязаны на этот identifier.
> **При переименовании пакета `links` НЕ меняется** (универсальность: имя плагина может быть
> `plugin-log-collector`, но links остаётся `tauri-plugin-logs`). Смена links на имя пакета
> → сборка хоста падает: `Permission logs:default not found` (verified 15.11.26).
> Без `links`/`build.rs` permissions не сгенерятся вообще. Без `global_api_script_path`
> не будет vanilla-канала.

### 4.2 `build.rs` — команды + `global_api_script_path` (vanilla-канал)

```rust
const COMMANDS: &[&str] = &["get_last_logs_path", "log_frontend_event", /* ... */];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .global_api_script_path("./api-iife.js")   // ← vanilla-канал (см. §5.4)
        .build();
}
```

`COMMANDS` — **единственный источник правды** для имён команд плагина.

### 4.3 `src/lib.rs` — Config, PluginState, `init`

**⚠️ Config ОБЯЗАН переживать `null`.** Если ключа `plugins.<id>` нет в
`tauri.conf.json` хоста, Tauri передаёт `null`, и наивный
`#[derive(Deserialize)]` паникует на старте хоста:
`invalid type: null, expected struct Config`. Поэтому **всегда** пишем
кастомный `Deserialize`, который через `Option::<...>::deserialize`
трактует `null` как `Default::default()` (см. `plugin-cloud-routers` /
`plugin-downloader`). Не добавляйте плагин в хост без этой защиты.

```rust
use serde::Deserialize;
use tauri::{plugin::{Builder, TauriPlugin}, Manager, Runtime};

/// Конфиг из tauri.conf.json хоста: "plugins": { "logs": { "log_file_name": "...", "last_logs": true } }
#[derive(Clone, Default)]
pub struct Config { /* ... */ }

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        // null (секции нет) → default; объект → поля.
        let _ = Option::<serde::de::IgnoredAny>::deserialize(deserializer)?;
        Ok(Config::default())
    }
    // Для конфига с полями: Option::<ConfigInner>::deserialize → map → unwrap_or_default
}

/// Вызывается из хоста: `.plugin(plugin_log_collector::init())`.
pub fn init<R: Runtime>() -> TauriPlugin<R, Config> {
    Builder::<R, Config>::new("logs")
        .setup(|app, api| { /* инициализация состояния, log::Log, путь last_logs */ Ok(()) })
        .invoke_handler(tauri::generate_handler![commands::get_last_logs_path, /* ... */])
        .build()
}
```

### 4.4 Команды — ОБЯЗАНЫ быть дженериками над `Runtime`

```rust
#[tauri::command]
pub fn get_last_logs_path<R: Runtime>(app: AppHandle<R>) -> String { /* ... */ }
```
Не-дженерик `AppHandle` даёт `E0277` на этапе сборки крейта хоста.

### 4.5 Порядок добавления НОВОЙ команды (чек-лист)

1. `#[tauri::command] pub fn my_cmd<R: Runtime>(...)` в `commands.rs`;
2. `"my_cmd"` в `COMMANDS` (`build.rs`) **и** в `generate_handler!` (`lib.rs`);
3. `[[permission]] allow-my-cmd` + в `[default].permissions` (`permissions/default.toml`).

Любое расхождение → рантайм-ошибка «permission not found» для команды.

---

## 5. Frontend-часть плагина

### 5.1 `guest-js/index.ts` — типизированный TS-API

```ts
import { invoke } from '@tauri-apps/api/core'

export async function getLastLogsPath(): Promise<string> {
  return invoke<string>('plugin:logs|get_last_logs_path')
}
// ... остальные команды

// Обязательно в конце: side-effect import регистрирует Web Component при импорте пакета.
import './web-components'
```

> Вызовы идут через `plugin:<identifier>|<command>` — identifier из `Builder::new("logs")`.

### 5.2 `package.json` плагина

```json
{
  "name": "@my-tauri-plugins/plugin-logs",
  "version": "0.1.0",
  "type": "module",
  "main": "dist-js/index.js",
  "types": "dist-js/index.d.ts",
  "scripts": {
    "build": "tsc -p tsconfig.json",
    "build:global": "esbuild guest-js/iife-entry.ts --bundle --format=iife --outfile=api-iife.js",
    "check:global": "esbuild guest-js/iife-entry.ts --bundle --format=iife --outfile=api-iife.js && node --check api-iife.js"
  },
  "dependencies": { "@tauri-apps/api": "^2", /* другие @tauri-apps/plugin-* */ },
  "devDependencies": { "typescript": "^5.4.0", "esbuild": "^0.21.0" }
}
```

### 5.3 `tsconfig.json`

```json
{
  "compilerOptions": {
    "target": "ES2020", "module": "ESNext", "moduleResolution": "Bundler",
    "lib": ["ES2020", "DOM", "DOM.Iterable"], "strict": true,
    "declaration": true, "outDir": "dist-js", "rootDir": "guest-js", "skipLibCheck": true
  },
  "include": ["guest-js/**/*.ts"]
}
```

### 5.4 vanilla-канал: global API script (`api-iife.js`) — СТАНДАРТ

Этот канал — официальный механизм Tauri v2 для доставки JS плагина в хост **без npm и бандлера**
(канал всех официальных плагинов Tauri). Конвейер:

1. Плагин собирает **IIFE** из `guest-js` (esbuild) и коммитит `api-iife.js` в корень плагина.
2. `build.rs` регистрирует его: `.global_api_script_path("./api-iife.js")` → Cargo/env
   `DEP_<crate>_GLOBAL_API_SCRIPT_PATH`.
3. `tauri-build` хоста на сборке кладёт скрипт в `OUT_DIR/__global-api-script.js`, `tauri-codegen`
   **вшивает его содержимое в бинарник** (только если `app.withGlobalTauri: true`).
4. Рантайм Tauri вставляет скрипт в вебвью **первым** (`main_frame_script`), ДО фронтенд-кода хоста.
   → `<logs-panel>` уже определён, `window.__TAURI__.<plugin>` уже доступен.

**Требования к `api-iife.js`:**
- Никаких голых импортов (`import { x } from '@tauri-apps/api/...'`). Всё, что использовал
  `guest-js` через `@tauri-apps/*`, в IIFE биндится на **глобалы**:
  - `@tauri-apps/api/core` → `window.__TAURI__.core` (иначе `window.__TAURI_INTERNALS__.invoke`);
  - `@tauri-apps/api/event` → `window.__TAURI__.event`;
  - `@tauri-apps/plugin-dialog` → `window.__TAURI__.dialog` (появляется, если хост включил этот
    плагин со своим iife); `@tauri-apps/plugin-updater` → `window.__TAURI__.updater`;
    `@tauri-apps/plugin-shell` → `window.__TAURI__.shell` — и т.д.
- **Динамические `import('@tauri-apps/...')` в IIFE НЕ работают** — их надо заменить на вызов
  через соответствующий глобал с проверкой наличия и фолбэком (см. пример ниже).
- Идемпотентность: guard в начале, как у официального `api-iife.js`:
  `if ("__TAURI__" in window && window.__TAURI__.<plugin>) return;`
- В конце: `customElements.define('logs-panel', ...)` (если WC) и/или
  `Object.defineProperty(window.__TAURI__, '<plugin>', { value: {...} })`.

**Как это уживается с `@tauri-apps/api`:** esbuild-бандл нельзя просто собрать из `guest-js`
напрямую — он импортирует `@tauri-apps/*`. Поэтому у каждого плагина есть `guest-js/iife-entry.ts`,
который импортирует код плагина через АЛИАСЫ esbuild (см. §10.3). Механика:
- в `guest-js/` рядом с кодом лежат шимы `shims.ts` вида:
  ```ts
  const core = (window as any).__TAURI__?.core ?? (window as any).__TAURI_INTERNALS__;
  export const invoke = core.invoke.bind(core);
  export const Channel = core.Channel;
  ```
  Соответственно шимы для `event`, `dialog`, `updater`, `shell`;
- `iife-entry.ts` собирает всё вместе: `import { ... } from './web-components'` + регистрация
  `window.__TAURI__.<plugin>` + вызов `initFrontendErrorCapture()` где есть;
- esbuild: `--alias:@tauri-apps/api/core=./guest-js/shims/core.ts` и т.д. (подменяет голые импорты
  на шимы; сам `guest-js/index.ts` и `web-components.ts` НЕ трогаются).

**Пример замены динамического диалога (внутри iife-entry или шима):**
```ts
export async function saveDialog(): Promise<string | null> {
  const dlg = (window as any).__TAURI__?.dialog
  if (!dlg?.save) return null          // диалог не включён в хосте — честный фолбэк
  return dlg.save({ title: 'Сохранить логи', filters: [...] }) ?? null
}
```

> Хост не получает JS плагина «из руки» — код вшивается Tauri из бинарника. Пропагация правок:
> `guest-js/*` → `npm run build:global` плагина (пересобрался `api-iife.js`) → пересборка хоста.
> Менять/копировать `api-iife.js` в хосте ЗАПРЕЩЕНО.

### 5.5 Стилизация Web Component (САМОЕ ВАЖНОЕ для GUI)

Темизируй компонент через CSS-переменные хоста (`var(--token, fallback)`). Custom properties со
`:root` хоста проникают сквозь Shadow DOM. **Никогда не хардкодь светлые цвета.** Внешние ссылки —
через `@tauri-apps/plugin-shell` → `open(...)` (не `window.open`).

---

## 6. Permissions (`permissions/default.toml`)

```toml
"$schema" = "../gen/schemas/acl.json"

[[permission]]
identifier = "allow-get-last-logs-path"
description = "..."
commands.allow = ["get_last_logs_path"]

# ... по одному allow-* на каждую команду из COMMANDS

[default]
description = "Default permissions for the logs plugin."
permissions = [ "allow-get-last-logs-path", /* ... */ ]
```

Хост подключает плагин целиком через `"logs:default"` в capability.

---

## 7. Интеграция в хост

### 7.1 `src-tauri/Cargo.toml` (путь — именно `../../`)

```toml
[dependencies]
plugin-log-collector = { path = "../../cross-app-plugins/plugin-log-collector" }
```
Плюс зависимости `package.json`/npm НЕ нужны — они только для bundler-хостов (см. §7.5).

### 7.2 `src-tauri/src/main.rs` / `lib.rs`

```rust
// в цепочке .plugin(...):
.plugin(plugin_log_collector::init())
// УБРАТЬ из invoke_handler дублирующие команды, которые теперь в плагине.
```

### 7.3 `src-tauri/capabilities/default.json`

```json
{
  "permissions": ["core:default", "logs:default"]
}
```

### 7.4 `src-tauri/tauri.conf.json`

```json
{
  "app": { "withGlobalTauri": true, ... },
  "plugins": { "logs": { "log_file_name": "repos_control.log", "last_logs": true } }
}
```

### 7.5 Фронтенд — ДВА канала (выбери один для своего хоста)

**A. Хост с npm/бандлером** — npm-канал: `package.json` хоста
`"@my-tauri-plugins/plugin-logs": "file:../cross-app-plugins/plugin-log-collector"`, затем
`import "@my-tauri-plugins/plugin-logs";` (side-effect → регистрация WC). Официальный путь для
bundler-проектов.

**B. Хост vanilla (без npm/бандлера)** — vanilla-канал: НИЧЕГО в npm не добавляем. Просто:
```html
<logs-panel></logs-panel>
```
JS плагина вшит в бинарник Тauri и вставлен до кода хоста. Доступ к API из своей UI:
`window.__TAURI__.log.logFront(...)`, `window.__TAURI__.event.listen('logs:message', ...)`.

---

## 8. Сборка

- **Плагин** (при правке `guest-js/*`): в папке плагина `npm install` → **`npm run build`**
  (→ `dist-js`) **и `npm run build:global`** (→ `api-iife.js`), коммит обоих артефактов.
  Без `build:global` vanilla-хост получит старый JS.
- **Хост**: только через `.bat`-обёртки (см. `desktop_rust_tauri_rules.md` §2). `build.bat` компилирует path-зависимый
  крейт плагина (а вместе с ним вшивает и `api-iife.js` из OUT_DIR).

---

## 9. Чек-лист создания/переноса плагина

1. Создать папку `plugin-<name>/` как member workspace `cross-app-plugins`
   (+ `plugin-<name>/core/` — ядро `<name>-core` без tauri, §0.1).
2. `Cargo.toml`: `links = "tauri-plugin-<id>"` (namespace, НЕ имя пакета!) +
   `[build-dependencies] tauri-plugin`; деп ядра `core` path.
3. `build.rs`: `COMMANDS` + `.global_api_script_path("./api-iife.js")` + `.build()`.
4. `lib.rs`: `Config`, `PluginState`, `init<R: Runtime>()`, `generate_handler!`.
5. Команды — строго `pub fn cmd<R: Runtime>(app: AppHandle<R>, ...)`; логика — в ядре,
   фасад только мосты с ОБЕИМИ сигнатурами (§0.1 п.2).
6. `permissions/default.toml`: `allow-*` на каждую команду + все в `[default]`.
7. `package.json`/`tsconfig.json`: scripts `build` (tsc) + `build:global` (esbuild), devDeps
   `typescript` + `esbuild`.
8. `guest-js/index.ts` (`plugin:<id>|<cmd>`) + `web-components.ts` (WC, `var(--token)`).
9. `guest-js/iife-entry.ts` + шимы `@tauri-apps/*` (глобалы `window.__TAURI__.*`) — §5.4, §10.3.
10. Собрать и закоммитить `dist-js/` И `api-iife.js`.
11. GPUI-фронтенд: карточка в `gpui-kit` за фичей + реэкспорт из `my-gpui-kit` (§0.1 п.3).
12. Хост: path-зависимость `../../`, `.plugin(...)`, capability `<id>:default`, `plugins.<id>`
    в конфиге; фронтенд — тег WC (vanilla) или `import` (npm); GPUI-хост — ядро напрямую + кит.
13. `npm install` + `npm run build` + `npm run build:global` в плагине → `.bat` хоста.
14. Валидация универсальности: `cargo check` ядра отдельно (0 tauri-символов) + тесты ядра +
    `build.bat` обоих типов хостов (Tauri и GPUI).

---

## 10. Подводные камни

- **camelCase, НЕ snake**: `fn tts_list_models(models_dir)` ждёт от JS `{ modelsDir }`.
- **Debug-бинарь грузит `devUrl`, а не `frontendDist`** — standalone только через `tauri build`
  (фича `custom-protocol`). В этом проекте dev-режим вообще запрещён.
- **`sccache` ломает C-сборку** — чисти `CC`/`CXX`/`rustc-wrapper` (см. `desktop_rust_tauri_rules.md` §5.1).
- **«permission not found»** → нет `links`/`build.rs` ИЛИ команда не во всех трёх местах, ИЛИ
  `links` ≠ `tauri-plugin-<id>` (переименование пакета с правкой links ломает namespace — §4.1).
- **Переименование папки/пакета плагина — править ВСЕ path-ссылки**: Cargo path-депы хостов,
  **npm `file:`-пути в `package.json` bundler-хостов** (vite: `Rollup failed to resolve import`),
  gpui-kit-депы, workspace members. Опущенный npm `file:`-путь ловится только на сборке хоста.
- **`E0277: AppHandle: CommandArg`** → команда не `<R: Runtime>`-дженерик.
- **`config().version` — `Option<String>`** → `app.package_info().version`.
- **Неверный путь зависимости** → от `src-tauri/` пиши `../../cross-app-plugins/...`, не `../`.
- **WC «белый в тёмной теме»** → захардкожены светлые цвета; только `var(--token)`.
- **Динамические `import('@tauri-apps/...')` ломают esbuild-IIFE** → замени на глобал
  `window.__TAURI__.<plugin>` с фолбэком.
- **Забыл исполнить `npm run build:global`** после правки `guest-js/*` → vanilla-хост тянет старый
  JS: всегда пересобирай оба артефакта.
- **`window.__TAURI__.<plugin>` нет в хосте** → в `build.rs` плагина не добавлен
  `global_api_script_path`, ИЛИ у хоста выключен `withGlobalTauri`.
- **Порядок iife при связанных плагинах** (напр. logs зависит от dialog): главный скрипт биндится на
  глобал зависимого плагина **в момент вызова**, а не при загрузке → связь по порядку не критична.
- **Facades: glob-реэкспорт + явные мосты** (`pub use core::*` + `pub fn load_config(&AppHandle)…`)
  — легально: явные items затеняют glob. НО следи, чтобы мосты не потеряли старые сигнатуры —
  Tauri-хосты падают с `E0061` (аргументов больше, чем у ядра).

---

## 11. Кодировка файлов исходного кода (КРИТИЧНО)

**Все файлы `.rs`, `.ts`, `.json`, `.toml`, `.md` в репозитории `cross-app-plugins` ОБЯЗАНЫ храниться в UTF-8 без BOM.**

### Почему это критично
- Rust-компилятор (`rustc`) требует UTF-8. Если файл содержит русский текст в CP1251 с UTF-8 BOM, `rustc` не выдаёт ошибку, а компилирует кракозябры прямо в бинарник.
- TypeScript-компилятор (`tsc`) также требует UTF-8.
- Нет никакого механизма, который бы автоматически обнаружил и исправил неправильную кодировку.

### Как предотвратить
1. **Настройте редактор:** Убедитесь, что ваш редактор (VS Code, Notepad++, etc.) сохраняет файлы в UTF-8 без BOM.
2. **Проверка перед коммитом:** Запустите проверку на наличие кракозябр перед коммитом:
   ```powershell
   # Поиск типичных кракозябр в файлах
   Get-ChildItem -Path . -Recurse -Include *.rs,*.ts,*.json,*.toml | ForEach-Object {
     $txt = [System.IO.File]::ReadAllText($_.FullName, [System.Text.Encoding]::UTF8)
     if ($txt -match 'Р[°±²іґµ¶·ё№є»јЅѕїА-Яа-я]|вЂ|В«|В»') {
       Write-Host "MOJIBAKE: $($_.FullName)"
     }
   }
   ```
3. **Автоматическая проверка в build-toolkit:** В `tauri-build-toolkit` добавлен скрипт проверки mojibake, который запускается на шаге `prep` и останавливает сборку при обнаружении кракозябр.

---

## 12. Обработка ошибок и логирование (КРИТИЧНО)

### 12.1 На бэкенде (Rust)

**Каждая Tauri-команда при возврате `Err` ОБЯЗАНА писать в лог:**

```rust
#[tauri::command]
pub async fn my_command(app: AppHandle, ...) -> Result<T, String> {
    match some_operation() {
        Ok(result) => {
            log::info!("my_command: успешно");
            Ok(result)
        }
        Err(e) => {
            log::error!("my_command: ошибка: {}", e);
            Err(format!("Ошибка: {}", e))
        }
    }
}
```

### 12.2 На фронтенде (TypeScript)

**Любой `catch (e)` в Web Component ОБЯЗАН:**
1. Показывать пользователю тост: `toast(msg, 'error')`
2. Логировать ошибку в системный лог: `logPlugin(msg)`

```ts
try {
  await someOperation()
} catch (e) {
  const msg = `Ошибка операции: ${String(e)}`
  logPlugin(`[my-plugin] ${msg}`)
  toast(msg, 'error')
}
```

### 12.3 Автономные функции в Web Component

**Каждый Web Component плагина ДОЛЖЕН содержать автономные функции `toast` и `logPlugin` без импорта из хост-приложения:**

```ts
function toast(msg: string, kind: 'success' | 'error' = 'success'): void {
  const el = document.createElement('div')
  el.textContent = msg
  el.style.cssText =
    `position:fixed; right:16px; bottom:16px; z-index:5000; max-width:420px; padding:10px 14px;` +
    `border-radius:8px; font:13px var(--font, system-ui, sans-serif); box-shadow:0 3px 14px rgba(0,0,0,.4);` +
    `background:${kind === 'success' ? 'var(--primary, #4a90d9)' : 'var(--danger, #b54242)'}; color:#fff;`
  document.body.appendChild(el)
  setTimeout(() => el.remove(), 3500)
}

function logPlugin(msg: string): void {
  void invoke('plugin:logs|log_frontend_event', { level: 'FE', msg }).catch(() => {})
}
```

### 12.4 Почему нельзя использовать `console.error`

`console.error` пишет **только в консоль Chromium DevTools (F12)**, которая закрыта у пользователя в релизе. В UI-вкладку «Логи» и лог-файл приложения сообщение не отправляется, и красная плашка-тост пользователю не показывается. Для юзера это выглядит как «нажал, ничего не произошло и тишина».

---

## 11. Планы (следующие модули)

`plugin-llama-engine` (GUI + логика движка llama.cpp) и др. — те же принципы: Rust-крейт +
Web Component + **два JS-канала** (npm `dist-js` + vanilla `api-iife.js`).

---

## 13. Исключение из стандарта: CLI-крейты без Tauri

`tools/agent-tools/` — **не Tauri-плагин**: у него нет GUI, поэтому ненужны `links`,
`build.rs`, `permissions/`, `guest-js/` и оба JS-канала. Это обычный
workspace-member с бинарником, который вызывает ИИ-агент кодер из своей оболочки.

Что из стандарта остаётся обязательным:

- **[workspace.dependencies]** вместо собственных версий (см. §1);
- **запись JSON — через `json-store`** (см. §2): временные файлы и lock-файл
  рядом с состоянием не мусор, это цена атомарности;
- **логирование** (§12): у CLI нет файла лога — ставится минимальный `log::Log`
  в stderr, иначе `log::warn!` внутри `json-store` исчезают без следа;
- **UTF-8 без BOM** (§11), в том числе в `tests/`;
- **тесты** — `test-plugin.bat <крейт>` (unit) и `cargo test -p <крейт>` (сквозные).
  Сквозные тесты CLI поднимают стаб-сервер на `127.0.0.1:0`, а не стучатся в живой
  шлюз; живой прогон помечается `#[ignore]`;
- **`unwrap()`/`expect()` — только в тестах**: процесс, который читает ввод и ходит
  в сеть, обязан отдавать код возврата, а не паниковать.

Контракт «тул ↔ агент» — **код возврата, а не текст**: значения зафиксированы в
`tools/agent-tools/src/exit.rs` и проверяются сквозными тестами.