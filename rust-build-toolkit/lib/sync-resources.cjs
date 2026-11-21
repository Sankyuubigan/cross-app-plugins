// Синхронизация каталогов из корня проекта в src-tauri/resources (для каждого
// имени из config.syncDirs). Заменяет scripts/sync-resources.cjs из эталона.

// Sync-зеркалирование каталогов из корня проекта в target/{debug,release}.
// Каждый элемент cfg.syncDirs — путь относительно projectRoot (может быть вложенным,
// напр. "src-tauri/mcp_servers"); имя итоговой папки = basename(src).
//
// Зачем (эталон King Orch): Tauri копирует ресурсы аддитивно, не удаляя файлы,
// выпиленные из исходников → в target-папках накапливаются «призраки». Здесь
// target-копия зеркалит исходник (копирует новое, удаляет отсутствующее).

const fs = require('fs');
const path = require('path');
const { resolveTargetDir } = require('./config.cjs');

function syncDir(src, dest) {
    if (!fs.existsSync(dest)) fs.mkdirSync(dest, { recursive: true });
    const srcNames = new Set(fs.readdirSync(src));
    for (const name of fs.readdirSync(dest)) {
        if (!srcNames.has(name)) {
            fs.rmSync(path.join(dest, name), { recursive: true, force: true });
        }
    }
    for (const name of srcNames) {
        const s = path.join(src, name);
        const d = path.join(dest, name);
        const st = fs.statSync(s);
        if (st.isDirectory()) syncDir(s, d);
        else if (st.isFile()) fs.copyFileSync(s, d);
    }
}

function syncConfiguredResources(cfg) {
    const entries = cfg.syncDirs;
    if (!entries || entries.length === 0) {
        console.log('syncDirs: not configured, skip resource sync.');
        return;
    }
    const targetDir = resolveTargetDir(cfg);
    for (const rel of entries) {
        const src = path.join(cfg.projectRoot, rel);
        if (!fs.existsSync(src)) {
            console.warn(`syncDirs: source not found, skipped: ${rel}`);
            continue;
        }
        const relName = path.basename(rel);
        for (const profile of ['debug', 'release']) {
            const dest = path.join(targetDir, profile, relName);
            // Каталог назначения создаём: в свежем общем target-dir папки
            // release/debug до первой сборки нет, но ресурсы обязаны попасть
            // в бинарь (раньше был тихий skip — в общем target-dir это
            // безвозвратно теряло ресурсы первой сборки).
            fs.mkdirSync(path.dirname(dest), { recursive: true });
            syncDir(src, dest);
            console.log(`[sync-resources] ${relName}/ -> ${dest}`);
        }
    }
}

// Скопировать syncDirs (basename каждой папки) в произвольную папку рядом с
// exe — для kind=cargo, где Tauri-ресурсов нет: destDir = папка exe при build
// и NSIS-стейдж при упаковке. Зеркалирование то же (syncDir). Ошибка источника
// — warn (как в syncConfiguredResources); ошибка копирования — throw:
// сборка/упаковка падают, молчаливой потери файлов нет.
function copySyncDirsTo(cfg, destDir) {
    const entries = cfg.syncDirs;
    if (!entries || entries.length === 0) return 0;
    fs.mkdirSync(destDir, { recursive: true });
    let count = 0;
    for (const rel of entries) {
        const src = path.join(cfg.projectRoot, rel);
        if (!fs.existsSync(src)) {
            console.warn(`syncDirs: source not found, skipped: ${rel}`);
            continue;
        }
        const dest = path.join(destDir, path.basename(rel));
        syncDir(src, dest);
        console.log(`[sync-resources] ${path.basename(rel)}/ -> ${dest}`);
        count++;
    }
    return count;
}

module.exports = { syncConfiguredResources, syncDir, copySyncDirsTo };