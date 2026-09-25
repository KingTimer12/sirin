use clap::ArgMatches;

use crate::diag::{parse_or_report, read_source};

pub fn execute(matches: &ArgMatches) {
    let path = matches.get_one::<String>("file").unwrap();
    let src = read_source(path).unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });

    let tokens = sirin_parser::lex(&src);
    match parse_or_report(path, &src, &tokens) {
        Some(ast) => println!("{:#?}", ast),
        None => std::process::exit(1),
    }
}
