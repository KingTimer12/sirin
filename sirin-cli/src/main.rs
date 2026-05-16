use clap::{Command, arg};

use crate::commands::register_commands;

mod commands;

fn file_arg() -> clap::Arg {
    arg!(<file> "Arquivo .sirin").required(true)
}

fn cli() -> Command {
    Command::new("sirin")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(Command::new("check").about("Checa tipos do arquivo").arg(file_arg()))
        .subcommand(Command::new("tokens").about("Imprime tokens do arquivo").arg(file_arg()))
        .subcommand(Command::new("ast").about("Imprime AST do arquivo").arg(file_arg()))
        .subcommand(Command::new("emit-c").about("Gera código C a partir do arquivo").arg(file_arg()))
        .subcommand(Command::new("build").about("Compila o arquivo e gera executável").arg(file_arg()))
}

fn main() {
    let matches = cli().get_matches();
    register_commands(matches.subcommand());
}
