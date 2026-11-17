//! Движок 3D-генерации: пресеты весов, жизненный цикл trellis-server,
//! HTTP-клиент `/generate`, сохранение GLB, префлайт и оркестрация.

pub mod client;
pub mod config;
pub mod generate;
pub mod installer;
pub mod outputs;
pub mod preflight;
pub mod presets;
pub mod prompting;
pub mod process;
pub mod rigging;

// Реэкспорт утилит процессов из SSOT-крейта (правка логики только там).
pub use engine_proc_util::{
    assign_child_to_kill_job, kill_active_engines, kill_pid_tree, kill_process_tree,
    register_engine_pid, unregister_engine_pid,
};

pub use client::{post_generate, GenerateFields, GEN_TIMEOUT};
pub use generate::{generate_mesh, validate, GenerateMeshParams, MeshGenOutcome};
pub use outputs::save_glb_sequenced;
pub use preflight::{
    check_engine_files, engine_paths, exe_path, weights_status, EnginePaths, QuantWeightsStatus,
};
pub use presets::{weights_dir, MeshPreset, MeshQuant, DEFAULT_QUANT, DEFAULT_RESOLUTION};
pub use prompting::{build_concept_prompt, ConceptKind, CONCEPT_KIND_CATALOG};
pub use process::{pick_free_port, MeshEngine, SpawnConfig};
pub use config::{load_skintokens_dir, rig_dir_early};
pub use rigging::{
    check_rig_files, cli_path, rig_mesh, validate_device, weights_file_paths, RigMeshParams,
    RigOutcome, RIG_TIMEOUT,
};
