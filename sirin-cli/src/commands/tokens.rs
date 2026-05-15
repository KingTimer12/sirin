use clap::ArgMatches;
use logos::Logos;
use sirin_lexer::token::Tokens;

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", path, e);
            std::process::exit(1);
        }
    };

    let mut had_error = false;
    for (token, span) in Tokens::lexer(&src).spanned() {
        match token {
            Ok(Tokens::Whitespace) => {}
            Ok(tok) => println!("{:?}  {:?}", span, tok),
            Err(e) => {
                eprintln!("lex error at {:?}: {:?}", span, e);
                had_error = true;
            }
        }
    }

    if had_error {
        std::process::exit(1);
    }
}
