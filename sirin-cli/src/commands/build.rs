use std::path::{Path, PathBuf};

use clap::ArgMatches;

use crate::pipeline::compile_to_c;
use crate::toolchain::build_executable;

/// Result of a successful build: the produced binary and every `.sn` file that
/// went into it (main + transitive local modules) — the watch set for `--watch`.
pub struct BuildResult {
    pub out_path: PathBuf,
    pub sources: Vec<PathBuf>,
}

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
    let size_before = file_kb(&executable_path(path));

    match try_build(path) {
        Ok(res) => {
            let size_after = file_kb(&res.out_path).unwrap_or(0);
            println!("{}", res.out_path.display());
            match size_before {
                Some(before) => println!("size: {}KB → {}KB", before, size_after),
                None         => println!("size: {}KB", size_after),
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

/// Source → executable, without exiting the process on failure (see `pipeline`).
pub fn try_build(path: &str) -> Result<BuildResult, String> {
    let program = compile_to_c(path)?;
    let out_path = executable_path(path);
    build_executable(&program.c, &out_path)?;
    Ok(BuildResult { out_path, sources: program.sources })
}

/// `main.sn` → `main.exe` on Windows, `main` elsewhere.
fn executable_path(path: &str) -> PathBuf {
    Path::new(path).with_extension(if cfg!(windows) { "exe" } else { "" })
}

fn file_kb(path: &Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| (m.len() + 512) / 1024)
}
