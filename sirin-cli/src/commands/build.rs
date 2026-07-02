use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use clap::ArgMatches;
use chumsky::Parser;
use chumsky::input::Input as _;
use chumsky::span::SimpleSpan;
use sirin_codegen_c::emit::{Emitter, ModuleExports};
use sirin_codegen_c::runtime;
use sirin_parser::aliases::resolve_aliases;
use sirin_parser::parser::parser;
use sirin_parser::stmt::Stmt;
use sirin_parser::types::Type;
use sirin_typechecker::checker::Checker;

use crate::resolver::{ModuleSource, collect_modules, resolve};

#[cfg(windows)]
use sirin_codegen_c::{TCC_DIR, TCC_RUNTIME_DIR, TCC_WIN32_DIR};
#[cfg(windows)]
use sirin_codegen_c::tinycc::{TCC_OUTPUT_EXE, Tcc};

fn file_kb(path: &std::path::Path) -> Option<u64> {
    std::fs::metadata(path).ok().map(|m| (m.len() + 512) / 1024)
}

/// Result of a successful build: the produced binary and every `.sn` file that
/// went into it (main + transitive local modules) — the watch set for `--watch`.
pub struct BuildResult {
    pub out_path: PathBuf,
    pub sources: Vec<PathBuf>,
}

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
    let out_path = std::path::Path::new(path)
        .with_extension(if cfg!(windows) { "exe" } else { "" });
    let size_before = file_kb(&out_path);

    match try_build(path) {
        Ok(res) => {
            let size_after = file_kb(&res.out_path).unwrap_or(0);
            println!("{}", res.out_path.display());
            match size_before {
                Some(before) => println!("tamanho: {}KB → {}KB", before, size_after),
                None         => println!("tamanho: {}KB", size_after),
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

/// Full pipeline (parse → resolve modules → typecheck → emit C → cc) with no
/// process exits, so `run --watch` can keep going after a failed rebuild.
/// Diagnostics still print to stderr as they are found; the Err is a summary.
pub fn try_build(path: &str) -> Result<BuildResult, String> {
    if !path.ends_with(".sn") {
        return Err(format!("error: expected a `.sn` source file, got `{}`", path));
    }
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("error: cannot read `{}`: {}", path, e))?;

    let main_path = PathBuf::from(path)
        .canonicalize()
        .map_err(|e| format!("error: {}", e))?;

    // Parse main (tokens + stmts share scope — no lifetime escape)
    let tokens = sirin_parser::lex(&src);
    let eoi = SimpleSpan::from(src.len()..src.len());
    let mut main_stmts = match parser().parse(tokens.as_slice().split_token_span(eoi)).into_result() {
        Ok(s) => s,
        Err(errors) => {
            for e in &errors { eprintln!("parse error: {:?}", e); }
            return Err("build failed: parse error".to_string());
        }
    };

    // Type aliases (`type Name = ...`) are erased before checking/codegen; the map is
    // shared so a module's aliases are visible to files that `use` it.
    let mut alias_map: HashMap<String, Type> = HashMap::new();

    // ── Collect transitive local module dependencies ──────────────────────────
    let mut dep_stack: Vec<PathBuf> = vec![];
    let mut dep_visited: HashSet<PathBuf> = HashSet::new();
    let mut ordered: Vec<(PathBuf, String)> = vec![];

    for s in &main_stmts {
        if let Stmt::Use { path: use_path } = &s.node {
            let refs: Vec<&str> = use_path.iter().copied().collect();
            match resolve(&refs, &main_path) {
                Ok(ModuleSource::Local(dep_path)) => {
                    collect_modules(&dep_path, &mut dep_stack, &mut dep_visited, &mut ordered)
                        .map_err(|e| format!("error: {}", e))?;
                }
                Ok(ModuleSource::Stdlib) => {}
                Err(e) => return Err(format!("error: {}", e)),
            }
        }
    }

    // ── Process each module: typecheck + emit ─────────────────────────────────
    let mut checker = Checker::new(&src);
    let mut all_exports = ModuleExports {
        fns: Default::default(),
        classes: Default::default(),
        class_methods: Default::default(),
        enums: Default::default(),
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
        let mut mod_stmts = match parser()
            .parse(mod_tokens.as_slice().split_token_span(mod_eoi))
            .into_result()
        {
            Ok(s) => s,
            Err(errors) => {
                for e in &errors { eprintln!("parse error in {}: {:?}", mod_path.display(), e); }
                return Err(format!("build failed: parse error in {}", mod_path.display()));
            }
        };

        resolve_aliases(&mut mod_stmts, &mut alias_map);
        checker.import_module(&mod_stmts);

        let label = mod_path
            .strip_prefix(main_path.parent().unwrap_or(&main_path))
            .unwrap_or(mod_path)
            .display()
            .to_string();
        let mut mod_emitter = Emitter::new();
        mod_emitter.absorb_exports(&all_exports);
        let (mod_c, exports) = mod_emitter.emit_module(&mod_stmts);
        modules_c.push_str(&format!("/* === módulo: {} === */\n{}\n", label, mod_c));

        all_exports.fns.extend(exports.fns);
        all_exports.classes.extend(exports.classes);
        all_exports.class_methods.extend(exports.class_methods);
        all_exports.enums.extend(exports.enums);
        for (k, v) in exports.prim_methods {
            all_exports.prim_methods.entry(k).or_default().extend(v);
        }
        all_exports.used_types.extend(exports.used_types);
        all_exports.named_collection_decls.extend(exports.named_collection_decls);
        if exports.io_imported { all_exports.io_imported = true; }
    }

    // ── Typecheck main ────────────────────────────────────────────────────────
    // Resolve aliases now that every module's `type` declarations are in the map.
    resolve_aliases(&mut main_stmts, &mut alias_map);

    let mut had_error = false;
    for stmt in &main_stmts {
        if let Err(e) = checker.check_stmt(stmt) {
            eprintln!("type error: {:?}", e);
            had_error = true;
        }
    }
    if had_error {
        return Err("build failed: type error".to_string());
    }

    // ── Emit ──────────────────────────────────────────────────────────────────
    let out_path = std::path::Path::new(path)
        .with_extension(if cfg!(windows) { "exe" } else { "" });

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
    compile_unix(&c_src, &defines_prefix, &out_str, net_imported)?;

    #[cfg(windows)]
    compile_windows(&c_src, &defines_prefix, &out_str, net_imported)?;

    let mut sources = vec![main_path];
    sources.extend(ordered.iter().map(|(p, _)| p.clone()));
    Ok(BuildResult { out_path, sources })
}

#[cfg(not(windows))]
fn compile_unix(c_src: &str, defines_prefix: &str, out: &str, net_imported: bool) -> Result<(), String> {
    let tmp = std::env::temp_dir();
    let h_path    = tmp.join("sirin_runtime.h");
    let c_rt      = tmp.join("sirin_runtime.c");
    let c_async_h = tmp.join("sirin_async.h");
    let c_async   = tmp.join("sirin_async.c");
    let c_net_h   = tmp.join("sirin_net.h");
    let c_net     = tmp.join("sirin_net.c");
    let c_prog    = tmp.join("sirin_program.c");

    std::fs::write(&h_path, runtime::RUNTIME_H)
        .map_err(|e| format!("error: cannot write runtime header: {}", e))?;
    let runtime_with_defines = format!("{}{}", defines_prefix, runtime::RUNTIME_C);
    std::fs::write(&c_rt, &runtime_with_defines)
        .map_err(|e| format!("error: cannot write runtime source: {}", e))?;
    std::fs::write(&c_async_h, runtime::ASYNC_H)
        .map_err(|e| format!("error: cannot write async header: {}", e))?;
    std::fs::write(&c_async, runtime::ASYNC_C)
        .map_err(|e| format!("error: cannot write async source: {}", e))?;
    if net_imported {
        std::fs::write(&c_net_h, runtime::NET_H)
            .map_err(|e| format!("error: cannot write net header: {}", e))?;
        std::fs::write(&c_net, runtime::NET_C)
            .map_err(|e| format!("error: cannot write net source: {}", e))?;
    }
    std::fs::write(&c_prog, c_src)
        .map_err(|e| format!("error: cannot write program source: {}", e))?;

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
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("compile error: compiler exited with {}", s)),
        Err(e) => Err(format!("compile error: cannot run compiler: {}", e)),
    }
}

