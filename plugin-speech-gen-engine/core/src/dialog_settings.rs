use serde::{Deserialize, Serialize};

/// Настройки диалогового движка speech.cpp (full-duplex разговор, модель Haru).
///
/// Сохраняются в `dialog_settings.json` рядом с exe (current_exe().parent()).
///
/// Разделение ответственности (как у STT): плагин владеет процессом движка
/// (`engine_exe`/`model`/`port` — что спавнить), хост владеет оверлеем диалога
/// (`language`/`name`/`persona`/… — что положить в WS-сообщение `start`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogSettings {
    /// Путь к speech.exe (пусто = авторезолв `<engine_dir>/speechcpp/<версия>/speech.exe`).
    pub engine_exe: String,
    /// Спецификация модели для `speech serve` (например `haru-4b:q8_0`).
    pub model: String,
    /// HTTP/WS порт сервера (0 = автоподбор свободного).
    pub port: u16,
    /// Язык карточки персонажа (ISO-код, например "ru"). Обязателен при любой карточке;
    /// сервер отвергает `auto`, если заданы name/persona и т.п.
    pub language: String,
    /// Имя персонажа (WS `start.name`); пусто = не задавать.
    pub name: String,
    /// Персона персонажа (WS `start.persona`); пусто = не задавать.
    pub persona: String,
    /// Имя собеседника (WS `start.other_name`); пусто = модель придумает сама
    /// (и обычно выдумывает произвольное — потому поле и стоит заполнять).
    #[serde(default)]
    pub other_name: String,
    /// Частота backchannel-реплик («да», «угу») — WS `start.backchannel`,
    /// значения low/mid/high (docs/models/haru.md). Пусто = «low».
    #[serde(default)]
    pub backchannel: String,
    /// Путь к файлу голоса (клонирование, WS `start.voice`); пусто = голос модели.
    pub voice: String,
    /// Seed генерации (WS `start.seed`; 0 = не задавать).
    pub seed: i64,
    /// Персонаж говорит первым (WS `start.speak_first`).
    pub speak_first: bool,
    /// Лимит длительности ответа в секундах (WS `start.max_seconds`; 0 = не задавать).
    pub max_seconds: u32,
    /// Имя устройства ввода (cpal); пусто = системный микрофон по умолчанию.
    #[serde(default)]
    pub mic_device: String,
}

impl Default for DialogSettings {
    fn default() -> Self {
        Self {
            engine_exe: String::new(),
            model: "haru-4b:q8_0".into(),
            port: 0,
            language: "ru".into(),
            // Дефолтная карточка НЕ пустая: без name/persona карточки нет и
            // модель говорит «как хочет» (китайский). Дубль фолбэка в хосте
            // (session.rs start_message) покрывает старые файлы с пустыми
            // полями.
            name: "Хару".into(),
            persona: "Дружелюбная собеседница. Говори только по-русски, коротко и живо, как в живом разговоре.".into(),
            other_name: String::new(),
            backchannel: String::new(),
            voice: String::new(),
            seed: 0,
            speak_first: false,
            max_seconds: 0,
            mic_device: String::new(),
        }
    }
}

impl DialogSettings {
    pub fn load() -> Self {
        let path = settings_path();
        if path.exists() {
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(s) = serde_json::from_str(&data) {
                    return s;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path();
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("failed to serialize dialog settings: {e}"))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("failed to write dialog settings to {}: {e}", path.display()))?;
        Ok(())
    }
}

fn settings_path() -> std::path::PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().unwrap_or(std::path::Path::new("."));
    dir.join("dialog_settings.json")
}
