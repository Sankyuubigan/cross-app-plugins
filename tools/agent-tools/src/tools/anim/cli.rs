//! CLI тула anim: разбор аргументов и справка.

use std::path::PathBuf;

/// Разобранные аргументы `agent-tools anim`.
pub struct AnimArgs {
    /// Имя клипа (и каталогов prompts/<name>.txt, out/<name>__s<seed>, neuro_soma/<name>).
    pub name: String,
    /// Текст промпта (--prompt) — plain English, правила docs/animation_generation.md §4.
    pub prompt: Option<String>,
    /// Либо файл промпта (--prompt-file).
    pub prompt_file: Option<PathBuf>,
    /// Кадров (fps=30): 24 инста-каст, 60 луп.
    pub frames: u32,
    /// Шаги диффузии (типично 50).
    pub steps: u32,
    /// Сколько сидов сгенерировать (лотерея хореографии — победителя берёт гейт).
    pub seeds: u32,
    /// База сидов (сиды идут подряд).
    pub seed_base: u64,
    /// Тип гейта: pistol_shoot | pistol_shoot_l | throw | none.
    pub gate_type: String,
    /// Корень kimodo.cpp.
    pub kimodo: PathBuf,
    /// Godot console exe (гейт).
    pub godot: PathBuf,
    /// Проект 3d_game (гейт и neuro_soma).
    pub project: PathBuf,
    /// Разрешить перезапись существующего клипа в neuro_soma.
    pub force: bool,
}

