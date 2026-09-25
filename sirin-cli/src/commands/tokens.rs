use clap::ArgMatches;
use logos::Logos;
use sirin_diagnostics::eprint_all;
use sirin_lexer::token::Tokens;

use crate::diag::read_source;

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
    let src = read_source(path).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });

    for (token, span) in Tokens::lexer(&src).spanned() {
        match token {
            Ok(Tokens::Whitespace) | Err(_) => {}
            Ok(tok) => println!("{:?}  {:?}", span, tok),
        }
    }

    let errors = sirin_parser::lex_diagnostics(&src);
    if !errors.is_empty() {
        eprint_all(&errors, path, &src);
        std::process::exit(1);
    }
}
