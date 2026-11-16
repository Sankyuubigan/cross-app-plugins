# rust-build-toolkit

Общий набор скриптов сборки/релизов Windows-приложений (Rust), вынесенный из
эталонного проекта King Orch. Поддерживает два `kind` проекта: **tauri**
(Tauri 2.0 + фронтенд) и **cargo** (чистый Rust без Tauri — GPUI/egui/CLI и т.п.).
Внутри — как собирать, как подключать, какие гарантии безопасности git/gh.

> **Zero-dependency**: чистый Node (CommonJS), без `npm i`. Для САMОГО пакета
> ставить ничего не нужно. `npx tauri ...` / `gh` / `cargo` вызываются из проекта.

## Что умеет

| Команда     | Что делает                                                                                                                                  |
|-------------|---------------------------------------------------------------------------------------------------------------------------------------------|
| `build`     | dev-сборка: бамп версии + sync resources + **автосборка локальных плагинов** + `npm install` + иконки + `npx tauri build` + запуск приложения |
| `prep`      | только подготовка (бамп версии + sync + автосборка плагинов + npm + иконки) — шаг 1 `generate_installer.bat`                                  |
| `installer` | `prep` + `npx tauri build --bundles nsis` + верификация установщика и `.sig` (подпись lenient: без ключа собирает без подписи)              |
| `release`   | полный релиз: сборка NSIS → подпись (strict) → `gh release create` (с `--repo`) → `latest.json` → коммит версии → push (ветка автодетект). В конце печатает success-отчёт (тег, продукт, установщик, URL релиза, `latest.json`, ветка); `release.bat` ставит `pause`, консоль не закрывается |
| `test`      | `cargo test [фильтр]` (MSVC-окружение — из вызывающего `.bat`)                                                                               |
| `version`   | только бамп версии по схеме `YY.M.P` и печать                                                                                                 |
| `doctor`    | read-only диагностика: конфиг, git origin, repo-guard, путь ключа подписи                                                                     |
| `init`      | раскладка `.bat`-шаблонов и `.build-config.json` в корень проекта                                                                            |
| `init --with-logs` | то же + вкладывает `LOGS_SETUP.md` — пошаговый гайд подключения `plugin-log-collector` (вкладка «Логи», `log::Log`, файл лога рядом с exe)      |

> Для `kind: "cargo"` (без Tauri): `build` = `cargo build --release` (без npm/иконок/tauri-CLI),
> `test` = `cargo test` в `crateDir`, `installer` = прямой makensis (шаблон `templates/app.nsi`,
> per-user, uninstaller, ярлыки; без NSIS → portable-zip) или через `"nsisPath"` в конфиге;
> артефакт `<product>_<version>_x64-setup.exe` + `.sig` — тот же контракт, что у tauri.

## Подключение к проекту (механизм интеграции)

1. **Положи toolkit** в предсказуемое место рядом с проектами, напр. `..\cross-app-plugins\rust-build-toolkit` (или задай env `BUILD_TOOLKIT` на `cli.cjs`). Сам пакет развёрнут как каталог; при работе он **не пишет** в свою папку.
2. **Раскидай `.bat`-обёртки** (одним из способов):
   ```powershell
   node <toolkit>\cli.cjs init --project "D:\Projects\<my_app>"
   node <toolkit>\cli.cjs init --with-logs --project "D:\Projects\<my_app>"   # + гайд подключения plugin-log-collector
   ```
   Либо скопируй `templates\build.bat`, `generate_installer.bat`, `release.bat`, `test.bat` в корень проекта вручную. Окончания строк — CRLF, кодировка ASCII.
3. **Создай `.build-config.json`** в корне проекта (см. `.build-config.example.json`). Минимум обязателен `repo`; остальное либо опционально, либо автодетектится:
   - `repo` — `<owner>/<repo>` для GitHub. **Гард безопасности:** origin git обязан совпадать, иначе `release`/git-операции падают.
   - `appExe` — имя exe приложения (дефолт: `package.name` из `src-tauri/Cargo.toml`).
   - `productName` — дефолт из `tauri.conf.json`.
   - `branch` — дефолт: `git branch --show-current`.
   - `syncDirs` — каталоги, копируемые в `src-tauri/resources` перед сборкой.
   - `devWindow` — окно dev-сборки (title, размеры, `additionalBrowserArgs` для CDP-порта). Dev-override в `build` отключает бандлинг (`bundle.active=false`).
   - `vcRedist` — копировать VC++ runtime DLL рядом с exe. Идентичная копия на месте — не перезаписывается; DLL залочена запущенным процессом (EBUSY, общий target-dir) — warn, сборка не падает (16.11.2026).
   - `npmInstallArgs` — аргументы `npm install` (дефолт `["--legacy-peer-deps"]`).
   - `kind` — тип проекта: `"tauri"` (дефолт-автодетект по `tauri.conf.json`) или `"cargo"` (без Tauri). Явное значение перекрывает автодетект.
   - `crateDir` — каталог крейта (где `Cargo.toml` с `[package]`). Дефолт: `src-tauri` для tauri; для cargo — автодетект (корень проекта или `src/`). Задавайте явно для нестандартной раскладки (воркспейс).
   - `signing.*` — переопределение пути ключа/пароля подписи (подробнее → `docs/build_release_rules.md` §3.1).
   - `syncPackageJsonVersion` — синхронизировать версию и в `package.json` (дефолт `false`; актуально только для tauri).
