# plugin-mesh-engine

Переиспользуемый Tauri-плагин **локальной 3D-генерации** (изображение → текстурированная
GLB-модель) через движок [pwilkin/trellis.cpp](https://github.com/pwilkin/trellis.cpp)
— без Python, без облака. Назначение в проекте: тул `generate_mesh` + плашка «Движок
3D-генерации» в King Orch, но плагин универсален (PLUGIN_STANDARD §0.1).

## Модели

Движок обслуживает **два семейства весов** в одной папке (docs/pixal3d/README.md
апстрима: «one model directory serves both families», выбор по флагу `--model`):

| Семейство | Флаг | Файлов | Источник | Кванты |
|---|---|---|---|---|
| TRELLIS.2 | `trellis` | 10 | [ilintar/trellis2-gguf](https://huggingface.co/ilintar/trellis2-gguf) | q4 / q8 / f16 (f16 в корне репо, q4/q8 в подпапках) |
| Pixal3D | `pixal3d` | 5 | [vegax87/Pixal3D](https://huggingface.co/vegax87/Pixal3D) | **только f16** (других не существует) |

Обе модели ставятся **всегда** (решение владельца 17.11.26 — чекбокса нет). Размеры
на диске: trellis2 — q4 ~6 ГБ / q8 ~9.5 ГБ / f16 ~16.5 ГБ; Pixal3D — +~11 ГБ (f16).

Честные нюансы (проверены по докам движка 17.11.2026):

- каскад 1024 у trellis2 работает на 16 ГБ карте (FlashAttention с padded K/V);
- текстурная стадия Pixal3D — пик памяти: на 16 ГБ возможен OOM на 1024
  (апстримовые хаки `--max-tokens` / `--no-naf` в этом плагине пока НЕ прокинуты);
- связка «квантованные декодеры trellis2 (q4/q8) + f16 pixal3d_*» апстримом прямо
  не гарантирована (их пример — полный f16-набор); отказ движка лечится докачкой
  f16-декодеров, не кодом;
- у Pixal3D `--res 512` без текстурной модели → движок сам падает в «только геометрия»;
- MoGe-2 (автодетект FOV) в движке не перенесён, флаг `--fov` не прокинут.

## Риггинг (skin-tokens.cpp, этап 2)

Плагин также обслуживает **авто-риггинг** — скелет + веса скиннинга (JOINTS_0/WEIGHTS_0)
на статическую GLB через CLI [localai-org/skin-tokens.cpp](https://github.com/localai-org/skin-tokens.cpp)
(`skintokens-cli rig`, синхронный спавн — HTTP-сервера у риггинга нет), веса
[LocalAI-io/SkinTokens-GGUF](https://huggingface.co/LocalAI-io/SkinTokens-GGUF) **F16
только** (mesh-encoder 57.8 МБ + skin-vae 244 МБ + tokenrig 949 МБ ≈ 1.25 ГБ).

- вход — РОВНО один GLB; CPU-процесс (VRAM не занимает), таймаут 3600 с;
  на больших моделях — десятки минут;
- **анимации это НЕ добавляет** (анимация — отдельный этап, kimodo.cpp);
- **пребилдов CLI upstream нет** — exe/DLL (`skintokens-cli.exe`, `skintokens.dll`,
  `ggml.dll`, `ggml-base.dll`) кладутся в `<rig_dir>/` вручную из локальной сборки;
  панель качает только 3 веса (kind `rig-engine`);
- папка движка: ключ `skintokens_dir` в app_config.json → дефолт `<exe>/skintokens`
  (SSOT `rig_dir_early()`); веса в `models/SkinTokens-GGUF/F16/`; кнопка
  «Изменить путь» панели — `mesh_rig_engine_set_dir`.
- **Обновления риггинга НЕТ (честно)**: upstream не публикует релизы (GitHub API
  `releases` — пусто, проверено 17.11.26) → CLI обновляется только пересборкой из
  git + перекладкой файлов вручную; веса не «обновляются» никогда (skip-логика
  downloader-core валидные GGUF пропускает) — только удалением
  `models/SkinTokens-GGUF/F16` и повторной скачкой. Механизм обновления по
  паттерну trellis (мета + check_update) — когда upstream начнёт публиковать
  пребилды.

## Text→3D промпты (концепт-маршрут)

SSOT промпт-шаблонов концепт-изображений — **ядро плагина**
(`core/src/engine/prompting.rs`, решение владельца 17.11.26 «шаблоны должны
быть в плагине лежать»): 4 шаблона дословно из документа специалиста
(`ConceptKind`: `prop` / `tree` / `weapon` / `character`) + сборка
`build_concept_prompt(subject_en, kind)` (подстановка `{USER_PROMPT}`) +
каталог `CONCEPT_KIND_CATALOG` для выбора категории агентом.

- Правила изображений зашиты В ТЕЛО шаблонов: чистый фон #FFFFFF, широкие
  поля от краёв, 3/4 ракурс (персонажи — фронтально A-pose), мягкий свет без
  теней, резкость без боке, деревья — «dense chunky sculpted foliage clusters».
  Причина — негативный промпт при `--cfg-scale 1.0` (Z-Image-Turbo класс) не
  работает: все запреты должны быть в главном промпте.
- Хост-тул `mesh_concept_prompt` (King Orch, `infra/tools/mesh.rs`) — тонкая
  обвязка: read-only, без вложений и без VRAM-suspend; агент передаёт
  `subject_en` (английское описание объекта) + `kind`, результат идёт
  ЦЕЛИКОМ в `generate_image` как `prompt_en`.
- Маршрут в King Orch: текст без картинки → факт `needs_3d_from_text` →
  агент `mesh_concept` рисует концепт → пользователь утверждает → следующий
  ход видит картинку в истории → существующая ветка `needs_3d_generate` →
  `generate_mesh` по концепту.
- **Честная оговорка**: параметры движка sd.cpp из документа специалиста
  (Z-Image-Turbo, `-W 1024 -H 1024`, `--steps 8`, `--cfg-scale 1.0`) тулом НЕ
  прокидываются — их задаёт активный бандл медиа-движка (плагин
  media-gen-engine); при другом бандле концепт генерится, но качество и
  композиция могут отличаться от референсных.

## Архитектура

```
src/       фасад tauri-плагина (init<R:Runtime>, 4 команды, Err → log::error!)
core/      mesh-gen-core — ЧИСТОЕ ядро без tauri (ССОТ):
           presets.rs   (MeshPreset Trellis|Pixal3d, MeshQuant q4|q8|f16, каноническая
                         раскладка weights_dir = engine_dir/models/<quant>)
           config.rs    (trellis_dir в app_config.json через json-store; SSOT-резолвер
                         engine_dir_early: конфиг → <exe>/trellis)
           installer.rs (установка: бандл GitHub + веса HF, staging + copy_tree_overwrite,
                         бекенд-детект по install.ps1, докачиваемость)
           process.rs   (спавн trellis-server через engine-proc-util: --models --model
                         --port; порты 21500–22999; health 300s; Drop/Exit kill)
           client.rs    (HTTP /generate multipart; GEN_TIMEOUT 1800s; GLB → model/gltf-binary)
           preflight.rs (check_engine_files: exe + *.gguf; ошибки-инструкции; VRAM-гейта
                         НЕТ сознательно — §00.2)
           generate.rs  (validate + оркестрация префлайт→спавн→генерация→сохранение)
           outputs.rs   (save_glb_sequenced: mesh-<N>.glb по факту файлов)
           prompting.rs (SSOT текст→3D: 4 шаблона концепт-промптов специалиста,
                         ConceptKind + build_concept_prompt + каталог категорий)
guest-js/  TS API + <mesh-engine-panel> (Shadow DOM) + shims window.__TAURI__
dist-js/, api-iife.js — коммитятся (два канала, PLUGIN_STANDARD §5)
```

## Команды (invoke `plugin:mesh-engine|<cmd>`)

- `mesh_engine_status` → `{installed, exePath, dir, weights: [{quant, dir, ggufCount, pixal3dFiles}]}`;
- `mesh_engine_install` `{backend?, quant?}` — установка/докачка (см. ниже);
- `mesh_engine_check_update` → `string | null` — тег нового релиза либо null
  (актуален); ошибка сети — честный Err, а не «актуален»;
- `mesh_engine_set_dir` `{path}` — папка движка → `trellis_dir` в app_config.json хоста;
- `mesh_generate` `{imagePath, sessionId, model?, quant?, backend?, gpu?, seed?,
  resolution?, bgRemoval?, faces?}` → `{glbPath, port, durationSecs}`;
- `mesh_rig_engine_status` → `{ready, exePresent, dllsPresent, weightsPresent,
  weightsTotal, dir, weightsDir}` — раскладка риггинга;
- `mesh_rig_engine_install` — скачать/докачать 3 веса риггинга F16 → свежий статус
  (прогресс `kind === 'rig-engine'`; CLI/DLL не качаются — пребилдов upstream нет);
- `mesh_rig_engine_set_dir` `{path}` — папка движка риггинга → `skintokens_dir`
  в app_config.json хоста (зеркало `mesh_engine_set_dir`).

**Прогресс установки** — события `downloader:progress` (SSOT downloader-core,
`set_emit_hook` фасада plugin-downloader): UI фильтрует по `kind === 'mesh-engine'`.

## Установка и обновление

- server-бандл: `github.com/pwilkin/trellis.cpp/releases/latest/download/trellis-<backend>-windows-x64.zip`
  (vulkan ~14 МБ / rocm ~190 МБ / cuda12 ~642 МБ / cuda ~715 МБ). Бекенд: явный выбор
  либо авто-детект по install.ps1 — nvidia-smi compute_cap ≥7.5 → `cuda`, <7.5 →
  `cuda12` (легаси), NVIDIA нет → `vulkan`.
- веса — поимённо в `models/<quant>/`; downloader-core пропускает валидные файлы
  (размер + GGUF-magic) → прерванная установка продолжается повторным «Установить»,
  кнопка при живой установке называется «Докачать / обновить».
- server-бандл качается в staging (`.staging-install`), распаковывается и переносится
  поверх через copy_tree_overwrite: ошибка не ломает старую версию (прецедент
  sdcpp_installer). После успеха staging очищается.
- **Обновление движка** — мета `<engine_dir>/.mesh-engine-meta.json` `{tag, backend,
  installed_at}` записывается инсталлером после успешного server-бандла (тег релиза
  резолвится GitHub API **до** скачивания; API лег → установка честно идёт без меты).
  `mesh_engine_check_update` сравнивает тег меты с `repos/pwilkin/trellis.cpp/releases/latest`
  (GitHub API). Честность: exe есть, меты нет (установка старой версии плагина) →
  Err «версия неизвестна — нажмите Докачать/обновить», а не «актуален»; движок не
  установлен → null. Кнопка «Обновить» панели переиспользует `mesh_engine_install`
  с текущими выборами кванта/бекенда (GGUF-веса пропускаются skip-логикой —
  **GGUF так не обновляются никогда**, только удалением `models/<quant>`).

## Раскладка на диске

```
<engine_dir>/                  ← trellis_dir или <exe>/trellis
├── trellis-server.exe         (+ CUDA/Vulkan-библиотеки бандла)
├── .mesh-engine-meta.json     ← тег релиза бандла (проверка обновлений)
└── models/<q4|q8|f16>/        ← все веса, оба семейства вместе

<rig_dir>/                     ← skintokens_dir или <exe>/skintokens
├── skintokens-cli.exe         (+ skintokens.dll, ggml.dll, ggml-base.dll — из локальной сборки)
└── models/SkinTokens-GGUF/F16/  ← 3 веса риггинга (качаются панелью)

<engine_dir>/../mesh_outputs/<session_id>/   ← GLB-результаты (mesh-<N>.glb, rigged-<N>.glb)
```

Вывод — отдельный корень рядом с папкой движка: движок можно удалить/переустановить,
не тронув сгенерированные модели.

## Интеграция хоста (King Orch)

- Хост-тул `generate_mesh` (`infra/tools/mesh.rs`) резолвит папку движка ТОЛЬКО через
  дверь `plugin_mesh_engine::engine::config::engine_dir_early()` — дубль резолва запрещён.
- Хост-тул `rig_mesh` (`infra/tools/rig.rs`) — та же дверь для риггинга:
  `engine::rig_dir_early()`; источник — `resolve_any` реестра или `{{ mesh_files }}`.
- Реестр артефактов — общий сессионный `ImageArtifactRegistry`; GLB attach строго
  path-only (без base64), маркеры `[MESH_SAVED id=artifact_N]` / `[RIGGED_SAVED id=…]`.
- Граф `agents/3d-gen/transitions/mesh-gen-flow.yaml` (13-й production-граф):
  generate/edit + ветка риггинга (факт `needs_rigging`, приоритет 1).
- Cleanup: `kill_active_engines` в `ExitRequested` + Windows Job Object (engine-proc-util).

## Сборка

```bash
npm install && npm run build && npm run build:global   # dist-js + api-iife.js (коммитятся)
test-plugin.bat mesh-gen-core            # тесты ядра (конвенция репозитория)
test-plugin.bat plugin-mesh-engine       # компиляция фасада
```

## Статус (17.11.2026)

Готово: ядро (49 passed: URL-контракты, детект бекенда, копирование, резолверы
пути trellis/skintokens, мета обновлений + честность `update_decision`, статус
и URL-контракты риггинга, text→3D шаблоны), фасад (6 команд), JS-каналы,
permissions, панель (установка + обновление + секция «Риггинг»), хост-интеграция
King Orch (тулы `generate_mesh` / `rig_mesh` / `mesh_concept_prompt` + dispatch +
граф + агенты). Осталось: ручной QA владельцем — установка через плашку,
раскладка CLI риггинга из локальной сборки, боевой прогон (generate → rig →
JOINTS_0/WEIGHTS_0) и боевой прогон концепт-маршрута.
Вне MVP: удаление движка, выбор GPU-индекса, прокидка `--fov`/`--max-tokens`/
`--no-naf`, дистрибуция CLI риггинга пользователям (пребилдов нет).
