//! Установка движка trellis.cpp: server-бандл (GitHub releases) + веса GGUF
//! (HuggingFace). Скачивание — SSOT `downloader-core`: прогресс `downloader:progress`
//! идёт через emit-хук фасада plugin-downloader, свой emit-механизм запрещён
//! (§0.7 — дублирование доставки). Веса качаются ПОСИМЕННО в `models/<quant>/` —
//! скачанное остаётся (downloader пропускает валидное существующее), прерванная
//! установка продолжается повторным «Установить». Server-бандл распаковывается
//! в staging и переносится в корень движка: ошибка установки не ломает старую
//! версию (прецедент `sdcpp_installer::install`).
//!
//! Источники проверены 17.11.2026: install/install.ps1 ветки main, GitHub API
//! releases (v0.8.1), HF API tree (ilintar/trellis2-gguf, vegax87/Pixal3D).

use crate::engine::presets::{weights_dir, MeshQuant};
use downloader_core::api::{download, DownloadOptions};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// kind событий `downloader:progress` (по нему фильтрует UI-панель).
pub const PROGRESS_KIND: &str = "mesh-engine";

/// Имя авто-детекта бекенда.
pub const BACKEND_AUTO: &str = "auto";
/// Допустимые бекенды server-бандла (имена ассетов GitHub releases).
pub const BACKENDS: [&str; 5] = ["auto", "vulkan", "cuda", "cuda12", "rocm"];

const RELEASE_LATEST: &str = "https://github.com/pwilkin/trellis.cpp/releases/latest/download";
const HF_TRELLIS2: &str = "https://huggingface.co/ilintar/trellis2-gguf/resolve/main";
const HF_PIXAL3D: &str = "https://huggingface.co/vegax87/Pixal3D/resolve/main";
const SERVER_EXE: &str = "trellis-server.exe";
const STAGING_DIR: &str = ".staging-install";

/// Мета установленного server-бандла в корне движка (тег релиза — источник
/// проверки обновлений; GGUF-веса версий не имеют и в мету не входят).
pub const META_FILE: &str = ".mesh-engine-meta.json";

/// GitHub API последнего релиза — источник тега для проверки обновлений.
const RELEASE_API: &str = "https://api.github.com/repos/pwilkin/trellis.cpp/releases/latest";

/// Веса TRELLIS.2 — 10 файлов (состав install.ps1).
pub const TRELLIS2_FILES: [&str; 10] = [
    "birefnet.gguf",
    "dinov3.gguf",
    "ss_flow.gguf",
    "ss_dec.gguf",
    "shape_flow_512.gguf",
    "shape_flow_1024.gguf",
    "shape_dec.gguf",
    "tex_flow_512.gguf",
    "tex_flow_1024.gguf",
    "tex_dec.gguf",
];

/// Веса Pixal3D — 5 файлов в корне репо, ТОЛЬКО f16 (кванты не публиковались;
/// декодеры/dinov3/birefnet общие с TRELLIS.2 — докачиваются при отсутствии).
pub const PIXAL3D_FILES: [&str; 5] = [
    "pixal3d_naf.gguf",
    "pixal3d_shape_flow_512.gguf",
    "pixal3d_shape_flow_1024.gguf",
    "pixal3d_ss_flow.gguf",
    "pixal3d_tex_flow_1024.gguf",
];

/// Имя zip-бандла сервера для бекенда.
pub fn server_zip_name(backend: &str) -> String {
    format!("trellis-{backend}-windows-x64.zip")
}

/// URL server-бандла (релизный канал latest).
pub fn server_zip_url(backend: &str) -> String {
    format!("{RELEASE_LATEST}/{}", server_zip_name(backend))
}

/// URL веса trellis2: f16 лежит в корне репо, q4/q8 — в одноимённых подпапках.
pub fn trellis2_url(quant_dir: &str, file: &str) -> String {
    if quant_dir == "f16" {
        format!("{HF_TRELLIS2}/{file}")
    } else {
        format!("{HF_TRELLIS2}/{quant_dir}/{file}")
    }
}

/// URL веса Pixal3D (всегда корень репо).
pub fn pixal3d_url(file: &str) -> String {
    format!("{HF_PIXAL3D}/{file}")
}

