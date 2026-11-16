// build / prep. Единый prep (version + sync + npm + icons) используется и
// installer, и release. Dev-overrides (bundle.active=false, devWindow) — только
// для dev-сборки; npm-install и бамп версии остаются как в эталоне.

const fs = require('fs');
const path = require('path');

const { resolveTargetDir } = require('./config.cjs');
const { run: sh, launchApp, killApp } = require('./run.cjs');
const { bumpVersion, readVersion } = require('./version.cjs');
const { ensureIcons } = require('./icons.cjs');
const { syncConfiguredResources } = require('./sync-resources.cjs');
const { copyVcRedistIfConfigured } = require('./copy-vc-redist.cjs');
const { checkProjectEncodings } = require('./encoding-check.cjs');

const BUNDLE_DIR = (cfg) => path.join(resolveTargetDir(cfg), 'release', 'bundle');
const RELEASE_EXE = (cfg) =>
    cfg.appExe ? path.join(resolveTargetDir(cfg), 'release', `${cfg.appExe}.exe`) : null;

function readTauriJson(cfg) {
    const p = path.join(cfg.projectRoot, 'src-tauri', 'tauri.conf.json');
    return JSON.parse(fs.readFileSync(p, 'utf8'));
}

function writeTauriJson(cfg, obj) {
    const p = path.join(cfg.projectRoot, 'src-tauri', 'tauri.conf.json');
    fs.writeFileSync(p, JSON.stringify(obj, null, 2) + '\n', 'utf8');
}

function runNpmInstall(cfg) {
    const pkg = path.join(cfg.projectRoot, 'package.json');
    if (!fs.existsSync(pkg)) {
        console.log('package.json not found, skip npm install.');
        return;
    }
    const args = ['install', ...(cfg.npmInstallArgs || [])];
    sh(cfg, 'npm', args, { echo: true });
}

// Автосборка локальных file: плагинов перед сборкой хоста.
// Плагины подключены через "file:../cross-app-plugins/..." — npm копирует их
// в node_modules при install, но НЕ компилирует guest-js → dist-js.
// Без этого шага хост подтягивает старый скомпилированный dist-js.
function buildLocalPlugins(cfg) {
    const pkgPath = path.join(cfg.projectRoot, 'package.json');
    if (!fs.existsSync(pkgPath)) return;
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
    const allDeps = { ...pkg.dependencies, ...pkg.devDependencies };
    const fileDeps = Object.entries(allDeps)
        .filter(([, v]) => typeof v === 'string' && v.startsWith('file:'))
        .map(([name, v]) => ({ name, dir: path.resolve(cfg.projectRoot, v.slice(5)) }));

    for (const { name, dir } of fileDeps) {
        const pluginPkgPath = path.join(dir, 'package.json');
        if (!fs.existsSync(pluginPkgPath)) continue;
        const pluginPkg = JSON.parse(fs.readFileSync(pluginPkgPath, 'utf8'));
        const scripts = pluginPkg.scripts || {};
        if (!scripts.build && !scripts['build:global']) continue;

        console.log(`\n[plugins] Building local plugin: ${name} (${dir})`);
        if (scripts.build) {
            sh(cfg, 'npm', ['run', 'build'], { cwd: dir, echo: true });
        }
        if (scripts['build:global']) {
            sh(cfg, 'npm', ['run', 'build:global'], { cwd: dir, echo: true });
        }
        console.log(`[plugins] Plugin ${name} built.`);
    }
}

// Dev-оверрайд записывается ОТДЕЛЬНЫМ файлом и подмешивается через
// `npx tauri build --config <override>` — коммиченный tauri.conf.json не
// трогается (как в эталоне). bundle отключается только для dev; окно —
// devWindow из конфига (иначе не трогаем app.windows).
function buildDevOverride(cfg) {
    const override = { bundle: { active: false } };
    if (cfg.devWindow) {
        override.app = { windows: [cfg.devWindow] };
    }
    return override;
}

// Для installer/release: убедиться, что бамдлинг включён (dev-сборка могла его отключить).
function ensureBundleActive(cfg) {
    const conf = readTauriJson(cfg);
    if (conf.bundle && conf.bundle.active === false) {
        conf.bundle.active = true;
        writeTauriJson(cfg, conf);
        console.log('Bundle active restored (true).');
    }
}

// Папки, НЕ являющиеся входами cargo-сборки (артефакты/окружение).
const NON_INPUT_DIRS = new Set(['target', 'node_modules', 'bundle', '.git']);

