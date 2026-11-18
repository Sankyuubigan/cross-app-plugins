//! anim — «сделать анимацию» одной командой (обёртка пайплайна kimodo.cpp).
//!
//! ССОТ пайплайна: docs/animation_generation.md в проекте 3d_game (§0–§2).
//! Тул закрывает шаги 1–4: промпт → генерация сидов kmd-generate → численный
//! гейт probe_hand_path.gd → установка победителя в neuro_soma. Ретаргет,
//! тесты и смоук остаются явными командами Godot — тул печатает их как
//! next steps, чтобы мусорная генерация не доезжала до игры молча.

use crate::exit::Exit;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod cli;
pub use self::cli::AnimArgs;

/// Параметры запуска kmd-generate (см. docs/animation_generation.md §1).
struct KmdSpec {
    kimodo: PathBuf,
    motion: PathBuf,
    text: PathBuf,
    prompt: PathBuf,
    frames: u32,
    steps: u32,
    seed: u64,
    out_dir: PathBuf,
}

pub fn run(args: Vec<String>) -> Exit {
    let parsed = cli::parse(&args);
    let a = match parsed {
        Ok(v) => v,
        Err(help) => {
            if help.is_empty() {
                return Exit::Input;
            }
            println!("{}", help);
            return Exit::Ok;
        }
    };
    match execute(&a) {
        Ok(report) => {
            println!("{}", report);
            Exit::Ok
        }
        Err(e) => {
            eprintln!("anim FAIL: {}", e);
            Exit::Internal
        }
    }
}

fn execute(a: &AnimArgs) -> Result<String, String> {
    // 1) Промпт → prompts/<clip>.txt
    let prompt_text = resolve_prompt(a)?;
    let prompts_dir = a.kimodo.join("prompts");
    fs::create_dir_all(&prompts_dir).map_err(|e| format!("prompts/: {}", e))?;
    let prompt_path = prompts_dir.join(format!("{}.txt", a.name));
    fs::write(&prompt_path, &prompt_text).map_err(|e| format!("write prompt: {}", e))?;

    // 2) Генерация сидов (параллельно, процессы kimodo тяжёлые по RAM ~8ГБ)
    let out_root = a.kimodo.join("out");
    fs::create_dir_all(&out_root).map_err(|e| format!("out/: {}", e))?;
    let mut children: Vec<(u64, PathBuf, std::process::Child)> = Vec::new();
    for i in 0..a.seeds {
        let seed = a.seed_base + i as u64;
        let dir = out_root.join(format!("{}__s{}", a.name, seed));
        let spec = KmdSpec {
            kimodo: a.kimodo.clone(),
            motion: a.kimodo.join("weights/kimodo-soma-rp-v1.1-f32.gguf"),
            text: a.kimodo.join("weights/Llama-3-Kimodo-Q6_K.gguf"),
            prompt: prompt_path.clone(),
            frames: a.frames,
            steps: a.steps,
            seed,
            out_dir: dir.clone(),
        };
        let child = spawn_kmd(&spec).map_err(|e| format!("seed {}: {}", seed, e))?;
        children.push((seed, dir, child));
    }
    let mut ok_dirs: Vec<(u64, PathBuf)> = Vec::new();
    for (seed, dir, mut child) in children {
        let st = child.wait().map_err(|e| format!("wait seed {}: {}", seed, e))?;
        if st.success() && data_files_present(&dir) {
            println!("anim: seed {} готов → {}", seed, dir.display());
            ok_dirs.push((seed, dir));
        } else {
            println!("anim: seed {} ПРОВАЛ (exit={:?})", seed, st.code());
        }
    }
    if ok_dirs.is_empty() {
        return Err("ни один сид не сгенерировался".into());
    }

    // 3) Численный гейт хореографии (ДО ретаргета)
    let mut winner: Option<(u64, PathBuf)> = None;
    if a.gate_type != "none" {
        for (seed, dir) in &ok_dirs {
            let pass = run_gate(a, &a.gate_type, dir)?;
            println!("anim: гейт {} seed {} → {}", a.gate_type, seed, if pass { "PASS" } else { "FAIL" });
            if pass {
                winner = Some((*seed, dir.clone()));
                break;
            }
        }
        if winner.is_none() {
            return Err(format!(
                "все сиды провалили гейт {} — смените промпт/сиды (мусор не доезжает до игры)",
                a.gate_type
            ));
        }
    } else {
        winner = Some(ok_dirs[0].clone());
        println!("anim: гейт пропущен (--type none), победитель — первый успешный сид");
    }
    let (seed, dir) = winner.unwrap();

    // 4) Установка победителя в neuro_soma (без перезаписи без --force)
    let dest = a.project.join("Graphics/Animations/neuro_soma").join(&a.name);
    if dest.exists() && !a.force {
        return Err(format!(
            "{} уже существует — укажите --force для перезаписи",
            dest.display()
        ));
    }
    fs::create_dir_all(&dest).map_err(|e| format!("neuro_soma: {}", e))?;
    for f in ["root_positions.f32", "local_rotations_xyzw.f32"] {
        fs::copy(dir.join(f), dest.join(f)).map_err(|e| format!("copy {}: {}", f, e))?;
    }
    fs::write(dest.join("prompt.txt"), &prompt_text).map_err(|e| format!("prompt.txt: {}", e))?;

    // 5) Отчёт + next steps (этапы 5–7 ССОТ — явные команды Godot)
    let godot = a.godot.display();
    Ok(format!(
        "anim: '{}' установлен (seed {}, источник {})\n\
         Далее (ССОТ docs/animation_generation.md §2):\n\
         \x20 \"{}\" --headless -s res://Generators/retarget_soma_anims.gd\n\
         \x20 \"{}\" --headless -s res://Generators/test_soma_invariants.gd\n\
         \x20 \"{}\" --headless -s res://Generators/test_cast_anims.gd\n\
         \x20 timeout 40 \"{}\" --headless res://Scenes/MainArena.tscn --quit-after 300",
        a.name, seed, dir.display(), godot, godot, godot, godot
    ))
}

