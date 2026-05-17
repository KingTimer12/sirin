use clap::ArgMatches;
use chumsky::Parser;
use chumsky::input::Input as _;
use chumsky::span::SimpleSpan;
use sirin_codegen_c::{TCC_DIR, TCC_RUNTIME_DIR, TCC_WIN32_DIR};
use sirin_codegen_c::emit::Emitter;
use sirin_codegen_c::runtime;
use sirin_codegen_c::tinycc::{TCC_OUTPUT_EXE, Tcc};
use sirin_parser::parser::parser;
use sirin_typechecker::checker::Checker;

fn file_kb(path: &std::path::Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| (m.len() + 512) / 1024)
}

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
    if !path.ends_with(".sn") {
        eprintln!("error: expected a `.sn` source file, got `{}`", path);
        std::process::exit(1);
    }
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path, e);
            std::process::exit(1);
        }
    };

    let tokens = sirin_parser::lex(&src);
    let eoi = SimpleSpan::from(src.len()..src.len());

    let stmts = match parser().parse(tokens.as_slice().split_token_span(eoi)).into_result() {
        Ok(s) => s,
        Err(errors) => {
            for e in &errors {
                eprintln!("parse error: {:?}", e);
            }
            std::process::exit(1);
        }
    };

    let mut checker = Checker::new(&src);
    let mut had_error = false;
    for stmt in &stmts {
        if let Err(e) = checker.check_stmt(stmt) {
            eprintln!("type error: {:?}", e);
            had_error = true;
        }
    }

    if had_error {
        std::process::exit(1);
    }

    let out_path = std::path::Path::new(path)
        .with_extension(if cfg!(windows) { "exe" } else { "" });

    // Size of previous build (if any) for the "antes" report
    let size_before = file_kb(&out_path);

    // emit_program_and_prefix returns (program_src_with_include, defines_prefix)
    // defines_prefix must be prepended to the runtime source so #ifdef guards activate
    let (c_src, defines_prefix) = Emitter::new().emit_program_and_prefix(&stmts);

    let tcc = match Tcc::new() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    tcc.set_lib_path(TCC_WIN32_DIR);

    if let Err(e) = tcc.add_library_path(TCC_RUNTIME_DIR) {
        eprintln!("tcc error: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = tcc.add_include_path(&format!("{}/include", TCC_DIR)) {
        eprintln!("tcc error: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = tcc.add_include_path(&format!("{}/include", TCC_WIN32_DIR)) {
        eprintln!("tcc error: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = tcc.add_include_path(&format!("{}/include/winapi", TCC_WIN32_DIR)) {
        eprintln!("tcc error: {}", e);
        std::process::exit(1);
    }

    // Write sirin_runtime.h to temp dir so TCC can resolve #include "sirin_runtime.h"
    let tmp = std::env::temp_dir();
    if let Err(e) = std::fs::write(tmp.join("sirin_runtime.h"), runtime::RUNTIME_H) {
        eprintln!("error: cannot write runtime header to temp: {}", e);
        std::process::exit(1);
    }
    let tmp_fwd = tmp.to_string_lossy().replace('\\', "/");
    if let Err(e) = tcc.add_include_path(&tmp_fwd) {
        eprintln!("tcc error: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = tcc.set_output_type(TCC_OUTPUT_EXE) {
        eprintln!("tcc error: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = tcc.set_options("-s") {
        eprintln!("tcc error: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = tcc.set_options("-Os") {
        eprintln!("tcc error: {}", e);
        std::process::exit(1);
    }

    // CRT startup — provides _start / mainCRTStartup which calls main()
    let crt1 = format!("{}/lib/crt1.c", TCC_WIN32_DIR);
    if let Err(e) = tcc.add_file(&crt1) {
        eprintln!("tcc error (crt1): {}", e);
        std::process::exit(1);
    }

    // Compile the sirin runtime with conditional-type defines prepended
    let runtime_with_defines = format!("{}{}", defines_prefix, runtime::RUNTIME_C);
    if let Err(e) = tcc.compile_string(&runtime_with_defines) {
        eprintln!("runtime compile error:\n{}", e);
        std::process::exit(1);
    }

    // Compile the generated program (already has defines at top)
    if let Err(e) = tcc.compile_string(&c_src) {
        eprintln!("compile error:\n{}", e);
        std::process::exit(1);
    }

    let out_str = out_path.to_string_lossy();

    if let Err(e) = tcc.output_file(&out_str) {
        eprintln!("link error: {}", e);
        std::process::exit(1);
    }

    let size_after = file_kb(&out_path).unwrap_or(0);

    println!("{}", out_str);

    match size_before {
        Some(before) => println!("tamanho: {}KB → {}KB", before, size_after),
        None         => println!("tamanho: {}KB", size_after),
    }
}
