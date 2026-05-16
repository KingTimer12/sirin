use clap::ArgMatches;
use chumsky::Parser;
use chumsky::input::Input as _;
use chumsky::span::SimpleSpan;
use sirin_codegen_c::emit::Emitter;
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

    let c_src = Emitter::new().emit_program(&stmts);

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
