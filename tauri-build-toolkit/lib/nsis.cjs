// NSIS-упаковка для kind=cargo (без Tauri bundler): детект makensis, запуск
// templates/app.nsi с /D-дефайнами. Нет NSIS → portable-zip (честный fallback,
// не тихий пропуск).
//
// Артефакт кладётся в <target>/release/bundle/nsis/<product>_<version>_x64-setup.exe —
// то же место и тот же паттерн имени, что у tauri-ветки: findInstallerBundle,
// release.cjs и github.cjs работают без изменений (анти-паттерн §3.7 соблюдён:
// точное совпадение `_X.Y.Z_x64-setup.exe`, никакого префиксного матчинга).
//
// Стейджинг: общий target-dir означает, что рядом с exe лежат чужие артефакты,
// поэтому в пакет собираем ТОЛЬКО exe приложения + vcRedist-DLL из конфига.

const fs = require('fs');
const path = require('path');
const { run: sh } = require('./run.cjs');
const { resolveTargetDir } = require('./config.cjs');
const { readVersion } = require('./version.cjs');

const APP_NSI = path.join(__dirname, '..', 'templates', 'app.nsi');

// cfg.nsisPath — файл makensis.exe ИЛИ папка NSIS (Bin/makensis.exe резолвится).
// Автодетект: tauri-скачанный NSIS (%LOCALAPPDATA%\tauri\NSIS) → Program Files.
function detectMakensis(cfg) {
    const candidates = [];
    if (cfg.nsisPath) {
        let st = null;
        try { st = fs.statSync(cfg.nsisPath); } catch (e) { st = null; }
        if (st && st.isDirectory()) {
            candidates.push(path.join(cfg.nsisPath, 'Bin', 'makensis.exe'));
            candidates.push(path.join(cfg.nsisPath, 'makensis.exe'));
        } else {
            candidates.push(cfg.nsisPath);
        }
    }
    if (process.env.LOCALAPPDATA) {
        candidates.push(path.join(process.env.LOCALAPPDATA, 'tauri', 'NSIS', 'Bin', 'makensis.exe'));
    }
    candidates.push('C:\\Program Files (x86)\\NSIS\\Bin\\makensis.exe');
    candidates.push('C:\\Program Files\\NSIS\\Bin\\makensis.exe');
    for (const c of candidates) {
        if (fs.existsSync(c)) return c;
    }
    return null;
}

function stageCargoFiles(cfg, exePath) {
    const stageDir = path.join(resolveTargetDir(cfg), 'release', 'bundle', 'cargo-stage');
    fs.rmSync(stageDir, { recursive: true, force: true });
    fs.mkdirSync(stageDir, { recursive: true });

    const exeName = `${cfg.appExe}.exe`;
    fs.copyFileSync(exePath, path.join(stageDir, exeName));

    let dllCount = 0;
    for (const dll of cfg.vcRedistDlls || []) {
        const src = path.join(path.dirname(exePath), dll);
        if (fs.existsSync(src)) {
            fs.copyFileSync(src, path.join(stageDir, dll));
            dllCount++;
        }
    }
    return { stageDir, exeName, dllCount };
}

// Fallback без NSIS: exe + DLL в zip. Подписи нет (minisign ставится на
// setup.exe) — это честно сообщается вызывающему по расширению артефакта.
function buildPortableZip(cfg, stageDir, outDir, version, productName) {
    const zipPath = path.join(outDir, `${productName}_${version}_x64-portable.zip`);
    sh(cfg, 'powershell', [
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command',
        `Compress-Archive -Path '${stageDir}\\*' -DestinationPath '${zipPath}' -Force`,
    ], { echo: true });
    if (!fs.existsSync(zipPath)) {
        throw new Error(`Portable zip не создан: ${zipPath}`);
    }
    console.log(`[portable] ${zipPath}`);
    return zipPath;
}

// Возвращает путь к установщику (setup.exe) или portable-zip.
function buildCargoInstaller(cfg, exePath) {
    const version = readVersion(cfg);
    const productName = cfg.productName || cfg.appExe;
    if (!productName || !cfg.appExe) {
        throw new Error(`Не определено имя приложения (appExe) для cargo-проекта: ${cfg.projectRoot}`);
    }
    const outDir = path.join(resolveTargetDir(cfg), 'release', 'bundle', 'nsis');
    fs.mkdirSync(outDir, { recursive: true });

    const { stageDir, exeName, dllCount } = stageCargoFiles(cfg, exePath);

    const makensis = detectMakensis(cfg);
    if (!makensis) {
        console.warn(
            '[nsis] makensis не найден (nsisPath не задан; автодетект: %LOCALAPPDATA%\\tauri\\NSIS, Program Files).\n' +
            '[nsis] Собираю portable-zip вместо установщика. Установи NSIS или задай "nsisPath" в .build-config.json.'
        );
        return buildPortableZip(cfg, stageDir, outDir, version, productName);
    }

    const outFile = path.join(outDir, `${productName}_${version}_x64-setup.exe`);
    const defines = [
        `/DPRODUCT_NAME=${productName}`,
        `/DVERSION=${version}`,
        `/DPUBLISHER=${cfg.publisher || productName}`,
        `/DSOURCE_DIR=${stageDir}`,
        `/DEXE_NAME=${exeName}`,
        `/DOUT_FILE=${outFile}`,
        `/DINSTALL_SUBDIR=${cfg.appExe}`,
        `/DUNINST_KEY=${cfg.appExe}`,
    ];
    sh(cfg, makensis, [...defines, APP_NSI], { echo: true });
    if (!fs.existsSync(outFile)) {
        throw new Error(`makensis завершился, но установщика нет: ${outFile}`);
    }
    console.log(`[nsis] installer: ${outFile} (vcRedist dlls staged: ${dllCount})`);
    return outFile;
}

module.exports = { buildCargoInstaller, detectMakensis };
