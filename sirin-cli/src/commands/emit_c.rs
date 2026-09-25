use std::path::Path;

use clap::ArgMatches;

use crate::pipeline::compile_to_c;

/// Writes `<file>.c` and, next to it, the `sirin-runtime/` it needs, then
/// prints how to compile them by hand.
pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
    if let Err(e) = emit(path) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

fn emit(path: &str) -> Result<(), String> {
    let program = compile_to_c(path)?;

    let out_path = Path::new(path).with_extension("c");
    std::fs::write(&out_path, &program.c.source)
        .map_err(|e| format!("error: cannot write `{}`: {}", out_path.display(), e))?;

    // Only the parts this program needs are written, so a glob picks up
    // exactly the right sources.
    let out_dir = out_path.parent().unwrap_or(Path::new("."));
    program
        .c
        .runtime
        .write_to(&out_dir.join("sirin-runtime"))
        .map_err(|e| format!("error: {}", e))?;

    let file_name = |p: &Path| p.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let winsock = if program.c.runtime.features.net { " (add -lws2_32 on Windows)" } else { "" };
    println!("{}", out_path.display());
    println!(
        "compile it with:\n  cc {} sirin-runtime/*/*.c -I sirin-runtime -o {}{}",
        file_name(&out_path),
        file_name(&Path::new(path).with_extension("")),
        winsock
    );
    Ok(())
}
