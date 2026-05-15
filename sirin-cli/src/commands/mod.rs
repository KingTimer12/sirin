use clap::ArgMatches;

mod ast;
mod checker;
mod tokens;

pub fn register_commands(args: Option<(&str, &ArgMatches)>) {
    match args {
        Some(("check", m)) => checker::execute(m),
        Some(("tokens", m)) => tokens::execute(m),
        Some(("ast", m)) => ast::execute(m),
        _ => eprintln!("unknown command"),
    }
}
