// node --test lib — страж: релизный путь собирает установщик ТОЛЬКО через
// общую функцию, применяющую версионный конфиг ресурсов.
//
// Регресс 27.11.2026: `release.cjs` собирал `npx tauri build --bundles nsis`
// напрямую, без `--config` → установщик уехал со старой раскладкой
// `$INSTDIR\agents\…`, exe искал `resources\<version>\agents` и находил 0 агентов.
// Ручной `generate_installer.bat` при этом работал — расхождение путей сборки
// одного артефакта и было причиной. Этот тест запрещает расхождению вернуться.

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const read = (f) => fs.readFileSync(path.join(__dirname, f), 'utf8');

test('release.cjs не собирает установщик напрямую — только через buildNsisInstaller', () => {
    const src = read('release.cjs');

    // Прямой вызов tauri build в релизном пути = обход stageVersionedResources.
    assert.doesNotMatch(
        src,
        /\[\s*['"]tauri['"]\s*,\s*['"]build['"]/,
        'release.cjs не должен вызывать `npx tauri build` напрямую: ' +
        'только buildNsisInstaller применяет версионный конфиг ресурсов'
    );
    assert.match(src, /buildNsisInstaller\s*\(/, 'release.cjs обязан звать buildNsisInstaller');
    assert.match(
        src,
        /require\(['"]\.\/installer\.cjs['"]\)/,
        'buildNsisInstaller берётся из installer.cjs — единственного места сборки'
    );
});

test('installer.cjs экспортирует buildNsisInstaller и применяет версионный конфиг', () => {
    const src = read('installer.cjs');

    assert.match(
        src,
        /module\.exports\s*=\s*\{[^}]*buildNsisInstaller/,
        'buildNsisInstaller должен быть экспортирован — им пользуется release.cjs'
    );
    // Внутри единой функции версионный конфиг обязан применяться.
    const fn = src.slice(src.indexOf('function buildNsisInstaller'));
    assert.match(fn, /stageVersionedResources\s*\(/, 'сборка обязана применять версионный конфиг');
    assert.match(fn, /--config/, 'версионный конфиг передаётся в tauri build через --config');
    assert.match(fn, /finally/, 'временный конфиг удаляется в finally (не течёт в git/релиз)');
});

test('ручной путь сборки тоже идёт через buildNsisInstaller, а не дублирует логику', () => {
    const src = read('installer.cjs');
    const runFn = src.slice(src.indexOf('async function run(cfg)'));

    assert.match(runFn, /buildNsisInstaller\s*\(/, 'installer.run обязан звать общую функцию');
    assert.doesNotMatch(
        runFn,
        /\[\s*['"]tauri['"]\s*,\s*['"]build['"]/,
        'installer.run не должен дублировать вызов tauri build'
    );
});
