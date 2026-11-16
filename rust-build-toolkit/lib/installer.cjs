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
const { readVersion } = require('./version.cjs');
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

async function run(cfg) {
    if (cfg.kind === 'cargo') return runCargoInstaller(cfg);

    await prep(cfg);

    console.log('\n=== installer ===');
    cleanupOldBundle(cfg);

    killApp(cfg); // РѕСЃРІРѕР±РѕРґРёС‚СЊ file lock exe РґРѕ РїРµСЂРµСЃР±РѕСЂРєРё
    setupSigningEnv(cfg, { strict: false });
    ensureBundleActive(cfg);

    // VC++ runtime DLL РєРѕРїРёСЂСѓСЋС‚СЃСЏ Р”Рћ СЃР±РѕСЂРєРё, С‡С‚РѕР±С‹ РїРѕРїР°СЃС‚СЊ РІ NSIS-Р±Р°РЅРґР».
    copyVcRedistTo(cfg, path.join(resolveTargetDir(cfg), 'release'));

    sh(cfg, 'npx', ['tauri', 'build', '--bundles', 'nsis'], { echo: true });

    const version = readVersion(cfg);
    const installer = findInstallerBundle(BUNDLE_DIR(cfg), version);
    if (!installer) {
        throw new Error(`Installer not found in ${BUNDLE_DIR(cfg)} for version ${version}`);
    }
    console.log('Installer:', installer);

    signInstaller(cfg, installer, { strict: false });

    console.log('\n[installer] done.');
    return installer;
}

module.exports = { run };
