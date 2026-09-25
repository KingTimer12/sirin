//! Shared error reporting for the CLI commands: every command parses and
//! type-checks the same way, so they all render errors the same way too.

use chumsky::span::SimpleSpan;
use sirin_diagnostics::eprint_all;
use sirin_lexer::token::Tokens;
use sirin_parser::span::Spanned;
use sirin_parser::stmt::Stmt;
use sirin_typechecker::checker::Checker;

/// Read a source file, or explain why it could not be read.
pub fn read_source(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("error: file `{}` does not exist", path),
        _ => format!("error: cannot read `{}`: {}", path, e),
    })
}

/// Parse `src`, printing every syntax error. `file` is the name shown in the
/// error output. Returns `None` when there were errors.
pub fn parse_or_report<'a>(
    file: &str,
    src: &str,
    tokens: &'a [(Tokens<'a>, SimpleSpan)],
) -> Option<Vec<Spanned<Stmt<'a>>>> {
    match sirin_parser::parse(src, tokens) {
        Ok(stmts) => Some(stmts),
        Err(diagnostics) => {
            eprint_all(&diagnostics, file, src);
            None
        }
    }
}

/// Type-check every top-level statement, printing each error. Returns the
/// number of errors found.
pub fn check_or_report<'a>(
    checker: &mut Checker<'a>,
    file: &str,
    src: &str,
    stmts: &[Spanned<Stmt<'a>>],
) -> usize {
    let mut errors = 0;
    for stmt in stmts {
        if let Err(e) = checker.check_stmt(stmt) {
            checker.take_diagnostic(&e, &stmt.span).eprint(file, src);
            errors += 1;
        }
    }
    errors
}

/// Closing line after errors, e.g. "could not compile `main.sn` due to 2 errors".
pub fn summary(path: &str, errors: usize) -> String {
    let plural = if errors == 1 { "" } else { "s" };
    format!("error: could not compile `{}` due to {} error{}", path, errors, plural)
}
