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

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
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

    // emit_program generates #include "sirin_runtime.h" at top
    let c_src = Emitter::new().emit_program(&stmts);

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
    // in both sirin_runtime.c and the generated program.
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

    // CRT startup — provides _start / mainCRTStartup which calls main()
    let crt1 = format!("{}/lib/crt1.c", TCC_WIN32_DIR);
    if let Err(e) = tcc.add_file(&crt1) {
        eprintln!("tcc error (crt1): {}", e);
        std::process::exit(1);
    }

    // Compile the sirin runtime (embedded via include_str! at build time)
    if let Err(e) = tcc.compile_string(runtime::RUNTIME_C) {
        eprintln!("runtime compile error:\n{}", e);
        std::process::exit(1);
    }

    // Compile the generated program
    if let Err(e) = tcc.compile_string(&c_src) {
        eprintln!("compile error:\n{}", e);
        std::process::exit(1);
    }

    let out_path = std::path::Path::new(path)
        .with_extension(if cfg!(windows) { "exe" } else { "" });
    let out_str = out_path.to_string_lossy();

    if let Err(e) = tcc.output_file(&out_str) {
        eprintln!("link error: {}", e);
        std::process::exit(1);
    }

    println!("{}", out_str);
}
