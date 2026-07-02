use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use clap::ArgMatches;
use chumsky::Parser;
use chumsky::input::Input as _;
use chumsky::span::SimpleSpan;
use sirin_codegen_c::emit::{Emitter, ModuleExports};
use sirin_parser::aliases::resolve_aliases;
use sirin_parser::parser::parser;
use sirin_parser::stmt::Stmt;
use sirin_parser::types::Type;
use sirin_typechecker::checker::Checker;

use crate::resolver::{ModuleSource, collect_modules, resolve};

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
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

    // Parse main
    let tokens = sirin_parser::lex(&src);
    let eoi = SimpleSpan::from(src.len()..src.len());
    let mut main_stmts = match parser().parse(tokens.as_slice().split_token_span(eoi)).into_result() {
        Ok(s) => s,
        Err(errors) => {
            for e in &errors { eprintln!("parse error: {:?}", e); }
            std::process::exit(1);
        }
    };

    // Type aliases are erased before checking/codegen; the map is shared across units.
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

    // ── Process modules ───────────────────────────────────────────────────────
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
        let mod_tokens = sirin_parser::lex(mod_src.as_str());
        let mod_eoi = SimpleSpan::from(mod_src.len()..mod_src.len());
        let mut mod_stmts = match parser()
            .parse(mod_tokens.as_slice().split_token_span(mod_eoi))
            .into_result()
        {
            Ok(s) => s,
            Err(errors) => {
                for e in &errors { eprintln!("parse error in {}: {:?}", mod_path.display(), e); }
                std::process::exit(1);
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
        for (k, v) in exports.prim_methods {
            all_exports.prim_methods.entry(k).or_default().extend(v);
        }
        all_exports.used_types.extend(exports.used_types);
        all_exports.named_collection_decls.extend(exports.named_collection_decls);
        if exports.io_imported { all_exports.io_imported = true; }
    }

    // ── Typecheck main ────────────────────────────────────────────────────────
    resolve_aliases(&mut main_stmts, &mut alias_map);

    let mut had_error = false;
    for stmt in &main_stmts {
        if let Err(e) = checker.check_stmt(stmt) {
            eprintln!("type error: {:?}", e);
            had_error = true;
        }
    }
    if had_error { std::process::exit(1); }

    // ── Emit combined C ───────────────────────────────────────────────────────
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

    let out_path = std::path::Path::new(path).with_extension("c");
    if let Err(e) = std::fs::write(&out_path, &c_src) {
        eprintln!("error: cannot write `{}`: {}", out_path.display(), e);
        std::process::exit(1);
    }

    let out_dir = out_path.parent().unwrap_or(std::path::Path::new("."));
    if let Err(e) = sirin_codegen_c::runtime::write_runtime(out_dir) {
        eprintln!("warning: {}", e);
    }

    println!("{}", out_path.display());
}
