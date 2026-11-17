const COMMANDS: &[&str] = &[
    "audio_gen_list_models",
    "audio_gen_generate",
    "audio_gen_status",
    "audio_gen_cancel",
    "audio_gen_open_output_dir",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .global_api_script_path("./api-iife.js")
        .build();
}
