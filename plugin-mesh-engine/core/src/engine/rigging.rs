//! Риггинг статических GLB через skin-tokens.cpp (этап 2 — PRP «Риггинг
//! skin-tokens.md»): авто-скелет + веса скиннинга (JOINTS_0/WEIGHTS_0 в
//! выходном GLB). Анимации это НЕ добавляет (анимация — kimodo.cpp,
//! дальнейший этап). Движок — CLI `skintokens-cli rig MODEL_DIR in.glb
//! out.glb [--device …]`: HTTP-сервера нет, синхронный спавн процесса.
//! Проба 17.11.26: сборка MSVC требует `/FS` + `/Z7` (PDB-гонка) +
//! `_CRT_SECURE_NO_WARNINGS` (getenv под /sdl); риг дака на CPU — ~30-40
//! минут (TokenRig F16 автогрессивный), отсюда таймаут RIG_TIMEOUT.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::assign_child_to_kill_job;

/// Таймаут одного рига: CPU-инференс TokenRig автогрессивный (проба
/// 17.11.26: ~30-40 минут для миниатюрного дака на Ryzen 9 7900X).
pub const RIG_TIMEOUT: Duration = Duration::from_secs(3600);

const CLI_FILE: &str = "skintokens-cli.exe";
/// Рантайм-DLL рядом с CLI (Windows-лоадер ищет в папке exe первым).
pub const RUNTIME_DLLS: [&str; 3] = ["skintokens.dll", "ggml.dll", "ggml-base.dll"];
/// Папка весов относительно rig_dir (F16 — единственный инференс-квант
/// SkinTokens-GGUF; F32 существует только для parity-тестов upstream).
pub const WEIGHTS_REL: &str = "models/SkinTokens-GGUF/F16";
pub const WEIGHTS_FILES: [&str; 3] = ["mesh-encoder.gguf", "skin-vae.gguf", "tokenrig.gguf"];
pub const DEVICES_ALLOWED: [&str; 3] = ["cpu", "vulkan", "auto"];
pub const DEFAULT_DEVICE: &str = "cpu";

const TAIL_CHARS: usize = 2000;

/// Путь CLI в папке рига.
pub fn cli_path(rig_dir: &Path) -> PathBuf {
    rig_dir.join(CLI_FILE)
}

/// Путь к весам: `<rig_dir>/models/SkinTokens-GGUF/F16`.
pub fn rig_weights_dir(rig_dir: &Path) -> PathBuf {
    rig_dir.join(WEIGHTS_REL)
}

/// Пути трёх весов риггинга.
pub fn weights_file_paths(rig_dir: &Path) -> Vec<PathBuf> {
    WEIGHTS_FILES
        .iter()
        .map(|f| rig_weights_dir(rig_dir).join(f))
        .collect()
}

/// Валидация `--device` (чистая, тестится без fs/сети). None → дефолт `cpu`
/// (пробная сборка без Vulkan — честный дефолт, а не «auto наугад»).
pub fn validate_device(device: Option<&str>) -> Result<String, String> {
    let raw = device
        .unwrap_or(DEFAULT_DEVICE)
        .trim()
        .to_ascii_lowercase();
    if DEVICES_ALLOWED.contains(&raw.as_str()) {
        Ok(raw)
    } else {
        Err(format!(
            "device '{}' не поддержан: допустимо cpu|vulkan|auto",
            raw
        ))
    }
}

/// Preflight раскладки рига: CLI + рантайм-DLL + 3 веса. Ошибка — готовая
/// инструкция (§0.2: отсутствие файлов — честный отказ, не ретрай).
pub fn check_rig_files(rig_dir: &Path) -> Result<(), String> {
    let cli = cli_path(rig_dir);
    if !cli.is_file() {
        return Err(format!(
            "skintokens-cli.exe не найден: {} — нужен движок риггинга (сборка \
             localai-org/skin-tokens.cpp), папка — ключ skintokens_dir в app_config.json",
            cli.display()
        ));
    }
    for dll in RUNTIME_DLLS {
        let p = rig_dir.join(dll);
        if !p.is_file() {
            return Err(format!(
                "Рантайм-DLL риггинга не найдена: {} — рядом со skintokens-cli.exe \
                 должны лежать skintokens.dll, ggml.dll, ggml-base.dll и ggml-cpu-*.dll \
                 из одной сборки",
                p.display()
            ));
        }
    }
    for w in weights_file_paths(rig_dir) {
        if !w.is_file() {
            return Err(format!(
                "Вес риггинга не найден: {} — скачай веса в панели «Движки» → \
                 Риггинг (LocalAI-io/SkinTokens-GGUF, F16)",
                w.display()
            ));
        }
    }
    Ok(())
}

