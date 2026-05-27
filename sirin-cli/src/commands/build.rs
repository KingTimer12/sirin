use std::collections::HashSet;
use std::path::PathBuf;

use clap::ArgMatches;
use chumsky::Parser;
use chumsky::input::Input as _;
use chumsky::span::SimpleSpan;
use sirin_codegen_c::emit::{Emitter, ModuleExports};
use sirin_codegen_c::runtime;
use sirin_parser::parser::parser;
use sirin_parser::stmt::Stmt;
use sirin_typechecker::checker::Checker;

use crate::resolver::{ModuleSource, collect_modules, resolve};

#[cfg(windows)]
use sirin_codegen_c::{TCC_DIR, TCC_RUNTIME_DIR, TCC_WIN32_DIR};
#[cfg(windows)]
use sirin_codegen_c::tinycc::{TCC_OUTPUT_EXE, Tcc};

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

    let main_path = match PathBuf::from(path).canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    // Parse main (tokens + stmts share scope — no lifetime escape)
    let tokens = sirin_parser::lex(&src);
    let eoi = SimpleSpan::from(src.len()..src.len());
    let main_stmts = match parser().parse(tokens.as_slice().split_token_span(eoi)).into_result() {
        Ok(s) => s,
        Err(errors) => {
            for e in &errors { eprintln!("parse error: {:?}", e); }
            std::process::exit(1);
        }
    };

    // ── Collect transitive local module dependencies ──────────────────────────
    let mut dep_stack: Vec<PathBuf> = vec![];
    let mut dep_visited: HashSet<PathBuf> = HashSet::new();
    let mut ordered: Vec<(PathBuf, String)> = vec![];

    for s in &main_stmts {
        if let Stmt::Use { path: use_path } = &s.node {
            let refs: Vec<&str> = use_path.iter().copied().collect();
            match resolve(&refs, &main_path) {
                Ok(ModuleSource::Local(dep_path)) => {
                    if let Err(e) = collect_modules(
                        &dep_path, &mut dep_stack, &mut dep_visited, &mut ordered,
                    ) {
                        eprintln!("error: {}", e);
                        std::process::exit(1);
                    }
                }
                Ok(ModuleSource::Stdlib) => {}
                Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
            }
        }
    }

    // ── Process each module: typecheck + emit ─────────────────────────────────
    let mut checker = Checker::new(&src);
    let mut all_exports = ModuleExports {
        fns: Default::default(),
        classes: Default::default(),
        class_methods: Default::default(),
        prim_methods: Default::default(),
        used_types: Default::default(),
        io_imported: false,
        named_collection_decls: Default::default(),
    };
    let mut modules_c = String::new();

    for (mod_path, mod_src) in &ordered {
        // tokens + stmts live only for this iteration
        let mod_tokens = sirin_parser::lex(mod_src.as_str());
        let mod_eoi = SimpleSpan::from(mod_src.len()..mod_src.len());
        let mod_stmts = match parser()
            .parse(mod_tokens.as_slice().split_token_span(mod_eoi))
            .into_result()
        {
            Ok(s) => s,
            Err(errors) => {
                for e in &errors { eprintln!("parse error in {}: {:?}", mod_path.display(), e); }
                std::process::exit(1);
            }
        };

        checker.import_module(&mod_stmts);

        let label = mod_path
            .strip_prefix(main_path.parent().unwrap_or(&main_path))
            .unwrap_or(mod_path)
            .display()
            .to_string();
        let (mod_c, exports) = Emitter::new().emit_module(&mod_stmts);
        modules_c.push_str(&format!("/* === módulo: {} === */\n{}\n", label, mod_c));

        all_exports.fns.extend(exports.fns);
        all_exports.classes.extend(exports.classes);
        all_exports.class_methods.extend(exports.class_methods);
        for (k, v) in exports.prim_methods {
            all_exports.prim_methods.entry(k).or_default().extend(v);
        }
        all_exports.used_types.extend(exports.used_types);
        if exports.io_imported { all_exports.io_imported = true; }
    }

    // ── Typecheck main ────────────────────────────────────────────────────────
    let mut had_error = false;
    for stmt in &main_stmts {
        if let Err(e) = checker.check_stmt(stmt) {
            eprintln!("type error: {:?}", e);
            had_error = true;
        }
    }
    if had_error { std::process::exit(1); }

    // ── Emit ──────────────────────────────────────────────────────────────────
    let out_path = std::path::Path::new(path)
        .with_extension(if cfg!(windows) { "exe" } else { "" });
    let size_before = file_kb(&out_path);

    let mut main_emitter = Emitter::new();
    main_emitter.absorb_exports(&all_exports);
    let (main_body, defines_prefix, io_imported, async_imported, net_imported, named_decls) =
        main_emitter.emit_body_and_prefix(&main_stmts);

    let io_include    = if io_imported    { "#include <stdio.h>\n" } else { "" };
    let async_include = if async_imported { "#include \"sirin_async.h\"\n#include <stdlib.h>\n" } else { "" };
    let net_include   = if net_imported   { "#include \"sirin_net.h\"\n" } else { "" };
    let c_src = if modules_c.is_empty() {
        format!(
            "{}#include \"sirin_runtime.h\"\n{}{}{}{}\n{}",
            defines_prefix, async_include, net_include, io_include, named_decls, main_body
        )
    } else {
        format!(
            "{}#include \"sirin_runtime.h\"\n{}{}{}{}\n{}{}",
            defines_prefix, async_include, net_include, io_include, named_decls, modules_c, main_body
        )
    };

    let out_str = out_path.to_string_lossy().into_owned();

    #[cfg(not(windows))]
    compile_unix(&c_src, &defines_prefix, &out_str, net_imported);

    #[cfg(windows)]
    compile_windows(&c_src, &defines_prefix, &out_str, net_imported);

    let size_after = file_kb(&out_path).unwrap_or(0);
    println!("{}", out_str);
    match size_before {
        Some(before) => println!("tamanho: {}KB → {}KB", before, size_after),
        None         => println!("tamanho: {}KB", size_after),
    }
}

