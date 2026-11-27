// installer: prep + npx tauri build --bundles nsis + РІРµСЂРёС„РёРєР°С†РёСЏ СѓСЃС‚Р°РЅРѕРІС‰РёРєР°
// Рё СЃРёРіРЅР°С‚СѓСЂС‹ (signer lenient: Р±РµР· РєР»СЋС‡Р° вЂ” РїСЂРµРґСѓРїСЂРµР¶РґР°РµРј Рё СЃРѕР±РёСЂР°РµРј Р±РµР· РїРѕРґРїРёСЃРё).

const fs = require('fs');
const path = require('path');

const { run: sh, killApp } = require('./run.cjs');
const { prep, ensureBundleActive, BUNDLE_DIR, checkExeFresh } = require('./build.cjs');
const { resolveTargetDir } = require('./config.cjs');
const { findInstallerBundle } = require('./selector.cjs');
const { setupSigningEnv, signInstaller } = require('./signing.cjs');
const { copyVcRedistTo } = require('./copy-vc-redist.cjs');
const { readVersion, readCargoVersion } = require('./version.cjs');
const { buildCargoInstaller } = require('./nsis.cjs');

function cleanupOldBundle(cfg) {
    // РЈРґР°Р»СЏРµРј СЃС‚Р°СЂС‹Рµ СѓСЃС‚Р°РЅРѕРІС‰РёРєРё/СЃРёРіРЅР°С‚СѓСЂС‹, С‡С‚РѕР±С‹ СЃРµР»РµРєС‚РѕСЂ РЅРµ РЅР°С€С‘Р» Р»РёС€РЅРµРµ.
    const nsisDir = path.join(BUNDLE_DIR(cfg), 'nsis');
    if (!fs.existsSync(nsisDir)) return;
    for (const f of fs.readdirSync(nsisDir)) {
        if (/\.(exe|sig)$/.test(f) && /_setup|setup/.test(f)) {
            try { fs.unlinkSync(path.join(nsisDir, f)); console.log('removed old:', f); } catch (e) {}
        }
    }
}

// kind=cargo: prep → cargo build --release → vcRedist к exe → NSIS (шаблон
// templates/app.nsi) или portable-zip → подпись setup.exe (lenient).
async function runCargoInstaller(cfg) {
    await prep(cfg);

    console.log('\n=== installer (cargo) ===');
    killApp(cfg); // освободить file lock exe до пересборки

    const buildStart = Date.now();
    sh(cfg, 'cargo', ['build', '--release'], { echo: true, cwd: cfg.crateDir });
    const exe = checkExeFresh(cfg, buildStart);

    // VC++ runtime рядом с exe ДО стейджинга — попадёт в установщик.
    copyVcRedistTo(cfg, path.dirname(exe));

    const installer = buildCargoInstaller(cfg, exe);
    if (installer.toLowerCase().endsWith('.exe')) {
        signInstaller(cfg, installer, { strict: false });
    } else {
        console.warn('[installer] portable-zip не подписывается (minisign ставится на setup.exe).');
    }

    console.log('\n[installer] done.');
    return installer;
}