/// Следующий номер выхода: max(rigged-N.glb) + 1, иначе 1 (аналог
/// outputs::next_sequence, но с префиксом rigged-).
fn next_rigged_sequence(out_dir: &Path) -> Result<u32, String> {
    let mut max: u32 = 0;
    let entries = std::fs::read_dir(out_dir)
        .map_err(|e| format!("не удалось прочитать {}: {e}", out_dir.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(rest) = name.strip_prefix("rigged-").and_then(|r| r.strip_suffix(".glb")) {
            if let Ok(n) = rest.parse::<u32>() {
                max = max.max(n);
            }
        }
    }
    Ok(max + 1)
}

/// Параметры одного рига.
pub struct RigMeshParams {
    /// Папка движка риггинга (`rig_dir_early()`).
    pub rig_dir: PathBuf,
    /// Папка вывода (сессия: `<exe>/mesh_outputs/<session_id>`).
    pub out_dir: PathBuf,
    /// Исходный статический GLB.
    pub mesh_path: PathBuf,
    /// `cpu|vulkan|auto` (None → `cpu`).
    pub device: Option<String>,
}

/// Результат рига.
#[derive(Debug)]
pub struct RigOutcome {
    /// GLB со скелетом и весами скиннинга.
    pub glb_path: PathBuf,
    /// Длительность прогона (сек) — для честного отчёта агенту.
    pub duration_secs: f64,
}

/// Один риггинг: валидация → префлайт → синхронный спавн CLI (таймаут
/// RIG_TIMEOUT) → проверка выходного файла. Консоль не мигает
/// (CREATE_NO_WINDOW), процесс не переживёт хост (Windows Job Object).
pub fn rig_mesh(p: &RigMeshParams) -> Result<RigOutcome, String> {
    let device = validate_device(p.device.as_deref())?;
    let input = &p.mesh_path;
    if !input.is_file() {
        return Err(format!("Исходный меш не найден: {}", input.display()));
    }
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "glb" && ext != "t2mesh" {
        return Err(format!(
            "Риггинг принимает .glb (или .t2mesh), получено .{}: {}",
            ext,
            input.display()
        ));
    }
    check_rig_files(&p.rig_dir)?;
    std::fs::create_dir_all(&p.out_dir)
        .map_err(|e| format!("не удалось создать папку {}: {e}", p.out_dir.display()))?;
    let out_path = p
        .out_dir
        .join(format!("rigged-{}.glb", next_rigged_sequence(&p.out_dir)?));

    let args: Vec<String> = vec![
        "rig".to_string(),
        rig_weights_dir(&p.rig_dir).to_string_lossy().into_owned(),
        input.to_string_lossy().into_owned(),
        out_path.to_string_lossy().into_owned(),
        "--device".to_string(),
        device,
    ];

    let mut cmd = Command::new(cli_path(&p.rig_dir));
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // ggml-бекенды грузятся динамически: PATH-префикс папки рига на случай,
    // когда лоадер не находит их по папке самой библиотеки.
    if let Ok(existing) = std::env::var("PATH") {
        cmd.env("PATH", format!("{};{}", p.rig_dir.display(), existing));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: не мигаем консолью из GUI-хоста.
        cmd.creation_flags(0x0800_0000);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Не удалось запустить {}: {e}", CLI_FILE))?;
    assign_child_to_kill_job(&child);

    // Пайпы читаем в отдельных потоках: без этого заполненный буфер пайпа
    // блокирует ребёнка, а мы его — deadlock до таймаута (паттерн runner.rs).
    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();
    let out_thread = std::thread::spawn(move || read_all(&mut stdout_pipe));
    let err_thread = std::thread::spawn(move || read_all(&mut stderr_pipe));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) if started.elapsed() >= RIG_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break None; // таймаут: пайпы закроются смертью ребёнка
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(format!("Ожидание {}: {e}", CLI_FILE)),
        }
    };
    let duration_secs = started.elapsed().as_secs_f64();
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    // Последние строки проговора — в лог (полные хвосты уходят в ошибку).
    let mut log_lines: Vec<&str> = stdout.lines().chain(stderr.lines()).collect();
    log_lines.reverse();
    for line in log_lines.into_iter().take(4) {
        log::info!("[skintokens] {line}");
    }

    let Some(st) = status else {
        return Err(format!(
            "Превышен таймаут {} сек: риггинг {} не завершился",
            RIG_TIMEOUT.as_secs(),
            input.display()
        ));
    };
    if !st.success() {
        return Err(format!(
            "skintokens-cli завершился с кодом {}: {}",
            st.code().unwrap_or(-1),
            tail(&format!("{stderr}{stdout}"), TAIL_CHARS)
        ));
    }
    if !out_path.is_file() {
        return Err(format!(
            "CLI отчитался об успехе, но выходной GLB не создан: {}",
            out_path.display()
        ));
    }
    log::info!(
        "[rig] GLB с ригом: {} ({:.1} сек)",
        out_path.display(),
        duration_secs
    );
    Ok(RigOutcome {
        glb_path: out_path,
        duration_secs,
    })
}

