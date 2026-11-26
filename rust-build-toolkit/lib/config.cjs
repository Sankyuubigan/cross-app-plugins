// Загрузка конфигурации проекта (.build-config.json) + автодетект.
//
// Единая точка, где резолвится projectRoot. ВСЯ работа со сборкой/релизом
// выполняется строго относительно projectRoot — это исключает «утечку»
// операций в папку плагина или в чужой проект.

const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const CONFIG_FILENAME = '.build-config.json';
const DEFAULT_SIGNING_KEYS_DIR =
    'D:\\Projects\\docusaurus-starter\\docs\\Sega Mega Note\\Моя картотека\\software\\настройки\\tauri_signed_keys';

function readJson(filePath) {
    return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function hasTauriProject(root) {
    return fs.existsSync(path.join(root, 'src-tauri', 'tauri.conf.json'));
}

// Cargo-проект (без Tauri): Cargo.toml в корне / src-tauri / src.
// С 16.11.2026 тулкит универсален: tauri И cargo (GPUI и прочие Rust-приложения).
function hasCargoProject(root) {
    for (const d of [root, path.join(root, 'src-tauri'), path.join(root, 'src')]) {
        if (fs.existsSync(path.join(d, 'Cargo.toml'))) return true;
    }
    return false;
}

// kind проекта: явный cfgRaw.kind ('tauri'|'cargo') сильнее автодетекта.
// Автодетект: tauri.conf.json → tauri; иначе Cargo.toml → cargo.
function detectProjectKind(root, cfgRaw) {
    const explicit = cfgRaw && typeof cfgRaw.kind === 'string' ? cfgRaw.kind.trim().toLowerCase() : null;
    if (explicit) {
        if (explicit !== 'tauri' && explicit !== 'cargo') {
            throw new Error(`kind="${cfgRaw.kind}" не поддерживается (разрешено: "tauri" | "cargo")`);
        }
        return explicit;
    }
    if (hasTauriProject(root)) return 'tauri';
    if (hasCargoProject(root)) return 'cargo';
    throw new Error(
        `Не удалось определить тип проекта: нет src-tauri/tauri.conf.json и нет Cargo.toml (корень / src-tauri / src).\n` +
        `Явно укажи "kind": "tauri" или "kind": "cargo" в .build-config.json.`
    );
}

// Папка крейта: src-tauri для tauri; для cargo — где нашёлся Cargo.toml.
function resolveCrateDir(root, kind) {
    if (kind === 'tauri') return path.join(root, 'src-tauri');
    for (const d of [root, path.join(root, 'src-tauri'), path.join(root, 'src')]) {
        if (fs.existsSync(path.join(d, 'Cargo.toml'))) return d;
    }
    throw new Error(`Cargo.toml не найден для cargo-проекта (${root}, src-tauri, src)`);
}

function looksLikeProject(root) {
    return (
        hasTauriProject(root) ||
        hasCargoProject(root) ||
        fs.existsSync(path.join(root, CONFIG_FILENAME))
    );
}

function resolveProjectRoot(projectArg, cwd) {
    if (projectArg && looksLikeProject(projectArg)) {
        return path.resolve(projectArg);
    }
    if (looksLikeProject(cwd)) {
        return path.resolve(cwd);
    }
    throw new Error(
        `Не найден корень проекта (нужны src-tauri/tauri.conf.json ИЛИ Cargo.toml ИЛИ .build-config.json).\n` +
        `  --project: ${projectArg}\n  cwd: ${cwd}\n` +
        `Запускай скрипт из корня проекта или передай --project "<корень>".`
    );
}

function readTauriConf(projectRoot) {
    return readJson(path.join(projectRoot, 'src-tauri', 'tauri.conf.json'));
}

function readCargoName(crateDir) {
    const text = fs.readFileSync(path.join(crateDir, 'Cargo.toml'), 'utf8');
    const articles = text.match(/^name\s*=\s*"([^"]+)"/m);
    return articles ? articles[1] : null;
}

// Репо можно вывести из tauri.conf.json (plugins.about-updates.repo или updater.endpoints).
function detectRepoFromTauri(tauriConf) {
    const plugins = tauriConf.plugins || {};
    if (plugins['about-updates'] && plugins['about-updates'].repo) {
        return plugins['about-updates'].repo;
    }
    if (plugins.updater && Array.isArray(plugins.updater.endpoints)) {
        for (const ep of plugins.updater.endpoints) {
            const m = String(ep).match(/github\.com\/([^/]+\/[^/]+)\/(?:main|master|head)\/latest\.json/);
            if (m) return m[1];
        }
    }
    return null;
}

