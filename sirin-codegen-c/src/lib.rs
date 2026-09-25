pub mod emit;
pub mod runtime;
pub mod tinycc;

use std::path::{Path, PathBuf};

// Locations of the vendored TinyCC tree on the machine that compiled this
// crate. Only valid for local/dev builds — see `tcc_paths` for installs.
pub const TCC_DIR: &str = env!("TCC_DIR");
pub const TCC_WIN32_DIR: &str = env!("TCC_WIN32_DIR");
pub const TCC_RUNTIME_DIR: &str = env!("TCC_RUNTIME_DIR");

/// Where TinyCC finds its headers, CRT sources and runtime library.
pub struct TccPaths {
    /// Holds `include/` (tccdefs.h, stdarg.h, ...).
    pub root: String,
    /// Holds `include/`, `include/winapi/` and `lib/` (crt1.c, *.def).
    pub win32: String,
    /// Holds `libtcc1.a`.
    pub runtime: String,
}

impl TccPaths {
    /// A relocatable bundle, as shipped in release archives:
    /// `<dir>/include`, `<dir>/win32/{include,lib}`, `<dir>/lib/libtcc1.a`.
    fn bundle(dir: &Path) -> Self {
        let fwd = |p: PathBuf| p.display().to_string().replace('\\', "/");
        Self {
            root: fwd(dir.to_path_buf()),
            win32: fwd(dir.join("win32")),
            runtime: fwd(dir.join("lib")),
        }
    }
}

/// Resolve the TinyCC support files, in order of preference:
/// 1. `SIRIN_TCC_DIR` (a bundle directory),
/// 2. a `tcc/` bundle next to the running executable (release installs),
/// 3. the vendored tree this crate was built from (dev builds).
pub fn tcc_paths() -> TccPaths {
    if let Some(dir) = std::env::var_os("SIRIN_TCC_DIR") {
        return TccPaths::bundle(Path::new(&dir));
    }
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("tcc")))
        .filter(|dir| dir.join("include").join("tccdefs.h").is_file());
    if let Some(dir) = beside_exe {
        return TccPaths::bundle(&dir);
    }
    TccPaths {
        root: TCC_DIR.to_string(),
        win32: TCC_WIN32_DIR.to_string(),
        runtime: TCC_RUNTIME_DIR.to_string(),
    }
}
