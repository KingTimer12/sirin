use sirin_parser::{expr::BinOp, types::Type};

#[derive(Debug)]
pub enum CheckerError<'a> {
    NameError(&'a str),          // variável "x" não declarada
    TypeError(Type, Type),       // esperava int, encontrou str
    ValueError(String),          // valor inválido para o contexto
    IndexError(Type),            // índice inválido para o tipo
    KeyError(&'a str),           // chave não existe no mapa
    ZeroDivisionError,           // divisão por zero detectável em compile time
    ReturnOutsideFn,
    GenericError(String),        // fallback
    PossibleNull(&'a str, Type), // quando pode ser nulo, não deixará continuar
    InvalidOperation { op: BinOp, ty: Type },
}