// Самый свежий mtime среди входов cargo-сборки: исходники, Cargo-манифесты,
// tauri.conf.json, иконки, .cargo/config.toml. Прокси для «есть ли что
// пересобирать» — cargo сам не сообщает, было ли что-то перекомпилировано.
function newestInputMtime(cfg) {
    let newest = 0;
    const roots = [
        path.join(cfg.projectRoot, 'Cargo.toml'),
        path.join(cfg.projectRoot, 'Cargo.lock'),
        path.join(cfg.projectRoot, '.cargo'),
        path.join(cfg.projectRoot, 'crates'),
        path.join(cfg.projectRoot, 'src-tauri'),
    ];
    const walk = (p) => {
        let st;
        try {
            st = fs.statSync(p);
        } catch {
            return;
        }
        if (st.isDirectory()) {
            if (NON_INPUT_DIRS.has(path.basename(p))) return;
            for (const e of fs.readdirSync(p)) walk(path.join(p, e));
            return;
        }
        if (st.mtimeMs > newest) newest = st.mtimeMs;
    };
    for (const r of roots) walk(r);
    return newest;
}

// Проверка exe после сборки. Легитимны два исхода: exe пересобран этим
// запуском (mtime >= начало сборки) ИЛИ cargo честно решил, что нечего
// пересобирать (без исходных изменений exe остаётся прежним — это НЕ провал).
// Провал — только когда входы сборки новее exe, а exe не обновился: значит
// сборка реально не пересобрала то, что должна была.
function checkExeFresh(cfg, buildStartMs) {
    const exe = RELEASE_EXE(cfg);
    if (!exe || !fs.existsSync(exe)) {
        throw new Error(`Built exe not found: ${exe}`);
    }
    const mtimeMs = fs.statSync(exe).mtimeMs;
    if (buildStartMs !== undefined && mtimeMs >= buildStartMs) return exe;
    if (newestInputMtime(cfg) > mtimeMs) {
        const ageSec = Math.round((Date.now() - mtimeMs) / 1000);
        throw new Error(`Built exe is stale (${ageSec}s). Inputs are newer than exe, build did not refresh it: ${exe}`);
    }
    console.log('[build] exe up-to-date (cargo: nothing to rebuild).');
    return exe;
}

function getExePath(cfg) {
    const exe = RELEASE_EXE(cfg);
    if (!exe || !fs.existsSync(exe)) return null;
    return exe;
}

// --- prep: version bump + sync resources + npm install + icons. ---
function prep(cfg, opts = {}) {
    console.log('=== prep ===');
    checkProjectEncodings(cfg);
    const bump = opts.bump !== false;
    if (bump) {
        const oldVersion = readVersion(cfg.projectRoot);
        const newVersion = bumpVersion(cfg.projectRoot, { syncPackageJson: cfg.syncPackageJsonVersion });
        console.log(`Version: ${oldVersion} -> ${newVersion}`);
    } else {
        console.log(`Version: ${readVersion(cfg.projectRoot)} (kept)`);
    }
    syncConfiguredResources(cfg);
    buildLocalPlugins(cfg);
    runNpmInstall(cfg);
    return ensureIcons(cfg);
}

// --- build (dev) ---
async function run(cfg, opts = {}) {
    const prepOnly = opts.prepOnly === true;
    const bump = opts.bump !== undefined ? opts.bump : prepOnly;

    await prep(cfg, { bump });
    if (prepOnly) {
        console.log('\n[prep] done. Run `installer` or `release` next.');
        return { skipped: true };
    }

    console.log('\n=== build (dev) ===');
    const ovrPath = path.join(cfg.projectRoot, 'src-tauri', 'tauri-dev-override.json');

    killApp(cfg); // освободить file lock exe до пересборки
    fs.writeFileSync(ovrPath, JSON.stringify(buildDevOverride(cfg), null, 2) + '\n', 'utf8');
    const buildStart = Date.now();
    try {
        sh(cfg, 'npx', ['tauri', 'build', '--config', ovrPath], { echo: true });
    } finally {
        fs.rmSync(ovrPath, { force: true });
    }

    const exe = checkExeFresh(cfg, buildStart);
    copyVcRedistIfConfigured(cfg, exe);

    // launchApp: false → ранний краш (PANIC/missing DLL) — фейлим билд, не молчим.
    const ok = await launchApp(cfg, exe);
    if (ok === false) {
        console.error('\n[build] FAILED: app did not stay alive after launch.');
        process.exitCode = 1;
        return { exe, launched: false };
    }
    console.log('\n[build] done.');
    return { exe, launched: true };
}

module.exports = {
    run,
    prep,
    buildDevOverride,
    buildLocalPlugins,
    ensureBundleActive,
    checkExeFresh,
    getExePath,
    BUNDLE_DIR,
    RELEASE_EXE,
};