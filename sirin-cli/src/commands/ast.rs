use clap::ArgMatches;
use chumsky::Parser;
use chumsky::input::Input as _;
use chumsky::span::SimpleSpan;
use sirin_parser::parser::parser;

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

    match parser().parse(tokens.as_slice().split_token_span(eoi)).into_result() {
        Ok(ast) => println!("{:#?}", ast),
        Err(errors) => {
            for e in &errors {
                eprintln!("parse error: {:?}", e);
            }
            std::process::exit(1);
        }
    }
}