pub fn usage() -> String {
    "anim — «сделать анимацию» одной командой (kimodo.cpp → гейт → neuro_soma)

USAGE:
    agent-tools anim --name <clip> (--prompt \"...\" | --prompt-file F) [опции]

ОБЯЗАТЕЛЬНО:
    --name <clip>        имя клипа (cast_zap, throw_big, ...)
    --prompt \"...\"       промпт plain English (правила: docs/animation_generation.md §4)
    --prompt-file F      ...или файл с промптом

ОПЦИИ (значения по умолчанию):
    --frames <N>         24    (fps=30; 24 инста-каст, 60 луп)
    --steps <N>          50    (шаги диффузии)
    --seeds <N>          3     (лотерея хореографии; победителя берёт гейт)
    --seed-base <N>      42
    --type <T>           none  (гейт: cast_push | cast_push_l | pistol_shoot |
                               pistol_shoot_l | left_bent_front | throw | none)
    --kimodo <DIR>       D:\\nn\\kimodo.cpp
    --godot <EXE>        D:\\Programs\\Godot\\Godot_v4.8-beta3\\Godot_v4.8-beta3_win64_console.exe
    --project <DIR>      D:\\Projects\\3d_game
    --force                    разрешить перезапись существующего клипа

Что делает: пишет prompts/<clip>.txt, параллельно генерит --seeds сидов
kmd-generate, прогоняет численный гейт probe_hand_path.gd (тип --type), первый
PASS ставит в Graphics/Animations/neuro_soma/<clip>/ (+prompt.txt). Ретаргет и
тесты НЕ запускает — печатает next steps (мусор не доезжает до игры молча)."
    .to_string()
}

/// Разбор argv после `anim`. Ok(args) | Err(справка) — справка печатается
/// и завершает тул кодом 0 (явный --help) или кодом 2 (ошибка ввода).
pub fn parse(args: &[String]) -> Result<AnimArgs, String> {
    if args.iter().any(|s| s == "--help" || s == "-h") {
        return Err(usage());
    }
    let mut a = AnimArgs {
        name: String::new(),
        prompt: None,
        prompt_file: None,
        frames: 24,
        steps: 50,
        seeds: 3,
        seed_base: 42,
        gate_type: "none".into(),
        kimodo: PathBuf::from("D:\\nn\\kimodo.cpp"),
        godot: PathBuf::from(
            "D:\\Programs\\Godot\\Godot_v4.8-beta3\\Godot_v4.8-beta3_win64_console.exe",
        ),
        project: PathBuf::from("D:\\Projects\\3d_game"),
        force: false,
    };
    let mut i = 2; // argv[0]="agent-tools", argv[1]="anim" — конвенция всех тулов (см. codemap)
    let mut name_set = false;
    while i < args.len() {
        let flag = args[i].as_str();
        let value = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i).cloned().ok_or_else(|| format!("нет значения для {}", args[*i - 1]))
        };
        match flag {
            "--name" => {
                a.name = value(&mut i)?;
                name_set = true;
            }
            "--prompt" => a.prompt = Some(value(&mut i)?),
            "--prompt-file" => a.prompt_file = Some(PathBuf::from(value(&mut i)?)),
            "--frames" => a.frames = value(&mut i)?.parse().map_err(|_| "--frames: число")?,
            "--steps" => a.steps = value(&mut i)?.parse().map_err(|_| "--steps: число")?,
            "--seeds" => a.seeds = value(&mut i)?.parse().map_err(|_| "--seeds: число")?,
            "--seed-base" => {
                a.seed_base = value(&mut i)?.parse().map_err(|_| "--seed-base: число")?
            }
            "--type" => a.gate_type = value(&mut i)?,
            "--kimodo" => a.kimodo = PathBuf::from(value(&mut i)?),
            "--godot" => a.godot = PathBuf::from(value(&mut i)?),
            "--project" => a.project = PathBuf::from(value(&mut i)?),
            "--force" => a.force = true,
            other => return Err(format!("неизвестный флаг {}", other)),
        }
        i += 1;
    }
    if !name_set {
        return Err("обязателен --name <clip>".into());
    }
    if !a.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("--name: только латиница, цифры и _ (имя становится каталогами)".into());
    }
    match a.gate_type.as_str() {
        "pistol_shoot" | "pistol_shoot_l" | "cast_push" | "cast_push_l" | "left_bent_front"
        | "throw" | "none" => {}
        other => return Err(format!("--type {}: нет такого типа гейта", other)),
    }
    if a.prompt.is_some() && a.prompt_file.is_some() {
        return Err("--prompt и --prompt-file взаимоисключающи".into());
    }
    Ok(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(items: &[&str]) -> Vec<String> {
        let mut v = vec!["agent-tools".to_string(), "anim".to_string()];
        v.extend(items.iter().map(|s| s.to_string()));
        v
    }

    #[test]
    fn parse_minimal_uses_defaults() {
        let a = parse(&argv(&["--name", "cast_zap", "--prompt", "a short cast"]))
            .expect("минимальный вызов обязан парситься");
        assert_eq!(a.name, "cast_zap");
        assert_eq!(a.frames, 24);
        assert_eq!(a.steps, 50);
        assert_eq!(a.seeds, 3);
        assert_eq!(a.seed_base, 42);
        assert_eq!(a.gate_type, "none");
        assert!(!a.force);
    }

    #[test]
    fn parse_requires_name() {
        assert!(parse(&argv(&["--prompt", "x"])).is_err());
    }

    #[test]
    fn parse_rejects_bad_name() {
        // Имя становится каталогами: дефис/слэш/юникод запрещены.
        for bad in ["cast-zap", "a/b", "клип"] {
            let res = parse(&argv(&["--name", bad, "--prompt", "x"]));
            assert!(res.is_err(), "имя '{}' обязано отбраковаться", bad);
        }
    }

    #[test]
    fn parse_accepts_every_probe_gate_type() {
        // Whitelist тулa обязан совпадать с типами probe_hand_path.gd (§5 ССОТ).
        for gate in [
            "pistol_shoot",
            "pistol_shoot_l",
            "cast_push",
            "cast_push_l",
            "left_bent_front",
            "throw",
            "none",
        ] {
            let res = parse(&argv(&["--name", "clip", "--prompt", "x", "--type", gate]));
            assert!(res.is_ok(), "гейт '{}' обязан парситься", gate);
        }
        assert!(parse(&argv(&["--name", "clip", "--prompt", "x", "--type", "kick"])).is_err());
    }

    #[test]
    fn parse_rejects_conflicting_prompts() {
        let res = parse(&argv(&[
            "--name", "clip", "--prompt", "x", "--prompt-file", "p.txt",
        ]));
        assert!(res.is_err());
    }

    #[test]
    fn parse_rejects_flag_without_value() {
        assert!(parse(&argv(&["--name", "clip", "--prompt"])).is_err());
    }

    #[test]
    fn help_flag_returns_usage() {
        match parse(&argv(&["--help"])) {
            Err(msg) => assert!(msg.contains("USAGE")),
            Ok(_) => panic!("--help обязан вернуть справку, а не аргументы"),
        }
    }
}
