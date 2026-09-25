use std::fmt;
use std::ops::Range;

use sirin_diagnostics::Diagnostic;
use sirin_parser::{expr::BinOp, types::Type};

#[derive(Debug)]
pub enum CheckerError<'a> {
    NameError(&'a str),          // variable "x" was never declared
    TypeError(Type, Type),       // (expected, found): expected int, found str
    ValueError(String),          // value not valid in this context
    IndexError(Type),            // type cannot be indexed
    KeyError(&'a str),           // key not present in the map
    ZeroDivisionError,           // division by zero detectable at compile time
    ReturnOutsideFn,
    GenericError(String),        // fallback
    PossibleNull(&'a str, Type), // value may be None; must be handled before use
    InvalidOperation { op: BinOp, ty: Type },
    UseAfterMove { var: &'a str, moved_to: String },
    MissingInterfaceMethod { class: String, interface: String, method: String },
    ModuleNotImported { module: String, function: String },
    PrivateAccess { name: String, module: String },
}

impl CheckerError<'_> {
    /// Short headline, suitable as a diagnostic title.
    pub fn title(&self) -> &'static str {
        match self {
            CheckerError::NameError(_) => "undeclared name",
            CheckerError::TypeError(..) => "mismatched types",
            CheckerError::ValueError(_) => "invalid value",
            CheckerError::IndexError(_) => "cannot index this value",
            CheckerError::KeyError(_) => "unknown key",
            CheckerError::ZeroDivisionError => "division by zero",
            CheckerError::ReturnOutsideFn => "`return` outside of a function",
            CheckerError::GenericError(_) => "type error",
            CheckerError::PossibleNull(..) => "value may be `None`",
            CheckerError::InvalidOperation { .. } => "invalid operation",
            CheckerError::UseAfterMove { .. } => "use of moved value",
            CheckerError::MissingInterfaceMethod { .. } => "interface not fully implemented",
            CheckerError::ModuleNotImported { .. } => "missing import",
            CheckerError::PrivateAccess { .. } => "private function",
        }
    }

    /// A suggestion on how to fix the error, when there is an obvious one.
    pub fn help(&self) -> Option<String> {
        match self {
            CheckerError::NameError(name) => Some(format!(
                "check the spelling, or declare it before this line (e.g. `{name} = ...`)"
            )),
            CheckerError::ReturnOutsideFn => {
                Some("`return` can only be used inside a `fn` body".to_string())
            }
            CheckerError::PossibleNull(name, _) => Some(format!(
                "handle the empty case first, e.g. `if Some(v) = {name} {{ ... }}` or `{name}.unwrap_or(default)`"
            )),
            CheckerError::InvalidOperation { op: BinOp::Add, .. } => {
                Some("`+` works on numbers and on `str` (concatenation)".to_string())
            }
            CheckerError::InvalidOperation { op: BinOp::Sub | BinOp::Mul | BinOp::Div, .. } => {
                Some("arithmetic operators only work on numbers (`int`, `float`, `u8`, ...)".to_string())
            }
            CheckerError::UseAfterMove { var, .. } => Some(format!(
                "clone it where it is moved (`{var}::clone`) if you still need `{var}` afterwards"
            )),
            CheckerError::MissingInterfaceMethod { class, method, .. } => {
                Some(format!("add `fn {method}(...)` to `{class}`"))
            }
            CheckerError::ModuleNotImported { module, .. } => {
                Some(format!("add `use {module}` at the top of the file"))
            }
            CheckerError::PrivateAccess { .. } => Some(
                "names starting with `_` are private to their module; drop the `_` to export it"
                    .to_string(),
            ),
            _ => None,
        }
    }

    /// Build a diagnostic pointing at `span`.
    pub fn to_diagnostic(&self, span: Range<usize>) -> Diagnostic {
        let d = Diagnostic::error(self.title(), span).with_label(self.to_string());
        match self.help() {
            Some(h) => d.with_help(h),
            None => d,
        }
    }
}

impl fmt::Display for CheckerError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckerError::NameError(name) => write!(f, "`{name}` is not declared in this scope"),
            CheckerError::TypeError(expected, found) => {
                write!(f, "expected `{expected}`, found `{found}`")
            }
            CheckerError::ValueError(msg) | CheckerError::GenericError(msg) => f.write_str(msg),
            CheckerError::IndexError(ty) => write!(f, "values of type `{ty}` cannot be indexed with `[...]`"),
            CheckerError::KeyError(key) => write!(f, "key `{key}` does not exist in this map"),
            CheckerError::ZeroDivisionError => write!(f, "this divides by zero"),
            CheckerError::ReturnOutsideFn => write!(f, "this `return` is not inside any function"),
            CheckerError::PossibleNull(name, ty) => {
                write!(f, "`{name}` has type `{ty}` and may be `None` here")
            }
            CheckerError::InvalidOperation { op, ty } => {
                write!(f, "operator `{op}` cannot be used on values of type `{ty}`")
            }
            CheckerError::UseAfterMove { var, moved_to } => {
                // `moved_to` is either a variable name or a description ("return value").
                if moved_to.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                    write!(f, "`{var}` was moved into `{moved_to}` and can no longer be used")
                } else {
                    write!(f, "`{var}` was already moved ({moved_to}) and can no longer be used")
                }
            }
            CheckerError::MissingInterfaceMethod { class, interface, method } => write!(
                f,
                "`{class}` implements `{interface}` but is missing the method `{method}`"
            ),
            CheckerError::ModuleNotImported { module, function } => {
                write!(f, "`{function}` comes from `{module}`, which is not imported")
            }
            CheckerError::PrivateAccess { name, module } => {
                write!(f, "`{name}` is private to {module} and cannot be called from here")
            }
        }
    }
}
