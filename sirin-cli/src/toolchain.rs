//! C → executable. The program and the runtime it needs are written into a
//! fresh build directory, then compiled by the system C compiler (Linux and
//! macOS) or by the embedded TinyCC (Windows).

use std::path::{Path, PathBuf};

use sirin_codegen_c::emit::CProgram;
use sirin_codegen_c::runtime::Features;

pub fn build_executable(program: &CProgram, out: &Path) -> Result<(), String> {
    let dir = BuildDir::new()?;
    let mut sources = program
        .runtime
        .write_to(dir.path())
        .map_err(|e| format!("error: {}", e))?;
    let main_c = dir.path().join("program.c");
    std::fs::write(&main_c, &program.source)
        .map_err(|e| format!("error: cannot write {}: {}", main_c.display(), e))?;
    sources.push(main_c);

    native::compile(dir.path(), &sources, program.runtime.features, out)
}

/// A temporary directory for one build, removed when dropped. Unique per
/// build, so concurrent builds (e.g. `run --watch` and `build`) don't collide.
struct BuildDir(PathBuf);

impl BuildDir {
    fn new() -> Result<Self, String> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "sirin-build-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("error: cannot create build directory {}: {}", path.display(), e))?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for BuildDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(not(windows))]
mod native {
    use super::*;
    use std::process::Command;

    /// The first of `cc`, `clang`, `gcc` that is installed.
    fn find_compiler() -> Result<&'static str, String> {
        ["cc", "clang", "gcc"]
            .into_iter()
            .find(|cmd| Command::new(cmd).arg("--version").output().is_ok())
            .ok_or_else(|| {
                "error: no C compiler found\n  help: install `cc`, `clang` or `gcc` \
                 (e.g. `sudo apt install build-essential` or `xcode-select --install`)"
                    .to_string()
            })
    }

    pub fn compile(dir: &Path, sources: &[PathBuf], _features: Features, out: &Path) -> Result<(), String> {
        let status = Command::new(find_compiler()?)
            .args(sources)
            .arg("-I")
            .arg(dir)
            .arg("-o")
            .arg(out)
            .args(["-O2", "-Wno-deprecated-declarations"])
            .status()
            .map_err(|e| format!("compile error: cannot run the C compiler: {}", e))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("compile error: the C compiler exited with {}", status))
        }
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use sirin_codegen_c::tcc_paths;
    use sirin_codegen_c::tinycc::{TCC_OUTPUT_EXE, Tcc};

    pub fn compile(dir: &Path, sources: &[PathBuf], features: Features, out: &Path) -> Result<(), String> {
        let tcc_err = |e: String| format!("tcc error: {}", e);
        let fwd = |p: &Path| p.to_string_lossy().replace('\\', "/");

        let tcc = Tcc::new().map_err(|e| format!("error: {}", e))?;
        let paths = tcc_paths();
        tcc.set_lib_path(&paths.win32);
        tcc.add_library_path(&paths.runtime).map_err(tcc_err)?;
        tcc.add_library_path(&format!("{}/lib", paths.win32)).map_err(tcc_err)?;
        tcc.add_include_path(&format!("{}/include", paths.root)).map_err(tcc_err)?;
        tcc.add_include_path(&format!("{}/include", paths.win32)).map_err(tcc_err)?;
        tcc.add_include_path(&format!("{}/include/winapi", paths.win32)).map_err(tcc_err)?;
        tcc.add_include_path(&fwd(dir)).map_err(tcc_err)?;

        tcc.set_output_type(TCC_OUTPUT_EXE).map_err(tcc_err)?;
        tcc.set_options("-s").map_err(tcc_err)?;
        tcc.set_options("-Os").map_err(tcc_err)?;

        tcc.add_file(&format!("{}/lib/crt1.c", paths.win32))
            .map_err(|e| format!("tcc error (crt1): {}", e))?;
        for src in sources {
            tcc.add_file(&fwd(src)).map_err(|e| format!("compile error:\n{}", e))?;
        }
        if features.net {
            tcc.add_library("ws2_32").map_err(tcc_err)?;
        }
        tcc.output_file(&fwd(out)).map_err(|e| format!("link error: {}", e))
    }
}
