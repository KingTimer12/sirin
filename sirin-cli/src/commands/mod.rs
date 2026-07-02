use clap::ArgMatches;

mod ast;
mod build;
mod checker;
mod emit_c;
mod run;
mod tokens;

pub fn register_commands(args: Option<(&str, &ArgMatches)>) {
    match args {
        Some(("check", m)) => checker::execute(m),
        Some(("tokens", m)) => tokens::execute(m),
        Some(("ast", m)) => ast::execute(m),
        Some(("emit-c", m)) => emit_c::execute(m),
        Some(("build", m)) => build::execute(m),
        Some(("run", m)) => run::execute(m),
        _ => eprintln!("unknown command"),
    }
}