function normalizeRepo(repo) {
    if (!repo) return null;
    return String(repo)
        .replace(/^https?:\/\/(www\.)?github\.com\//, '')
        .replace(/^git@github\.com:/, '')
        .replace(/\.git$/, '')
        .replace(/\/$/, '');
}

function gitCurrentBranch(projectRoot) {
    try {
        return execSync(`git -C "${projectRoot}" branch --show-current`, {
            encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'],
        }).trim();
    } catch (e) {
        return null;
    }
}

function gitRemoteOrigin(projectRoot) {
    try {
        return execSync(`git -C "${projectRoot}" remote get-url origin`, {
            encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'],
        }).trim();
    } catch (e) {
        return null;
    }
}

// Единый источник правды о проекте. cfgRaw — содержимое .build-config.json
// (только отличия от дефолта), всё остальное либо авто-детектится, либо берёт
// безопасный дефолт. Плагин НЕ содержит имён/репозиториев конкретных проектов.
function loadProjectConfig(projectArg, cwd) {
    const projectRoot = resolveProjectRoot(projectArg, cwd);

    let cfgRaw = {};
    const cfgPath = path.join(projectRoot, CONFIG_FILENAME);
    if (fs.existsSync(cfgPath)) {
        cfgRaw = readJson(cfgPath) || {};
    }

    const kind = detectProjectKind(projectRoot, cfgRaw);
    // Явный crateDir из конфига перекрывает автодетект — обязателен для
    // воркспейсов, где корневой Cargo.toml — [workspace] без [package]
    // (версии/имя лежат в member-крейте).
    const crateDir = cfgRaw.crateDir
        ? path.resolve(projectRoot, cfgRaw.crateDir)
        : resolveCrateDir(projectRoot, kind);
    if (!fs.existsSync(path.join(crateDir, 'Cargo.toml'))) {
        throw new Error(`crateDir="${cfgRaw.crateDir}" не содержит Cargo.toml (${crateDir})`);
    }
    // tauri.conf.json есть только у tauri-проекта; cargo-конфигурации читают
    // имя/продукт из Cargo.toml.
    const tauriConf = kind === 'tauri' ? readTauriConf(projectRoot) : null;
    const cargoName = readCargoName(crateDir);
    const origin = gitRemoteOrigin(projectRoot);
    const fallbackRepo =
        normalizeRepo(cfgRaw.repo) ||
        normalizeRepo(kind === 'tauri' ? detectRepoFromTauri(tauriConf) : null) ||
        normalizeRepo(origin);

    const signing = cfgRaw.signing && typeof cfgRaw.signing === 'object' ? cfgRaw.signing : {};

    return {
        projectRoot,
        configPath: cfgPath,
        configRaw: cfgRaw,

        // Тип проекта ('tauri' | 'cargo') и папка крейта — всюду ниже по пайплайну
        // ветвление идёт по этим полям, а не по наличию файлов.
        kind,
        crateDir,

        appExe: cfgRaw.appExe || cargoName || null,
        productName: cfgRaw.productName || (tauriConf && tauriConf.productName) || cargoName || null,
        repo: fallbackRepo,
        branch: cfgRaw.branch || gitCurrentBranch(projectRoot) || 'main',

        npmInstallArgs: Array.isArray(cfgRaw.npmInstallArgs) ? cfgRaw.npmInstallArgs : ['--legacy-peer-deps'],
        syncDirs: Array.isArray(cfgRaw.syncDirs) ? cfgRaw.syncDirs : [],

        // Версионные ресурсы (opt-in, 26.11.2026): бандлимые ресурсы Tauri
        // кладутся не в корень $INSTDIR, а в resources/<version>/. Причина —
        // NSIS /UPDATE не удаляет файлы, которых нет в текущей сборке: призраки
        // прошлых версий остаются в $INSTDIR навсегда и участвуют в резолве
        // (инцидент: старый shadow_worker из archive/ отвечал в чате вместо
        // актуального frontend/). rootResources — source-ключи bundle.resources,
        // которые остаются в корне (redist/, gateway exe рядом с хостом).
        versionedResources: cfgRaw.versionedResources === true,
        rootResources: Array.isArray(cfgRaw.rootResources) ? cfgRaw.rootResources : [],
        devWindow: cfgRaw.devWindow || null,

        // Временная папка сборки. null → <projectRoot>/temp/buildtmp (см.
        // lib/scratch.cjs). Переопределение нужно для ASCII-пути, когда путь
        // проекта содержит кириллицу, либо чтобы вынести temp на короткий путь.
        buildTemp: typeof cfgRaw.buildTemp === 'string' && cfgRaw.buildTemp.trim()
            ? cfgRaw.buildTemp.trim()
            : null,

        vcRedist: cfgRaw.vcRedist === true,
        vcRedistDir: cfgRaw.vcRedistDir || 'redist',
        vcRedistDlls: Array.isArray(cfgRaw.vcRedistDlls) && cfgRaw.vcRedistDlls.length
            ? cfgRaw.vcRedistDlls
            : ['vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll', 'msvcp140_1.dll', 'concrt140.dll'],

        // kind=cargo: NSIS-упаковка. nsisPath — путь к makensis.exe ИЛИ папке
        // NSIS (Bin/makensis.exe резолвится); null → автодетект (tauri-NSIS,
        // Program Files). publisher — метаданные установщика (дефолт productName).
        nsisPath: cfgRaw.nsisPath || null,
        publisher: cfgRaw.publisher || null,

        signing: {
            keyPath: signing.keyPath || null,
            passwordFile: signing.passwordFile || null,
            password: signing.password || null,
        },
        defaultKeysDir: process.env.TAURI_KEYS_DIR || DEFAULT_SIGNING_KEYS_DIR,

        syncPackageJsonVersion: cfgRaw.syncPackageJsonVersion === true,
        releaseNotesTemplate: cfgRaw.releaseNotesTemplate || 'Auto release ${product} ${tag}',

        latestJsonPath: path.join(projectRoot, 'latest.json'),
    };
}

// ── Резолв папки cargo-артефактов (target dir) ──
//
// Единственный источник правды — сам cargo (`cargo metadata`): он учитывает
// и ~/.cargo/config.toml ([build] target-dir), и env CARGO_TARGET_DIR.
// Хардкод <projectRoot>/src-tauri/target ломает проекты с общим target-dir:
// после успешной сборки тулкит не находил exe ("Built exe not found").
// TAURI_CARGO_TARGET_DIR — отладочный override с наивысшим приоритетом.
// Fallback — старый путь + WARN: конфигурации без общего target-dir работают
// как раньше, ничего не падает.

const targetDirCache = new Map(); // projectRoot -> dir

function cargoTargetDirFromMetadata(root) {
    // Cargo.toml может быть в корне (workspace) или в src-tauri — пробуем оба.
    for (const cwd of [root, path.join(root, 'src-tauri')]) {
        try {
            const out = execSync('cargo metadata --no-deps --format-version 1', {
                cwd,
                encoding: 'utf8',
                maxBuffer: 64 * 1024 * 1024,
                stdio: ['ignore', 'pipe', 'ignore'],
            });
            const meta = JSON.parse(out);
            if (meta && meta.target_directory) return meta.target_directory;
        } catch (e) {
            // нет Cargo.toml в этом каталоге или cargo недоступен — пробуем следующий.
        }
    }
    return null;
}

function resolveTargetDir(cfg) {
    const root = cfg.projectRoot;
    if (targetDirCache.has(root)) return targetDirCache.get(root);

    let dir = null;
    let source = null;

    const envDir = process.env.TAURI_CARGO_TARGET_DIR;
    if (envDir && envDir.trim()) {
        dir = path.resolve(envDir.trim());
        source = 'env TAURI_CARGO_TARGET_DIR';
    } else {
        dir = cargoTargetDirFromMetadata(root);
        if (dir) source = 'cargo metadata';
    }

    if (!dir) {
        dir = cfg.kind === 'tauri' ? path.join(root, 'src-tauri', 'target') : path.join(root, 'target');
        source = 'fallback';
        console.warn(`[target-dir] WARN: cargo metadata недоступен, использую ${dir}`);
    }

    targetDirCache.set(root, dir);
    console.log(`[target-dir] ${dir} (источник: ${source})`);
    return dir;
}

module.exports = {
    resolveTargetDir,
    loadProjectConfig,
    resolveProjectRoot,
    readJson,
    readTauriConf,
    readCargoName,
    detectProjectKind,
    resolveCrateDir,
    normalizeRepo,
    gitCurrentBranch,
    gitRemoteOrigin,
    CONFIG_FILENAME,
};