// Versионные ресурсы (opt-in `versionedResources: true` в .build-config.json;
// 26.11.2026). Бандлимые ресурсы кладутся в `resources/<version>/`, а НЕ в корень
// $INSTDIR: NSIS `/UPDATE` не удаляет файлы, которых нет в текущей сборке, поэтому
// призраки прошлых версий в корне живут вечно и участвуют в резолве (инцидент
// «старый shadow_worker отвечает в чате»). tauri.conf.json остаётся единым SSOT
// без захардкоженной версии — версия подставляется здесь.
//
// Возвращает путь к временному конфигу или null (выключено).
function stageVersionedResources(cfg) {
    if (cfg.versionedResources !== true) return null;

    const confPath = path.join(cfg.projectRoot, 'src-tauri', 'tauri.conf.json');
    const conf = JSON.parse(fs.readFileSync(confPath, 'utf8'));
    const tauriVersion = readVersion(cfg);
    const cargoVersion = readCargoVersion(cfg.crateDir);

    // SSOT-страж: установщик разложит resources/<tauri-версия>/, а exe ищет
    // resources/<CARGO_PKG_VERSION>/ (env! в рантайме). Расхождение = пустой
    // каталог агентов у пользователя, поэтому сборка падает здесь, а не молчит.
    if (tauriVersion !== cargoVersion) {
        throw new Error(
            `versionedResources: версии разошлись — tauri.conf.json ${tauriVersion} ≠ Cargo.toml ${cargoVersion}. ` +
            `Ресурсы легли бы в resources/${tauriVersion}/, а приложение ищет resources/${cargoVersion}/. ` +
            `Синхронизируйте версии (bumpVersion) и пересоберите.`
        );
    }

    const resources = (conf.bundle && conf.bundle.resources) || {};
    const rootKeys = new Set(cfg.rootResources || []);
    const override = {};
    const versioned = [];
    for (const [source, target] of Object.entries(resources)) {
        if (rootKeys.has(source)) {
            override[source] = target;
            continue;
        }
        const versionedTarget = `resources/${tauriVersion}/${target}`;
        override[source] = versionedTarget;
        versioned.push(`${source} -> ${versionedTarget}`);
    }
    if (versioned.length === 0) {
        throw new Error('versionedResources: в bundle.resources нет ни одного ресурса для версионирования');
    }

    const outPath = path.join(cfg.projectRoot, 'src-tauri', 'tauri.versioned.conf.json');
    fs.writeFileSync(outPath, JSON.stringify({ bundle: { resources: override } }, null, 2) + '\n', 'utf8');
    console.log(`[installer] versioned resources (v${tauriVersion}):`);
    for (const line of versioned) console.log(`  ${line}`);
    const rooted = Object.keys(override).filter((k) => rootKeys.has(k));
    if (rooted.length) console.log(`[installer] root resources: ${rooted.join(', ')}`);
    return outPath;
}

/**
 * Проверить СГЕНЕРИРОВАННЫЙ `installer.nsi` на соответствие раскладке ресурсов.
 *
 * ## Зачем гейт на артефакте, а не только тест на исходнике
 *
 * Тест-страж `installer-path.test.cjs` ловит «кто-то снова позвал tauri build
 * напрямую». Но он проверяет ТЕКСТ ИСХОДНИКА, а дефект 27.11.2026 состоял в том,
 * что в артефакт уехала старая раскладка — при формально правильном коде вызова.
 * Поэтому проверяем то, что реально попадёт пользователю: сгенерированный NSIS-скрипт.
 *
 * Инвариант при `versionedResources: true`:
 *   - ресурсы обязаны лежать в `$INSTDIR\resources\<version>\…`;
 *   - `$INSTDIR\agents\`, `$INSTDIR\mcp_servers\`, `$INSTDIR\tasks_for_test_llm\`
 *     в корне быть НЕ ДОЛЖНО (это и есть источник агентов-призраков);
 *   - `rootResources` (redist, gateway) остаются в корне — их наличие не проверяем,
 *     их отсутствие не ломает резолв.
 *
 * Падение здесь означает: установщик НЕ будет выпущен. Пользователь не получит
 * «0 агентов» — сборка упадёт раньше.
 */
