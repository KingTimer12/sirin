//! Source → C: parse the main file, resolve its local modules, type-check,
//! and emit one C program. Shared by `build`, `run` and `emit-c`.
//!
//! Nothing here exits the process, so `run --watch` can keep going after a
//! failed rebuild. Diagnostics print to stderr as they are found; the `Err`
//! is the closing summary line.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use sirin_codegen_c::emit::{CProgram, Emitter, ModuleExports};
use sirin_parser::aliases::resolve_aliases;
use sirin_parser::span::Spanned;
use sirin_parser::stmt::Stmt;
use sirin_parser::types::Type;
use sirin_typechecker::checker::Checker;

use crate::diag::{check_or_report, parse_or_report, read_source, summary};
use crate::resolver::{ModuleSource, collect_modules, resolve};

/// A program ready for the C compiler.
pub struct CompiledProgram {
    pub c: CProgram,
    /// Every `.sn` file that went into it: main first, then its modules.
    pub sources: Vec<PathBuf>,
}

pub fn compile_to_c(path: &str) -> Result<CompiledProgram, String> {
    if !path.ends_with(".sn") {
        return Err(format!("error: expected a `.sn` source file, got `{}`", path));
    }
    let src = read_source(path)?;
    let main_path = PathBuf::from(path)
        .canonicalize()
        .map_err(|e| format!("error: {}", e))?;

    let tokens = sirin_parser::lex(&src);
    let mut main_stmts = parse_or_report(path, &src, &tokens)
        .ok_or_else(|| format!("error: could not compile `{}` due to syntax errors", path))?;

    // Type aliases (`type Name = ...`) are erased before checking and codegen.
    // The map is shared so a module's aliases are visible to files that `use` it.
    let mut alias_map: HashMap<String, Type> = HashMap::new();
    let mut checker = Checker::new(&src);

    let modules = local_modules(&main_stmts, &main_path)?;
    let emitted = emit_modules(&modules, &main_path, &mut checker, &mut alias_map)?;

    resolve_aliases(&mut main_stmts, &mut alias_map);
    let errors = check_or_report(&mut checker, path, &src, &main_stmts);
    if errors > 0 {
        return Err(summary(path, errors));
    }

    let mut emitter = Emitter::new();
    emitter.absorb_exports(&emitted.exports);
    let c = emitter.emit_program(&main_stmts, &emitted.c);

    let mut sources = vec![main_path];
    sources.extend(modules.into_iter().map(|(p, _)| p));
    Ok(CompiledProgram { c, sources })
}

/// The local modules `stmts` uses, transitively, deepest dependencies first.
pub fn local_modules(stmts: &[Spanned<Stmt<'_>>], main_path: &Path)
    -> Result<Vec<(PathBuf, String)>, String>
{
    let mut stack = vec![];
    let mut visited = HashSet::new();
    let mut ordered = vec![];
    for s in stmts {
        if let Stmt::Use { path: use_path } = &s.node {
            let refs: Vec<&str> = use_path.iter().copied().collect();
            match resolve(&refs, main_path).map_err(|e| format!("error: {}", e))? {
                ModuleSource::Local(dep) => collect_modules(&dep, &mut stack, &mut visited, &mut ordered)
                    .map_err(|e| format!("error: {}", e))?,
                ModuleSource::Stdlib => {}
            }
        }
    }
    Ok(ordered)
}

/// C code for all modules, and the symbols they export to the main file.
struct EmittedModules {
    c: String,
    exports: ModuleExports,
}

fn emit_modules(
    modules: &[(PathBuf, String)],
    main_path: &Path,
    checker: &mut Checker<'_>,
    alias_map: &mut HashMap<String, Type>,
) -> Result<EmittedModules, String> {
    let mut exports = ModuleExports::default();
    let mut c = String::new();
    for_each_module(modules, checker, alias_map, |mod_path, stmts| {
        let label = mod_path
            .strip_prefix(main_path.parent().unwrap_or(main_path))
            .unwrap_or(mod_path)
            .display()
            .to_string();
        let mut emitter = Emitter::new();
        emitter.absorb_exports(&exports);
        let (mod_c, mod_exports) = emitter.emit_module(stmts);
        c.push_str(&format!("/* === module: {} === */\n{}\n", label, mod_c));
        exports.merge(mod_exports);
    })?;
    Ok(EmittedModules { c, exports })
}

/// Parse each module in order, teach the checker its signatures, then hand
/// its statements to `f`.
pub fn for_each_module(
    modules: &[(PathBuf, String)],
    checker: &mut Checker<'_>,
    alias_map: &mut HashMap<String, Type>,
    mut f: impl FnMut(&Path, &[Spanned<Stmt<'_>>]),
) -> Result<(), String> {
    for (mod_path, mod_src) in modules {
        let mod_name = mod_path.display().to_string();
        let mod_tokens = sirin_parser::lex(mod_src);
        let mut mod_stmts = parse_or_report(&mod_name, mod_src, &mod_tokens).ok_or_else(|| {
            format!("error: could not compile module `{}` due to syntax errors", mod_name)
        })?;
        resolve_aliases(&mut mod_stmts, alias_map);
        checker.import_module(&mod_stmts);
        f(mod_path, &mod_stmts);
    }
    Ok(())
}
