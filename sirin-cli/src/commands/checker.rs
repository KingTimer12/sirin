use std::collections::HashMap;

use clap::ArgMatches;
use sirin_parser::aliases::resolve_aliases;
use sirin_typechecker::checker::Checker;

use crate::diag::{check_or_report, parse_or_report, read_source, summary};

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
    let src = read_source(path).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });

    let tokens = sirin_parser::lex(&src);
    let Some(mut stmts) = parse_or_report(path, &src, &tokens) else {
        std::process::exit(1);
    };

    let mut alias_map = HashMap::new();
    resolve_aliases(&mut stmts, &mut alias_map);

    let mut checker = Checker::new(&src);
    let errors = check_or_report(&mut checker, path, &src, &stmts);
    if errors > 0 {
        eprintln!("{}", summary(path, errors));
        std::process::exit(1);
    }
    println!("ok: no errors found in `{}`", path);
}
