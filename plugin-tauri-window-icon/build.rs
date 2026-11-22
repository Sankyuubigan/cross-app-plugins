// У плагина НЕТ команд (поверхность invoke отсутствует — см. PRP): пустой
// COMMANDS, build.rs нужен только ради `links` (PLUGIN_STANDARD §4.1).
const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
