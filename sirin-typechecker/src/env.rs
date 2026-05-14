use std::collections::HashMap;

use sirin_parser::types::Type;

pub struct Env<'a> {
    vars: HashMap<&'a str, Type>,               // ident, valor
    fns:  HashMap<&'a str, (Vec<Type>, Type)>,  // args, retorno
}