fn resolve_prompt(a: &AnimArgs) -> Result<String, String> {
    if let Some(f) = &a.prompt_file {
        return fs::read_to_string(f).map_err(|e| format!("prompt-file: {}", e));
    }
    match &a.prompt {
        Some(p) if !p.trim().is_empty() => Ok(p.clone()),
        _ => Err("нужен --prompt \"...\" или --prompt-file F".into()),
    }
}

fn data_files_present(dir: &Path) -> bool {
    dir.join("root_positions.f32").is_file() && dir.join("local_rotations_xyzw.f32").is_file()
}

/// kmd-generate: PATH-квирик (bin/ содержит только DLL, exe и dll-зависимости
/// разнесены по build/cpu и build/cpu/bin) + cwd=<kimodo>.
fn spawn_kmd(s: &KmdSpec) -> Result<std::process::Child, String> {
    let exe = s.kimodo.join("build/cpu/kmd-generate.exe");
    if !exe.is_file() {
        return Err(format!("нет {}", exe.display()));
    }
    let bin = s.kimodo.join("build/cpu/bin");
    let cpu = s.kimodo.join("build/cpu");
    let old_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{};{};{}", bin.display(), cpu.display(), old_path);
    Command::new(&exe)
        .args([
            s.motion.to_string_lossy().as_ref(),
            s.text.to_string_lossy().as_ref(),
            s.prompt.to_string_lossy().as_ref(),
            &s.frames.to_string(),
            &s.steps.to_string(),
            &s.seed.to_string(),
            s.out_dir.to_string_lossy().as_ref(),
        ])
        .current_dir(&s.kimodo)
        .env("PATH", new_path)
        .spawn()
        .map_err(|e| format!("spawn kmd-generate: {}", e))
}

/// Гейт хореографии: godot --headless -s probe_hand_path.gd -- <type> <dir>.
/// EXIT=0 = PASS (контракт зонда).
fn run_gate(a: &AnimArgs, gate_type: &str, dir: &Path) -> Result<bool, String> {
    let st = Command::new(&a.godot)
        .args([
            "--headless",
            "--path",
            a.project.to_string_lossy().as_ref(),
            "-s",
            "res://Generators/probe_hand_path.gd",
            "--",
            gate_type,
            dir.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|e| format!("spawn godot (gate): {}", e))?;
    // Лог зонда — на экран агенту (верdict-строки PASS/FAIL с метриками)
    let out = String::from_utf8_lossy(&st.stdout);
    for line in out.lines().filter(|l| l.starts_with("GATE")) {
        println!("anim: {}", line);
    }
    Ok(st.status.success())
}