/// Чистая функция маппинга compute capability → бекенд (логика install.ps1:
/// NVIDIA cc < 7.5 → cuda12 legacy-рантайм, cc ≥ 7.5 → cuda; NVIDIA нет → vulkan).
pub fn backend_from_compute_cap(cap: Option<f32>) -> &'static str {
    match cap {
        Some(cc) if cc >= 7.5 => "cuda",
        Some(_) => "cuda12",
        None => "vulkan",
    }
}

fn nvidia_compute_cap() -> Option<f32> {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args(["--query-gpu=compute_cap", "--format=csv,noheader"]);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW — консоль не мелькает
    let out = cmd.output().ok()?;
    let first = String::from_utf8_lossy(&out.stdout).lines().next()?.trim().to_string();
    first.parse::<f32>().ok()
}

/// Резолв бекенда: явный выбор пользователя либо авто-детект.
pub fn resolve_backend(pref: Option<&str>) -> String {
    match pref {
        Some(p) if !p.is_empty() && p != BACKEND_AUTO && BACKENDS.contains(&p) => p.to_string(),
        _ => backend_from_compute_cap(nvidia_compute_cap()).to_string(),
    }
}

/// Мета установленного server-бандла — источник проверки обновлений.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EngineMeta {
    /// Тег релиза GitHub, из которого установлен server-бандл.
    pub tag: String,
    /// Бекенд, с которым ставился бандл (справка/диагностика).
    pub backend: String,
    /// Момент записи меты (unix-секунды, строкой).
    pub installed_at: String,
}

fn installed_meta_path(dir: &Path) -> PathBuf {
    dir.join(META_FILE)
}

/// Читает мету установленного движка; нет файла или битый JSON → None.
pub fn read_installed_meta(dir: &Path) -> Option<EngineMeta> {
    let text = std::fs::read_to_string(installed_meta_path(dir)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Пишет мету после успешной установки server-бандла.
fn write_installed_meta(dir: &Path, tag: &str, backend: &str) -> Result<(), String> {
    let meta = EngineMeta {
        tag: tag.to_string(),
        backend: backend.to_string(),
        installed_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default(),
    };
    let text = serde_json::to_string_pretty(&meta)
        .map_err(|e| format!("Ошибка сериализации меты: {e}"))?;
    std::fs::write(installed_meta_path(dir), text)
        .map_err(|e| format!("Ошибка записи меты {}: {e}", installed_meta_path(dir).display()))
}

/// Последний релизный тег trellis.cpp (GitHub API). Сеть недоступна → Err —
/// «не смогли проверить» честно отличается от «актуален» (core_rules §2.2).
pub async fn latest_release_tag() -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent("king-orch-mesh-engine")
        .build()
        .map_err(|e| format!("HTTP-клиент: {e}"))?;
    let resp = client
        .get(RELEASE_API)
        .send()
        .await
        .map_err(|e| format!("GitHub API недоступен: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GitHub API: HTTP {}", resp.status()));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Ответ GitHub API не JSON: {e}"))?;
    json.get("tag_name")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| "GitHub API: в ответе нет tag_name".to_string())
}

/// Чистое решение проверки обновлений (без сети — покрывается юнит-тестами).
/// Правила честности: exe нет → Ok(None) (проверять нечего, кнопка скрыта);
/// exe есть, меты нет → Err («версия неизвестна», НЕ «актуален» — лечится
/// повторной установкой, которая запишет мету); тег меты ≠ latest → Some(tag).
pub fn update_decision(
    exe_exists: bool,
    meta: Option<&EngineMeta>,
    latest: &str,
) -> Result<Option<String>, String> {
    if !exe_exists {
        return Ok(None);
    }
    match meta {
        None => Err("Установленная версия неизвестна (движок поставлен до записи меты) \
                     — нажмите «Докачать / обновить», после установки версия запишется."
            .to_string()),
        Some(m) if m.tag == latest => Ok(None),
        Some(_) => Ok(Some(latest.to_string())),
    }
}

/// Проверка обновления server-бандла: тег из меты vs последний релиз GitHub.
/// `Some(tag)` — доступно обновление, `Ok(None)` — актуален.
pub async fn check_update(
    dir: &Path,
    on_log: &(dyn Fn(String) + Send + Sync),
) -> Result<Option<String>, String> {
    let exe_exists = dir.join(SERVER_EXE).is_file();
    if !exe_exists {
        on_log("Движок не установлен — проверять нечего".to_string());
        return Ok(None);
    }
    let meta = read_installed_meta(dir);
    let latest = latest_release_tag().await?;
    let decision = update_decision(exe_exists, meta.as_ref(), &latest)?;
    match &decision {
        Some(tag) => on_log(format!(
            "🔄 Доступно обновление движка: {} → {tag}",
            meta.as_ref().map(|m| m.tag.as_str()).unwrap_or("?")
        )),
        None => on_log(format!("✅ Движок trellis.cpp актуален ({latest})")),
    }
    Ok(decision)
}

