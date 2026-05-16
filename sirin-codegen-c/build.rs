use std::path::PathBuf;

fn main() {
    let tcc_dir = PathBuf::from("vendor/tinycc")
        .canonicalize()
        .expect("vendor/tinycc not found");
    let win32_dir = tcc_dir.join("win32");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Always use forward slashes — backslashes in C #define strings are escape sequences
    let tcc_fwd = tcc_dir.display().to_string().replace('\\', "/");
    let win32_fwd = win32_dir.display().to_string().replace('\\', "/");

    // Bake absolute paths so the build command can use them at runtime
    println!("cargo:rustc-env=TCC_DIR={}", tcc_fwd);
    println!("cargo:rustc-env=TCC_WIN32_DIR={}", win32_fwd);

    // config.h for TCC — PE target for Windows executables
    // CONFIG_TCC_SYSINCLUDEPATHS: semicolon-separated on Windows.
    // Must include the root include/ dir (for tccdefs.h) AND win32/include/ (for C headers).
    let config_h = format!(
        r#"
#define TCC_VERSION "0.9.27"
#define CONFIG_TCCDIR "{win32}"
#define CONFIG_TCC_CRTPREFIX "{win32}/lib"
#define CONFIG_TCC_ELFINTERP ""
#define CONFIG_TCC_LIBPATHS "{win32}/lib"
#define CONFIG_TCC_SYSINCLUDEPATHS "{tcc}/include;{win32}/include"
#define HOST_OS "Windows"
#define HOST_ARCH "x86_64"
#define TCC_TARGET_X86_64 1
#define TCC_TARGET_PE 1
#define ONE_SOURCE 1
"#,
        tcc = tcc_fwd,
        win32 = win32_fwd
    );
    std::fs::write(out_dir.join("config.h"), config_h).unwrap();

    cc::Build::new()
        .compiler("clang")
        .file(tcc_dir.join("libtcc.c"))
        .include(&tcc_dir)
        .include(&out_dir)
        .define("ONE_SOURCE", None)
        .define("TCC_TARGET_X86_64", None)
        .define("TCC_TARGET_PE", None)
        .warnings(false)
        .compile("tcc");

    // Build libtcc1.a — TCC's PE linker requires this runtime helper archive.
    // Compile to object then archive with llvm-ar (available alongside clang).
    let libtcc1_obj = out_dir.join("libtcc1.o");
    let libtcc1_a = out_dir.join("libtcc1.a");
    let status = std::process::Command::new("clang")
        .args([
            "-c",
            "-O2",
            "-DTCC_TARGET_X86_64",
            "-DTCC_TARGET_PE",
            &format!("-I{}", tcc_fwd),
            &format!("-I{}", out_dir.display()),
            &tcc_dir.join("lib/libtcc1.c").display().to_string(),
            "-o",
            &libtcc1_obj.display().to_string(),
        ])
        .status()
        .expect("clang not found — needed to build libtcc1");
    assert!(status.success(), "failed to compile libtcc1.c");

    let ar_cmd = ["llvm-ar", "ar"]
        .into_iter()
        .find(|cmd| std::process::Command::new(cmd).arg("--version").output().is_ok())
        .expect("no ar tool found — install LLVM tools or binutils");
    let status = std::process::Command::new(ar_cmd)
        .args([
            "rcs",
            &libtcc1_a.display().to_string(),
            &libtcc1_obj.display().to_string(),
        ])
        .status()
        .unwrap_or_else(|e| panic!("{ar_cmd} failed to start: {e}"));
    assert!(status.success(), "failed to create libtcc1.a");

    let out_fwd = out_dir.display().to_string().replace('\\', "/");
    println!("cargo:rustc-env=TCC_RUNTIME_DIR={}", out_fwd);

    if cfg!(target_os = "windows") {
        println!("cargo:rustc-link-lib=oldnames");
    }

    println!("cargo:rerun-if-changed=vendor/tinycc/libtcc.c");
    println!("cargo:rerun-if-changed=vendor/tinycc/tcc.h");
    println!("cargo:rerun-if-changed=vendor/tinycc/lib/libtcc1.c");
    println!("cargo:rerun-if-changed=../sirin-runtime/sirin_runtime.h");
    println!("cargo:rerun-if-changed=../sirin-runtime/sirin_runtime.c");
}