4. **Запускай всё через `.bat`** (они инициализируют MSVC: vswhere → vcvarsall x64), либо:
   ```powershell
   node <toolkit>\cli.cjs doctor --project "D:\Projects\<my_app>"
   ```

## Автосборка локальных плагинов

При сборке хоста (`build`/`prep`/`installer`/`release`) toolkit автоматически
находит все зависимости в `package.json` с префиксом `file:` и запускает
`npm run build` + `npm run build:global` для каждого локального плагина перед
`npm install` и сборкой хоста.

Это гарантирует, что свежие артефакты `dist-js/` и `api-iife.js` всегда
попадают в сборку — не нужно вручную пересобирать плагины после правки `guest-js/*`.

## cargo-проекты (без Tauri: GPUI, egui, CLI)

Для проекта без Tauri укажите в `.build-config.json` `"kind": "cargo"` (либо
положите `Cargo.toml` с `[package]` в корень — автодетект примет его за cargo).

- **SSOT версии** — `[package] version` в `Cargo.toml` крейта (не `tauri.conf.json`).
  Схема та же `YY.M.P`; `syncPackageJsonVersion` не применяется (нет `package.json`).
- **`build`** — `cargo build --release` в `crateDir`, далее тот же конвейер:
  `killApp → checkExeFresh (mtime) → vcRedist → launchApp`. `devWindow` не поддерживается
  (окно создаёт само приложение); GUI-запуск приложения — обязанность приложения.
- **`test`** — `cargo test --release` (дефолт) в `crateDir`.
- **`version`** — бамп `YY.M.P` в `[package]` `Cargo.toml` (только секция `[package]`,
  версии зависимостей воркспейса не трогаются).
- **Не поддерживается для cargo** (честный отказ/предупреждение, не тихий пропуск):
  иконки (`ensureIcons` — обязанность проекта: winres/embed-resource в `build.rs`), и
  `syncDirs` (sync-resources предупреждает и пропускает). `npm install` пропускается
  при отсутствии `package.json` (штатно для cargo).
- **Статус поддержки**: `test`/`version`/`build`/`installer` — работают (верифицировано
  16.11.2026 на toy-проекте, toy-воркспейсе и VibeBridge; installer — NSIS setup + minisign .sig,
  тот же контракт артефакта `bundle/nsis/<product>_<version>_x64-setup.exe`, что у tauri).
  `release` для cargo — код готов (cargo-ветка `buildCargoInstaller` + общий gh/latest.json
  конвейер), полный релиз-прогон на cargo-проекте пока не выполнялся. `version` понимает
  воркспейсы (`version.workspace = true` в `[package]` → литерал читается/пишется в
  `[workspace.package]` корня; для нестандартной раскладки задайте `"crateDir"` в конфиге).
- **Updater (клиентская часть)** — СУЩЕСТВУЮЩИЙ крейт `cross-app-plugins/plugin-about-updates/core`
  (about-updates-core, без Tauri: version compare, minisign verify + tauri base64-обёртка,
  latest.json→GitHub API fallback, NSIS-установка `["/P","/UPDATE","/R"]`, install report/
  rollback; для Tauri-хостов — фасад plugin-about-updates с web-компонентами UI).
  Серверная часть — тулкит (`release`/`latest.json`), формат манифеста и подписей без
  изменений. Отдельный updater-крейт НЕ создаётся (инцидент 16.11.26: дубль
  about-updates-core был создан и в тот же день удалён по указанию владельца).

## Правила, которые встроены (не нарушать)

- **Никаких прямых `cargo`/`npx tauri` вручную**, если в проекте есть `.bat`-обёртки — только через них (см. `docs/build_release_rules.md` §2).
- **Единственный источник правды профиля** — `Cargo.toml [profile.release]`; `.bat`-шаблоны сбрасывают `CARGO_PROFILE_RELEASE_*`, `CC/CXX/RUSTC_WRAPPER` и т.п.
- **latest.json — только ПОСЛЕ публикации** (`gh release create`), с жёсткими проверками.
- **Выбор установщика — только точным совпадением** `_<версия>_x64-setup.exe`, никакой сортировки (анти-паттерн §3.7).
- **Сброс ключей/пароля из env, не из кода.** Ключи подписи — общие для всех проектов, в глобальной доке.

## Как протестировать пакет (короткий путь)

```powershell
node cli.cjs doctor --project "D:\Projects\<my_app>"
node cli.cjs prep --project "D:\Projects\<my_app>"     # без GUI-запуска
```
Полный `build` через `Start-Process cmd ...` — см. `docs/build_release_rules.md` §9.

## Документация

- `docs/README.md` — этот файл (механизм интеграции, команды, конфиг).
- `docs/build_release_rules.md` — правила сборки/релизов (перенесено из глобальной
  доки `desktop_rust_tauri_rules.md` §2-§3 с обобщением под любой проект).