fn read_all<R: Read>(pipe: &mut Option<R>) -> String {
    let Some(pipe) = pipe.as_mut() else {
        return String::new();
    };
    let mut buf = Vec::new();
    if pipe.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Хвост строки с учётом границ UTF-8 (лог CLI может оборваться на байте).
fn tail(s: &str, max: usize) -> String {
    let bytes = s.as_bytes();
    if bytes.len() <= max {
        return s.to_string();
    }
    let mut start = bytes.len() - max;
    while start < bytes.len() && (bytes[start] & 0b1100_0000) == 0b1000_0000 {
        start += 1;
    }
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "skintokens-rig-test-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn device_defaults_to_cpu_and_rejects_unknown() {
        assert_eq!(validate_device(None).unwrap(), "cpu");
        for ok in ["cpu", "vulkan", "auto", " CPU "].iter() {
            assert!(validate_device(Some(ok)).is_ok(), "{ok}");
        }
        let err = validate_device(Some("cuda")).unwrap_err();
        assert!(err.contains("cpu|vulkan|auto"), "{err}");
    }

    #[test]
    fn layout_paths_live_under_rig_dir() {
        let dir = Path::new("D:/rig");
        assert!(cli_path(dir).starts_with(dir));
        assert!(rig_weights_dir(dir).starts_with(dir));
        for w in weights_file_paths(dir) {
            assert!(w.starts_with(rig_weights_dir(dir)), "{}", w.display());
        }
        assert!(weights_file_paths(dir).len() == WEIGHTS_FILES.len());
    }

    #[test]
    fn preflight_reports_missing_exe_honestly() {
        let dir = temp_dir("no-exe");
        let err = check_rig_files(&dir).unwrap_err();
        assert!(err.contains("skintokens-cli.exe не найден"), "{err}");
        assert!(err.contains(&dir.display().to_string()), "{err}");
    }

    #[test]
    fn preflight_reports_missing_weight_with_panel_hint() {
        let dir = temp_dir("no-weights");
        touch(&cli_path(&dir));
        for dll in RUNTIME_DLLS {
            touch(&dir.join(dll));
        }
        let err = check_rig_files(&dir).unwrap_err();
        assert!(err.contains("mesh-encoder.gguf"), "{err}");
        assert!(err.contains("Риггинг"), "{err}");
    }

    #[test]
    fn preflight_reports_missing_runtime_dll() {
        let dir = temp_dir("no-dll");
        touch(&cli_path(&dir));
        let err = check_rig_files(&dir).unwrap_err();
        assert!(err.contains("Рантайм-DLL"), "{err}");
    }

    #[test]
    fn preflight_passes_when_layout_complete() {
        let dir = temp_dir("complete");
        touch(&cli_path(&dir));
        for dll in RUNTIME_DLLS {
            touch(&dir.join(dll));
        }
        for w in weights_file_paths(&dir) {
            touch(&w);
        }
        check_rig_files(&dir).unwrap();
    }

    #[test]
    fn sequenced_out_paths_increment() {
        let dir = temp_dir("sequence");
        assert_eq!(next_rigged_sequence(&dir).unwrap(), 1);
        touch(&dir.join("rigged-1.glb"));
        touch(&dir.join("rigged-3.glb"));
        assert_eq!(next_rigged_sequence(&dir).unwrap(), 4);
    }

    #[test]
    fn input_extension_gate_rejects_obj() {
        let dir = temp_dir("ext");
        let rig = temp_dir("ext-rig");
        touch(&cli_path(&rig));
        for dll in RUNTIME_DLLS {
            touch(&rig.join(dll));
        }
        for w in weights_file_paths(&rig) {
            touch(&w);
        }
        let obj = dir.join("duck.obj");
        touch(&obj);
        let err = rig_mesh(&RigMeshParams {
            rig_dir: rig.clone(),
            out_dir: dir.join("out"),
            mesh_path: obj.clone(),
            device: None,
        })
        .unwrap_err();
        assert!(err.contains(".glb"), "{err}");
        assert!(err.contains("duck.obj"), "{err}");
    }

    #[test]
    fn missing_input_mesh_is_honest_error() {
        let dir = temp_dir("no-input");
        let err = rig_mesh(&RigMeshParams {
            rig_dir: dir.join("rig"),
            out_dir: dir.join("out"),
            mesh_path: dir.join("absent.glb"),
            device: None,
        })
        .unwrap_err();
        assert!(err.contains("не найден"), "{err}");
    }

    #[test]
    fn tail_keeps_utf8_boundaries() {
        let s = "привет мир".repeat(200);
        let t = tail(&s, 10);
        assert!(!t.is_empty());
        assert!(t.chars().count() <= 10);
    }
}
