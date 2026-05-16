use std::path::Path;

pub const RUNTIME_H: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sirin-runtime/sirin_runtime.h"
));

pub const RUNTIME_C: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../sirin-runtime/sirin_runtime.c"
));

/// Writes `sirin_runtime.h` and `sirin_runtime.c` into `output_dir`.
/// Called by `emit-c` to place the runtime alongside the generated C file.
pub fn write_runtime(output_dir: &Path) -> Result<(), String> {
    let h = output_dir.join("sirin_runtime.h");
    let c = output_dir.join("sirin_runtime.c");
    std::fs::write(&h, RUNTIME_H)
        .map_err(|e| format!("cannot write {}: {}", h.display(), e))?;
    std::fs::write(&c, RUNTIME_C)
        .map_err(|e| format!("cannot write {}: {}", c.display(), e))?;
    Ok(())
}