#[cfg(windows)]
fn compile_windows(c_src: &str, defines_prefix: &str, out: &str, _net_imported: bool) -> Result<(), String> {
    let tcc = Tcc::new().map_err(|e| format!("error: {}", e))?;

    tcc.set_lib_path(TCC_WIN32_DIR);
    tcc.add_library_path(TCC_RUNTIME_DIR).map_err(|e| format!("tcc error: {}", e))?;
    tcc.add_include_path(&format!("{}/include", TCC_DIR)).map_err(|e| format!("tcc error: {}", e))?;
    tcc.add_include_path(&format!("{}/include", TCC_WIN32_DIR)).map_err(|e| format!("tcc error: {}", e))?;
    tcc.add_include_path(&format!("{}/include/winapi", TCC_WIN32_DIR)).map_err(|e| format!("tcc error: {}", e))?;

    let tmp = std::env::temp_dir();
    std::fs::write(tmp.join("sirin_runtime.h"), runtime::RUNTIME_H)
        .map_err(|e| format!("error: cannot write runtime header to temp: {}", e))?;
    let tmp_fwd = tmp.to_string_lossy().replace('\\', "/");
    tcc.add_include_path(&tmp_fwd).map_err(|e| format!("tcc error: {}", e))?;

    tcc.set_output_type(TCC_OUTPUT_EXE).map_err(|e| format!("tcc error: {}", e))?;
    tcc.set_options("-s").map_err(|e| format!("tcc error: {}", e))?;
    tcc.set_options("-Os").map_err(|e| format!("tcc error: {}", e))?;

    let crt1 = format!("{}/lib/crt1.c", TCC_WIN32_DIR);
    tcc.add_file(&crt1).map_err(|e| format!("tcc error (crt1): {}", e))?;

    let runtime_with_defines = format!("{}{}", defines_prefix, runtime::RUNTIME_C);
    tcc.compile_string(&runtime_with_defines).map_err(|e| format!("runtime compile error:\n{}", e))?;
    tcc.compile_string(c_src).map_err(|e| format!("compile error:\n{}", e))?;
    tcc.output_file(out).map_err(|e| format!("link error: {}", e))?;
    Ok(())
}
