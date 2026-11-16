// Единый скрипт синхронизации версий (правило YY.M.P из глобальной доки §3.2).
// Все сценарии (build/release/prep) используют ТОЛЬКО эти функции — никаких
// отдельных реализаций бампа в оркестраторах.
//
// SSOT версии зависит от kind проекта (16.11.2026, универсализация cargo —
// поддержка GPUI и прочих Rust-приложений без Tauri):
//   tauri → src-tauri/tauri.conf.json (Cargo.toml синхронизируется рядом);
//   cargo → [package] version в Cargo.toml (корень / src-tauri / src).
// Функции принимают cfg (результат loadProjectConfig: cfg.kind, cfg.crateDir).

const fs = require('fs');
const path = require('path');

const TAURI_VERSION_RE = /"version"\s*:\s*"(\d+)\.(\d+)\.(\d+)"/;
const CARGO_VERSION_RE = /^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"/m;

// Папка крейта: cfg.crateDir (ставит loadProjectConfig) либо скан
// (корень → src-tauri → src). Локальная копия скана нужна, чтобы version.cjs
// не импортировал config.cjs (там execSync/кеш; циклов зависимостей не заводим).
function crateDirOf(cfg) {
    if (cfg.crateDir && fs.existsSync(path.join(cfg.crateDir, 'Cargo.toml'))) {
        return cfg.crateDir;
    }
    for (const d of [cfg.projectRoot, path.join(cfg.projectRoot, 'src-tauri'), path.join(cfg.projectRoot, 'src')]) {
        if (fs.existsSync(path.join(d, 'Cargo.toml'))) return d;
    }
    throw new Error(`Cargo.toml не найден (${cfg.projectRoot}, src-tauri, src)`);
}

// --- чтение ---
function readVersion(cfg) {
    if (cfg.kind === 'cargo') {
        const cargoPath = path.join(crateDirOf(cfg), 'Cargo.toml');
        const text = fs.readFileSync(cargoPath, 'utf8');
        const pkgIdx = text.indexOf('[package]');
        // Только [package]: в воркспейсе `version = "..."` встречается и у зависимостей.
        const scope = pkgIdx === -1 ? text : text.slice(pkgIdx);
        const m = scope.match(CARGO_VERSION_RE);
        if (!m) throw new Error(`Версия не найдена в [package] (${cargoPath})`);
        return `${m[1]}.${m[2]}.${m[3]}`;
    }
    const confPath = path.join(cfg.projectRoot, 'src-tauri', 'tauri.conf.json');
    const confText = fs.readFileSync(confPath, 'utf8');
    const m = confText.match(TAURI_VERSION_RE);
    if (!m) throw new Error(`Версия не найдена в tauri.conf.json (${confPath})`);
    return `${m[1]}.${m[2]}.${m[3]}`;
}

function readLatestVersion(projectRoot) {
    const latestPath = path.join(projectRoot, 'latest.json');
    if (!fs.existsSync(latestPath)) return null;
    try {
        const latest = JSON.parse(fs.readFileSync(latestPath, 'utf8'));
        const m = String(latest.version).match(/^(\d+)\.(\d+)\.(\d+)$/);
        if (!m) return null;
        return { maj: parseInt(m[1], 10), min: parseInt(m[2], 10), pat: parseInt(m[3], 10) };
    } catch (e) {
        return null; // latest.json повреждён — игнорируем (как в исходном build.cjs)
    }
}

// --- запись ---
function writeVersionToConf(cfg, version, syncPackageJson) {
    const confPath = path.join(cfg.projectRoot, 'src-tauri', 'tauri.conf.json');
    const cargoPath = path.join(cfg.crateDir, 'Cargo.toml');

    let confText = fs.readFileSync(confPath, 'utf8');
    confText = confText.replace(/"version"\s*:\s*".*?"/, `"version": "${version}"`);
    fs.writeFileSync(confPath, confText, 'utf8');

    let cargoText = fs.readFileSync(cargoPath, 'utf8');
    cargoText = cargoText.replace(/^version\s*=\s*".*?"/m, `version = "${version}"`);
    fs.writeFileSync(cargoPath, cargoText, 'utf8');

    if (syncPackageJson) {
        const pkgPath = path.join(cfg.projectRoot, 'package.json');
        if (fs.existsSync(pkgPath)) {
            const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
            if (pkg.version && pkg.version !== version) {
                pkg.version = version;
                fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n', 'utf8');
                console.log(`  package.json: ${version}`);
            }
        }
    }
}

// В воркспейсе `version = "..."` встречается и у зависимостей — меняем только
// первый match ПОСЛЕ заголовка [package] (это и есть версия самого пакета).
function writeVersionToCargo(cfg, version) {
    const cargoPath = path.join(crateDirOf(cfg), 'Cargo.toml');
    const text = fs.readFileSync(cargoPath, 'utf8');
    const pkgIdx = text.indexOf('[package]');
    if (pkgIdx === -1) throw new Error(`Секция [package] не найдена в ${cargoPath}`);
    const head = text.slice(0, pkgIdx);
    let tail = text.slice(pkgIdx);
    if (!CARGO_VERSION_RE.test(tail)) {
        throw new Error(`version не найден в [package] (${cargoPath})`);
    }
    tail = tail.replace(CARGO_VERSION_RE, `version = "${version}"`);
    fs.writeFileSync(cargoPath, head + tail, 'utf8');
}

// Инкремент патча по YY.M.P. При guardAgainstLatest (дефолт) берём строго
// бОльшую версию, чем в latest.json — защита от отката ниже уже выпущенного
// релиза (инцидент 26.8.173 < 26.8.174).
function bumpVersion(cfg, opts = {}) {
    const syncPackageJson = opts.syncPackageJson === true;
    const guardAgainstLatest = opts.guardAgainstLatest !== false;

    const old = readVersion(cfg).split('.').map((n) => parseInt(n, 10));
    const [oldMaj, oldMin, oldPat] = old;

    const now = new Date();
    const curMaj = parseInt(now.getFullYear().toString().slice(-2), 10);
    const curMin = now.getMonth() + 1;

    let newMaj = curMaj;
    let newMin = curMin;
    let newPat = (oldMaj === curMaj && oldMin === curMin) ? oldPat + 1 : 1;

    if (guardAgainstLatest) {
        const rel = readLatestVersion(cfg.projectRoot);
        if (rel) {
            const below =
                newMaj < rel.maj ||
                (newMaj === rel.maj && newMin < rel.min) ||
                (newMaj === rel.maj && newMin === rel.min && newPat <= rel.pat);
            if (below) {
                newMaj = rel.maj;
                newMin = rel.min;
                newPat = rel.pat + 1;
            }
        }
    }

    const version = `${newMaj}.${newMin}.${newPat}`;
    if (cfg.kind === 'cargo') {
        writeVersionToCargo(cfg, version);
        if (syncPackageJson) {
            const pkgPath = path.join(cfg.projectRoot, 'package.json');
            if (fs.existsSync(pkgPath)) {
                const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
                if (pkg.version && pkg.version !== version) {
                    pkg.version = version;
                    fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n', 'utf8');
                    console.log(`  package.json: ${version}`);
                }
            }
        }
    } else {
        writeVersionToConf(cfg, version, syncPackageJson);
    }
    return version;
}

module.exports = { readVersion, bumpVersion, writeVersionToConf, writeVersionToCargo };