/// Полная установка: server-бандл + веса trellis2 + веса Pixal3D (ОБЕ модели
/// всегда, решение владельца 17.11.26 — чекбокса нет; docs/pixal3d/README.md:
/// одна папка `models/<quant>/` обслуживает оба семейства, выбор по `--model`).
/// Прогресс — события `downloader:progress` (kind mesh-engine).
pub async fn install(
    dir: &Path,
    backend_pref: Option<&str>,
    quant: MeshQuant,
    on_log: &(dyn Fn(String) + Send + Sync),
) -> Result<(), String> {
    let backend = resolve_backend(backend_pref);
    on_log(format!("Бекенд: {backend} (trellis.cpp)"));
    // Тег релиза резолвим ДО скачивания: после успешного бандла он ляжет в мету —
    // источник проверки обновлений. API легло (rate-limit/сеть) — установка всё
    // равно честно идёт, но мета не пишется (проверка скажет «версия неизвестна»).
    let latest_tag = match latest_release_tag().await {
        Ok(tag) => {
            on_log(format!("Релиз: {tag}"));
            Some(tag)
        }
        Err(e) => {
            on_log(format!(
                "⚠️ Не удалось определить последний релиз ({e}) — мета версии не будет записана"
            ));
            None
        }
    };
    let weights = weights_dir(dir, quant);
    std::fs::create_dir_all(&weights)
        .map_err(|e| format!("Не удалось создать папку весов {}: {e}", weights.display()))?;
    install_server_bundle(dir, &backend, on_log).await?;
    // Мета — сразу после успешного бандла: веса можно докачивать позже повторным
    // «Установить», движок уже именно той версии.
    if let Some(tag) = &latest_tag {
        write_installed_meta(&dir, tag, &backend)?;
    }
    install_trellis2_weights(&weights, quant, on_log).await?;
    install_pixal3d_weights(&weights, on_log).await?;
    on_log(format!("📂 Установка завершена: {}", dir.display()));
    Ok(())
}

/// Server-бандл: staging → zip → распаковка → перенос в корень движка.
async fn install_server_bundle(
    dir: &Path,
    backend: &str,
    on_log: &(dyn Fn(String) + Send + Sync),
) -> Result<(), String> {
    let staging = dir.join(STAGING_DIR);
    let zip_path = staging.join(server_zip_name(backend));
    let extract_dir = staging.join("unpacked");
    let mut opts = DownloadOptions::default();
    opts.label = format!("Движок trellis.cpp ({backend})");
    opts.kind = PROGRESS_KIND.to_string();
    opts.min_size = Some(1_000_000); // у vulkan-бандла ~14 МБ — меньше быть не может
    download(&server_zip_url(backend), &zip_path, opts, Some(on_log)).await?;
    on_log("📦 Распаковка server-бандла…".to_string());
    extract_all(&zip_path, &extract_dir, on_log)?;
    let exe_src = find_file(&extract_dir, SERVER_EXE)
        .ok_or_else(|| format!("В бандле не найден {SERVER_EXE} — установка прервана (старая версия не тронута)"))?;
    let source_root = exe_src.parent().unwrap_or(&extract_dir).to_path_buf();
    copy_tree_overwrite(&source_root, dir)?;
    if !dir.join(SERVER_EXE).is_file() {
        return Err(format!("{SERVER_EXE} не появился в {} — установка прервана", dir.display()));
    }
    clear_dir(&staging);
    on_log(format!("✅ Движок: {}", dir.join(SERVER_EXE).display()));
    Ok(())
}

/// Веса TRELLIS.2. Общие декодеры (birefnet/dinov3/…) ставятся всегда —
/// пресет Pixal3D их переиспользует; после них докачиваются 5 pixal3d_*
/// (f16 — других квантов upstream нет).
async fn install_trellis2_weights(
    weights: &Path,
    quant: MeshQuant,
    on_log: &(dyn Fn(String) + Send + Sync),
) -> Result<(), String> {
    let quant_dir = quant.dir_name();
    on_log(format!("Веса TRELLIS.2 ({quant_dir}) — {} файлов", TRELLIS2_FILES.len()));
    for file in TRELLIS2_FILES {
        download_gguf(&trellis2_url(&quant_dir, file), &weights.join(file), on_log).await?;
    }
    Ok(())
}