function assertVersionedLayout(cfg, installerPath) {
    if (cfg.versionedResources !== true) return;

    const nsiPath = path.join(path.dirname(installerPath), '..', 'nsis', 'x64', 'installer.nsi');
    // tauri-бандлер кладёт nsi в <target>/release/nsis/x64, а installer.exe —
    // в <target>/release/bundle/nsis. Путь может отличаться между версиями CLI,
    // поэтому ищем по обоим кандидатам, а не хардкодим один.
    const candidates = [
        nsiPath,
        path.join(resolveTargetDir(cfg), 'release', 'nsis', 'x64', 'installer.nsi'),
    ];
    const found = candidates.find((p) => fs.existsSync(p));
    if (!found) {
        console.warn(
            '[installer] ГЕЙТ ПРОПУЩЕН: installer.nsi не найден (проверьте раскладку ресурсов вручную). ' +
            `Искали: ${candidates.join(' | ')}`
        );
        return;
    }

    const src = fs.readFileSync(found, 'utf8');
    const version = readVersion(cfg);
    const versionedMarker = `$INSTDIR\\resources\\${version}\\`;
    const legacyMarkers = [
        '$INSTDIR\\agents\\',
        '$INSTDIR\\mcp_servers\\',
        '$INSTDIR\\tasks_for_test_llm\\',
    ];

    const versionedCount = src.split(versionedMarker).length - 1;
    if (versionedCount === 0) {
        throw new Error(
            `[installer] РАСКЛАДКА РЕСУРСОВ НЕВЕРНА: в ${found} нет ни одного пути ` +
            `"${versionedMarker}". Ресурсы уедут в корень установки, а exe ищет их в ` +
            `resources/${version}/ — пользователь получит 0 агентов (инцидент 27.11.2026). ` +
            `Установщик не выпущен.`
        );
    }

    const leaked = legacyMarkers.filter((m) => src.includes(m));
    if (leaked.length) {
        throw new Error(
            `[installer] РАСКЛАДКА РЕСУРСОВ НЕВЕРНА: в ${found} ресурсы дублируются в корень ` +
            `установки (${leaked.join(', ')}). Это источник агентов-призраков: NSIS /UPDATE не ` +
            `удаляет файлы, которых нет в текущей сборке. Установщик не выпущен.`
        );
    }

    console.log(
        `[installer] раскладка проверена: ${versionedCount} путей в resources/${version}/, ` +
        `legacy-корня нет`
    );
}

/**
 * Собрать NSIS-установщик — ЕДИНСТВЕННЫЙ путь сборки установщика в toolkit.
 *
 * ## Почему это отдельная функция, а не два вызова `npx tauri build`
 *
 * `installer.cjs` и `release.cjs` собирали установщик каждый по-своему:
 * `installer.cjs` через `stageVersionedResources()` (правильная раскладка
 * `resources/<version>/`), а `release.cjs` — голым `npx tauri build --bundles nsis`
 * БЕЗ `--config`. Следствие (регресс 27.11.2026): ручной `generate_installer.bat`
 * давал рабочий установщик, а `release.bat` — установщик со старой раскладкой
 * `$INSTDIR\agents\…`. Пользователь обновился релизом 26.11.13 и получил
 * «0 агентов»: exe ищет `resources\26.11.13\agents`, а его там нет.
 *
 * Разные пути сборки одного артефакта — это и есть причина, поэтому здесь один
 * путь. Оба сценария обязаны звать `buildNsisInstaller`, а не `npx tauri build`.
 *
 * Предусловия (prep/killApp/vcRedist) остаются у вызывающего: у installer и
 * release они разные по смыслу (release ещё чистит bundle и ставит strict-подпись).
 */
function buildNsisInstaller(cfg) {
    const versionedConf = stageVersionedResources(cfg);
    try {
        const buildArgs = ['tauri', 'build', '--bundles', 'nsis'];
        if (versionedConf) buildArgs.push('--config', path.relative(cfg.projectRoot, versionedConf));
        sh(cfg, 'npx', buildArgs, { echo: true });
    } finally {
        // Временный конфиг не должен попасть ни в git, ни в релиз.
        if (versionedConf) fs.rmSync(versionedConf, { force: true });
    }

    const version = readVersion(cfg);
    const installer = findInstallerBundle(BUNDLE_DIR(cfg), version);
    if (!installer) {
        throw new Error(`Installer not found in ${BUNDLE_DIR(cfg)} for version ${version}`);
    }
    // Гейт на артефакте: битый установщик не должен покинуть сборку.
    assertVersionedLayout(cfg, installer);
    return installer;
}

async function run(cfg) {
    if (cfg.kind === 'cargo') return runCargoInstaller(cfg);

    await prep(cfg);

    console.log('\n=== installer ===');
    cleanupOldBundle(cfg);

    killApp(cfg); // освободить file lock exe до пересборки
    setupSigningEnv(cfg, { strict: false });
    ensureBundleActive(cfg);

    // VC++ runtime DLL копируются ДО сборки, чтобы попасть в NSIS-бандл.
    copyVcRedistTo(cfg, path.join(resolveTargetDir(cfg), 'release'));

    const installer = buildNsisInstaller(cfg);
    console.log('Installer:', installer);

    signInstaller(cfg, installer, { strict: false });

    console.log('\n[installer] done.');
    return installer;
}

module.exports = { run, buildNsisInstaller };
