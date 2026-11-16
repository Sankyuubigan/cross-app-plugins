// test.bat-аналог: cargo test в src-tauri.
// Дефолт профиля — release (решение владельца 16.11.2026):
// 1) debug-харнесс на части машин не стартует (0xc0000139, desktop §6.2);
// 2) target/debug раздувается до десятков ГБ на проект (кейс king_orch: 127 ГБ),
//    build/release от debug-артефактов не зависят.
// Явный --profile отменяет дефолт (cargo запрещает --release + --profile).
// Явный --release дубль не создаёт. Вставка --release строго ДО `--`.

const path = require('path');
const { run: sh } = require('./run.cjs');
const { checkProjectEncodings } = require('./encoding-check.cjs');

function run(cfg, extras) {
    checkProjectEncodings(cfg);
    const args = ['test', ...(extras || [])];
    const hasProfile = args.some((a) => a === '--profile' || a.startsWith('--profile='));
    const hasRelease = args.includes('--release');
    if (!hasProfile && !hasRelease) {
        const dd = args.indexOf('--');
        if (dd === -1) args.push('--release');
        else args.splice(dd, 0, '--release');
    }
    sh(cfg, 'cargo', args, { echo: true, cwd: path.join(cfg.projectRoot, 'src-tauri') });
    console.log('\n[test] done.');
}

module.exports = { run };
