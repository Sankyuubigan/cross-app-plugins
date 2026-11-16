// Подпись установщиков (правила §3.1 глобальной доки).
// Ключи — ОБЩИЕ для всех проектов, лежат в глобальном каталоге документации.
// Никаких хардкодов паролей (исходник хранил '123' прямо в .cjs).

const fs = require('fs');
const path = require('path');

// Приоритет пути к ключу:
//   1) env TAURI_PRIVATE_KEY_ORIGINAL
//   2) config.signing.keyPath
//   3) env TAURI_KEYS_DIR (если задан) -> <dir>/tauri.key
//   4) глобальный каталог ключей из документации
function resolveKeyPath(cfg) {
    if (process.env.TAURI_PRIVATE_KEY_ORIGINAL) return process.env.TAURI_PRIVATE_KEY_ORIGINAL;
    if (cfg.signing && cfg.signing.keyPath) return cfg.signing.keyPath;
    if (process.env.TAURI_KEYS_DIR) return path.join(process.env.TAURI_KEYS_DIR, 'tauri.key');
    return path.join(cfg.defaultKeysDir || '', 'tauri.key');
}

// Приоритет пароля:
//   1) env TAURI_SIGNING_PRIVATE_KEY_PASSWORD
//   2) config.signing.password
//   3) config.signing.passwordFile
//   4) TAURI_KEY_PASSWORD.txt рядом с ключом (эталон Global docs §3.1)
//   5) env TAURI_KEY_PASSWORD
function resolvePassword(cfg, keyPath) {
    if (process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD) {
        return process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD;
    }
    if (cfg.signing && cfg.signing.password) return cfg.signing.password;

    const candidates = [];
    if (cfg.signing && cfg.signing.passwordFile) candidates.push(cfg.signing.passwordFile);
    if (process.env.TAURI_KEYS_DIR) candidates.push(path.join(process.env.TAURI_KEYS_DIR, 'TAURI_KEY_PASSWORD.txt'));
    if (keyPath) candidates.push(path.join(path.dirname(keyPath), 'TAURI_KEY_PASSWORD.txt'));

    for (const f of candidates) {
        if (f && fs.existsSync(f)) {
            return fs.readFileSync(f, 'utf8').trim();
        }
    }
    if (process.env.TAURI_KEY_PASSWORD) return process.env.TAURI_KEY_PASSWORD;
    return null;
}

// Настраивает окружение подписи Tauri.
// strict=true  — ключ ОБЯЗАН существовать (release); иначе бросаем ошибку.
// strict=false — нет ключа -> предупреждаем и продолжаем без подписи (installer).
// Возвращает { keyPath, keyContent }.
function setupSigningEnv(cfg, opts = {}) {
    const strict = opts.strict !== false;
    const keyPath = resolveKeyPath(cfg);

    if (!fs.existsSync(keyPath)) {
        if (!strict) {
            console.warn('WARNING: Signing key not found. Building without signature.');
            process.env.TAURI_SIGNING_PRIVATE_KEY = '';
            return { keyPath: null, keyContent: null };
        }
        throw new Error(
            `Private key NOT FOUND:\n${keyPath}\n` +
            `Путь можно переопределить через env TAURI_PRIVATE_KEY_ORIGINAL, ` +
            `config.signing.keyPath или env TAURI_KEYS_DIR (см. глобальную доку §3.1).`
        );
    }

    const keyContent = fs.readFileSync(keyPath, 'utf8').trim();
    const singleLineKey = keyContent.replace(/\r?\n|\r/g, '');
    process.env.TAURI_SIGNING_PRIVATE_KEY = singleLineKey;

    const password = resolvePassword(cfg, keyPath);
    if (password) {
        // Ставим и новое, и старое имя env: CLI >= 2.2 читает
        // TAURI_SIGNING_PRIVATE_KEY_PASSWORD, старые версии — TAURI_PRIVATE_KEY_PASSWORD.
        process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD = password;
        process.env.TAURI_PRIVATE_KEY_PASSWORD = password;
    } else {
        delete process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD;
        delete process.env.TAURI_PRIVATE_KEY_PASSWORD;
    }

    delete process.env.TAURI_SIGNING_PRIVATE_KEY_PATH;
    delete process.env.TAURI_PRIVATE_KEY;
    delete process.env.TAURI_KEY_PASSWORD;

    console.log('Signing key loaded.');
    return { keyPath, keyContent, singleLineKey, password };
}

// Подписать установщик (Tauri signer). Если .sig уже существует — не переписываем.
// strict=false — ключа нет: предупреждаем и продолжаем без подписи (installer).
// strict=true  — ключа нет: ошибка (release). Возвращает путь к .sig либо null.
function signInstaller(cfg, installerPath, opts = {}) {
    const strict = opts.strict !== false;
    const sig = `${installerPath}.sig`;
    if (fs.existsSync(sig)) {
        console.log('Signature already present:', sig);
        return sig;
    }

    const { keyContent, singleLineKey, password } = setupSigningEnv(cfg, { strict });
    if (!keyContent) {
        return null;
    }

    // Ключ передаём СОДЕРЖИМЫМ, одной строкой (тот же формат, что в env для
    // bundler'а; e2e-проверка парсинга на CLI 2.9.0 и 2.12.4 — 2026-11-14).
    // Temp-копия ключа в %TEMP% больше не создаётся.
    // env-дубль стираем: CLI >= 2.12 мапит TAURI_SIGNING_PRIVATE_KEY на аргумент
    // --private-key и при стороннем источнике этого аргумента падает
    // clap-конфликтом (регрессия релиза VibeBridge 26.11.1); CLI 2.9.0 env для
    // signer sign игнорирует — стрип для него безвреден.
    // ВАЖНО: run() подменяет env дочернего процесса ЦЕЛИКОМ (не мержит),
    // поэтому передаём полный клон process.env.
    const env = { ...process.env };
    delete env.TAURI_SIGNING_PRIVATE_KEY;

    const args = ['tauri', 'signer', 'sign', '--private-key', singleLineKey];
    if (password) {
        args.push('--password', password);
    }
    args.push(installerPath);

    // Echo вручную, а не run({ echo: true }): в командной строке ключ и пароль —
    // в лог им нельзя.
    console.log('$ npx tauri signer sign --private-key *** --password ***', installerPath);
    const { run } = require('./run.cjs');
    run(cfg, 'npx', args, { env });

    if (!fs.existsSync(sig)) {
        throw new Error(`Signer finished but signature not produced: ${sig}`);
    }
    console.log('Signed:', sig);
    return sig;
}

module.exports = { setupSigningEnv, resolveKeyPath, resolvePassword, signInstaller };