async fn install_pixal3d_weights(
    weights: &Path,
    on_log: &(dyn Fn(String) + Send + Sync),
) -> Result<(), String> {
    on_log(format!("Веса Pixal3D (f16, {} файлов)", PIXAL3D_FILES.len()));
    for file in PIXAL3D_FILES {
        download_gguf(&pixal3d_url(file), &weights.join(file), on_log).await?;
    }
    Ok(())
}

async fn download_gguf(url: &str, dest: &Path, on_log: &(dyn Fn(String) + Send + Sync)) -> Result<(), String> {
    let mut opts = DownloadOptions::gguf();
    opts.label = format!("Веса: {}", dest.file_name().unwrap_or_default().to_string_lossy());
    opts.kind = PROGRESS_KIND.to_string();
    download(url, dest, opts, Some(on_log)).await
}

fn extract_all(zip_path: &Path, dest_dir: &Path, on_log: &(dyn Fn(String) + Send + Sync)) -> Result<u32, String> {
    let data = std::fs::read(zip_path).map_err(|e| format!("Ошибка чтения zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
        .map_err(|e| format!("Ошибка чтения zip: {e}"))?;
    let mut count = 0u32;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("Ошибка zip-записи: {e}"))?;
        let name = entry.name().replace('\\', "/");
        let clean = name.trim_start_matches('/');
        if clean.is_empty() || clean.ends_with('/') {
            continue;
        }
        let out_path = dest_dir.join(clean);
        if !out_path.starts_with(dest_dir) {
            continue; // защита от path traversal
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Ошибка создания папки {}: {e}", parent.display()))?;
        }
        let mut out_file = std::fs::File::create(&out_path)
            .map_err(|e| format!("Ошибка создания {}: {e}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out_file)
            .map_err(|e| format!("Ошибка распаковки {}: {e}", clean))?;
        count += 1;
    }
    on_log(format!("Распаковано файлов: {count}"));
    Ok(count)
}

/// Рекурсивный поиск файла по имени (внутри extract_dir).
fn find_file(root: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(root).ok()? {
        let path = entry.ok()?.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name()?.to_string_lossy() == name {
            return Some(path);
        }
    }
    None
}

/// Рекурсивное копирование содержимого `src` в `dst` (файлы перезаписываются).
fn copy_tree_overwrite(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("Ошибка создания {}: {e}", dst.display()))?;
    for entry in std::fs::read_dir(src).map_err(|e| format!("Ошибка чтения {}: {e}", src.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let target = dst.join(path.file_name().expect("имя файла есть"));
        if path.is_dir() {
            copy_tree_overwrite(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)
                .map_err(|e| format!("Ошибка копирования {} → {}: {e}", path.display(), target.display()))?;
        }
    }
    Ok(())
}

fn clear_dir(dir: &Path) {
    if dir.exists() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

// ─────────────────────── Риггинг (skin-tokens.cpp, этап 2) ───────────────────────

/// kind событий `downloader:progress` для весов риггинга (панель фильтрует
/// по нему; отдельно от PROGRESS_KIND, чтобы прогресс не путал секции).
pub const RIG_PROGRESS_KIND: &str = "rig-engine";

/// HF-репозиторий весов риггинга (F16 — единственный инференс-квант).
const HF_SKINTOKENS: &str = "https://huggingface.co/LocalAI-io/SkinTokens-GGUF/resolve/main";

/// Файлы весов риггинга в HF-репозитории (подпапка F16).
pub const SKINTOKENS_FILES: [&str; 3] = ["mesh-encoder.gguf", "skin-vae.gguf", "tokenrig.gguf"];

/// URL веса риггинга: `<HF>/resolve/main/F16/<file>`.
pub fn skintokens_url(file: &str) -> String {
    format!("{HF_SKINTOKENS}/F16/{file}")
}

/// Статус раскладки риггинга для секции «Риггинг» плашки. Честность: exe и
/// рантайм-DLL НЕ скачиваются панелью (пребилдов upstream нет — exe кладётся
/// из локальной сборки), панель качает только 3 веса.
pub struct RigStatus {
    pub dir: String,
    pub exe_present: bool,
    pub dlls_present: bool,
    pub weights_present: usize,
    pub weights_dir: String,
}

impl RigStatus {
    /// Движок готов к ригу: exe + DLL + все 3 веса.
    pub fn ready(&self) -> bool {
        self.exe_present && self.dlls_present && self.weights_present == SKINTOKENS_FILES.len()
    }
}

pub fn rig_status(rig_dir: &Path) -> RigStatus {
    let weights_dir = crate::engine::rigging::rig_weights_dir(rig_dir);
    let weights_present = SKINTOKENS_FILES
        .iter()
        .filter(|f| weights_dir.join(f).is_file())
        .count();
    let exe_present = crate::engine::rigging::cli_path(rig_dir).is_file();
    let dlls_present = crate::engine::rigging::RUNTIME_DLLS
        .iter()
        .all(|d| rig_dir.join(d).is_file());
    RigStatus {
        dir: rig_dir.display().to_string(),
        exe_present,
        dlls_present,
        weights_present,
        weights_dir: weights_dir.display().to_string(),
    }
}

/// Скачать/докачать 3 веса риггинга F16 → `<rig_dir>/models/SkinTokens-GGUF/F16`.
/// Скачанное докачивается повторным вызовом (downloader-core skip-логика).
pub async fn install_rig_weights(
    rig_dir: &Path,
    on_log: &(dyn Fn(String) + Send + Sync),
) -> Result<RigStatus, String> {
    let weights_dir = crate::engine::rigging::rig_weights_dir(rig_dir);
    std::fs::create_dir_all(&weights_dir)
        .map_err(|e| format!("Ошибка создания {}: {e}", weights_dir.display()))?;
    on_log(format!(
        "Веса риггинга SkinTokens-GGUF F16 ({} файлов)",
        SKINTOKENS_FILES.len()
    ));
    for file in SKINTOKENS_FILES {
        let mut opts = DownloadOptions::gguf();
        opts.label = format!("Веса риггинга: {file}");
        opts.kind = RIG_PROGRESS_KIND.to_string();
        download(&skintokens_url(file), &weights_dir.join(file), opts, Some(on_log)).await?;
    }
    Ok(rig_status(rig_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_urls_match_release_assets() {
        assert_eq!(server_zip_name("vulkan"), "trellis-vulkan-windows-x64.zip");
        assert_eq!(server_zip_name("cuda"), "trellis-cuda-windows-x64.zip");
        assert_eq!(
            server_zip_url("rocm"),
            "https://github.com/pwilkin/trellis.cpp/releases/latest/download/trellis-rocm-windows-x64.zip"
        );
    }

    #[test]
    fn trellis2_urls_f16_root_quants_subdirs() {
        assert_eq!(
            trellis2_url("f16", "dinov3.gguf"),
            "https://huggingface.co/ilintar/trellis2-gguf/resolve/main/dinov3.gguf"
        );
        assert_eq!(
            trellis2_url("q8", "ss_flow.gguf"),
            "https://huggingface.co/ilintar/trellis2-gguf/resolve/main/q8/ss_flow.gguf"
        );
        assert_eq!(
            pixal3d_url("pixal3d_naf.gguf"),
            "https://huggingface.co/vegax87/Pixal3D/resolve/main/pixal3d_naf.gguf"
        );
    }

    #[test]
    fn file_lists_match_upstream() {
        // install.ps1 качает ровно 10 файлов trellis2; Pixal3D — 5 (HF API).
        assert_eq!(TRELLIS2_FILES.len(), 10);
        assert!(TRELLIS2_FILES.contains(&"shape_flow_1024.gguf"));
        assert_eq!(PIXAL3D_FILES.len(), 5);
        assert!(PIXAL3D_FILES.iter().all(|f| f.starts_with("pixal3d_")));
    }

    #[test]
    fn backend_detection_follows_install_ps1() {
        assert_eq!(backend_from_compute_cap(Some(8.6)), "cuda");
        assert_eq!(backend_from_compute_cap(Some(7.5)), "cuda");
        assert_eq!(backend_from_compute_cap(Some(6.1)), "cuda12");
        assert_eq!(backend_from_compute_cap(None), "vulkan");
    }

    #[test]
    fn resolve_backend_prefers_explicit_choice() {
        assert_eq!(resolve_backend(Some("rocm")), "rocm");
        // Авто-пути ведут к живому детекту nvidia-smi (машино-зависим), поэтому
        // детерминированно проверяем только их согласованность между собой —
        // сама таблица cc→бекенд покрыта тестом backend_detection_follows_install_ps1.
        let detected = resolve_backend(None);
        assert_eq!(resolve_backend(Some(BACKEND_AUTO)), detected);
        assert_eq!(resolve_backend(Some("неизвестный")), detected);
        assert_eq!(resolve_backend(Some("")), detected);
    }

    #[test]
    fn copy_tree_overwrite_copies_nested_files() {
        let tmp = std::env::temp_dir().join(format!("mesh_inst_test_{}", std::process::id()));
        let src = tmp.join("src/bin");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("server.exe"), b"new").unwrap();
        let dst = tmp.join("dst");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(dst.join("server.exe"), b"old").unwrap();
        copy_tree_overwrite(&tmp.join("src"), &dst).unwrap();
        assert_eq!(std::fs::read(dst.join("bin/server.exe")).unwrap(), b"new");
        clear_dir(&tmp);
    }

    #[test]
    fn meta_roundtrip_and_path() {
        let tmp = std::env::temp_dir().join(format!("mesh_meta_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(read_installed_meta(&tmp).is_none());
        write_installed_meta(&tmp, "v0.8.1", "cuda").unwrap();
        let meta = read_installed_meta(&tmp).unwrap();
        assert_eq!(meta.tag, "v0.8.1");
        assert_eq!(meta.backend, "cuda");
        assert!(!meta.installed_at.is_empty());
        assert!(tmp.join(META_FILE).is_file());
        clear_dir(&tmp);
    }

    #[test]
    fn meta_garbage_is_none() {
        let tmp = std::env::temp_dir().join(format!("mesh_meta_bad_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join(META_FILE), "не json").unwrap();
        assert!(read_installed_meta(&tmp).is_none());
        clear_dir(&tmp);
    }

    #[test]
    fn update_decision_honesty() {
        let meta = EngineMeta {
            tag: "v0.8.1".into(),
            backend: "cuda".into(),
            installed_at: "0".into(),
        };
        // exe нет — проверять нечего (кнопка проверки в UI скрыта).
        assert_eq!(update_decision(false, Some(&meta), "v0.9.0").unwrap(), None);
        // Теги равны — актуален.
        assert_eq!(update_decision(true, Some(&meta), "v0.8.1").unwrap(), None);
        // Тег отличается — обновление доступно.
        assert_eq!(
            update_decision(true, Some(&meta), "v0.9.0").unwrap(),
            Some("v0.9.0".to_string())
        );
        // exe есть, меты нет — «версия неизвестна», а НЕ «актуален» (§00.3).
        assert!(update_decision(true, None, "v0.9.0").is_err());
        // Ни exe, ни меты — нечего проверять.
        assert_eq!(update_decision(false, None, "v0.9.0").unwrap(), None);
    }

    #[test]
    fn skintokens_urls_f16_subfolder() {
        // Все 3 веса — из подпапки F16 HF-репозитория (единственный инференс-квант).
        assert_eq!(
            skintokens_url("mesh-encoder.gguf"),
            "https://huggingface.co/LocalAI-io/SkinTokens-GGUF/resolve/main/F16/mesh-encoder.gguf"
        );
    }

    fn rig_fixture_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("mesh_engine_rig_fixture")
            .join(format!("{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rig_status_on_empty_dir_is_honest() {
        let dir = rig_fixture_dir("empty");
        let status = rig_status(&dir);
        assert!(!status.exe_present);
        assert!(!status.dlls_present);
        assert_eq!(status.weights_present, 0);
        assert!(!status.ready());
        assert!(status.weights_dir.contains("SkinTokens-GGUF"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rig_status_ready_when_all_present() {
        let dir = rig_fixture_dir("ready");
        let weights = crate::engine::rigging::rig_weights_dir(&dir);
        std::fs::create_dir_all(&weights).unwrap();
        std::fs::write(crate::engine::rigging::cli_path(&dir), b"stub").unwrap();
        for dll in crate::engine::rigging::RUNTIME_DLLS {
            std::fs::write(dir.join(dll), b"stub").unwrap();
        }
        for file in SKINTOKENS_FILES {
            std::fs::write(weights.join(file), b"stub").unwrap();
        }
        let status = rig_status(&dir);
        assert!(status.exe_present && status.dlls_present);
        assert_eq!(status.weights_present, SKINTOKENS_FILES.len());
        assert!(status.ready());
        std::fs::remove_dir_all(&dir).ok();
    }
}
