// Копирование VC++ runtime рядом с exe (config.vcRedist).
// Заменяет scripts/copy-vc-redist.cjs из эталона.
//
// Источники DLL: System32 (как эталон) + Microsoft.VC*.CRT из установленного VS.
// Назначение (эталон King Orch — проверщик ensure_vc_redist ищет DLL по
// `current_exe().parent()/redist` первым кандидатом):
//   - целевая папка (target/release);
//   - target/release/<vcRedistDir> (по умолчанию "redist");
//   - src-tauri/redist (эталонный источник).

const fs = require('fs');
const path = require('path');

const DEFAULT_VCREDIST_DIR = 'redist';

function findVcCrtDirs() {
    const dirs = [];
    if (process.env.SystemRoot) {
        dirs.push(path.join(process.env.SystemRoot, 'System32'));
    }
    const roots = [
        process.env['ProgramFiles(x86)'] && path.join(process.env['ProgramFiles(x86)'], 'Microsoft Visual Studio'),
        process.env.ProgramFiles && path.join(process.env.ProgramFiles, 'Microsoft Visual Studio'),
    ].filter(Boolean);

    for (const root of roots) {
        if (!fs.existsSync(root)) continue;
        let editions;
        try { editions = fs.readdirSync(root); } catch (e) { continue; }
        for (const edition of editions) {
            const redistRoot = path.join(root, edition, 'VC', 'Redist', 'MSVC');
            if (!fs.existsSync(redistRoot)) continue;
            let versions;
            try { versions = fs.readdirSync(redistRoot); } catch (e) { continue; }
            for (const ver of versions) {
                const x64 = path.join(redistRoot, ver, 'x64');
                if (!fs.existsSync(x64)) continue;
                let entries;
                try { entries = fs.readdirSync(x64); } catch (e) { continue; }
                for (const e of entries) {
                    const full = path.join(x64, e);
                    if (fs.statSync(full).isDirectory() && /^Microsoft\.VC\d+\.CRT$/i.test(e)) {
                        dirs.push(full);
                    }
                }
            }
        }
    }
    return dirs;
}

function copyVcRedistTo(cfg, destDir) {
    if (!cfg.vcRedist) {
        console.log('vcRedist: false, skip VC runtime copy.');
        return;
    }
    if (!destDir) {
        console.warn('vcRedist: destDir is unknown, skip VC runtime copy.');
        return;
    }

    const sourceDirs = findVcCrtDirs();
    if (!sourceDirs.length) {
        console.warn('vcRedist: no VC runtime sources found (System32 / VS Redist).');
        return;
    }

    const dests = [destDir];
    const sub = cfg.vcRedistDir || DEFAULT_VCREDIST_DIR;
    if (sub) dests.push(path.join(destDir, sub));
    dests.push(path.join(cfg.projectRoot, 'src-tauri', 'redist'));

    for (const d of dests) fs.mkdirSync(d, { recursive: true });

    // Копия рядом с exe может быть залочена живым процессом (exe берёт DLL сначала
    // из своей папки). С общим target-dir (cargo target-dir на все проекты) в одной
    // папке живут exe разных проектов — перезапись идентичной DLL падала EBUSY и
    // валила сборку ПОСЛЕ успешного линка (инцидент 16.11.2026, king_orch).
    // Решение: идентичную копию не трогаем вовсе; залоченную отличающуюся — честный
    // warn вместо падения сборки (старая копия продолжает работать).
    let copied = 0;
    let identical = 0;
    let locked = 0;
    for (const dll of cfg.vcRedistDlls) {
        const src = sourceDirs.map((s) => path.join(s, dll)).find((p) => fs.existsSync(p));
        if (!src) {
            console.warn(`vcRedist: DLL not found: ${dll}`);
            continue;
        }
        const srcBuf = fs.readFileSync(src);
        for (const d of dests) {
            const dstPath = path.join(d, dll);
            try {
                if (fs.existsSync(dstPath) && fs.readFileSync(dstPath).equals(srcBuf)) {
                    identical++;
                    continue;
                }
            } catch (e) {
                // не смогли прочитать текущую копию — пробуем скопировать как раньше
            }
            try {
                fs.copyFileSync(src, dstPath);
                copied++;
            } catch (e) {
                locked++;
                console.warn(`vcRedist: ${dll} -> ${dstPath} skipped (${e.code || e.message}): файл залочен запущенным процессом, оставлена существующая копия`);
            }
        }
    }
    console.log(`VC redist: copied ${copied}, identical ${identical}, locked-skip ${locked} (${cfg.vcRedistDlls.length} dlls) -> ${dests.join(', ')}`);
}

// Копирование рядом с собранным exe (для dev-сборки после билда).
function copyVcRedistIfConfigured(cfg, exePath) {
    if (!exePath) {
        console.warn('vcRedist: exePath is unknown, skip VC runtime copy.');
        return;
    }
    copyVcRedistTo(cfg, path.dirname(exePath));
}

module.exports = { copyVcRedistIfConfigured, copyVcRedistTo, findVcCrtDirs };