#[cfg(not(windows))]
fn compile_unix(c_src: &str, defines_prefix: &str, out: &str, net_imported: bool) {
    let tmp = std::env::temp_dir();
    let h_path    = tmp.join("sirin_runtime.h");
    let c_rt      = tmp.join("sirin_runtime.c");
    let c_async_h = tmp.join("sirin_async.h");
    let c_async   = tmp.join("sirin_async.c");
    let c_net_h   = tmp.join("sirin_net.h");
    let c_net     = tmp.join("sirin_net.c");
    let c_prog    = tmp.join("sirin_program.c");

    if let Err(e) = std::fs::write(&h_path, runtime::RUNTIME_H) {
        eprintln!("error: cannot write runtime header: {}", e);
        std::process::exit(1);
    }
    let runtime_with_defines = format!("{}{}", defines_prefix, runtime::RUNTIME_C);
    if let Err(e) = std::fs::write(&c_rt, &runtime_with_defines) {
        eprintln!("error: cannot write runtime source: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = std::fs::write(&c_async_h, runtime::ASYNC_H) {
        eprintln!("error: cannot write async header: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = std::fs::write(&c_async, runtime::ASYNC_C) {
        eprintln!("error: cannot write async source: {}", e);
        std::process::exit(1);
    }
    if net_imported {
        if let Err(e) = std::fs::write(&c_net_h, runtime::NET_H) {
            eprintln!("error: cannot write net header: {}", e);
            std::process::exit(1);
        }
        if let Err(e) = std::fs::write(&c_net, runtime::NET_C) {
            eprintln!("error: cannot write net source: {}", e);
            std::process::exit(1);
        }
    }
    if let Err(e) = std::fs::write(&c_prog, c_src) {
        eprintln!("error: cannot write program source: {}", e);
        std::process::exit(1);
    }

    let compiler = ["cc", "clang", "gcc"]
        .iter()
        .find(|&&cmd| std::process::Command::new(cmd).arg("--version").output().is_ok())
        .copied()
        .unwrap_or("cc");

    let mut cmd_args: Vec<String> = vec![
        c_rt.to_str().unwrap().to_owned(),
        c_async.to_str().unwrap().to_owned(),
    ];
    if net_imported {
        cmd_args.push(c_net.to_str().unwrap().to_owned());
    }
    cmd_args.push(c_prog.to_str().unwrap().to_owned());
    cmd_args.extend_from_slice(&[
        "-I".to_owned(), tmp.to_str().unwrap().to_owned(),
        "-o".to_owned(), out.to_owned(),
        "-O2".to_owned(),
        "-Wno-deprecated-declarations".to_owned(),
    ]);

    let status = std::process::Command::new(compiler)
        .args(&cmd_args)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => { eprintln!("compile error: compiler exited with {}", s); std::process::exit(1); }
        Err(e) => { eprintln!("compile error: cannot run compiler: {}", e); std::process::exit(1); }
    }
}

#[cfg(windows)]
fn compile_windows(c_src: &str, defines_prefix: &str, out: &str, _net_imported: bool) {
    let tcc = match Tcc::new() {
        Ok(t) => t,
        Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
    };

    tcc.set_lib_path(TCC_WIN32_DIR);
    if let Err(e) = tcc.add_library_path(TCC_RUNTIME_DIR) {
        eprintln!("tcc error: {}", e); std::process::exit(1);
    }
    if let Err(e) = tcc.add_include_path(&format!("{}/include", TCC_DIR)) {
        eprintln!("tcc error: {}", e); std::process::exit(1);
    }
    if let Err(e) = tcc.add_include_path(&format!("{}/include", TCC_WIN32_DIR)) {
        eprintln!("tcc error: {}", e); std::process::exit(1);
    }
    if let Err(e) = tcc.add_include_path(&format!("{}/include/winapi", TCC_WIN32_DIR)) {
        eprintln!("tcc error: {}", e); std::process::exit(1);
    }

    let tmp = std::env::temp_dir();
    if let Err(e) = std::fs::write(tmp.join("sirin_runtime.h"), runtime::RUNTIME_H) {
        eprintln!("error: cannot write runtime header to temp: {}", e);
        std::process::exit(1);
    }
    let tmp_fwd = tmp.to_string_lossy().replace('\\', "/");
    if let Err(e) = tcc.add_include_path(&tmp_fwd) {
        eprintln!("tcc error: {}", e); std::process::exit(1);
    }

    if let Err(e) = tcc.set_output_type(TCC_OUTPUT_EXE) {
        eprintln!("tcc error: {}", e); std::process::exit(1);
    }
    if let Err(e) = tcc.set_options("-s") {
        eprintln!("tcc error: {}", e); std::process::exit(1);
    }
    if let Err(e) = tcc.set_options("-Os") {
        eprintln!("tcc error: {}", e); std::process::exit(1);
    }

    let crt1 = format!("{}/lib/crt1.c", TCC_WIN32_DIR);
    if let Err(e) = tcc.add_file(&crt1) {
        eprintln!("tcc error (crt1): {}", e); std::process::exit(1);
    }

    let runtime_with_defines = format!("{}{}", defines_prefix, runtime::RUNTIME_C);
    if let Err(e) = tcc.compile_string(&runtime_with_defines) {
        eprintln!("runtime compile error:\n{}", e); std::process::exit(1);
    }
    if let Err(e) = tcc.compile_string(c_src) {
        eprintln!("compile error:\n{}", e); std::process::exit(1);
    }
    if let Err(e) = tcc.output_file(out) {
        eprintln!("link error: {}", e); std::process::exit(1);
    }
}
