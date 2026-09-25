use clap::{Command, arg};

use crate::commands::register_commands;

mod commands;
mod diag;
mod pipeline;
mod resolver;
mod toolchain;

fn file_arg() -> clap::Arg {
    arg!(<file> "Path to a .sn source file").required(true)
}

fn cli() -> Command {
    Command::new("sirin")
        .about("The Sirin compiler")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(Command::new("check").about("Type-check a file without building it").arg(file_arg()))
        .subcommand(Command::new("tokens").about("Print the tokens of a file").arg(file_arg()))
        .subcommand(Command::new("ast").about("Print the syntax tree of a file").arg(file_arg()))
        .subcommand(Command::new("emit-c").about("Generate C source code from a file").arg(file_arg()))
        .subcommand(Command::new("build").about("Compile a file into an executable").arg(file_arg()))
        .subcommand(
            Command::new("run")
                .about("Compile a file and run it")
                .arg(file_arg())
                .arg(
                    arg!(--watch "Rebuild and restart whenever the file or one of its modules is saved")
                        .action(clap::ArgAction::SetTrue),
                ),
        )
}

fn main() {
    let matches = cli().get_matches();
    register_commands(matches.subcommand());
}
