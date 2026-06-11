use std::collections::HashMap;

use clap::ArgMatches;
use chumsky::Parser;
use chumsky::input::Input as _;
use chumsky::span::SimpleSpan;
use sirin_parser::aliases::resolve_aliases;
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

    let mut stmts = match parser().parse(tokens.as_slice().split_token_span(eoi)).into_result() {
        Ok(s) => s,
        Err(errors) => {
            for e in &errors {
                eprintln!("parse error: {:?}", e);
            }
            std::process::exit(1);
        }
    };

    let mut alias_map = HashMap::new();
    resolve_aliases(&mut stmts, &mut alias_map);

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
    } else {
        println!("ok");
    }
}
