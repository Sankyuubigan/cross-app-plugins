//! (реэкспорт-шим) Утилиты жизненного цикла процессов движков вынесены в
//! общий крейт `engine-proc-util` (SSOT, 17.11.2026: потребителей стало
//! двое — media-gen-core и mesh-gen-core; правка логики только там).
//! Все символы доступны по прежним путям: `crate::engine::process_util::*`
//! и `media_gen_core::engine::process_util::*`.

pub use engine_proc_util::*;
