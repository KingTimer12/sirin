use std::collections::HashMap;
use std::path::PathBuf;

use clap::ArgMatches;
use sirin_parser::aliases::resolve_aliases;
use sirin_typechecker::checker::Checker;

use crate::diag::{check_or_report, parse_or_report, read_source, summary};
use crate::pipeline::{for_each_module, local_modules};

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

    // Local modules (`use http`) are loaded as `build` does, so their names resolve.
    let mut alias_map = HashMap::new();
    let mut checker = Checker::new(&src);
    let modules = local_modules(&stmts, &main_path)?;
    for_each_module(&modules, &mut checker, &mut alias_map, |_, _| {})?;

    resolve_aliases(&mut stmts, &mut alias_map);
    let errors = check_or_report(&mut checker, path, &src, &stmts);
    if errors > 0 {
        return Err(summary(path, errors));
    }
    Ok(())
}
