#!/usr/bin/env node
// tauri-build-toolkit CLI.
// Subcommands:
//   build      - dev build (version bump + npm + icons + tauri build + launch app)
//   prep       - prep-only (version bump + npm + icons) — шаг 1 generate_installer.bat
//   installer  - full NSIS installer build + verification
//   release    - full release (build + sign + gh release + latest.json + commit/push)
//   test       - cargo test (filter)
//   version    - bump version only (YY.M.P) and print
//   doctor     - print resolved project settings + git safety checks (read-only)
//   init       - scaffold .bat templates + .build-config.json into a project
//                (--with-logs — вложить в проект гайд подключения plugin-log-collector)

const { loadProjectConfig } = require('./lib/config.cjs');
const { isolateBuildTemp, SPAWNING_COMMANDS } = require('./lib/scratch.cjs');

const COMMANDS = ['build', 'prep', 'installer', 'release', 'test', 'version', 'doctor', 'init'];

function parseArgs(argv) {
    const args = { command: null, project: null, extras: [], flags: {} };
    const tokens = argv.slice(2);
    let passthrough = false; // после `--` все токены — позиционные (cargo test -- --ignored)
    for (let i = 0; i < tokens.length; i++) {
        const t = tokens[i];
        if (!passthrough && t === '--') {
            passthrough = true;
            args.extras.push('--');
        } else if (passthrough) {
            args.extras.push(t);
        } else if (t === '--project') {
            args.project = tokens[++i];
        } else if (t === '--prep-only') {
            args.flags.prepOnly = true;
        } else if (t === '--release' || t === '--workspace') {
            // Флаги cargo, а не флаги тулкита. `--release` сейчас дефолт профиля
            // тестов (lib/test.cjs добавляет его сам) — явный флаг tolerated
            // (дубль не создаётся); `--workspace` — cargo-воркспейс, где без
            // флага cargo выбирает только пакет из cwd.
            args.extras.push(t);
        } else if (t.startsWith('--')) {
            args.flags[t.slice(2)] = true;
        } else if (!args.command && COMMANDS.includes(t)) {
            args.command = t;
        } else {
            args.extras.push(t);
        }
    }
    return args;
}

async function main() {
    const argv = parseArgs(process.argv);
    const command = argv.command;

    if (!command) {
        console.log('Usage: node cli.cjs <build|prep|installer|release|test|version|doctor|init> [--project <dir>] [args]');
        console.log('  --project <dir> — корень Tauri-проекта (иначе: текущая рабочая папка).');
        process.exit(1);
    }

    const projectArg = argv.project || process.cwd();
    const cfg = loadProjectConfig(argv.project, process.cwd());

    // Единственная точка подстановки временной папки сборки. Здесь, а не в
    // каждом .bat и не в каждом модуле: process.env наследуется ВСЕМИ
    // дочерними процессами, поэтому подстановка не расходится между
    // build/test/installer/release и не забывается в новой команде.
    // Подробности и обоснование — lib/scratch.cjs.
    if (SPAWNING_COMMANDS.has(command)) {
        try {
            const tmp = isolateBuildTemp(cfg);
            console.log(`[build-temp] ${tmp}`);
        } catch (e) {
            console.error('\n[ERROR]', e.message);
            process.exit(1);
        }
    }

    switch (command) {
        case 'build':
            return require('./lib/build.cjs').run(cfg, { prepOnly: !!argv.flags.prepOnly });
        case 'prep':
            return require('./lib/build.cjs').run(cfg, { prepOnly: true });
        case 'installer':
            return require('./lib/installer.cjs').run(cfg);
        case 'release':
            return require('./lib/release.cjs').run(cfg);
        case 'test':
            return require('./lib/test.cjs').run(cfg, argv.extras);
        case 'version': {
            const { bumpVersion } = require('./lib/version.cjs');
            const v = bumpVersion(cfg, { syncPackageJson: cfg.syncPackageJsonVersion });
            console.log(`Version: ${v}`);
            return;
        }
        case 'doctor':
            return require('./lib/doctor.cjs').run(cfg);
        case 'init':
            return require('./lib/init.cjs').run(cfg, argv.flags);
        default:
            console.error(`[ERROR] Unknown command: ${command}`);
            process.exit(1);
    }
}

main().catch((e) => {
    console.error('\n[ERROR]', e.message);
    process.exit(1);
});