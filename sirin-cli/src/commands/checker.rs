use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use clap::ArgMatches;
use sirin_parser::aliases::resolve_aliases;
use sirin_parser::stmt::Stmt;
use sirin_typechecker::checker::Checker;

use crate::diag::{check_or_report, parse_or_report, read_source, summary};
use crate::resolver::{ModuleSource, collect_modules, resolve};

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
    if let Err(e) = check(path) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
    println!("ok: no errors found in `{}`", path);
}

fn check(path: &str) -> Result<(), String> {
    let src = read_source(path)?;
    let main_path = PathBuf::from(path)
        .canonicalize()
        .map_err(|e| format!("error: {}", e))?;

    let tokens = sirin_parser::lex(&src);
    let mut stmts = parse_or_report(path, &src, &tokens)
        .ok_or_else(|| format!("error: could not compile `{}` due to syntax errors", path))?;

    // Local modules (`use http`), deepest dependencies first, as `build` does.
    let mut stack = vec![];
    let mut visited = HashSet::new();
    let mut ordered: Vec<(PathBuf, String)> = vec![];
    for s in &stmts {
        if let Stmt::Use { path: use_path } = &s.node {
            let refs: Vec<&str> = use_path.iter().copied().collect();
            match resolve(&refs, &main_path).map_err(|e| format!("error: {}", e))? {
                ModuleSource::Local(dep) => collect_modules(&dep, &mut stack, &mut visited, &mut ordered)
                    .map_err(|e| format!("error: {}", e))?,
                ModuleSource::Stdlib => {}
            }
        }
    }

    let mut alias_map = HashMap::new();
    let mut checker = Checker::new(&src);
    for (mod_path, mod_src) in &ordered {
        let mod_name = mod_path.display().to_string();
        let mod_tokens = sirin_parser::lex(mod_src);
        let mut mod_stmts = parse_or_report(&mod_name, mod_src, &mod_tokens).ok_or_else(|| {
            format!("error: could not compile module `{}` due to syntax errors", mod_name)
        })?;
        resolve_aliases(&mut mod_stmts, &mut alias_map);
        checker.import_module(&mod_stmts);
    }

    resolve_aliases(&mut stmts, &mut alias_map);
    let errors = check_or_report(&mut checker, path, &src, &stmts);
    if errors > 0 {
        return Err(summary(path, errors));
    }
    Ok(())
}
